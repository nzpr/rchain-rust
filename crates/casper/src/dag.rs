//! Block DAG key-value storage (port of `casper/dag/BlockDagKeyValueStorage.scala`).
//!
//! The full `BlockDagKeyValueStorage` is an async store over `BlockMetadataStore` + the
//! deploy/fringe stores; the pure helpers it relies on are ported here first.

use std::collections::{BTreeMap, BTreeSet};

use rchain_block_storage::dag::finalizer::Message;
use rchain_models::block_hash::BlockHash;
use rchain_models::block_metadata::BlockMetadata;
use rchain_models::validator::Validator;

/// Build a `Message` from a `BlockMetadata` given the current message map (port of
/// `BlockDagKeyValueStorage.messageFromBlockMetadata`). A missing justification panics, exactly as
/// the Scala `Map.apply` does.
pub fn message_from_block_metadata(
    block: &BlockMetadata,
    msg_map: &BTreeMap<BlockHash, Message<BlockHash, Validator>>,
) -> Message<BlockHash, Validator> {
    let seen: BTreeSet<BlockHash> = block
        .justifications
        .iter()
        .flat_map(|p| {
            msg_map
                .get(p)
                .expect("justification not present in message map")
                .seen
                .iter()
                .copied()
        })
        .chain(std::iter::once(block.block_hash))
        .collect();
    Message {
        id: block.block_hash,
        height: block.block_num,
        sender: block.sender,
        sender_seq: block.seq_num,
        bonds_map: block.bonds_map.clone(),
        parents: block.justifications.clone(),
        fringe: block.fringe.clone(),
        seen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rchain_models::block::state_hash::StateHash;

    fn hash(byte: u8) -> BlockHash {
        let mut bytes = [0u8; 32];
        bytes[0] = byte;
        BlockHash::new(bytes)
    }

    fn meta(hash: BlockHash, parents: &[BlockHash], block_num: i64) -> BlockMetadata {
        BlockMetadata {
            block_hash: hash,
            block_num,
            sender: Validator::new([0u8; 65]),
            seq_num: 0,
            justifications: parents.iter().copied().collect(),
            bonds_map: BTreeMap::new(),
            validated: true,
            validation_failed: false,
            fringe: BTreeSet::new(),
            fringe_state_hash: StateHash::new([0u8; 32]),
            member_of_fringe: None,
        }
    }

    #[test]
    fn seen_is_justifications_seen_plus_own_hash() {
        let genesis_hash = hash(0);
        let genesis_meta = meta(genesis_hash, &[], 0);
        let genesis = message_from_block_metadata(&genesis_meta, &BTreeMap::new());
        assert_eq!(genesis.seen, [genesis_hash].into_iter().collect());

        let mut map = BTreeMap::new();
        map.insert(genesis_hash, genesis);
        let child_hash = hash(1);
        let child_meta = meta(child_hash, &[genesis_hash], 1);
        let child = message_from_block_metadata(&child_meta, &map);
        assert_eq!(child.seen, [genesis_hash, child_hash].into_iter().collect());
        assert_eq!(child.parents, [genesis_hash].into_iter().collect());
    }

    #[test]
    #[should_panic(expected = "justification not present")]
    fn missing_justification_panics() {
        let child_meta = meta(hash(1), &[hash(0)], 1);
        let _ = message_from_block_metadata(&child_meta, &BTreeMap::new());
    }
}
