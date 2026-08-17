//! Faithful Rust port of the RChain `node` module (glue: configuration, runtime, API).
//!
//! Mirrors `node/src/main/scala/coop/rchain/node/`. The first slice is the `configuration`
//! sub-package (CLI + HOCON config).

pub mod api;
pub mod configuration;
pub mod dag;
pub mod diagnostics;
pub mod effects;
pub mod instances;
pub mod revvaultexport;
pub mod runtime;
pub mod state;
pub mod web;
