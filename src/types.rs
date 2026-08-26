use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Ok,
    Degraded,
    Failed,
    Skipped,
}

impl CheckStatus {
    /// Same lowercase form serde uses for JSON - see WifiEncryption::as_str
    /// and PingTargetLabel::as_str for why `{:?}` isn't used for anything
    /// rendered to the user.
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Ok => "ok",
            CheckStatus::Degraded => "degraded",
            CheckStatus::Failed => "failed",
            CheckStatus::Skipped => "skipped",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Good,
    Info,
    Warn,
    Alert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub points: u32,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult<T> {
    pub name: String,
    pub status: CheckStatus,
    pub data: Option<T>,
    pub errors: Vec<String>,
    pub findings: Vec<Finding>,
    pub duration_ms: u64,
}

// --- Security ---

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
pub enum DohProvider {
    Cloudflare,
    Google,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DohProbe {
    pub provider: DohProvider,
    pub egress_ip: Option<String>,
    pub reachable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DnsLeakVerdict {
    Clean,
    Leaked,
    Uncertain,
}

impl DnsLeakVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            DnsLeakVerdict::Clean => "clean",
            DnsLeakVerdict::Leaked => "leaked",
            DnsLeakVerdict::Uncertain => "uncertain",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsLeakResult {
    pub system_egress_ip: Option<String>,
    pub probes: Vec<DohProbe>,
    pub leaked: bool,
    pub verdict: DnsLeakVerdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CaptivePortalMethod {
    Redirect,
    ContentMismatch,
    None,
}

impl CaptivePortalMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            CaptivePortalMethod::Redirect => "redirect",
            CaptivePortalMethod::ContentMismatch => "content-mismatch",
            CaptivePortalMethod::None => "none",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptivePortalResult {
    pub detected: bool,
    pub method: CaptivePortalMethod,
    pub redirect_location: Option<String>,
    pub canary_url: String,
    pub http_status: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityData {
    pub ssid: Option<String>,
    pub encryption: WifiEncryption,
    pub channel: Option<u32>,
    pub frequency_mhz: Option<u32>,
    pub signal_percent: Option<u32>,
    pub dns: Option<DnsResolverInfo>,
    pub dns_leak: DnsLeakResult,
    pub captive_portal: CaptivePortalResult,
}

// --- Speed ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedData {
    pub download_mbps: f64,
    pub upload_mbps: f64,
    pub latency_ms: f64,
    pub jitter_ms: f64,
    pub source: String, // always "ndt7"
}

// --- Reliability ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PingTargetLabel {
    Gateway,
    GoogleDns,
    CloudflareDns,
}

impl PingTargetLabel {
    /// Same kebab-case form serde uses for JSON, for building stable
    /// finding IDs (e.g. `reliability.packet-loss.google-dns`) - `{:?}`
    /// would produce PascalCase and silently diverge from the TS version's
    /// finding IDs.
    pub fn as_str(&self) -> &'static str {
        match self {
            PingTargetLabel::Gateway => "gateway",
            PingTargetLabel::GoogleDns => "google-dns",
            PingTargetLabel::CloudflareDns => "cloudflare-dns",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingTargetResult {
    pub host: String,
    pub label: PingTargetLabel,
    pub transmitted: u32,
    pub received: u32,
    pub packet_loss_pct: f64,
    pub min_ms: Option<f64>,
    pub avg_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub rtts: Vec<f64>,
    pub reachable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReliabilityData {
    pub targets: Vec<PingTargetResult>,
    pub gateway_reachable: bool,
    pub internet_reachable: bool,
}

// --- Topology ---

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyData {
    pub interface: String,
    pub interface_kind: InterfaceKind,
    pub ip_cidr: String,
    pub gateway: String,
    pub neighbors: Vec<ArpNeighbor>,
    pub passive_notice: String,
}

// --- Report ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreResult {
    pub total: u32,
    pub level: RiskLevel,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub version: String,
    pub timestamp: String,
    pub security: CheckResult<SecurityData>,
    pub speed: CheckResult<SpeedData>,
    pub reliability: CheckResult<ReliabilityData>,
    pub topology: CheckResult<TopologyData>,
    pub score: ScoreResult,
}
