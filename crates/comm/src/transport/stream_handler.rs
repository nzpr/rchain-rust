//! Streamed-message reassembly.
//!
//! Mirrors `comm/src/main/scala/coop/rchain/comm/transport/StreamHandler.scala`. The Monix
//! `Observable` fold becomes a plain fold over `&[Chunk]`, and the `TrieMap` cache becomes a
//! `HashMap<String, Vec<u8>>`. Only the pure reassembly/decompression logic is ported here.

use std::collections::HashMap;

use rchain_models::comm::protocol::{chunk, Chunk};

use crate::peer_node::PeerNode;
use crate::rp::protocol_helper::blob;
use crate::transport::chunker::Blob;

/// A streamed-message header (the local `Header`, distinct from the routing `Header`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub sender: PeerNode,
    pub type_id: String,
    pub content_length: i32,
    pub network_id: String,
    pub compressed: bool,
}

/// A stream error (port of `StreamHandler.StreamError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamError {
    WrongNetworkId,
    MaxSizeReached,
    NotFullMessage(String),
    Unexpected(String),
}

/// The stream circuit state (port of `Circuit`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Circuit {
    Opened(StreamError),
    Closed,
}

impl Circuit {
    pub fn broken(&self) -> bool {
        matches!(self, Circuit::Opened(_))
    }
}

/// In-progress stream state (port of `Streamed`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Streamed {
    pub header: Option<Header>,
    pub read_so_far: i64,
    pub circuit: Circuit,
    pub key: String,
}

impl Streamed {
    pub fn new(key: String) -> Self {
        Streamed {
            header: None,
            read_so_far: 0,
            circuit: Circuit::Closed,
            key,
        }
    }
}

/// A circuit breaker deciding whether a stream has reached an error state.
pub type CircuitBreaker = dyn Fn(&Streamed) -> Circuit;

/// Fold chunks into stream state, honoring the circuit breaker (port of `collect`).
pub fn collect(
    init: &Streamed,
    chunks: &[Chunk],
    breaker: &CircuitBreaker,
    cache: &mut HashMap<String, Vec<u8>>,
) -> Result<Streamed, StreamError> {
    let mut stmd = init.clone();
    for chunk in chunks {
        match &chunk.content {
            Some(chunk::Content::Header(h)) => {
                let sender = h
                    .sender
                    .as_ref()
                    .ok_or_else(|| StreamError::Unexpected("chunk header missing sender".to_string()))?;
                stmd.header = Some(Header {
                    sender: PeerNode::from_node(sender),
                    type_id: h.type_id.clone(),
                    content_length: h.content_length,
                    network_id: h.network_id.clone(),
                    compressed: h.compressed,
                });
                let circuit = breaker(&stmd);
                if circuit.broken() {
                    stmd.circuit = circuit;
                    break;
                }
            }
            Some(chunk::Content::Data(d)) => {
                cache
                    .entry(stmd.key.clone())
                    .or_default()
                    .extend_from_slice(&d.content_data);
                stmd.read_so_far += d.content_data.len() as i64;
                let circuit = breaker(&stmd);
                if circuit.broken() {
                    stmd.circuit = circuit;
                    break;
                }
            }
            None => {
                stmd.circuit = Circuit::Opened(StreamError::NotFullMessage(
                    "Not all data received".to_string(),
                ));
                break;
            }
        }
    }
    match stmd.circuit {
        Circuit::Opened(e) => Err(e),
        Circuit::Closed => Ok(stmd),
    }
}

/// Check that a completed stream is full and produce a `StreamMessage` (port of `toResult`).
pub fn to_result(stmd: &Streamed) -> Result<crate::transport::messages::StreamMessage, StreamError> {
    match &stmd.header {
        Some(h) => {
            let result = crate::transport::messages::StreamMessage {
                sender: h.sender.clone(),
                type_id: h.type_id.clone(),
                key: stmd.key.clone(),
                compressed: h.compressed,
                content_length: h.content_length,
            };
            if !h.compressed && stmd.read_so_far != h.content_length as i64 {
                Err(StreamError::NotFullMessage(format!("{stmd:?}")))
            } else {
                Ok(result)
            }
        }
        None => Err(StreamError::NotFullMessage(format!("{stmd:?}"))),
    }
}

