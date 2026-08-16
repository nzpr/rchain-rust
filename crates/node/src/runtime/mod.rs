//! Node runtime (port of `coop.rchain.node.runtime`).

pub mod node_call_ctx;
pub mod repl_runtime;

pub use node_call_ctx::NodeCallCtx;
pub use repl_runtime::ReplRuntime;
