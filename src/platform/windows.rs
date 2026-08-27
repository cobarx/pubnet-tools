//! Windows implementation of PlatformProbe.
//! Every probe shells out to PowerShell's `Get-Net*` / `Get-DnsClient*`
//! cmdlets rendered with `Format-List`. Their property names are English
//! regardless of the Windows display language, so the "Key : Value" text is
//! a stable parse target — `ipconfig` / `route print` localize and aren't.
//! WiFi is the one exception: `netsh wlan show interfaces` has no structured
//! equivalent, and its labels *do* localize (see `parse_netsh_wlan`).

use super::{is_vpn_iface, AddrInfo, PlatformProbe, RouteInfo, WifiInfo};
use crate::exec::{cmd, exec_cmd, ExecResult};
use crate::network::lookup_mac_vendor;
use crate::types::{ArpNeighbor, DnsResolverInfo, DnsSource, InterfaceKind, WifiEncryption};

fn empty() -> ExecResult {
    ExecResult { stdout: String::new(), stderr: String::new(), exit_code: None }
}

/// Wraps a PowerShell one-liner as an argv for `exec_cmd` (no shell, no
/// injection surface — the command string is a single fixed argument).
fn powershell(command: &str) -> Vec<String> {
    cmd(&["powershell", "-NoProfile", "-NonInteractive", "-Command", command])
}

// ---------------------------------------------------------------------------
// Format-List parsing
// ---------------------------------------------------------------------------

/// One `Format-List` record: the `Key : Value` lines between blank lines,
/// keyed by trimmed key. PowerShell pads keys to a common width and emits
/// leading/trailing blank lines; both are tolerated here.
type FlRecord = Vec<(String, String)>;

fn parse_format_list(raw: &str) -> Vec<FlRecord> {
    let mut records = Vec::new();
    let mut current: FlRecord = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            if !current.is_empty() {
                records.push(std::mem::take(&mut current));
            }
            continue;
        }
        // Split on the first ':' — values (IPv6, "{a, b}") may contain more.
        if let Some((key, value)) = line.split_once(':') {
            current.push((key.trim().to_string(), value.trim().to_string()));
        } else {
            // A continuation line (wrapped value). Append to the last value.
            if let Some(last) = current.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
        }
    }
    if !current.is_empty() {
        records.push(current);
    }
    records
}

fn field<'a>(record: &'a FlRecord, key: &str) -> Option<&'a str> {
    record.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

// ---------------------------------------------------------------------------
// Parsers
// ---------------------------------------------------------------------------

/// Parses `Get-NetRoute -DestinationPrefix 0.0.0.0/0 | Format-List`.
/// Takes the lowest-metric record when several default routes exist.
pub fn parse_get_netroute(raw: &str) -> Option<RouteInfo> {
    parse_format_list(raw)
        .into_iter()
        .filter_map(|r| {
            let gateway = field(&r, "NextHop")?.to_string();
            let device = field(&r, "InterfaceAlias")?.to_string();
            let metric = field(&r, "RouteMetric").and_then(|m| m.parse::<i64>().ok()).unwrap_or(i64::MAX);
            // 0.0.0.0 NextHop means "on-link" — not a usable gateway.
            if gateway == "0.0.0.0" {
                return None;
            }
            Some((metric, RouteInfo { gateway, device }))
        })
        .min_by_key(|(metric, _)| *metric)
        .map(|(_, route)| route)
}

/// Parses `Get-NetIPAddress -AddressFamily IPv4 | Format-List` for the
/// address on the given interface.
pub fn parse_get_netipaddress(raw: &str, iface: &str) -> Option<AddrInfo> {
    parse_format_list(raw).into_iter().find_map(|r| {
        if field(&r, "InterfaceAlias") != Some(iface) {
            return None;
        }
        let ip = field(&r, "IPAddress")?.to_string();
        let prefix = field(&r, "PrefixLength")?.parse().ok()?;
        Some(AddrInfo { ip, prefix })
    })
}

/// Parses `Get-NetNeighbor -AddressFamily IPv4 | Format-List`. Filters the
/// same way the macOS `arp` parser does: broadcast and multicast MACs
/// dropped, incomplete entries (no MAC) dropped.
pub fn parse_get_netneighbor(raw: &str, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
    let mut neighbors = Vec::new();
    for r in parse_format_list(raw) {
        if field(&r, "InterfaceAlias") != Some(iface) {
            continue;
        }
        let Some(ip) = field(&r, "IPAddress") else { continue };
        let mac_raw = field(&r, "LinkLayerAddress").unwrap_or("");
        if mac_raw.is_empty() {
            continue;
        }
        // Multicast/group bit in the first octet, or broadcast.
        let first_byte =
            u8::from_str_radix(mac_raw.split(['-', ':']).next().unwrap_or("0"), 16).unwrap_or(0);
        if first_byte & 1 != 0 {
            continue;
        }
        let mac = Some(mac_raw.to_string());
        let state = field(&r, "State").unwrap_or("Unknown").to_string();
        let is_gateway = gateway_ip.is_some_and(|g| g == ip);
        neighbors.push(ArpNeighbor {
            ip: ip.to_string(),
            vendor: lookup_mac_vendor(mac.as_deref()),
            mac,
            state,
            device: iface.to_string(),
            is_gateway,
        });
    }
    neighbors
}

