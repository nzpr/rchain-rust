//! The runtime manager façade (port of `casper/rholang/RuntimeManager.scala`, read-only surface).

use std::collections::{BTreeMap, BTreeSet};

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_crypto::public_key::PublicKey;
use rchain_models::ast::{Expr, Par, Var};
use rchain_models::block::state_hash::StateHash;
use rchain_models::casper::protocol::casper_message::{
    Event, PCost, ProcessedDeploy, ProcessedSystemDeploy, SignedDeployData, SystemDeployData,
};
use rchain_models::par_ops::from_expr;
use rchain_models::rholang::RhoType::RhoName;
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_models::validator::Validator;
use rchain_rholang::evaluate_result::EvaluateResult;
use rchain_rholang::runtime::RhoRuntime;
use rchain_rholang::storage::RhoHistoryRepository;
use rchain_rholang::system_processes::BlockData;

use crate::event_converter::to_casper_event;
use crate::rholang::{SystemDeployRuntimeResult, UserDeployRuntimeResult};
use crate::system_deploy::{process_bool_result, EvalCollector, SystemDeploy, SystemDeployUserError};

/// The runtime manager (port of `RuntimeManager`). Deploy execution (user deploys + system
/// deploys), genesis/state computation, and bond/validator queries are implemented; replay is
/// deferred pending the replay-runtime wiring.
pub struct RuntimeManager {
    runtime: RhoRuntime,
    history_repo: RhoHistoryRepository,
}

impl RuntimeManager {
    pub fn new(runtime: RhoRuntime, history_repo: RhoHistoryRepository) -> Self {
        RuntimeManager {
            runtime,
            history_repo,
        }
    }

    pub fn get_history_repo(&self) -> &RhoHistoryRepository {
        &self.history_repo
    }

    pub fn runtime(&self) -> &RhoRuntime {
        &self.runtime
    }

    /// Read the `Par`s at a channel in the state identified by `hash` (port of `getData`).
    pub async fn get_data(&self, hash: &StateHash, channel: &Par) -> Result<Vec<Par>, String> {
        self.runtime.reset(to_blake(hash)).await.map_err(|e| e)?;
        self.runtime
            .get_data_par(channel)
            .await
            .map_err(|e| e.to_string())
    }

    /// Read the `ParBody` continuations at `channels` in the state identified by `hash` (port of
    /// `getContinuation`).
    pub async fn get_continuation(
        &self,
        hash: &StateHash,
        channels: &[Par],
    ) -> Result<Vec<(Vec<BindPattern>, Par)>, String> {
        self.runtime.reset(to_blake(hash)).await.map_err(|e| e)?;
        self.runtime
            .get_continuation_par(channels)
            .await
            .map_err(|e| e.to_string())
    }

    /// Execute a single deploy, returning its `ProcessedDeploy` + evaluation result (port of
    /// `processDeploy`).
    pub async fn process_deploy(
        &self,
        deploy: &SignedDeployData,
        rand: &Blake2b512Random,
    ) -> Result<(ProcessedDeploy, EvaluateResult), String> {
        let fallback = self.runtime.create_soft_checkpoint().await;
        let eval_result = self
            .runtime
            .evaluate(&deploy.data.term, rand)
            .map_err(|e| e.to_string())?;
        let checkpoint = self.runtime.create_soft_checkpoint().await;
        let succeeded = eval_result.errors.is_empty();
        let deploy_log: Vec<Event> = checkpoint.log.iter().map(to_casper_event).collect();
        let processed = ProcessedDeploy {
            deploy: deploy.clone(),
            cost: PCost {
                cost: eval_result.cost.value.max(0) as u64,
            },
            deploy_log,
            is_failed: !succeeded,
            system_deploy_error: None,
        };
        if !succeeded {
            self.runtime.revert_to_soft_checkpoint(fallback).await;
        }
        Ok((processed, eval_result))
    }

