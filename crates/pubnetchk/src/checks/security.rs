//! specs: dns-leak-detection, captive-portal-detection
//! See docs/decisions/2026-08-24-dns-leak-address-family-matching.md

use crate::network::{IpFamily, extract_remote_ip, ip_family};
use crate::platform::PlatformProbe;
use crate::types::{
    CaptivePortalMethod, CaptivePortalResult, CheckResult, CheckStatus, DnsLeakResult,
    DnsLeakVerdict, DohProbe, DohProvider, Finding, SecurityData, Severity, WifiEncryption,
};
use std::time::{Duration, Instant};

const DOH_TIMEOUT: Duration = Duration::from_secs(8);
const CAPTIVE_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// DNS leak - spec: dns-leak-detection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RawDohProbe {
    pub provider: DohProvider,
    pub reachable: bool,
    pub egress_ip: Option<String>,
}

fn same_slash24(a: &str, b: &str) -> bool {
    let prefix = |s: &str| s.splitn(4, '.').take(3).collect::<Vec<_>>().join(".");
    prefix(a) == prefix(b)
}

/// spec: dns-leak-detection#S1-S5
/// Only IPv4-vs-IPv4 pairs are ever comparable - a family-mismatched or
/// IPv6-vs-IPv6 pair counts as neither agreement nor disagreement, never
/// as a false leak or a false clean.
pub fn classify_dns_leak(
    system_egress_ip: Option<&str>,
    raw_probes: &[RawDohProbe],
) -> DnsLeakResult {
    let probes: Vec<DohProbe> = raw_probes
        .iter()
        .map(|p| DohProbe {
            provider: p.provider,
            egress_ip: p.egress_ip.clone(),
            reachable: p.reachable,
        })
        .collect();

    let mut any_comparable = false;
    let mut any_disagree = false;

    if let Some(system_ip) = system_egress_ip
        && ip_family(system_ip) == IpFamily::V4
    {
        for p in raw_probes {
            if !p.reachable {
                continue;
            }
            let Some(probe_ip) = &p.egress_ip else {
                continue;
            };
            if ip_family(probe_ip) != IpFamily::V4 {
                continue;
            }
            any_comparable = true;
            if !same_slash24(system_ip, probe_ip) {
                any_disagree = true;
            }
        }
    }

    let verdict = if any_disagree {
        DnsLeakVerdict::Leaked
    } else if any_comparable {
        DnsLeakVerdict::Clean
    } else {
        DnsLeakVerdict::Uncertain
    };

    DnsLeakResult {
        system_egress_ip: system_egress_ip.map(String::from),
        probes,
        leaked: verdict == DnsLeakVerdict::Leaked,
        verdict,
    }
}

async fn probe_doh(client: &reqwest::Client, provider: DohProvider) -> RawDohProbe {
    let url = match provider {
        DohProvider::Cloudflare => {
            "https://cloudflare-dns.com/dns-query?name=whoami.cloudflare.com&type=TXT"
        }
        DohProvider::Google => "https://dns.google/resolve?name=whoami.cloudflare.com&type=TXT",
    };

    let mut req = client.get(url).timeout(DOH_TIMEOUT);
    if provider == DohProvider::Cloudflare {
        req = req.header("accept", "application/dns-json");
    }

    let unreachable = RawDohProbe {
        provider,
        reachable: false,
        egress_ip: None,
    };
    let Ok(res) = req.send().await else {
        return unreachable;
    };
    if res.status() != 200 {
        return unreachable;
    }
    let Ok(body) = res.text().await else {
        return unreachable;
    };
    let egress_ip = extract_remote_ip(&body);
    RawDohProbe {
        provider,
        reachable: egress_ip.is_some(),
        egress_ip,
    }
}

// ---------------------------------------------------------------------------
// Captive portal - spec: captive-portal-detection
// ---------------------------------------------------------------------------

pub struct CanaryResponse {
    pub status: Option<u16>,
    pub location: Option<String>,
    pub body: String,
}

pub struct CanaryExpectation {
    pub expected_status: u16,
    pub expected_body_contains: Option<&'static str>,
}

/// spec: captive-portal-detection#S1-S3
pub fn classify_captive_portal(
    response: &CanaryResponse,
    expectation: &CanaryExpectation,
) -> (bool, CaptivePortalMethod, Option<String>, Option<u16>) {
    let Some(status) = response.status else {
        return (false, CaptivePortalMethod::None, None, None);
    };

    if (300..400).contains(&status) {
        return (
            true,
            CaptivePortalMethod::Redirect,
            response.location.clone(),
            Some(status),
        );
    }

    let status_matches = status == expectation.expected_status;
    let body_matches = expectation
        .expected_body_contains
        .is_none_or(|needle| response.body.contains(needle));

    if status_matches && body_matches {
        (false, CaptivePortalMethod::None, None, Some(status))
    } else {
        (
            true,
            CaptivePortalMethod::ContentMismatch,
            None,
            Some(status),
        )
    }
}

