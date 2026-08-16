//! Faithful Rust port of the RChain `comm` module (peer-to-peer networking).
//!
//! Mirrors `comm/src/main/scala/coop/rchain/comm/`. This crate ports the full networking stack:
//! peer identity, the Kademlia `PeerTable` + gRPC discovery RPC, the gRPC/TLS `TransportLayer`
//! (client/server/receiver with mutual TLS and node-id trust), the message buffers/`PacketOps`/
//! `StreamHandler`, and the rp `Connect`/`HandleMessages` layers. UPnP/WhoAmI external-IP discovery
//! is deferred (peripheral, off the transport critical path); only the UPnP IPv4 private-address
//! classifier is ported.

pub mod discovery;
pub mod errors;
pub mod peer_node;
pub mod rp;
pub mod transport;
pub mod upnp;
pub mod who_am_i;
