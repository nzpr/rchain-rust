//! A state hash.
//!
//! Mirrors `models/src/main/scala/coop/rchain/models/block/StateHash.scala`.

use crate::errors::ModelsError;

/// The length of a `StateHash` in bytes.
pub const LENGTH: usize = 32;

/// A 32-byte state hash.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StateHash([u8; LENGTH]);

impl StateHash {
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

impl TryFrom<&[u8]> for StateHash {
    type Error = ModelsError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() != LENGTH {
            return Err(ModelsError::Length {
                got: bytes.len(),
                expected: LENGTH,
            });
        }
        Ok(Self::from_slice(bytes))
    }
}
