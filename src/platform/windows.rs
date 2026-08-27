//! Windows implementation of PlatformProbe — calls the Win32 API directly
//! (IP Helper + WLAN + ICMP) rather than shelling out to PowerShell / netsh /
//! ping.exe. See docs/decisions/2026-08-28-windows-probes-via-win32-api.md.
//!
//! The API surface is language-invariant and structured (auth algorithm is an
//! enum, not a localized label), so there is no captured command output to
//! parse or to sanitize. Coverage that the pointer-walking is correct comes
//! from the contract tests run on a real Windows machine; the unit tests here
//! cover the pure enum/byte mappings.

#![allow(non_upper_case_globals)]

use super::{is_vpn_iface, AddrInfo, PlatformProbe, RouteInfo, WifiInfo};
use crate::network::{lookup_mac_vendor, PingSummary};
use crate::types::{ArpNeighbor, DnsResolverInfo, DnsSource, InterfaceKind, WifiEncryption};
use std::net::Ipv4Addr;

use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, HANDLE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetAdaptersAddresses, GetBestRoute2, GetIpNetTable2, IcmpCloseHandle,
    IcmpCreateFile, IcmpSendEcho2, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST,
    ICMP_ECHO_REPLY, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211,
    IF_TYPE_PPP, IF_TYPE_TUNNEL, IP_ADAPTER_ADDRESSES_LH, IP_SUCCESS, MIB_IPFORWARD_ROW2,
    MIB_IPNET_TABLE2,
};
use windows_sys::Win32::NetworkManagement::WiFi::{
    wlan_intf_opcode_channel_number, wlan_intf_opcode_current_connection, WlanCloseHandle,
    WlanEnumInterfaces, WlanFreeMemory, WlanOpenHandle, WlanQueryInterface, DOT11_AUTH_ALGORITHM,
    WLAN_CONNECTION_ATTRIBUTES, WLAN_INTERFACE_INFO_LIST,
};
use windows_sys::Win32::Networking::WinSock::{
    AF_INET, IN_ADDR, IN_ADDR_0, SOCKADDR, SOCKADDR_IN, SOCKADDR_INET,
};

/// `IcmpCreateFile` failure sentinel.
const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;

// ---------------------------------------------------------------------------
// Small pure helpers (unit-tested)
// ---------------------------------------------------------------------------

/// First octet's group bit (multicast) or an all-ones broadcast address.
fn is_group_or_broadcast(mac: &[u8]) -> bool {
    match mac.first() {
        None => true,
        Some(first) => first & 1 != 0 || mac.iter().all(|b| *b == 0xff),
    }
}

/// `AA-BB-CC-DD-EE-FF`, uppercase, matching what `lookup_mac_vendor` and the
/// macOS/Linux probes produce.
fn format_mac(mac: &[u8]) -> Option<String> {
    if mac.is_empty() {
        return None;
    }
    Some(mac.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join("-"))
}

/// `dot11AuthAlgorithm` (from `WLAN_SECURITY_ATTRIBUTES`) → our bucket.
/// Values per the DOT11_AUTH_ALGO_* constants.
fn classify_dot11_auth(algo: DOT11_AUTH_ALGORITHM) -> WifiEncryption {
    match algo {
        1 => WifiEncryption::Open,           // 80211_OPEN
        2..=5 => WifiEncryption::Wpa,        // SHARED_KEY (WEP-era) / WPA / WPA_PSK / WPA_NONE
        6 => WifiEncryption::Wpa2Enterprise, // RSNA (802.1X)
        7 => WifiEncryption::Wpa2,           // RSNA_PSK
        8..=11 => WifiEncryption::Wpa3,      // WPA3 / SAE / OWE / WPA3_ENT
        _ => WifiEncryption::Unknown,
    }
}

/// `IfType` (from `IP_ADAPTER_ADDRESSES`) → our interface kind.
fn classify_if_type(if_type: u32, iface: &str) -> InterfaceKind {
    if is_vpn_iface(iface) {
        return InterfaceKind::Vpn;
    }
    match if_type {
        IF_TYPE_IEEE80211 => InterfaceKind::WiFi,
        IF_TYPE_ETHERNET_CSMACD => InterfaceKind::Ethernet,
        IF_TYPE_TUNNEL | IF_TYPE_PPP => InterfaceKind::Vpn,
        _ => InterfaceKind::Other,
    }
}

