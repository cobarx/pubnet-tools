//! Plain-language HTML report — the "show your mom" view.
//!
//! Unlike renderer.rs (terminal, condensed, for someone who already knows
//! what a gateway and packet loss are), this translates every finding into
//! one sentence of "what it means for you" and leads with a single verdict:
//! is this network safe to use, and for what. The nerdy detail (interface,
//! CIDR, per-target RTT/jitter) is still present but tucked into a collapsed
//! <details> block so it never gets in the way of the headline.
//!
//! Output is a single self-contained HTML string: inline CSS, no external
//! assets or fonts, no JavaScript. That's what makes it openable straight
//! from file:// (via xdg-open) and safe to email or copy around — see
//! docs/decisions/2026-08-26-html-report-output.md.

use crate::types::{
    CaptivePortalMethod, DnsLeakVerdict, Finding, InterfaceKind, PingTargetLabel, ReliabilityData, Report, RiskLevel,
    Severity, WifiEncryption,
};
use std::sync::OnceLock;
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};

/// The machine's local UTC offset, captured once at startup. `None` means we
/// couldn't read it (or `init_local_offset` was never called — e.g. in tests),
/// in which case the footer falls back to UTC.
static LOCAL_OFFSET: OnceLock<Option<UtcOffset>> = OnceLock::new();

/// Capture the local UTC offset. MUST be called while the process is still
/// single-threaded — the `time` crate refuses to read the local offset once
/// other threads exist (reading the environment's timezone concurrently is
/// unsound), so `main` calls this before building the tokio runtime. Idempotent:
/// a second call is ignored.
pub fn init_local_offset() {
    let _ = LOCAL_OFFSET.set(UtcOffset::current_local_offset().ok());
}

fn local_offset() -> Option<UtcOffset> {
    LOCAL_OFFSET.get().copied().flatten()
}

fn month_name(m: time::Month) -> &'static str {
    use time::Month::*;
    match m {
        January => "January",
        February => "February",
        March => "March",
        April => "April",
        May => "May",
        June => "June",
        July => "July",
        August => "August",
        September => "September",
        October => "October",
        November => "November",
        December => "December",
    }
}

/// Turns the machine timestamp (RFC3339 with sub-second precision, e.g.
/// `2026-08-27T05:44:03.518909347Z`) into something a person reads without
/// flinching: "August 26, 2026 at 10:44 PM". Shown in the reader's own local
/// time when we captured the offset at startup (no zone label needed — it
/// matches their wall clock); otherwise it falls back to UTC, labelled as
/// such so a time that doesn't match their clock isn't silently misleading.
/// Falls back to the raw string if parsing ever fails, so the footer never
/// breaks.
fn friendly_timestamp(rfc3339: &str) -> String {
    format_at(rfc3339, local_offset())
}

fn format_at(rfc3339: &str, offset: Option<UtcOffset>) -> String {
    let Ok(mut dt) = OffsetDateTime::parse(rfc3339, &Rfc3339) else {
        return rfc3339.to_string();
    };
    let suffix = match offset {
        Some(off) => {
            dt = dt.to_offset(off);
            ""
        }
        None => " UTC",
    };
    let hour24 = dt.hour();
    let (hour12, meridiem) = match hour24 {
        0 => (12, "AM"),
        1..=11 => (hour24, "AM"),
        12 => (12, "PM"),
        _ => (hour24 - 12, "PM"),
    };
    format!(
        "{month} {day}, {year} at {hour12}:{minute:02} {meridiem}{suffix}",
        month = month_name(dt.month()),
        day = dt.day(),
        year = dt.year(),
        minute = dt.minute(),
    )
}

/// Minimal HTML-entity escaping for the handful of values that come from the
/// network rather than from us (SSID, gateway vendor, interface name). Not a
/// general-purpose escaper — just enough to keep an adversarial SSID from
/// breaking out of a text node or attribute.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

struct Verdict {
    headline: &'static str,
    subtext: &'static str,
    class: &'static str,
}

