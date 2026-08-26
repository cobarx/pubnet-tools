//! macOS implementation of PlatformProbe.
//! Commands: route, ifconfig, arp, airport, scutil.

use super::{AddrInfo, PlatformProbe, RouteInfo, WifiInfo};
use crate::exec::{cmd, exec_cmd, ExecResult};
use crate::network::lookup_mac_vendor;
use crate::types::{ArpNeighbor, DnsResolverInfo, DnsSource, WifiEncryption};
use regex::Regex;
use std::sync::LazyLock;

const AIRPORT: &str =
    "/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport";

fn empty() -> ExecResult {
    ExecResult { stdout: String::new(), stderr: String::new(), exit_code: None }
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

static IFCONFIG_INET_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*inet (\d+\.\d+\.\d+\.\d+) netmask (0x[0-9a-f]+)").unwrap());

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

static ARP_ENTRY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\((\d+\.\d+\.\d+\.\d+)\) at ([0-9a-f:]+) on (\S+)").unwrap()
});

/// Parses `arp -an -i <iface>`.
pub fn parse_arp(raw: &str, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
    let mut neighbors = Vec::new();
    for line in raw.lines() {
        let Some(caps) = ARP_ENTRY_RE.captures(line) else { continue };
        let ip = caps[1].to_string();
        let mac_str = &caps[2];
        // "ff:ff:ff:ff:ff:ff" is broadcast — skip
        if mac_str == "ff:ff:ff:ff:ff:ff" {
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

fn classify_airport_security(link_auth: &str) -> WifiEncryption {
    match link_auth.trim() {
        "open" => WifiEncryption::Open,
        s if s.contains("wpa3") => WifiEncryption::Wpa3,
        s if s.contains("wpa2") && s.contains("enterprise") => WifiEncryption::Wpa2Enterprise,
        s if s.contains("wpa2") => WifiEncryption::Wpa2,
        s if s.contains("wpa") => WifiEncryption::Wpa,
        _ => WifiEncryption::Unknown,
    }
}

fn rssi_to_percent(rssi: i32) -> u32 {
    ((rssi + 100) * 2).clamp(0, 100) as u32
}

/// Parses `airport -I`.
pub fn parse_airport(raw: &str) -> Option<WifiInfo> {
    let mut ssid = None;
    let mut encryption = WifiEncryption::Unknown;
    let mut channel: Option<u32> = None;
    let mut signal_percent: Option<u32> = None;

    for line in raw.lines() {
        let Some((key, val)) = line.split_once(':') else { continue };
        match key.trim() {
            "SSID" => ssid = Some(val.trim().to_string()),
            "link auth" => encryption = classify_airport_security(val),
            "agrCtlRSSI" => {
                if let Ok(rssi) = val.trim().parse::<i32>() {
                    signal_percent = Some(rssi_to_percent(rssi));
                }
            }
            "channel" => {
                // Format: "6,1" or just "6"
                channel = val.trim().split(',').next().and_then(|s| s.parse().ok());
            }
            _ => {}
        }
    }

    Some(WifiInfo { ssid: ssid?, encryption, channel, frequency_mhz: None, signal_percent })
}

/// Parses `scutil --dns` for the resolver block matching the given interface.
/// nameserver lines appear before if_index in scutil output, so we accumulate
/// each block's servers and interface name together, then check at the block
/// boundary rather than while streaming.
pub fn parse_scutil_dns(raw: &str, iface: &str) -> Option<DnsResolverInfo> {
    let iface_pattern = format!("({iface})");
    let mut block_servers: Vec<String> = Vec::new();
    let mut block_iface: Option<String> = None;

    let flush = |servers: &mut Vec<String>, found_iface: &mut Option<String>| -> Option<DnsResolverInfo> {
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

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

pub struct MacProbe;

impl PlatformProbe for MacProbe {
    async fn default_route(&self) -> Option<RouteInfo> {
        let r = exec_cmd(cmd(&["route", "-n", "get", "default"])).await.ok()?;
        parse_route_get(&r.stdout)
    }

    async fn interface_addr(&self, iface: &str) -> Option<AddrInfo> {
        let r = exec_cmd(cmd(&["ifconfig", iface])).await.ok()?;
        parse_ifconfig(&r.stdout)
    }

    async fn arp_neighbors(&self, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
        let r = exec_cmd(cmd(&["arp", "-an", "-i", iface])).await.unwrap_or_else(|_| empty());
        parse_arp(&r.stdout, iface, gateway_ip)
    }

    async fn wifi_info(&self) -> Option<WifiInfo> {
        let r = exec_cmd(cmd(&[AIRPORT, "-I"])).await.ok()?;
        parse_airport(&r.stdout)
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
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn parse_airport_extracts_ssid_encryption_channel_signal() {
        let raw = "     agrCtlRSSI: -60\n          state: running\n      link auth: wpa2-psk\n           SSID: MyNetwork\n        channel: 6,1";
        let result = parse_airport(raw).unwrap();
        assert_eq!(result.ssid, "MyNetwork");
        assert_eq!(result.encryption, WifiEncryption::Wpa2);
        assert_eq!(result.channel, Some(6));
        assert_eq!(result.signal_percent, Some(80)); // (-60 + 100) * 2 = 80
    }

    #[test]
    fn parse_airport_open_network() {
        let raw = "      link auth: open\n           SSID: CoffeeShop\n        channel: 1";
        let result = parse_airport(raw).unwrap();
        assert_eq!(result.encryption, WifiEncryption::Open);
    }

    #[test]
    fn parse_airport_wpa3() {
        let raw = "      link auth: wpa3-sae\n           SSID: SecureNet\n        channel: 36";
        let result = parse_airport(raw).unwrap();
        assert_eq!(result.encryption, WifiEncryption::Wpa3);
    }

    #[test]
    fn parse_airport_no_ssid_returns_none() {
        let raw = "      link auth: wpa2-psk\n        channel: 6";
        assert!(parse_airport(raw).is_none());
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
