//! Faithful Rust port of the RChain rholang interpreter core.
//!
//! Mirrors `rholang/src/main/scala/coop/rchain/rholang/interpreter/`. This crate ports the pure,
//! testable interpreter core (de Bruijn `Env`, capture-avoiding `Substitute`, the spatial matcher,
//! gas accounting, and the pure eval surface of `Reduce`) over a minimal in-memory `Tuplespace`.
//! The parser/normalizer and runtime glue are deferred.

pub mod accounting;
pub mod compiler;
pub mod contract_call;
pub mod dispatch;
pub mod env;
pub mod errors;
pub mod evaluate_result;
pub mod matcher;
pub mod normalizer;
pub mod pretty_printer;
pub mod proc_ast;
pub mod reduce;
pub mod registry;
pub mod runtime;
pub mod storage;
pub mod storage_printer;
pub mod substitute;
pub mod system_processes;
pub mod util;
