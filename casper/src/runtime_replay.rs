//! Replay orchestration over a `ReplayRhoRuntime` (port of
//! `casper/rholang/syntax/RuntimeReplaySyntax.scala`).
//!
//! Re-executes processed deploys and block-level system deploys against the recorded COMM trace,
//! verifying that the replayed status/cost match the play result and that every recorded COMM was
//! consumed (Law 11).

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_crypto::public_key::PublicKey;
use rchain_models::ast::{Expr, Par, Var};
use rchain_models::casper::protocol::casper_message::{
    ProcessedDeploy, ProcessedSystemDeploy, SystemDeployData,
};
use rchain_models::par_ops::from_expr;
use rchain_models::rholang::RhoType::RhoNumber;
use rchain_models::runtime::{BindPattern, ListParWithRandom, TaggedContinuation};
use rchain_rholang::evaluate_result::EvaluateResult;
use rchain_rholang::errors::RholangError;
use rchain_rholang::reporting_runtime::ReportingRuntime;
use rchain_rholang::runtime::ReplayRhoRuntime;
use rchain_rholang::system_processes::BlockData;
use rchain_rspace::checkpoint::{Checkpoint, SoftCheckpoint};
use rchain_rspace::errors::RSpaceError;
use rchain_rspace::hashing::stable_hash_provider::hash_channel;
use rchain_rspace::internal::Datum;
use rchain_rspace::merger::event_log_index::NumberChannelsDiff;
use rchain_rspace::trace::Log;
use rchain_rspace::util::ReplayException;

use crate::event_converter::to_rspace_event;
use crate::rholang::ReplayFailure;
use crate::system_deploy::{process_bool_result, SystemDeploy, SystemDeployUserError};

/// Random-seed split indices for the pre-charge / user-deploy / refund sequence (port of
/// `BlockRandomSeed`).
const PRE_CHARGE_SPLIT_INDEX: u8 = 0;
const USER_DEPLOY_SPLIT_INDEX: u8 = 1;
const REFUND_SPLIT_INDEX: u8 = 2;

/// The subset of a replay runtime needed to re-execute a block (implemented by both
/// [`ReplayRhoRuntime`] and [`ReportingRuntime`]).
#[async_trait]
pub trait ReplayRuntime {
    fn set_block_data(&self, block_data: BlockData);

    async fn reset(&self, root: Blake2b256Hash) -> Result<(), String>;

    async fn evaluate(&self, term: &str, rand: &Blake2b512Random) -> Result<EvaluateResult, RholangError>;

    async fn evaluate_with_env(
        &self,
        term: &str,
        env: &BTreeMap<String, Par>,
        rand: &Blake2b512Random,
    ) -> Result<EvaluateResult, RholangError>;

    async fn create_soft_checkpoint(
        &self,
    ) -> SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation>;

    async fn revert_to_soft_checkpoint(
        &self,
        checkpoint: SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation>,
    );

    async fn rig(&self, log: Log);

    async fn check_replay_data(&self) -> Result<(), ReplayException>;

    async fn get_data(&self, channel: &Par) -> Result<Vec<Datum<ListParWithRandom>>, RSpaceError>;

    async fn consume_result(
        &self,
        channels: &[Par],
        patterns: &[BindPattern],
    ) -> Result<Option<(TaggedContinuation, Vec<ListParWithRandom>)>, RSpaceError>;

    async fn create_checkpoint(&self) -> Result<Checkpoint, String>;
}

/// Replay orchestration (port of `RuntimeReplayOps`).
pub struct RuntimeReplayOps<'a, R: ReplayRuntime + ?Sized> {
    runtime: &'a R,
}

impl<'a, R: ReplayRuntime + ?Sized> RuntimeReplayOps<'a, R> {
    pub fn new(runtime: &'a R) -> Self {
        RuntimeReplayOps { runtime }
    }

    /// Evaluate (and validate) deploys + system deploys, checkpointing to validate the final state
    /// hash (port of `replayComputeState`).
    pub async fn replay_compute_state(
        &self,
        start_hash: &Blake2b256Hash,
        rand: &Blake2b512Random,
        terms: &[ProcessedDeploy],
        system_deploys: &[ProcessedSystemDeploy],
        block_data: BlockData,
        with_cost_accounting: bool,
    ) -> Result<(Blake2b256Hash, Vec<NumberChannelsDiff>), ReplayFailure> {
        self.runtime.set_block_data(block_data);
        self.replay_deploys(start_hash, rand, terms, system_deploys, with_cost_accounting)
            .await
    }

