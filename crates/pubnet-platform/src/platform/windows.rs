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

use super::{AddrInfo, PlatformProbe, RouteInfo, WifiInfo, is_vpn_iface};
use crate::network::{PingSummary, lookup_mac_vendor};
use crate::bss::parse_rsn_ie;
use crate::types::{ArpNeighbor, BssEntry, DnsResolverInfo, DnsSource, InterfaceKind, WifiEncryption};
use std::net::Ipv4Addr;

use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, HANDLE, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
    GetBestRoute2, GetIpNetTable2, ICMP_ECHO_REPLY, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211,
    IF_TYPE_PPP, IF_TYPE_TUNNEL, IP_ADAPTER_ADDRESSES_LH, IP_SUCCESS, IcmpCloseHandle,
    IcmpCreateFile, IcmpSendEcho2, MIB_IPFORWARD_ROW2, MIB_IPNET_TABLE2,
};
use windows_sys::Win32::NetworkManagement::WiFi::{
    DOT11_AUTH_ALGORITHM, WLAN_BSS_ENTRY, WLAN_BSS_LIST, WLAN_CONNECTION_ATTRIBUTES,
    WLAN_CONNECTION_PARAMETERS, WLAN_INTERFACE_INFO_LIST, WlanCloseHandle, WlanConnect,
    WlanDeleteProfile, WlanEnumInterfaces, WlanFreeMemory, WlanGetNetworkBssList, WlanOpenHandle,
    WlanQueryInterface, WlanSetProfile, dot11_BSS_type_any, dot11_BSS_type_infrastructure,
    wlan_connection_mode_profile, wlan_interface_state_connected, wlan_intf_opcode_channel_number,
    wlan_intf_opcode_current_connection,
};
use windows_sys::Win32::Networking::WinSock::{
    AF_INET, IN_ADDR, IN_ADDR_0, SOCKADDR, SOCKADDR_IN, SOCKADDR_INET,
};

/// `IcmpCreateFile` failure sentinel.
const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;

// Compile-time layout checks. Each one is a property the `unsafe` below
// actually relies on, stated from the documented C layout — if a `windows-sys`
// bump (or a wrong feature set) changed a struct out from under us, the build
// stops here instead of silently reading the wrong bytes. Not a full offset
// audit — that's `windows-sys`'s job, and the topology contract test's
// gateway-in-subnet check is the real cross-check on a live machine.
const _: () = {
    use std::mem::{align_of, size_of};
    // The ICMP reply buffer is `[u64; _]`; the `&ICMP_ECHO_REPLY` read out of
    // it is only sound if that alignment is enough.
    assert!(align_of::<ICMP_ECHO_REPLY>() <= 8);
    // `sockaddr_ptr_ipv4` does `read_unaligned::<SOCKADDR_IN>` from a pointer
    // the OS only guarantees points to `sizeof(sockaddr_in)` == 16 bytes.
    assert!(size_of::<SOCKADDR>() == 16);
    assert!(size_of::<SOCKADDR_IN>() == 16);
    assert!(size_of::<IN_ADDR>() == 4);
    // `.Luid.Value` / `.InterfaceLuid.Value` read the whole union as one u64.
    assert!(size_of::<windows_sys::Win32::NetworkManagement::Ndis::NET_LUID_LH>() == 8);
    // `list_adapters` casts a `Vec<u64>` (align 8) buffer to this.
    assert!(align_of::<IP_ADAPTER_ADDRESSES_LH>() <= 8);
};

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
const MAX_BSS_ENTRIES: usize = 512;
const MAX_IE_BYTES: usize = 4096;

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
    Some(
        mac.iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join("-"),
    )
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
        sin_addr: IN_ADDR {
            S_un: IN_ADDR_0 {
                S_addr: u32::from_ne_bytes(ip.octets()),
            },
        },
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
    let all_fail = PingSummary {
        transmitted: count,
        received: 0,
        rtts: Vec::new(),
    };

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
    PingSummary {
        transmitted: count,
        received: rtts.len() as u32,
        rtts,
    }
}