/// `NL_NEIGHBOR_STATE` (i32) → the short label the report shows.
fn neighbor_state_str(state: i32) -> &'static str {
    match state {
        0 => "Unreachable",
        1 => "Incomplete",
        2 => "Probe",
        3 => "Delay",
        4 => "Stale",
        5 => "Reachable",
        6 => "Permanent",
        _ => "Unknown",
    }
}

// ---------------------------------------------------------------------------
// FFI value helpers
// ---------------------------------------------------------------------------

/// Reads a NUL-terminated wide (UTF-16) string.
unsafe fn wide_ptr_to_string(mut p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut units = Vec::new();
    while unsafe { *p } != 0 {
        units.push(unsafe { *p });
        p = unsafe { p.add(1) };
    }
    String::from_utf16_lossy(&units)
}

/// IPv4 out of a raw `SOCKADDR*` (from a `SOCKET_ADDRESS`), if it is AF_INET.
unsafe fn sockaddr_ptr_ipv4(sa: *const SOCKADDR) -> Option<Ipv4Addr> {
    if sa.is_null() || unsafe { (*sa).sa_family } != AF_INET {
        return None;
    }
    let v4 = sa as *const SOCKADDR_IN;
    let bytes = unsafe { (*v4).sin_addr.S_un.S_addr }.to_ne_bytes();
    Some(Ipv4Addr::from(bytes))
}

/// IPv4 out of a `SOCKADDR_INET` union, if it is AF_INET.
unsafe fn sockaddr_inet_ipv4(sa: &SOCKADDR_INET) -> Option<Ipv4Addr> {
    if unsafe { sa.si_family } != AF_INET {
        return None;
    }
    let bytes = unsafe { sa.Ipv4.sin_addr.S_un.S_addr }.to_ne_bytes();
    Some(Ipv4Addr::from(bytes))
}

fn ipv4_sockaddr_inet(ip: Ipv4Addr) -> SOCKADDR_INET {
    let mut sa: SOCKADDR_INET = unsafe { std::mem::zeroed() };
    sa.Ipv4 = SOCKADDR_IN {
        sin_family: AF_INET,
        sin_port: 0,
        sin_addr: IN_ADDR { S_un: IN_ADDR_0 { S_addr: u32::from_ne_bytes(ip.octets()) } },
        sin_zero: [0; 8],
    };
    sa
}

// ---------------------------------------------------------------------------
// GetAdaptersAddresses snapshot
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct AdapterInfo {
    luid: u64,
    friendly_name: String,
    if_type: u32,
    ipv4: Vec<(Ipv4Addr, u8)>,
    dns_servers: Vec<Ipv4Addr>,
}

/// One `GetAdaptersAddresses` call → owned Rust structs (no live pointers).
fn list_adapters() -> Vec<AdapterInfo> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
    let mut size: u32 = 16 * 1024;
    let mut buf: Vec<u64> = Vec::new();

    // Grow-and-retry: the first call sizes the buffer.
    for _ in 0..4 {
        buf = vec![0u64; (size as usize).div_ceil(8)];
        let ret = unsafe {
            GetAdaptersAddresses(
                AF_INET as u32,
                flags,
                std::ptr::null(),
                buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                &mut size,
            )
        };
        if ret == NO_ERROR {
            break;
        }
        if ret != ERROR_BUFFER_OVERFLOW {
            return Vec::new();
        }
    }

    let mut adapters = Vec::new();
    let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !cur.is_null() {
        let a = unsafe { &*cur };

        let mut ipv4 = Vec::new();
        let mut uni = a.FirstUnicastAddress;
        while !uni.is_null() {
            let u = unsafe { &*uni };
            if let Some(ip) = unsafe { sockaddr_ptr_ipv4(u.Address.lpSockaddr) } {
                ipv4.push((ip, u.OnLinkPrefixLength));
            }
            uni = u.Next;
        }

        let mut dns_servers = Vec::new();
        let mut dns = a.FirstDnsServerAddress;
        while !dns.is_null() {
            let d = unsafe { &*dns };
            if let Some(ip) = unsafe { sockaddr_ptr_ipv4(d.Address.lpSockaddr) } {
                dns_servers.push(ip);
            }
            dns = d.Next;
        }

        adapters.push(AdapterInfo {
            luid: unsafe { a.Luid.Value },
            friendly_name: unsafe { wide_ptr_to_string(a.FriendlyName) },
            if_type: a.IfType,
            ipv4,
            dns_servers,
        });

        cur = a.Next;
    }
    adapters
}

