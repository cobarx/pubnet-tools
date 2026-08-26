//! Contract level: verifies our parsing assumptions against this machine's
//! real `ip` output. Asserts shape, not exact values — real networks vary.
//! spec: topology-default-route-precondition#S1, #S3

use pubnet_tools::exec::exec_cmd;
use pubnet_tools::checks::topology::check_topology;
use pubnet_tools::network::is_valid_ipv4;
use pubnet_tools::types::CheckStatus;

#[tokio::test]
async fn discovers_default_interface_gateway_and_arp_neighbors_passively() {
    let result = check_topology(&exec_cmd).await;

    assert_ne!(result.status, CheckStatus::Failed);
    assert_ne!(result.status, CheckStatus::Skipped);
    assert!(result.data.is_some());

    let data = result.data.unwrap();
    assert!(!data.interface.is_empty());
    assert!(is_valid_ipv4(&data.gateway));
    assert!(data.ip_cidr.contains('/'));
    for neighbor in &data.neighbors {
        assert!(is_valid_ipv4(&neighbor.ip));
        assert_eq!(neighbor.device, data.interface);
    }
    assert_eq!(data.passive_notice, "Passive ARP cache — no active scan performed.");
}
