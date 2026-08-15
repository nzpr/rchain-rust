//! Packet type tags + packet serialization.
//!
//! Mirrors `models/src/main/scala/coop/rchain/casper/protocol/PacketTypeTag.scala`. The Scala
//! enumeratum `PacketTypeTag` becomes a Rust enum; `ToPacket`/`FromPacket` become traits. Concrete
//! per-message serde instances are wired in `comm` (P6), where packets are actually dispatched.

use crate::proto::routing::Packet;

/// A packet type tag (port of the `PacketTypeTag` enum; the wire tag is the Scala `entryName`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PacketTypeTag {
    BlockHashMessage,
    BlockMessage,
    HasBlockRequest,
    HasBlock,
    BlockRequest,
    ForkChoiceTipRequest,
    FinalizedFringeRequest,
    FinalizedFringe,
    StoreItemsMessageRequest,
    StoreItemsMessage,
}

impl PacketTypeTag {
    /// The wire tag string (the Scala `entryName`).
    pub fn tag(&self) -> &'static str {
        match self {
            PacketTypeTag::BlockHashMessage => "BlockHashMessage",
            PacketTypeTag::BlockMessage => "BlockMessage",
            PacketTypeTag::HasBlockRequest => "HasBlockRequest",
            PacketTypeTag::HasBlock => "HasBlock",
            PacketTypeTag::BlockRequest => "BlockRequest",
            PacketTypeTag::ForkChoiceTipRequest => "ForkChoiceTipRequest",
            PacketTypeTag::FinalizedFringeRequest => "FinalizedFringeRequest",
            PacketTypeTag::FinalizedFringe => "FinalizedFringe",
            PacketTypeTag::StoreItemsMessageRequest => "StoreItemsMessageRequest",
            PacketTypeTag::StoreItemsMessage => "StoreItemsMessage",
        }
    }
}

/// A packet parse result (port of `PacketParseResult`).
pub type PacketParseResult<A> = Result<A, crate::errors::ModelsError>;

/// Serialize a model into a `Packet` (port of `ToPacket[A]`).
pub trait ToPacket<A> {
    fn tag(&self) -> PacketTypeTag;
    fn content(&self, model: &A) -> Vec<u8>;

    fn mk_packet(&self, model: &A) -> Packet {
        Packet {
            type_id: self.tag().tag().to_string(),
            content: self.content(model),
        }
    }
}

/// Parse a `Packet` back into a model (port of `FromPacket[Tag]`).
pub trait FromPacket<A>: ToPacket<A> {
    fn parse(&self, content: &[u8]) -> PacketParseResult<A>;

    fn parse_from(&self, packet: &Packet) -> PacketParseResult<A> {
        if packet.type_id == self.tag().tag() {
            self.parse(&packet.content)
        } else {
            Err(crate::errors::ModelsError::PacketTypeMismatch {
                got: packet.type_id.clone(),
                expected: self.tag().tag().to_string(),
            })
        }
    }
}
