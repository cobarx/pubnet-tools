//! Generates the sample HTML report committed at
//! `docs/examples/sample-report.html`, so a reviewer (or a curious reader)
//! can see what `pubnetchk --html` produces without running it.
//!
//! Deliberately synthetic — a made-up open-WiFi network, not a real capture —
//! so nothing about anyone's actual network is published, and the output is
//! deterministic (fixed timestamp, no local offset, so the footer renders in
//! UTC). It shows the range of the design at once: a High-risk verdict, an
//! alert card, and two "what's fine" reassurances.
//!
//! Regenerate with:
//!     cargo run --example sample_report > docs/examples/sample-report.html

use pubnet_tools::output::html::render_html;
use pubnet_tools::types::*;

fn empty_findings() -> Vec<Finding> {
    vec![]
}

fn target(label: PingTargetLabel, host: &str, avg_ms: f64) -> PingTargetResult {
    PingTargetResult {
        host: host.to_string(),
        label,
        transmitted: 10,
        received: 10,
        packet_loss_pct: 0.0,
        min_ms: Some(avg_ms - 2.0),
        avg_ms: Some(avg_ms),
        max_ms: Some(avg_ms + 5.0),
        jitter_ms: Some(2.0),
        rtts: vec![avg_ms - 2.0, avg_ms, avg_ms + 5.0],
        reachable: true,
    }
}

fn sample_report() -> Report {
    let wifi_open = Finding {
        id: "security.wifi-open".to_string(),
        severity: Severity::Alert,
        points: 40,
        title: "WiFi is open (unencrypted)".to_string(),
        detail: None,
    };
    let dns_clean = Finding {
        id: "security.dns-clean".to_string(),
        severity: Severity::Good,
        points: 0,
        title: "No DNS leak detected".to_string(),
        detail: None,
    };
    let no_portal = Finding {
        id: "security.no-captive-portal".to_string(),
        severity: Severity::Good,
        points: 0,
        title: "No captive portal detected".to_string(),
        detail: None,
    };

    Report {
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: "2026-08-24T19:30:00Z".to_string(),
        topology: CheckResult {
            name: "topology".to_string(),
            status: CheckStatus::Ok,
            data: Some(TopologyData {
                interface: "wlan0".to_string(),
                interface_kind: InterfaceKind::WiFi,
                ip_cidr: "10.5.12.83/24".to_string(),
                gateway: "10.5.12.1".to_string(),
                neighbors: vec![ArpNeighbor {
                    ip: "10.5.12.1".to_string(),
                    mac: Some("aa:bb:cc:dd:ee:ff".to_string()),
                    state: "REACHABLE".to_string(),
                    device: "wlan0".to_string(),
                    is_gateway: true,
                    vendor: Some("Ubiquiti".to_string()),
                }],
                passive_notice: "Passive ARP cache — no active scan performed.".to_string(),
            }),
            errors: vec![],
            findings: empty_findings(),
            duration_ms: 6,
        },
        security: CheckResult {
            name: "security".to_string(),
            status: CheckStatus::Ok,
            data: Some(SecurityData {
                ssid: Some("Airport_Free_WiFi".to_string()),
                encryption: WifiEncryption::Open,
                channel: Some(11),
                frequency_mhz: Some(2462),
                signal_percent: Some(64),
                dns: None,
                dns_leak: DnsLeakResult {
                    system_egress_ip: None,
                    probes: vec![],
                    leaked: false,
                    verdict: DnsLeakVerdict::Clean,
                },
                captive_portal: CaptivePortalResult {
                    detected: false,
                    method: CaptivePortalMethod::None,
                    redirect_location: None,
                    canary_url: "http://connectivity-check.example/generate_204".to_string(),
                    http_status: Some(204),
                },
            }),
            errors: vec![],
            findings: vec![wifi_open.clone(), dns_clean.clone(), no_portal.clone()],
            duration_ms: 820,
        },
        reliability: CheckResult {
            name: "reliability".to_string(),
            status: CheckStatus::Ok,
            data: Some(ReliabilityData {
                targets: vec![
                    target(PingTargetLabel::Gateway, "10.5.12.1", 3.1),
                    target(PingTargetLabel::GoogleDns, "8.8.8.8", 22.4),
                    target(PingTargetLabel::CloudflareDns, "1.1.1.1", 19.8),
                ],
                gateway_reachable: true,
                internet_reachable: true,
            }),
            errors: vec![],
            findings: empty_findings(),
            duration_ms: 2100,
        },
        speed: CheckResult {
            name: "speed".to_string(),
            status: CheckStatus::Ok,
            data: Some(SpeedData {
                download_mbps: 38.2,
                upload_mbps: 11.7,
                latency_ms: 24.6,
                jitter_ms: 6.1,
                source: "ndt7".to_string(),
            }),
            errors: vec![],
            findings: empty_findings(),
            duration_ms: 20000,
        },
        score: ScoreResult {
            total: 40,
            level: RiskLevel::High,
            findings: vec![wifi_open, dns_clean, no_portal],
        },
    }
}

fn main() {
    print!("{}", render_html(&sample_report()));
}
