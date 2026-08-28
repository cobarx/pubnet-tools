use clap::Parser;
use pubnet_platform::platform::PlatformProbe;
use pubnet_platform::types::{AuthMode, BssEntry};

#[cfg(target_os = "windows")]
use pubnet_platform::platform::windows::WindowsProbe as Probe;
#[cfg(target_os = "linux")]
use pubnet_platform::platform::linux::LinuxProbe as Probe;
#[cfg(target_os = "macos")]
use pubnet_platform::platform::macos::MacProbe as Probe;

#[derive(Parser)]
#[command(
    name = "pubnetdiag",
    version,
    about = "Scan visible Wi-Fi APs and flag WPA2+WPA3 transition-mode issues."
)]
struct Cli {
    /// Only show APs matching this SSID.
    ssid: Option<String>,

    /// Force WPA2-PSK for the target SSID and reconnect.
    #[arg(long)]
    repair: bool,
}

fn auth_label(mode: AuthMode) -> &'static str {
    match mode {
        AuthMode::Psk => "WPA2-PSK",
        AuthMode::Sae => "WPA3-SAE",
        AuthMode::SaeTransition => "Transition",
        AuthMode::Unknown => "Unknown",
    }
}

fn band_str(band: Option<f64>) -> String {
    match band {
        Some(b) => format!("{b:.1}"),
        None => "-".to_string(),
    }
}

fn channel_str(ch: Option<u32>) -> String {
    match ch {
        Some(c) => c.to_string(),
        None => "-".to_string(),
    }
}

fn print_table(entries: &[&BssEntry]) {
    let ssid_w = entries
        .iter()
        .map(|e| e.ssid.as_deref().unwrap_or("(hidden)").len())
        .max()
        .unwrap_or(4)
        .max("SSID".len());
    let auth_w = entries
        .iter()
        .map(|e| auth_label(e.auth_mode).len())
        .max()
        .unwrap_or(4)
        .max("Auth".len());

    println!(
        "  {:<ssid_w$}  {:<17}  {:<auth_w$}  {:<4}  {:<3}  Signal",
        "SSID", "BSSID", "Auth", "Band", "Ch",
    );
    let sep_len = 2 + ssid_w + 2 + 17 + 2 + auth_w + 2 + 4 + 2 + 3 + 2 + 6;
    println!("  {}", "─".repeat(sep_len));

    for e in entries {
        let marker = if e.auth_mode == AuthMode::SaeTransition { "⚠" } else { " " };
        let ssid = e.ssid.as_deref().unwrap_or("(hidden)");
        let connected = if e.is_connected { "  [connected]" } else { "" };
        println!(
            "{marker} {ssid:<ssid_w$}  {bssid:<17}  {auth:<auth_w$}  {band:<4}  {ch:<3}  {sig:>3}%{connected}",
            bssid = e.bssid,
            auth = auth_label(e.auth_mode),
            band = band_str(e.band),
            ch = channel_str(e.channel),
            sig = e.signal,
        );
    }
}

const FINDING: &str = "\
Warning: WPA2+WPA3 transition mode detected

  This AP accepts both WPA2 and WPA3 clients (transition mode). Some drivers —
  including Intel Wireless-AC 9000-series on Windows — fail the WPA3 handshake
  against such APs and report \"bad password\" even with the correct passphrase.

  To connect: run `pubnetdiag --repair <SSID>` to force WPA2 and connect, or
  manually add a WPA2-PSK profile:
    netsh wlan add profile filename=profile.xml";