    /// Reset to `start_hash`, replay each deploy then each system deploy, and checkpoint (port of
    /// `replayDeploys`).
    async fn replay_deploys(
        &self,
        start_hash: &Blake2b256Hash,
        rand: &Blake2b512Random,
        terms: &[ProcessedDeploy],
        system_deploys: &[ProcessedSystemDeploy],
        with_cost_accounting: bool,
    ) -> Result<(Blake2b256Hash, Vec<NumberChannelsDiff>), ReplayFailure> {
        self.runtime
            .reset(*start_hash)
            .await
            .map_err(ReplayFailure::internal_error)?;

        let mut mergeable: Vec<NumberChannelsDiff> = Vec::new();
        for (i, term) in terms.iter().enumerate() {
            mergeable.push(
                self.replay_deploy_e(term, rand.split_byte(i as u8), with_cost_accounting)
                    .await?,
            );
        }
        for (i, sd) in system_deploys.iter().enumerate() {
            mergeable.push(
                self.replay_block_system_deploy(sd, rand.split_byte((terms.len() + i) as u8))
                    .await?,
            );
        }

        let checkpoint = self
            .runtime
            .create_checkpoint()
            .await
            .map_err(ReplayFailure::internal_error)?;
        Ok((checkpoint.root, mergeable))
    }

    /// Replay a single user deploy (port of `replayDeployE`).
    pub(crate) async fn replay_deploy_e(
        &self,
        processed_deploy: &ProcessedDeploy,
        rand: Blake2b512Random,
        with_cost_accounting: bool,
    ) -> Result<NumberChannelsDiff, ReplayFailure> {
        let mut mergeable: BTreeSet<Par> = BTreeSet::new();
        let expected_failure = processed_deploy.system_deploy_error.clone();

        // Load the deploy's recorded trace before re-executing (port of `rigWithCheck`).
        self.rig_deploy(processed_deploy).await;

        let succeeded = self
            .replay_deploy_evaluator(
                processed_deploy,
                rand,
                with_cost_accounting,
                expected_failure.as_deref(),
                &mut mergeable,
            )
            .await?;

        self.check_replay_data_with_fix(succeeded).await?;

        self.get_number_channels_data(&mergeable).await
    }

    /// The pre-charge / user-deploy / refund fold yielding whether the deploy succeeded (port of
    /// `replayDeployE`'s `evaluatorT`).
    #[allow(clippy::too_many_arguments)]
    async fn replay_deploy_evaluator(
        &self,
        processed_deploy: &ProcessedDeploy,
        rand: Blake2b512Random,
        with_cost_accounting: bool,
        expected_failure: Option<&str>,
        mergeable: &mut BTreeSet<Par>,
    ) -> Result<bool, ReplayFailure> {
        if !with_cost_accounting {
            let eval_result = self
                .replay_deploy_eval(processed_deploy, rand.split_byte(USER_DEPLOY_SPLIT_INDEX))
                .await?;
            if eval_result.succeeded() {
                mergeable.extend(eval_result.mergeable.iter().cloned());
            }
            return Ok(eval_result.succeeded());
        }

        // Pre-charge.
        let pre_charge = SystemDeploy::pre_charge(
            processed_deploy.deploy.data.total_phlo_charge(),
            &PublicKey::new(processed_deploy.deploy.deployer.clone()),
            rand.split_byte(PRE_CHARGE_SPLIT_INDEX),
        );
        let (_pre_result, pre_eval) = self
            .replay_system_deploy_internal(&pre_charge, expected_failure)
            .await?;
        self.runtime.create_soft_checkpoint().await;
        if pre_eval.succeeded() {
            mergeable.extend(pre_eval.mergeable.iter().cloned());
        }

        if expected_failure.is_some() {
            return Ok(true);
        }

        // User deploy (reverted on failure).
        let eval_result = self
            .replay_deploy_eval(processed_deploy, rand.split_byte(USER_DEPLOY_SPLIT_INDEX))
            .await?;
        if eval_result.succeeded() {
            mergeable.extend(eval_result.mergeable.iter().cloned());
            self.runtime.create_soft_checkpoint().await;
        }

        // Refund.
        let refund = SystemDeploy::refund(
            refund_amount(processed_deploy),
            rand.split_byte(REFUND_SPLIT_INDEX),
        );
        let (_refund_result, refund_eval) = self.replay_system_deploy_internal(&refund, None).await?;
        self.runtime.create_soft_checkpoint().await;
        if refund_eval.succeeded() {
            mergeable.extend(refund_eval.mergeable.iter().cloned());
        }

        Ok(eval_result.succeeded())
    }

