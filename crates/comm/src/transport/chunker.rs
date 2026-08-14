//! Packet chunking.
//!
//! Mirrors `comm/src/main/scala/coop/rchain/comm/transport/Chunker.scala`.

use rchain_models::comm::protocol::{chunk, Chunk, ChunkData, ChunkHeader, Packet};

use crate::peer_node::PeerNode;

/// A sender + packet to be chunked (port of the `Blob` used by `Chunker.chunkIt`).
#[derive(Clone, PartialEq)]
pub struct Blob {
    pub sender: PeerNode,
    pub packet: Packet,
}

/// Chunk a blob into a header chunk followed by data chunks (port of `Chunker.chunkIt`).
pub fn chunk_it(network_id: &str, blob: &Blob, max_message_size: usize) -> Vec<Chunk> {
    let raw = blob.packet.content.clone();
    let kb500 = 1024 * 500;
    let compress = raw.len() > kb500;
    let content = if compress {
        rchain_shared::compression::compress(&raw)
    } else {
        raw.clone()
    };

    let header = Chunk {
        content: Some(chunk::Content::Header(ChunkHeader {
            sender: Some(blob.sender.to_node()),
            type_id: blob.packet.type_id.clone(),
            compressed: compress,
            content_length: raw.len() as i32,
            network_id: network_id.to_string(),
        })),
    };

    let buffer = 2 * 1024;
    let chunk_size = max_message_size - buffer;
    let mut chunks = vec![header];
    for data in content.chunks(chunk_size) {
        chunks.push(Chunk {
            content: Some(chunk::Content::Data(ChunkData {
                content_data: data.to_vec(),
            })),
        });
    }
    chunks
}
