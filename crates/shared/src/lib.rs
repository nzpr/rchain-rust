//! Faithful Rust port of the RChain `shared` module.
//!
//! Mirrors `shared/src/main/scala/coop/rchain/`. This crate is *near-leaf* (the Scala version
//! depends only on `sdk`) and is imported by everything else. Ported here are the
//! framework-agnostic, load-bearing pieces; the LMDB FFI, scodec codecs, cats-contrib monad
//! shims, `StreamT`, metrics, and fs2/grpc/monix interop are deferred until the async runtime
//! (tokio) is wired in.

pub mod base16;
pub mod dag;
pub mod key_value_cache;
pub mod language;
pub mod long_ops;
pub mod matcher;
pub mod path_ops;
pub mod seq_ops;
pub mod serialize;
pub mod state;
pub mod stopwatch;
pub mod store;
pub mod string_ops;
pub mod terminal_mode;
pub mod throwable_ops;
pub mod time;

#[cfg(feature = "tokio")]
pub mod compression;
#[cfg(feature = "tokio")]
pub mod log;
#[cfg(feature = "tokio")]
pub mod metrics;
#[cfg(feature = "tokio")]
pub mod monixable;
#[cfg(feature = "tokio")]
pub mod store_manager;
#[cfg(feature = "tokio")]
pub mod typed_store;
