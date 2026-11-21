//! Operating-system pieces of the LAN link. unix.rs (macOS and Linux) and
//! windows.rs provide the same items, and the rest of the crate never branches
//! on the operating system:
//!   wake_pair, Wake, Waiter, wait, Wait   listener cancellation
//!   addresses                             this device's LAN IPv4 addresses
//!   overlay_addresses                     its Tailscale-style tunnel IPv4 addresses
//!   nearby                                whether a caller is on this device's own network
//!   create_directory, protect_directory,  owner-only credentials on disk
//!     private_file
#[cfg(unix)]
#[path = "unix.rs"]
mod host;
#[cfg(windows)]
#[path = "windows.rs"]
mod host;
pub(crate) use host::*;

/// 100.64.0.0/10: the shared address space that Tailscale (and similar overlay
/// networks) assigns to each device on its tunnel interface.
pub(crate) fn overlay(ip: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && (64..128).contains(&b)
}

/// Whether a caller at `peer`, taken on this device's address `here`, is on
/// the device's own network: a private or link-local address beside one of
/// its Ethernet or Wi-Fi addresses (`lan`). A tunnel's caller is not, whatever
/// range the tunnel uses, since the connection does not arrive on a LAN
/// address; nor is one a router forwards from outside, whose address is
/// public. A caller on this device itself is.
pub(crate) fn nearby(peer: std::net::IpAddr, here: std::net::IpAddr, lan: &[String]) -> bool {
    use std::net::IpAddr::V4;
    if peer.is_loopback() {
        return true;
    }
    let (V4(peer), V4(here)) = (peer, here) else {
        return false;
    };
    (peer.is_private() || peer.is_link_local())
        && !overlay(peer)
        && !overlay(here)
        && lan.contains(&here.to_string())
}

#[cfg(test)]
mod tests {
    use super::{nearby, overlay};
    use std::net::Ipv4Addr;

    #[test]
    fn nearby_is_a_private_caller_on_a_lan_address() {
        let lan = ["192.168.1.19".to_string()];
        let at = |peer: [u8; 4], here: [u8; 4]| {
            nearby(
                Ipv4Addr::from(peer).into(),
                Ipv4Addr::from(here).into(),
                &lan,
            )
        };
        assert!(at([192, 168, 1, 40], [192, 168, 1, 19]));
        // A phone's hotspot hands out link-local-like private addresses too.
        assert!(nearby(
            Ipv4Addr::new(172, 20, 10, 1).into(),
            Ipv4Addr::new(172, 20, 10, 4).into(),
            &["172.20.10.4".to_string()],
        ));
        // Tailscale: the caller and this device's own address are in its range.
        assert!(!at([100, 66, 108, 61], [100, 69, 88, 16]));
        // Another tunnel with private addresses does not arrive on a LAN address.
        assert!(!at([10, 147, 20, 5], [10, 147, 20, 1]));
        // A router forwarding a port from outside keeps the caller's public address.
        assert!(!at([14, 153, 86, 232], [192, 168, 1, 19]));
        assert!(nearby(
            Ipv4Addr::LOCALHOST.into(),
            Ipv4Addr::LOCALHOST.into(),
            &[]
        ));
        assert!(!nearby(
            "fd7a:115c:a1e0::1".parse().unwrap(),
            Ipv4Addr::new(192, 168, 1, 19).into(),
            &lan,
        ));
    }

    #[test]
    fn overlay_is_the_shared_address_space_only() {
        assert!(overlay(Ipv4Addr::new(100, 64, 0, 1)));
        assert!(overlay(Ipv4Addr::new(100, 69, 88, 16)));
        assert!(overlay(Ipv4Addr::new(100, 127, 255, 254)));
        assert!(!overlay(Ipv4Addr::new(100, 63, 255, 255)));
        assert!(!overlay(Ipv4Addr::new(100, 128, 0, 1)));
        assert!(!overlay(Ipv4Addr::new(192, 168, 1, 19)));
        assert!(!overlay(Ipv4Addr::new(198, 18, 0, 1)));
    }
}
