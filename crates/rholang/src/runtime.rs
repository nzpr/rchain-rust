//! The rholang runtime façade (port of `RhoRuntime.scala`, core).

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::ast::Par;
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_rspace::checkpoint::Checkpoint;
use rchain_rspace::errors::RSpaceError;
use rchain_rspace::i_space::ISpace;
use rchain_rspace::internal::{Datum, Row, WaitingContinuation};
use rchain_rspace::rspace::RSpace;
use rchain_rspace::tuple_space::Tuplespace as RSpaceTuplespace;

use crate::accounting::CostAccounting;
use crate::dispatch::RholangAndScalaDispatcher;
use crate::env::Env;
use crate::errors::RholangError;
use crate::reduce::DebruijnInterpreter;
use crate::storage::{ChargingRSpace, RhoHistoryRepository};

/// The concrete rspace type the runtime operates on.
pub type RhoSpace = Arc<RSpace<Par, BindPattern, ListParWithRandom, TaggedContinuation>>;

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

/// The rholang runtime (port of `RhoRuntime`). `evaluate` (parse+run) and system processes are
/// deferred; `inj`, checkpointing, and tuplespace reads are provided.
pub struct RhoRuntime {
    reducer: Rc<RhoReducer>,
    space: RhoSpace,
    cost: Rc<CostAccounting>,
    _history: RhoHistoryRepository,
}

impl RhoRuntime {
    pub fn create(
        space: RhoSpace,
        history: RhoHistoryRepository,
        mergeable_tag_name: Par,
    ) -> std::io::Result<RhoRuntime> {
        let cost = Rc::new(CostAccounting::new());
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?,
        );
        let charging_space = ChargingRSpace::new(space.clone(), runtime);
        let reducer = setup_reducer(charging_space, cost.clone(), mergeable_tag_name);
        Ok(RhoRuntime {
            reducer,
            space,
            cost,
            _history: history,
        })
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
