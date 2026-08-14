//! Faithful Rust port of the RChain `comm` module (peer-to-peer networking).
//!
//! Mirrors `comm/src/main/scala/coop/rchain/comm/`. This phase ports the pure core — peer identity,
//! the Kademlia `PeerTable` (XOR distance / bucket routing), the lock-free message buffers, and the
//! transport/rp configuration. The gRPC/TLS transport and the rp `Connect`/`HandleMessages` layers
//! land in the next phase.

pub mod discovery;
pub mod errors;
pub mod peer_node;
pub mod rp;
pub mod transport;
