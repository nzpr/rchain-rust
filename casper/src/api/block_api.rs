//! Block-info constructors (port of `BlockApi.scala`).
//!
//! The `BlockApi` trait + `BlockApiImpl` are deferred (they need the runtime/DAG/web wiring).

use rchain_models::casper::protocol::casper_message::BlockMessage;
use rchain_models::casper::protocol::deploy_service::{BlockInfo, BondInfo, LightBlockInfo};
use rchain_models::validator::Validator;
use rchain_shared::base16;

/// Build a bond info (port of `bondToBondInfo`).
pub fn bond_to_bond_info(bond: (&Validator, i64)) -> BondInfo {
    BondInfo {
        validator: base16::encode(bond.0.as_bytes()),
        stake: bond.1,
    }
}

/// Build the full block info (port of `getFullBlockInfo`).
pub fn get_full_block_info(block: &BlockMessage) -> BlockInfo {
    construct_block_info(block)
}

/// Build the light block info (port of `getLightBlockInfo`).
pub fn get_light_block_info(block: &BlockMessage) -> LightBlockInfo {
    construct_light_block_info(block)
}

fn construct_block_info(block: &BlockMessage) -> BlockInfo {
    let light_block_info = construct_light_block_info(block);
    let deploys = block
        .state
        .deploys
        .iter()
        .map(|d| d.to_deploy_info())
        .collect();
    BlockInfo {
        block_info: light_block_info,
        deploys,
    }
}

fn construct_light_block_info(block: &BlockMessage) -> LightBlockInfo {
    LightBlockInfo {
        version: block.version,
        shard_id: block.shard_id.clone(),
        block_hash: base16::encode(block.block_hash.as_bytes()),
        block_number: block.block_number,
        sender: base16::encode(block.sender.as_bytes()),
        seq_num: block.seq_num,
        pre_state_hash: base16::encode(&block.pre_state_hash),
        post_state_hash: base16::encode(&block.post_state_hash),
        justifications: block
            .justifications
            .iter()
            .map(|h| base16::encode(h.as_bytes()))
            .collect(),
        bonds: block
            .bonds
            .iter()
            .map(|(v, s)| bond_to_bond_info((v, *s)))
            .collect(),
        sig_algorithm: block.sig_algorithm.clone(),
        sig: base16::encode(&block.sig),
        block_size: block.to_bytes().len().to_string(),
        deploy_count: block.state.deploys.len() as i32,
        rejected_deploys: block
            .rejected_deploys
            .iter()
            .map(|d| base16::encode(d))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rchain_models::block_hash::BlockHash;
    use rchain_models::casper::protocol::casper_message::RholangState;
    use std::collections::{BTreeMap, BTreeSet};

    fn block() -> BlockMessage {
        BlockMessage {
            version: 1,
            shard_id: "root".to_string(),
            block_hash: BlockHash::new([1u8; 32]),
            block_number: 5,
            sender: Validator::new([2u8; 65]),
            seq_num: 3,
            pre_state_hash: vec![0xab; 32],
            post_state_hash: vec![0xcd; 32],
            justifications: vec![BlockHash::new([9u8; 32])],
            bonds: BTreeMap::from([(Validator::new([2u8; 65]), 100)]),
            rejected_deploys: BTreeSet::new(),
            rejected_blocks: BTreeSet::new(),
            rejected_senders: BTreeSet::new(),
            state: RholangState::default(),
            sig_algorithm: "secp256k1".to_string(),
            sig: vec![0xee],
        }
    }

    #[test]
    fn light_block_info_renders_hashes_as_hex() {
        let info = get_light_block_info(&block());
        assert_eq!(info.block_number, 5);
        assert_eq!(info.seq_num, 3);
        assert_eq!(info.deploy_count, 0);
        assert_eq!(info.bonds.len(), 1);
        assert_eq!(info.bonds[0].stake, 100);
        assert!(info.block_hash.starts_with("0101"));
    }

    #[test]
    fn full_block_info_carries_deploys() {
        let info = get_full_block_info(&block());
        assert_eq!(info.block_info.block_number, 5);
        assert!(info.deploys.is_empty());
    }
}
