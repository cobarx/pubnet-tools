//! Contract level: real platform probes, real DoH providers, real canary endpoints.
//! Asserts shape, not exact values — real networks vary.
//! specs: dns-leak-detection, captive-portal-detection

use pubnet_tools::checks::security::check_security;
use pubnet_tools::checks::topology::check_topology;
use pubnet_tools::types::{CaptivePortalMethod, CheckStatus, DnsLeakVerdict};

#[cfg(target_os = "linux")]
use pubnet_tools::platform::linux::LinuxProbe;
#[cfg(target_os = "macos")]
use pubnet_tools::platform::macos::MacProbe;
#[cfg(target_os = "windows")]
use pubnet_tools::platform::windows::WindowsProbe;

#[tokio::test]
async fn produces_full_security_data_from_real_probes() {
    #[cfg(target_os = "linux")]
    let probe = LinuxProbe;
    #[cfg(target_os = "macos")]
    let probe = MacProbe;
    #[cfg(target_os = "windows")]
    let probe = WindowsProbe;

    let topology = check_topology(&probe).await;
    let iface = topology.data.map(|d| d.interface);

    let client = reqwest::Client::new();
    // wifi_detail: true — exercise both the fast and slow Wi-Fi reads.
    let result = check_security(iface.as_deref(), &probe, &client, true).await;

    assert_ne!(result.status, CheckStatus::Failed);
    let data = result.data.expect("expected security data");

    // On Wi-Fi we get an encryption verdict; a redacted SSID (macOS 15+) is
    // fine, but then the hidden-SSID finding must explain the gap.
    if data.ssid.is_none() && data.encryption != pubnet_tools::types::WifiEncryption::Unknown {
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.id == "security.wifi-ssid-hidden")
        );
    }

    assert!(matches!(
        data.dns_leak.verdict,
        DnsLeakVerdict::Clean | DnsLeakVerdict::Leaked | DnsLeakVerdict::Uncertain
    ));
    assert_eq!(data.dns_leak.probes.len(), 2);
    assert!(matches!(
        data.captive_portal.method,
        CaptivePortalMethod::Redirect
            | CaptivePortalMethod::ContentMismatch
            | CaptivePortalMethod::None
    ));
    assert!(!data.captive_portal.canary_url.is_empty());
}
