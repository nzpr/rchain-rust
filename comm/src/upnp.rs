//! UPnP helpers (port of the pure part of `comm/UPnP.scala`).
//!
//! The weupnp-based port forwarding (`discover`, `assurePortForwarding`, …) is deferred; only the
//! IPv4 private-address classification is ported.

use std::net::Ipv4Addr;

/// Classify an IP address as private (port of `UPnP.isPrivateIpAddress`).
///
/// Returns `None` when the input is not a valid IPv4 address; otherwise `Some(true)` for
/// private/loopback/link-local/unspecified addresses and `Some(false)` for public ones.
pub fn is_private_ip_address(ip: &str) -> Option<bool> {
    let addr: Ipv4Addr = ip.parse().ok()?;
    let private = match addr.octets() {
        [10, _, _, _] => true,
        [127, _, _, _] => true,
        [192, 168, _, _] => true,
        [172, b, _, _] if (16..=31).contains(&b) => true,
        [169, 254, _, _] => true,
        [0, 0, 0, 0] => true,
        _ => false,
    };
    Some(private)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_private_ranges() {
        assert_eq!(is_private_ip_address("10.0.0.1"), Some(true));
        assert_eq!(is_private_ip_address("127.0.0.1"), Some(true));
        assert_eq!(is_private_ip_address("192.168.1.1"), Some(true));
        assert_eq!(is_private_ip_address("172.16.0.1"), Some(true));
        assert_eq!(is_private_ip_address("172.31.255.255"), Some(true));
        assert_eq!(is_private_ip_address("169.254.0.1"), Some(true));
        assert_eq!(is_private_ip_address("0.0.0.0"), Some(true));
    }

    #[test]
    fn classifies_public_ranges() {
        assert_eq!(is_private_ip_address("8.8.8.8"), Some(false));
        assert_eq!(is_private_ip_address("172.15.0.1"), Some(false));
        assert_eq!(is_private_ip_address("172.32.0.1"), Some(false));
        assert_eq!(is_private_ip_address("192.169.1.1"), Some(false));
    }

    #[test]
    fn returns_none_for_non_ipv4() {
        assert_eq!(is_private_ip_address("not-an-ip"), None);
        assert_eq!(is_private_ip_address("256.0.0.1"), None);
        assert_eq!(is_private_ip_address("::1"), None);
    }
}