struct Canary {
    url: &'static str,
    expectation: CanaryExpectation,
}

fn canaries() -> Vec<Canary> {
    vec![
        Canary {
            url: "http://connectivitycheck.gstatic.com/generate_204",
            expectation: CanaryExpectation {
                expected_status: 204,
                expected_body_contains: None,
            },
        },
        Canary {
            url: "http://captive.apple.com/hotspot-detect.html",
            expectation: CanaryExpectation {
                expected_status: 200,
                expected_body_contains: Some("Success"),
            },
        },
    ]
}

/// Reqwest's redirect policy is set at Client construction, not per-request —
/// a redirect must not be followed so the classifier can see it as a redirect.
async fn probe_captive_portal() -> CaptivePortalResult {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("building the no-redirect captive-portal client should never fail");

    for canary in canaries() {
        let req = client.get(canary.url).timeout(CAPTIVE_TIMEOUT);
        let Ok(res) = req.send().await else { continue };
        let status = res.status().as_u16();
        let location = res
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let body = res.text().await.unwrap_or_default();

        let (detected, method, redirect_location, http_status) = classify_captive_portal(
            &CanaryResponse {
                status: Some(status),
                location,
                body,
            },
            &canary.expectation,
        );
        return CaptivePortalResult {
            detected,
            method,
            redirect_location,
            canary_url: canary.url.to_string(),
            http_status,
        };
    }
    let first_url = canaries().into_iter().next().unwrap().url;
    CaptivePortalResult {
        detected: false,
        method: CaptivePortalMethod::None,
        redirect_location: None,
        canary_url: first_url.to_string(),
        http_status: None,
    }
}

// ---------------------------------------------------------------------------
// Findings
// ---------------------------------------------------------------------------

fn wifi_findings(encryption: WifiEncryption) -> Vec<Finding> {
    match encryption {
        WifiEncryption::Open => vec![Finding {
            id: "security.wifi-open".to_string(),
            severity: Severity::Alert,
            points: 40,
            title: "WiFi is open (unencrypted)".to_string(),
            detail: None,
        }],
        WifiEncryption::Wpa => vec![Finding {
            id: "security.wifi-wpa".to_string(),
            severity: Severity::Warn,
            points: 20,
            title: "WiFi uses WPA, not WPA2/WPA3".to_string(),
            detail: None,
        }],
        WifiEncryption::Wpa2 => vec![Finding {
            id: "security.wifi-wpa2".to_string(),
            severity: Severity::Info,
            points: 5,
            title: "WiFi uses WPA2, not WPA3".to_string(),
            detail: None,
        }],
        WifiEncryption::Wpa3 | WifiEncryption::Wpa2Enterprise => vec![Finding {
            id: "security.wifi-strong".to_string(),
            severity: Severity::Good,
            points: 0,
            title: format!("WiFi uses {}", encryption.as_str()),
            detail: None,
        }],
        WifiEncryption::Unknown => vec![],
    }
}

fn dns_leak_findings(dns_leak: &DnsLeakResult) -> Vec<Finding> {
    match dns_leak.verdict {
        DnsLeakVerdict::Leaked => vec![Finding {
            id: "security.dns-leak".to_string(),
            severity: Severity::Alert,
            points: 25,
            title: "DNS leak detected".to_string(),
            detail: None,
        }],
        DnsLeakVerdict::Uncertain => vec![Finding {
            id: "security.dns-leak-uncertain".to_string(),
            severity: Severity::Warn,
            points: 5,
            title: "DNS leak status could not be verified".to_string(),
            detail: None,
        }],
        DnsLeakVerdict::Clean => vec![Finding {
            id: "security.dns-clean".to_string(),
            severity: Severity::Good,
            points: 0,
            title: "No DNS leak detected".to_string(),
            detail: None,
        }],
    }
}

fn captive_portal_findings(portal: &CaptivePortalResult) -> Vec<Finding> {
    if portal.detected {
        vec![Finding {
            id: "security.captive-portal".to_string(),
            severity: Severity::Warn,
            points: 15,
            title: "Captive portal detected".to_string(),
            detail: None,
        }]
    } else {
        vec![Finding {
            id: "security.no-captive-portal".to_string(),
            severity: Severity::Good,
            points: 0,
            title: "No captive portal detected".to_string(),
            detail: None,
        }]
    }
}

