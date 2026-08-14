//! Finalized-fringe data.
//!
//! Mirrors `models/src/main/scala/coop/rchain/models/FringeData.scala`. `Hash` is overridden to
//! hash only `fringe_hash` (mirrors the Scala), and `fringe_hash` is computed over the *sorted*
//! fringe (Law 18: fringe identity order-independent). Wire `from_proto`/`to_proto` are deferred
//! to the prost layer.

use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;

use crate::block_hash::BlockHash;

/// Fringe data (fringe identity + rejected deploy/block/sender data).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FringeData {
    pub fringe_hash: Blake2b256Hash,
    pub fringe: BTreeSet<BlockHash>,
    pub fringe_diff: BTreeSet<BlockHash>,
    pub state_hash: Blake2b256Hash,
    pub rejected_deploys: BTreeSet<Vec<u8>>,
    pub rejected_blocks: BTreeSet<BlockHash>,
    pub rejected_senders: BTreeSet<Vec<u8>>,
}

// FringeData is uniquely identified by the hash of its fringe hashes (per the Scala).
impl Hash for FringeData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fringe_hash.hash(state);
    }
}

impl FringeData {
    /// Hash of the (sorted) fringe — used as the fringe store primary key (Law 18).
    pub fn fringe_hash_of(fringe: &BTreeSet<BlockHash>) -> Blake2b256Hash {
        let parts: Vec<&[u8]> = fringe.iter().map(|h| h.as_bytes() as &[u8]).collect();
        Blake2b256Hash::create_many(&parts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: u8) -> BlockHash {
        let mut bytes = [0u8; 32];
        bytes[0] = byte;
        BlockHash::new(bytes)
    }

    #[test]
    fn law18_fringe_hash_is_order_independent() {
        // `fringe_hash` is over the sorted fringe, so it is independent of input order.
        let h1 = hash(1);
        let h2 = hash(2);
        let h3 = hash(3);
        let fringe: BTreeSet<BlockHash> = [h3, h1, h2].into_iter().collect();
        let expected = {
            let parts: Vec<&[u8]> = [&h1, &h2, &h3].iter().map(|h| h.as_bytes() as &[u8]).collect();
            Blake2b256Hash::create_many(&parts)
        };
        assert_eq!(FringeData::fringe_hash_of(&fringe), expected);
    }
}