fn adapter_by_name<'a>(adapters: &'a [AdapterInfo], name: &str) -> Option<&'a AdapterInfo> {
    adapters.iter().find(|a| a.friendly_name == name)
}

// ---------------------------------------------------------------------------
// ICMP ping (used by checks::reliability on Windows)
// ---------------------------------------------------------------------------

fn icmp_ping_blocking(ip: Ipv4Addr, count: u32) -> PingSummary {
    let all_fail = PingSummary { transmitted: count, received: 0, rtts: Vec::new() };

    let handle = unsafe { IcmpCreateFile() };
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return all_fail;
    }

    let dest: u32 = u32::from_ne_bytes(ip.octets());
    let request = [0x61u8; 32]; // arbitrary payload
    // ICMP_ECHO_REPLY + payload + 8 bytes for an optional ICMP error record.
    let mut reply = [0u8; std::mem::size_of::<ICMP_ECHO_REPLY>() + 32 + 8];

    let mut rtts = Vec::new();
    for i in 0..count {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let n = unsafe {
            IcmpSendEcho2(
                handle,
                std::ptr::null_mut(),
                None,
                std::ptr::null(),
                dest,
                request.as_ptr() as *const _,
                request.len() as u16,
                std::ptr::null(),
                reply.as_mut_ptr() as *mut _,
                reply.len() as u32,
                2000,
            )
        };
        if n >= 1 {
            let r = unsafe { &*(reply.as_ptr() as *const ICMP_ECHO_REPLY) };
            if r.Status == IP_SUCCESS {
                rtts.push(r.RoundTripTime as f64);
            }
        }
    }

    unsafe { IcmpCloseHandle(handle) };
    PingSummary { transmitted: count, received: rtts.len() as u32, rtts }
}

/// Ping `host` `count` times over ICMP. Runs on the blocking pool — each echo
/// is a synchronous `IcmpSendEcho2`.
pub async fn icmp_ping(host: &str, count: u32) -> PingSummary {
    let Ok(ip) = host.parse::<Ipv4Addr>() else {
        return PingSummary { transmitted: count, received: 0, rtts: Vec::new() };
    };
    tokio::task::spawn_blocking(move || icmp_ping_blocking(ip, count))
        .await
        .unwrap_or(PingSummary { transmitted: count, received: 0, rtts: Vec::new() })
}

// ---------------------------------------------------------------------------
// WLAN
// ---------------------------------------------------------------------------

fn wlan_info() -> Option<WifiInfo> {
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut negotiated: u32 = 0;
    if unsafe { WlanOpenHandle(2, std::ptr::null(), &mut negotiated, &mut handle) } != NO_ERROR {
        return None; // wlansvc not running, or no WLAN stack
    }

    let result = (|| {
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if unsafe { WlanEnumInterfaces(handle, std::ptr::null(), &mut list) } != NO_ERROR
            || list.is_null()
        {
            return None;
        }
        let l = unsafe { &*list };
        let ifaces =
            unsafe { std::slice::from_raw_parts(l.InterfaceInfo.as_ptr(), l.dwNumberOfItems as usize) };
        // Prefer a connected interface; fall back to the first.
        let chosen = ifaces.iter().find(|i| i.isState == 1).or_else(|| ifaces.first());
        let guid = match chosen {
            Some(i) => i.InterfaceGuid,
            None => {
                unsafe { WlanFreeMemory(list as *const _) };
                return None;
            }
        };
        unsafe { WlanFreeMemory(list as *const _) };

        // current_connection → SSID, auth algorithm, signal quality
        let mut size: u32 = 0;
        let mut data: *mut std::ffi::c_void = std::ptr::null_mut();
        if unsafe {
            WlanQueryInterface(
                handle,
                &guid,
                wlan_intf_opcode_current_connection,
                std::ptr::null(),
                &mut size,
                &mut data,
                std::ptr::null_mut(),
            )
        } != NO_ERROR
            || data.is_null()
        {
            return None;
        }
        let conn = unsafe { &*(data as *const WLAN_CONNECTION_ATTRIBUTES) };
        let assoc = &conn.wlanAssociationAttributes;
        let ssid_bytes = &assoc.dot11Ssid.ucSSID[..(assoc.dot11Ssid.uSSIDLength as usize).min(32)];
        let ssid = String::from_utf8_lossy(ssid_bytes).into_owned();
        let signal_percent = Some(assoc.wlanSignalQuality.min(100));
        let encryption = classify_dot11_auth(conn.wlanSecurityAttributes.dot11AuthAlgorithm);
        unsafe { WlanFreeMemory(data as *const _) };

        // channel_number (a separate query; may be absent on some drivers)
        let mut csize: u32 = 0;
        let mut cdata: *mut std::ffi::c_void = std::ptr::null_mut();
        let channel = if unsafe {
            WlanQueryInterface(
                handle,
                &guid,
                wlan_intf_opcode_channel_number,
                std::ptr::null(),
                &mut csize,
                &mut cdata,
                std::ptr::null_mut(),
            )
        } == NO_ERROR
            && !cdata.is_null()
        {
            let ch = unsafe { *(cdata as *const u32) };
            unsafe { WlanFreeMemory(cdata as *const _) };
            (ch != 0).then_some(ch)
        } else {
            None
        };

        if ssid.is_empty() {
            return None;
        }
        Some(WifiInfo { ssid, encryption, channel, frequency_mhz: None, signal_percent })
    })();

    unsafe { WlanCloseHandle(handle, std::ptr::null()) };
    result
}

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

