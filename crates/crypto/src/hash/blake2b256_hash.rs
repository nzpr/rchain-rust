//! A 32-byte Blake2b256 hash wrapper.
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/hashing/Blake2b256Hash.scala`, hoisted into
//! `crypto` so that `models` can depend on it without depending on `rspace` (per AGENTS.md).
//! The scodec codecs are deferred.

use rchain_shared::base16;

/// The length of a `Blake2b256Hash` in bytes.
pub const LENGTH: usize = 32;

/// A 32-byte Blake2b256 hash.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Blake2b256Hash([u8; LENGTH]);

impl Blake2b256Hash {
    /// Hash `bytes` and wrap the result.
    pub fn create(bytes: &[u8]) -> Self {
        let digest: [u8; LENGTH] = super::blake2b256::hash(bytes).try_into().unwrap();
        Self(digest)
    }

    /// Hash the concatenation of `parts` and wrap the result.
    pub fn create_many(parts: &[&[u8]]) -> Self {
        let digest: [u8; LENGTH] = super::blake2b256::hash_many(parts).try_into().unwrap();
        Self(digest)
    }

    /// Wrap an existing 32-byte array without hashing.
    pub fn from_bytes(bytes: [u8; LENGTH]) -> Self {
        Self(bytes)
    }

    /// Wrap a byte slice, requiring it to be exactly 32 bytes.
    pub fn from_byte_array(bytes: &[u8]) -> Self {
        assert_eq!(
            bytes.len(),
            LENGTH,
            "Expected {} but got {}",
            LENGTH,
            bytes.len()
        );
        let mut arr = [0u8; LENGTH];
        arr.copy_from_slice(bytes);
        Self(arr)
    }

    /// Parse a hex string, ignoring non-hex characters (the Scala `fromHex` / `unsafeDecode`).
    pub fn from_hex(string: &str) -> Self {
        Self::from_byte_array(&base16::unsafe_decode(string))
    }

    /// Parse a hex string, failing on invalid input or an incorrect length.
    pub fn from_hex_either(string: &str) -> Result<Self, String> {
        match base16::decode(string) {
            Some(bytes) if bytes.len() == LENGTH => Ok(Self::from_byte_array(&bytes)),
            _ => Err(format!("Invalid hex string {string}")),
        }
    }

    /// The underlying 32 bytes.
    pub fn as_bytes(&self) -> &[u8; LENGTH] {
        &self.0
    }

    /// The underlying 32 bytes as a slice.
    pub fn to_byte_array(&self) -> [u8; LENGTH] {
        self.0
    }

    /// Hex-encode the hash.
    pub fn to_hex(&self) -> String {
        base16::encode(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_hashes_input() {
        let h = Blake2b256Hash::create(b"abc");
        assert_eq!(
            h.to_hex(),
            "bddd813c634239723171ef3fee98579b94964e3bb1cb3e427262c8c068d52319"
        );
    }

    #[test]
    fn create_many_matches_concatenation() {
        assert_eq!(
            Blake2b256Hash::create_many(&[b"ab", b"c"]),
            Blake2b256Hash::create(b"abc")
        );
    }

    #[test]
    fn from_hex_round_trips() {
        let h = Blake2b256Hash::create(b"abc");
        assert_eq!(Blake2b256Hash::from_hex(&h.to_hex()), h);
    }

    #[test]
    fn from_hex_either_rejects_bad_input() {
        assert!(Blake2b256Hash::from_hex_either("zz").is_err());
        assert!(Blake2b256Hash::from_hex_either("0e5751c026").is_err());
    }

    #[test]
    fn orders_lexicographically() {
        let a = Blake2b256Hash::from_bytes([0u8; 32]);
        let b = Blake2b256Hash::from_bytes([1u8; 32]);
        assert!(a < b);
    }
}