fn verdict_for(level: RiskLevel) -> Verdict {
    match level {
        RiskLevel::Low => Verdict {
            headline: "This network looks safe",
            subtext: "Everyday browsing, email, and video look fine here.",
            class: "ok",
        },
        RiskLevel::Medium => Verdict {
            headline: "Use this network with some care",
            subtext: "It's fine for browsing, but avoid banking, shopping, or logging into important accounts.",
            class: "warn",
        },
        RiskLevel::High => Verdict {
            headline: "Be careful on this network",
            subtext: "We'd avoid banking, shopping, or signing into important accounts here. Save those for a network you trust.",
            class: "alert",
        },
    }
}

/// One sentence of "what this means for you", keyed by finding id. Dynamic
/// ids (the per-target reliability ones carry a `.gateway`/`.google-dns`
/// suffix) are matched by prefix. Returns None for a finding we have no
/// plain-language gloss for — those simply fall back to their title, so a
/// newly-added finding is never silently dropped from the page.
fn explain(id: &str) -> Option<&'static str> {
    // Prefix matches first, for the ids that carry a target suffix.
    if id.starts_with("reliability.packet-loss") {
        return Some("Some data is getting lost on the way to the internet. Calls and video may stutter or freeze.");
    }
    if id.starts_with("reliability.high-latency") {
        return Some("The connection is slow to respond — there's noticeable lag. Video calls and games may feel delayed.");
    }
    if id.starts_with("reliability.jitter") {
        return Some("The connection's timing is uneven, so voice and video calls may sound choppy.");
    }
    let text = match id {
        "security.wifi-open" => "This WiFi has no password protection at all. Someone nearby on the same network could potentially see which sites you visit. Avoid anything sensitive.",
        "security.wifi-wpa" => "This WiFi uses WPA — an old, weak kind of password protection. Better than nothing, but not considered secure anymore.",
        "security.wifi-wpa2" => "This WiFi uses WPA2 protection, the common standard. Fine for everyday use; the newest networks use the stronger WPA3.",
        "security.wifi-strong" => "This WiFi uses WPA3, the strongest protection available. Nothing to worry about here.",
        "security.dns-leak" => "Something on this network appears to be tampering with how your computer looks up websites. It could send you to fake versions of real sites. Be cautious.",
        "security.dns-leak-uncertain" => "We couldn't fully confirm the network looks up website addresses honestly. Nothing alarming, but not a clean bill of health either.",
        "security.dns-clean" => "The network looks up website addresses honestly — nothing is tampering with them.",
        "security.captive-portal" => "This network has a sign-in or \u{201c}accept the terms\u{201d} page, like hotels and cafés use. That's normal — just make sure you're on the real page before typing anything.",
        "security.no-captive-portal" => "No sign-in page stands between you and the internet — you have a direct connection.",
        "reliability.gateway-unreachable" => "Your computer can't reliably reach the router itself. The connection may be very unstable.",
        "reliability.internet-unreachable" => "Your computer reaches the local network but not the wider internet. You may be \u{201c}connected\u{201d} yet unable to load websites.",
        "speed.slow-download" => "This connection is very slow to download. Web pages and video may struggle to load.",
        "speed.failed" => "We weren't able to measure this connection's speed.",
        _ => return None,
    };
    Some(text)
}

/// Warn/Alert findings are "things to know"; Good/Info are reassurances.
fn is_problem(sev: Severity) -> bool {
    matches!(sev, Severity::Warn | Severity::Alert)
}

fn severity_class(sev: Severity) -> &'static str {
    match sev {
        Severity::Alert => "alert",
        Severity::Warn => "warn",
        Severity::Good => "ok",
        Severity::Info => "info",
    }
}

fn finding_card(f: &Finding) -> String {
    let body = explain(&f.id).map(str::to_string).unwrap_or_else(|| esc(&f.title));
    format!(
        "      <div class=\"card {cls}\">\n        <div class=\"card-title\">{title}</div>\n        <div class=\"card-body\">{body}</div>\n      </div>\n",
        cls = severity_class(f.severity),
        title = esc(&f.title),
        body = body,
    )
}

/// A friendly one-liner for a download speed, in terms of what it's good for.
fn speed_descriptor(down_mbps: f64) -> &'static str {
    if down_mbps >= 25.0 {
        "Fast enough for HD streaming and video calls."
    } else if down_mbps >= 5.0 {
        "Fine for browsing, email, and standard-quality video."
    } else {
        "On the slow side — web pages and video may struggle."
    }
}

