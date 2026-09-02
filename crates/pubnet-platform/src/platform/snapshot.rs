//! Data-driven `PlatformProbe`: answers every probe method from a `HostSnapshot`
//! of pre-gathered facts instead of running commands. The mechanism behind the
//! Android front-end (a phone app cannot shell out to `ip`/`nmcli`/`resolvectl`),
//! but it is not Android-specific — it is a pure data → `PlatformProbe` adapter,
//! also handy as a test seam.
//!
//! spec: docs/specs/android-host-snapshot.md

use super::{AddrInfo, PlatformProbe, RouteInfo, WifiInfo};
use crate::network::lookup_mac_vendor;
use crate::types::{ArpNeighbor, BssEntry, DnsResolverInfo, DnsSource, InterfaceKind, WifiEncryption};
use serde::Deserialize;

/// One-shot capture of the active network's facts. Deserialized from JSON
/// (camelCase) supplied by the caller. Every sub-object is optional: a caller
/// that could not read a fact leaves it `null`, and the checks downstream
/// already tolerate the corresponding `None` / empty result.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSnapshot {
    #[serde(default)]
    pub default_route: Option<SnapshotRoute>,
    #[serde(default)]
    pub interface_addr: Option<SnapshotAddr>,
    #[serde(default)]
    pub arp_neighbors: Vec<SnapshotNeighbor>,
    #[serde(default)]
    pub wifi: Option<SnapshotWifi>,
    #[serde(default)]
    pub dns: Option<SnapshotDns>,
    #[serde(default = "default_interface_kind")]
    pub interface_kind: InterfaceKind,
}

