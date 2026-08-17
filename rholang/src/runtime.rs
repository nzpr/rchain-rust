//! The rholang runtime façade (port of `RhoRuntime.scala`, core).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::ast::{Bundle, Expr, Par, Var};
use rchain_models::par_ops::from_expr;
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_rspace::checkpoint::{Checkpoint, SoftCheckpoint};
use rchain_rspace::errors::RSpaceError;
use rchain_rspace::i_replay_space::IReplaySpace;
use rchain_rspace::i_space::ISpace;
use rchain_rspace::internal::{Datum, Row, WaitingContinuation};
use rchain_rspace::replay_rspace::ReplayRSpace;
use rchain_rspace::rspace::RSpace;
use rchain_rspace::trace::Log;
use rchain_rspace::tuple_space::Tuplespace as RSpaceTuplespace;
use rchain_rspace::util::ReplayException;

use crate::accounting::CostAccounting;
use crate::dispatch::RholangAndScalaDispatcher;
use crate::env::Env;
use crate::errors::RholangError;
use crate::evaluate_result::EvaluateResult;
use crate::reduce::DebruijnInterpreter;
use crate::storage::{ChargingRSpace, RhoHistoryRepository, RhoTuplespace};
use crate::system_processes::{BlockData, FixedChannels, SystemProcesses};

/// The concrete rspace type the runtime operates on.
pub type RhoSpace = Arc<RSpace<Par, BindPattern, ListParWithRandom, TaggedContinuation>>;

/// The replay rspace type the replay runtime operates on (port of `RhoReplayISpace`).
pub type RhoReplaySpace = Arc<ReplayRSpace<Par, BindPattern, ListParWithRandom, TaggedContinuation>>;

/// The reducer type wired to the charging space and dispatcher.
pub type RhoReducer =
    DebruijnInterpreter<ChargingRSpace, Rc<RholangAndScalaDispatcher>>;

/// Wire the reducer and dispatcher together (breaking their mutual recursion) and return the
/// reducer.
pub fn setup_reducer(
    charging_space: ChargingRSpace,
    cost: Rc<CostAccounting>,
    mergeable_tag_name: Par,
) -> Rc<RhoReducer> {
    let dispatcher = Rc::new(RholangAndScalaDispatcher::new(BTreeMap::new()));
    let reducer = Rc::new(DebruijnInterpreter::new(
        charging_space,
        dispatcher.clone(),
        BTreeMap::new(),
        mergeable_tag_name,
    ));
    let reducer_for_eval = reducer.clone();
    let cost_for_eval = cost.clone();
    dispatcher.set_eval(Box::new(move |par, env, rand| {
        reducer_for_eval.eval(par, env, rand, cost_for_eval.as_ref())
    }));
    reducer
}

/// The rholang runtime (port of `RhoRuntime`). `evaluate` (parse+run) is deferred; `inj`,
/// checkpointing, and tuplespace reads are provided.
pub struct RhoRuntime {
    reducer: Rc<RhoReducer>,
    space: RhoSpace,
    cost: Rc<CostAccounting>,
    block_data: Rc<RefCell<BlockData>>,
    _history: RhoHistoryRepository,
}

/// A write-only bundle over `channel` (port of `Bundle(channel, writeFlag = true)`).
fn write_bundle(channel: Par) -> Par {
    Par {
        bundles: vec![Bundle {
            body: Box::new(channel),
            write_flag: true,
            read_flag: false,
        }],
        ..Par::default()
    }
}

/// Install each system-contract definition as a persistent join on its fixed channel (port of
/// `RhoRuntime.introduceSystemProcesses`). The `space` is a `Tuplespace` so both the play `RSpace`
/// and the `ReplayRSpace` can be installed into.
fn install_system_processes(
    space: &RhoTuplespace,
    runtime: &tokio::runtime::Runtime,
    proc_defs: &[(Par, i32, i64)],
) -> Result<(), RholangError> {
    for (name, arity, body_ref) in proc_defs {
        let patterns = vec![BindPattern {
            patterns: (0..*arity)
                .map(|i| from_expr(Expr::EVar(Box::new(Var::FreeVar(i)))))
                .collect(),
            remainder: None,
            free_count: *arity,
        }];
        let continuation = TaggedContinuation::ScalaBodyRef(*body_ref);
        runtime
            .block_on(space.install(&[name.clone()], &patterns, continuation))
            .map_err(|e| RholangError::ReduceError(e.to_string()))?;
    }
    Ok(())
}

