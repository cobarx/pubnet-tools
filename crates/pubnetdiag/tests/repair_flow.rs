// Integration tests for the repair detection + dispatch pipeline.
//
// Scenario being replicated: Intel Wireless-AC 9560 (driver v23.110.0.5) connecting
// to an AT&T residential gateway that broadcasts WPA2+WPA3 transition mode. The
// driver mishandles SAE against transition-mode APs, producing a bogus
// "bad password" error. pubnetdiag --repair should detect the condition and apply
// a forced WPA2-PSK profile.
//
// OS boundaries mocked here:
//   - BSS scan (WlanGetNetworkBssList): replaced with synthetic BssEntry fixtures
//   - Profile apply (WlanSetProfile + WlanConnect): not called; the profile that
//     *would* be applied is validated separately in pubnet-platform/tests/wpa2_profile.rs
//
// Real-hardware coverage (to run at the original AT&T transition-mode AP):
//   - pubnetdiag --repair attinternet → WlanSetProfile + WlanConnect → connected

use pubnet_platform::types::{AuthMode, BssEntry};
use pubnetdiag::repair::{detect_repairs, RepairAction};

fn bss(ssid: &str, auth: AuthMode) -> BssEntry {
    BssEntry {
        ssid: Some(ssid.to_string()),
        bssid: "D0:4F:58:81:88:70".to_string(),
        auth_mode: auth,
        band: Some(5.0),
        channel: Some(120),
        signal: 88,
        is_connected: false,
    }
}

// --- S9: transition mode detected, repair recommended ---

#[test]
fn transition_mode_produces_force_wpa2_repair() {
    // Simulates the AT&T gateway scan result on a machine with the broken driver.
    // The AP advertises both PSK and SAE — the AC 9560 v23.x will pick SAE and fail.
    let entries = vec![bss("attinternet", AuthMode::SaeTransition)];
    let repairs = detect_repairs("attinternet", &entries);

    assert_eq!(repairs.len(), 1, "expected exactly one repair action");
    assert!(
        matches!(repairs[0], RepairAction::ForceWpa2Psk { .. }),
        "expected ForceWpa2Psk, got something else"
    );
}

#[test]
fn repair_description_mentions_wpa2() {
    let entries = vec![bss("attinternet", AuthMode::SaeTransition)];
    let repairs = detect_repairs("attinternet", &entries);
    assert!(
        repairs[0].description().to_lowercase().contains("wpa2"),
        "action description must mention WPA2 so the user knows what's happening"
    );
}

// --- S10: WPA2-only AP (Henry's place) — no repair needed ---

#[test]
fn wpa2_only_needs_no_repair() {
    // Henry's attinternet is WPA2-Personal only — scanner returns Psk, no SaeTransition.
    // The driver bug doesn't apply; no repair should be recommended.
    let entries = vec![bss("attinternet", AuthMode::Psk)];
    assert!(
        detect_repairs("attinternet", &entries).is_empty(),
        "WPA2-only AP must not trigger a repair"
    );
}

#[test]
fn pure_wpa3_needs_no_repair() {
    // Galaxy S23 hotspot: WPA3-SAE only, no transition mode.
    // Driver may still fail, but that's a different failure mode — not covered here.
    let entries = vec![bss("Galaxy S23", AuthMode::Sae)];
    assert!(detect_repairs("Galaxy S23", &entries).is_empty());
}

// --- SSID filtering ---

#[test]
fn repair_only_triggered_for_target_ssid() {
    let entries = vec![
        bss("attinternet", AuthMode::SaeTransition),
        bss("other-net", AuthMode::SaeTransition),
    ];
    // detect_repairs filters by SSID — only attinternet should match
    let repairs_att = detect_repairs("attinternet", &entries);
    let repairs_other = detect_repairs("other-net", &entries);
    assert_eq!(repairs_att.len(), 1);
    assert_eq!(repairs_other.len(), 1);
    // And a completely absent SSID gets nothing
    assert!(detect_repairs("nonexistent", &entries).is_empty());
}

#[test]
fn multiple_bssids_same_ssid_one_action() {
    // AT&T deploys mesh networks — attinternet may have 16+ BSSIDs, all SaeTransition.
    // detect_repairs should produce one ForceWpa2Psk action, not one per BSSID.
    let entries = vec![
        bss("attinternet", AuthMode::SaeTransition),
        bss("attinternet", AuthMode::SaeTransition),
        bss("attinternet", AuthMode::SaeTransition),
    ];
    let repairs = detect_repairs("attinternet", &entries);
    assert_eq!(repairs.len(), 1, "one repair action regardless of BSS count");
}
