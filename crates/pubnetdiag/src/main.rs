use clap::Parser;
use pubnet_platform::platform::PlatformProbe;
use pubnet_platform::types::{AuthMode, BssEntry};
use pubnetdiag::{exit_codes, repair::{detect_repairs, find_latest_repair_log, reset_repair}};

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

    /// Detect issues with the target SSID and apply the appropriate fix.
    #[arg(long)]
    repair: bool,

    /// Remove a previously installed repair profile, returning to unfixed state.
    #[arg(long)]
    reset: bool,
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let probe = Probe;

    let entries = match probe.scan_bss_list().await {
        // spec: pubnetdiag-scan#S3
        None => {
            eprintln!("No Wi-Fi adapter found or adapter is disabled.");
            std::process::exit(exit_codes::NO_ADAPTER);
        }
        Some(e) => e,
    };

    if cli.reset {
        let target = match cli.ssid.as_deref() {
            Some(s) => s,
            None => {
                eprintln!("--reset requires a target SSID: pubnetdiag <SSID> --reset");
                std::process::exit(exit_codes::USAGE_ERROR);
            }
        };
        match reset_repair(target).await {
            Ok(()) => {
                println!("Forced WPA2-PSK profile for '{target}' removed. Machine is now in unfixed state.");
            }
            Err(e) if e.contains("no saved profile") => {
                println!("No forced profile found for '{target}' — already in unfixed state.");
                if let Some(log_path) = find_latest_repair_log(target) {
                    let _ = std::fs::remove_file(log_path);
                }
            }
            Err(e) => {
                eprintln!("Reset failed: {e}");
                std::process::exit(exit_codes::REPAIR_FAILED);
            }
        }
        std::process::exit(exit_codes::OK);
    }

    if cli.repair {
        let target = match cli.ssid.as_deref() {
            Some(s) => s,
            None => {
                eprintln!("--repair requires a target SSID: pubnetdiag --repair <SSID>");
                std::process::exit(exit_codes::USAGE_ERROR);
            }
        };

        let ssid_visible = entries.iter().any(|e| e.ssid.as_deref() == Some(target));
        if !ssid_visible {
            println!("'{}' not found.", target);
            if !entries.is_empty() {
                println!();
                println!("Visible networks:");
                for e in &entries {
                    println!("  {}", e.ssid.as_deref().unwrap_or("(hidden)"));
                }
            }
            std::process::exit(exit_codes::OK);
        }

        // spec: pubnetdiag-scan#S10
        let actions = detect_repairs(target, &entries);
        if actions.is_empty() {
            println!("No repair needed for '{}' — no known issues detected.", target);
            std::process::exit(exit_codes::OK);
        }

        for action in &actions {
            println!("Applying: {}", action.description());
            match action.apply().await {
                Ok(()) => println!("Connected to '{}'.", target),
                Err(reason) => {
                    eprintln!("Repair failed: {reason}");
                    std::process::exit(exit_codes::REPAIR_FAILED);
                }
            }
        }
        std::process::exit(exit_codes::OK);
    }

    // --- scan-only path ---

    if entries.is_empty() {
        // spec: pubnetdiag-scan#S4
        println!("No networks found.");
        std::process::exit(exit_codes::OK);
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
            std::process::exit(exit_codes::OK);
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
        // spec: pubnetdiag-scan#S2
        println!();
        println!("{FINDING}");
        std::process::exit(exit_codes::TRANSITION_FOUND);
    }

    std::process::exit(exit_codes::OK);
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
