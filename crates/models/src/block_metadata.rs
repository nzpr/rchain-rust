//! Block metadata.
//!
//! Mirrors `models/src/main/scala/coop/rchain/models/BlockMetadata.scala`. `Hash` is overridden to
//! hash only `block_hash` (mirrors the Scala). Wire `from_proto`/`to_proto` are deferred to the
//! prost layer.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;

use crate::block::state_hash::StateHash;
use crate::block_hash::BlockHash;
use crate::validator::Validator;

/// A block's metadata (the block-storage DAG index entry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockMetadata {
    pub block_hash: BlockHash,
    pub block_num: i64,
    pub sender: Validator,
    pub seq_num: i64,
    pub justifications: BTreeSet<BlockHash>,
    pub bonds_map: BTreeMap<Validator, i64>,
    pub validated: bool,
    pub validation_failed: bool,
    pub fringe: BTreeSet<BlockHash>,
    pub fringe_state_hash: StateHash,
    pub member_of_fringe: Option<Blake2b256Hash>,
}

// BlockMetadata is uniquely identified by its block hash (per the Scala).
impl Hash for BlockMetadata {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.block_hash.hash(state);
    }
}