    /// Run deploys from `start_hash` and return the post-state hash + processed deploys (port of
    /// `playDeploys`).
    pub async fn play_deploys(
        &self,
        start_hash: &Blake2b256Hash,
        terms: &[SignedDeployData],
        rand: &Blake2b512Random,
    ) -> Result<(Blake2b256Hash, Vec<UserDeployRuntimeResult>), String> {
        self.runtime.reset(*start_hash).await.map_err(|e| e)?;
        let mut results = Vec::new();
        for (i, d) in terms.iter().enumerate() {
            let r = rand.split_byte(i as u8);
            let (processed, eval_result) = self.process_deploy(d, &r).await?;
            results.push(UserDeployRuntimeResult {
                deploy: processed,
                mergeable: BTreeMap::new(),
                eval_result,
            });
        }
        let checkpoint = self.runtime.create_checkpoint().await.map_err(|e| e)?;
        Ok((checkpoint.root, results))
    }

    /// Execute a user deploy with the pre-charge/refund system deploys (port of
    /// `playDeployWithCostAccounting`).
    pub async fn play_deploy_with_cost_accounting(
        &self,
        deploy: &SignedDeployData,
        rand: &Blake2b512Random,
    ) -> Result<UserDeployRuntimeResult, String> {
        let mut collector = EvalCollector::default();

        let pre_charge = SystemDeploy::pre_charge(
            deploy.data.total_phlo_charge(),
            &PublicKey::new(deploy.deployer.clone()),
            rand.split_byte(0),
        );
        let (pre_result, pre_eval) = self.eval_system_deploy(&pre_charge).await?;
        let pre_checkpoint = self.runtime.create_soft_checkpoint().await;
        collector = collector.add(
            &pre_checkpoint.log.iter().map(to_casper_event).collect::<Vec<_>>(),
            &pre_eval.mergeable,
        );

        if let Err(e) = pre_result {
            let failed = ProcessedDeploy {
                deploy: deploy.clone(),
                cost: PCost { cost: 0 },
                deploy_log: collector.event_log.clone(),
                is_failed: true,
                system_deploy_error: Some(e.0),
            };
            return Ok(UserDeployRuntimeResult {
                deploy: failed,
                mergeable: BTreeMap::new(),
                eval_result: EvaluateResult {
                    cost: rchain_rholang::accounting::Cost::new(0, "pre-charge"),
                    errors: Vec::new(),
                    mergeable: BTreeSet::new(),
                },
            });
        }

        let (mut processed, eval_result) = self.process_deploy(deploy, &rand.split_byte(1)).await?;
        collector = collector.add(&processed.deploy_log, &eval_result.mergeable);

        let refund = SystemDeploy::refund(deploy.data.phlo_limit, rand.split_byte(2));
        let _ = self.eval_system_deploy(&refund).await?;

        processed.deploy_log = collector.event_log.clone();
        Ok(UserDeployRuntimeResult {
            deploy: processed,
            mergeable: BTreeMap::new(),
            eval_result,
        })
    }

    /// Run deploys with cost accounting from `start_hash` (port of `playDeploys` with
    /// `playDeployWithCostAccounting`).
    pub async fn play_deploys_with_cost_accounting(
        &self,
        start_hash: &Blake2b256Hash,
        terms: &[SignedDeployData],
        rand: &Blake2b512Random,
    ) -> Result<(Blake2b256Hash, Vec<UserDeployRuntimeResult>), String> {
        self.runtime.reset(*start_hash).await.map_err(|e| e)?;
        let mut results = Vec::new();
        for (i, d) in terms.iter().enumerate() {
            let r = rand.split_byte(i as u8);
            results.push(self.play_deploy_with_cost_accounting(d, &r).await?);
        }
        let checkpoint = self.runtime.create_checkpoint().await.map_err(|e| e)?;
        Ok((checkpoint.root, results))
    }

