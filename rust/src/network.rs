//! Port of src/utils/network.ts: pure synchronous parsers for shell output
//! and small classification helpers. No I/O — every function here takes
//! already-captured text and returns structured data.

use crate::types::{ArpNeighbor, DnsResolverInfo, DnsSource, WifiEncryption};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq)]
pub struct NmcliWifiResult {
    pub ssid: String,
    pub encryption: WifiEncryption,
    pub channel: Option<u32>,
    pub frequency_mhz: Option<u32>,
    pub signal_percent: Option<u32>,
}

fn classify_security(security: &str) -> WifiEncryption {
    if security.is_empty() {
        WifiEncryption::Open
    } else if security.contains("802.1X") {
        WifiEncryption::Wpa2Enterprise
    } else if security.contains("WPA3") {
        WifiEncryption::Wpa3
    } else if security.contains("WPA2") {
        WifiEncryption::Wpa2
    } else if security.contains("WPA") {
        WifiEncryption::Wpa
    } else {
        WifiEncryption::Unknown
    }
}

/// nmcli's terse (`-t`) output backslash-escapes ':' and '\' within field
/// values, so a colon in an SSID doesn't collide with the field delimiter.
/// Splits on unescaped colons only, then unescapes each field.
fn split_terse_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                current.push(next);
            }
        } else if ch == ':' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }
    fields.push(current);
    fields
}

fn parse_u32_or_none(value: Option<&str>) -> Option<u32> {
    value.and_then(|v| {
        let digits: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() { None } else { digits.parse().ok() }
    })
}

