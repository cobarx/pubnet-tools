use pubnet_platform::types::{AuthMode, BssEntry};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub enum RepairAction {
    ForceWpa2Psk { ssid: String },
}

impl RepairAction {
    pub fn description(&self) -> &str {
        match self {
            Self::ForceWpa2Psk { .. } => "force WPA2-PSK profile to bypass SAE handshake failure",
        }
    }

    // spec: pubnetdiag-scan#S9, #S11
    pub async fn apply(&self) -> Result<(), String> {
        match self {
            Self::ForceWpa2Psk { ssid } => {
                let passphrase =
                    rpassword::prompt_password(format!("Passphrase for '{ssid}': "))
                        .map_err(|e| format!("could not read passphrase: {e}"))?;
                #[cfg(target_os = "windows")]
                {
                    let result =
                        pubnet_platform::platform::windows::repair_wpa2(ssid, &passphrase).await;
                    if result.is_ok() {
                        log_repair(ssid, "ForceWpa2Psk");
                    }
                    return result;
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = passphrase;
                    Err("--repair is not yet supported on this platform.".to_string())
                }
            }
        }
    }
}

/// Returns the repairs applicable to `ssid` given the current BSS list.
/// An empty vec means no known issues — no repair is needed.
pub fn detect_repairs(ssid: &str, entries: &[BssEntry]) -> Vec<RepairAction> {
    let mut actions = Vec::new();
    let bsses: Vec<_> = entries
        .iter()
        .filter(|e| e.ssid.as_deref() == Some(ssid))
        .collect();
    if bsses.iter().any(|e| e.auth_mode == AuthMode::SaeTransition) {
        actions.push(RepairAction::ForceWpa2Psk { ssid: ssid.to_string() });
    }
    actions
}

// ---------------------------------------------------------------------------
// Repair log — records what was applied so --reset can undo it
// ---------------------------------------------------------------------------

fn repair_log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    return std::env::var("USERPROFILE")
        .ok()
        .map(|p| PathBuf::from(p).join(".pubnetdiag").join("repairs"));
    #[cfg(not(target_os = "windows"))]
    return std::env::var("HOME")
        .ok()
        .map(|p| PathBuf::from(p).join(".pubnetdiag").join("repairs"));
}

fn sanitize_ssid(ssid: &str) -> String {
    ssid.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}

/// Write a record of the applied repair to `~/.pubnetdiag/repairs/`.
/// Silently no-ops if the directory cannot be created or the file cannot be written.
pub fn log_repair(ssid: &str, action: &str) {
    let Some(dir) = repair_log_dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let fname = format!("{}_{}.json", sanitize_ssid(ssid), ts);
    let escaped = ssid.replace('\\', "\\\\").replace('"', "\\\"");
    let content = format!(
        "{{\"ssid\":\"{escaped}\",\"action\":\"{action}\",\"applied_at\":{ts}}}\n"
    );
    let _ = std::fs::write(dir.join(fname), content);
}

/// Return the path of the most recent repair log entry for `ssid`, if any.
pub fn find_latest_repair_log(ssid: &str) -> Option<PathBuf> {
    let dir = repair_log_dir()?;
    let prefix = sanitize_ssid(ssid) + "_";
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(&prefix) && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();
    entries.sort();
    entries.pop()
}

/// Undo a previous `--repair`: delete the forced WPA2-PSK profile and remove
/// the repair log entry. Returns the machine to unfixed state.
pub async fn reset_repair(ssid: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let result = pubnet_platform::platform::windows::delete_wlan_profile(ssid);
        if let Some(log_path) = find_latest_repair_log(ssid) {
            let _ = std::fs::remove_file(log_path);
        }
        result
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = ssid;
        Err("--reset is not yet supported on this platform.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ssid: Option<&str>, auth: AuthMode) -> BssEntry {
        BssEntry {
            ssid: ssid.map(|s| s.to_string()),
            bssid: "AA:BB:CC:DD:EE:FF".to_string(),
            auth_mode: auth,
            band: Some(2.4),
            channel: Some(6),
            signal: 80,
            is_connected: false,
        }
    }

    #[test]
    fn transition_mode_produces_force_wpa2() {
        let entries = vec![entry(Some("TestNet"), AuthMode::SaeTransition)];
        let actions = detect_repairs("TestNet", &entries);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], RepairAction::ForceWpa2Psk { .. }));
    }

    #[test]
    fn psk_only_needs_no_repair() {
        let entries = vec![entry(Some("TestNet"), AuthMode::Psk)];
        assert!(detect_repairs("TestNet", &entries).is_empty());
    }

    #[test]
    fn sae_only_needs_no_repair() {
        let entries = vec![entry(Some("TestNet"), AuthMode::Sae)];
        assert!(detect_repairs("TestNet", &entries).is_empty());
    }

    #[test]
    fn unmatched_ssid_needs_no_repair() {
        let entries = vec![entry(Some("Other"), AuthMode::SaeTransition)];
        assert!(detect_repairs("TestNet", &entries).is_empty());
    }

    #[test]
    // spec: pubnetdiag-scan#S10
    fn repair_not_needed_when_no_transition() {
        let entries = vec![entry(Some("attinternet"), AuthMode::Psk)];
        assert!(detect_repairs("attinternet", &entries).is_empty());
    }

    #[test]
    // spec: pubnetdiag-scan#S9
    fn repair_needed_when_transition_present() {
        let entries = vec![entry(Some("attinternet"), AuthMode::SaeTransition)];
        assert!(!detect_repairs("attinternet", &entries).is_empty());
    }

    #[test]
    fn sanitize_ssid_replaces_spaces_and_special_chars() {
        assert_eq!(sanitize_ssid("My Network!"), "My_Network_");
        assert_eq!(sanitize_ssid("attinternet"), "attinternet");
        assert_eq!(sanitize_ssid("Blade Runner 2049"), "Blade_Runner_2049");
        assert_eq!(sanitize_ssid("net-5GHz"), "net-5GHz");
    }
}
