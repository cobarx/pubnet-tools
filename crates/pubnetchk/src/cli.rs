use crate::checks::reliability::{check_reliability, system_ping};
use crate::checks::security::check_security;
use crate::checks::speed::{DEFAULT_TEST_DURATION, check_speed, default_locate};
use crate::checks::topology::check_topology;
#[cfg(not(windows))]
use crate::exec::{cmd, exec_cmd};
use crate::output::renderer::render_report;
use crate::output::reporter::{default_reports_dir, save_html_report, save_report};
use crate::scoring::{ScorableResult, calculate_score};
use crate::types::{CheckResult, CheckStatus, PingTargetLabel, Report};
use clap::{Parser, Subcommand};
use indicatif::ProgressBar;
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const CHECK_NAMES: &[&str] = &["topology", "security", "reliability", "speed"];

#[derive(Parser)]
#[command(name = "pubnetchk", version = VERSION, about = "Audit the public WiFi or network you just joined.")]
struct Cli {
    /// print JSON to stdout, suppress spinners
    #[arg(long)]
    json: bool,
    /// write the report to ~/.pubnetchk/reports/ (off by default)
    #[arg(long)]
    save: bool,
    /// write a plain-language HTML report (the "show your family" view) to
    /// ~/.pubnetchk/reports/ and print its path
    #[arg(long)]
    html: bool,
    /// open the HTML report in your default browser when done (implies --html)
    #[arg(long)]
    open: bool,
    /// comma list of checks to run: topology,security,reliability,speed
    #[arg(long)]
    only: Option<String>,
    /// exit non-zero on Medium/High risk
    #[arg(long)]
    strict: bool,
    /// show per-target reliability detail (loss/min/avg/max/jitter),
    /// not just the condensed local/internet summary
    #[arg(short, long)]
    verbose: bool,
    /// skip the topology check
    #[arg(long)]
    no_topology: bool,
    /// skip the security check
    #[arg(long)]
    no_security: bool,
    /// skip the reliability check
    #[arg(long)]
    no_reliability: bool,
    /// skip the speed check
    #[arg(long)]
    no_speed: bool,
    /// drop a specific external reliability target: google or cloudflare
    /// (repeatable; may not exclude both)
    #[arg(long = "exclude-target")]
    exclude_target: Vec<String>,
    /// seconds per direction (download, upload) for the speed test
    /// (default: 10; not combinable with --quick)
    #[arg(long = "speed-duration", value_name = "SECONDS")]
    speed_duration: Option<u64>,
    /// shorthand for --speed-duration 4 - faster, less accurate
    /// (not combinable with --speed-duration)
    #[arg(short = 'q', long = "quick")]
    quick: bool,
    /// read Wi-Fi channel and signal too (macOS: a ~7s `system_profiler`
    /// call). Default: on when the speed test runs and --quick is off (its
    /// wall time hides the cost), off otherwise. Not combinable with
    /// --no-wifi-detail.
    #[arg(long = "wifi-detail")]
    wifi_detail: bool,
    /// skip the Wi-Fi channel/signal read even when the speed test runs
    /// (SSID and encryption are still read). Not combinable with --wifi-detail.
    #[arg(long = "no-wifi-detail")]
    no_wifi_detail: bool,

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
        Err(format!(
            "--only must be a comma-separated list from: {}",
            CHECK_NAMES.join(", ")
        ))
    } else {
        Ok(valid)
    }
}

pub struct NoFlags {
    pub no_topology: bool,
    pub no_security: bool,
    pub no_reliability: bool,
    pub no_speed: bool,
}

/// `--only` and `--no-<check>` are two different ways of saying the same
/// kind of thing (which checks run) and combining them would mean one
/// silently overrides the other - rejected outright as a usage error
/// instead, matching how `speedtest-cli`-style tools with a similar
/// dual-shape option set (`--server`/`--exclude`) tend to keep the two
/// mutually exclusive rather than defining an interaction order.
fn resolve_check_selection(
    only: &Option<String>,
    no_flags: &NoFlags,
) -> Result<Option<Vec<CheckName>>, String> {
    let any_no = no_flags.no_topology
        || no_flags.no_security
        || no_flags.no_reliability
        || no_flags.no_speed;
    if only.is_some() && any_no {
        return Err("--only cannot be combined with --no-<check> flags".to_string());
    }
    if let Some(v) = only {
        return parse_only(v).map(Some);
    }
    if any_no {
        let mut selected = vec![
            CheckName::Topology,
            CheckName::Security,
            CheckName::Reliability,
            CheckName::Speed,
        ];
        if no_flags.no_topology {
            selected.retain(|c| *c != CheckName::Topology);
        }
        if no_flags.no_security {
            selected.retain(|c| *c != CheckName::Security);
        }
        if no_flags.no_reliability {
            selected.retain(|c| *c != CheckName::Reliability);
        }
        if no_flags.no_speed {
            selected.retain(|c| *c != CheckName::Speed);
        }
        return Ok(Some(selected));
    }
    Ok(None)
}

