//! Contract level: verifies our ping parsing against this machine's real
//! network. spec: reliability-check-resilience#S1 (real ping, real gateway)

use pubnet_tools::checks::reliability::{check_reliability, system_ping};
use pubnet_tools::checks::topology::check_topology;

#[cfg(target_os = "linux")]
use pubnet_tools::platform::linux::LinuxProbe;
#[cfg(target_os = "macos")]
use pubnet_tools::platform::macos::MacProbe;
#[cfg(target_os = "windows")]
use pubnet_tools::platform::windows::WindowsProbe;

#[tokio::test]
async fn pings_the_real_gateway_and_two_external_targets() {
    #[cfg(target_os = "linux")]
    let probe = LinuxProbe;
    #[cfg(target_os = "macos")]
    let probe = MacProbe;
    #[cfg(target_os = "windows")]
    let probe = WindowsProbe;

    let topology = check_topology(&probe).await;
    let gateway_ip = topology.data.map(|d| d.gateway);

    let result = check_reliability(gateway_ip.as_deref(), &system_ping, &[]).await;

    assert_ne!(result.status, pubnet_tools::types::CheckStatus::Failed);
    let data = result.data.expect("expected reliability data");
    assert_eq!(data.targets.len(), 3);
    for target in &data.targets {
        assert_eq!(target.transmitted, 10);
        assert!(target.packet_loss_pct >= 0.0 && target.packet_loss_pct <= 100.0);
        if target.reachable {
            assert!(!target.rtts.is_empty());
            assert!(target.jitter_ms.unwrap() >= 0.0);
            assert!(target.min_ms.unwrap() <= target.avg_ms.unwrap());
            assert!(target.avg_ms.unwrap() <= target.max_ms.unwrap());
        }
    }
}