/// Parses `Get-DnsClientServerAddress -AddressFamily IPv4 | Format-List`.
/// `ServerAddresses` renders as `{192.168.1.1, 8.8.8.8}` (or `{}` when unset).
pub fn parse_get_dnsclientserveraddress(raw: &str, iface: &str) -> Option<DnsResolverInfo> {
    parse_format_list(raw).into_iter().find_map(|r| {
        if field(&r, "InterfaceAlias") != Some(iface) {
            return None;
        }
        let servers: Vec<String> = field(&r, "ServerAddresses")?
            .trim_matches(['{', '}'])
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        if servers.is_empty() {
            return None;
        }
        Some(DnsResolverInfo {
            link: iface.to_string(),
            current_server: servers.first().cloned(),
            servers,
            source: DnsSource::ResolvConf,
        })
    })
}

/// Classifies `Get-NetAdapter | Format-List`'s `PhysicalMediaType` for the
/// given interface. "Native 802.11" is WiFi; "802.3" is Ethernet; anything
/// else (Bluetooth PAN, WWAN, virtual) is Other.
pub fn parse_get_netadapter_kind(raw: &str, iface: &str) -> Option<InterfaceKind> {
    parse_format_list(raw).into_iter().find_map(|r| {
        if field(&r, "Name") != Some(iface) {
            return None;
        }
        let media = field(&r, "PhysicalMediaType").unwrap_or("");
        Some(if media.contains("802.11") {
            InterfaceKind::WiFi
        } else if media.contains("802.3") {
            InterfaceKind::Ethernet
        } else {
            InterfaceKind::Other
        })
    })
}

fn classify_netsh_auth(auth: &str) -> WifiEncryption {
    let a = auth.to_lowercase();
    if a.contains("open") || a.is_empty() {
        WifiEncryption::Open
    } else if a.contains("wpa3") {
        WifiEncryption::Wpa3
    } else if a.contains("wpa2") && a.contains("enterprise") {
        WifiEncryption::Wpa2Enterprise
    } else if a.contains("wpa2") {
        WifiEncryption::Wpa2
    } else if a.contains("wpa") {
        WifiEncryption::Wpa
    } else {
        WifiEncryption::Unknown
    }
}

/// Parses `netsh wlan show interfaces`. Unlike the `Get-Net*` cmdlets this
/// output localizes — the labels ("SSID", "Authentication", "Channel",
/// "Signal") are English only on an English-language Windows. A non-English
/// system falls through to `None`, which the security check already treats
/// as "no WiFi info", the same as an Ethernet connection.
///
/// NOTE: no real connected-WiFi capture exists yet — see
/// `tests/fixtures/NEEDED.md`. This parser is written against the documented
/// format and the fixture that *is* captured (wlansvc stopped -> no match).
pub fn parse_netsh_wlan(raw: &str) -> Option<WifiInfo> {
    let mut ssid = None;
    let mut encryption = WifiEncryption::Unknown;
    let mut channel = None;
    let mut signal_percent = None;

    for line in raw.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim();
        let value = value.trim();
        match key {
            // "SSID" but not "BSSID"; exact match avoids the collision.
            "SSID" => ssid = Some(value.to_string()),
            "Authentication" => encryption = classify_netsh_auth(value),
            "Channel" => channel = value.parse().ok(),
            "Signal" => signal_percent = value.trim_end_matches('%').parse().ok(),
            _ => {}
        }
    }

    Some(WifiInfo {
        ssid: ssid?,
        encryption,
        channel,
        frequency_mhz: None,
        signal_percent,
    })
}

// ---------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------

pub struct WindowsProbe;

impl PlatformProbe for WindowsProbe {
    async fn default_route(&self) -> Option<RouteInfo> {
        let r = exec_cmd(powershell(
            "Get-NetRoute -DestinationPrefix 0.0.0.0/0 | Select-Object NextHop,InterfaceAlias,InterfaceIndex,RouteMetric | Format-List",
        ))
        .await
        .ok()?;
        parse_get_netroute(&r.stdout)
    }

