//! Tuple-space state export/import (port of `rspace/.../state/`).
//!
//! Ported here are the pure data types. The algorithmic pieces — `RSpaceExporter.traverseHistory`
//! (which depends on `RadixTree.sequentialExport`), `RSpaceImporter.validateStateItems`, the
//! store-backed instances (`RSpaceExporterStore`/`RSpaceImporterStore`/`RSpaceStateManagerImpl`),
//! and the disk exporter (`RSpaceExporterDisk`) — are deferred pending a port of the radix-tree
//! export traversal and the store wiring. The foundational `TrieExporter`/`TrieNode`/`TrieImporter`/
//! `StateManager` abstractions live in `rchain_shared::state`.

use std::fmt;

use rchain_crypto::hash::blake2b256_hash::Blake2b256Hash;
use rchain_shared::state::{TrieExporter, TrieImporter};

/// Export skip/take counters (port of `RSpaceExporter.Counter`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counter {
    pub skip: usize,
    pub take: usize,
}

/// Raised when the history is empty (port of `RSpaceExporter.EmptyHistoryException`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmptyHistoryException;

impl fmt::Display for EmptyHistoryException {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EmptyHistoryException")
    }
}

impl std::error::Error for EmptyHistoryException {}

/// A state-validation failure (port of `RSpaceImporter.StateValidationError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateValidationError(pub String);

impl fmt::Display for StateValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StateValidationError: {}", self.0)
    }
}

impl std::error::Error for StateValidationError {}

/// A chunk of exported items plus the path of the last item (port of
/// `RSpaceExporterItems.StoreItems`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreItems<KeyHash, Value> {
    pub items: Vec<(KeyHash, Value)>,
    pub last_path: Vec<(KeyHash, Option<u8>)>,
}

/// Format a `(hash, index)` path for pretty printing (port of `RSpaceExporter.pathPretty`).
pub fn path_pretty(path: &(Blake2b256Hash, Option<u8>)) -> String {
    let (hash, idx) = path;
    let idx_str = match idx {
        None => "--".to_string(),
        Some(i) => format!("{:02x}", i & 0xff),
    };
    let hash_hex: String = hash
        .as_bytes()
        .iter()
        .take(4)
        .map(|b| format!("{:02x}", b))
        .collect();
    format!("{}:{}", idx_str, hash_hex)
}

/// The rspace exporter (port of `RSpaceExporter`).
pub trait RSpaceExporter: TrieExporter<Blake2b256Hash> {
    fn get_root(&self) -> Blake2b256Hash;
}

/// The rspace importer (port of `RSpaceImporter`).
pub trait RSpaceImporter: TrieImporter<Blake2b256Hash> {
    fn get_history_item(&self, hash: Blake2b256Hash) -> Option<Vec<u8>>;
}