pub fn parse_nmcli_wifi(raw: &str) -> Option<NmcliWifiResult> {
    for line in raw.lines() {
        let fields = split_terse_fields(line);
        if fields.len() < 3 || fields[0] != "yes" {
            continue;
        }
        return Some(NmcliWifiResult {
            ssid: fields[1].clone(),
            encryption: classify_security(&fields[2]),
            channel: parse_u32_or_none(fields.get(3).map(String::as_str)),
            frequency_mhz: parse_u32_or_none(fields.get(4).map(String::as_str)),
            signal_percent: parse_u32_or_none(fields.get(5).map(String::as_str)),
        });
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct IpRouteResult {
    pub gateway: String,
    pub device: String,
}

static IP_ROUTE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^default via (\S+) dev (\S+)").unwrap());

pub fn parse_ip_route(raw: &str) -> Option<IpRouteResult> {
    let caps = IP_ROUTE_RE.captures(raw)?;
    Some(IpRouteResult {
        gateway: caps[1].to_string(),
        device: caps[2].to_string(),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct IpAddrResult {
    pub ip: String,
    pub prefix: u32,
}

static IP_ADDR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^\s*inet (\d+\.\d+\.\d+\.\d+)/(\d+)").unwrap());

pub fn parse_ip_addr(raw: &str) -> Option<IpAddrResult> {
    let caps = IP_ADDR_RE.captures(raw)?;
    Some(IpAddrResult {
        ip: caps[1].to_string(),
        prefix: caps[2].parse().ok()?,
    })
}

/// OUI (first 3 octets of a MAC) to vendor name. A curated subset of
/// consumer/SOHO networking and smart-home equipment vendors — not the
/// full ~30k-entry IEEE registry, which is mostly irrelevant hardware
/// that would never turn up as a home gateway or ARP neighbor. Every
/// prefix here is verified against the real IEEE OUI registry
/// (standards-oui.ieee.org/oui/oui.txt), not invented.
const OUI_VENDORS: &[(&str, &str)] = &[
    ("687FF0", "TP-Link"),
    ("34F716", "TP-Link"),
    ("54A703", "TP-Link"),
    ("B0BE76", "TP-Link"),
    ("405D82", "NETGEAR"),
    ("DCEF09", "NETGEAR"),
    ("100C6B", "NETGEAR"),
    ("002618", "ASUSTek"),
    ("049226", "ASUSTek"),
    ("1831BF", "ASUSTek"),
    ("BC2228", "D-Link"),
    ("A0A3F0", "D-Link"),
    ("BC0F9A", "D-Link"),
    ("001D7E", "Cisco-Linksys"),
    ("0014BF", "Cisco-Linksys"),
    ("48F8B3", "Cisco-Linksys"),
    ("D8EC5E", "Belkin"),
    ("E89F80", "Belkin"),
    ("58EF68", "Belkin"),
    ("F09FC2", "Ubiquiti"),
    ("802AA8", "Ubiquiti"),
    ("788A20", "Ubiquiti"),
    ("E80AB9", "Cisco Systems"),
    ("481BA4", "Cisco Systems"),
    ("6C03B5", "Cisco Systems"),
    ("0015D1", "CommScope"),
    ("2C301A", "Technicolor"),
    ("FC2BB2", "Actiontec"),
    ("A0A3E2", "Actiontec"),
    ("5016F4", "Motorola Mobility"),
    ("C4A052", "Motorola Mobility"),
    ("6070C6", "Google"),
    ("C82ADD", "Google"),
    ("242934", "Google"),
    ("842859", "Amazon"),
    ("2873F6", "Amazon"),
    ("E0CB1D", "Amazon"),
    ("F0EE7A", "Apple"),
    ("58AD12", "Apple"),
    ("60FDA6", "Apple"),
    ("E00630", "Huawei"),
    ("D8DAF1", "Huawei"),
    ("581DD8", "Sagemcom"),
    ("C03C04", "Sagemcom"),
    ("F80DA9", "Zyxel"),
    ("88ACC0", "Zyxel"),
    ("08F01E", "eero"),
    ("98ED7E", "eero"),
    ("80DA13", "eero"),
    ("00043C", "Sonos"),
    ("7828CA", "Sonos"),
    ("085531", "MikroTik"),
    ("B869F4", "MikroTik"),
    ("000C42", "MikroTik"),
];

pub fn lookup_mac_vendor(mac: Option<&str>) -> Option<String> {
    let mac = mac?;
    let prefix: String = mac
        .chars()
        .filter(|c| *c != ':' && *c != '-')
        .collect::<String>()
        .to_uppercase()
        .chars()
        .take(6)
        .collect();
    OUI_VENDORS
        .iter()
        .find(|(k, _)| *k == prefix)
        .map(|(_, v)| v.to_string())
}

pub fn parse_ip_neigh(raw: &str, device: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
    let mut neighbors = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let Some(ip) = parts.first() else { continue };
        let mac = parts
            .iter()
            .position(|p| *p == "lladdr")
            .and_then(|i| parts.get(i + 1))
            .map(|s| s.to_string());
        let state = parts.last().unwrap_or(&"UNKNOWN").to_string();
        let is_gateway = gateway_ip.is_some_and(|g| g == *ip);
        neighbors.push(ArpNeighbor {
            ip: ip.to_string(),
            vendor: lookup_mac_vendor(mac.as_deref()),
            mac,
            state,
            device: device.to_string(),
            is_gateway,
        });
    }
    neighbors
}

#[derive(Debug, Clone, PartialEq)]
pub struct PingSummary {
    pub transmitted: u32,
    pub received: u32,
    pub rtts: Vec<f64>,
}

static PING_TIME_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"time=([\d.]+) ms").unwrap());
static PING_SUMMARY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\d+) packets transmitted, (\d+) received").unwrap());

pub fn parse_ping_output(raw: &str) -> PingSummary {
    let rtts: Vec<f64> = PING_TIME_RE
        .captures_iter(raw)
        .filter_map(|c| c[1].parse().ok())
        .collect();

    let (transmitted, received) = PING_SUMMARY_RE
        .captures(raw)
        .map(|c| {
            (
                c[1].parse().unwrap_or(0),
                c[2].parse().unwrap_or(0),
            )
        })
        .unwrap_or((0, 0));

    PingSummary { transmitted, received, rtts }
}

pub fn parse_resolvectl_status(raw: &str, iface: &str) -> Option<DnsResolverInfo> {
    let header_re = Regex::new(&format!(r"(?m)^Link \d+ \({}\)$", regex::escape(iface))).ok()?;
    let header_match = header_re.find(raw)?;

    let block_start = header_match.end();
    let rest = &raw[block_start..];
    let next_header_re = Regex::new(r"(?m)^Link \d+ \(").unwrap();
    let block_end = next_header_re
        .find(rest)
        .map(|m| block_start + m.start())
        .unwrap_or(raw.len());
    let block = &raw[block_start..block_end];

    let current_server_re = Regex::new(r"(?m)^Current DNS Server: (\S+)").unwrap();
    let servers_re = Regex::new(r"(?m)^\s*DNS Servers: (.+)$").unwrap();

    let current_server = current_server_re.captures(block).map(|c| c[1].to_string());
    let servers = servers_re
        .captures(block)
        .map(|c| c[1].split_whitespace().map(String::from).collect())
        .unwrap_or_default();

    Some(DnsResolverInfo {
        link: iface.to_string(),
        current_server,
        servers,
        source: DnsSource::Resolvectl,
    })
}

pub fn stddev(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    variance.sqrt()
}

static IPV4_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$").unwrap());

pub fn is_valid_ipv4(s: &str) -> bool {
    match IPV4_RE.captures(s) {
        Some(caps) => (1..=4).all(|i| caps[i].parse::<u32>().is_ok_and(|n| n <= 255)),
        None => false,
    }
}

/// Extracts a `remote_ip` value from raw response text — tolerant of
/// resolvectl's quoted TXT line, Google DoH's unquoted `data` field, and
/// Cloudflare DoH's escaped-quote `data` field, without needing to parse
/// JSON: the surrounding punctuation just isn't part of the character class.
static REMOTE_IP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"remote_ip:\s*([0-9a-fA-F.:]+)").unwrap());

