//! Wire codecs bridging the typed store to protobuf (+LZ4 for block messages).
//!
//! Mirrors `block-storage/src/main/scala/coop/rchain/blockstorage/dag/codecs.scala`. The scodec
//! `Codec` becomes [`rchain_shared::typed_store::Codec`]; `bytes(BlockHash.Length)` becomes the raw
//! 32-byte [`BlockHashCodec`], and `codecBlockMessage` composes LZ4 with protobuf.

use rchain_models::block_hash::BlockHash;
use rchain_models::block_metadata::BlockMetadata;
use rchain_models::casper::protocol::casper_message::{BlockMessage, FinalizedFringe};
use rchain_models::fringe_data::FringeData;
use rchain_shared::typed_store::Codec;

use crate::block_store::{block_message_to_bytes, bytes_to_block_message};

/// Raw 32-byte block-hash codec (the Scala `bytes(BlockHash.Length)`).
#[derive(Default)]
pub struct BlockHashCodec;

impl Codec<BlockHash> for BlockHashCodec {
    fn encode(&self, value: &BlockHash) -> Vec<u8> {
        value.as_bytes().to_vec()
    }

    fn decode(&self, bytes: &[u8]) -> Result<BlockHash, String> {
        if bytes.len() != 32 {
            return Err(format!("expected 32 bytes, got {}", bytes.len()));
        }
        Ok(BlockHash::from_slice(bytes))
    }
}

/// Protobuf + LZ4 block-message codec (the Scala `codecBlockMessage`).
#[derive(Default)]
pub struct BlockMessageCodec;

impl Codec<BlockMessage> for BlockMessageCodec {
    fn encode(&self, value: &BlockMessage) -> Vec<u8> {
        block_message_to_bytes(value)
    }

    fn decode(&self, bytes: &[u8]) -> Result<BlockMessage, String> {
        bytes_to_block_message(bytes)
    }
}

/// Protobuf block-metadata codec (the Scala `codecBlockMetadata`).
#[derive(Default)]
pub struct BlockMetadataCodec;

impl Codec<BlockMetadata> for BlockMetadataCodec {
    fn encode(&self, value: &BlockMetadata) -> Vec<u8> {
        value.to_bytes()
    }

    fn decode(&self, bytes: &[u8]) -> Result<BlockMetadata, String> {
        BlockMetadata::from_bytes(bytes).map_err(|e| e.to_string())
    }
}

/// Protobuf fringe-data codec (the Scala `codecFringeData`).
#[derive(Default)]
pub struct FringeDataCodec;

impl Codec<FringeData> for FringeDataCodec {
    fn encode(&self, value: &FringeData) -> Vec<u8> {
        value.to_bytes()
    }

    fn decode(&self, bytes: &[u8]) -> Result<FringeData, String> {
        FringeData::from_bytes(bytes).map_err(|e| e.to_string())
    }
}

/// Protobuf finalized-fringe codec (the Scala `codecFringe`).
#[derive(Default)]
pub struct FringeCodec;

impl Codec<FinalizedFringe> for FringeCodec {
    fn encode(&self, value: &FinalizedFringe) -> Vec<u8> {
        value.to_bytes()
    }

    fn decode(&self, bytes: &[u8]) -> Result<FinalizedFringe, String> {
        FinalizedFringe::from_bytes(bytes).map_err(|e| e.to_string())
    }
}

/// Single-byte codec (the Scala `byte`, used for the approved-store key).
#[derive(Default)]
pub struct ByteCodec;

impl Codec<u8> for ByteCodec {
    fn encode(&self, value: &u8) -> Vec<u8> {
        vec![*value]
    }

    fn decode(&self, bytes: &[u8]) -> Result<u8, String> {
        match bytes {
            [b] => Ok(*b),
            _ => Err(format!("expected 1 byte, got {}", bytes.len())),
        }
    }
}
