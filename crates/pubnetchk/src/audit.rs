//! Audit orchestration — platform-neutral. Runs the four checks against a
//! `PlatformProbe`, drives the shared spinner, and assembles the `Report`.
//!
//! Split out of `cli.rs` so it compiles for Android (where `cli.rs`, with its
//! clap surface and desktop-only `run`/`record` paths, is not built): the
//! Android front-end calls `run_audit_with_probe` with a `SnapshotProbe`.

use crate::checks::reliability::{check_reliability, system_ping};
use crate::checks::security::check_security;
use crate::checks::speed::{check_speed, default_locate};
use crate::checks::topology::check_topology;
use crate::platform::PlatformProbe;
use crate::scoring::{ScorableResult, calculate_score};
use crate::types::{CheckResult, CheckStatus, PingTargetLabel, Report};
use indicatif::ProgressBar;
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckName {
    Topology,
    Security,
    Reliability,
    Speed,
}

pub struct RunAuditOptions {
    pub only: Option<Vec<CheckName>>,
    pub quiet: bool,
    pub exclude_targets: Vec<PingTargetLabel>,
    pub speed_duration: Duration,
    pub wifi_detail: bool,
}

fn excluded_result<T>(name: &str) -> CheckResult<T> {
    CheckResult {
        name: name.to_string(),
        status: CheckStatus::Skipped,
        data: None,
        errors: vec!["Excluded by --only".to_string()],
        findings: vec![],
        duration_ms: 0,
    }
}

fn finish_spinner(spinner: &ProgressBar, status: CheckStatus, text: &str) {
    let symbol = match status {
        CheckStatus::Ok => console::style("✔").green(),
        CheckStatus::Degraded | CheckStatus::Skipped => console::style("⚠").yellow(),
        CheckStatus::Failed => console::style("✖").red(),
    };
    spinner.finish_with_message(format!("{symbol} {text}"));
}

/// Worst-of ordering so one shared spinner can reflect several checks' outcomes.
fn combined_status(statuses: &[CheckStatus]) -> CheckStatus {
    if statuses.contains(&CheckStatus::Failed) {
        CheckStatus::Failed
    } else if statuses
        .iter()
        .any(|s| matches!(s, CheckStatus::Degraded | CheckStatus::Skipped))
    {
        CheckStatus::Degraded
    } else {
        CheckStatus::Ok
    }
}

/// A run passing cleanly is the common case - naming all four as "ok"
/// every time is noise, not information. Only the ones with something to
/// report get named; a clean run gets one terse line instead.
fn summarize_run_status(results: &[(&str, CheckStatus)]) -> String {
    let issues: Vec<String> = results
        .iter()
        .filter(|(_, s)| *s != CheckStatus::Ok)
        .map(|(label, s)| format!("{label}: {}", s.as_str()))
        .collect();
    if issues.is_empty() {
        "All checks passed".to_string()
    } else {
        issues.join(" · ")
    }
}

/// Run the audit against the host's own `PlatformProbe`. The binary entry
/// point; not compiled for Android, where there is no native probe (the app
/// calls `run_audit_with_probe` with a `SnapshotProbe` instead).
#[cfg(not(target_os = "android"))]
pub async fn run_audit(options: RunAuditOptions) -> Report {
    #[cfg(target_os = "linux")]
    let probe = crate::platform::linux::LinuxProbe;
    #[cfg(target_os = "macos")]
    let probe = crate::platform::macos::MacProbe;
    #[cfg(target_os = "windows")]
    let probe = crate::platform::windows::WindowsProbe;

    run_audit_with_probe(&probe, options).await
}