pub fn extract_remote_ip(raw: &str) -> Option<String> {
    REMOTE_IP_RE.captures(raw).map(|c| c[1].to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpFamily {
    V4,
    V6,
}

pub fn ip_family(ip: &str) -> IpFamily {
    if is_valid_ipv4(ip) { IpFamily::V4 } else { IpFamily::V6 }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_nmcli_wifi ---

    #[test]
    fn classifies_active_row_with_wpa3_and_real_fields() {
        let raw = [
            "no:Patels:WPA2 WPA3:8:2447 MHz:67",
            "no::WPA2:11:2462 MHz:70",
            "yes:ABVI_Dunnigan_Guest:WPA3:117:6535 MHz:50",
            "no:Super8_Admin:WPA2 WPA3:11:2462 MHz:69",
        ]
        .join("\n");
        let result = parse_nmcli_wifi(&raw).unwrap();
        assert_eq!(result.ssid, "ABVI_Dunnigan_Guest");
        assert_eq!(result.encryption, WifiEncryption::Wpa3);
        assert_eq!(result.channel, Some(117));
        assert_eq!(result.frequency_mhz, Some(6535));
        assert_eq!(result.signal_percent, Some(50));
    }

    #[test]
    fn classifies_plain_wpa2_active_row() {
        let result = parse_nmcli_wifi("yes:CorpNet:WPA2:6:2437 MHz:80").unwrap();
        assert_eq!(result.ssid, "CorpNet");
        assert_eq!(result.encryption, WifiEncryption::Wpa2);
        assert_eq!(result.channel, Some(6));
    }

    #[test]
    fn wpa2_wpa3_transition_classifies_as_wpa3() {
        let result = parse_nmcli_wifi("yes:Transition:WPA2 WPA3:6:2437 MHz:80").unwrap();
        assert_eq!(result.encryption, WifiEncryption::Wpa3);
    }

    #[test]
    fn empty_security_field_means_open() {
        let result = parse_nmcli_wifi("yes:Berkeley-Visitor::6:2437 MHz:80").unwrap();
        assert_eq!(result.encryption, WifiEncryption::Open);
    }

    #[test]
    fn dot1x_suffix_means_wpa2_enterprise() {
        let result = parse_nmcli_wifi("yes:CorpSecure:WPA2 802.1X:6:2437 MHz:80").unwrap();
        assert_eq!(result.encryption, WifiEncryption::Wpa2Enterprise);
    }

    #[test]
    fn ssid_containing_colon_is_unescaped() {
        let raw = r"yes:Cafe\: Downtown:WPA2:6:2437 MHz:80";
        let result = parse_nmcli_wifi(raw).unwrap();
        assert_eq!(result.ssid, "Cafe: Downtown");
        assert_eq!(result.encryption, WifiEncryption::Wpa2);
    }

    #[test]
    fn missing_numeric_fields_are_none() {
        let result = parse_nmcli_wifi("yes:NoSignalData:WPA2").unwrap();
        assert_eq!(result.channel, None);
        assert_eq!(result.frequency_mhz, None);
        assert_eq!(result.signal_percent, None);
    }

    #[test]
    fn no_active_row_returns_none() {
        let raw = "no:Patels:WPA2 WPA3\nno::WPA2";
        assert!(parse_nmcli_wifi(raw).is_none());
    }

    // --- parse_ip_route ---

    #[test]
    fn extracts_gateway_and_device_from_real_route_line() {
        let raw = "default via 192.168.5.1 dev wlan0 proto dhcp src 192.168.5.151 metric 600 \n";
        let result = parse_ip_route(raw).unwrap();
        assert_eq!(result.gateway, "192.168.5.1");
        assert_eq!(result.device, "wlan0");
    }

    #[test]
    fn no_default_route_returns_none() {
        assert!(parse_ip_route("").is_none());
    }

    // --- parse_ip_addr ---

    #[test]
    fn extracts_ipv4_address_and_prefix_ignoring_inet6() {
        let raw = "2: wlan0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500\n    inet 192.168.5.151/24 brd 192.168.5.255 scope global dynamic noprefixroute wlan0\n    inet6 fe80::f3a:aa70:6a12:23a8/64 scope link noprefixroute";
        let result = parse_ip_addr(raw).unwrap();
        assert_eq!(result.ip, "192.168.5.151");
        assert_eq!(result.prefix, 24);
    }

    #[test]
    fn no_inet_line_returns_none() {
        assert!(parse_ip_addr("2: wlan0: <BROADCAST> mtu 1500").is_none());
    }

    // --- lookup_mac_vendor ---

    #[test]
    fn known_vendor_prefixes_resolve_correctly() {
        // Real gateway MAC seen on this dev machine, verified against the
        // real IEEE OUI registry — see network.ts's port of this test.
        assert_eq!(lookup_mac_vendor(Some("68:7f:f0:55:77:7b")), Some("TP-Link".to_string()));
        assert_eq!(lookup_mac_vendor(Some("40:5d:82:aa:bb:cc")), Some("NETGEAR".to_string()));
        assert_eq!(lookup_mac_vendor(Some("f0:ee:7a:aa:bb:cc")), Some("Apple".to_string()));
        assert_eq!(lookup_mac_vendor(Some("08:55:31:aa:bb:cc")), Some("MikroTik".to_string()));
    }

    #[test]
    fn unrecognized_prefix_returns_none() {
        assert_eq!(lookup_mac_vendor(Some("02:00:00:aa:bb:cc")), None);
    }

    #[test]
    fn none_mac_returns_none() {
        assert_eq!(lookup_mac_vendor(None), None);
    }

    #[test]
    fn is_case_and_separator_insensitive() {
        assert_eq!(lookup_mac_vendor(Some("68-7F-F0-55-77-7B")), Some("TP-Link".to_string()));
    }

    // --- parse_ip_neigh ---

    #[test]
    fn parses_neighbors_and_flags_gateway() {
        let raw = "192.168.5.1 lladdr 68:7f:f0:55:77:7b REACHABLE \n192.168.5.60 lladdr 68:72:c3:87:16:66 STALE ";
        let neighbors = parse_ip_neigh(raw, "wlan0", Some("192.168.5.1"));
        assert_eq!(neighbors.len(), 2);
        assert_eq!(neighbors[0].ip, "192.168.5.1");
        assert!(neighbors[0].is_gateway);
        assert_eq!(neighbors[0].vendor, Some("TP-Link".to_string()));
        assert_eq!(neighbors[1].ip, "192.168.5.60");
        assert!(!neighbors[1].is_gateway);
        // Samsung — real vendor, not in the curated networking-equipment
        // table, so this exercises "known MAC, unrecognized vendor": None,
        // not a guess.
        assert_eq!(neighbors[1].vendor, None);
    }

    #[test]
    fn incomplete_entry_has_none_mac_and_vendor() {
        let neighbors = parse_ip_neigh("192.168.5.99 INCOMPLETE ", "wlan0", Some("192.168.5.1"));
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].mac, None);
        assert_eq!(neighbors[0].vendor, None);
        assert_eq!(neighbors[0].state, "INCOMPLETE");
    }

    #[test]
    fn empty_arp_cache_returns_empty_vec() {
        assert!(parse_ip_neigh("", "wlan0", Some("192.168.5.1")).is_empty());
    }

    // --- parse_ping_output ---

    #[test]
    fn parses_per_packet_rtts_and_summary() {
        let raw = "PING 1.1.1.1 (1.1.1.1) 56(84) bytes of data.\n64 bytes from 1.1.1.1: icmp_seq=1 ttl=56 time=15.4 ms\n64 bytes from 1.1.1.1: icmp_seq=2 ttl=56 time=9.69 ms\n64 bytes from 1.1.1.1: icmp_seq=3 ttl=56 time=20.9 ms\n\n--- 1.1.1.1 ping statistics ---\n3 packets transmitted, 3 received, 0% packet loss, time 401ms\nrtt min/avg/max/mdev = 9.685/15.348/20.918/4.586 ms";
        let result = parse_ping_output(raw);
        assert_eq!(result.transmitted, 3);
        assert_eq!(result.received, 3);
        assert_eq!(result.rtts, vec![15.4, 9.69, 20.9]);
    }

    #[test]
    fn hundred_percent_loss_reports_zero_received_no_rtts() {
        let raw = "PING 10.0.0.99 (10.0.0.99) 56(84) bytes of data.\n\n--- 10.0.0.99 ping statistics ---\n10 packets transmitted, 0 received, 100% packet loss, time 2049ms";
        let result = parse_ping_output(raw);
        assert_eq!(result.transmitted, 10);
        assert_eq!(result.received, 0);
        assert!(result.rtts.is_empty());
    }

    // --- parse_resolvectl_status ---

    fn resolvectl_fixture() -> &'static str {
        "Global\n           Protocols: +LLMNR +mDNS -DNSOverTLS DNSSEC=no/unsupported\n    resolv.conf mode: foreign\nFallback DNS Servers: 9.9.9.9#dns.quad9.net 2620:fe::9#dns.quad9.net\n                      1.1.1.1#cloudflare-dns.com\n\nLink 2 (wlan0)\n    Current Scopes: DNS LLMNR/IPv4 LLMNR/IPv6 mDNS/IPv4 mDNS/IPv6\n         Protocols: +DefaultRoute +LLMNR +mDNS -DNSOverTLS DNSSEC=no/unsupported\nCurrent DNS Server: 192.168.5.1\n       DNS Servers: 192.168.5.1\n     Default Route: yes\n\nLink 4 (vmnet8)\n    Current Scopes: LLMNR/IPv4 LLMNR/IPv6 mDNS/IPv4 mDNS/IPv6\n         Protocols: -DefaultRoute +LLMNR +mDNS -DNSOverTLS DNSSEC=no/unsupported\n     Default Route: no"
    }

    #[test]
    fn parses_active_link_block_ignoring_fallback_servers() {
        let result = parse_resolvectl_status(resolvectl_fixture(), "wlan0").unwrap();
        assert_eq!(result.link, "wlan0");
        assert_eq!(result.current_server, Some("192.168.5.1".to_string()));
        assert_eq!(result.servers, vec!["192.168.5.1".to_string()]);
    }

    #[test]
    fn no_matching_link_block_returns_none() {
        assert!(parse_resolvectl_status(resolvectl_fixture(), "eth9").is_none());
    }

    // --- stddev ---

    #[test]
    fn population_stddev_of_known_set() {
        let result = stddev(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]);
        assert!((result - 2.0).abs() < 1e-5);
    }

    #[test]
    fn single_value_has_zero_deviation() {
        assert_eq!(stddev(&[42.0]), 0.0);
    }

    #[test]
    fn empty_slice_has_zero_deviation() {
        assert_eq!(stddev(&[]), 0.0);
    }

    // --- is_valid_ipv4 / ip_family ---

    #[test]
    fn validates_ipv4_addresses() {
        assert!(is_valid_ipv4("1.1.1.1"));
        assert!(is_valid_ipv4("192.168.5.151"));
        assert!(is_valid_ipv4("255.255.255.255"));
        assert!(is_valid_ipv4("0.0.0.0"));
        assert!(!is_valid_ipv4("256.1.1.1"));
        assert!(!is_valid_ipv4("1.1.1"));
        assert!(!is_valid_ipv4("1.1.1.1.1"));
        assert!(!is_valid_ipv4("not-an-ip"));
        assert!(!is_valid_ipv4(""));
    }

    #[test]
    fn classifies_ip_family() {
        assert_eq!(ip_family("1.1.1.1"), IpFamily::V4);
        assert_eq!(ip_family("192.168.5.151"), IpFamily::V4);
        assert_eq!(ip_family("2607:f8b0:4004:1001::12e"), IpFamily::V6);
        assert_eq!(ip_family("::1"), IpFamily::V6);
    }

    // --- extract_remote_ip ---

    #[test]
    fn extracts_ipv6_remote_ip_from_resolvectl_txt() {
        let raw = "whoami.cloudflare.com IN TXT \"remote_ip: 2607:f8b0:4004:1001::12e\" -- link: wlan0";
        assert_eq!(extract_remote_ip(raw), Some("2607:f8b0:4004:1001::12e".to_string()));
    }

    #[test]
    fn extracts_ipv4_remote_ip_from_cloudflare_doh_json_escaped_quotes() {
        let raw = r#"{"Answer":[{"data":"\"asn: 13335\""},{"data":"\"remote_ip: 162.159.0.0\""}]}"#;
        assert_eq!(extract_remote_ip(raw), Some("162.159.0.0".to_string()));
    }

    #[test]
    fn extracts_ipv6_remote_ip_from_google_doh_json_no_quoting() {
        let raw = r#"{"Answer":[{"data":"remote_ip: 2607:f8b0:4004:1009::12c"}]}"#;
        assert_eq!(extract_remote_ip(raw), Some("2607:f8b0:4004:1009::12c".to_string()));
    }

    #[test]
    fn no_remote_ip_field_returns_none() {
        assert_eq!(extract_remote_ip(r#"{"Answer":[{"data":"asn: 13335"}]}"#), None);
    }
}