    /// Compute the genesis state from deploys (port of `computeGenesis`).
    pub async fn compute_genesis(
        &self,
        terms: &[SignedDeployData],
        rand: &Blake2b512Random,
        block_data: BlockData,
    ) -> Result<(Blake2b256Hash, Blake2b256Hash, Vec<UserDeployRuntimeResult>), String> {
        self.runtime.set_block_data(block_data);
        let pre_state_hash = self.runtime.empty_state_hash().await?;
        let (post_state_hash, processed) = self.play_deploys(&pre_state_hash, terms, rand).await?;
        Ok((pre_state_hash, post_state_hash, processed))
    }

    /// Run a system deploy's source (port of `evaluateSystemSource`).
    fn evaluate_system_source(&self, deploy: &SystemDeploy) -> Result<EvaluateResult, String> {
        self.runtime
            .evaluate_with_env(deploy.source, &deploy.normalizer_env, &deploy.rand)
            .map_err(|e| e.to_string())
    }

    /// Consume the result produced on the system deploy's return channel (port of
    /// `consumeSystemResult`).
    async fn consume_system_result(
        &self,
        deploy: &SystemDeploy,
    ) -> Result<Option<(TaggedContinuation, Vec<ListParWithRandom>)>, String> {
        let pattern = BindPattern {
            patterns: vec![from_expr(Expr::EVar(Box::new(Var::FreeVar(0))))],
            remainder: None,
            free_count: 1,
        };
        self.runtime
            .consume_result(&[deploy.return_channel.clone()], &[pattern])
            .await
            .map_err(|e| e.to_string())
    }

    /// Evaluate a system deploy and extract its result (port of `evalSystemDeploy`).
    pub async fn eval_system_deploy(
        &self,
        deploy: &SystemDeploy,
    ) -> Result<(Result<(), SystemDeployUserError>, EvaluateResult), String> {
        let eval_result = self.evaluate_system_source(deploy)?;
        if !eval_result.errors.is_empty() {
            return Err(format!("Unexpected system errors: {:?}", eval_result.errors));
        }
        let consumed = self.consume_system_result(deploy).await?;
        match consumed {
            Some((_, data)) => match data.as_slice() {
                [single] if single.pars.len() == 1 => {
                    let result = process_bool_result(&single.pars[0]);
                    Ok((result, eval_result))
                }
                _ => Err("Unexpected system-deploy result".to_string()),
            },
            None => Err("Unable to consume results of system deploy".to_string()),
        }
    }

    /// Play a single block-level system deploy from `state_hash` (port of `playSystemDeploy`).
    pub async fn play_system_deploy(
        &self,
        state_hash: &Blake2b256Hash,
        deploy: &SystemDeploy,
    ) -> Result<(Blake2b256Hash, SystemDeployRuntimeResult), String> {
        self.runtime.reset(*state_hash).await.map_err(|e| e)?;
        let (result, _eval_result) = self.eval_system_deploy(deploy).await?;
        let checkpoint = self.runtime.create_soft_checkpoint().await;
        let event_list: Vec<Event> = checkpoint.log.iter().map(to_casper_event).collect();
        let final_hash = self.runtime.create_checkpoint().await.map_err(|e| e)?.root;
        match result {
            Ok(()) => {
                let processed = ProcessedSystemDeploy::Succeeded {
                    event_list,
                    system_deploy: SystemDeployData::Empty,
                };
                Ok((
                    final_hash,
                    SystemDeployRuntimeResult {
                        deploy: processed,
                        mergeable: BTreeMap::new(),
                    },
                ))
            }
            Err(e) => Err(format!("System deploy failed: {}", e.0)),
        }
    }

    /// Compute the post-state from user deploys + block-level system deploys (port of
    /// `computeState`).
    pub async fn compute_state(
        &self,
        start_hash: &Blake2b256Hash,
        terms: &[SignedDeployData],
        system_deploys: &[SystemDeploy],
        rand: &Blake2b512Random,
        block_data: BlockData,
    ) -> Result<
        (
            Blake2b256Hash,
            Vec<UserDeployRuntimeResult>,
            Vec<SystemDeployRuntimeResult>,
        ),
        String,
    > {
        self.runtime.set_block_data(block_data);
        let (mut state_hash, processed_deploys) =
            self.play_deploys_with_cost_accounting(start_hash, terms, rand).await?;
        let mut processed_system_deploys = Vec::new();
        for sd in system_deploys {
            let (new_hash, processed) = self.play_system_deploy(&state_hash, sd).await?;
            state_hash = new_hash;
            processed_system_deploys.push(processed);
        }
        Ok((state_hash, processed_deploys, processed_system_deploys))
    }