/// The shared reducer/system-process wiring built over a `Tuplespace` (port of `createRhoEnv` +
/// `setupReducer`). The play `RhoRuntime`, the replay `ReplayRhoRuntime`, and the reporting
/// `ReportingRuntime` reuse this core; the only difference is the concrete space each retains for
/// `ISpace`/`IReplaySpace` operations.
pub(crate) struct RuntimeCore {
    pub(crate) reducer: Rc<RhoReducer>,
    pub(crate) cost: Rc<CostAccounting>,
    pub(crate) block_data: Rc<RefCell<BlockData>>,
}

pub(crate) fn build_runtime_core(
    space: &RhoTuplespace,
    runtime: &Arc<tokio::runtime::Runtime>,
    mergeable_tag_name: Par,
) -> std::io::Result<RuntimeCore> {
    let cost = Rc::new(CostAccounting::from_initial(crate::accounting::Costs::unsafe_max()));
    let charging_space = ChargingRSpace::new(space.clone(), runtime.clone());
    let block_data = Rc::new(RefCell::new(BlockData::empty()));

    // Build the dispatcher (empty), then the system processes, then wire them together.
    let dispatcher = Rc::new(RholangAndScalaDispatcher::new(BTreeMap::new()));
    let system_processes = SystemProcesses::new(charging_space.clone(), dispatcher.clone(), block_data.clone());

    let mut dispatch_table = BTreeMap::new();
    let mut urn_map = BTreeMap::new();
    let mut proc_defs: Vec<(Par, i32, i64)> = Vec::new();
    for d in system_processes.definitions() {
        dispatch_table.insert(d.body_ref, d.handler);
        urn_map.insert(d.urn, write_bundle(d.fixed_channel.clone()));
        proc_defs.push((d.fixed_channel, d.arity, d.body_ref));
    }
    // The registry bootstrap channels (port of `basicProcesses`).
    urn_map.insert(
        "rho:registry:lookup".to_string(),
        write_bundle(FixedChannels::reg_lookup()),
    );
    urn_map.insert(
        "rho:registry:insertArbitrary".to_string(),
        write_bundle(FixedChannels::reg_insert_random()),
    );
    urn_map.insert(
        "rho:registry:insertSigned:secp256k1".to_string(),
        write_bundle(FixedChannels::reg_insert_signed()),
    );

    dispatcher.set_dispatch_table(dispatch_table);
    install_system_processes(space, runtime, &proc_defs)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    let reducer = Rc::new(DebruijnInterpreter::new(
        charging_space,
        dispatcher.clone(),
        urn_map,
        mergeable_tag_name,
    ));
    let reducer_for_eval = reducer.clone();
    let cost_for_eval = cost.clone();
    dispatcher.set_eval(Box::new(move |par, env, rand| {
        reducer_for_eval.eval(par, env, rand, cost_for_eval.as_ref())
    }));

    Ok(RuntimeCore {
        reducer,
        cost,
        block_data,
    })
}

