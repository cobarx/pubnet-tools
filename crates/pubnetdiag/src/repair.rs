use pubnet_platform::types::{AuthMode, BssEntry};

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
                return pubnet_platform::platform::windows::repair_wpa2(ssid, &passphrase).await;
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
}
