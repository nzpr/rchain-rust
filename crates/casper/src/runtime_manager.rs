//! The runtime manager façade (port of `casper/rholang/RuntimeManager.scala`, read-only surface).

use std::collections::BTreeMap;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::ast::Par;
use rchain_models::block::state_hash::StateHash;
use rchain_models::casper::protocol::casper_message::{Event, PCost, ProcessedDeploy, SignedDeployData};
use rchain_models::runtime::BindPattern;
use rchain_rholang::evaluate_result::EvaluateResult;
use rchain_rholang::runtime::RhoRuntime;
use rchain_rholang::storage::RhoHistoryRepository;
use rchain_rholang::system_processes::BlockData;

use crate::event_converter::to_casper_event;
use crate::rholang::UserDeployRuntimeResult;

/// The runtime manager (port of `RuntimeManager`). The deploy-execution, replay, and bond-computation
/// methods are deferred pending the system deploys + replay runtime wiring.
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
}

fn to_blake(hash: &StateHash) -> Blake2b256Hash {
    Blake2b256Hash::from_byte_array(hash.as_bytes())
}
