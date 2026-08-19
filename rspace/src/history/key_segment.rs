//! A radix-tree key segment (0–127 bytes).
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/history/KeySegment.scala`.

use rchain_shared::base16;

/// A path segment of a radix key (port of `KeySegment`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct KeySegment {
    value: Vec<u8>,
}

/// Validated construction: a key segment is at most 127 bytes (the radix-tree wire invariant).
impl TryFrom<Vec<u8>> for KeySegment {
    type Error = String;
    fn try_from(value: Vec<u8>) -> Result<Self, String> {
        if value.len() <= 127 {
            Ok(KeySegment { value })
        } else {
            Err(format!("key segment length {} exceeds 127", value.len()))
        }
    }
}

impl KeySegment {
    /// Total constructor on already-valid input: the caller guarantees `value.len() <= 127` (the
    /// radix-tree wire invariant). Use [`TryFrom`] at boundaries where the length is untrusted.
    pub fn new(value: Vec<u8>) -> Self {
        KeySegment { value }
    }

    pub fn empty() -> Self {
        KeySegment { value: Vec::new() }
    }

    pub fn len(&self) -> usize {
        self.value.len()
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn head(&self) -> u8 {
        self.value[0]
    }

    pub fn tail(&self) -> KeySegment {
        KeySegment::new(self.value[1..].to_vec())
    }

    pub fn head_option(&self) -> Option<u8> {
        self.value.first().copied()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.value
    }

    /// Concatenate two segments (port of `++`).
    pub fn concat(&self, other: &KeySegment) -> KeySegment {
        let mut value = self.value.clone();
        value.extend_from_slice(&other.value);
        KeySegment::new(value)
    }

    /// Append a byte (port of `:+`).
    pub fn append(&self, byte: u8) -> KeySegment {
        let mut value = self.value.clone();
        value.push(byte);
        KeySegment::new(value)
    }

    pub fn to_hex(&self) -> String {
        base16::encode(&self.value)
    }

    /// The common prefix of `a` and `b`, plus their remainders (port of `commonPrefix`).
    pub fn common_prefix(a: &KeySegment, b: &KeySegment) -> (KeySegment, KeySegment, KeySegment) {
        let mut i = 0;
        while i < a.value.len() && i < b.value.len() && a.value[i] == b.value[i] {
            i += 1;
        }
        (
            KeySegment::new(a.value[..i].to_vec()),
            KeySegment::new(a.value[i..].to_vec()),
            KeySegment::new(b.value[i..].to_vec()),
        )
    }
}
