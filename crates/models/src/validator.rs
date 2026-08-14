//! A validator identity.
//!
//! Mirrors `models/src/main/scala/coop/rchain/models/Validator.scala`.

/// The length of a `Validator` in bytes (an uncompressed secp256k1 public key).
pub const LENGTH: usize = 65;

/// A 65-byte validator identity.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Validator([u8; LENGTH]);

impl Validator {
    pub fn new(bytes: [u8; LENGTH]) -> Self {
        Self(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Self {
        assert_eq!(bytes.len(), LENGTH, "expected {LENGTH} bytes");
        let mut arr = [0u8; LENGTH];
        arr.copy_from_slice(bytes);
        Self(arr)
    }

    pub fn as_bytes(&self) -> &[u8; LENGTH] {
        &self.0
    }
}