    async fn interface_addr(&self, iface: &str) -> Option<AddrInfo> {
        let r = exec_cmd(powershell(
            "Get-NetIPAddress -AddressFamily IPv4 | Select-Object IPAddress,InterfaceAlias,PrefixLength | Format-List",
        ))
        .await
        .ok()?;
        parse_get_netipaddress(&r.stdout, iface)
    }

    async fn arp_neighbors(&self, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
        let r = exec_cmd(powershell(
            "Get-NetNeighbor -AddressFamily IPv4 | Select-Object IPAddress,LinkLayerAddress,State,InterfaceAlias | Format-List",
        ))
        .await
        .unwrap_or_else(|_| empty());
        parse_get_netneighbor(&r.stdout, iface, gateway_ip)
    }

    async fn wifi_info(&self) -> Option<WifiInfo> {
        let r = exec_cmd(cmd(&["netsh", "wlan", "show", "interfaces"])).await.ok()?;
        parse_netsh_wlan(&r.stdout)
    }

    async fn dns_info(&self, iface: &str) -> Option<DnsResolverInfo> {
        let r = exec_cmd(powershell(
            "Get-DnsClientServerAddress -AddressFamily IPv4 | Select-Object InterfaceAlias,InterfaceIndex,ServerAddresses | Format-List",
        ))
        .await
        .ok()?;
        parse_get_dnsclientserveraddress(&r.stdout, iface)
    }

    /// Not implemented on Windows — returns None, same as macOS.
    /// TODO: `Resolve-DnsName -Type TXT whoami.cloudflare.com` uses the
    /// system resolver and would give the egress IP.
    async fn system_egress_ip(&self) -> Option<String> {
        None
    }

