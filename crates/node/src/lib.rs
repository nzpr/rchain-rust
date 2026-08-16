//! Faithful Rust port of the RChain `node` module (glue: configuration, runtime, API).
//!
//! Mirrors `node/src/main/scala/coop/rchain/node/`. The first slice is the `configuration`
//! sub-package (CLI + HOCON config).

pub mod configuration;
pub mod diagnostics;
