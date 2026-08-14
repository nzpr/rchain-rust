//! Byte-level encoders matching scodec (the serialization foundation for Laws 7/8/10).
//!
//! Mirrors the scodec encodings used by `ScodecSerialize.scala` and `Serialize.codecByteVector`:
//! - `variableSizeBytesLong(int64, bytes)` = 8-byte big-endian length + raw bytes.
//! - `seqOfN(int32, codec)` = 4-byte big-endian count + per-element codec.
//! - `bool(8)` = a single byte.

use rchain_shared::serialize::Serialize;

/// `int64` length prefix + raw bytes (port of `variableSizeBytesLong(int64, bytes)`).
pub fn size_head(bytes: &[u8]) -> Vec<u8> {
    let mut out = (bytes.len() as i64).to_be_bytes().to_vec();
    out.extend_from_slice(bytes);
    out
}

/// Encode a sequence of byte vectors as `int32` count + `int64`-length-prefixed elements (port of
/// `codecSeqByteVector` = `seqOfN(int32, variableSizeBytesLong(int64, bytes))`).
pub fn encode_seq_byte_vectors(elements: &[Vec<u8>]) -> Vec<u8> {
    let mut out = (elements.len() as i32).to_be_bytes().to_vec();
    for element in elements {
        out.extend_from_slice(&(element.len() as i64).to_be_bytes());
        out.extend_from_slice(element);
    }
    out
}

/// Encode a boolean as an 8-bit value (port of `bool(8)`).
pub fn bool8(value: bool) -> Vec<u8> {
    vec![u8::from(value)]
}

/// Encode each element with its `Serialize` instance and sort by `ordByteVector` (port of
/// `toOrderedByteVectors`).
pub fn to_ordered_byte_vectors<A>(elements: &[A]) -> Vec<Vec<u8>>
where
    A: Serialize<A>,
{
    let mut encoded: Vec<Vec<u8>> = elements
        .iter()
        .map(|e| <A as Serialize<A>>::encode(e))
        .collect();
    encoded.sort_by(|a, b| crate::util::veccmp(a, b));
    encoded
}
