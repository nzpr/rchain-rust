//! Faithful Rust port of the RChain `rspace` module (the concurrent tuple space and its
//! content-addressed radix history).
//!
//! Mirrors `rspace/src/main/scala/coop/rchain/rspace/`. Encodes Laws 7–11 (join commutativity,
//! deterministic COMM, merge monoid, Merkle determinism, replay determinism). The crate is generic
//! over channel/pattern/datum/continuation types `C/P/A/K` (via `Serialize`) plus a `Match<P, A>`
//! typeclass.

pub mod concurrent;
pub mod hashing;
pub mod history;
pub mod internal;
pub mod merger;
pub mod serializers;
pub mod trace;
pub mod util;