fn plain_encryption(enc: WifiEncryption) -> &'static str {
    match enc {
        WifiEncryption::Wpa3 => "Strong password protection (WPA3)",
        WifiEncryption::Wpa2 | WifiEncryption::Wpa2Enterprise => "Standard password protection (WPA2)",
        WifiEncryption::Wpa => "Weak, outdated protection (WPA)",
        WifiEncryption::Open => "No password protection at all",
        WifiEncryption::Unknown => "Protection type unknown",
    }
}

/// The "at a glance" facts, in words rather than fields.
fn render_facts(report: &Report) -> String {
    let mut rows: Vec<(String, String)> = Vec::new();

    if let Some(sec) = &report.security.data {
        if let Some(ssid) = &sec.ssid {
            rows.push(("Network name".to_string(), esc(ssid)));
            rows.push(("WiFi protection".to_string(), plain_encryption(sec.encryption).to_string()));
        } else if let Some(topo) = &report.topology.data {
            if topo.interface_kind == InterfaceKind::Ethernet {
                rows.push(("Connection".to_string(), "Wired (Ethernet) cable".to_string()));
            }
        }
    }

    if let Some(topo) = &report.topology.data {
        let vendor = topo.neighbors.iter().find(|n| n.is_gateway).and_then(|n| n.vendor.clone());
        let router = match vendor {
            Some(v) => format!("{} ({})", esc(&topo.gateway), esc(&v)),
            None => esc(&topo.gateway),
        };
        rows.push(("Your router".to_string(), router));
    }

    if let Some(speed) = &report.speed.data {
        rows.push((
            "Speed".to_string(),
            format!("{:.0} Mbps down · {:.0} Mbps up — {}", speed.download_mbps, speed.upload_mbps, speed_descriptor(speed.download_mbps)),
        ));
    }

    if rows.is_empty() {
        return String::new();
    }

    let mut html = String::from("      <table class=\"facts\">\n");
    for (k, v) in rows {
        html.push_str(&format!("        <tr><th>{k}</th><td>{v}</td></tr>\n"));
    }
    html.push_str("      </table>\n");
    html
}

fn ms(v: Option<f64>) -> String {
    v.map(|x| format!("{x:.1} ms")).unwrap_or_else(|| "—".to_string())
}

/// The collapsed "for the curious" block: the same data the terminal
/// --verbose view shows, laid out as tables. Present but out of the way.
fn render_technical(report: &Report) -> String {
    let mut html = String::from(
        "    <details class=\"tech\">\n      <summary>Technical details</summary>\n",
    );

    if let Some(topo) = &report.topology.data {
        html.push_str("      <h3>Network</h3>\n      <table class=\"kv\">\n");
        html.push_str(&format!("        <tr><th>Interface</th><td>{} ({})</td></tr>\n", esc(&topo.interface), topo.interface_kind.as_str()));
        html.push_str(&format!("        <tr><th>Your address</th><td>{}</td></tr>\n", esc(&topo.ip_cidr)));
        html.push_str(&format!("        <tr><th>Gateway</th><td>{}</td></tr>\n", esc(&topo.gateway)));
        html.push_str("      </table>\n");
    }

    if let Some(sec) = &report.security.data {
        html.push_str("      <h3>Security</h3>\n      <table class=\"kv\">\n");
        html.push_str(&format!("        <tr><th>Encryption</th><td>{}</td></tr>\n", sec.encryption.as_str()));
        let dns = match sec.dns_leak.verdict {
            DnsLeakVerdict::Clean => "Not intercepted",
            DnsLeakVerdict::Leaked => "Intercepted",
            DnsLeakVerdict::Uncertain => "Uncertain",
        };
        html.push_str(&format!("        <tr><th>DNS check</th><td>{dns}</td></tr>\n"));
        let portal = if sec.captive_portal.detected {
            let method = match sec.captive_portal.method {
                CaptivePortalMethod::Redirect => "redirect",
                CaptivePortalMethod::ContentMismatch => "content mismatch",
                CaptivePortalMethod::None => "none",
            };
            format!("Detected ({method})")
        } else {
            "None".to_string()
        };
        html.push_str(&format!("        <tr><th>Captive portal</th><td>{portal}</td></tr>\n"));
        html.push_str("      </table>\n");
    }

    if let Some(rel) = &report.reliability.data {
        html.push_str(&render_reliability_table(rel));
    }

    if let Some(speed) = &report.speed.data {
        html.push_str("      <h3>Speed</h3>\n      <table class=\"kv\">\n");
        html.push_str(&format!("        <tr><th>Download</th><td>{:.1} Mbps</td></tr>\n", speed.download_mbps));
        html.push_str(&format!("        <tr><th>Upload</th><td>{:.1} Mbps</td></tr>\n", speed.upload_mbps));
        html.push_str(&format!("        <tr><th>Latency</th><td>{:.1} ms (jitter {:.1} ms)</td></tr>\n", speed.latency_ms, speed.jitter_ms));
        html.push_str("      </table>\n");
    }

    html.push_str("    </details>\n");
    html
}

