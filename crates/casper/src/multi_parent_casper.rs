//! The multi-parent CBC-Casper façade (port of `MultiParentCasper.scala`).
//!
//! The pre-state computation (`getPreStateForParents`) and block `validate` orchestration are
//! deferred pending `InterpreterUtil.validateBlockCheckpoint` and the mergeable-channel store.

use std::collections::BTreeMap;

use rchain_block_storage::block_store::BlockStore;
use rchain_block_storage::dag::dag_storage::{BlockDagStorage, DeployId};
use rchain_models::casper::protocol::casper_message::{BlockMessage, SignedDeployData};

/// A deploy-parsing error (port of `ParsingError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsingError(pub String);

/// The size of the deploy safety range (port of `deployLifespan`).
pub const DEPLOY_LIFESPAN: i64 = 50;

/// Build a `ParsingError` from details (port of `parsingError`).
pub fn parsing_error(details: impl Into<String>) -> ParsingError {
    ParsingError(format!("Parsing error: {}", details.into()))
}

/// Look up the last finalized block (port of `lastFinalizedBlock`).
pub async fn last_finalized_block(
    dag: &dyn BlockDagStorage,
    block_store: &BlockStore,
) -> Result<BlockMessage, String> {
    let repr = dag.get_representation().await;
    let hash = repr
        .last_finalized_block_hash()
        .ok_or_else(|| "no finalized block in the DAG".to_string())?;
    let mut vals = block_store.get(&[hash]).await?;
    vals.pop()
        .flatten()
        .ok_or_else(|| format!("missing finalized block {}", hash.to_hex()))
}

/// Add a deploy to the deploy pool and return its id (port of `addDeploy`).
pub async fn add_deploy(
    dag: &dyn BlockDagStorage,
    deploy: &SignedDeployData,
) -> Result<DeployId, String> {
    dag.add_deploy(deploy.clone()).await?;
    Ok(deploy.sig.clone())
}

/// Parse-check a deploy term, then add the deploy to the pool (port of `deploy`).
///
/// The Scala normalizes with `NormalizerEnv(deploy)` (deployer id / return channels); here the term
/// is normalized against an empty environment (the full `NormalizerEnv` is deferred).
pub async fn deploy(
    dag: &dyn BlockDagStorage,
    deploy: &SignedDeployData,
) -> Result<DeployId, ParsingError> {
    rchain_rholang::normalizer::source_to_adt_with_env(&deploy.data.term, &BTreeMap::new())
        .map_err(|e| parsing_error(format!("Error in parsing term: \n{e}")))?;
    add_deploy(dag, deploy)
        .await
        .map_err(parsing_error)
}
