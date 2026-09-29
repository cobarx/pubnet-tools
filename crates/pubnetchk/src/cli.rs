use crate::audit::{CheckName, RunAuditOptions, VERSION, now_iso8601, run_audit};
use crate::checks::speed::DEFAULT_TEST_DURATION;
#[cfg(not(windows))]
use crate::exec::{cmd, exec_cmd};
use crate::output::renderer::render_report;
use crate::output::reporter::{default_reports_dir, save_html_report, save_report};
use crate::types::PingTargetLabel;
use clap::{Parser, Subcommand};
use std::time::Duration;

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
    /// check whether this terminal can read the Wi-Fi network name, and help
    /// grant Location Services access if not (macOS only — see
    /// docs/specs/permissions-helper.md)
    Permissions {
        /// open System Settings to the Location Services pane
        #[arg(long)]
        open: bool,
        /// ask CoreLocation directly for "when in use" authorization —
        /// pubnetchk isn't a signed .app, so this may not show a prompt at
        /// all (macOS only; see
        /// docs/decisions/2026-09-04-macos-location-authorization-request.md)
        #[arg(long)]
        request: bool,
    },
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

/// The `x-apple.systempreferences:` URL that macOS's `open` accepts to jump
/// straight to the Location Services pane, skipping Privacy & Security's
/// section list.
#[cfg(target_os = "macos")]
const LOCATION_SERVICES_SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_LocationServices";

/// Maps `TERM_PROGRAM` to the name shown in System Settings' app list, so the
/// instructions name the actual terminal rather than a generic "your
/// terminal app". Unrecognized or absent values fall back to that generic
/// phrase — new terminal emulators shouldn't need a code change to get a
/// (still correct, just less specific) instruction.
fn friendly_terminal_name(term_program: Option<&str>) -> String {
    match term_program {
        Some("Apple_Terminal") => "Terminal".to_string(),
        Some("iTerm.app") => "iTerm2".to_string(),
        Some("vscode") => "Visual Studio Code".to_string(),
        Some("WarpTerminal") => "Warp".to_string(),
        Some("ghostty") => "Ghostty".to_string(),
        Some("WezTerm") => "WezTerm".to_string(),
        Some("Hyper") => "Hyper".to_string(),
        _ => "your terminal app".to_string(),
    }
}

/// spec: docs/specs/permissions-helper.md
/// A bare CLI binary (and the terminal hosting it) never appears in System
/// Settings' Location Services list on its own, because neither one calls
/// CoreLocation's authorization API the way a real app does (confirmed
/// empirically: Terminal.app and third-party terminals alike). `--request`
/// (docs/decisions/2026-09-04-macos-location-authorization-request.md) makes
/// pubnetchk itself call that API via an embedded Info.plist section +
/// CoreLocation binding — whether that actually produces a prompt (and, if
/// granted, an unredacted SSID) for a non-bundled process is genuinely
/// uncertain, which is exactly what this exists to find out empirically
/// rather than staying theoretical. See
/// https://github.com/cobarx/pubnet-tools/issues/48.
async fn permissions_command(open: bool, request: bool) -> i32 {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (open, request);
        println!(
            "pubnetchk permissions is a macOS-only helper: only macOS gates the Wi-Fi \
             network name behind Location Services. There's nothing to grant on this platform."
        );
        0
    }
    #[cfg(target_os = "macos")]
    {
        permissions_command_macos(open, request).await
    }
}

#[cfg(target_os = "macos")]
async fn permissions_command_macos(open: bool, request: bool) -> i32 {
    use crate::platform::PlatformProbe;
    use crate::platform::macos::MacProbe;

    if request {
        let status = crate::macos_location::request_when_in_use_authorization(
            std::time::Duration::from_secs(10),
        );
        println!(
            "CoreLocation authorization for pubnetchk: {} (waited up to 10s for a response).",
            crate::macos_location::describe(status)
        );
    }

    let probe = MacProbe;
    let wifi = match probe.default_route().await {
        Some(route) => probe.wifi_info(&route.device, false).await,
        None => None,
    };

    let Some(wifi) = wifi else {
        println!(
            "You're not currently on Wi-Fi (or there's no default route to check). \
             Re-run `pubnetchk permissions` once you're connected to a Wi-Fi network."
        );
        return 0;
    };

    if !wifi.ssid_hidden {
        match &wifi.ssid {
            Some(ssid) => println!(
                "Location Services access is already granted — SSID: {ssid}. Nothing to do."
            ),
            None => println!(
                "The Wi-Fi network name isn't hidden by Location Services here. Nothing to do."
            ),
        }
        return 0;
    }

    let terminal = friendly_terminal_name(std::env::var("TERM_PROGRAM").ok().as_deref());
    println!(
        "The Wi-Fi network name (SSID) is hidden. macOS 15+ only reveals it to a \
         process that holds Location Services authorization — neither {terminal} nor \
         pubnetchk shows up in System Settings' Location Services list on its own, \
         since neither calls CoreLocation's authorization API by default."
    );
    if !request {
        println!(
            "Run `pubnetchk permissions --request` to have pubnetchk ask directly. \
             This is genuinely uncertain to work — historically only a bundled, \
             GUI-capable app could get a real prompt out of macOS, and pubnetchk is a \
             bare binary — but it's now a real attempt, not just theory."
        );
    }
    println!(
        "Also worth trying (unconfirmed, costs nothing): System Settings ▸ Privacy & \
         Security ▸ Location Services ▸ scroll to System Services ▸ Details…, and \
         check whether \"Wi-Fi Networking\" is enabled — a system-wide switch, not a \
         per-app one."
    );
    println!("Tracked at https://github.com/cobarx/pubnet-tools/issues/48.");

    if open {
        match tokio::process::Command::new("open")
            .arg(LOCATION_SERVICES_SETTINGS_URL)
            .status()
            .await
        {
            Ok(status) if status.success() => {
                println!("Opened System Settings to the Location Services pane.")
            }
            Ok(status) => eprintln!(
                "`open` exited {status} — open System Settings ▸ Privacy & Security ▸ \
                 Location Services yourself if you want to check System Services."
            ),
            Err(_) => eprintln!(
                "Could not launch System Settings — open System Settings ▸ Privacy & \
                 Security ▸ Location Services yourself if you want to check System \
                 Services."
            ),
        }
    } else {
        println!("Run `pubnetchk permissions --open` to jump straight to that pane.");
    }

    0
}

pub async fn run() {
    let cli = Cli::parse();

    let exit_code = match &cli.command {
        Some(Commands::Record) => record_command().await,
        Some(Commands::Permissions { open, request }) => {
            permissions_command(*open, *request).await
        }
        None => run_command(&cli).await,
    };

    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: docs/specs/permissions-helper.md (terminal-name detection)
    #[test]
    fn friendly_terminal_name_recognizes_known_terminals() {
        assert_eq!(friendly_terminal_name(Some("Apple_Terminal")), "Terminal");
        assert_eq!(friendly_terminal_name(Some("iTerm.app")), "iTerm2");
        assert_eq!(friendly_terminal_name(Some("vscode")), "Visual Studio Code");
    }

    #[test]
    fn friendly_terminal_name_falls_back_for_unknown_or_absent() {
        assert_eq!(friendly_terminal_name(Some("SomeNewTerminal")), "your terminal app");
        assert_eq!(friendly_terminal_name(None), "your terminal app");
    }

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