// ---------------------------------------------------------------------------
// Check orchestration
// ---------------------------------------------------------------------------

pub async fn check_security<P: PlatformProbe>(
    iface: Option<&str>,
    probe: &P,
    http_client: &reqwest::Client,
    wifi_detail: bool,
) -> CheckResult<SecurityData> {
    let start = Instant::now();

    let (wifi, dns, system_egress_ip, cloudflare_probe, google_probe, captive_portal) = tokio::join!(
        async {
            if let Some(iface) = iface {
                probe.wifi_info(iface, wifi_detail).await
            } else {
                None
            }
        },
        async {
            if let Some(iface) = iface {
                probe.dns_info(iface).await
            } else {
                None
            }
        },
        probe.system_egress_ip(),
        probe_doh(http_client, DohProvider::Cloudflare),
        probe_doh(http_client, DohProvider::Google),
        probe_captive_portal(),
    );

    let dns_leak = classify_dns_leak(
        system_egress_ip.as_deref(),
        &[cloudflare_probe, google_probe],
    );
    let encryption = wifi
        .as_ref()
        .map(|w| w.encryption)
        .unwrap_or(WifiEncryption::Unknown);

    let data = SecurityData {
        ssid: wifi.as_ref().and_then(|w| w.ssid.clone()),
        encryption,
        channel: wifi.as_ref().and_then(|w| w.channel),
        frequency_mhz: wifi.as_ref().and_then(|w| w.frequency_mhz),
        signal_percent: wifi.as_ref().and_then(|w| w.signal_percent),
        dns: dns.clone(),
        dns_leak: dns_leak.clone(),
        captive_portal: captive_portal.clone(),
    };

    let mut errors = Vec::new();
    let degraded = iface.is_some() && dns.is_none();
    if degraded {
        errors.push(format!(
            "Could not determine DNS servers for {}",
            iface.unwrap()
        ));
    }

    let mut findings = wifi_findings(encryption);
    if wifi.as_ref().is_some_and(|w| w.ssid_hidden) {
        findings.push(Finding {
            id: "security.wifi-ssid-hidden".to_string(),
            severity: Severity::Info,
            points: 0,
            title: "Wi-Fi network name (SSID) hidden by the OS".to_string(),
            detail: Some(
                "macOS withholds the SSID from command-line tools unless the terminal has \
                 Location Services access (System Settings ▸ Privacy & Security ▸ Location Services)."
                    .to_string(),
            ),
        });
    }
    findings.extend(dns_leak_findings(&dns_leak));
    findings.extend(captive_portal_findings(&captive_portal));

    CheckResult {
        name: "security".to_string(),
        status: if degraded {
            CheckStatus::Degraded
        } else {
            CheckStatus::Ok
        },
        data: Some(data),
        errors,
        findings,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(provider: DohProvider, reachable: bool, egress_ip: Option<&str>) -> RawDohProbe {
        RawDohProbe {
            provider,
            reachable,
            egress_ip: egress_ip.map(String::from),
        }
    }

    // spec: dns-leak-detection#S1
    #[test]
    fn two_comparable_agreeing_probes_are_clean() {
        let result = classify_dns_leak(
            Some("203.0.113.9"),
            &[
                probe(DohProvider::Cloudflare, true, Some("203.0.113.4")),
                probe(DohProvider::Google, true, Some("203.0.113.200")),
            ],
        );
        assert_eq!(result.verdict, DnsLeakVerdict::Clean);
        assert!(!result.leaked);
    }

    // spec: dns-leak-detection#S2
    #[test]
    fn every_probe_unreachable_is_uncertain_never_clean() {
        let result = classify_dns_leak(
            Some("203.0.113.9"),
            &[
                probe(DohProvider::Cloudflare, false, None),
                probe(DohProvider::Google, false, None),
            ],
        );
        assert_eq!(result.verdict, DnsLeakVerdict::Uncertain);
        assert!(!result.leaked);
    }

    // spec: dns-leak-detection#S3
    #[test]
    fn comparable_disagreeing_probe_is_leaked() {
        let result = classify_dns_leak(
            Some("203.0.113.9"),
            &[
                probe(DohProvider::Cloudflare, true, Some("198.51.100.4")),
                probe(DohProvider::Google, true, Some("203.0.113.200")),
            ],
        );
        assert_eq!(result.verdict, DnsLeakVerdict::Leaked);
        assert!(result.leaked);
        let cf = result
            .probes
            .iter()
            .find(|p| p.provider == DohProvider::Cloudflare)
            .unwrap();
        assert_eq!(cf.egress_ip, Some("198.51.100.4".to_string()));
    }

    // spec: dns-leak-detection#S4
    #[test]
    fn one_reachable_agreeing_probe_is_enough_for_clean() {
        let result = classify_dns_leak(
            Some("203.0.113.9"),
            &[
                probe(DohProvider::Cloudflare, true, Some("203.0.113.4")),
                probe(DohProvider::Google, false, None),
            ],
        );
        assert_eq!(result.verdict, DnsLeakVerdict::Clean);
        let google = result
            .probes
            .iter()
            .find(|p| p.provider == DohProvider::Google)
            .unwrap();
        assert!(!google.reachable);
    }

    // spec: dns-leak-detection#S5
    #[test]
    fn family_mismatched_probe_counts_as_neither_when_another_agrees() {
        let result = classify_dns_leak(
            Some("203.0.113.9"),
            &[
                probe(
                    DohProvider::Cloudflare,
                    true,
                    Some("2607:f8b0:4004:1001::12e"),
                ),
                probe(DohProvider::Google, true, Some("203.0.113.4")),
            ],
        );
        assert_eq!(result.verdict, DnsLeakVerdict::Clean);
    }

    // spec: dns-leak-detection#S5
    #[test]
    fn family_mismatched_probe_with_no_other_comparable_is_uncertain() {
        let result = classify_dns_leak(
            Some("2607:f8b0:4004:1001::12e"),
            &[
                probe(DohProvider::Cloudflare, true, Some("203.0.113.4")),
                probe(DohProvider::Google, true, Some("2607:f8b0:4004:1009::12c")),
            ],
        );
        assert_eq!(result.verdict, DnsLeakVerdict::Uncertain);
    }

    #[test]
    fn no_system_egress_ip_is_uncertain() {
        let result = classify_dns_leak(
            None,
            &[probe(DohProvider::Cloudflare, true, Some("203.0.113.4"))],
        );
        assert_eq!(result.verdict, DnsLeakVerdict::Uncertain);
    }

    // --- classify_captive_portal ---

    fn expect_204() -> CanaryExpectation {
        CanaryExpectation {
            expected_status: 204,
            expected_body_contains: None,
        }
    }
    fn expect_200_success() -> CanaryExpectation {
        CanaryExpectation {
            expected_status: 200,
            expected_body_contains: Some("Success"),
        }
    }

    // spec: captive-portal-detection#S1
    #[test]
    fn unmodified_204_is_not_a_portal() {
        let (detected, method, ..) = classify_captive_portal(
            &CanaryResponse {
                status: Some(204),
                location: None,
                body: String::new(),
            },
            &expect_204(),
        );
        assert!(!detected);
        assert_eq!(method, CaptivePortalMethod::None);
    }

    #[test]
    fn unmodified_200_body_match_is_not_a_portal() {
        let (detected, method, ..) = classify_captive_portal(
            &CanaryResponse {
                status: Some(200),
                location: None,
                body: "<HTML><BODY>Success</BODY></HTML>".to_string(),
            },
            &expect_200_success(),
        );
        assert!(!detected);
        assert_eq!(method, CaptivePortalMethod::None);
    }

    // spec: captive-portal-detection#S2
    #[test]
    fn redirect_is_a_portal() {
        let (detected, method, location, _) = classify_captive_portal(
            &CanaryResponse {
                status: Some(302),
                location: Some("http://portal.example.com/login".to_string()),
                body: String::new(),
            },
            &expect_204(),
        );
        assert!(detected);
        assert_eq!(method, CaptivePortalMethod::Redirect);
        assert_eq!(
            location,
            Some("http://portal.example.com/login".to_string())
        );
    }

    // spec: captive-portal-detection#S3
    #[test]
    fn expected_status_with_substituted_content_is_content_mismatch() {
        let (detected, method, ..) = classify_captive_portal(
            &CanaryResponse {
                status: Some(200),
                location: None,
                body: "<HTML><BODY>Please log in</BODY></HTML>".to_string(),
            },
            &expect_200_success(),
        );
        assert!(detected);
        assert_eq!(method, CaptivePortalMethod::ContentMismatch);
    }

    #[test]
    fn unreachable_canary_is_not_detected() {
        let (detected, method, ..) = classify_captive_portal(
            &CanaryResponse {
                status: None,
                location: None,
                body: String::new(),
            },
            &expect_204(),
        );
        assert!(!detected);
        assert_eq!(method, CaptivePortalMethod::None);
    }
}
