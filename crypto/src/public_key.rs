//! A public key.
//!
//! Mirrors `crypto/src/main/scala/coop/rchain/crypto/PublicKey.scala`.

use std::cmp::Ordering;

/// A public key, as a raw byte array.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct PublicKey(pub Vec<u8>);

impl PublicKey {
    /// Construct from raw bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The raw key bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

// The Scala `Sorting.publicKeyOrdering` orders public keys by signed-byte lexicographic comparison.
impl PartialOrd for PublicKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PublicKey {
    fn cmp(&self, other: &Self) -> Ordering {
        crate::util::sorting::compare_byte_arrays(&self.0, &other.0)
    }
}