pub struct WindowsProbe;

impl PlatformProbe for WindowsProbe {
    async fn default_route(&self) -> Option<RouteInfo> {
        // GetBestRoute2 to a public address = "what's my default route".
        let dest = ipv4_sockaddr_inet(Ipv4Addr::new(8, 8, 8, 8));
        let mut row: MIB_IPFORWARD_ROW2 = unsafe { std::mem::zeroed() };
        let mut best_src: SOCKADDR_INET = unsafe { std::mem::zeroed() };
        let err = unsafe {
            GetBestRoute2(
                std::ptr::null(),
                0,
                std::ptr::null(),
                &dest,
                0,
                &mut row,
                &mut best_src,
            )
        };
        if err != NO_ERROR {
            return None;
        }
        let gateway = unsafe { sockaddr_inet_ipv4(&row.NextHop) }?;
        if gateway.is_unspecified() {
            return None; // on-link route, no gateway
        }
        let luid = unsafe { row.InterfaceLuid.Value };
        let adapters = list_adapters();
        let device = adapters
            .iter()
            .find(|a| a.luid == luid)
            .map(|a| a.friendly_name.clone())?;
        Some(RouteInfo { gateway: gateway.to_string(), device })
    }

    async fn interface_addr(&self, iface: &str) -> Option<AddrInfo> {
        let adapters = list_adapters();
        let a = adapter_by_name(&adapters, iface)?;
        let (ip, prefix) = a.ipv4.first()?;
        Some(AddrInfo { ip: ip.to_string(), prefix: *prefix as u32 })
    }

    async fn arp_neighbors(&self, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
        let adapters = list_adapters();
        let Some(target_luid) = adapter_by_name(&adapters, iface).map(|a| a.luid) else {
            return Vec::new();
        };

        let mut table: *mut MIB_IPNET_TABLE2 = std::ptr::null_mut();
        if unsafe { GetIpNetTable2(AF_INET, &mut table) } != NO_ERROR || table.is_null() {
            return Vec::new();
        }
        let t = unsafe { &*table };
        let rows = unsafe {
            std::slice::from_raw_parts(t.Table.as_ptr(), t.NumEntries as usize)
        };

        let mut neighbors = Vec::new();
        for row in rows {
            if unsafe { row.InterfaceLuid.Value } != target_luid {
                continue;
            }
            let Some(ip) = (unsafe { sockaddr_inet_ipv4(&row.Address) }) else { continue };
            let mac_bytes = &row.PhysicalAddress[..(row.PhysicalAddressLength as usize).min(32)];
            if mac_bytes.is_empty() || is_group_or_broadcast(mac_bytes) {
                continue;
            }
            let mac = format_mac(mac_bytes);
            let ip_str = ip.to_string();
            neighbors.push(ArpNeighbor {
                is_gateway: gateway_ip == Some(ip_str.as_str()),
                vendor: lookup_mac_vendor(mac.as_deref()),
                ip: ip_str,
                mac,
                state: neighbor_state_str(row.State).to_string(),
                device: iface.to_string(),
            });
        }

        unsafe { FreeMibTable(table as *const _) };
        neighbors
    }