async fn do_repair(ssid: &str, passphrase: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        return pubnet_platform::platform::windows::repair_wpa2(ssid, passphrase).await;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (ssid, passphrase);
        return Err("--repair is not yet supported on this platform.".to_string());
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let probe = Probe;

    let entries = match probe.scan_bss_list().await {
        // spec: pubnetdiag-scan#S3
        None => {
            eprintln!("No Wi-Fi adapter found or adapter is disabled.");
            std::process::exit(2);
        }
        Some(e) => e,
    };

    if cli.repair {
        let target = match cli.ssid.as_deref() {
            Some(s) => s,
            None => {
                eprintln!("--repair requires a target SSID: pubnetdiag --repair <SSID>");
                std::process::exit(1);
            }
        };

        let matching: Vec<&BssEntry> = entries
            .iter()
            .filter(|e| e.ssid.as_deref() == Some(target))
            .collect();

        if matching.is_empty() {
            println!("'{}' not found.", target);
            if !entries.is_empty() {
                println!();
                println!("Visible networks:");
                for e in &entries {
                    println!("  {}", e.ssid.as_deref().unwrap_or("(hidden)"));
                }
            }
            std::process::exit(0);
        }

        // spec: pubnetdiag-scan#S10
        if !matching.iter().any(|e| e.auth_mode == AuthMode::SaeTransition) {
            println!("No repair needed for '{}' — not in transition mode.", target);
            std::process::exit(0);
        }

        // spec: pubnetdiag-scan#S9, #S11
        let passphrase = match rpassword::prompt_password(format!("Passphrase for '{}': ", target))
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Could not read passphrase: {e}");
                std::process::exit(1);
            }
        };

        match do_repair(target, &passphrase).await {
            Ok(()) => {
                println!("Connected to '{}'.", target);
                std::process::exit(0);
            }
            Err(reason) => {
                eprintln!("Repair failed: {reason}");
                std::process::exit(1);
            }
        }
    }

    // --- scan-only path ---

    // spec: pubnetdiag-scan#S4
    if entries.is_empty() {
        println!("No networks found.");
        std::process::exit(0);
    }

    let (displayed, has_transition) = if let Some(target) = &cli.ssid {
        let matched: Vec<&BssEntry> = entries
            .iter()
            .filter(|e| e.ssid.as_deref() == Some(target.as_str()))
            .collect();

        if matched.is_empty() {
            // spec: pubnetdiag-scan#S8
            println!("'{}' not found.", target);
            println!();
            println!("Visible networks:");
            for e in &entries {
                println!("  {}", e.ssid.as_deref().unwrap_or("(hidden)"));
            }
            std::process::exit(0);
        }

        let transition = matched.iter().any(|e| e.auth_mode == AuthMode::SaeTransition);
        (matched, transition)
    } else {
        let transition = entries.iter().any(|e| e.auth_mode == AuthMode::SaeTransition);
        (entries.iter().collect(), transition)
    };

    // spec: pubnetdiag-scan#S1, #S2, #S5, #S6, #S7
    print_table(&displayed);

    if has_transition {
        println!();
        println!("{FINDING}");
        std::process::exit(1);
    }

    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ssid: Option<&str>, auth: AuthMode, signal: u32, connected: bool) -> BssEntry {
        BssEntry {
            ssid: ssid.map(|s| s.to_string()),
            bssid: "AA:BB:CC:DD:EE:FF".to_string(),
            auth_mode: auth,
            band: Some(2.4),
            channel: Some(6),
            signal,
            is_connected: connected,
        }
    }

    #[test]
    fn auth_labels_match_spec() {
        assert_eq!(auth_label(AuthMode::Psk), "WPA2-PSK");
        assert_eq!(auth_label(AuthMode::Sae), "WPA3-SAE");
        assert_eq!(auth_label(AuthMode::SaeTransition), "Transition");
        assert_eq!(auth_label(AuthMode::Unknown), "Unknown");
    }

    #[test]
    fn band_and_channel_fallback_to_dash() {
        assert_eq!(band_str(None), "-");
        assert_eq!(band_str(Some(5.0)), "5.0");
        assert_eq!(channel_str(None), "-");
        assert_eq!(channel_str(Some(36)), "36");
    }

    #[test]
    fn ssid_filter_matches_exact() {
        let entries = vec![
            entry(Some("HomeNet"), AuthMode::Psk, 80, false),
            entry(Some("TargetNet"), AuthMode::SaeTransition, 70, false),
        ];
        let matched: Vec<&BssEntry> = entries
            .iter()
            .filter(|e| e.ssid.as_deref() == Some("TargetNet"))
            .collect();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].auth_mode, AuthMode::SaeTransition);
    }

    #[test]
    fn transition_detection_across_entries() {
        let entries = vec![
            entry(Some("Safe"), AuthMode::Psk, 90, false),
            entry(Some("Danger"), AuthMode::SaeTransition, 60, true),
        ];
        assert!(entries.iter().any(|e| e.auth_mode == AuthMode::SaeTransition));
    }

    #[test]
    fn no_transition_when_all_psk_or_sae() {
        let entries = vec![
            entry(Some("A"), AuthMode::Psk, 80, false),
            entry(Some("B"), AuthMode::Sae, 70, false),
        ];
        assert!(!entries.iter().any(|e| e.auth_mode == AuthMode::SaeTransition));
    }

    #[test]
    // spec: pubnetdiag-scan#S10
    fn repair_not_needed_when_no_transition() {
        let entries = vec![entry(Some("attinternet"), AuthMode::Psk, 80, true)];
        let matching: Vec<&BssEntry> = entries
            .iter()
            .filter(|e| e.ssid.as_deref() == Some("attinternet"))
            .collect();
        assert!(!matching.iter().any(|e| e.auth_mode == AuthMode::SaeTransition));
    }

    #[test]
    // spec: pubnetdiag-scan#S9
    fn repair_needed_when_transition_present() {
        let entries = vec![entry(Some("attinternet"), AuthMode::SaeTransition, 70, false)];
        let matching: Vec<&BssEntry> = entries
            .iter()
            .filter(|e| e.ssid.as_deref() == Some("attinternet"))
            .collect();
        assert!(matching.iter().any(|e| e.auth_mode == AuthMode::SaeTransition));
    }
}