/// spec-lite (see resolve_check_selection's doc comment for the sibling
/// mutual-exclusion reasoning): excluding both external targets would
/// leave `internet_reachable` untestable by anything, which is different
/// from "false" - rejected rather than silently producing a misleading
/// result. Mirrors speedtest-cli's `--exclude <server>`, repeatable.
fn parse_exclude_targets(values: &[String]) -> Result<Vec<PingTargetLabel>, String> {
    let mut labels = Vec::new();
    for v in values {
        match v.as_str() {
            "google" => labels.push(PingTargetLabel::GoogleDns),
            "cloudflare" => labels.push(PingTargetLabel::CloudflareDns),
            other => {
                return Err(format!(
                    "--exclude-target: unknown target '{other}' (expected google or cloudflare)"
                ));
            }
        }
    }
    if labels.contains(&PingTargetLabel::GoogleDns)
        && labels.contains(&PingTargetLabel::CloudflareDns)
    {
        return Err("--exclude-target cannot exclude both google and cloudflare - internet reachability would be untestable".to_string());
    }
    Ok(labels)
}

const QUICK_TEST_DURATION: Duration = Duration::from_secs(4);

/// spec: docs/decisions/2026-08-25-configurable-speed-duration.md
/// --speed-duration and --quick are two ways to set the same thing, so
/// combining them is a usage error rather than picking a winner - same
/// shape as resolve_check_selection's --only/--no-<check> conflict.
fn resolve_speed_duration(seconds: Option<u64>, quick: bool) -> Result<Duration, String> {
    if quick && seconds.is_some() {
        return Err("--quick cannot be combined with --speed-duration".to_string());
    }
    if quick {
        return Ok(QUICK_TEST_DURATION);
    }
    match seconds {
        None => Ok(DEFAULT_TEST_DURATION),
        Some(0) => Err("--speed-duration must be at least 1".to_string()),
        Some(s) => Ok(Duration::from_secs(s)),
    }
}

