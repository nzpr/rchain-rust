//! Block DAG storage interface.
//!
//! Mirrors `block-storage/src/main/scala/coop/rchain/blockstorage/dag/BlockDagStorage.scala`. The
//! concrete `BlockDagKeyValueStorage` is casper-owned in Scala, so only the trait is ported here.

use std::collections::BTreeMap;

use async_trait::async_trait;

use rchain_models::block_hash::BlockHash;
use rchain_models::block_metadata::BlockMetadata;
use rchain_models::casper::protocol::casper_message::{BlockMessage, SignedDeployData};

use super::representation::DagRepresentation;

/// A deploy id (the Scala `BlockDagStorage.DeployId = ByteString`).
pub type DeployId = Vec<u8>;

/// The block DAG storage interface (port of `BlockDagStorage[F]`).
// impl deferred to casper (the concrete `BlockDagKeyValueStorage`).
#[async_trait]
pub trait BlockDagStorage: Send + Sync {
    async fn get_representation(&self) -> DagRepresentation;

    async fn insert(&self, block_metadata: BlockMetadata, block: BlockMessage);

    async fn lookup(&self, block_hash: &BlockHash) -> Option<BlockMetadata>;

    /// Look up a block hash by the deploy id included in the DAG.
    async fn lookup_by_deploy_id(&self, deploy_id: &DeployId) -> Option<BlockHash>;

    /// Add a deploy to the (unprocessed) deploy pool.
    async fn add_deploy(&self, deploy: SignedDeployData);

    async fn pooled_deploys(&self) -> BTreeMap<DeployId, SignedDeployData>;

    async fn contains_deploy_in_pool(&self, deploy_id: &DeployId) -> bool;
}
