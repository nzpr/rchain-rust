//! Storage errors.
//!
//! Mirrors `block-storage/src/main/scala/coop/rchain/blockstorage/errors.scala`.

use std::fmt;

/// A storage error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    /// The topo-sort range parameter is invalid.
    TopoSortFragmentParameterError { start_block_number: i64, end_block_number: i64 },
    /// LZ4 block-message decompression failed.
    DecompressionError,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::TopoSortFragmentParameterError {
                start_block_number,
                end_block_number,
            } => write!(
                f,
                "topo-sort fragment parameter error: start {start_block_number}, end {end_block_number}"
            ),
            StorageError::DecompressionError => write!(f, "block message decompression failed"),
        }
    }
}

impl std::error::Error for StorageError {}
