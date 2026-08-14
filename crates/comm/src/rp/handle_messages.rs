//! Inbound protocol message dispatch.
//!
//! Mirrors `comm/src/main/scala/coop/rchain/comm/rp/HandleMessages.scala`. The fs2 routing-message
//! queue becomes a `tokio::sync::mpsc::Sender`.

use std::net::IpAddr;

use rchain_models::comm::protocol::{protocol, Packet, Protocol};

use crate::errors::CommError;
use crate::peer_node::PeerNode;
use crate::rp::connect::{add_conn, refresh_conn, remove_conn};
use crate::rp::protocol_helper;
use crate::rp::rp_conf::RPConf;
use crate::transport::communication_response::CommunicationResponse;
use crate::transport::transport_layer::TransportLayer;

/// A routing packet addressed from a peer (port of `RoutingMessage`).
#[derive(Clone, Debug)]
pub struct RoutingMessage {
    pub peer: PeerNode,
    pub packet: Packet,
}

/// Whether a host is a local/private address (port of `HandleMessages.isLocalAddress`). Only IPv4
/// literals are classified (matching the oracle tests); hostname resolution is not performed.
pub fn is_local_address(host: &str) -> bool {
    if let Ok(IpAddr::V4(ip)) = host.parse::<IpAddr>() {
        let o = ip.octets();
        ip.is_unspecified() // 0.0.0.0
            || ip.is_loopback() // 127/8
            || ip.is_multicast() // 224/4
            || (o[0] == 169 && o[1] == 254) // link-local 169.254/16
            || o[0] == 10 // 10/8
            || (o[0] == 172 && (16..=31).contains(&o[1])) // 172.16/12
            || (o[0] == 192 && o[1] == 168) // 192.168/16
    } else {
        false
    }
}

/// Whether `peer` is on the same subnetwork class (public vs local) as the local node (port of
/// `checkPeerOnSameNetwork`).
pub fn check_peer_on_same_network(conf: &RPConf, peer: &PeerNode) -> bool {
    is_local_address(&conf.local.endpoint.host) == is_local_address(&peer.endpoint.host)
}

/// Dispatch an inbound protocol message (port of `handle`).
pub async fn handle<T: TransportLayer + ?Sized>(
    proto: Protocol,
    conf: &RPConf,
    transport: &T,
    connections: &mut Vec<PeerNode>,
    routing_queue: &tokio::sync::mpsc::Sender<RoutingMessage>,
) -> CommunicationResponse {
    let sender = protocol_helper::sender(&proto);
    match proto.message {
        Some(protocol::Message::Heartbeat(_)) => {
            *connections = refresh_conn(connections, &sender);
            CommunicationResponse::handled_without_message()
        }
        Some(protocol::Message::ProtocolHandshake(_)) => {
            handle_protocol_handshake(transport, conf, connections, &sender).await
        }
        Some(protocol::Message::ProtocolHandshakeResponse(_)) => {
            *connections = add_conn(connections, &[sender]);
            CommunicationResponse::handled_without_message()
        }
        Some(protocol::Message::Disconnect(_)) => {
            *connections = remove_conn(connections, &[sender]);
            CommunicationResponse::handled_without_message()
        }
        Some(protocol::Message::Packet(packet)) => {
            let _ = routing_queue.try_send(RoutingMessage {
                peer: sender,
                packet,
            });
            CommunicationResponse::handled_without_message()
        }
        other => CommunicationResponse::not_handled(CommError::UnexpectedMessage(format!(
            "{other:?}"
        ))),
    }
}

/// Handle an inbound protocol handshake (port of `handleProtocolHandshake`): accept only peers on
/// the same subnetwork class, respond with a handshake response, and record the connection.
pub async fn handle_protocol_handshake<T: TransportLayer + ?Sized>(
    transport: &T,
    conf: &RPConf,
    connections: &mut Vec<PeerNode>,
    peer: &PeerNode,
) -> CommunicationResponse {
    if check_peer_on_same_network(conf, peer) {
        let response = protocol_helper::protocol_handshake_response(&conf.local, &conf.network_id);
        if transport.send(peer, response).await.is_ok() {
            *connections = add_conn(connections, &[peer.clone()]);
        }
    }
    CommunicationResponse::handled_without_message()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_private_addresses_as_local() {
        for host in ["0.0.0.0", "127.0.0.1", "10.0.0.1", "172.16.0.1", "172.31.255.255", "192.168.1.1"] {
            assert!(is_local_address(host), "{host} should be local");
        }
    }

    #[test]
    fn classifies_public_addresses_as_remote() {
        for host in ["8.8.8.8", "1.2.3.4", "172.32.0.1", "192.169.0.1"] {
            assert!(!is_local_address(host), "{host} should be public");
        }
    }
}