/// `--wifi-detail` and `--no-wifi-detail` are two ways to set the same thing,
/// so combining them is a usage error - same shape as the --only/--no-<check>
/// and --quick/--speed-duration conflicts. With neither, the slow Wi-Fi read
/// follows the speed check: on when a full-length speed test runs (its wall
/// time hides the cost), off under --no-speed / --quick / an --only without
/// speed. See docs/decisions/2026-08-26-macos-wifi-without-airport.md.
fn resolve_wifi_detail(
    wifi_detail: bool,
    no_wifi_detail: bool,
    speed_runs_full: bool,
) -> Result<bool, String> {
    if wifi_detail && no_wifi_detail {
        return Err("--wifi-detail cannot be combined with --no-wifi-detail".to_string());
    }
    if wifi_detail {
        return Ok(true);
    }
    if no_wifi_detail {
        return Ok(false);
    }
    Ok(speed_runs_full)
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

pub struct RunAuditOptions {
    pub only: Option<Vec<CheckName>>,
    pub quiet: bool,
    pub exclude_targets: Vec<PingTargetLabel>,
    pub speed_duration: Duration,
    pub wifi_detail: bool,
}

pub async fn run_audit(options: RunAuditOptions) -> Report {
    #[cfg(target_os = "linux")]
    let probe = crate::platform::linux::LinuxProbe;
    #[cfg(target_os = "macos")]
    let probe = crate::platform::macos::MacProbe;
    #[cfg(target_os = "windows")]
    let probe = crate::platform::windows::WindowsProbe;

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
        check_topology(&probe).await
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
                check_security(iface.as_deref(), &probe, &http_client, options.wifi_detail).await
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

fn now_iso8601() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("formatting the current time as RFC3339 should never fail")
}

async fn run_command(cli: &Cli) -> i32 {
    let no_flags = NoFlags {
        no_topology: cli.no_topology,
        no_security: cli.no_security,
        no_reliability: cli.no_reliability,
        no_speed: cli.no_speed,
    };
    let only = match resolve_check_selection(&cli.only, &no_flags) {
        Ok(only) => only,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let exclude_targets = match parse_exclude_targets(&cli.exclude_target) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let speed_duration = match resolve_speed_duration(cli.speed_duration, cli.quick) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let speed_runs_full = !cli.quick
        && only
            .as_ref()
            .is_none_or(|checks| checks.contains(&CheckName::Speed));
    let wifi_detail =
        match resolve_wifi_detail(cli.wifi_detail, cli.no_wifi_detail, speed_runs_full) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        };

    let report = run_audit(RunAuditOptions {
        only,
        quiet: cli.json,
        exclude_targets,
        speed_duration,
        wifi_detail,
    })
    .await;

    if cli.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("Report serialization should never fail")
        );
    } else {
        println!("{}", render_report(&report, cli.verbose));
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

    // --open implies --html: opening a report you never generated is
    // meaningless, so the friendlier reading is "generate it, then open it".
    if cli.html || cli.open {
        match save_html_report(&report, &default_reports_dir()).await {
            Ok(path) => {
                if !cli.json {
                    println!("HTML report: {}", path.display());
                }
                if cli.open {
                    open_in_browser(&path).await;
                }
            }
            Err(e) => eprintln!("Failed to save HTML report: {e}"),
        }
    }

    if cli.strict
        && matches!(
            report.score.level,
            crate::types::RiskLevel::Medium | crate::types::RiskLevel::High
        )
    {
        1
    } else {
        0
    }
}

/// Hand the report file to the desktop's default handler — `xdg-open` on
/// Linux, `open` on macOS, `explorer` on Windows. All three fork the real
/// application and return quickly, so this never blocks on the browser
/// staying open. Uses `tokio::process::Command` directly rather than the
/// `exec` wrapper, since that wrapper isn't in scope on Windows (where the
/// checks don't shell out). Failure to launch is reported without failing the
/// run — the file is already written and its path was printed.
async fn open_in_browser(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let opener = "xdg-open";

    match tokio::process::Command::new(opener)
        .arg(path)
        .status()
        .await
    {
        // explorer.exe returns exit code 1 even on success, so on Windows a
        // launched process is treated as good enough; elsewhere a non-zero
        // exit is a real failure worth surfacing.
        Ok(status) if status.success() || cfg!(windows) => {}
        Ok(status) => {
            eprintln!(
                "Could not open the report ({opener} exited {status}). Open it yourself: {}",
                path.display()
            )
        }
        Err(_) => eprintln!(
            "Could not find '{opener}' to open the report. Open it yourself: {}",
            path.display()
        ),
    }
}

#[cfg(not(windows))]
async fn detect_asciinema_version() -> Option<u32> {
    let which = exec_cmd(cmd(&["which", "asciinema"])).await.ok()?;
    if which.exit_code != Some(0) {
        return None;
    }
    let version_result = exec_cmd(cmd(&["asciinema", "--version"])).await.ok()?;
    let re = regex::Regex::new(r"(\d+)\.\d+\.\d+").ok()?;
    let major = re
        .captures(&version_result.stdout)
        .and_then(|c| c[1].parse::<u32>().ok());
    match major {
        Some(2) => Some(2),
        Some(3) => Some(3),
        _ => Some(1),
    }
}

async fn record_command() -> i32 {
    #[cfg(windows)]
    {
        eprintln!(
            "`pubnetchk record` wraps the run in asciinema, which has no Windows build. \
             Use Windows Terminal's own session recording, or run pubnetchk under WSL."
        );
        1
    }
    #[cfg(not(windows))]
    {
        record_command_unix().await
    }
}

