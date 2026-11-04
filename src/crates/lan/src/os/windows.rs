//! Windows IPv4 interface discovery and protected, inheritable user ACLs.
//! The nonblocking listener polls every 25 ms, so it needs no wake-up.
use crate::Result;
use std::{
    collections::BTreeMap,
    net::{Ipv4Addr, TcpListener},
};
use windows_sys::Win32::{
    Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS},
    NetworkManagement::{
        IpHelper::{
            GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
            GAA_FLAG_SKIP_MULTICAST, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211,
            IF_TYPE_SOFTWARE_LOOPBACK, IP_ADAPTER_ADDRESSES_LH,
        },
        Ndis::IfOperStatusUp,
    },
    Networking::WinSock::{AF_INET, SOCKADDR_IN},
};
#[path = "windows/acl.rs"]
mod acl;
pub(crate) use acl::{create_directory, private_file, protect_directory};

pub(crate) enum Wait {
    Accept,
    #[allow(dead_code)]
    Stop,
    #[allow(dead_code)]
    Retry,
}

pub(crate) struct Waiter;
pub(crate) struct Wake;

pub(crate) fn wake_pair() -> Result<(Waiter, Wake)> {
    Ok((Waiter, Wake))
}

impl Wake {
    pub(crate) fn wake(self) {}
}

/// The nonblocking accept itself polls; the stop flag ends the loop.
pub(crate) fn wait(_listener: &TcpListener, _waiter: &Waiter) -> Wait {
    Wait::Accept
}

pub(crate) fn addresses() -> Vec<String> {
    adapters()
        .into_iter()
        .map(|(_, ip)| ip.to_string())
        .collect()
}

/// Tunnel adapters' addresses in the overlay range, such as Tailscale's.
pub(crate) fn overlay_addresses() -> Vec<String> {
    overlay_only(adapters())
}

fn overlay_only(rows: Vec<((bool, u32), Ipv4Addr)>) -> Vec<String> {
    rows.into_iter()
        .filter(|((tunnel, _), ip)| *tunnel && super::overlay(*ip))
        .map(|(_, ip)| ip.to_string())
        .collect()
}

/// IPv4 addresses ranked Ethernet/Wi-Fi first, then by interface metric. The
/// rank's flag is true for any other adapter type.
fn adapters() -> Vec<((bool, u32), Ipv4Addr)> {
    // Microsoft recommends a 15 KB initial buffer. A changing adapter list can
    // require another call; u64 storage preserves the native structs' alignment.
    let mut size = 15_000u32;
    for _ in 0..3 {
        let mut buffer = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
        let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        let status = unsafe {
            GetAdaptersAddresses(
                u32::from(AF_INET),
                GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
                std::ptr::null(),
                first,
                &mut size,
            )
        };
        if status == ERROR_SUCCESS {
            return unsafe { collect_addresses(first) };
        }
        if status != ERROR_BUFFER_OVERFLOW {
            break;
        }
    }
    Vec::new()
}

unsafe fn collect_addresses(
    mut at: *const IP_ADAPTER_ADDRESSES_LH,
) -> Vec<((bool, u32), Ipv4Addr)> {
    let mut rows = BTreeMap::new();
    while let Some(adapter) = unsafe { at.as_ref() } {
        if adapter.OperStatus == IfOperStatusUp && adapter.IfType != IF_TYPE_SOFTWARE_LOOPBACK {
            let rank = (
                !matches!(adapter.IfType, IF_TYPE_ETHERNET_CSMACD | IF_TYPE_IEEE80211),
                adapter.Ipv4Metric,
            );
            let mut unicast = adapter.FirstUnicastAddress;
            while let Some(address) = unsafe { unicast.as_ref() } {
                let socket = address.Address.lpSockaddr;
                if !socket.is_null()
                    && address.Address.iSockaddrLength >= size_of::<SOCKADDR_IN>() as i32
                    && unsafe { (*socket).sa_family } == AF_INET
                {
                    let address = unsafe { &*socket.cast::<SOCKADDR_IN>() };
                    let ip = Ipv4Addr::from(unsafe { address.sin_addr.S_un.S_addr }.to_ne_bytes());
                    if !ip.is_loopback()
                        && !ip.is_unspecified()
                        && !ip.is_multicast()
                        && !ip.is_broadcast()
                    {
                        rows.entry(ip)
                            .and_modify(|previous: &mut (bool, u32)| {
                                *previous = (*previous).min(rank)
                            })
                            .or_insert(rank);
                    }
                }
                unicast = address.Next;
            }
        }
        at = adapter.Next;
    }
    let mut rows: Vec<_> = rows.into_iter().map(|(ip, rank)| (rank, ip)).collect();
    rows.sort_unstable();
    rows
}

#[cfg(test)]
#[path = "windows/tests.rs"]
mod tests;
