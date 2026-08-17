//! Node DAG glue (port of `coop.rchain.node.dag`).

pub mod implementation;
pub mod rnode_state_setup;

pub use implementation::{BlockStatus, NetworkBlockRequester, RNodeDagManager};
pub use rnode_state_setup::setup_rnode_state;
