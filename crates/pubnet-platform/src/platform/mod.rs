//! Platform abstraction: each OS implements PlatformProbe to translate
//! native commands into common types. Checks call probe methods — they
//! never invoke platform-specific binaries directly.

use crate::types::{ArpNeighbor, DnsResolverInfo, InterfaceKind, WifiEncryption};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

/// True when the interface name identifies a software-defined tunnel (VPN).
/// utun* = macOS Network Extension tunnels (Tailscale, OpenVPN, WireGuard, built-in macOS VPN)
/// tun*/tap* = Linux userspace tunnel interfaces
/// wg* = WireGuard kernel interfaces on Linux
/// tailscale* = Tailscale on Linux (uses tailscale0, not tun*)
/// ppp* = PPP-based VPNs
pub fn is_vpn_iface(iface: &str) -> bool {
    iface.starts_with("utun")
        || iface.starts_with("tun")
        || iface.starts_with("tap")
        || iface.starts_with("wg")
        || iface.starts_with("tailscale")
        || iface.starts_with("ppp")
}

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
    /// `None` when the OS joins a network but withholds its name (macOS 15+
    /// gates the SSID behind Location Services). See `ssid_hidden`.
    pub ssid: Option<String>,
    /// True when the interface is on Wi-Fi but the SSID was withheld for
    /// privacy rather than simply absent. Always `false` off macOS.
    pub ssid_hidden: bool,
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

    /// Active WiFi network on `iface`: SSID, encryption, and — when `detail`
    /// is set — channel, frequency, and signal. `detail` exists because on
    /// macOS the channel/signal source (`system_profiler`) takes several
    /// seconds; callers pass `false` to skip it. Linux and Windows read
    /// everything in one call and ignore `detail`.
    async fn wifi_info(&self, iface: &str, detail: bool) -> Option<WifiInfo>;

    /// DNS resolver configuration for the given interface.
    async fn dns_info(&self, iface: &str) -> Option<DnsResolverInfo>;

    /// IP seen by the system DNS resolver (used for DNS leak detection).
    /// Returns None on platforms where this isn't directly obtainable.
    /// TODO: add dig/curl fallback for macOS.
    async fn system_egress_ip(&self) -> Option<String>;

    /// Whether the interface is WiFi, Ethernet, or something else.
    async fn interface_type(&self, iface: &str) -> InterfaceKind;
}
