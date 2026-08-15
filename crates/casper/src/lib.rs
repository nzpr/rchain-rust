//! Faithful Rust port of the RChain `casper` module (CBC-Casper consensus + DAG).
//!
//! Mirrors `casper/src/main/scala/coop/rchain/casper/`. Encodes Laws 14–18 (finality, fringe
//! monotonicity, block numbering/content-addressing, merge determinism, height-map contiguity).

pub mod block_status;
pub mod dag;
pub mod merging;
pub mod proto_util;
pub mod validate;