fn render_reliability_table(rel: &ReliabilityData) -> String {
    let mut html = String::from(
        "      <h3>Connection reliability</h3>\n      <table class=\"kv wide\">\n        <tr><th>Target</th><th>Loss</th><th>Avg</th><th>Min</th><th>Max</th><th>Jitter</th></tr>\n",
    );
    for t in &rel.targets {
        let label = match t.label {
            PingTargetLabel::Gateway => "Router",
            PingTargetLabel::GoogleDns => "Google DNS",
            PingTargetLabel::CloudflareDns => "Cloudflare DNS",
        };
        html.push_str(&format!(
            "        <tr><td>{label}</td><td>{:.0}%</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            t.packet_loss_pct,
            ms(t.avg_ms),
            ms(t.min_ms),
            ms(t.max_ms),
            ms(t.jitter_ms),
        ));
    }
    html.push_str("      </table>\n");
    html
}

const STYLE: &str = r#"
    :root {
      --bg: #f4f5f7; --card: #ffffff; --ink: #1c2024; --muted: #5b6470;
      --line: #e2e5ea; --ok: #1f8f4e; --ok-bg: #e7f6ec; --warn: #b9770a;
      --warn-bg: #fdf3e0; --alert: #c1362c; --alert-bg: #fceceb; --info: #3a6ea5; --info-bg: #eaf1f8;
    }
    @media (prefers-color-scheme: dark) {
      :root {
        --bg: #16181c; --card: #21252b; --ink: #e6e8eb; --muted: #9aa4b0;
        --line: #313640; --ok: #4cc27a; --ok-bg: #17301f; --warn: #e0a745;
        --warn-bg: #33280f; --alert: #ef6b60; --alert-bg: #341a18;
        --info: #7fa9d6; --info-bg: #1a2634;
      }
    }
    * { box-sizing: border-box; }
    body { margin: 0; background: var(--bg); color: var(--ink);
      font: 16px/1.55 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }
    .wrap { max-width: 680px; margin: 0 auto; padding: 32px 20px 64px; }
    h1 { font-size: 1.15rem; font-weight: 600; margin: 0 0 4px; }
    .sub { color: var(--muted); font-size: 0.9rem; margin: 0 0 28px; }
    .verdict { border-radius: 14px; padding: 24px 24px; margin: 0 0 28px; border: 1px solid var(--line); }
    .verdict .badge { display: inline-block; font-size: 0.72rem; font-weight: 700; letter-spacing: 0.06em;
      text-transform: uppercase; padding: 3px 10px; border-radius: 999px; margin-bottom: 12px; }
    .verdict h2 { font-size: 1.5rem; margin: 0 0 6px; line-height: 1.25; }
    .verdict p { margin: 0; color: var(--muted); font-size: 1rem; }
    .verdict.ok { background: var(--ok-bg); border-color: var(--ok); }
    .verdict.ok .badge { background: var(--ok); color: #fff; }
    .verdict.warn { background: var(--warn-bg); border-color: var(--warn); }
    .verdict.warn .badge { background: var(--warn); color: #fff; }
    .verdict.alert { background: var(--alert-bg); border-color: var(--alert); }
    .verdict.alert .badge { background: var(--alert); color: #fff; }
    h3.section { font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.06em;
      color: var(--muted); margin: 32px 0 12px; }
    table.facts { width: 100%; border-collapse: collapse; background: var(--card);
      border: 1px solid var(--line); border-radius: 12px; overflow: hidden; }
    table.facts th, table.facts td { text-align: left; padding: 12px 16px; vertical-align: top;
      border-top: 1px solid var(--line); }
    table.facts tr:first-child th, table.facts tr:first-child td { border-top: none; }
    table.facts th { width: 38%; color: var(--muted); font-weight: 500; }
    .card { background: var(--card); border: 1px solid var(--line); border-left-width: 4px;
      border-radius: 10px; padding: 14px 16px; margin: 10px 0; }
    .card-title { font-weight: 600; margin-bottom: 3px; }
    .card-body { color: var(--muted); font-size: 0.95rem; }
    .card.alert { border-left-color: var(--alert); }
    .card.warn { border-left-color: var(--warn); }
    .card.ok { border-left-color: var(--ok); }
    .card.info { border-left-color: var(--info); }
    details.tech { margin-top: 36px; background: var(--card); border: 1px solid var(--line);
      border-radius: 12px; padding: 4px 18px; }
    details.tech summary { cursor: pointer; padding: 12px 0; font-weight: 600; color: var(--muted); }
    details.tech h3 { font-size: 0.9rem; margin: 18px 0 8px; }
    table.kv { width: 100%; border-collapse: collapse; margin-bottom: 8px; font-size: 0.9rem; }
    table.kv th, table.kv td { text-align: left; padding: 6px 8px; border-bottom: 1px solid var(--line); }
    table.kv th { color: var(--muted); font-weight: 500; }
    table.kv.wide th { font-size: 0.8rem; }
    footer { margin-top: 40px; color: var(--muted); font-size: 0.8rem; text-align: center; }
"#;

/// Render the full self-contained HTML page for a report.
pub fn render_html(report: &Report) -> String {
    let verdict = verdict_for(report.score.level);

    let problems: Vec<&Finding> = report.score.findings.iter().filter(|f| is_problem(f.severity)).collect();
    let reassurances: Vec<&Finding> = report.score.findings.iter().filter(|f| !is_problem(f.severity)).collect();

    let mut body = String::new();

    // Verdict banner.
    body.push_str(&format!(
        "    <div class=\"verdict {cls}\">\n      <span class=\"badge\">{badge}</span>\n      <h2>{headline}</h2>\n      <p>{subtext}</p>\n    </div>\n",
        cls = verdict.class,
        badge = match report.score.level {
            RiskLevel::Low => "Looks good",
            RiskLevel::Medium => "Some caution",
            RiskLevel::High => "Take care",
        },
        headline = verdict.headline,
        subtext = verdict.subtext,
    ));

    // At-a-glance facts.
    let facts = render_facts(report);
    if !facts.is_empty() {
        body.push_str("    <h3 class=\"section\">At a glance</h3>\n");
        body.push_str(&facts);
    }

    // Things to know (problems).
    if !problems.is_empty() {
        body.push_str("    <h3 class=\"section\">Things to know</h3>\n");
        for f in &problems {
            body.push_str(&finding_card(f));
        }
    }

    // Reassurances (good/info).
    if !reassurances.is_empty() {
        body.push_str("    <h3 class=\"section\">What's fine</h3>\n");
        for f in &reassurances {
            body.push_str(&finding_card(f));
        }
    }

    // Technical details.
    body.push_str(&render_technical(report));

    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>Network check</title>\n<style>{style}</style>\n</head>\n<body>\n  <div class=\"wrap\">\n    <h1>Network check</h1>\n    <p class=\"sub\">A quick, plain-language look at the network you just joined.</p>\n{body}    <footer>Checked {ts} · pubnetchk v{ver}</footer>\n  </div>\n</body>\n</html>\n",
        style = STYLE,
        body = body,
        ts = esc(&friendly_timestamp(&report.timestamp)),
        ver = esc(&report.version),
    )
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
        let wifi_open = Finding {
            id: "security.wifi-open".to_string(),
            severity: Severity::Alert,
            points: 40,
            title: "WiFi is open (unencrypted)".to_string(),
            detail: None,
        };
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
                findings: vec![],
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
                        verdict: DnsLeakVerdict::Clean,
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
                findings: vec![wifi_open.clone()],
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
                findings: vec![],
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
                findings: vec![],
                duration_ms: 20000,
            },
            score: ScoreResult { total: 40, level: RiskLevel::High, findings: vec![wifi_open] },
        }
    }

    #[test]
    fn is_a_self_contained_html_document() {
        let html = render_html(&base_report());
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<style>"));
        // no external asset references
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn leads_with_a_plain_language_verdict() {
        let html = render_html(&base_report());
        assert!(html.contains("Be careful on this network"));
        // not the raw scoring jargon
        assert!(!html.contains("40 pts"));
    }

    #[test]
    fn low_risk_reads_as_safe() {
        let mut report = base_report();
        report.score.level = RiskLevel::Low;
        let html = render_html(&report);
        assert!(html.contains("This network looks safe"));
    }

    #[test]
    fn translates_findings_into_what_it_means_for_you() {
        let html = render_html(&base_report());
        assert!(html.contains("no password protection"));
        // the card still shows the original title as a heading
        assert!(html.contains("WiFi is open (unencrypted)"));
    }

    #[test]
    fn shows_the_network_name_and_speed_in_plain_words() {
        let html = render_html(&base_report());
        assert!(html.contains("Berkeley-Visitor"));
        assert!(html.contains("Network name"));
        assert!(html.contains("47 Mbps down"));
        assert!(html.contains("HD streaming"));
    }

    #[test]
    fn keeps_the_technical_detail_but_collapsed() {
        let html = render_html(&base_report());
        assert!(html.contains("<details"));
        assert!(html.contains("Technical details"));
        assert!(html.contains("192.168.5.151/24"));
        assert!(html.contains("Cloudflare DNS"));
    }

    #[test]
    fn escapes_hostile_values_from_the_network() {
        let mut report = base_report();
        report.security.data.as_mut().unwrap().ssid = Some("<script>alert(1)</script>".to_string());
        let html = render_html(&report);
        assert!(!html.contains("<script>alert(1)"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn footer_shows_a_human_readable_timestamp_not_nanoseconds() {
        let mut report = base_report();
        report.timestamp = "2026-08-27T05:44:03.518909347Z".to_string();
        let html = render_html(&report);
        assert!(html.contains("August 27, 2026 at 5:44 AM UTC"));
        assert!(!html.contains("518909347"));
    }

    #[test]
    fn friendly_timestamp_handles_noon_midnight_and_pm() {
        assert_eq!(friendly_timestamp("2026-01-01T00:00:00Z"), "January 1, 2026 at 12:00 AM UTC");
        assert_eq!(friendly_timestamp("2026-08-24T12:00:00.000Z"), "August 24, 2026 at 12:00 PM UTC");
        assert_eq!(friendly_timestamp("2026-12-31T23:09:00Z"), "December 31, 2026 at 11:09 PM UTC");
    }

    #[test]
    fn friendly_timestamp_falls_back_on_unparseable_input() {
        assert_eq!(friendly_timestamp("not-a-timestamp"), "not-a-timestamp");
    }

    #[test]
    fn format_at_converts_to_local_offset_and_drops_the_utc_label() {
        // 05:44 UTC at offset -07:00 is the previous evening, 10:44 PM local.
        let pdt = UtcOffset::from_hms(-7, 0, 0).unwrap();
        assert_eq!(format_at("2026-08-27T05:44:03Z", Some(pdt)), "August 26, 2026 at 10:44 PM");
        // A positive offset that crosses midnight forward.
        let cest = UtcOffset::from_hms(2, 0, 0).unwrap();
        assert_eq!(format_at("2026-08-26T23:30:00Z", Some(cest)), "August 27, 2026 at 1:30 AM");
    }

    #[test]
    fn format_at_without_an_offset_stays_utc_labelled() {
        assert_eq!(format_at("2026-08-24T12:00:00.000Z", None), "August 24, 2026 at 12:00 PM UTC");
    }

    #[test]
    fn unglossed_finding_falls_back_to_its_title() {
        let mut report = base_report();
        let novel = Finding {
            id: "security.something-new".to_string(),
            severity: Severity::Warn,
            points: 5,
            title: "A brand new finding".to_string(),
            detail: None,
        };
        report.score.findings = vec![novel];
        let html = render_html(&report);
        assert!(html.contains("A brand new finding"));
    }
}
