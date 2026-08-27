use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WifiEncryption {
    #[serde(rename = "WPA3")]
    Wpa3,
    #[serde(rename = "WPA2")]
    Wpa2,
    #[serde(rename = "WPA2-Enterprise")]
    Wpa2Enterprise,
    #[serde(rename = "WPA")]
    Wpa,
    Open,
    Unknown,
}

impl WifiEncryption {
    /// Same form serde uses for JSON - `{:?}` would produce PascalCase
    /// ("Wpa3", "Wpa2Enterprise") and silently diverge from the TS
    /// version's finding titles (see PingTargetLabel::as_str for the same
    /// pattern, caught the same way while porting reliability.rs).
    pub fn as_str(&self) -> &'static str {
        match self {
            WifiEncryption::Wpa3 => "WPA3",
            WifiEncryption::Wpa2 => "WPA2",
            WifiEncryption::Wpa2Enterprise => "WPA2-Enterprise",
            WifiEncryption::Wpa => "WPA",
            WifiEncryption::Open => "Open",
            WifiEncryption::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsResolverInfo {
    pub link: String,
    pub current_server: Option<String>,
    pub servers: Vec<String>,
    pub source: DnsSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DnsSource {
    Resolvectl,
    ResolvConf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterfaceKind {
    WiFi,
    Ethernet,
    Vpn,
    Other,
}

impl InterfaceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            InterfaceKind::WiFi => "WiFi",
            InterfaceKind::Ethernet => "Ethernet",
            InterfaceKind::Vpn => "VPN",
            InterfaceKind::Other => "Other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArpNeighbor {
    pub ip: String,
    pub mac: Option<String>,
    pub state: String,
    pub device: String,
    pub is_gateway: bool,
    pub vendor: Option<String>,
}