impl RhoRuntime {
    pub fn create(
        space: RhoSpace,
        history: RhoHistoryRepository,
        mergeable_tag_name: Par,
    ) -> std::io::Result<RhoRuntime> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?,
        );
        let tuplespace: RhoTuplespace = space.clone();
        let core = build_runtime_core(&tuplespace, &runtime, mergeable_tag_name)?;
        Ok(RhoRuntime {
            reducer: core.reducer,
            space,
            cost: core.cost,
            block_data: core.block_data,
            _history: history,
        })
    }

    /// Set the per-block data exposed to the `rho:block:data` contract (port of `setBlockData`).
    pub fn set_block_data(&self, block_data: BlockData) {
        *self.block_data.borrow_mut() = block_data;
    }

    /// Execute a `Par` in the given environment (port of `inj`).
    pub fn inj(
        &self,
        par: &Par,
        env: &Env<Par>,
        rand: &Blake2b512Random,
    ) -> Result<(), RholangError> {
        self.reducer.eval(par, env, rand, self.cost.as_ref())
    }

    /// Parse + run a rholang term (port of `evaluate`).
    pub fn evaluate(
        &self,
        term: &str,
        rand: &Blake2b512Random,
    ) -> Result<EvaluateResult, RholangError> {
        self.evaluate_with_env(term, &BTreeMap::new(), rand)
    }

    /// Parse + run a rholang term with an explicit normalizer environment (port of `evaluate`).
    pub fn evaluate_with_env(
        &self,
        term: &str,
        env: &BTreeMap<String, Par>,
        rand: &Blake2b512Random,
    ) -> Result<EvaluateResult, RholangError> {
        let par = crate::normalizer::source_to_adt_with_env(term, env)?;
        let errors = match self.inj(&par, &Env::new(), rand) {
            Ok(()) => Vec::new(),
            Err(e) => vec![e],
        };
        Ok(EvaluateResult {
            cost: crate::accounting::Cost::new(self.cost.total_charged(), "evaluate"),
            errors,
            mergeable: BTreeSet::new(),
        })
    }

    /// The empty-state hash: reset to the empty root, bootstrap the registry, and checkpoint (port
    /// of `emptyStateHash`).
    pub async fn empty_state_hash(&self) -> Result<Blake2b256Hash, String> {
        self.space
            .reset(rchain_rspace::history::history::empty_root_hash_value())
            .await
            .map_err(|e| e.to_string())?;
        let rand = Blake2b512Random::default_random();
        self.inj(
            &crate::registry::registry_bootstrap_ast(),
            &Env::new(),
            &rand,
        )
        .map_err(|e| e.to_string())?;
        let checkpoint = self.space.create_checkpoint().await.map_err(|e| e.to_string())?;
        Ok(checkpoint.root)
    }

    pub fn space(&self) -> &RhoSpace {
        &self.space
    }

    pub fn cost(&self) -> &CostAccounting {
        self.cost.as_ref()
    }

    pub async fn create_checkpoint(&self) -> Result<Checkpoint, String> {
        self.space.create_checkpoint().await
    }

    pub async fn reset(&self, root: Blake2b256Hash) -> Result<(), String> {
        self.space.reset(root).await
    }

    /// Capture a soft (in-memory) checkpoint for rollback (port of `createSoftCheckpoint`).
    pub async fn create_soft_checkpoint(
        &self,
    ) -> SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation> {
        self.space.create_soft_checkpoint().await
    }

    /// Roll back to a soft checkpoint (port of `revertToSoftCheckpoint`).
    pub async fn revert_to_soft_checkpoint(
        &self,
        checkpoint: SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation>,
    ) {
        self.space.revert_to_soft_checkpoint(checkpoint).await
    }

    pub async fn get_data(
        &self,
        channel: &Par,
    ) -> Result<Vec<Datum<ListParWithRandom>>, RSpaceError> {
        self.space.get_data(channel).await
    }

    pub async fn get_joins(&self, channel: &Par) -> Result<Vec<Vec<Par>>, RSpaceError> {
        self.space.get_joins(channel).await
    }

    pub async fn get_continuation(
        &self,
        channels: &[Par],
    ) -> Result<Vec<WaitingContinuation<BindPattern, TaggedContinuation>>, RSpaceError> {
        self.space.get_waiting_continuations(channels).await
    }

    /// Read all `Par`s at a channel (port of `getDataPar`).
    pub async fn get_data_par(&self, channel: &Par) -> Result<Vec<Par>, RSpaceError> {
        let data = self.space.get_data(channel).await?;
        Ok(data.into_iter().flat_map(|d| d.a.pars).collect())
    }

    /// Read the waiting `ParBody` continuations as `(patterns, body)` (port of
    /// `getContinuationPar`).
    pub async fn get_continuation_par(
        &self,
        channels: &[Par],
    ) -> Result<Vec<(Vec<BindPattern>, Par)>, RSpaceError> {
        let conts = self.space.get_waiting_continuations(channels).await?;
        Ok(conts
            .into_iter()
            .filter_map(|wc| match wc.continuation {
                TaggedContinuation::ParBody(pwr) => Some((wc.patterns, pwr.body)),
                _ => None,
            })
            .collect())
    }

    /// Consume the result at a channel with a pattern (port of `consumeResult`).
    pub async fn consume_result(
        &self,
        channels: &[Par],
        patterns: &[BindPattern],
    ) -> Result<Option<(TaggedContinuation, Vec<ListParWithRandom>)>, RSpaceError> {
        let result = self
            .space
            .consume(channels, patterns, TaggedContinuation::Empty, false, BTreeSet::new())
            .await?;
        Ok(result.map(|(cont, data)| {
            (
                cont.continuation,
                data.into_iter().map(|d| d.matched_datum).collect(),
            )
        }))
    }

    pub async fn get_hot_changes(
        &self,
    ) -> BTreeMap<Vec<Par>, Row<BindPattern, ListParWithRandom, TaggedContinuation>> {
        self.space.to_map().await
    }
}

