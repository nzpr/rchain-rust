//! The multi-parent CBC-Casper façade (port of `MultiParentCasper.scala`).
//!
//! The block `validate` orchestration is deferred pending `InterpreterUtil.validateBlockCheckpoint`
//! and the mergeable-channel store.

use std::collections::{BTreeMap, BTreeSet};

use rchain_block_storage::block_store::BlockStore;
use rchain_block_storage::dag::dag_storage::{BlockDagStorage, DeployId};
use rchain_block_storage::dag::finalizer::{Finalizer, Message};
use rchain_block_storage::dag::message_map;
use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_models::block::state_hash::StateHash;
use rchain_models::block_hash::BlockHash;
use rchain_models::block_metadata::BlockMetadata;
use rchain_models::casper::protocol::casper_message::{BlockMessage, SignedDeployData};
use rchain_models::validator::Validator;

use crate::merging::{BlockIndex, DeployChainIndex, MergeScope, ParentsMergedState};
use crate::runtime_manager::RuntimeManager;

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

async fn get_block_unsafe(block_store: &BlockStore, hash: &BlockHash) -> Result<BlockMessage, String> {
    let mut vals = block_store.get(&[*hash]).await?;
    vals.pop()
        .flatten()
        .ok_or_else(|| format!("missing block {}", hash.to_hex()))
}

/// Compute the merged pre-state for a set of parent blocks (port of `getPreStateForParents`).
pub async fn get_pre_state_for_parents<F, Fut>(
    dag: &dyn BlockDagStorage,
    block_store: &BlockStore,
    runtime: &RuntimeManager,
    parent_hashes: &BTreeSet<BlockHash>,
    block_index: &F,
) -> Result<ParentsMergedState, String>
where
    F: Fn(BlockHash) -> Fut,
    Fut: std::future::Future<Output = Result<BlockIndex, String>>,
{
    assert!(
        !parent_hashes.is_empty(),
        "Parents must not be empty to calculate pre-state. Genesis block pre-state is loaded from config."
    );

    let dag_repr = dag.get_representation().await;
    let msg_map = &dag_repr.dag_message_state.msg_map;

    let mut justifications: Vec<BlockMetadata> = Vec::new();
    for h in parent_hashes {
        let meta = dag
            .lookup(h)
            .await?
            .ok_or_else(|| format!("missing justification {}", h.to_hex()))?;
        justifications.push(meta);
    }

    let parents: BTreeSet<Message<BlockHash, Validator>> = parent_hashes
        .iter()
        .map(|h| msg_map.get(h).expect("parent not in message map").clone())
        .collect();

    // Currently finalized fringe.
    let prev_fringe = message_map::latest_fringe(msg_map, &parents);
    let prev_fringe_hashes: BTreeSet<BlockHash> = prev_fringe.iter().map(|m| m.id).collect();
    let fringe_record = dag_repr
        .fringe_states
        .get(&prev_fringe_hashes)
        .ok_or_else(|| {
            format!(
                "Fringe state not available in state cache, fringe: {:?}",
                prev_fringe_hashes
            )
        })?;
    let prev_fringe_state = fringe_record.state_hash;
    let prev_fringe_rejected_deploys = fringe_record.rejected_deploys.clone();

    // Bonds map: from the latest justification for an empty fringe, else from the PoS contract.
    let bonds_map = if prev_fringe.is_empty() {
        justifications
            .first()
            .map(|j| j.bonds_map.clone())
            .unwrap_or_default()
    } else {
        let state_hash = StateHash::from_slice(prev_fringe_state.as_bytes());
        runtime.compute_bonds(&state_hash).await?
    };

    // If a new fringe is finalized, merge it.
    let finalizer = Finalizer::new(msg_map.clone());
    let (_parent_fringe, new_fringe_opt) = finalizer.calculate_finalization(&parents, &bonds_map);
    let new_fringe_hashes: Option<BTreeSet<BlockHash>> =
        new_fringe_opt.map(|f| f.iter().map(|m| m.id).collect());

    let new_fringe_result = match &new_fringe_hashes {
        Some(fringe) => {
            let (m_scope, base_opt) =
                MergeScope::from_dag(fringe, &prev_fringe_hashes, &dag_repr.child_map, msg_map);
            let base_state = match base_opt {
                Some(h) => {
                    Blake2b256Hash::from_byte_array(
                        &get_block_unsafe(block_store, &h).await?.post_state_hash,
                    )
                }
                None => prev_fringe_state,
            };
            let result = MergeScope::merge(
                &m_scope,
                base_state,
                &dag_repr.fringe_states,
                runtime.get_history_repo(),
                block_index,
                DeployChainIndex::deploy_chain_cost,
            )
            .await?;
            Some(result)
        }
        None => None,
    };
    let (fringe_state, fringe_rejected_deploys) =
        new_fringe_result.unwrap_or((prev_fringe_state, prev_fringe_rejected_deploys));

    let max_height = justifications
        .iter()
        .map(|m| m.block_num)
        .max()
        .unwrap_or(-1);
    let max_seq_nums: BTreeMap<Validator, i64> =
        justifications.iter().map(|m| (m.sender, m.seq_num)).collect();
    let new_fringe = new_fringe_hashes.unwrap_or(prev_fringe_hashes);

    // Merge the conflict scope (non-finalized blocks above the fringe).
    let (pre_state_hash, cs_rejected_deploys) = if parent_hashes.len() == 1 {
        let parent = parent_hashes.iter().next().unwrap();
        let block = get_block_unsafe(block_store, parent).await?;
        (
            Blake2b256Hash::from_byte_array(&block.post_state_hash),
            BTreeSet::new(),
        )
    } else {
        let (m_scope, base_opt) =
            MergeScope::from_dag(parent_hashes, &new_fringe, &dag_repr.child_map, msg_map);
        let base_state = match base_opt {
            Some(h) => {
                Blake2b256Hash::from_byte_array(
                    &get_block_unsafe(block_store, &h).await?.post_state_hash,
                )
            }
            None => fringe_state,
        };
        MergeScope::merge(
            &m_scope,
            base_state,
            &dag_repr.fringe_states,
            runtime.get_history_repo(),
            block_index,
            DeployChainIndex::deploy_chain_cost,
        )
        .await?
    };

    Ok(ParentsMergedState {
        justifications,
        max_block_num: max_height,
        max_seq_nums,
        fringe: new_fringe,
        fringe_state,
        fringe_bonds_map: bonds_map,
        fringe_rejected_deploys,
        pre_state_hash,
        rejected_deploys: cs_rejected_deploys,
    })
}

/// Compute the pre-state for a new block from the DAG's latest messages (port of
/// `getPreStateForNewBlock`).
pub async fn get_pre_state_for_new_block<F, Fut>(
    dag: &dyn BlockDagStorage,
    block_store: &BlockStore,
    runtime: &RuntimeManager,
    block_index: &F,
) -> Result<ParentsMergedState, String>
where
    F: Fn(BlockHash) -> Fut,
    Fut: std::future::Future<Output = Result<BlockIndex, String>>,
{
    let dag_repr = dag.get_representation().await;
    let parent_hashes: BTreeSet<BlockHash> = dag_repr
        .dag_message_state
        .latest_msgs
        .iter()
        .map(|m| m.id)
        .collect();
    get_pre_state_for_parents(dag, block_store, runtime, &parent_hashes, block_index).await
}
