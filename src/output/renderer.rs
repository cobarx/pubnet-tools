//! Port of src/output/renderer.ts: Network -> Security -> Performance
//! terminal sections, condensed local/internet loss+latency, WiFi risk
//! callout.

use crate::types::{DnsLeakVerdict, PingTargetLabel, ReliabilityData, Report, RiskLevel, Severity};
use console::{style, Style};

fn level_style(level: RiskLevel) -> Style {
    match level {
        RiskLevel::Low => Style::new().green(),
        RiskLevel::Medium => Style::new().yellow(),
        RiskLevel::High => Style::new().red(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HopSummary {
    pub loss_pct: f64,
    pub avg_latency_ms: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReliabilitySummary {
    pub local: Option<HopSummary>,
    pub internet: Option<HopSummary>,
}

/// Condenses per-target ping data into the two things worth seeing at a
/// glance: is the local hop (gateway) lossy/slow, and is "the internet"
/// (the external targets, aggregated) lossy/slow. Per-target detail and
/// jitter stay in the JSON report - this is deliberately less than that,
/// not a different view of the same amount of information.
pub fn summarize_reliability(rel: &ReliabilityData) -> ReliabilitySummary {
    let gateway = rel.targets.iter().find(|t| t.label == PingTargetLabel::Gateway);
    let external: Vec<_> = rel.targets.iter().filter(|t| t.label != PingTargetLabel::Gateway).collect();

    let local = gateway.map(|g| HopSummary { loss_pct: g.packet_loss_pct, avg_latency_ms: g.avg_ms });

    let internet = if external.is_empty() {
        None
    } else {
        let reachable: Vec<_> = external.iter().filter(|t| t.reachable && t.avg_ms.is_some()).collect();
        let loss_pct = external.iter().map(|t| t.packet_loss_pct).fold(f64::MIN, f64::max);
        let avg_latency_ms = if reachable.is_empty() {
            None
        } else {
            Some(reachable.iter().filter_map(|t| t.avg_ms).sum::<f64>() / reachable.len() as f64)
        };
        Some(HopSummary { loss_pct, avg_latency_ms })
    };

    ReliabilitySummary { local, internet }
}

fn render_hop(label: &str, hop: Option<HopSummary>) -> String {
    match hop {
        None => format!("  {label}: no data"),
        Some(h) => {
            let latency = h.avg_latency_ms.map(|ms| format!("{ms:.1}ms")).unwrap_or_else(|| "unreachable".to_string());
            format!("  {label}: {:.0}% loss, {latency}", h.loss_pct)
        }
    }
}

/// Findings drive the score but aren't rendered as their own list - every
/// finding restates something already visible in the sections below
/// (encryption, DNS leak verdict, captive portal, packet loss/latency), so
/// a separate list would just repeat it.
fn render_network_section(report: &Report) -> Vec<String> {
    let mut lines = vec!["Network:".to_string()];
    let topo = &report.topology.data;
    let sec = &report.security.data;

    if let Some(topo) = topo {
        let gateway_vendor = topo.neighbors.iter().find(|n| n.is_gateway).and_then(|n| n.vendor.clone());
        let vendor_suffix = gateway_vendor.map(|v| format!(" ({v})")).unwrap_or_default();
        lines.push(format!("  Interface: {} · {} ({})", style(&topo.interface).bold(), topo.interface_kind.as_str(), topo.ip_cidr));
        lines.push(format!("  Gateway: {}{}", topo.gateway, vendor_suffix));
    } else {
        lines.push(format!("  Topology: {}", report.topology.status.as_str()));
    }

    if let Some(sec) = sec {
        if let Some(ssid) = &sec.ssid {
            lines.push(format!("  SSID: {} — {}", ssid, sec.encryption.as_str()));
            if let Some(channel) = sec.channel {
                let freq = sec.frequency_mhz.map(|f| format!(" ({f} MHz)")).unwrap_or_default();
                let signal = sec.signal_percent.map(|s| format!(", Signal: {s}%")).unwrap_or_default();
                lines.push(format!("  Channel: {channel}{freq}{signal}"));
            }
        }
    }

    lines
}

/// spec: discussed 2026-08-25 - "DNS leak" is VPN-testing jargon (a VPN's
/// tunnel is supposed to carry DNS, and a "leak" means it escaped through
/// the regular network instead) that doesn't match what this check does:
/// verifying nothing on the local network is intercepting/rewriting DNS
/// answers, independent of any VPN. This is terminal-display wording
/// only, so `DnsLeakVerdict::as_str()`, the JSON field name, and the
/// underlying spec/decision docs keep the original "dns leak"
/// terminology; `--json` output and existing docs are unaffected.
fn describe_dns_leak(verdict: DnsLeakVerdict) -> &'static str {
    match verdict {
        DnsLeakVerdict::Clean => "not intercepted",
        DnsLeakVerdict::Leaked => "intercepted",
        DnsLeakVerdict::Uncertain => "uncertain",
    }
}

fn render_security_section(report: &Report) -> Vec<String> {
    let mut lines = vec!["Security:".to_string()];
    let sec = &report.security.data;

    if let Some(sec) = sec {
        let wifi_risk = report
            .security
            .findings
            .iter()
            .find(|f| f.id.starts_with("security.wifi-") && matches!(f.severity, Severity::Alert | Severity::Warn));
        if let Some(risk) = wifi_risk {
            let styled = if risk.severity == Severity::Alert {
                style(format!("⚠ {}", risk.title)).red()
            } else {
                style(format!("⚠ {}", risk.title)).yellow()
            };
            lines.push(format!("  {styled}"));
        }
        lines.push(format!("  DNS check: {}", describe_dns_leak(sec.dns_leak.verdict)));
        let portal = if sec.captive_portal.detected {
            format!("detected ({})", sec.captive_portal.method.as_str())
        } else {
            "none".to_string()
        };
        lines.push(format!("  Captive portal: {portal}"));
    } else {
        lines.push(format!("  Security: {}", report.security.status.as_str()));
    }

    lines
}

fn ms_or_dash(value: Option<f64>) -> String {
    value.map(|v| format!("{v:.1}ms")).unwrap_or_else(|| "—".to_string())
}

/// The per-target detail --verbose restores: everything summarize_reliability
/// condenses away for the default view (per-target min/avg/max/jitter, not
/// just loss+avg latency aggregated into Local/Internet).
fn render_target_detail(rel: &ReliabilityData) -> Vec<String> {
    let mut lines = vec!["  Targets:".to_string()];
    for t in &rel.targets {
        lines.push(format!(
            "    {} ({}): {:.0}% loss, min {}, avg {}, max {}, jitter {}",
            style(t.label.as_str()).bold(),
            t.host,
            t.packet_loss_pct,
            ms_or_dash(t.min_ms),
            ms_or_dash(t.avg_ms),
            ms_or_dash(t.max_ms),
            ms_or_dash(t.jitter_ms),
        ));
    }
    lines
}

fn render_performance_section(report: &Report, verbose: bool) -> Vec<String> {
    let mut lines = vec!["Performance:".to_string()];
    let rel = &report.reliability.data;
    let speed = &report.speed.data;

    if let Some(rel) = rel {
        let summary = summarize_reliability(rel);
        lines.push(render_hop("Local", summary.local));
        lines.push(render_hop("Internet", summary.internet));
        if verbose {
            lines.extend(render_target_detail(rel));
        }
    } else {
        lines.push(format!("  Reliability: {}", report.reliability.status.as_str()));
    }

    if let Some(speed) = speed {
        lines.push(format!(
            "  {} {:.1} Mbps down / {:.1} Mbps up",
            style("Speed:").bold(),
            speed.download_mbps,
            speed.upload_mbps
        ));
    } else {
        lines.push(format!("  Speed: {}", report.speed.status.as_str()));
    }

    lines
}

pub fn render_report(report: &Report, verbose: bool) -> String {
    let level_style = level_style(report.score.level);
    let mut lines = vec![
        String::new(),
        level_style.apply_to(format!("Risk: {:?} ({} pts)", report.score.level, report.score.total)).bold().to_string(),
        String::new(),
    ];
    lines.extend(render_network_section(report));
    lines.push(String::new());
    lines.extend(render_security_section(report));
    lines.push(String::new());
    lines.extend(render_performance_section(report, verbose));
    lines.push(String::new());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn target(label: PingTargetLabel, avg_ms: Option<f64>, packet_loss_pct: f64, reachable: bool) -> PingTargetResult {
        PingTargetResult {
            host: "1.1.1.1".to_string(),
            label,
            transmitted: 10,
            received: 10,
            packet_loss_pct,
            min_ms: Some(9.0),
            avg_ms,
            max_ms: Some(20.0),
            jitter_ms: Some(2.1),
            rtts: vec![9.0, 12.0, 20.0],
            reachable,
        }
    }

    fn base_report() -> Report {
        let empty_findings: Vec<Finding> = vec![];
        Report {
            version: "0.1.0".to_string(),
            timestamp: "2026-08-24T12:00:00.000Z".to_string(),
            topology: CheckResult {
                name: "topology".to_string(),
                status: CheckStatus::Ok,
                data: Some(TopologyData {
                    interface: "wlan0".to_string(),
                    interface_kind: InterfaceKind::WiFi,
                    ip_cidr: "192.168.5.151/24".to_string(),
                    gateway: "192.168.5.1".to_string(),
                    neighbors: vec![ArpNeighbor {
                        ip: "192.168.5.1".to_string(),
                        mac: Some("68:7f:f0:55:77:7b".to_string()),
                        state: "REACHABLE".to_string(),
                        device: "wlan0".to_string(),
                        is_gateway: true,
                        vendor: Some("TP-Link".to_string()),
                    }],
                    passive_notice: "Passive ARP cache — no active scan performed.".to_string(),
                }),
                errors: vec![],
                findings: empty_findings.clone(),
                duration_ms: 5,
            },
            security: CheckResult {
                name: "security".to_string(),
                status: CheckStatus::Ok,
                data: Some(SecurityData {
                    ssid: Some("Berkeley-Visitor".to_string()),
                    encryption: WifiEncryption::Open,
                    channel: Some(6),
                    frequency_mhz: Some(2437),
                    signal_percent: Some(80),
                    dns: None,
                    dns_leak: DnsLeakResult {
                        system_egress_ip: None,
                        probes: vec![],
                        leaked: false,
                        verdict: DnsLeakVerdict::Uncertain,
                    },
                    captive_portal: CaptivePortalResult {
                        detected: false,
                        method: CaptivePortalMethod::None,
                        redirect_location: None,
                        canary_url: "x".to_string(),
                        http_status: Some(204),
                    },
                }),
                errors: vec![],
                findings: vec![Finding {
                    id: "security.wifi-open".to_string(),
                    severity: Severity::Alert,
                    points: 40,
                    title: "WiFi is open (unencrypted)".to_string(),
                    detail: None,
                }],
                duration_ms: 10,
            },
            reliability: CheckResult {
                name: "reliability".to_string(),
                status: CheckStatus::Ok,
                data: Some(ReliabilityData {
                    targets: vec![
                        target(PingTargetLabel::Gateway, Some(7.3), 0.0, true),
                        target(PingTargetLabel::GoogleDns, Some(13.2), 0.0, true),
                        target(PingTargetLabel::CloudflareDns, Some(12.0), 0.0, true),
                    ],
                    gateway_reachable: true,
                    internet_reachable: true,
                }),
                errors: vec![],
                findings: empty_findings.clone(),
                duration_ms: 2000,
            },
            speed: CheckResult {
                name: "speed".to_string(),
                status: CheckStatus::Ok,
                data: Some(SpeedData {
                    download_mbps: 46.6,
                    upload_mbps: 23.6,
                    latency_ms: 23.1,
                    jitter_ms: 5.3,
                    source: "ndt7".to_string(),
                }),
                errors: vec![],
                findings: empty_findings.clone(),
                duration_ms: 20000,
            },
            score: ScoreResult {
                total: 40,
                level: RiskLevel::High,
                findings: vec![Finding {
                    id: "security.wifi-open".to_string(),
                    severity: Severity::Alert,
                    points: 40,
                    title: "WiFi is open (unencrypted)".to_string(),
                    detail: None,
                }],
            },
        }
    }

    #[test]
    fn local_is_the_gateway_target_directly() {
        let summary = summarize_reliability(base_report().reliability.data.as_ref().unwrap());
        assert_eq!(summary.local, Some(HopSummary { loss_pct: 0.0, avg_latency_ms: Some(7.3) }));
    }

    #[test]
    fn internet_aggregates_external_targets_worst_loss_average_latency() {
        let mut rel = base_report().reliability.data.unwrap();
        rel.targets = vec![
            target(PingTargetLabel::Gateway, Some(7.3), 0.0, true),
            target(PingTargetLabel::GoogleDns, Some(10.0), 20.0, true),
            target(PingTargetLabel::CloudflareDns, Some(20.0), 0.0, true),
        ];
        let summary = summarize_reliability(&rel);
        assert_eq!(summary.internet, Some(HopSummary { loss_pct: 20.0, avg_latency_ms: Some(15.0) }));
    }

    #[test]
    fn fully_unreachable_hop_reports_loss_with_none_latency() {
        let mut rel = base_report().reliability.data.unwrap();
        rel.targets = vec![
            target(PingTargetLabel::Gateway, None, 100.0, false),
            target(PingTargetLabel::GoogleDns, Some(10.0), 0.0, true),
            target(PingTargetLabel::CloudflareDns, Some(20.0), 0.0, true),
        ];
        let summary = summarize_reliability(&rel);
        assert_eq!(summary.local, Some(HopSummary { loss_pct: 100.0, avg_latency_ms: None }));
    }

    #[test]
    fn includes_risk_level_and_score() {
        let output = render_report(&base_report(), false);
        assert!(output.contains("High"));
        assert!(output.contains("40"));
    }

    #[test]
    fn orders_sections_network_security_performance() {
        let output = render_report(&base_report(), false);
        let network_idx = output.find("Network:").unwrap();
        let security_idx = output.find("Security:").unwrap();
        let perf_idx = output.find("Performance:").unwrap();
        assert!(security_idx > network_idx);
        assert!(perf_idx > security_idx);
    }

    #[test]
    fn network_section_has_interface_gateway_ssid_channel_no_passive_notice() {
        let output = render_report(&base_report(), false);
        let lines: Vec<&str> = output.lines().collect();
        let network_idx = lines.iter().position(|l| *l == "Network:").unwrap();
        let security_idx = lines.iter().position(|l| *l == "Security:").unwrap();

        assert!(output.contains("wlan0"));
        assert!(output.contains("192.168.5.1"));
        assert!(output.contains("TP-Link"));
        assert!(!output.contains("Passive ARP cache"));

        let interface_idx = lines.iter().position(|l| l.contains("Interface:")).unwrap();
        let gateway_idx = lines.iter().position(|l| l.contains("Gateway:")).unwrap();
        assert_eq!(gateway_idx, interface_idx + 1);
        assert!(!lines[interface_idx].contains("gateway"));

        let ssid_idx = lines.iter().position(|l| l.contains("SSID:")).unwrap();
        assert!(ssid_idx > network_idx && ssid_idx < security_idx);
        assert!(lines[ssid_idx].contains("Berkeley-Visitor"));
        assert!(lines[ssid_idx].contains("Open"));

        let channel_idx = lines.iter().position(|l| l.contains("Channel:")).unwrap();
        assert_eq!(channel_idx, ssid_idx + 1);
        assert!(lines[channel_idx].contains('6'));
        assert!(lines[channel_idx].contains("2437"));
        assert!(lines[channel_idx].contains("80%"));
    }

    #[test]
    fn network_section_omits_ssid_and_channel_when_no_wifi() {
        let mut report = base_report();
        let sec = report.security.data.as_mut().unwrap();
        sec.ssid = None;
        sec.encryption = WifiEncryption::Unknown;
        sec.channel = None;
        sec.frequency_mhz = None;
        sec.signal_percent = None;
        let output = render_report(&report, false);
        assert!(!output.contains("SSID:"));
        assert!(!output.contains("Channel:"));
        assert!(!output.contains("no SSID"));
    }

    #[test]
    fn security_section_has_only_dns_leak_and_captive_portal() {
        let output = render_report(&base_report(), false);
        assert!(output.contains("uncertain"));

        let lines: Vec<&str> = output.lines().collect();
        let security_idx = lines.iter().position(|l| *l == "Security:").unwrap();
        let perf_idx = lines.iter().position(|l| *l == "Performance:").unwrap();
        let section = &lines[security_idx..perf_idx];
        assert!(!section.iter().any(|l| l.contains("SSID:")));
        assert!(!section.iter().any(|l| l.contains("Channel:")));
        assert!(!section.iter().any(|l| l.contains("Berkeley-Visitor")));
    }

    #[test]
    fn dns_check_uses_plain_language_not_the_raw_leak_jargon() {
        let mut report = base_report();
        report.security.data.as_mut().unwrap().dns_leak.verdict = DnsLeakVerdict::Clean;
        let output = render_report(&report, false);
        assert!(output.contains("DNS check: not intercepted"));
        assert!(!output.contains("DNS leak"));

        report.security.data.as_mut().unwrap().dns_leak.verdict = DnsLeakVerdict::Leaked;
        let output = render_report(&report, false);
        assert!(output.contains("DNS check: intercepted"));
    }

    #[test]
    fn security_calls_out_inadequate_wifi_encryption() {
        let output = render_report(&base_report(), false);
        let lines: Vec<&str> = output.lines().collect();
        let security_idx = lines.iter().position(|l| *l == "Security:").unwrap();
        let perf_idx = lines.iter().position(|l| *l == "Performance:").unwrap();
        let section = &lines[security_idx..perf_idx];
        assert!(section.iter().any(|l| l.contains("WiFi is open (unencrypted)")));
    }

    #[test]
    fn no_wifi_risk_callout_when_encryption_is_adequate() {
        let mut report = base_report();
        report.security.findings = vec![Finding {
            id: "security.wifi-strong".to_string(),
            severity: Severity::Good,
            points: 0,
            title: "WiFi uses WPA3".to_string(),
            detail: None,
        }];
        let output = render_report(&report, false);
        let lines: Vec<&str> = output.lines().collect();
        let security_idx = lines.iter().position(|l| *l == "Security:").unwrap();
        let perf_idx = lines.iter().position(|l| *l == "Performance:").unwrap();
        let section = &lines[security_idx..perf_idx];
        assert!(!section.iter().any(|l| l.contains("WiFi")));
    }

    #[test]
    fn does_not_repeat_dns_leak_or_captive_portal_finding_titles() {
        let mut report = base_report();
        report.security.findings = vec![
            Finding {
                id: "security.wifi-strong".to_string(),
                severity: Severity::Good,
                points: 0,
                title: "WiFi uses WPA3".to_string(),
                detail: None,
            },
            Finding {
                id: "security.dns-leak".to_string(),
                severity: Severity::Alert,
                points: 25,
                title: "DNS leak detected".to_string(),
                detail: None,
            },
        ];
        let output = render_report(&report, false);
        assert!(!output.contains("DNS leak detected"));
    }

    #[test]
    fn performance_section_shows_local_internet_speed_no_jitter_no_per_target() {
        let output = render_report(&base_report(), false);
        assert!(output.contains("Local"));
        assert!(output.contains("Internet"));
        assert!(output.contains("46.6"));
        assert!(output.contains("23.6"));
        assert!(!output.contains("Jitter"));
        assert!(!output.contains("google-dns"));
        assert!(!output.contains("cloudflare-dns"));
    }

    #[test]
    fn verbose_adds_per_target_detail_default_view_omits_it() {
        let quiet = render_report(&base_report(), false);
        assert!(!quiet.contains("Targets:"));
        assert!(!quiet.contains("jitter"));

        let output = render_report(&base_report(), true);
        assert!(output.contains("Targets:"));
        assert!(output.contains("gateway"));
        assert!(output.contains("google-dns"));
        assert!(output.contains("cloudflare-dns"));
        assert!(output.contains("1.1.1.1"));
        assert!(output.contains("min"));
        assert!(output.contains("jitter"));
        // still condensed Local/Internet lines above the detail block
        assert!(output.contains("Local"));
        assert!(output.contains("Internet"));
    }

    #[test]
    fn falls_back_to_check_status_when_data_is_none() {
        let mut report = base_report();
        report.topology = CheckResult {
            name: "topology".to_string(),
            status: CheckStatus::Skipped,
            data: None,
            errors: vec!["No default route found".to_string()],
            findings: vec![],
            duration_ms: 1,
        };
        let output = render_report(&report, false);
        assert!(output.contains("skipped"));
    }
}