/// The replay runtime (port of `ReplayRhoRuntime`). Wraps a `ReplayRSpace` so that `inj`/`evaluate`
/// re-execute against the recorded COMM trace, and exposes `rig`/`check_replay_data` (Law 11).
pub struct ReplayRhoRuntime {
    reducer: Rc<RhoReducer>,
    space: RhoReplaySpace,
    cost: Rc<CostAccounting>,
    block_data: Rc<RefCell<BlockData>>,
    _history: RhoHistoryRepository,
}

impl ReplayRhoRuntime {
    pub fn create(
        space: RhoReplaySpace,
        history: RhoHistoryRepository,
        mergeable_tag_name: Par,
    ) -> std::io::Result<ReplayRhoRuntime> {
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?,
        );
        let tuplespace: RhoTuplespace = space.clone();
        let core = build_runtime_core(&tuplespace, &runtime, mergeable_tag_name)?;
        Ok(ReplayRhoRuntime {
            reducer: core.reducer,
            space,
            cost: core.cost,
            block_data: core.block_data,
            _history: history,
        })
    }

    /// Set the per-block data exposed to the `rho:block:data` contract (port of `setBlockData`).
    pub fn set_block_data(&self, block_data: BlockData) {
        *self.block_data.borrow_mut() = block_data;
    }

    /// Execute a `Par` in the given environment (port of `inj`).
    pub fn inj(
        &self,
        par: &Par,
        env: &Env<Par>,
        rand: &Blake2b512Random,
    ) -> Result<(), RholangError> {
        self.reducer.eval(par, env, rand, self.cost.as_ref())
    }

    /// Parse + run a rholang term (port of `evaluate`).
    pub fn evaluate(
        &self,
        term: &str,
        rand: &Blake2b512Random,
    ) -> Result<EvaluateResult, RholangError> {
        self.evaluate_with_env(term, &BTreeMap::new(), rand)
    }

    /// Parse + run a rholang term with an explicit normalizer environment (port of `evaluate`).
    pub fn evaluate_with_env(
        &self,
        term: &str,
        env: &BTreeMap<String, Par>,
        rand: &Blake2b512Random,
    ) -> Result<EvaluateResult, RholangError> {
        let par = crate::normalizer::source_to_adt_with_env(term, env)?;
        let errors = match self.inj(&par, &Env::new(), rand) {
            Ok(()) => Vec::new(),
            Err(e) => vec![e],
        };
        Ok(EvaluateResult {
            cost: crate::accounting::Cost::new(self.cost.total_charged(), "evaluate"),
            errors,
            mergeable: BTreeSet::new(),
        })
    }

    pub fn space(&self) -> &RhoReplaySpace {
        &self.space
    }

    pub fn cost(&self) -> &CostAccounting {
        self.cost.as_ref()
    }

    pub async fn create_checkpoint(&self) -> Result<Checkpoint, String> {
        self.space.create_checkpoint().await
    }

    pub async fn reset(&self, root: Blake2b256Hash) -> Result<(), String> {
        self.space.reset(root).await
    }

    /// Capture a soft (in-memory) checkpoint for rollback (port of `createSoftCheckpoint`).
    pub async fn create_soft_checkpoint(
        &self,
    ) -> SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation> {
        self.space.create_soft_checkpoint().await
    }

    /// Roll back to a soft checkpoint (port of `revertToSoftCheckpoint`).
    pub async fn revert_to_soft_checkpoint(
        &self,
        checkpoint: SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation>,
    ) {
        self.space.revert_to_soft_checkpoint(checkpoint).await
    }

    pub async fn get_data(
        &self,
        channel: &Par,
    ) -> Result<Vec<Datum<ListParWithRandom>>, RSpaceError> {
        self.space.get_data(channel).await
    }

    /// Read all `Par`s at a channel (port of `getDataPar`).
    pub async fn get_data_par(&self, channel: &Par) -> Result<Vec<Par>, RSpaceError> {
        let data = self.space.get_data(channel).await?;
        Ok(data.into_iter().flat_map(|d| d.a.pars).collect())
    }

    /// Consume the result at a channel with a pattern (port of `consumeResult`).
    pub async fn consume_result(
        &self,
        channels: &[Par],
        patterns: &[BindPattern],
    ) -> Result<Option<(TaggedContinuation, Vec<ListParWithRandom>)>, RSpaceError> {
        let result = self
            .space
            .consume(channels, patterns, TaggedContinuation::Empty, false, BTreeSet::new())
            .await?;
        Ok(result.map(|(cont, data)| {
            (
                cont.continuation,
                data.into_iter().map(|d| d.matched_datum).collect(),
            )
        }))
    }

    /// Load the replay trace (port of `rig`).
    pub async fn rig(&self, log: Log) {
        self.space.rig(log).await;
    }

    /// Verify every recorded COMM was consumed by the replay (port of `checkReplayData`).
    pub async fn check_replay_data(&self) -> Result<(), ReplayException> {
        self.space.check_replay_data().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rchain_rspace::tuple_space::{ContResult, Result as RSpaceResult};
    use std::sync::Mutex;

    struct MockSpace {
        produced: Mutex<Vec<(Par, ListParWithRandom, bool)>>,
    }

    #[async_trait]
    impl RSpaceTuplespace<Par, BindPattern, ListParWithRandom, TaggedContinuation> for MockSpace {
        async fn consume(
            &self,
            _channels: &[Par],
            _patterns: &[BindPattern],
            _continuation: TaggedContinuation,
            _persist: bool,
            _peeks: BTreeSet<usize>,
        ) -> Result<
            Option<(
                ContResult<Par, BindPattern, TaggedContinuation>,
                Vec<RSpaceResult<Par, ListParWithRandom>>,
            )>,
            RSpaceError,
        > {
            Ok(None)
        }

        async fn produce(
            &self,
            channel: Par,
            data: ListParWithRandom,
            persist: bool,
        ) -> Result<
            Option<(
                ContResult<Par, BindPattern, TaggedContinuation>,
                Vec<RSpaceResult<Par, ListParWithRandom>>,
            )>,
            RSpaceError,
        > {
            self.produced.lock().unwrap().push((channel, data, persist));
            Ok(None)
        }

        async fn install(
            &self,
            _channels: &[Par],
            _patterns: &[BindPattern],
            _continuation: TaggedContinuation,
        ) -> Result<Option<(TaggedContinuation, Vec<ListParWithRandom>)>, RSpaceError> {
            Ok(None)
        }
    }

    #[test]
    fn inj_send_produces_through_bridge() {
        let mock = Arc::new(MockSpace {
            produced: Mutex::new(Vec::new()),
        });
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        );
        let charging = ChargingRSpace::new(mock.clone(), runtime);
        let cost = Rc::new(CostAccounting::from_initial(crate::accounting::Costs::unsafe_max()));
        let reducer = setup_reducer(charging, cost.clone(), Par::default());

        let send = rchain_models::ast::Send {
            chan: Box::new(rchain_models::par_ops::from_expr(rchain_models::ast::Expr::GInt(1))),
            data: vec![rchain_models::par_ops::from_expr(rchain_models::ast::Expr::GInt(2))],
            persistent: false,
            locally_free: rchain_models::ast::AlwaysEqual(vec![]),
            connective_used: false,
        };
        let par = Par {
            sends: vec![send],
            ..Par::default()
        };
        let rand = Blake2b512Random::new_random(128);
        reducer
            .eval(&par, &Env::new(), &rand, cost.as_ref())
            .unwrap();

        let produced = mock.produced.lock().unwrap();
        assert_eq!(produced.len(), 1);
        assert_eq!(produced[0].1.pars, vec![rchain_models::par_ops::from_expr(
            rchain_models::ast::Expr::GInt(2)
        )]);
    }
}
