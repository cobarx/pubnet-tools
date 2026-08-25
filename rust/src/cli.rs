//! Port of src/cli.ts: clap setup, single-spinner orchestration ("Analyzing...",
//! "All checks passed" / only-the-issues summary), --json/--save/--only/--strict,
//! and the `record` subcommand.

use crate::checks::reliability::check_reliability;
use crate::checks::security::check_security;
use crate::checks::speed::{check_speed, default_locate};
use crate::checks::topology::check_topology;
use crate::exec::{cmd, exec_cmd};
use crate::output::renderer::render_report;
use crate::output::reporter::{default_reports_dir, save_report};
use crate::scoring::{calculate_score, ScorableResult};
use crate::types::{CheckResult, CheckStatus, Report};
use clap::{Parser, Subcommand};
use indicatif::ProgressBar;
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const CHECK_NAMES: &[&str] = &["topology", "security", "reliability", "speed"];

#[derive(Parser)]
#[command(name = "conncheck", version = VERSION, about = "Audit the public WiFi or network you just joined.")]
struct Cli {
    /// print JSON to stdout, suppress spinners
    #[arg(long)]
    json: bool,
    /// write the report to ~/.conncheck/reports/ (off by default)
    #[arg(long)]
    save: bool,
    /// comma list of checks to run: topology,security,reliability,speed
    #[arg(long)]
    only: Option<String>,
    /// exit non-zero on Medium/High risk
    #[arg(long)]
    strict: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// wrap a full run in asciinema for session capture
    Record,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckName {
    Topology,
    Security,
    Reliability,
    Speed,
}

fn parse_only(value: &str) -> Result<Vec<CheckName>, String> {
    let valid: Vec<CheckName> = value
        .split(',')
        .map(str::trim)
        .filter_map(|s| match s {
            "topology" => Some(CheckName::Topology),
            "security" => Some(CheckName::Security),
            "reliability" => Some(CheckName::Reliability),
            "speed" => Some(CheckName::Speed),
            _ => None,
        })
        .collect();
    if valid.is_empty() {
        Err(format!("--only must be a comma-separated list from: {}", CHECK_NAMES.join(", ")))
    } else {
        Ok(valid)
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
    } else if statuses.iter().any(|s| matches!(s, CheckStatus::Degraded | CheckStatus::Skipped)) {
        CheckStatus::Degraded
    } else {
        CheckStatus::Ok
    }
}

/// A run passing cleanly is the common case - naming all four as "ok"
/// every time is noise, not information. Only the ones with something to
/// report get named; a clean run gets one terse line instead.
fn summarize_run_status(results: &[(&str, CheckStatus)]) -> String {
    let issues: Vec<String> =
        results.iter().filter(|(_, s)| *s != CheckStatus::Ok).map(|(label, s)| format!("{label}: {}", s.as_str())).collect();
    if issues.is_empty() {
        "All checks passed".to_string()
    } else {
        issues.join(" · ")
    }
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

#[derive(Default)]
pub struct RunAuditOptions {
    pub only: Option<Vec<CheckName>>,
    pub quiet: bool,
}

pub async fn run_audit(options: RunAuditOptions) -> Report {
    let should_run = |name: CheckName| options.only.as_ref().is_none_or(|only| only.contains(&name));
    let will_run_topology = should_run(CheckName::Topology);
    let concurrent_names: Vec<CheckName> =
        [CheckName::Security, CheckName::Reliability, CheckName::Speed].into_iter().filter(|n| should_run(*n)).collect();

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

    let topology = if will_run_topology { check_topology(&exec_cmd).await } else { excluded_result("topology") };

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
                check_security(iface.as_deref(), &exec_cmd, &http_client).await
            } else {
                excluded_result("security")
            }
        },
        async {
            if should_run(CheckName::Reliability) {
                check_reliability(gateway_ip.as_deref(), &exec_cmd).await
            } else {
                excluded_result("reliability")
            }
        },
        async {
            if should_run(CheckName::Speed) {
                check_speed(&default_locate).await
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
        finish_spinner(sp, combined_status(&statuses), &summarize_run_status(&results));
    }

    let score = calculate_score(&[
        ScorableResult { status: topology.status, findings: &topology.findings },
        ScorableResult { status: security.status, findings: &security.findings },
        ScorableResult { status: reliability.status, findings: &reliability.findings },
        ScorableResult { status: speed.status, findings: &speed.findings },
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

fn now_iso8601() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("formatting the current time as RFC3339 should never fail")
}

async fn run_command(cli: &Cli) -> i32 {
    let only = match &cli.only {
        Some(v) => match parse_only(v) {
            Ok(names) => Some(names),
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        },
        None => None,
    };

    let report = run_audit(RunAuditOptions { only, quiet: cli.json }).await;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report).expect("Report serialization should never fail"));
    } else {
        println!("{}", render_report(&report));
    }