/// Ping `host` `count` times over ICMP. Runs on the blocking pool — each echo
/// is a synchronous `IcmpSendEcho2`.
pub async fn icmp_ping(host: &str, count: u32) -> PingSummary {
    let Ok(ip) = host.parse::<Ipv4Addr>() else {
        return PingSummary {
            transmitted: count,
            received: 0,
            rtts: Vec::new(),
        };
    };
    tokio::task::spawn_blocking(move || icmp_ping_blocking(ip, count))
        .await
        .unwrap_or(PingSummary {
            transmitted: count,
            received: 0,
            rtts: Vec::new(),
        })
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
fn wlan_query(
    handle: HANDLE,
    guid: &windows_sys::core::GUID,
    opcode: i32,
) -> Option<(WlanMem, u32)> {
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
    let channel =
        wlan_query(handle.0, &guid, wlan_intf_opcode_channel_number).and_then(|(mem, size)| {
            if (size as usize) < std::mem::size_of::<u32>() {
                return None;
            }
            let ch = unsafe { *(mem.0 as *const u32) };
            (1..=196).contains(&ch).then_some(ch)
        });

    Some(WifiInfo {
        ssid: Some(ssid),
        ssid_hidden: false,
        encryption,
        channel,
        frequency_mhz: None,
        signal_percent,
    })
}

// ---------------------------------------------------------------------------
// BSS scan
// ---------------------------------------------------------------------------

/// `ulChCenterFrequency` (kHz) → 2.4 / 5.0 / 6.0 GHz band label.
fn freq_khz_to_band(freq_khz: u32) -> Option<f64> {
    match freq_khz {
        2_400_000..=2_500_000 => Some(2.4),
        5_170_000..=5_950_000 => Some(5.0),
        5_950_001..=7_125_000 => Some(6.0),
        _ => None,
    }
}

/// Convert center-frequency (kHz) to an approximate channel number.
fn freq_khz_to_channel(freq_khz: u32) -> Option<u32> {
    let freq_mhz = freq_khz / 1000;
    match freq_mhz {
        // 2.4 GHz: ch 1 = 2412 MHz, step 5 MHz
        2412..=2484 => {
            if freq_mhz == 2484 {
                Some(14)
            } else {
                Some((freq_mhz - 2412) / 5 + 1)
            }
        }
        // 5 GHz: ch 36 = 5180 MHz, step 5 MHz
        5180..=5885 => Some((freq_mhz - 5180) / 5 + 36),
        // 6 GHz: ch 1 = 5955 MHz, step 5 MHz
        5955..=7115 => Some((freq_mhz - 5955) / 5 + 1),
        _ => None,
    }
}

/// BSSID bytes (6 octets) → `XX:XX:XX:XX:XX:XX` uppercase string.
fn format_bssid(mac: &[u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Blocking BSS list scan. Opens its own WLAN handle so it can be called
/// independently of `wlan_info`. Returns `None` when no WLAN adapter is found
/// (maps to exit 2); `Some` once the adapter is confirmed (empty = no APs).
fn wlan_scan_bss() -> Option<Vec<BssEntry>> {
    let mut raw: HANDLE = std::ptr::null_mut();
    let mut negotiated: u32 = 0;
    if unsafe { WlanOpenHandle(2, std::ptr::null(), &mut negotiated, &mut raw) } != NO_ERROR {
        return None; // wlansvc not running / no WLAN stack
    }
    let handle = WlanHandle(raw);

    let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    if unsafe { WlanEnumInterfaces(handle.0, std::ptr::null(), &mut list) } != NO_ERROR
        || list.is_null()
    {
        return None; // no WLAN interface
    }
    let list_mem = WlanMem(list as *mut _);
    let l = unsafe { &*list };
    let n = (l.dwNumberOfItems as usize).min(MAX_WLAN_INTERFACES);
    let ifaces = unsafe { std::slice::from_raw_parts(l.InterfaceInfo.as_ptr(), n) };
    let Some(iface) = ifaces.iter().find(|i| i.isState == 1).or_else(|| ifaces.first()) else {
        return None; // no WLAN interface in the list
    };
    let guid = iface.InterfaceGuid;

    // Connected BSSID — for marking `is_connected` on the matching entry.
    let connected_bssid: Option<[u8; 6]> =
        wlan_query(handle.0, &guid, wlan_intf_opcode_current_connection).and_then(
            |(conn_mem, conn_size)| {
                if (conn_size as usize) < std::mem::size_of::<WLAN_CONNECTION_ATTRIBUTES>() {
                    return None;
                }
                let conn = unsafe { &*(conn_mem.0 as *const WLAN_CONNECTION_ATTRIBUTES) };
                // Only meaningful when actually associated
                if conn.isState != 1 {
                    return None;
                }
                Some(conn.wlanAssociationAttributes.dot11Bssid)
            },
        );

    drop(list_mem);

    let mut bss_list: *mut WLAN_BSS_LIST = std::ptr::null_mut();
    let ret = unsafe {
        WlanGetNetworkBssList(
            handle.0,
            &guid,
            std::ptr::null(),    // all SSIDs
            dot11_BSS_type_any,
            0,                   // not security-enabled filter
            std::ptr::null(),
            &mut bss_list,
        )
    };
    if ret != NO_ERROR || bss_list.is_null() {
        // Adapter present but BSS query failed (e.g. adapter disabled mid-scan)
        return Some(Vec::new());
    }
    let bss_mem = WlanMem(bss_list as *mut _);

    let bl = unsafe { &*bss_list };
    let count = (bl.dwNumberOfItems as usize).min(MAX_BSS_ENTRIES);
    let entries = unsafe { std::slice::from_raw_parts(bl.wlanBssEntries.as_ptr(), count) };

    let mut results = Vec::with_capacity(count);
    for entry in entries {
        let entry: &WLAN_BSS_ENTRY = entry;

        // SSID
        let ssid_len = (entry.dot11Ssid.uSSIDLength as usize).min(entry.dot11Ssid.ucSSID.len());
        let ssid = if ssid_len == 0 {
            None
        } else {
            let raw = &entry.dot11Ssid.ucSSID[..ssid_len];
            // Valid UTF-8 is expected; fall back gracefully for non-UTF-8 networks
            Some(String::from_utf8_lossy(raw).into_owned())
        };

        let bssid = format_bssid(&entry.dot11Bssid);

        // Parse RSN IE for auth mode — the IEs follow the WLAN_BSS_ENTRY struct
        // in the blob at offset uIeOffset, size uIeSize (relative to the entry).
        let auth_mode = {
            let ie_offset = entry.ulIeOffset as usize;
            let ie_size = (entry.ulIeSize as usize).min(MAX_IE_BYTES);
            // Safety: the pointer arithmetic is bounded by MAX_IE_BYTES and
            // ie_offset, both clamped; the blob was allocated by WlanGetNetworkBssList
            // and is alive for the scope of `bss_mem`.
            let entry_ptr = entry as *const WLAN_BSS_ENTRY as *const u8;
            let ie_slice = if ie_size > 0 {
                unsafe { std::slice::from_raw_parts(entry_ptr.add(ie_offset), ie_size) }
            } else {
                &[]
            };
            parse_rsn_ie(ie_slice)
        };

        let freq_khz = entry.ulChCenterFrequency;
        let signal = entry.uLinkQuality.min(100);
        let is_connected = connected_bssid == Some(entry.dot11Bssid);

        results.push(BssEntry {
            ssid,
            bssid,
            auth_mode,
            band: freq_khz_to_band(freq_khz),
            channel: freq_khz_to_channel(freq_khz),
            signal,
            is_connected,
        });
    }

    drop(bss_mem);
    Some(results)
}

// ---------------------------------------------------------------------------
// Repair (WPA2-PSK profile creation + WlanConnect)
// ---------------------------------------------------------------------------

// Per-user profile scope — no elevation required.
const WLAN_PROFILE_USER: u32 = 2;

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn str_to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn build_wpa2_profile(ssid: &str, passphrase: &str) -> String {
    let s = xml_escape(ssid);
    let p = xml_escape(passphrase);
    format!(
        r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
  <name>{s}</name>
  <SSIDConfig><SSID><name>{s}</name></SSID></SSIDConfig>
  <connectionType>ESS</connectionType>
  <connectionMode>auto</connectionMode>
  <MSM><security>
    <authEncryption>
      <authentication>WPA2PSK</authentication>
      <encryption>AES</encryption>
      <useOneX>false</useOneX>
    </authEncryption>
    <sharedKey>
      <keyType>passPhrase</keyType>
      <protected>false</protected>
      <keyMaterial>{p}</keyMaterial>
    </sharedKey>
  </security></MSM>
  <MacRandomization xmlns="http://www.microsoft.com/networking/WLAN/profile/v3">
    <enableRandomization>false</enableRandomization>
  </MacRandomization>
</WLANProfile>"#
    )
}

fn wlan_repair_blocking(ssid: &str, passphrase: &str) -> Result<(), String> {
    // Open handle
    let mut raw: HANDLE = std::ptr::null_mut();
    let mut negotiated: u32 = 0;
    if unsafe { WlanOpenHandle(2, std::ptr::null(), &mut negotiated, &mut raw) } != NO_ERROR {
        return Err("no WLAN adapter available".to_string());
    }
    let handle = WlanHandle(raw);

    // Pick an interface (prefer connected)
    let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    if unsafe { WlanEnumInterfaces(handle.0, std::ptr::null(), &mut list) } != NO_ERROR
        || list.is_null()
    {
        return Err("no WLAN interface found".to_string());
    }
    let list_mem = WlanMem(list as *mut _);
    let l = unsafe { &*list };
    let n = (l.dwNumberOfItems as usize).min(MAX_WLAN_INTERFACES);
    let ifaces = unsafe { std::slice::from_raw_parts(l.InterfaceInfo.as_ptr(), n) };
    let Some(iface) = ifaces.iter().find(|i| i.isState == 1).or_else(|| ifaces.first()) else {
        return Err("no WLAN interface found".to_string());
    };
    let guid = iface.InterfaceGuid;
    drop(list_mem);

    // Install WPA2-PSK profile (user scope, overwrites existing)
    let xml = build_wpa2_profile(ssid, passphrase);
    let wide_xml = str_to_wide(&xml);
    let mut reason_code: u32 = 0;
    let ret = unsafe {
        WlanSetProfile(
            handle.0,
            &guid,
            WLAN_PROFILE_USER,
            wide_xml.as_ptr(),
            std::ptr::null(), // no all-user security descriptor for user-scope profiles
            1,                // bOverwrite = TRUE
            std::ptr::null(),
            &mut reason_code,
        )
    };
    if ret != NO_ERROR {
        return Err(format!(
            "could not create profile (error {ret}, reason code {reason_code})"
        ));
    }

    // Initiate connection using the new profile
    let wide_name = str_to_wide(ssid);
    let params = WLAN_CONNECTION_PARAMETERS {
        wlanConnectionMode: wlan_connection_mode_profile,
        strProfile: wide_name.as_ptr(),
        pDot11Ssid: std::ptr::null_mut(),
        pDesiredBssidList: std::ptr::null_mut(),
        dot11BssType: dot11_BSS_type_infrastructure,
        dwFlags: 0,
    };
    let ret = unsafe { WlanConnect(handle.0, &guid, &params, std::ptr::null()) };
    if ret != NO_ERROR {
        // Remove the profile we just created before bailing
        let wide_del = str_to_wide(ssid);
        unsafe { WlanDeleteProfile(handle.0, &guid, wide_del.as_ptr(), std::ptr::null()) };
        return Err(format!("could not initiate connection (error {ret})"));
    }

    // Poll for connected state — up to 15 seconds at 500 ms intervals
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Some((mem, size)) =
            wlan_query(handle.0, &guid, wlan_intf_opcode_current_connection)
        {
            if (size as usize) < std::mem::size_of::<WLAN_CONNECTION_ATTRIBUTES>() {
                continue;
            }
            let conn = unsafe { &*(mem.0 as *const WLAN_CONNECTION_ATTRIBUTES) };
            if conn.isState == wlan_interface_state_connected {
                let ssid_len = (conn.wlanAssociationAttributes.dot11Ssid.uSSIDLength as usize)
                    .min(conn.wlanAssociationAttributes.dot11Ssid.ucSSID.len());
                let connected =
                    String::from_utf8_lossy(&conn.wlanAssociationAttributes.dot11Ssid.ucSSID[..ssid_len]);
                if connected == ssid {
                    return Ok(());
                }
            }
        }
    }

    // Timed out — remove the profile we created
    let wide_del = str_to_wide(ssid);
    unsafe { WlanDeleteProfile(handle.0, &guid, wide_del.as_ptr(), std::ptr::null()) };
    Err("connection timed out after 15 seconds".to_string())
}

/// Remove the saved WLAN profile named `ssid` from all interfaces.
/// Returns `Err` only if the WLAN subsystem is unavailable; a missing profile
/// is not an error (the caller treats it as "already in unfixed state").
pub fn delete_wlan_profile(ssid: &str) -> Result<(), String> {
    let mut raw: HANDLE = std::ptr::null_mut();
    let mut negotiated: u32 = 0;
    if unsafe { WlanOpenHandle(2, std::ptr::null(), &mut negotiated, &mut raw) } != NO_ERROR {
        return Err("no WLAN adapter available".to_string());
    }
    let handle = WlanHandle(raw);

    let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    if unsafe { WlanEnumInterfaces(handle.0, std::ptr::null(), &mut list) } != NO_ERROR
        || list.is_null()
    {
        return Err("no WLAN interface found".to_string());
    }
    let list_mem = WlanMem(list as *mut _);
    let l = unsafe { &*list };
    let n = (l.dwNumberOfItems as usize).min(MAX_WLAN_INTERFACES);
    let ifaces = unsafe { std::slice::from_raw_parts(l.InterfaceInfo.as_ptr(), n) };

    let wide_ssid = str_to_wide(ssid);
    let mut found = false;
    for iface in ifaces {
        let ret = unsafe {
            WlanDeleteProfile(handle.0, &iface.InterfaceGuid, wide_ssid.as_ptr(), std::ptr::null())
        };
        if ret == NO_ERROR {
            found = true;
        }
    }
    drop(list_mem);

    if found {
        Ok(())
    } else {
        Err(format!("no saved profile named '{ssid}'"))
    }
}


/// Create a WPA2-PSK profile for `ssid` and connect. Blocks the current
/// thread for up to ~15 s polling for the connected state. Cleans up the
/// created profile on any failure after profile creation.
pub async fn repair_wpa2(ssid: &str, passphrase: &str) -> Result<(), String> {
    let ssid = ssid.to_string();
    let passphrase = passphrase.to_string();
    tokio::task::spawn_blocking(move || wlan_repair_blocking(&ssid, &passphrase))
        .await
        .unwrap_or_else(|_| Err("internal error in repair task".to_string()))
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
        Some(RouteInfo {
            gateway: gateway.to_string(),
            device,
        })
    }

    async fn interface_addr(&self, iface: &str) -> Option<AddrInfo> {
        let adapters = list_adapters();
        let a = adapter_by_name(&adapters, iface)?;
        let (ip, prefix) = a.ipv4.iter().find(|(ip, _)| is_plausible_host_ipv4(*ip))?;
        Some(AddrInfo {
            ip: ip.to_string(),
            prefix: *prefix,
        })
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
            let Some(ip) = (unsafe { sockaddr_inet_ipv4(&row.Address) }) else {
                continue;
            };
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

    /// The WLAN API returns SSID, auth algorithm, signal, and channel in one
    /// query, so `iface` and `detail` are unused here.
    async fn wifi_info(&self, _iface: &str, _detail: bool) -> Option<WifiInfo> {
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

    async fn scan_bss_list(&self) -> Option<Vec<BssEntry>> {
        tokio::task::spawn_blocking(wlan_scan_bss)
            .await
            .unwrap_or(None)
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
        assert_eq!(
            classify_if_type(IF_TYPE_IEEE80211, "Wi-Fi"),
            InterfaceKind::WiFi
        );
        assert_eq!(
            classify_if_type(IF_TYPE_ETHERNET_CSMACD, "Ethernet0"),
            InterfaceKind::Ethernet
        );
        assert_eq!(classify_if_type(IF_TYPE_TUNNEL, "wg0"), InterfaceKind::Vpn);
        assert_eq!(classify_if_type(999, "weird0"), InterfaceKind::Other);
        // name-based VPN detection wins even when the media type says Ethernet
        assert_eq!(
            classify_if_type(IF_TYPE_ETHERNET_CSMACD, "tailscale0"),
            InterfaceKind::Vpn
        );
    }

    #[test]
    fn filters_multicast_and_broadcast_macs() {
        assert!(is_group_or_broadcast(&[0x01, 0x00, 0x5e, 0, 0, 0x16])); // IPv4 multicast
        assert!(is_group_or_broadcast(&[0xff; 6])); // broadcast
        assert!(is_group_or_broadcast(&[])); // no MAC
        assert!(!is_group_or_broadcast(&[
            0x00, 0x50, 0x56, 0xfe, 0x6e, 0xa0
        ])); // unicast
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

    #[test]
    fn freq_khz_band_classification() {
        assert_eq!(freq_khz_to_band(2_437_000), Some(2.4)); // ch 6
        assert_eq!(freq_khz_to_band(5_180_000), Some(5.0)); // ch 36
        assert_eq!(freq_khz_to_band(6_115_000), Some(6.0)); // 6 GHz
        assert_eq!(freq_khz_to_band(0), None);
        assert_eq!(freq_khz_to_band(60_000_000), None); // 60 GHz (not classified)
    }

    #[test]
    fn freq_khz_channel_mapping() {
        assert_eq!(freq_khz_to_channel(2_412_000), Some(1));
        assert_eq!(freq_khz_to_channel(2_437_000), Some(6));
        assert_eq!(freq_khz_to_channel(2_484_000), Some(14));
        assert_eq!(freq_khz_to_channel(5_180_000), Some(36));
        assert_eq!(freq_khz_to_channel(5_500_000), Some(100));
        assert_eq!(freq_khz_to_channel(5_955_000), Some(1)); // 6 GHz ch 1
        assert_eq!(freq_khz_to_channel(0), None);
    }

    #[test]
    fn bssid_format_is_colon_separated_uppercase() {
        assert_eq!(
            format_bssid(&[0x00, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e]),
            "00:1A:2B:3C:4D:5E"
        );
    }

    #[test]
    fn xml_escape_handles_reserved_chars() {
        assert_eq!(xml_escape("AT&T"), "AT&amp;T");
        assert_eq!(xml_escape("<tag>"), "&lt;tag&gt;");
        assert_eq!(xml_escape("a\"b"), "a&quot;b");
        assert_eq!(xml_escape("it's"), "it&apos;s");
        assert_eq!(xml_escape("normal"), "normal");
    }

    #[test]
    fn wpa2_profile_contains_correct_auth_and_ssid() {
        let xml = build_wpa2_profile("MyNetwork", "hunter2");
        assert!(xml.contains("<name>MyNetwork</name>"));
        assert!(xml.contains("<authentication>WPA2PSK</authentication>"));
        assert!(xml.contains("<encryption>AES</encryption>"));
        assert!(xml.contains("<keyMaterial>hunter2</keyMaterial>"));
        assert!(xml.contains("<enableRandomization>false</enableRandomization>"));
        assert!(xml.contains("<useOneX>false</useOneX>"));
    }

    #[test]
    fn wpa2_profile_escapes_special_chars_in_ssid_and_passphrase() {
        let xml = build_wpa2_profile("AT&T Fiber", "pass<word>&\"it's\"");
        assert!(xml.contains("AT&amp;T Fiber"));
        assert!(xml.contains("pass&lt;word&gt;&amp;&quot;it&apos;s&quot;"));
    }

    #[test]
    fn str_to_wide_is_nul_terminated() {
        let wide = str_to_wide("hi");
        assert_eq!(wide, vec!['h' as u16, 'i' as u16, 0]);
    }
}
