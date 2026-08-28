//! macOS implementation of PlatformProbe.
//! Commands: route, ifconfig, arp, scutil, networksetup, ipconfig, system_profiler.

use super::{AddrInfo, PlatformProbe, RouteInfo, WifiInfo, is_vpn_iface};
use crate::exec::{ExecResult, cmd, exec_cmd};
use crate::network::lookup_mac_vendor;
use crate::types::{ArpNeighbor, BssEntry, DnsResolverInfo, DnsSource, InterfaceKind, WifiEncryption};
use regex::Regex;
use serde::Deserialize;
use std::sync::LazyLock;

fn empty() -> ExecResult {
    ExecResult {
        stdout: String::new(),
        stderr: String::new(),
        exit_code: None,
    }
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

static ROUTE_GW_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*gateway:\s+(\S+)").unwrap());
static ROUTE_IF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*interface:\s+(\S+)").unwrap());

/// Parses `route -n get default` into gateway + interface.
pub fn parse_route_get(raw: &str) -> Option<RouteInfo> {
    let gateway = ROUTE_GW_RE.captures(raw).map(|c| c[1].to_string())?;
    let device = ROUTE_IF_RE.captures(raw).map(|c| c[1].to_string())?;
    Some(RouteInfo { gateway, device })
}

static IFCONFIG_INET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^\s*inet (\d+\.\d+\.\d+\.\d+) netmask (0x[0-9a-f]+)").unwrap()
});

fn hex_mask_to_prefix(hex: &str) -> Option<u32> {
    let val = u32::from_str_radix(hex.trim_start_matches("0x"), 16).ok()?;
    Some(val.count_ones())
}

/// Parses `ifconfig <iface>` into IP + prefix length.
pub fn parse_ifconfig(raw: &str) -> Option<AddrInfo> {
    let caps = IFCONFIG_INET_RE.captures(raw)?;
    let ip = caps[1].to_string();
    let prefix = hex_mask_to_prefix(&caps[2])?;
    Some(AddrInfo { ip, prefix })
}

static ARP_ENTRY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\((\d+\.\d+\.\d+\.\d+)\) at ([0-9a-f:]+) on (\S+)").unwrap());

/// Parses `arp -an -i <iface>`.
pub fn parse_arp(raw: &str, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
    let mut neighbors = Vec::new();
    for line in raw.lines() {
        let Some(caps) = ARP_ENTRY_RE.captures(line) else {
            continue;
        };
        let ip = caps[1].to_string();
        let mac_str = &caps[2];
        // Skip broadcast (ff:ff:ff:ff:ff:ff) and multicast (I/G bit set in first octet)
        let first_byte =
            u8::from_str_radix(mac_str.split(':').next().unwrap_or("0"), 16).unwrap_or(0);
        if first_byte & 1 != 0 {
            continue;
        }
        let mac = Some(mac_str.to_string());
        let is_gateway = gateway_ip.is_some_and(|g| g == ip);
        neighbors.push(ArpNeighbor {
            ip,
            vendor: lookup_mac_vendor(mac.as_deref()),
            mac,
            state: "REACHABLE".to_string(),
            device: iface.to_string(),
            is_gateway,
        });
    }
    neighbors
}

fn rssi_to_percent(rssi: i32) -> u32 {
    ((rssi + 100) * 2).clamp(0, 100) as u32
}

const REDACTED: &str = "<redacted>";

/// Classifies a Wi-Fi security label into a `WifiEncryption`. Handles both the
/// `ipconfig getsummary` form (`WPA2_PSK`, `WPA3_SAE`, `NONE`, `802_1X`, …) and
/// the `system_profiler` form (`spairport_security_mode_wpa2_personal`,
/// `…_none`, `…_wpa2_enterprise`, …) — the substring checks cover both.
fn classify_wifi_security(raw: &str) -> WifiEncryption {
    let s = raw.to_ascii_lowercase();
    if s.trim().is_empty() {
        WifiEncryption::Unknown
    } else if s.contains("wpa3") {
        WifiEncryption::Wpa3
    } else if s.contains("enterprise") || s.contains("802.1x") || s.contains("802_1x") {
        WifiEncryption::Wpa2Enterprise
    } else if s.contains("wpa2") {
        WifiEncryption::Wpa2
    } else if s.contains("wpa") || s.contains("wep") {
        WifiEncryption::Wpa
    } else if s.contains("none") || s.contains("open") {
        WifiEncryption::Open
    } else {
        WifiEncryption::Unknown
    }
}

