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

// Defensive upper bounds on anything the OS reports as a count or length. A
// real machine is orders of magnitude below every one of these; a value past
// the bound means the data is malformed, and we clamp/stop rather than feed it
// to `slice::from_raw_parts` or loop on it. Clamping a length can only ever
// *shorten* a slice, so it never introduces an out-of-bounds read.
const MAX_ADAPTERS: usize = 4096;
const MAX_ADDRS_PER_ADAPTER: usize = 512;
const MAX_NEIGHBORS: usize = 131_072;
const MAX_WLAN_INTERFACES: usize = 64;
const MAX_WIDE_STR_UNITS: usize = 4096;

// ---------------------------------------------------------------------------
// Small pure helpers (unit-tested)
// ---------------------------------------------------------------------------

/// A value we'd accept as a real gateway / neighbour / local host address:
/// routable-looking unicast IPv4, not loopback / multicast / broadcast /
/// "0.0.0.0". Link-local (169.254/16) is allowed — it's a legitimate DHCP
/// failure address a user might want flagged, not garbage.
fn is_plausible_host_ipv4(ip: Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_multicast() && !ip.is_broadcast() && !ip.is_unspecified()
}

/// `OnLinkPrefixLength` (or a route prefix) as a CIDR width, or `None` if the
/// OS reported something out of range (it uses 255 for "unknown").
fn valid_ipv4_prefix(prefix: u8) -> Option<u32> {
    (prefix <= 32).then_some(prefix as u32)
}

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

/// Reads a NUL-terminated wide (UTF-16) string, stopping at `MAX_WIDE_STR_UNITS`
/// so a (contract-violating) unterminated string can't run off into memory.
unsafe fn wide_ptr_to_string(mut p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut units = Vec::new();
    while units.len() < MAX_WIDE_STR_UNITS && unsafe { *p } != 0 {
        units.push(unsafe { *p });
        p = unsafe { p.add(1) };
    }
    String::from_utf16_lossy(&units)
}

/// IPv4 out of a raw `SOCKADDR*` (from a `SOCKET_ADDRESS`), if it is AF_INET.
/// The pointer is read with `read_unaligned` — `SOCKET_ADDRESS.lpSockaddr` is
/// only documented to be a valid sockaddr, not to be aligned for `SOCKADDR_IN`.
unsafe fn sockaddr_ptr_ipv4(sa: *const SOCKADDR) -> Option<Ipv4Addr> {
    if sa.is_null() {
        return None;
    }
    let generic: SOCKADDR = unsafe { std::ptr::read_unaligned(sa) };
    if generic.sa_family != AF_INET {
        return None;
    }
    let v4: SOCKADDR_IN = unsafe { std::ptr::read_unaligned(sa as *const SOCKADDR_IN) };
    let s_addr = unsafe { v4.sin_addr.S_un.S_addr };
    Some(Ipv4Addr::from(s_addr.to_ne_bytes()))
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
    /// (address, CIDR prefix width). Prefix already validated to be `<= 32`.
    ipv4: Vec<(Ipv4Addr, u32)>,
    dns_servers: Vec<Ipv4Addr>,
}