    /// Replay the user deploy body and verify status/cost match the play result (port of
    /// `deployEvaluator`).
    async fn replay_deploy_eval(
        &self,
        processed_deploy: &ProcessedDeploy,
        rand: Blake2b512Random,
    ) -> Result<EvaluateResult, ReplayFailure> {
        // Soft transaction: revert the deploy's effects if it failed.
        let fallback = self.runtime.create_soft_checkpoint().await;
        let result = self
            .runtime
            .evaluate(&processed_deploy.deploy.data.term, &rand)
            .await
            .map_err(|e| ReplayFailure::internal_error(e.to_string()))?;
        if result.failed() {
            self.runtime.revert_to_soft_checkpoint(fallback).await;
        }

        // Verify deploy status matches.
        if processed_deploy.is_failed != result.failed() {
            return Err(ReplayFailure::replay_status_mismatch(
                processed_deploy.is_failed,
                result.failed(),
            ));
        }
        // Verify evaluation cost matches.
        let recorded_cost = i64::try_from(processed_deploy.cost.cost).map_err(|_| {
            ReplayFailure::replay_cost_mismatch(i64::MAX, result.cost.value)
        })?;
        if recorded_cost != result.cost.value {
            return Err(ReplayFailure::replay_cost_mismatch(recorded_cost, result.cost.value));
        }
        Ok(result)
    }

    /// Replay a block-level system deploy (port of `replayBlockSystemDeploy`).
    pub(crate) async fn replay_block_system_deploy(
        &self,
        processed: &ProcessedSystemDeploy,
        rand: Blake2b512Random,
    ) -> Result<NumberChannelsDiff, ReplayFailure> {
        let system_deploy_data = match processed {
            ProcessedSystemDeploy::Succeeded { system_deploy, .. } => system_deploy,
            ProcessedSystemDeploy::Failed { .. } => {
                return Err(ReplayFailure::internal_error("Expected system deploy"));
            }
        };
        let deploy = match system_deploy_data {
            SystemDeployData::Slash(validator) => SystemDeploy::slash(validator, rand),
            SystemDeployData::CloseBlock => SystemDeploy::close_block(rand),
            SystemDeployData::Empty => {
                return Err(ReplayFailure::internal_error("Expected system deploy"));
            }
        };

        self.rig_system_deploy(processed).await;

        let (_result, eval_res) = self.replay_system_deploy_internal(&deploy, None).await?;
        if eval_res.succeeded() {
            self.runtime.create_soft_checkpoint().await;
        }
        let data = self.get_number_channels_data(&eval_res.mergeable).await?;

        self.check_replay_data_with_fix(eval_res.succeeded()).await?;

        Ok(data)
    }

    /// Evaluate a system deploy and compare its play/replay status (port of
    /// `replaySystemDeployInternal`).
    async fn replay_system_deploy_internal(
        &self,
        system_deploy: &SystemDeploy,
        expected_failure_msg: Option<&str>,
    ) -> Result<(Result<(), SystemDeployUserError>, EvaluateResult), ReplayFailure> {
        let (result, eval_res) = self
            .eval_system_deploy(system_deploy)
            .await
            .map_err(ReplayFailure::internal_error)?;

        match (expected_failure_msg, &result) {
            // Replayed successful execution.
            (None, Ok(())) => {}
            // Replayed failed execution with a matching error.
            (Some(expected), Err(SystemDeployUserError(actual))) if expected == actual.as_str() => {}
            // Error messages differ.
            (Some(expected), Err(SystemDeployUserError(actual))) => {
                return Err(ReplayFailure::system_deploy_error_mismatch(
                    expected.to_string(),
                    actual.clone(),
                ));
            }
            // Error expected, replay successful.
            (Some(_), Ok(())) => {
                return Err(ReplayFailure::replay_status_mismatch(true, false));
            }
            // No error expected, replay failed.
            (None, Err(_)) => {
                return Err(ReplayFailure::replay_status_mismatch(false, true));
            }
        }
        Ok((result, eval_res))
    }

