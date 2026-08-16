//! Interpreter utilities (port of `rholang/InterpreterUtil.scala`).
//!
//! `validateBlockCheckpoint` is deferred pending `MultiParentCasper.getPreStateForParents` and
//! `BlockRandomSeed`.

use std::collections::BTreeMap;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_models::ast::Par;
use rchain_models::casper::protocol::casper_message::{BlockMessage, SignedDeployData};
use rchain_rholang::errors::RholangError;
use rchain_rholang::system_processes::BlockData;

use crate::rholang::{ReplayFailure, SystemDeployRuntimeResult, UserDeployRuntimeResult};
use crate::runtime_manager::RuntimeManager;
use crate::system_deploy::SystemDeploy;

/// Parse + normalize a rholang term (port of `mkTerm`).
pub fn mk_term(rho: &str, env: &BTreeMap<String, Par>) -> Result<Par, RholangError> {
    rchain_rholang::normalizer::source_to_adt_with_env(rho, env)
}

/// Replay a block's deploys and return the computed state hash (port of `replayBlock`).
pub async fn replay_block(
    runtime: &RuntimeManager,
    block: &BlockMessage,
    rand: &Blake2b512Random,
) -> Result<Blake2b256Hash, ReplayFailure> {
    let start_hash = Blake2b256Hash::from_byte_array(&block.pre_state_hash);
    let block_data = BlockData::from_block(block);
    let with_cost_accounting = !block.justifications.is_empty();
    let (state_hash, _mergeable) = runtime
        .replay_compute_state(
            &start_hash,
            &block.state.deploys,
            &block.state.system_deploys,
            rand,
            block_data,
            with_cost_accounting,
        )
        .await?;
    Ok(state_hash)
}

/// Map a replay result into an `Option` of the matching state hash (port of `handleErrors`).
pub fn handle_errors(
    ts_hash: &Blake2b256Hash,
    result: Result<Blake2b256Hash, ReplayFailure>,
) -> Result<Option<Blake2b256Hash>, String> {
    match result {
        Ok(computed) => {
            if *ts_hash == computed {
                Ok(Some(computed))
            } else {
                Ok(None)
            }
        }
        Err(ReplayFailure::InternalError(cause)) => Err(format!(
            "Internal errors encountered while processing deploy: {cause}"
        )),
        Err(_) => Ok(None),
    }
}

/// Compute the post-state + processed deploys from a deploy sequence (port of
/// `computeDeploysCheckpoint`).
#[allow(clippy::too_many_arguments)]
pub async fn compute_deploys_checkpoint(
    runtime: &RuntimeManager,
    deploys: &[SignedDeployData],
    system_deploys: &[SystemDeploy],
    rand: &Blake2b512Random,
    block_data: BlockData,
    pre_state_hash: &Blake2b256Hash,
) -> Result<
    (
        Blake2b256Hash,
        Vec<UserDeployRuntimeResult>,
        Vec<SystemDeployRuntimeResult>,
    ),
    String,
> {
    runtime
        .compute_state(pre_state_hash, deploys, system_deploys, rand, block_data)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> Blake2b256Hash {
        Blake2b256Hash::from_bytes([byte; 32])
    }

    #[test]
    fn handle_errors_accepts_matching_hash() {
        assert_eq!(
            handle_errors(&hash(1), Ok(hash(1))).unwrap(),
            Some(hash(1))
        );
    }

    #[test]
    fn handle_errors_rejects_mismatching_hash() {
        assert_eq!(handle_errors(&hash(1), Ok(hash(2))).unwrap(), None);
    }

    #[test]
    fn handle_errors_raises_internal_error() {
        let r = handle_errors(&hash(1), Err(ReplayFailure::internal_error("boom")));
        assert!(r.is_err());
    }

    #[test]
    fn handle_errors_soft_fails_on_replay_status_mismatch() {
        let r = handle_errors(
            &hash(1),
            Err(ReplayFailure::replay_status_mismatch(true, false)),
        );
        assert_eq!(r.unwrap(), None);
    }
}