/// One `GetAdaptersAddresses` call → owned Rust structs (no live pointers).
/// Returns an empty Vec on any API failure or if the linked list looks
/// malformed (see the `MAX_*` bounds).
fn list_adapters() -> Vec<AdapterInfo> {
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
    let mut size: u32 = 16 * 1024;
    let mut buf: Vec<u64> = Vec::new();
    let mut last = ERROR_BUFFER_OVERFLOW;

    // Grow-and-retry: the first call sizes the buffer.
    for _ in 0..4 {
        buf = vec![0u64; (size as usize).div_ceil(8)];
        last = unsafe {
            GetAdaptersAddresses(
                AF_INET as u32,
                flags,
                std::ptr::null(),
                buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                &mut size,
            )
        };
        if last != ERROR_BUFFER_OVERFLOW {
            break;
        }
    }
    if last != NO_ERROR {
        return Vec::new();
    }

    let mut adapters = Vec::new();
    let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !cur.is_null() && adapters.len() < MAX_ADAPTERS {
        let a = unsafe { &*cur };

        let mut ipv4 = Vec::new();
        let mut uni = a.FirstUnicastAddress;
        while !uni.is_null() && ipv4.len() < MAX_ADDRS_PER_ADAPTER {
            let u = unsafe { &*uni };
            if let Some(ip) = unsafe { sockaddr_ptr_ipv4(u.Address.lpSockaddr) }
                && let Some(prefix) = valid_ipv4_prefix(u.OnLinkPrefixLength)
            {
                ipv4.push((ip, prefix));
            }
            uni = u.Next;
        }

        let mut dns_servers = Vec::new();
        let mut dns = a.FirstDnsServerAddress;
        while !dns.is_null() && dns_servers.len() < MAX_ADDRS_PER_ADAPTER {
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

/// Closes the ICMP handle on drop, so an unexpected panic between create and
/// the loop's end can't leak it.
struct IcmpHandle(HANDLE);
impl Drop for IcmpHandle {
    fn drop(&mut self) {
        unsafe { IcmpCloseHandle(self.0) };
    }
}

fn icmp_ping_blocking(ip: Ipv4Addr, count: u32) -> PingSummary {
    let all_fail = PingSummary { transmitted: count, received: 0, rtts: Vec::new() };

    let raw = unsafe { IcmpCreateFile() };
    if raw == INVALID_HANDLE_VALUE || raw.is_null() {
        return all_fail;
    }
    let handle = IcmpHandle(raw);

    const REQUEST_LEN: usize = 32;
    let dest: u32 = u32::from_ne_bytes(ip.octets());
    let request = [0x61u8; REQUEST_LEN]; // arbitrary payload

    // Reply buffer: one ICMP_ECHO_REPLY + the echoed payload + 8 bytes for an
    // optional ICMP error record (per the IcmpSendEcho2 docs). Backed by
    // `[u64]`, not `[u8]`, so it is 8-byte aligned for the ICMP_ECHO_REPLY
    // read below (which has a pointer field).
    const REPLY_BYTES: usize = std::mem::size_of::<ICMP_ECHO_REPLY>() + REQUEST_LEN + 8;
    let mut reply = [0u64; REPLY_BYTES.div_ceil(8)];
    let reply_bytes = std::mem::size_of_val(&reply) as u32;

    let mut rtts = Vec::new();
    for i in 0..count {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let n = unsafe {
            IcmpSendEcho2(
                handle.0,
                std::ptr::null_mut(),
                None,
                std::ptr::null(),
                dest,
                request.as_ptr() as *const _,
                request.len() as u16,
                std::ptr::null(),
                reply.as_mut_ptr() as *mut _,
                reply_bytes,
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

    drop(handle); // IcmpCloseHandle
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

/// Closes the WLAN client handle on drop.
struct WlanHandle(HANDLE);
impl Drop for WlanHandle {
    fn drop(&mut self) {
        unsafe { WlanCloseHandle(self.0, std::ptr::null()) };
    }
}

/// `WlanFreeMemory`s a `Wlan*`-allocated blob on drop.
struct WlanMem(*mut std::ffi::c_void);
impl Drop for WlanMem {
    fn drop(&mut self) {
        unsafe { WlanFreeMemory(self.0) };
    }
}

/// `opcode` → a `WlanMem` blob plus its byte size, or `None`.
fn wlan_query(handle: HANDLE, guid: &windows_sys::core::GUID, opcode: i32) -> Option<(WlanMem, u32)> {
    let mut size: u32 = 0;
    let mut data: *mut std::ffi::c_void = std::ptr::null_mut();
    let ret = unsafe {
        WlanQueryInterface(
            handle,
            guid,
            opcode,
            std::ptr::null(),
            &mut size,
            &mut data,
            std::ptr::null_mut(),
        )
    };
    if ret != NO_ERROR || data.is_null() {
        return None;
    }
    Some((WlanMem(data), size))
}

fn wlan_info() -> Option<WifiInfo> {
    let mut raw: HANDLE = std::ptr::null_mut();
    let mut negotiated: u32 = 0;
    if unsafe { WlanOpenHandle(2, std::ptr::null(), &mut negotiated, &mut raw) } != NO_ERROR {
        return None; // wlansvc not running, or no WLAN stack
    }
    let handle = WlanHandle(raw);

    // Which WLAN interface — prefer a connected one, else the first.
    let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    if unsafe { WlanEnumInterfaces(handle.0, std::ptr::null(), &mut list) } != NO_ERROR
        || list.is_null()
    {
        return None;
    }
    let list_mem = WlanMem(list as *mut _);
    let l = unsafe { &*list };
    let n = (l.dwNumberOfItems as usize).min(MAX_WLAN_INTERFACES);
    let ifaces = unsafe { std::slice::from_raw_parts(l.InterfaceInfo.as_ptr(), n) };
    let guid = ifaces
        .iter()
        .find(|i| i.isState == 1)
        .or_else(|| ifaces.first())
        .map(|i| i.InterfaceGuid)?;
    drop(list_mem);

    // current_connection → SSID, auth algorithm, signal quality
    let (conn_mem, conn_size) = wlan_query(handle.0, &guid, wlan_intf_opcode_current_connection)?;
    if (conn_size as usize) < std::mem::size_of::<WLAN_CONNECTION_ATTRIBUTES>() {
        return None;
    }
    let conn = unsafe { &*(conn_mem.0 as *const WLAN_CONNECTION_ATTRIBUTES) };
    let assoc = &conn.wlanAssociationAttributes;
    let ssid_len = (assoc.dot11Ssid.uSSIDLength as usize).min(assoc.dot11Ssid.ucSSID.len());
    let ssid = String::from_utf8_lossy(&assoc.dot11Ssid.ucSSID[..ssid_len]).into_owned();
    if ssid.is_empty() {
        return None;
    }
    let signal_percent = Some(assoc.wlanSignalQuality.min(100));
    let encryption = classify_dot11_auth(conn.wlanSecurityAttributes.dot11AuthAlgorithm);

    // channel_number — a separate query, absent on some drivers.
    let channel = wlan_query(handle.0, &guid, wlan_intf_opcode_channel_number).and_then(
        |(mem, size)| {
            if (size as usize) < std::mem::size_of::<u32>() {
                return None;
            }
            let ch = unsafe { *(mem.0 as *const u32) };
            (1..=196).contains(&ch).then_some(ch)
        },
    );

    Some(WifiInfo { ssid, encryption, channel, frequency_mhz: None, signal_percent })
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
        // `is_plausible_host_ipv4` also rejects 0.0.0.0 — an on-link route with
        // no gateway — which is what we'd get on a link with no router.
        let gateway = unsafe { sockaddr_inet_ipv4(&row.NextHop) }?;
        if !is_plausible_host_ipv4(gateway) {
            return None;
        }
        let luid = unsafe { row.InterfaceLuid.Value };
        if luid == 0 {
            return None;
        }
        let device = list_adapters()
            .into_iter()
            .find(|a| a.luid == luid)
            .map(|a| a.friendly_name)
            .filter(|name| !name.is_empty())?;
        Some(RouteInfo { gateway: gateway.to_string(), device })
    }

    async fn interface_addr(&self, iface: &str) -> Option<AddrInfo> {
        let adapters = list_adapters();
        let a = adapter_by_name(&adapters, iface)?;
        let (ip, prefix) = a.ipv4.iter().find(|(ip, _)| is_plausible_host_ipv4(*ip))?;
        Some(AddrInfo { ip: ip.to_string(), prefix: *prefix })
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
        // Clamp the count the OS reported before handing it to `from_raw_parts`
        // (clamping can only shorten the slice, never overrun the allocation).
        let count = (t.NumEntries as usize).min(MAX_NEIGHBORS);
        let rows = unsafe { std::slice::from_raw_parts(t.Table.as_ptr(), count) };

        let mut neighbors = Vec::new();
        for row in rows {
            if unsafe { row.InterfaceLuid.Value } != target_luid {
                continue;
            }
            let Some(ip) = (unsafe { sockaddr_inet_ipv4(&row.Address) }) else { continue };
            if !is_plausible_host_ipv4(ip) {
                continue;
            }
            let mac_len = (row.PhysicalAddressLength as usize).min(row.PhysicalAddress.len());
            let mac_bytes = &row.PhysicalAddress[..mac_len];
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

    #[test]
    fn plausible_host_ipv4_rejects_non_unicast() {
        assert!(is_plausible_host_ipv4(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(is_plausible_host_ipv4(Ipv4Addr::new(169, 254, 3, 4))); // link-local is allowed
        assert!(!is_plausible_host_ipv4(Ipv4Addr::UNSPECIFIED)); // 0.0.0.0 (on-link route)
        assert!(!is_plausible_host_ipv4(Ipv4Addr::new(127, 0, 0, 1)));
        assert!(!is_plausible_host_ipv4(Ipv4Addr::new(224, 0, 0, 251))); // mDNS multicast
        assert!(!is_plausible_host_ipv4(Ipv4Addr::new(255, 255, 255, 255)));
    }

    #[test]
    fn ipv4_prefix_range_is_enforced() {
        assert_eq!(valid_ipv4_prefix(0), Some(0));
        assert_eq!(valid_ipv4_prefix(24), Some(24));
        assert_eq!(valid_ipv4_prefix(32), Some(32));
        assert_eq!(valid_ipv4_prefix(33), None);
        assert_eq!(valid_ipv4_prefix(255), None); // the OS "unknown" sentinel
    }
}
