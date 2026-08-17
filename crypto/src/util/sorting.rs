//! Orderings.
//!
//! Mirrors `crypto/src/main/scala/coop/rchain/crypto/util/Sorting.scala`.

use std::cmp::Ordering;

/// Lexicographic ordering over signed bytes (mirrors Scala `Ordering.by((_: Array[Byte]).toIterable)`).
pub fn compare_byte_arrays(a: &[u8], b: &[u8]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let ord = (*x as i8).cmp(&(*y as i8));
        if ord != Ordering::Equal {
            return ord;
        }
    }
    a.len().cmp(&b.len())
}