/// Maps a channel number + its band (as printed by `system_profiler`, e.g.
/// `"48 (5GHz, 80MHz)"`) to a centre frequency in MHz.
fn channel_band_to_mhz(channel: u32, raw_channel: &str) -> Option<u32> {
    if raw_channel.contains("6GHz") {
        Some(5950 + channel * 5)
    } else if raw_channel.contains("5GHz") {
        Some(5000 + channel * 5)
    } else if raw_channel.contains("2GHz") {
        Some(if channel == 14 {
            2484
        } else {
            2407 + channel * 5
        })
    } else {
        None
    }
}

/// The fast Wi-Fi read: `ipconfig getsummary <iface>` → SSID + encryption.
pub struct IpconfigWifi {
    /// `None` when the SSID key was `<redacted>`, empty, or absent.
    pub ssid: Option<String>,
    pub ssid_hidden: bool,
    pub encryption: WifiEncryption,
}

/// Parses `ipconfig getsummary <iface>`. Returns `None` when the interface is
/// not an associated Wi-Fi link (Ethernet, VPN, or Wi-Fi with no association),
/// so the caller can treat "not on Wi-Fi" and "parse failed" the same way.
pub fn parse_ipconfig_getsummary(raw: &str) -> Option<IpconfigWifi> {
    let mut interface_type: Option<String> = None;
    let mut link_active = false;
    let mut ssid: Option<String> = None;
    let mut security: Option<String> = None;

    for line in raw.lines() {
        let Some((key, val)) = line.split_once(':') else {
            continue;
        };
        let (key, val) = (key.trim(), val.trim());
        match key {
            "InterfaceType" => interface_type = Some(val.to_string()),
            "LinkStatusActive" => link_active = val.eq_ignore_ascii_case("true"),
            "SSID" => ssid = Some(val.to_string()),
            "Security" => security = Some(val.to_string()),
            _ => {}
        }
    }

    // Only Wi-Fi interfaces carry an SSID/Security. If InterfaceType says
    // otherwise, or nothing points to an association, this isn't a Wi-Fi link.
    let is_wifi = interface_type
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case("wifi"));
    if !is_wifi || !(link_active || security.is_some() || ssid.is_some()) {
        return None;
    }

    let visible_ssid = ssid.filter(|s| s.as_str() != REDACTED && !s.is_empty());
    let encryption = security
        .filter(|s| !s.is_empty())
        .map(|s| classify_wifi_security(&s))
        .unwrap_or(WifiEncryption::Unknown);

    Some(IpconfigWifi {
        ssid_hidden: visible_ssid.is_none(),
        ssid: visible_ssid,
        encryption,
    })
}

// --- system_profiler -json SPAirPortDataType ---

#[derive(Deserialize)]
struct SpRoot {
    #[serde(rename = "SPAirPortDataType", default)]
    data: Vec<SpData>,
}

#[derive(Deserialize)]
struct SpData {
    #[serde(default)]
    spairport_airport_interfaces: Vec<SpIface>,
}

#[derive(Deserialize)]
struct SpIface {
    #[serde(rename = "_name", default)]
    name: String,
    #[serde(default)]
    spairport_status_information: String,
    spairport_current_network_information: Option<SpCurrentNetwork>,
}

#[derive(Deserialize, Default)]
struct SpCurrentNetwork {
    #[serde(rename = "_name", default)]
    name: String,
    #[serde(default)]
    spairport_network_channel: String,
    #[serde(default)]
    spairport_security_mode: String,
    #[serde(default)]
    spairport_signal_noise: String,
}

/// The slow Wi-Fi read: `system_profiler -json SPAirPortDataType` → channel,
/// frequency, signal (and encryption/SSID as a fallback for the fast path).
pub struct SystemProfilerWifi {
    pub ssid: Option<String>,
    pub encryption: WifiEncryption,
    pub channel: Option<u32>,
    pub frequency_mhz: Option<u32>,
    pub signal_percent: Option<u32>,
}

