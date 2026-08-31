// Integration tests for WPA2-PSK profile XML generation.
//
// The profile is applied by repair_wpa2() (WlanSetProfile) to bypass the
// Intel AC 9560 v23.x SAE handshake failure against transition-mode APs.
// These tests verify the profile structure without touching the WLAN API.

#[cfg(target_os = "windows")]
mod windows {
    use pubnet_platform::platform::windows::build_wpa2_profile;

    #[test]
    fn profile_forces_wpa2psk_not_sae() {
        let xml = build_wpa2_profile("attinternet", "testpassphrase");
        assert!(
            xml.contains("<authentication>WPA2PSK</authentication>"),
            "profile must force WPA2PSK to bypass AC 9560 SAE failure"
        );
        assert!(
            !xml.contains("WPA3") && !xml.contains("SAE"),
            "profile must not reference WPA3 or SAE"
        );
    }

    #[test]
    fn profile_uses_aes_cipher() {
        let xml = build_wpa2_profile("attinternet", "testpassphrase");
        assert!(xml.contains("<encryption>AES</encryption>"));
    }

    #[test]
    fn profile_disables_one_x() {
        let xml = build_wpa2_profile("attinternet", "testpassphrase");
        assert!(xml.contains("<useOneX>false</useOneX>"));
    }

    #[test]
    fn profile_disables_mac_randomization() {
        // AT&T gateways reject randomized MACs during association.
        let xml = build_wpa2_profile("attinternet", "testpassphrase");
        assert!(
            xml.contains("<enableRandomization>false</enableRandomization>"),
            "MAC randomization must be disabled for AT&T gateway compatibility"
        );
    }

    #[test]
    fn profile_contains_ssid_and_passphrase() {
        let xml = build_wpa2_profile("MyNetwork", "correcthorsebatterystaple");
        assert!(xml.contains("<name>MyNetwork</name>"));
        assert!(xml.contains("<keyMaterial>correcthorsebatterystaple</keyMaterial>"));
    }

    #[test]
    fn profile_passphrase_is_passphrase_type() {
        let xml = build_wpa2_profile("MyNetwork", "testpass");
        assert!(xml.contains("<keyType>passPhrase</keyType>"));
        assert!(xml.contains("<protected>false</protected>"));
    }

    #[test]
    fn profile_xml_escapes_ampersand_in_ssid() {
        let xml = build_wpa2_profile("Test&Net", "testpass");
        assert!(xml.contains("Test&amp;Net"), "& in SSID must be XML-escaped");
        assert!(!xml.contains("Test&Net"), "unescaped & must not appear");
    }

    #[test]
    fn profile_xml_escapes_angle_brackets_in_passphrase() {
        let xml = build_wpa2_profile("TestNet", "pass<word>");
        assert!(
            xml.contains("pass&lt;word&gt;"),
            "< and > in passphrase must be XML-escaped"
        );
    }

    #[test]
    fn profile_xml_escapes_quotes_and_apostrophe() {
        let xml = build_wpa2_profile("Test\"Net'X", "pa\"ss'ph");
        assert!(xml.contains("Test&quot;Net&apos;X"));
        assert!(xml.contains("pa&quot;ss&apos;ph"));
    }

    #[test]
    fn profile_connection_mode_is_auto() {
        let xml = build_wpa2_profile("TestNet", "testpass");
        assert!(xml.contains("<connectionMode>auto</connectionMode>"));
    }
}
