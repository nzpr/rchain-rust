//! Casper deploy-service API protocol types (port of `DeployService{Common,V1}.proto`).
//!
//! Hand-written data structs (no protobuf wire format) — the wire serialization is deferred.

use crate::ast::Par;
use crate::runtime::BindPattern;

/// A validator bond (port of `BondInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BondInfo {
    pub validator: String,
    pub stake: i64,
}

/// Lightweight block metadata exposed to clients (port of `LightBlockInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LightBlockInfo {
    pub version: i32,
    pub shard_id: String,
    pub block_hash: String,
    pub block_number: i64,
    pub sender: String,
    pub seq_num: i64,
    pub pre_state_hash: String,
    pub post_state_hash: String,
    pub justifications: Vec<String>,
    pub bonds: Vec<BondInfo>,
    pub sig_algorithm: String,
    pub sig: String,
    pub block_size: String,
    pub deploy_count: i32,
    pub rejected_deploys: Vec<String>,
}

/// Deploy metadata (port of `DeployInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeployInfo {
    pub deployer: String,
    pub term: String,
    pub timestamp: i64,
    pub sig: String,
    pub sig_algorithm: String,
    pub phlo_price: i64,
    pub phlo_limit: i64,
    pub valid_after_block_number: i64,
    pub cost: u64,
    pub errored: bool,
    pub system_deploy_error: String,
}

/// A block plus its deploys (port of `BlockInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockInfo {
    pub block_info: LightBlockInfo,
    pub deploys: Vec<DeployInfo>,
}

/// Post-block data plus the block it belongs to (port of `DataWithBlockInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataWithBlockInfo {
    pub post_block_data: Vec<Par>,
    pub block: LightBlockInfo,
}

/// API/node version info (port of `VersionInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionInfo {
    pub api: String,
    pub node: String,
}

/// Node status (port of `Status`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub version: VersionInfo,
    pub address: String,
    pub network_id: String,
    pub shard_id: String,
    pub peers: i32,
    pub nodes: i32,
    pub min_phlo_price: i64,
    pub latest_block_number: i64,
}

/// Post-block continuations plus the block they belong to (port of
/// `ContinuationsWithBlockInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContinuationsWithBlockInfo {
    pub post_block_continuations: Vec<WaitingContinuationInfo>,
    pub block: LightBlockInfo,
}

/// A single waiting continuation at a name (port of `WaitingContinuationInfo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaitingContinuationInfo {
    pub post_block_patterns: Vec<BindPattern>,
    pub post_block_continuation: Par,
}

/// A deploy execution status (port of `DeployExecStatus`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeployExecStatus {
    ProcessedWithSuccess {
        deploy_result: Vec<Par>,
        block: LightBlockInfo,
    },
    ProcessedWithError {
        deploy_error: String,
        block: LightBlockInfo,
    },
    NotProcessed {
        status: String,
    },
}
