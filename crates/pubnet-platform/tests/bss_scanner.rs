// Contract test: at least one BSS entry returned on a Wi-Fi-enabled Windows
// machine. Shape check only — real networks vary.
// spec: pubnetdiag-scan#S3, #S6

#[cfg(target_os = "windows")]
mod windows {
    use pubnet_platform::platform::windows::WindowsProbe;
    use pubnet_platform::platform::PlatformProbe;
    use pubnet_platform::types::AuthMode;

    #[tokio::test]
    async fn scan_bss_list_returns_entries() {
        let probe = WindowsProbe;
        let entries = probe.scan_bss_list().await;

        // spec: pubnetdiag-scan#S3 — empty Vec is fine when WLAN is absent;
        // but on a Wi-Fi-enabled machine this should not be empty.
        assert!(
            !entries.is_empty(),
            "expected at least one BSS entry — is Wi-Fi enabled?"
        );

        for entry in &entries {
            // SSID is Option<String> — None is valid for hidden networks (S6)
            if let Some(ssid) = &entry.ssid {
                assert!(!ssid.is_empty(), "non-None SSID must not be empty");
            }

            // BSSID must be XX:XX:XX:XX:XX:XX (17 chars, colon-separated)
            assert_eq!(entry.bssid.len(), 17, "BSSID must be 17 chars: {}", entry.bssid);
            let octets: Vec<&str> = entry.bssid.split(':').collect();
            assert_eq!(octets.len(), 6, "BSSID must have 6 octets: {}", entry.bssid);

            // auth_mode must be a valid variant
            let _ = match entry.auth_mode {
                AuthMode::Psk | AuthMode::Sae | AuthMode::SaeTransition | AuthMode::Unknown => true,
            };

            // signal is 0-100
            assert!(entry.signal <= 100, "signal {} out of range", entry.signal);
        }
    }
}
