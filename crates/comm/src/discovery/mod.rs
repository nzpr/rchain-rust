//! Peer discovery (Kademlia).
//!
//! Mirrors `comm/src/main/scala/coop/rchain/comm/discovery/`.

use rchain_models::comm::discovery::Node;

use crate::peer_node::{Endpoint, NodeIdentifier, PeerNode};

pub mod grpc_kademlia_rpc;
pub mod grpc_kademlia_rpc_server;
pub mod kademlia_handle_rpc;
pub mod kademlia_node_discovery;
pub mod kademlia_rpc;
pub mod kademlia_store;
pub mod node_discovery;
pub mod peer_table;

pub use kademlia_rpc::KademliaRpc;
pub use kademlia_store::KademliaStore;
pub use node_discovery::NodeDiscovery;

/// Convert a Kademlia proto `Node` to a `PeerNode` (port of `discovery.toPeerNode`).
pub fn to_peer_node(node: &Node) -> PeerNode {
    PeerNode {
        id: NodeIdentifier::new(node.id.clone()),
        endpoint: Endpoint {
            host: String::from_utf8_lossy(&node.host).to_string(),
            tcp_port: node.tcp_port as i32,
            udp_port: node.udp_port as i32,
        },
    }
}

/// Convert a `PeerNode` to a Kademlia proto `Node` (port of `discovery.toNode`).
pub fn to_node(peer: &PeerNode) -> Node {
    Node {
        id: peer.key().to_vec(),
        host: peer.endpoint.host.as_bytes().to_vec(),
        tcp_port: peer.endpoint.tcp_port as u32,
        udp_port: peer.endpoint.udp_port as u32,
    }
}