    async fn wifi_info(&self) -> Option<WifiInfo> {
        tokio::task::spawn_blocking(wlan_info).await.ok().flatten()
    }

    async fn dns_info(&self, iface: &str) -> Option<DnsResolverInfo> {
        let adapters = list_adapters();
        let a = adapter_by_name(&adapters, iface)?;
        if a.dns_servers.is_empty() {
            return None;
        }
        let servers: Vec<String> = a.dns_servers.iter().map(|ip| ip.to_string()).collect();
        Some(DnsResolverInfo {
            link: iface.to_string(),
            current_server: servers.first().cloned(),
            servers,
            source: DnsSource::ResolvConf,
        })
    }

    /// Not implemented on Windows — returns None, same as macOS. The
    /// DNS-interception verdict is `uncertain` as a result.
    async fn system_egress_ip(&self) -> Option<String> {
        None
    }

    async fn interface_type(&self, iface: &str) -> InterfaceKind {
        let adapters = list_adapters();
        match adapter_by_name(&adapters, iface) {
            Some(a) => classify_if_type(a.if_type, iface),
            None => classify_if_type(0, iface),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — pure mappings only. Pointer-walking is covered by the contract
// tests (tests/topology.rs, tests/security.rs, tests/reliability.rs) on a
// real Windows machine.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot11_auth_maps_to_encryption_bucket() {
        assert_eq!(classify_dot11_auth(1), WifiEncryption::Open);
        assert_eq!(classify_dot11_auth(4), WifiEncryption::Wpa); // WPA_PSK
        assert_eq!(classify_dot11_auth(6), WifiEncryption::Wpa2Enterprise); // RSNA
        assert_eq!(classify_dot11_auth(7), WifiEncryption::Wpa2); // RSNA_PSK
        assert_eq!(classify_dot11_auth(8), WifiEncryption::Wpa3); // WPA3
        assert_eq!(classify_dot11_auth(9), WifiEncryption::Wpa3); // WPA3_SAE
        assert_eq!(classify_dot11_auth(-2), WifiEncryption::Unknown); // IHV range
    }

    #[test]
    fn if_type_maps_to_interface_kind() {
        assert_eq!(classify_if_type(IF_TYPE_IEEE80211, "Wi-Fi"), InterfaceKind::WiFi);
        assert_eq!(classify_if_type(IF_TYPE_ETHERNET_CSMACD, "Ethernet0"), InterfaceKind::Ethernet);
        assert_eq!(classify_if_type(IF_TYPE_TUNNEL, "wg0"), InterfaceKind::Vpn);
        assert_eq!(classify_if_type(999, "weird0"), InterfaceKind::Other);
        // name-based VPN detection wins even when the media type says Ethernet
        assert_eq!(classify_if_type(IF_TYPE_ETHERNET_CSMACD, "tailscale0"), InterfaceKind::Vpn);
    }

    #[test]
    fn filters_multicast_and_broadcast_macs() {
        assert!(is_group_or_broadcast(&[0x01, 0x00, 0x5e, 0, 0, 0x16])); // IPv4 multicast
        assert!(is_group_or_broadcast(&[0xff; 6])); // broadcast
        assert!(is_group_or_broadcast(&[])); // no MAC
        assert!(!is_group_or_broadcast(&[0x00, 0x50, 0x56, 0xfe, 0x6e, 0xa0])); // unicast
    }

    #[test]
    fn format_mac_is_uppercase_dash_separated() {
        assert_eq!(
            format_mac(&[0x00, 0x50, 0x56, 0xfe, 0x6e, 0xa0]).as_deref(),
            Some("00-50-56-FE-6E-A0")
        );
        // and round-trips through the shared vendor lookup
        assert_eq!(
            lookup_mac_vendor(format_mac(&[0x68, 0x7f, 0xf0, 1, 2, 3]).as_deref()),
            Some("TP-Link".to_string())
        );
        assert_eq!(format_mac(&[]), None);
    }

    #[test]
    fn neighbor_state_labels() {
        assert_eq!(neighbor_state_str(5), "Reachable");
        assert_eq!(neighbor_state_str(4), "Stale");
        assert_eq!(neighbor_state_str(6), "Permanent");
        assert_eq!(neighbor_state_str(42), "Unknown");
    }
}
