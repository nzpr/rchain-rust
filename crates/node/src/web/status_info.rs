//! Node status DTO (port of `web/StatusInfo.scala`).
//!
//! The effectful `status` builder (reads comm `ConnectionsCell`/`NodeDiscovery`/`RPConfAsk`) and
//! the http4s `service` are deferred.

/// Lightweight node status (port of `StatusInfo.Status`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub address: String,
    pub version: String,
    pub peers: i32,
    pub nodes: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_fields_are_accessible() {
        let status = Status {
            address: "addr".to_string(),
            version: "v1".to_string(),
            peers: 3,
            nodes: 5,
        };
        assert_eq!(status.address, "addr");
        assert_eq!(status.version, "v1");
        assert_eq!(status.peers, 3);
        assert_eq!(status.nodes, 5);
    }
}
