//! Block validation predicates (port of `Validate.scala`) — the pure, effect-free checks.

use rchain_models::block_hash::BlockHash;
use rchain_models::block_version::SUPPORTED;
use rchain_models::casper::protocol::casper_message::BlockMessage;

use crate::block_status::BlockStatus;
use crate::proto_util::hash_block;

/// Validate that the block's identifying fields are non-empty (port of `formatOfFields`).
pub fn format_of_fields(b: &BlockMessage) -> bool {
    if b.block_hash == BlockHash::new([0u8; 32]) {
        false
    } else if b.sig.is_empty() {
        false
    } else if b.sig_algorithm.is_empty() {
        false
    } else if b.shard_id.is_empty() {
        false
    } else if b.post_state_hash.is_empty() {
        false
    } else {
        true
    }
}

/// Validate that the block version is supported (port of `version`).
pub fn version(b: &BlockMessage) -> bool {
    SUPPORTED.contains(&b.version)
}

/// Validate that the block hash matches its content-addressed value (Law 16; port of `blockHash`).
pub fn block_hash(b: &BlockMessage) -> bool {
    b.block_hash == hash_block(b)
}

/// Validate that no deploy is scheduled for a future block (port of `futureTransaction`).
pub fn future_transaction(b: &BlockMessage) -> BlockStatus {
    if b.state
        .deploys
        .iter()
        .any(|d| d.deploy.data.valid_after_block_number > b.block_number)
    {
        BlockStatus::ContainsFutureDeploy
    } else {
        BlockStatus::Valid
    }
}

/// Validate that no deploy has expired (port of `transactionExpiration`).
pub fn transaction_expiration(b: &BlockMessage, expiration_threshold: i64) -> BlockStatus {
    let earliest = b.block_number - expiration_threshold;
    if b.state
        .deploys
        .iter()
        .any(|d| d.deploy.data.valid_after_block_number <= earliest)
    {
        BlockStatus::ContainsExpiredDeploy
    } else {
        BlockStatus::Valid
    }
}

/// Validate that all deploys belong to the validator's shard (port of `deploysShardIdentifier`).
pub fn deploys_shard_identifier(b: &BlockMessage, shard_id: &str) -> BlockStatus {
    if b.state.deploys.iter().all(|d| d.deploy.data.shard_id == shard_id) {
        BlockStatus::Valid
    } else {
        BlockStatus::InvalidDeployShardId
    }
}

/// Validate that all deploys meet the minimum phlo price (port of `phloPrice`).
pub fn phlo_price(b: &BlockMessage, min_phlo_price: i64) -> BlockStatus {
    if b.state
        .deploys
        .iter()
        .all(|d| d.deploy.data.phlo_price >= min_phlo_price)
    {
        BlockStatus::Valid
    } else {
        BlockStatus::ContainsLowCostDeploy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    use rchain_models::casper::protocol::casper_message::{
        DeployData, ProcessedDeploy, RholangState, SignedDeployData, PCost,
    };
    use rchain_models::validator::Validator;

    fn deploy(valid_after: i64, phlo_price: i64, shard_id: &str) -> ProcessedDeploy {
        ProcessedDeploy {
            deploy: SignedDeployData {
                data: DeployData {
                    term: "Nil".to_string(),
                    timestamp: 0,
                    phlo_price,
                    phlo_limit: 100,
                    valid_after_block_number: valid_after,
                    shard_id: shard_id.to_string(),
                },
                deployer: vec![],
                sig: vec![1],
                sig_algorithm: "secp256k1".to_string(),
            },
            cost: PCost { cost: 0 },
            deploy_log: vec![],
            is_failed: false,
            system_deploy_error: None,
        }
    }

    fn block() -> BlockMessage {
        BlockMessage {
            version: 1,
            shard_id: "root".to_string(),
            block_hash: BlockHash::new([0xab; 32]),
            block_number: 10,
            sender: Validator::new([0x11; 65]),
            seq_num: 0,
            pre_state_hash: vec![1],
            post_state_hash: vec![2],
            justifications: vec![],
            bonds: BTreeMap::new(),
            rejected_deploys: BTreeSet::new(),
            rejected_blocks: BTreeSet::new(),
            rejected_senders: BTreeSet::new(),
            state: RholangState {
                deploys: vec![],
                system_deploys: vec![],
            },
            sig_algorithm: "secp256k1".to_string(),
            sig: vec![1],
        }
    }

    #[test]
    fn version_and_format_checks() {
        let mut b = block();
        assert!(version(&b));
        b.version = 2;
        assert!(!version(&b));
        b.version = 1;
        assert!(format_of_fields(&b));
        b.sig = vec![];
        assert!(!format_of_fields(&b));
    }

    #[test]
    fn block_hash_detects_tampering() {
        let mut b = block();
        let h = hash_block(&b);
        b.block_hash = h;
        assert!(block_hash(&b));
        b.block_number = 999;
        assert!(!block_hash(&b));
    }

    #[test]
    fn deploy_validators() {
        let mut b = block();
        b.state.deploys = vec![deploy(5, 10, "root")];
        assert_eq!(future_transaction(&b), BlockStatus::Valid);
        assert_eq!(transaction_expiration(&b, 100), BlockStatus::Valid);
        assert_eq!(deploys_shard_identifier(&b, "root"), BlockStatus::Valid);
        assert_eq!(phlo_price(&b, 10), BlockStatus::Valid);

        b.state.deploys = vec![deploy(20, 10, "root")];
        assert_eq!(future_transaction(&b), BlockStatus::ContainsFutureDeploy);

        b.state.deploys = vec![deploy(0, 10, "other")];
        assert_eq!(deploys_shard_identifier(&b, "root"), BlockStatus::InvalidDeployShardId);

        b.state.deploys = vec![deploy(5, 1, "root")];
        assert_eq!(phlo_price(&b, 10), BlockStatus::ContainsLowCostDeploy);
    }
}