/// Parses `system_profiler -json SPAirPortDataType` for the connected network
/// on `iface`. Returns `None` if the JSON doesn't parse or `iface` isn't a
/// connected Wi-Fi interface in it.
pub fn parse_system_profiler_wifi(raw: &str, iface: &str) -> Option<SystemProfilerWifi> {
    let root: SpRoot = serde_json::from_str(raw).ok()?;
    let current = root
        .data
        .iter()
        .flat_map(|d| &d.spairport_airport_interfaces)
        .find(|i| i.name == iface && i.spairport_status_information == "spairport_status_connected")
        .and_then(|i| i.spairport_current_network_information.as_ref())?;

    let channel = current
        .spairport_network_channel
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<u32>().ok());
    let frequency_mhz =
        channel.and_then(|c| channel_band_to_mhz(c, &current.spairport_network_channel));
    let signal_percent = current
        .spairport_signal_noise
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<i32>().ok())
        .map(rssi_to_percent);
    let ssid = match current.name.as_str() {
        "" | REDACTED => None,
        s => Some(s.to_string()),
    };

    Some(SystemProfilerWifi {
        ssid,
        encryption: classify_wifi_security(&current.spairport_security_mode),
        channel,
        frequency_mhz,
        signal_percent,
    })
}

/// Parses `scutil --dns` for the resolver block matching the given interface.
/// nameserver lines appear before if_index in scutil output, so we accumulate
/// each block's servers and interface name together, then check at the block
/// boundary rather than while streaming.
pub fn parse_scutil_dns(raw: &str, iface: &str) -> Option<DnsResolverInfo> {
    let iface_pattern = format!("({iface})");
    let mut block_servers: Vec<String> = Vec::new();
    let mut block_iface: Option<String> = None;

    let flush =
        |servers: &mut Vec<String>, found_iface: &mut Option<String>| -> Option<DnsResolverInfo> {
            if found_iface.as_deref() == Some(iface) && !servers.is_empty() {
                Some(DnsResolverInfo {
                    link: iface.to_string(),
                    current_server: servers.first().cloned(),
                    servers: servers.clone(),
                    source: DnsSource::Resolvectl,
                })
            } else {
                None
            }
        };

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("resolver #") {
            if let Some(result) = flush(&mut block_servers, &mut block_iface) {
                return Some(result);
            }
            block_servers.clear();
            block_iface = None;
        } else if let Some(ns) = trimmed.strip_prefix("nameserver[") {
            if let Some((_, addr)) = ns.split_once("] : ") {
                block_servers.push(addr.trim().to_string());
            }
        } else if trimmed.contains("if_index") && trimmed.contains(&iface_pattern) {
            block_iface = Some(iface.to_string());
        }
    }
    // Check the last block
    flush(&mut block_servers, &mut block_iface)
}

