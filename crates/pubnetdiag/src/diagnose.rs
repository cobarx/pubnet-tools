use pubnet_platform::types::{AuthMode, BssEntry};

fn event_label(id: u32) -> &'static str {
    match id {
        8001 => "connected",
        8002 => "connection failed",
        8003 => "disconnected",
        11004 => "security stopped",
        11005 => "security succeeded",
        11006 => "security failed",
        11010 => "security started",
        _ => "wlan event",
    }
}


/// Run the diagnose flow for `ssid`: show the current BSS picture, then the
/// most recent WLAN connection events from the Windows event log.
pub fn run_diagnose(ssid: &str, entries: &[BssEntry]) {
    // --- BSS state -----------------------------------------------------------
    let bsses: Vec<&BssEntry> = entries
        .iter()
        .filter(|e| e.ssid.as_deref() == Some(ssid))
        .collect();

    if bsses.is_empty() {
        println!("'{ssid}' is not currently visible.");
    } else {
        let transition = bsses.iter().any(|e| e.auth_mode == AuthMode::SaeTransition);
        println!("Current BSS state for '{ssid}':");
        for e in &bsses {
            let auth = match e.auth_mode {
                AuthMode::Psk => "WPA2-PSK",
                AuthMode::Sae => "WPA3-SAE",
                AuthMode::SaeTransition => "Transition (WPA2+WPA3)",
                AuthMode::Unknown => "Unknown",
            };
            let band = e.band.map(|b| format!("{b:.1} GHz")).unwrap_or_else(|| "-".to_string());
            let ch = e.channel.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string());
            println!("  {} | {} | band {} ch {} | signal {}%", e.bssid, auth, band, ch, e.signal);
        }
        if transition {
            println!();
            println!("  WPA2+WPA3 transition mode detected — run `pubnetdiag {ssid} --repair` to fix.");
        }
    }

    println!();

    // --- Event log -----------------------------------------------------------
    println!("Recent WLAN connection events for '{ssid}':");

    #[cfg(target_os = "windows")]
    {
        use pubnet_platform::platform::windows::{query_wlan_events, wlan_reason_code_to_string};
        let events = query_wlan_events(ssid, 20);
        if events.is_empty() {
            println!("  No recent events found in Microsoft-Windows-WLAN-AutoConfig/Operational.");
            println!("  (Try connecting to '{ssid}' and running --diagnose again.)");
        } else {
            println!("  {:<19}  {:<5}  {}", "Timestamp", "ID", "Description");
            println!("  {}  {}  {}", "─".repeat(19), "─".repeat(5), "─".repeat(48));
            for ev in &events {
                let label = event_label(ev.event_id);

                // Prefer the human-readable hint; fall back to decoded reason code.
                let detail = ev.hint.as_deref()
                    .map(|h| format!(" — {h}"))
                    .or_else(|| ev.reason_code.filter(|&c| c != 0).map(|c| format!(" — {}", wlan_reason_code_to_string(c))))
                    .unwrap_or_default();

                println!("  {:<19}  {:>5}  {}{}", ev.timestamp, ev.event_id, label, detail);
            }

            let has_psk_mismatch = events.iter().any(|e| {
                e.hint.as_deref().map(|h| h.to_ascii_lowercase().contains("psk mismatch")).unwrap_or(false)
                    || e.reason_code == Some(0x48005)
                    || e.reason_code == Some(0x10010)
            });
            let transition = bsses.iter().any(|e| e.auth_mode == AuthMode::SaeTransition);

            if has_psk_mismatch && transition {
                println!();
                println!(
                    "  Diagnosis: PSK mismatch on a transition-mode AP matches the Intel AC 9560\n\
                     \x20  v23 SAE bug. Run `pubnetdiag {ssid} --repair` to force WPA2 and connect."
                );
            } else if has_psk_mismatch {
                println!();
                println!("  PSK mismatch detected — verify the passphrase is correct.");
                println!("  If the AP broadcasts WPA2+WPA3 transition mode, `--repair` may help.");
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        println!("  WLAN event log diagnostics are only available on Windows.");
    }
}
