//! Faithful Rust port of the RChain rholang interpreter core.
//!
//! Mirrors `rholang/src/main/scala/coop/rchain/rholang/interpreter/`. This crate ports the pure,
//! testable interpreter core (de Bruijn `Env`, capture-avoiding `Substitute`, the spatial matcher,
//! gas accounting, and the pure eval surface of `Reduce`) over a minimal in-memory `Tuplespace`.
//! The parser/normalizer and runtime glue are deferred.

pub mod env;
pub mod errors;
