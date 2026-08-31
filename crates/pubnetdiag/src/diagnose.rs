use pubnet_platform::types::{AuthMode, BssEntry};

fn event_label(id: u32) -> &'static str {
    match id {
        8001 => "connected",
        8002 => "connection failed",
        8003 => "disconnected",
        11004 => "association failed",
        11005 => "security complete / connected",
        11006 => "connection canceled",
        11010 => "security failure",
        _ => "wlan event",
    }
}

fn interpret_reason(code: u32) -> String {
    match code {
        0x10001 => "unknown reason".to_string(),
        0x10002 => "profile incompatible with network".to_string(),
        0x10005 => "network not visible".to_string(),
        0x1000E => "network not available".to_string(),
        // KEY_MISMATCH: what the Intel AC 9560 reports when SAE handshake fails
        // against a transition-mode AP ("bad password" in the UI).
        0x10010 => "key mismatch — wrong passphrase, or auth protocol rejected by driver".to_string(),
        0x10011 => "user did not respond to prompt".to_string(),
        c => format!("0x{c:X}"),
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
        use pubnet_platform::platform::windows::query_wlan_events;
        let events = query_wlan_events(ssid, 20);
        if events.is_empty() {
            println!("  No recent events found in Microsoft-Windows-WLAN-AutoConfig/Operational.");
            println!("  (Try connecting to '{ssid}' and running --diagnose again.)");
        } else {
            for ev in &events {
                let label = event_label(ev.event_id);
                let reason_part = match ev.reason_code {
                    Some(c) => format!(" — {}", interpret_reason(c)),
                    None => String::new(),
                };
                println!("  {}  {:5}  {}{}", ev.timestamp, ev.event_id, label, reason_part);
            }

            let has_key_mismatch = events
                .iter()
                .any(|e| e.reason_code == Some(0x10010));
            let transition = bsses.iter().any(|e| e.auth_mode == AuthMode::SaeTransition);
            if has_key_mismatch && transition {
                println!();
                println!(
                    "  Diagnosis: key-mismatch failures on a transition-mode AP match the\n\
                     \x20  Intel AC 9560 v23 SAE bug. Run `pubnetdiag {ssid} --repair` to fix."
                );
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        println!("  WLAN event log diagnostics are only available on Windows.");
    }
}
