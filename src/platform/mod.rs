//! Platform abstraction: each OS implements PlatformProbe to translate
//! native commands into common types. Checks call probe methods — they
//! never invoke platform-specific binaries directly.

use crate::types::{ArpNeighbor, DnsResolverInfo, WifiEncryption};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;

#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub gateway: String,
    pub device: String,
}

#[derive(Debug, Clone)]
pub struct AddrInfo {
    pub ip: String,
    pub prefix: u32,
}

#[derive(Debug, Clone)]
pub struct WifiInfo {
    pub ssid: String,
    pub encryption: WifiEncryption,
    pub channel: Option<u32>,
    pub frequency_mhz: Option<u32>,
    pub signal_percent: Option<u32>,
}

#[allow(async_fn_in_trait)] // internal trait, not part of a public API surface
pub trait PlatformProbe {
    /// Default gateway IP and the interface it's on.
    async fn default_route(&self) -> Option<RouteInfo>;

    /// IPv4 address and prefix length for the given interface.
    async fn interface_addr(&self, iface: &str) -> Option<AddrInfo>;

    /// Passive ARP neighbors on the given interface. Never performs active scanning.
    async fn arp_neighbors(&self, iface: &str, gateway_ip: Option<&str>) -> Vec<ArpNeighbor>;

    /// Active WiFi network: SSID, encryption, channel, frequency, signal.
    async fn wifi_info(&self) -> Option<WifiInfo>;

    /// DNS resolver configuration for the given interface.
    async fn dns_info(&self, iface: &str) -> Option<DnsResolverInfo>;

    /// IP seen by the system DNS resolver (used for DNS leak detection).
    /// Returns None on platforms where this isn't directly obtainable.
    /// TODO: add dig/curl fallback for macOS.
    async fn system_egress_ip(&self) -> Option<String>;
}