    if cli.save {
        match save_report(&report, &default_reports_dir()).await {
            Ok(path) => {
                if !cli.json {
                    println!("Saved report to {}", path.display());
                }
            }
            Err(e) => eprintln!("Failed to save report: {e}"),
        }
    }

    if cli.strict && matches!(report.score.level, crate::types::RiskLevel::Medium | crate::types::RiskLevel::High) {
        1
    } else {
        0
    }
}

async fn detect_asciinema_version() -> Option<u32> {
    let which = exec_cmd(cmd(&["which", "asciinema"])).await.ok()?;
    if which.exit_code != Some(0) {
        return None;
    }
    let version_result = exec_cmd(cmd(&["asciinema", "--version"])).await.ok()?;
    let re = regex::Regex::new(r"(\d+)\.\d+\.\d+").ok()?;
    let major = re.captures(&version_result.stdout).and_then(|c| c[1].parse::<u32>().ok());
    match major {
        Some(2) => Some(2),
        Some(3) => Some(3),
        _ => Some(1),
    }
}

async fn record_command() -> i32 {
    let Some(version) = detect_asciinema_version().await else {
        eprintln!("asciinema is not installed. Install it with: sudo pacman -S asciinema");
        return 1;
    };

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let timestamp = now_iso8601().replace(':', "-");
    let timestamp = timestamp.split('.').next().unwrap_or(&timestamp);
    let recordings_dir = format!("{home}/.conncheck/recordings");
    let path = format!("{recordings_dir}/{timestamp}.cast");
    let _ = exec_cmd(cmd(&["mkdir", "-p", &recordings_dir])).await;

    let args: Vec<String> =
        if version >= 3 { vec!["rec".to_string(), "--output".to_string(), path, "--".to_string(), "conncheck".to_string()] } else { vec!["rec".to_string(), path, "--".to_string(), "conncheck".to_string()] };

    let status = tokio::process::Command::new("asciinema")
        .args(&args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await;

    match status {
        Ok(s) => s.code().unwrap_or(0),
        Err(_) => 1,
    }
}

pub async fn run() {
    let cli = Cli::parse();

    let exit_code = match &cli.command {
        Some(Commands::Record) => record_command().await,
        None => run_command(&cli).await,
    };

    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_only_accepts_valid_names() {
        let result = parse_only("topology,security").unwrap();
        assert_eq!(result, vec![CheckName::Topology, CheckName::Security]);
    }

    #[test]
    fn parse_only_rejects_all_invalid_names() {
        assert!(parse_only("bogus").is_err());
    }

    #[test]
    fn parse_only_trims_whitespace() {
        let result = parse_only(" topology , speed ").unwrap();
        assert_eq!(result, vec![CheckName::Topology, CheckName::Speed]);
    }

    #[test]
    fn summarize_run_status_all_ok_is_terse() {
        let results = [("Topology", CheckStatus::Ok), ("Security", CheckStatus::Ok)];
        assert_eq!(summarize_run_status(&results), "All checks passed");
    }

    #[test]
    fn summarize_run_status_only_names_issues() {
        let results = [("Topology", CheckStatus::Ok), ("Reliability", CheckStatus::Degraded)];
        assert_eq!(summarize_run_status(&results), "Reliability: degraded");
    }

    #[test]
    fn combined_status_worst_of() {
        assert_eq!(combined_status(&[CheckStatus::Ok, CheckStatus::Degraded]), CheckStatus::Degraded);
        assert_eq!(combined_status(&[CheckStatus::Ok, CheckStatus::Failed, CheckStatus::Degraded]), CheckStatus::Failed);
        assert_eq!(combined_status(&[CheckStatus::Ok, CheckStatus::Ok]), CheckStatus::Ok);
    }
}
