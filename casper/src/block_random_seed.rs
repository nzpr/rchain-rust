//! Block random seed (port of `rholang/BlockRandomSeed.scala`).
//!
//! Deterministically derives a [`Blake2b512Random`] from a block's identity fields so that
//! deployment and genesis unforgeable names are reproducible.

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_crypto::hash::blake2b512_random::Blake2b512Random;
use rchain_crypto::public_key::PublicKey;
use rchain_models::ast::Par;
use rchain_models::casper::protocol::casper_message::BlockMessage;
use rchain_models::rholang::RhoType::RhoName;

/// Random-seed split index for the pre-charge deploy (port of `PreChargeSplitIndex`).
pub const PRE_CHARGE_SPLIT_INDEX: u8 = 0;
/// Random-seed split index for the user deploy (port of `UserDeploySplitIndex`).
pub const USER_DEPLOY_SPLIT_INDEX: u8 = 1;
/// Random-seed split index for the refund deploy (port of `RefundSplitIndex`).
pub const REFUND_SPLIT_INDEX: u8 = 2;

/// A deterministic random seed derived from a block (port of `BlockRandomSeed`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockRandomSeed {
    pub shard_id: String,
    pub block_number: i64,
    pub sender: PublicKey,
    pub pre_state_hash: Blake2b256Hash,
}

impl BlockRandomSeed {
    pub fn new(
        shard_id: String,
        block_number: i64,
        sender: PublicKey,
        pre_state_hash: Blake2b256Hash,
    ) -> Self {
        assert!(
            shard_id.is_ascii(),
            "Shard name should contain only ASCII characters"
        );
        BlockRandomSeed {
            shard_id,
            block_number,
            sender,
            pre_state_hash,
        }
    }

    /// The genesis seed (port of `BlockRandomSeed.apply(shardId)`).
    pub fn from_shard_id(shard_id: &str) -> Self {
        BlockRandomSeed::new(
            shard_id.to_string(),
            0,
            PublicKey::new(vec![]),
            Blake2b256Hash::create(&[]),
        )
    }

    /// The seed for a block (port of `BlockRandomSeed.apply(block)`).
    pub fn from_block(block: &BlockMessage) -> Self {
        if block.justifications.is_empty() {
            BlockRandomSeed::from_shard_id(&block.shard_id)
        } else {
            BlockRandomSeed::new(
                block.shard_id.clone(),
                block.block_number,
                PublicKey::new(block.sender.as_bytes().to_vec()),
                Blake2b256Hash::from_byte_array(&block.pre_state_hash),
            )
        }
    }

    /// Serialize the seed (port of the scodec `codecBlockRandomSeed`).
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = var_size(self.shard_id.as_bytes());
        bytes.extend(vlong_encode(self.block_number));
        bytes.extend(var_size(self.sender.bytes()));
        bytes.extend_from_slice(self.pre_state_hash.as_bytes());
        bytes
    }

    /// Derive the random generator from this seed (port of `randomGenerator(BlockRandomSeed)`).
    pub fn random_generator(&self) -> Blake2b512Random {
        Blake2b512Random::from_init(&self.encode())
    }

    /// `randomGenerator(shardId, blockNumber, sender, preStateHash)`.
    pub fn random_generator_from(
        shard_id: &str,
        block_number: i64,
        sender: PublicKey,
        pre_state_hash: Blake2b256Hash,
    ) -> Blake2b512Random {
        BlockRandomSeed::new(shard_id.to_string(), block_number, sender, pre_state_hash)
            .random_generator()
    }

    /// `randomGenerator(shardId)` (genesis).
    pub fn random_generator_from_shard_id(shard_id: &str) -> Blake2b512Random {
        BlockRandomSeed::from_shard_id(shard_id).random_generator()
    }

    /// `randomGenerator(block)`.
    pub fn random_generator_from_block(block: &BlockMessage) -> Blake2b512Random {
        BlockRandomSeed::from_block(block).random_generator()
    }

    /// `splitRandomNumberFromGenesis`.
    pub fn split_random_number_from_genesis(
        shard_id: &str,
        index: u8,
        index2: u8,
    ) -> Blake2b512Random {
        BlockRandomSeed::random_generator_from_shard_id(shard_id)
            .split_byte(index)
            .split_byte(index2)
    }

    /// The unforgeable name of the NonNegativeNumber contract (port of
    /// `nonNegativeMergeableTagName`).
    pub fn non_negative_mergeable_tag_name(shard_id: &str) -> Par {
        let mut rand =
            BlockRandomSeed::split_random_number_from_genesis(shard_id, 3, USER_DEPLOY_SPLIT_INDEX);
        rand.next();
        RhoName::apply_bytes(rand.next())
    }

    /// The REV transfer unforgeable name (port of `transferUnforgeable`).
    pub fn transfer_unforgeable(shard_id: &str) -> Par {
        let mut rand =
            BlockRandomSeed::split_random_number_from_genesis(shard_id, 6, USER_DEPLOY_SPLIT_INDEX);
        for _ in 0..10 {
            rand.next();
        }
        RhoName::apply_bytes(rand.next())
    }

    /// The store-token unforgeable name (port of `storeTokenUnforgeable`).
    pub fn store_token_unforgeable(shard_id: &str) -> Par {
        let mut rand =
            BlockRandomSeed::split_random_number_from_genesis(shard_id, 0, USER_DEPLOY_SPLIT_INDEX);
        for _ in 0..9 {
            rand.next();
        }
        RhoName::apply_bytes(rand.next())
    }

    /// The REV vault unforgeable name (port of `revVaultUnforgeable`).
    pub fn rev_vault_unforgeable(shard_id: &str) -> Par {
        let mut rand =
            BlockRandomSeed::split_random_number_from_genesis(shard_id, 6, USER_DEPLOY_SPLIT_INDEX);
        RhoName::apply_bytes(rand.next())
    }
}

/// `variableSizeBytes(uint8, X)` — a 1-byte length prefix followed by the bytes.
fn var_size(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
    out
}

/// Zigzag-encode an `i64` into `u64`.
fn zigzag_encode(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

/// LEB128 varint-encode a `u64`.
fn varint_encode(mut n: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while n >= 0x80 {
        out.push((n as u8 & 0x7f) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
    out
}

/// scodec `vlong` — zigzag + LEB128 varint.
fn vlong_encode(n: i64) -> Vec<u8> {
    varint_encode(zigzag_encode(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vlong_encodes_zero_and_small_values() {
        assert_eq!(vlong_encode(0), vec![0x00]);
        assert_eq!(vlong_encode(1), vec![0x02]);
        assert_eq!(vlong_encode(5), vec![0x0a]);
    }

    #[test]
    fn seed_is_deterministic() {
        let seed = BlockRandomSeed::from_shard_id("root");
        assert_eq!(seed.encode(), BlockRandomSeed::from_shard_id("root").encode());
        assert_ne!(
            seed.encode(),
            BlockRandomSeed::new("root".to_string(), 1, PublicKey::new(vec![]), Blake2b256Hash::create(&[])).encode()
        );
    }

    #[test]
    fn random_generator_produces_names() {
        let name = BlockRandomSeed::non_negative_mergeable_tag_name("root");
        assert!(!name.unforgeables.is_empty() || !name.exprs.is_empty());
    }
}
