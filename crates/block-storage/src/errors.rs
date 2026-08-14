//! Storage errors.
//!
//! Mirrors `block-storage/src/main/scala/coop/rchain/blockstorage/errors.scala`.

/// A storage error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageError {
    /// The topo-sort range parameter is invalid.
    TopoSortFragmentParameterError { start_block_number: i64, end_block_number: i64 },
}