/// Parses `networksetup -listallhardwareports` to check if an interface is Wi-Fi.
/// Each block (separated by blank lines) has "Hardware Port: X" then "Device: Y".
pub fn is_wifi_hardware_port(raw: &str, iface: &str) -> bool {
    let mut current_is_wifi = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            current_is_wifi = false;
            continue;
        }
        if let Some(port) = trimmed.strip_prefix("Hardware Port:") {
            let lower = port.trim().to_lowercase();
            current_is_wifi =
                lower.contains("wi-fi") || lower.contains("airport") || lower.contains("wireless");
        } else if let Some(dev) = trimmed.strip_prefix("Device:")
            && dev.trim() == iface
            && current_is_wifi
        {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

pub struct MacProbe;

impl PlatformProbe for MacProbe {
    async fn default_route(&self) -> Option<RouteInfo> {
        let r = exec_cmd(cmd(&["route", "-n", "get", "default"]))
            .await
            .ok()?;
        parse_route_get(&r.stdout)
    }

    async fn interface_addr(&self, iface: &str) -> Option<AddrInfo> {
        let r = exec_cmd(cmd(&["ifconfig", iface])).await.ok()?;
        parse_ifconfig(&r.stdout)
    }

    async fn arp_neighbors(&self, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
        let r = exec_cmd(cmd(&["arp", "-an", "-i", iface]))
            .await
            .unwrap_or_else(|_| empty());
        parse_arp(&r.stdout, iface, gateway_ip)
    }

    /// `airport` was removed in macOS 15/26. The fast path
    /// (`ipconfig getsummary`) gives SSID + encryption instantly; the slow
    /// path (`system_profiler`, ~7s) adds channel/frequency/signal and only
    /// runs when `detail` is set. See
    /// docs/decisions/2026-08-26-macos-wifi-without-airport.md.
    async fn wifi_info(&self, iface: &str, detail: bool) -> Option<WifiInfo> {
        let fast = exec_cmd(cmd(&["ipconfig", "getsummary", iface]))
            .await
            .ok()
            .and_then(|r| parse_ipconfig_getsummary(&r.stdout));

        let slow = if detail {
            exec_cmd(cmd(&["system_profiler", "-json", "SPAirPortDataType"]))
                .await
                .ok()
                .and_then(|r| parse_system_profiler_wifi(&r.stdout, iface))
        } else {
            None
        };

        if fast.is_none() && slow.is_none() {
            return None;
        }

        // Fast path owns SSID + encryption; slow path fills channel/signal and
        // backfills SSID/encryption only where the fast path came up empty.
        let ssid = fast
            .as_ref()
            .and_then(|f| f.ssid.clone())
            .or_else(|| slow.as_ref().and_then(|s| s.ssid.clone()));
        let encryption = fast
            .as_ref()
            .map(|f| f.encryption)
            .filter(|e| *e != WifiEncryption::Unknown)
            .or_else(|| slow.as_ref().map(|s| s.encryption))
            .unwrap_or(WifiEncryption::Unknown);

        Some(WifiInfo {
            ssid_hidden: ssid.is_none(),
            ssid,
            encryption,
            channel: slow.as_ref().and_then(|s| s.channel),
            frequency_mhz: slow.as_ref().and_then(|s| s.frequency_mhz),
            signal_percent: slow.as_ref().and_then(|s| s.signal_percent),
        })
    }

    async fn dns_info(&self, iface: &str) -> Option<DnsResolverInfo> {
        let r = exec_cmd(cmd(&["scutil", "--dns"])).await.ok()?;
        parse_scutil_dns(&r.stdout, iface)
    }

    /// Not implemented on macOS — returns None.
    /// TODO: fall back to `dig +short TXT whoami.cloudflare.com` or a curl DoH call.
    async fn system_egress_ip(&self) -> Option<String> {
        None
    }

    async fn interface_type(&self, iface: &str) -> InterfaceKind {
        if is_vpn_iface(iface) {
            return InterfaceKind::Vpn;
        }
        if let Ok(r) = exec_cmd(cmd(&["networksetup", "-listallhardwareports"])).await
            && is_wifi_hardware_port(&r.stdout, iface)
        {
            return InterfaceKind::WiFi;
        }
        InterfaceKind::Ethernet
    }

    async fn scan_bss_list(&self) -> Option<Vec<BssEntry>> {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Fixture helpers — load real captured output from tests/fixtures/<context>/<file>
    macro_rules! fixture {
        ($context:literal, $file:literal) => {
            include_str!(concat!("../../tests/fixtures/", $context, "/", $file))
        };
    }

    // --- Fixture-based tests: home-wifi-macos (en0, WPA2/DHCP, Google router) ---

    #[test]
    fn fixture_route_get_parses_home_wifi_macos() {
        let raw = fixture!("home-wifi-macos", "route_-n_get_default.txt");
        let result = parse_route_get(raw).unwrap();
        assert_eq!(result.gateway, "192.168.86.1");
        assert_eq!(result.device, "en0");
    }

    #[test]
    fn fixture_ifconfig_parses_home_wifi_macos() {
        let raw = fixture!("home-wifi-macos", "ifconfig_en0.txt");
        let result = parse_ifconfig(raw).unwrap();
        assert_eq!(result.ip, "192.168.86.247");
        assert_eq!(result.prefix, 24);
    }

    #[test]
    fn fixture_arp_parses_home_wifi_macos_excludes_multicast_and_broadcast() {
        let raw = fixture!("home-wifi-macos", "arp_-an_-i_en0.txt");
        let neighbors = parse_arp(raw, "en0", Some("192.168.86.1"));
        // No multicast (1:0:5e:...) or broadcast (ff:ff:...) MACs in results
        for n in &neighbors {
            let mac = n.mac.as_deref().unwrap_or("");
            assert_ne!(mac, "ff:ff:ff:ff:ff:ff", "broadcast should be filtered");
            let first_byte =
                u8::from_str_radix(mac.split(':').next().unwrap_or("0"), 16).unwrap_or(0);
            assert_eq!(first_byte & 1, 0, "multicast MAC should be filtered: {mac}");
        }
        // Gateway must be present
        assert!(neighbors.iter().any(|n| n.is_gateway));
    }

    #[test]
    fn fixture_scutil_dns_parses_home_wifi_macos() {
        let raw = fixture!("home-wifi-macos", "scutil_--dns.txt");
        // Contains a "for scoped queries" section — parser must return the first match
        let result = parse_scutil_dns(raw, "en0").unwrap();
        assert_eq!(result.link, "en0");
        assert_eq!(result.current_server, Some("192.168.86.1".to_string()));
    }

    #[test]
    fn fixture_networksetup_identifies_en0_as_wifi() {
        let raw = fixture!("home-wifi-macos", "networksetup_-listallhardwareports.txt");
        assert!(is_wifi_hardware_port(raw, "en0"));
        assert!(!is_wifi_hardware_port(raw, "en1")); // Thunderbolt 1, not WiFi
    }

    #[test]
    fn parse_route_get_extracts_gateway_and_interface() {
        let raw = "   route to: default\ndestination: default\n       mask: default\n    gateway: 192.168.1.1\n  interface: en0\n      flags: <UP,GATEWAY,DONE,STATIC>";
        let result = parse_route_get(raw).unwrap();
        assert_eq!(result.gateway, "192.168.1.1");
        assert_eq!(result.device, "en0");
    }

    #[test]
    fn parse_route_get_returns_none_for_empty() {
        assert!(parse_route_get("").is_none());
    }

    #[test]
    fn parse_ifconfig_converts_hex_mask_to_prefix() {
        let raw = "en0: flags=8863<UP> mtu 1500\n\tinet 192.168.1.100 netmask 0xffffff00 broadcast 192.168.1.255";
        let result = parse_ifconfig(raw).unwrap();
        assert_eq!(result.ip, "192.168.1.100");
        assert_eq!(result.prefix, 24);
    }

    #[test]
    fn parse_ifconfig_handles_slash16() {
        let raw = "\tinet 10.0.0.50 netmask 0xffff0000 broadcast 10.0.255.255";
        let result = parse_ifconfig(raw).unwrap();
        assert_eq!(result.prefix, 16);
    }

    #[test]
    fn parse_arp_extracts_neighbors_and_flags_gateway() {
        let raw = "? (192.168.1.1) at 68:7f:f0:55:77:7b on en0 ifscope [ethernet]\n? (192.168.1.50) at a4:c3:f0:aa:bb:cc on en0 ifscope [ethernet]\n? (192.168.1.255) at ff:ff:ff:ff:ff:ff on en0 ifscope [ethernet]";
        let neighbors = parse_arp(raw, "en0", Some("192.168.1.1"));
        // broadcast is filtered
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors[0].is_gateway);
        assert_eq!(neighbors[0].vendor, Some("TP-Link".to_string()));
        assert!(!neighbors[1].is_gateway);
    }

    // --- Wi-Fi: ipconfig getsummary + system_profiler (airport was removed
    //     in macOS 15/26). spec: wifi-info-detection ---

    #[test]
    fn fixture_ipconfig_getsummary_redacted_ssid_home_wifi_macos() {
        // spec: wifi-info-detection#S2 — macOS withholds the SSID, encryption survives
        let raw = fixture!("home-wifi-macos", "ipconfig_getsummary_en0.txt");
        let w = parse_ipconfig_getsummary(raw).unwrap();
        assert_eq!(w.ssid, None);
        assert!(w.ssid_hidden);
        assert_eq!(w.encryption, WifiEncryption::Wpa2);
    }

    #[test]
    fn fixture_system_profiler_wifi_home_wifi_macos() {
        // spec: wifi-info-detection#S5 — slow path yields channel + signal
        let raw = fixture!(
            "home-wifi-macos",
            "system_profiler_-json_SPAirPortDataType.json"
        );
        let w = parse_system_profiler_wifi(raw, "en0").unwrap();
        assert_eq!(w.channel, Some(48));
        assert_eq!(w.frequency_mhz, Some(5240)); // 5000 + 48*5
        assert!(w.signal_percent.is_some());
        assert_eq!(w.encryption, WifiEncryption::Wpa2);
        assert_eq!(w.ssid, None); // <redacted>
    }

    #[test]
    fn system_profiler_wifi_wrong_iface_returns_none() {
        let raw = fixture!(
            "home-wifi-macos",
            "system_profiler_-json_SPAirPortDataType.json"
        );
        assert!(parse_system_profiler_wifi(raw, "en7").is_none());
    }

    #[test]
    fn ipconfig_getsummary_ethernet_returns_none() {
        // spec: wifi-info-detection#S3
        let raw = "<dictionary> {\n  InterfaceType : Ethernet\n  LinkStatusActive : TRUE\n}";
        assert!(parse_ipconfig_getsummary(raw).is_none());
    }

    #[test]
    fn ipconfig_getsummary_visible_ssid() {
        // spec: wifi-info-detection#S1
        let raw = "<dictionary> {\n  InterfaceType : WiFi\n  LinkStatusActive : TRUE\n  SSID : CoffeeShop\n  Security : NONE\n}";
        let w = parse_ipconfig_getsummary(raw).unwrap();
        assert_eq!(w.ssid.as_deref(), Some("CoffeeShop"));
        assert!(!w.ssid_hidden);
        assert_eq!(w.encryption, WifiEncryption::Open);
    }

    #[test]
    fn classify_wifi_security_covers_both_formats() {
        assert_eq!(classify_wifi_security("WPA2_PSK"), WifiEncryption::Wpa2);
        assert_eq!(classify_wifi_security("WPA3_SAE"), WifiEncryption::Wpa3);
        assert_eq!(
            classify_wifi_security("802_1X"),
            WifiEncryption::Wpa2Enterprise
        );
        assert_eq!(classify_wifi_security("NONE"), WifiEncryption::Open);
        assert_eq!(
            classify_wifi_security("spairport_security_mode_wpa2_personal"),
            WifiEncryption::Wpa2
        );
        assert_eq!(
            classify_wifi_security("spairport_security_mode_wpa3_personal"),
            WifiEncryption::Wpa3
        );
        assert_eq!(
            classify_wifi_security("spairport_security_mode_wpa2_enterprise"),
            WifiEncryption::Wpa2Enterprise
        );
        assert_eq!(
            classify_wifi_security("spairport_security_mode_none"),
            WifiEncryption::Open
        );
        assert_eq!(classify_wifi_security(""), WifiEncryption::Unknown);
    }

    #[test]
    fn channel_band_to_mhz_by_band() {
        assert_eq!(channel_band_to_mhz(6, "6 (2GHz, 20MHz)"), Some(2437));
        assert_eq!(channel_band_to_mhz(48, "48 (5GHz, 80MHz)"), Some(5240));
        assert_eq!(channel_band_to_mhz(37, "37 (6GHz, 160MHz)"), Some(6135));
    }

    #[test]
    fn parse_scutil_dns_finds_resolver_for_interface() {
        let raw = "DNS configuration\n\nresolver #1\n  nameserver[0] : 192.168.1.1\n  if_index : 6 (en0)\n\nresolver #2\n  nameserver[0] : 8.8.8.8\n  if_index : 3 (utun0)";
        let result = parse_scutil_dns(raw, "en0").unwrap();
        assert_eq!(result.link, "en0");
        assert_eq!(result.current_server, Some("192.168.1.1".to_string()));
        assert_eq!(result.servers, vec!["192.168.1.1".to_string()]);
    }

    #[test]
    fn parse_scutil_dns_no_match_returns_none() {
        let raw = "resolver #1\n  nameserver[0] : 8.8.8.8\n  if_index : 3 (utun0)";
        assert!(parse_scutil_dns(raw, "en0").is_none());
    }
}