    async fn interface_type(&self, iface: &str) -> InterfaceKind {
        if is_vpn_iface(iface) {
            return InterfaceKind::Vpn;
        }
        if let Ok(r) = exec_cmd(powershell(
            "Get-NetAdapter | Select-Object Name,InterfaceDescription,PhysicalMediaType,Status,ifIndex | Format-List",
        ))
        .await
            && let Some(kind) = parse_get_netadapter_kind(&r.stdout, iface)
        {
            return kind;
        }
        InterfaceKind::Ethernet
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! fixture {
        ($context:literal, $file:literal) => {
            include_str!(concat!("../../tests/fixtures/", $context, "/", $file))
        };
    }

    const CTX: &str = "ethernet-vmware-windows";

    // --- Fixture-based tests: ethernet-vmware-windows ---

    #[test]
    fn fixture_get_netroute_parses_ethernet_vmware_windows() {
        let raw = fixture!("ethernet-vmware-windows", "get-netroute_default.txt");
        let route = parse_get_netroute(raw).unwrap();
        assert_eq!(route.gateway, "172.16.228.2");
        assert_eq!(route.device, "Ethernet0");
    }

    #[test]
    fn fixture_get_netipaddress_parses_ethernet_vmware_windows() {
        let raw = fixture!("ethernet-vmware-windows", "get-netipaddress_ipv4.txt");
        let addr = parse_get_netipaddress(raw, "Ethernet0").unwrap();
        assert_eq!(addr.ip, "172.16.228.128");
        assert_eq!(addr.prefix, 24);
        // The loopback record must not be picked up for a real interface.
        assert!(parse_get_netipaddress(raw, "Ethernet1").is_none());
    }

    #[test]
    fn fixture_get_netneighbor_filters_multicast_and_broadcast() {
        let raw = fixture!("ethernet-vmware-windows", "get-netneighbor_ipv4.txt");
        let neighbors = parse_get_netneighbor(raw, "Ethernet0", Some("172.16.228.2"));
        assert!(!neighbors.is_empty());
        for n in &neighbors {
            let mac = n.mac.as_deref().unwrap_or("");
            let first = u8::from_str_radix(mac.split('-').next().unwrap_or("0"), 16).unwrap_or(0);
            assert_eq!(first & 1, 0, "multicast/broadcast MAC leaked: {mac}");
            assert_eq!(n.device, "Ethernet0");
        }
        // The gateway (a real VMware NIC) is present and flagged.
        let gw = neighbors.iter().find(|n| n.ip == "172.16.228.2").unwrap();
        assert!(gw.is_gateway);
        assert_eq!(gw.mac.as_deref(), Some("00-50-56-FE-6E-A0"));
        assert_eq!(gw.state, "Reachable");
        // 00:50:56 (VMware) is not in the curated vendor table -> None, not a guess.
        assert_eq!(gw.vendor, None);
    }

    #[test]
    fn fixture_get_dnsclientserveraddress_parses_ethernet_vmware_windows() {
        let raw = fixture!("ethernet-vmware-windows", "get-dnsclientserveraddress_ipv4.txt");
        let dns = parse_get_dnsclientserveraddress(raw, "Ethernet0").unwrap();
        assert_eq!(dns.servers, vec!["172.16.228.2".to_string()]);
        assert_eq!(dns.current_server, Some("172.16.228.2".to_string()));
        // Loopback has an empty {} server list -> no resolver info.
        assert!(parse_get_dnsclientserveraddress(raw, "Loopback Pseudo-Interface 1").is_none());
    }

    #[test]
    fn fixture_get_netadapter_identifies_ethernet_vmware_windows() {
        let raw = fixture!("ethernet-vmware-windows", "get-netadapter.txt");
        assert_eq!(parse_get_netadapter_kind(raw, "Ethernet0"), Some(InterfaceKind::Ethernet));
        assert_eq!(parse_get_netadapter_kind(raw, "Wi-Fi"), None);
    }

    #[test]
    fn fixture_netsh_wlan_service_not_running_yields_no_wifi() {
        let raw = fixture!("ethernet-vmware-windows", "netsh_wlan_show_interfaces.txt");
        // "The Wireless AutoConfig Service (wlansvc) is not running." — no SSID line.
        assert!(parse_netsh_wlan(raw).is_none());
    }

    #[test]
    #[ignore = "needs: tests/fixtures/wifi-windows/netsh_wlan_show_interfaces.txt — run capture.sh on a Windows machine associated to an AP (see tests/fixtures/NEEDED.md)"]
    fn fixture_netsh_wlan_connected_reports_ssid_and_encryption() {
        // No real connected-WiFi capture exists yet. The synthetic-input tests
        // above pin the parse against Microsoft's documented format; this test
        // stays ignored until a real capture can replace them with exact
        // assertions on a known network.
    }

    // Silence dead-code warning for CTX while it documents the fixture dir.
    #[test]
    fn fixture_context_name_is_stable() {
        assert_eq!(CTX, "ethernet-vmware-windows");
    }

    // --- Synthetic-input parser unit tests (labels/format from Microsoft docs) ---

    #[test]
    fn get_netroute_prefers_lowest_metric_and_skips_onlink() {
        let raw = "\nNextHop        : 0.0.0.0\nInterfaceAlias : Ethernet0\nRouteMetric    : 5\n\nNextHop        : 10.0.0.1\nInterfaceAlias : Wi-Fi\nRouteMetric    : 25\n\nNextHop        : 10.0.0.254\nInterfaceAlias : Ethernet 2\nRouteMetric    : 10\n";
        let route = parse_get_netroute(raw).unwrap();
        assert_eq!(route.gateway, "10.0.0.254");
        assert_eq!(route.device, "Ethernet 2");
    }

    #[test]
    fn netsh_wlan_wpa2_personal_is_wpa2_not_enterprise() {
        let raw = "    Name                   : Wi-Fi\n    SSID                    : CoffeeShop\n    BSSID                   : aa:bb:cc:dd:ee:ff\n    Authentication         : WPA2-Personal\n    Channel                : 44\n    Signal                 : 78%\n";
        let w = parse_netsh_wlan(raw).unwrap();
        assert_eq!(w.ssid, "CoffeeShop");
        assert_eq!(w.encryption, WifiEncryption::Wpa2);
        assert_eq!(w.channel, Some(44));
        assert_eq!(w.signal_percent, Some(78));
    }

    #[test]
    fn netsh_wlan_open_network() {
        let raw = "    SSID                    : FreeWiFi\n    Authentication         : Open\n    Channel                : 6\n    Signal                 : 40%\n";
        assert_eq!(parse_netsh_wlan(raw).unwrap().encryption, WifiEncryption::Open);
    }

    #[test]
    fn netsh_wlan_wpa3_and_enterprise() {
        let wpa3 = "    SSID : Secure\n    Authentication : WPA3-Personal\n";
        assert_eq!(parse_netsh_wlan(wpa3).unwrap().encryption, WifiEncryption::Wpa3);
        let ent = "    SSID : Corp\n    Authentication : WPA2-Enterprise\n";
        assert_eq!(parse_netsh_wlan(ent).unwrap().encryption, WifiEncryption::Wpa2Enterprise);
    }

    #[test]
    fn netsh_wlan_no_ssid_returns_none() {
        let raw = "    There is no wireless interface on the system.\n";
        assert!(parse_netsh_wlan(raw).is_none());
    }

    #[test]
    fn format_list_handles_crlf_and_blank_padding() {
        let raw = "\r\n\r\nKey1 : a\r\nKey2 : b\r\n\r\nKey1 : c\r\n\r\n\r\n";
        let records = parse_format_list(raw);
        assert_eq!(records.len(), 2);
        assert_eq!(field(&records[0], "Key1"), Some("a"));
        assert_eq!(field(&records[1], "Key1"), Some("c"));
    }
}