/// Reassemble a `Blob` from a `StreamMessage` and its cached bytes (port of `restore`).
pub fn restore(
    msg: &crate::transport::messages::StreamMessage,
    cache: &mut HashMap<String, Vec<u8>>,
) -> Result<Blob, String> {
    let content = cache
        .remove(&msg.key)
        .ok_or_else(|| "Could not read streamed data from cache".to_string())?;
    let decompressed = decompress_content(&content, msg.compressed, msg.content_length)?;
    Ok(blob(msg.sender.clone(), msg.type_id.clone(), decompressed))
}

/// Decompress if flagged (port of `decompressContent`).
pub fn decompress_content(
    raw: &[u8],
    compressed: bool,
    content_length: i32,
) -> Result<Vec<u8>, String> {
    if compressed {
        rchain_shared::compression::decompress(raw, content_length as usize)
            .ok_or_else(|| "Could not decompress data".to_string())
    } else {
        Ok(raw.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer_node::NodeIdentifier;
    use rchain_models::comm::protocol::{ChunkData, ChunkHeader};

    fn peer() -> PeerNode {
        PeerNode::from(NodeIdentifier::new(vec![1, 2, 3]), "host".into(), 40400, 40404)
    }

    fn header_chunk(content_length: i32, compressed: bool) -> Chunk {
        Chunk {
            content: Some(chunk::Content::Header(ChunkHeader {
                sender: Some(peer().to_node()),
                type_id: "BlockMessage".to_string(),
                compressed,
                content_length,
                network_id: "testnet".to_string(),
            })),
        }
    }

    fn data_chunk(data: &[u8]) -> Chunk {
        Chunk {
            content: Some(chunk::Content::Data(ChunkData {
                content_data: data.to_vec(),
            })),
        }
    }

    fn closed_breaker(_: &Streamed) -> Circuit {
        Circuit::Closed
    }

    #[test]
    fn collect_and_to_result_reassembles() {
        let init = Streamed::new("k".to_string());
        let chunks = vec![
            header_chunk(6, false),
            data_chunk(&[1, 2, 3]),
            data_chunk(&[4, 5, 6]),
        ];
        let mut cache = HashMap::new();
        let stmd = collect(&init, &chunks, &closed_breaker, &mut cache).unwrap();
        assert_eq!(stmd.read_so_far, 6);
        let msg = to_result(&stmd).unwrap();
        assert_eq!(msg.content_length, 6);
        assert_eq!(msg.type_id, "BlockMessage");
    }

    #[test]
    fn to_result_rejects_incomplete_stream() {
        let init = Streamed::new("k".to_string());
        let chunks = vec![header_chunk(6, false), data_chunk(&[1, 2])];
        let mut cache = HashMap::new();
        let stmd = collect(&init, &chunks, &closed_breaker, &mut cache).unwrap();
        assert!(matches!(to_result(&stmd), Err(StreamError::NotFullMessage(_))));
    }

    #[test]
    fn restore_decompresses() {
        let raw = b"hello world hello world hello world".to_vec();
        let compressed = rchain_shared::compression::compress(&raw);
        let msg = crate::transport::messages::StreamMessage {
            sender: peer(),
            type_id: "BlockMessage".to_string(),
            key: "k".to_string(),
            compressed: true,
            content_length: raw.len() as i32,
        };
        let mut cache = HashMap::from([("k".to_string(), compressed)]);
        let blob = restore(&msg, &mut cache).unwrap();
        assert_eq!(blob.packet.content, raw);
    }
}
