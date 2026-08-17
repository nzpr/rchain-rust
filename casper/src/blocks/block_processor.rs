//! Block processing (port of `blocks/BlockProcessor.scala`).
//!
//! The streaming `apply` (fs2 + `CommUtil` broadcast) is deferred.

use rchain_block_storage::block_store::BlockStore;
use rchain_block_storage::dag::dag_storage::BlockDagStorage;
use rchain_models::block_hash::BlockHash;
use rchain_models::casper::protocol::casper_message::BlockMessage;

use crate::block_status::BlockStatus;
use crate::merging::BlockIndex;
use crate::runtime_manager::RuntimeManager;

/// Validate a block and insert it into the DAG (port of `validateAndAddToDag`).
pub async fn validate_and_add_to_dag<F, Fut>(
    dag: &dyn BlockDagStorage,
    block_store: &BlockStore,
    runtime: &RuntimeManager,
    block: BlockMessage,
    shard_id: &str,
    min_phlo_price: i64,
    block_index: &F,
) -> Result<Result<(), BlockStatus>, String>
where
    F: Fn(BlockHash) -> Fut,
    Fut: std::future::Future<Output = Result<BlockIndex, String>>,
{
    let result = crate::multi_parent_casper::validate(
        dag,
        block_store,
        runtime,
        &block,
        shard_id,
        min_phlo_price,
        block_index,
    )
    .await;
    let (block_meta, status) = match result {
        Ok(meta) => (meta, Ok(())),
        Err((meta, status)) => (meta, Err(status)),
    };
    dag.insert(block_meta, block).await?;
    Ok(status)
}
