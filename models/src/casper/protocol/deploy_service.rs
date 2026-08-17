//! Casper deploy-service API protocol types (port of `DeployService{Common,V1}.proto`).
//!
//! Hand-written data structs (no protobuf wire format) — the wire serialization is deferred.

use serde::{Deserialize, Serialize};

use crate::ast::Par;
use crate::runtime::BindPattern;

/// A validator bond (port of `BondInfo`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BondInfo {
    pub validator: String,
    pub stake: i64,
}

/// Lightweight block metadata exposed to clients (port of `LightBlockInfo`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

// -------------------------------------------------------------------------------------------------
// gRPC service queries (port of the `*Query` messages in `DeployServiceCommon.proto`)
// -------------------------------------------------------------------------------------------------

/// A deploy-service error (port of `ServiceError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceError {
    pub messages: Vec<String>,
}

impl ServiceError {
    pub fn new(message: impl Into<String>) -> Self {
        ServiceError {
            messages: vec![message.into()],
        }
    }
}

/// `FindDeployQuery` (deploy id → containing block).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindDeployQuery {
    pub deploy_id: Vec<u8>,
}

/// `BlockQuery` (block hash → block info).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockQuery {
    pub hash: String,
}

/// `ReportQuery` (block report by hash, optionally forcing replay).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportQuery {
    pub hash: String,
    pub force_replay: bool,
}

/// `BlocksQuery` (latest blocks by depth).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlocksQuery {
    pub depth: i32,
}

/// `BlocksQueryByHeight` (blocks in an inclusive height range).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlocksQueryByHeight {
    pub start_block_number: i64,
    pub end_block_number: i64,
}

/// `DataAtNameQuery` (data sent to a name, by depth).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataAtNameQuery {
    pub depth: i32,
    pub name: Par,
}

/// `DataAtNameByBlockQuery` (data sent to a name at a specific block).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataAtNameByBlockQuery {
    pub par: Par,
    pub block_hash: String,
    pub use_pre_state_hash: bool,
}

/// `ContinuationAtNameQuery` (continuations listening on names).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContinuationAtNameQuery {
    pub depth: i32,
    pub names: Vec<Par>,
}

/// `VisualizeDagQuery` (Graphviz DAG rendering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualizeDagQuery {
    pub depth: i32,
    pub show_justification_lines: bool,
    pub start_block_number: i32,
}

/// `MachineVerifyQuery` (machine-verifiable DAG edges).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MachineVerifyQuery {
    pub depth: i32,
}

/// `IsFinalizedQuery` (finality check for a block hash).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IsFinalizedQuery {
    pub hash: String,
}

/// `BondStatusQuery` (bond check for a validator public key).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BondStatusQuery {
    pub public_key: Vec<u8>,
}

/// `ExploratoryDeployQuery` (read-only deploy with immediate rollback).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExploratoryDeployQuery {
    pub term: String,
    pub block_hash: String,
    pub use_pre_state_hash: bool,
}