    /// Run a read-only exploratory deploy and capture its result (port of `playExploratoryDeploy`).
    pub async fn play_exploratory_deploy(
        &self,
        term: &str,
        hash: &StateHash,
    ) -> Result<Vec<Par>, String> {
        let rand = Blake2b512Random::default_random();
        let mut return_rand = rand.copy();
        let return_channel = RhoName::apply_bytes(return_rand.next());
        self.capture_results(hash, term, &rand, &return_channel).await
    }

    async fn capture_results(
        &self,
        start: &StateHash,
        term: &str,
        rand: &Blake2b512Random,
        return_channel: &Par,
    ) -> Result<Vec<Par>, String> {
        self.runtime.reset(to_blake(start)).await.map_err(|e| e)?;
        let eval = self.runtime.evaluate(term, rand).map_err(|e| e.to_string())?;
        if !eval.errors.is_empty() {
            return Err(format!("{:?}", eval.errors));
        }
        self.runtime
            .get_data_par(return_channel)
            .await
            .map_err(|e| e.to_string())
    }

    /// Query the current active validators at `hash` (port of `getActiveValidators`).
    pub async fn get_active_validators(&self, hash: &StateHash) -> Result<Vec<Validator>, String> {
        let pars = self
            .play_exploratory_deploy(ACTIVATE_VALIDATOR_QUERY_SOURCE, hash)
            .await?;
        if pars.len() != 1 {
            return Err(format!("Incorrect number of results: {}", pars.len()));
        }
        Ok(to_validator_seq(&pars[0]))
    }

    /// Query the current bonds at `hash` (port of `computeBonds`).
    pub async fn compute_bonds(&self, hash: &StateHash) -> Result<BTreeMap<Validator, i64>, String> {
        let pars = self.play_exploratory_deploy(BONDS_QUERY_SOURCE, hash).await?;
        if pars.len() != 1 {
            return Err(format!("Incorrect number of results: {}", pars.len()));
        }
        Ok(to_bond_map(&pars[0]))
    }
}

fn to_validator_seq(p: &Par) -> Vec<Validator> {
    let mut out = Vec::new();
    if let Some(Expr::ESet(set)) = p.exprs.first() {
        for v in &set.ps {
            if let Some(Expr::GByteArray(bytes)) = v.exprs.first() {
                out.push(Validator::from_slice(bytes));
            }
        }
    }
    out
}

fn to_bond_map(p: &Par) -> BTreeMap<Validator, i64> {
    let mut out = BTreeMap::new();
    if let Some(Expr::EMap(map)) = p.exprs.first() {
        for (k, v) in &map.kvs {
            if let (Some(Expr::GByteArray(vb)), Some(Expr::GInt(bond))) =
                (k.exprs.first(), v.exprs.first())
            {
                out.insert(Validator::from_slice(vb), *bond);
            }
        }
    }
    out
}

const ACTIVATE_VALIDATOR_QUERY_SOURCE: &str = r#"new return, rl(`rho:registry:lookup`), poSCh in {
  rl!(`rho:rchain:pos`, *poSCh) |
  for(@(_, Pos) <- poSCh) {
    @Pos!("getActiveValidators", *return)
  }
}"#;

const BONDS_QUERY_SOURCE: &str = r#"new return, rl(`rho:registry:lookup`), poSCh in {
  rl!(`rho:rchain:pos`, *poSCh) |
  for(@(_, Pos) <- poSCh) {
    @Pos!("getBonds", *return)
  }
}"#;

fn to_blake(hash: &StateHash) -> Blake2b256Hash {
    Blake2b256Hash::from_byte_array(hash.as_bytes())
}