    /// Evaluate a system deploy on the replay runtime (port of `evalSystemDeploy`).
    async fn eval_system_deploy(
        &self,
        deploy: &SystemDeploy,
    ) -> Result<(Result<(), SystemDeployUserError>, EvaluateResult), String> {
        let eval_result = self
            .runtime
            .evaluate_with_env(deploy.source, &deploy.normalizer_env, &deploy.rand)
            .await
            .map_err(|e| e.to_string())?;
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

    /// Load the recorded trace of a processed deploy (port of `rig(ProcessedDeploy)`).
    async fn rig_deploy(&self, processed_deploy: &ProcessedDeploy) {
        let log = processed_deploy
            .deploy_log
            .iter()
            .map(to_rspace_event)
            .collect();
        self.runtime.rig(log).await;
    }

    /// Load the recorded trace of a processed system deploy (port of `rig(ProcessedSystemDeploy)`).
    async fn rig_system_deploy(&self, processed: &ProcessedSystemDeploy) {
        let event_list = match processed {
            ProcessedSystemDeploy::Succeeded { event_list, .. } => event_list,
            ProcessedSystemDeploy::Failed { event_list, .. } => event_list,
        };
        let log = event_list.iter().map(to_rspace_event).collect();
        self.runtime.rig(log).await;
    }

    /// Verify the replay trace, ignoring unused-COMM failures for failed deploys (port of
    /// `checkReplayDataWithFix`).
    async fn check_replay_data_with_fix(&self, eval_successful: bool) -> Result<(), ReplayFailure> {
        match self.runtime.check_replay_data().await {
            Ok(()) => Ok(()),
            Err(replay_exception) => {
                let failure = ReplayFailure::unused_comm_event(replay_exception.0);
                // TODO: temp fix for replay error mismatch (RCHAIN-3505).
                if eval_successful {
                    Err(failure)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Read the numeric values of the mergeable (number) channels (port of
    /// `getNumberChannelsData`).
    async fn get_number_channels_data(
        &self,
        channels: &BTreeSet<Par>,
    ) -> Result<NumberChannelsDiff, ReplayFailure> {
        let mut out = BTreeMap::new();
        for chan in channels {
            if let Some((hash, num)) = self.get_number_channel(chan).await? {
                out.insert(hash, num);
            }
        }
        Ok(out)
    }

    /// Read a single mergeable channel's number value (port of `getNumberChannel`).
    async fn get_number_channel(
        &self,
        chan: &Par,
    ) -> Result<Option<(Blake2b256Hash, i64)>, ReplayFailure> {
        let data = self
            .runtime
            .get_data(chan)
            .await
            .map_err(|e| ReplayFailure::internal_error(e.to_string()))?;
        if data.is_empty() {
            return Ok(None);
        }
        if data.len() != 1 {
            return Err(ReplayFailure::internal_error(
                "Number channel must have singleton value.",
            ));
        }
        let num = get_number_with_rnd(&data[0].a);
        let ch_hash = hash_channel(chan);
        Ok(Some((ch_hash, num)))
    }
}

#[async_trait]
impl ReplayRuntime for ReplayRhoRuntime {
    fn set_block_data(&self, block_data: BlockData) {
        ReplayRhoRuntime::set_block_data(self, block_data);
    }

    async fn reset(&self, root: Blake2b256Hash) -> Result<(), String> {
        ReplayRhoRuntime::reset(self, root).await
    }

    async fn evaluate(&self, term: &str, rand: &Blake2b512Random) -> Result<EvaluateResult, RholangError> {
        ReplayRhoRuntime::evaluate(self, term, rand).await
    }

    async fn evaluate_with_env(
        &self,
        term: &str,
        env: &BTreeMap<String, Par>,
        rand: &Blake2b512Random,
    ) -> Result<EvaluateResult, RholangError> {
        ReplayRhoRuntime::evaluate_with_env(self, term, env, rand).await
    }

    async fn create_soft_checkpoint(
        &self,
    ) -> SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation> {
        ReplayRhoRuntime::create_soft_checkpoint(self).await
    }

    async fn revert_to_soft_checkpoint(
        &self,
        checkpoint: SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation>,
    ) {
        ReplayRhoRuntime::revert_to_soft_checkpoint(self, checkpoint).await
    }

    async fn rig(&self, log: Log) {
        ReplayRhoRuntime::rig(self, log).await
    }

    async fn check_replay_data(&self) -> Result<(), ReplayException> {
        ReplayRhoRuntime::check_replay_data(self).await
    }

    async fn get_data(&self, channel: &Par) -> Result<Vec<Datum<ListParWithRandom>>, RSpaceError> {
        ReplayRhoRuntime::get_data(self, channel).await
    }

    async fn consume_result(
        &self,
        channels: &[Par],
        patterns: &[BindPattern],
    ) -> Result<Option<(TaggedContinuation, Vec<ListParWithRandom>)>, RSpaceError> {
        ReplayRhoRuntime::consume_result(self, channels, patterns).await
    }

    async fn create_checkpoint(&self) -> Result<Checkpoint, String> {
        ReplayRhoRuntime::create_checkpoint(self).await
    }
}

#[async_trait]
impl ReplayRuntime for ReportingRuntime {
    fn set_block_data(&self, block_data: BlockData) {
        ReportingRuntime::set_block_data(self, block_data);
    }

    async fn reset(&self, root: Blake2b256Hash) -> Result<(), String> {
        ReportingRuntime::reset(self, root).await
    }

    async fn evaluate(&self, term: &str, rand: &Blake2b512Random) -> Result<EvaluateResult, RholangError> {
        ReportingRuntime::evaluate(self, term, rand).await
    }

    async fn evaluate_with_env(
        &self,
        term: &str,
        env: &BTreeMap<String, Par>,
        rand: &Blake2b512Random,
    ) -> Result<EvaluateResult, RholangError> {
        ReportingRuntime::evaluate_with_env(self, term, env, rand).await
    }

    async fn create_soft_checkpoint(
        &self,
    ) -> SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation> {
        ReportingRuntime::create_soft_checkpoint(self).await
    }

    async fn revert_to_soft_checkpoint(
        &self,
        checkpoint: SoftCheckpoint<Par, BindPattern, ListParWithRandom, TaggedContinuation>,
    ) {
        ReportingRuntime::revert_to_soft_checkpoint(self, checkpoint).await
    }

    async fn rig(&self, log: Log) {
        ReportingRuntime::rig(self, log).await
    }

    async fn check_replay_data(&self) -> Result<(), ReplayException> {
        ReportingRuntime::check_replay_data(self).await
    }

    async fn get_data(&self, channel: &Par) -> Result<Vec<Datum<ListParWithRandom>>, RSpaceError> {
        ReportingRuntime::get_data(self, channel).await
    }

    async fn consume_result(
        &self,
        channels: &[Par],
        patterns: &[BindPattern],
    ) -> Result<Option<(TaggedContinuation, Vec<ListParWithRandom>)>, RSpaceError> {
        ReportingRuntime::consume_result(self, channels, patterns).await
    }

    async fn create_checkpoint(&self) -> Result<Checkpoint, String> {
        ReportingRuntime::create_checkpoint(self).await
    }
}

/// Extract the numeric value from a number-channel datum (port of `getNumberWithRnd`).
fn get_number_with_rnd(par_with_rnd: &ListParWithRandom) -> i64 {
    assert_eq!(
        par_with_rnd.pars.len(),
        1,
        "Number channel should contain single Int term."
    );
    RhoNumber::unapply(&par_with_rnd.pars[0])
        .expect("Number channel should contain single Int term.")
}

/// The phlo refunded after a deploy (port of `ProcessedDeploy.refundAmount`).
fn refund_amount(processed_deploy: &ProcessedDeploy) -> i64 {
    processed_deploy
        .deploy
        .data
        .phlo_limit
        .saturating_sub_unsigned(processed_deploy.cost.cost)
        .max(0)
        * processed_deploy.deploy.data.phlo_price
}
