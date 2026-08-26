//! Contract level: real nmcli/resolvectl, real DoH providers, real canary
//! endpoints. Asserts shape, not exact values - real networks vary.
//! specs: dns-leak-detection, captive-portal-detection

use pubnet_tools::checks::security::check_security;
use pubnet_tools::checks::topology::check_topology;
use pubnet_tools::exec::exec_cmd;
use pubnet_tools::types::{CaptivePortalMethod, CheckStatus, DnsLeakVerdict};

#[tokio::test]
async fn produces_full_security_data_from_real_probes() {
    let topology = check_topology(&exec_cmd).await;
    let iface = topology.data.map(|d| d.interface);

    let client = reqwest::Client::new();
    let result = check_security(iface.as_deref(), &exec_cmd, &client).await;

    assert_ne!(result.status, CheckStatus::Failed);
    let data = result.data.expect("expected security data");

    assert!(matches!(
        data.dns_leak.verdict,
        DnsLeakVerdict::Clean | DnsLeakVerdict::Leaked | DnsLeakVerdict::Uncertain
    ));
    assert_eq!(data.dns_leak.probes.len(), 2);
    assert!(matches!(
        data.captive_portal.method,
        CaptivePortalMethod::Redirect | CaptivePortalMethod::ContentMismatch | CaptivePortalMethod::None
    ));
    assert!(!data.captive_portal.canary_url.is_empty());
}