/// Run the audit against a caller-supplied probe. Same behavior as `run_audit`
/// minus the platform's probe selection: the CLI passes its native probe, the
/// Android front-end passes a `pubnet_platform::platform::snapshot::SnapshotProbe`.
/// The progress spinner is still driven here, gated by `options.quiet` (the
/// Android caller passes `quiet: true`).
pub async fn run_audit_with_probe<P: PlatformProbe>(probe: &P, options: RunAuditOptions) -> Report {
    let should_run = |name: CheckName| {
        options
            .only
            .as_ref()
            .is_none_or(|only| only.contains(&name))
    };
    let will_run_topology = should_run(CheckName::Topology);
    let concurrent_names: Vec<CheckName> = [
        CheckName::Security,
        CheckName::Reliability,
        CheckName::Speed,
    ]
    .into_iter()
    .filter(|n| should_run(*n))
    .collect();

    // One spinner instance for the entire run - its text is updated
    // between phases rather than replaced, matching the TS version's
    // single-ora-instance discipline (multiple live spinners on one
    // stream causes visual corruption).
    let initial_label = if will_run_topology {
        Some("Checking network topology...")
    } else if !concurrent_names.is_empty() {
        Some("Analyzing...")
    } else {
        None
    };
    let spinner = if options.quiet { None } else { initial_label }.map(|label| {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(Duration::from_millis(80));
        pb.set_message(label);
        pb
    });

    let topology = if will_run_topology {
        check_topology(probe).await
    } else {
        excluded_result("topology")
    };

    let gateway_ip = topology.data.as_ref().map(|d| d.gateway.clone());
    let iface = topology.data.as_ref().map(|d| d.interface.clone());

    if let Some(sp) = &spinner
        && will_run_topology
        && !concurrent_names.is_empty()
    {
        sp.set_message("Analyzing...");
    }

    let http_client = reqwest::Client::new();
    let (security, reliability, speed) = tokio::join!(
        async {
            if should_run(CheckName::Security) {
                check_security(iface.as_deref(), probe, &http_client, options.wifi_detail).await
            } else {
                excluded_result("security")
            }
        },
        async {
            if should_run(CheckName::Reliability) {
                check_reliability(
                    gateway_ip.as_deref(),
                    &system_ping,
                    &options.exclude_targets,
                )
                .await
            } else {
                excluded_result("reliability")
            }
        },
        async {
            if should_run(CheckName::Speed) {
                check_speed(&default_locate, options.speed_duration).await
            } else {
                excluded_result("speed")
            }
        },
    );

    if let Some(sp) = &spinner {
        let mut results: Vec<(&str, CheckStatus)> = Vec::new();
        if will_run_topology {
            results.push(("Topology", topology.status));
        }
        if should_run(CheckName::Security) {
            results.push(("Security", security.status));
        }
        if should_run(CheckName::Reliability) {
            results.push(("Reliability", reliability.status));
        }
        if should_run(CheckName::Speed) {
            results.push(("Speed", speed.status));
        }
        let statuses: Vec<CheckStatus> = results.iter().map(|(_, s)| *s).collect();
        finish_spinner(
            sp,
            combined_status(&statuses),
            &summarize_run_status(&results),
        );
    }

    let score = calculate_score(&[
        ScorableResult {
            status: topology.status,
            findings: &topology.findings,
        },
        ScorableResult {
            status: security.status,
            findings: &security.findings,
        },
        ScorableResult {
            status: reliability.status,
            findings: &reliability.findings,
        },
        ScorableResult {
            status: speed.status,
            findings: &speed.findings,
        },
    ]);

    Report {
        version: VERSION.to_string(),
        timestamp: now_iso8601(),
        security,
        speed,
        reliability,
        topology,
        score,
    }
}

/// Current UTC time as an RFC 3339 string. Also used by `cli::record` for the
/// recording filename.
pub fn now_iso8601() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("formatting the current time as RFC3339 should never fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_run_status_all_ok_is_terse() {
        let results = [("Topology", CheckStatus::Ok), ("Security", CheckStatus::Ok)];
        assert_eq!(summarize_run_status(&results), "All checks passed");
    }

    #[test]
    fn summarize_run_status_only_names_issues() {
        let results = [
            ("Topology", CheckStatus::Ok),
            ("Reliability", CheckStatus::Degraded),
        ];
        assert_eq!(summarize_run_status(&results), "Reliability: degraded");
    }

    #[test]
    fn combined_status_worst_of() {
        assert_eq!(
            combined_status(&[CheckStatus::Ok, CheckStatus::Degraded]),
            CheckStatus::Degraded
        );
        assert_eq!(
            combined_status(&[CheckStatus::Ok, CheckStatus::Failed, CheckStatus::Degraded]),
            CheckStatus::Failed
        );
        assert_eq!(
            combined_status(&[CheckStatus::Ok, CheckStatus::Ok]),
            CheckStatus::Ok
        );
    }
}