#[cfg(not(windows))]
async fn record_command_unix() -> i32 {
    let Some(version) = detect_asciinema_version().await else {
        eprintln!("asciinema is not installed. Install it with: sudo pacman -S asciinema");
        return 1;
    };

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let timestamp = now_iso8601().replace(':', "-");
    let timestamp = timestamp.split('.').next().unwrap_or(&timestamp);
    let recordings_dir = format!("{home}/.pubnetchk/recordings");
    let path = format!("{recordings_dir}/{timestamp}.cast");
    let _ = exec_cmd(cmd(&["mkdir", "-p", &recordings_dir])).await;

    let args: Vec<String> = if version >= 3 {
        vec![
            "rec".to_string(),
            "--output".to_string(),
            path,
            "--".to_string(),
            "pubnetchk".to_string(),
        ]
    } else {
        vec![
            "rec".to_string(),
            path,
            "--".to_string(),
            "pubnetchk".to_string(),
        ]
    };

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

    // --- resolve_check_selection ---

    fn no_flags() -> NoFlags {
        NoFlags {
            no_topology: false,
            no_security: false,
            no_reliability: false,
            no_speed: false,
        }
    }

    #[test]
    fn no_only_no_no_flags_runs_everything() {
        let result = resolve_check_selection(&None, &no_flags()).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn only_alone_still_works_as_before() {
        let result =
            resolve_check_selection(&Some("topology,speed".to_string()), &no_flags()).unwrap();
        assert_eq!(result, Some(vec![CheckName::Topology, CheckName::Speed]));
    }

    #[test]
    fn no_flags_alone_exclude_from_the_full_set() {
        let flags = NoFlags {
            no_security: true,
            no_speed: true,
            ..no_flags()
        };
        let result = resolve_check_selection(&None, &flags).unwrap();
        assert_eq!(
            result,
            Some(vec![CheckName::Topology, CheckName::Reliability])
        );
    }

    #[test]
    fn only_and_no_flags_together_is_a_usage_error() {
        let flags = NoFlags {
            no_speed: true,
            ..no_flags()
        };
        assert!(resolve_check_selection(&Some("topology".to_string()), &flags).is_err());
    }

    // --- parse_exclude_targets ---

    #[test]
    fn parse_exclude_targets_accepts_known_names() {
        let result = parse_exclude_targets(&["google".to_string()]).unwrap();
        assert_eq!(result, vec![PingTargetLabel::GoogleDns]);
    }

    #[test]
    fn parse_exclude_targets_rejects_unknown_name() {
        assert!(parse_exclude_targets(&["bogus".to_string()]).is_err());
    }

    #[test]
    fn parse_exclude_targets_rejects_excluding_both() {
        assert!(parse_exclude_targets(&["google".to_string(), "cloudflare".to_string()]).is_err());
    }

    #[test]
    fn parse_exclude_targets_empty_is_fine() {
        assert_eq!(parse_exclude_targets(&[]).unwrap(), vec![]);
    }

    // --- resolve_speed_duration ---

    #[test]
    fn neither_flag_is_the_default_duration() {
        assert_eq!(
            resolve_speed_duration(None, false).unwrap(),
            DEFAULT_TEST_DURATION
        );
    }

    #[test]
    fn quick_is_the_quick_preset() {
        assert_eq!(
            resolve_speed_duration(None, true).unwrap(),
            QUICK_TEST_DURATION
        );
    }

    #[test]
    fn explicit_seconds_is_used_as_given() {
        assert_eq!(
            resolve_speed_duration(Some(7), false).unwrap(),
            Duration::from_secs(7)
        );
    }

    #[test]
    fn zero_seconds_is_rejected() {
        assert!(resolve_speed_duration(Some(0), false).is_err());
    }

    #[test]
    fn quick_and_explicit_seconds_together_is_a_usage_error() {
        assert!(resolve_speed_duration(Some(5), true).is_err());
    }

    // --- resolve_wifi_detail ---

    #[test]
    fn wifi_detail_follows_the_speed_check_by_default() {
        assert!(resolve_wifi_detail(false, false, true).unwrap());
        assert!(!resolve_wifi_detail(false, false, false).unwrap());
    }

    #[test]
    fn explicit_wifi_detail_flags_override_the_speed_check() {
        assert!(resolve_wifi_detail(true, false, false).unwrap());
        assert!(!resolve_wifi_detail(false, true, true).unwrap());
    }

    #[test]
    fn both_wifi_detail_flags_together_is_a_usage_error() {
        assert!(resolve_wifi_detail(true, true, true).is_err());
    }
}