fn default_interface_kind() -> InterfaceKind {
    InterfaceKind::Other
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRoute {
    pub gateway: String,
    pub device: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotAddr {
    pub ip: String,
    pub prefix: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotNeighbor {
    pub ip: String,
    #[serde(default)]
    pub mac: Option<String>,
    #[serde(default)]
    pub is_gateway: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotWifi {
    #[serde(default)]
    pub ssid: Option<String>,
    #[serde(default)]
    pub ssid_hidden: bool,
    #[serde(default = "unknown_encryption")]
    pub encryption: WifiEncryption,
    #[serde(default)]
    pub channel: Option<u32>,
    #[serde(default)]
    pub frequency_mhz: Option<u32>,
    #[serde(default)]
    pub signal_percent: Option<u32>,
}

fn unknown_encryption() -> WifiEncryption {
    WifiEncryption::Unknown
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDns {
    #[serde(default)]
    pub servers: Vec<String>,
    #[serde(default)]
    pub current_server: Option<String>,
}

/// `PlatformProbe` backed by a `HostSnapshot`. Construct with
/// `SnapshotProbe::new(snapshot)` or `HostSnapshot::into()`.
pub struct SnapshotProbe {
    snapshot: HostSnapshot,
}

impl SnapshotProbe {
    pub fn new(snapshot: HostSnapshot) -> Self {
        Self { snapshot }
    }
}

impl From<HostSnapshot> for SnapshotProbe {
    fn from(snapshot: HostSnapshot) -> Self {
        Self::new(snapshot)
    }
}

impl PlatformProbe for SnapshotProbe {
    async fn default_route(&self) -> Option<RouteInfo> {
        self.snapshot.default_route.as_ref().map(|r| RouteInfo {
            gateway: r.gateway.clone(),
            device: r.device.clone(),
        })
    }

    async fn interface_addr(&self, _iface: &str) -> Option<AddrInfo> {
        self.snapshot.interface_addr.as_ref().map(|a| AddrInfo {
            ip: a.ip.clone(),
            prefix: a.prefix,
        })
    }

    async fn arp_neighbors(&self, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor> {
        self.snapshot
            .arp_neighbors
            .iter()
            .map(|n| {
                // Trust the caller's `is_gateway`, but also honour a gateway_ip
                // match so callers that don't set the flag still get it right.
                let is_gateway = n.is_gateway || gateway_ip.is_some_and(|g| g == n.ip);
                ArpNeighbor {
                    ip: n.ip.clone(),
                    vendor: lookup_mac_vendor(n.mac.as_deref()),
                    mac: n.mac.clone(),
                    state: "REACHABLE".to_string(),
                    device: iface.to_string(),
                    is_gateway,
                }
            })
            .collect()
    }

    async fn wifi_info(&self, _iface: &str, _detail: bool) -> Option<WifiInfo> {
        self.snapshot.wifi.as_ref().map(|w| WifiInfo {
            ssid: w.ssid.clone(),
            ssid_hidden: w.ssid_hidden,
            encryption: w.encryption,
            channel: w.channel,
            frequency_mhz: w.frequency_mhz,
            signal_percent: w.signal_percent,
        })
    }

    async fn dns_info(&self, iface: &str) -> Option<DnsResolverInfo> {
        self.snapshot.dns.as_ref().map(|d| DnsResolverInfo {
            link: iface.to_string(),
            current_server: d.current_server.clone(),
            servers: d.servers.clone(),
            source: DnsSource::ResolvConf,
        })
    }

    /// Android has no way to observe the resolver's egress IP, so the DNS-leak
    /// verdict is `uncertain` rather than `clean` / `leaked` (same as
    /// macOS/Windows).
    async fn system_egress_ip(&self) -> Option<String> {
        None
    }

    async fn interface_type(&self, _iface: &str) -> InterfaceKind {
        self.snapshot.interface_kind
    }

    /// BSS scanning is `pubnetdiag`'s job (Windows-only); a snapshot never
    /// carries a scan.
    async fn scan_bss_list(&self) -> Option<Vec<BssEntry>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(json: &str) -> SnapshotProbe {
        let snapshot: HostSnapshot = serde_json::from_str(json).expect("snapshot JSON parses");
        SnapshotProbe::new(snapshot)
    }

    const FULL: &str = r#"{
        "defaultRoute":  { "gateway": "192.168.1.1", "device": "wlan0" },
        "interfaceAddr": { "ip": "192.168.1.34", "prefix": 24 },
        "arpNeighbors": [
            { "ip": "192.168.1.1", "mac": "a4:2b:b0:11:22:33", "isGateway": true },
            { "ip": "192.168.1.50", "mac": "b8:27:eb:44:55:66" }
        ],
        "wifi": {
            "ssid": "CoffeeWiFi",
            "ssidHidden": false,
            "encryption": "WPA2",
            "channel": 6,
            "frequencyMhz": 2437,
            "signalPercent": 72
        },
        "dns": { "servers": ["192.168.1.1"], "currentServer": "192.168.1.1" },
        "interfaceKind": "wifi"
    }"#;

    // spec: android-host-snapshot#S1
    #[tokio::test]
    async fn full_snapshot_on_wifi() {
        let p = probe(FULL);

        let route = p.default_route().await.expect("route");
        assert_eq!(route.gateway, "192.168.1.1");
        assert_eq!(route.device, "wlan0");

        let addr = p.interface_addr("wlan0").await.expect("addr");
        assert_eq!(addr.ip, "192.168.1.34");
        assert_eq!(addr.prefix, 24);

        let neighbors = p.arp_neighbors("wlan0", Some("192.168.1.1")).await;
        assert_eq!(neighbors.len(), 2);
        let gw = neighbors.iter().find(|n| n.is_gateway).expect("gateway neighbor");
        assert_eq!(gw.ip, "192.168.1.1");
        assert_eq!(gw.mac.as_deref(), Some("a4:2b:b0:11:22:33"));
        assert_eq!(gw.device, "wlan0");
        assert!(!neighbors[1].is_gateway);

        let wifi = p.wifi_info("wlan0", false).await.expect("wifi");
        assert_eq!(wifi.ssid.as_deref(), Some("CoffeeWiFi"));
        assert!(!wifi.ssid_hidden);
        assert_eq!(wifi.encryption, WifiEncryption::Wpa2);
        assert_eq!(wifi.channel, Some(6));

        let dns = p.dns_info("wlan0").await.expect("dns");
        assert_eq!(dns.link, "wlan0");
        assert_eq!(dns.servers, vec!["192.168.1.1".to_string()]);
        assert_eq!(dns.current_server.as_deref(), Some("192.168.1.1"));

        assert_eq!(p.interface_type("wlan0").await, InterfaceKind::WiFi);
        assert_eq!(p.system_egress_ip().await, None);
        assert!(p.scan_bss_list().await.is_none());
    }

    // spec: android-host-snapshot#S2
    #[tokio::test]
    async fn redacted_ssid_keeps_encryption() {
        let p = probe(
            r#"{ "wifi": { "ssid": null, "ssidHidden": true, "encryption": "WPA2" },
                 "interfaceKind": "wifi" }"#,
        );
        let wifi = p.wifi_info("wlan0", false).await.expect("wifi");
        assert_eq!(wifi.ssid, None);
        assert!(wifi.ssid_hidden);
        assert_eq!(wifi.encryption, WifiEncryption::Wpa2);
    }

    // spec: android-host-snapshot#S3
    #[tokio::test]
    async fn not_on_wifi_has_no_wifi_info() {
        let p = probe(r#"{ "wifi": null, "interfaceKind": "ethernet" }"#);
        assert!(p.wifi_info("eth0", true).await.is_none());
        assert_eq!(p.interface_type("eth0").await, InterfaceKind::Ethernet);

        let vpn = probe(r#"{ "interfaceKind": "vpn" }"#);
        assert_eq!(vpn.interface_type("tun0").await, InterfaceKind::Vpn);
    }

    // spec: android-host-snapshot#S4
    #[tokio::test]
    async fn no_default_route() {
        let p = probe(r#"{ "defaultRoute": null, "interfaceKind": "other" }"#);
        assert!(p.default_route().await.is_none());
    }

    // spec: android-host-snapshot#S5
    #[tokio::test]
    async fn empty_arp_cache() {
        let p = probe(
            r#"{ "defaultRoute": { "gateway": "10.0.0.1", "device": "wlan0" },
                 "interfaceAddr": { "ip": "10.0.0.2", "prefix": 24 },
                 "arpNeighbors": [], "interfaceKind": "wifi" }"#,
        );
        assert!(p.arp_neighbors("wlan0", Some("10.0.0.1")).await.is_empty());
        // interface_addr present -> topology would still be `ok`
        assert!(p.interface_addr("wlan0").await.is_some());
    }

    // spec: android-host-snapshot#S6
    #[tokio::test]
    async fn address_unavailable() {
        let p = probe(
            r#"{ "defaultRoute": { "gateway": "10.0.0.1", "device": "wlan0" },
                 "interfaceAddr": null, "interfaceKind": "wifi" }"#,
        );
        assert!(p.default_route().await.is_some());
        assert!(p.interface_addr("wlan0").await.is_none());
    }

    // spec: android-host-snapshot#S7
    #[tokio::test]
    async fn arp_entry_without_mac_is_kept() {
        let p = probe(
            r#"{ "arpNeighbors": [ { "ip": "192.168.1.9", "mac": null } ],
                 "interfaceKind": "wifi" }"#,
        );
        let neighbors = p.arp_neighbors("wlan0", None).await;
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].mac, None);
        assert_eq!(neighbors[0].vendor, None);
    }

    #[tokio::test]
    async fn gateway_flag_inferred_from_gateway_ip() {
        // caller didn't set isGateway, but passed the gateway IP
        let p = probe(
            r#"{ "arpNeighbors": [ { "ip": "192.168.1.1", "mac": "aa:bb:cc:dd:ee:ff" } ],
                 "interfaceKind": "wifi" }"#,
        );
        let neighbors = p.arp_neighbors("wlan0", Some("192.168.1.1")).await;
        assert!(neighbors[0].is_gateway);
    }

    #[tokio::test]
    async fn minimal_snapshot_parses() {
        // everything optional -> an empty object is a valid (useless) snapshot
        let p = probe("{}");
        assert!(p.default_route().await.is_none());
        assert!(p.wifi_info("x", false).await.is_none());
        assert!(p.dns_info("x").await.is_none());
        assert_eq!(p.interface_type("x").await, InterfaceKind::Other);
    }
}
