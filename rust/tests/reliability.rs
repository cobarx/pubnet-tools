//! Contract level: verifies our ping parsing against this machine's real
//! network. spec: reliability-check-resilience#S1 (real ping, real gateway)

use conncheck::checks::reliability::check_reliability;
use conncheck::checks::topology::check_topology;
use conncheck::exec::exec_cmd;

#[tokio::test]
async fn pings_the_real_gateway_and_two_external_targets() {
    let topology = check_topology(&exec_cmd).await;
    let gateway_ip = topology.data.map(|d| d.gateway);

    let result = check_reliability(gateway_ip.as_deref(), &exec_cmd, &[]).await;

    assert_ne!(result.status, conncheck::types::CheckStatus::Failed);
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
