//! Contract level: verifies parsing assumptions against this machine's real commands.
//! Asserts shape, not exact values — real networks vary.
//! spec: topology-default-route-precondition#S1, #S3

use pubnet_tools::checks::topology::check_topology;
use pubnet_tools::network::{ipv4_in_cidr, is_valid_ipv4};
use pubnet_tools::types::{CheckStatus, InterfaceKind};

#[cfg(target_os = "linux")]
use pubnet_tools::platform::linux::LinuxProbe;
#[cfg(target_os = "macos")]
use pubnet_tools::platform::macos::MacProbe;
#[cfg(target_os = "windows")]
use pubnet_tools::platform::windows::WindowsProbe;

#[tokio::test]
async fn discovers_default_interface_gateway_and_arp_neighbors_passively() {
    #[cfg(target_os = "linux")]
    let probe = LinuxProbe;
    #[cfg(target_os = "macos")]
    let probe = MacProbe;
    #[cfg(target_os = "windows")]
    let probe = WindowsProbe;

    let result = check_topology(&probe).await;

    assert_ne!(result.status, CheckStatus::Failed);
    assert_ne!(result.status, CheckStatus::Skipped);
    assert!(result.data.is_some());

    let data = result.data.unwrap();
    assert!(!data.interface.is_empty());
    assert!(is_valid_ipv4(&data.gateway));
    assert!(data.ip_cidr.contains('/'));

    // Cross-field invariant: on a normal L2 network the gateway is on the
    // interface's own subnet. A wrong `NextHop` / address / prefix read from
    // the platform probe (e.g. a Win32 struct-layout regression) shows up
    // here even though each field is individually well-formed. VPN /
    // point-to-point links legitimately have an off-subnet gateway, so this
    // is scoped to WiFi/Ethernet.
    if matches!(data.interface_kind, InterfaceKind::WiFi | InterfaceKind::Ethernet) {
        assert_eq!(
            ipv4_in_cidr(&data.gateway, &data.ip_cidr),
            Some(true),
            "gateway {} is not on the interface subnet {}",
            data.gateway,
            data.ip_cidr
        );
    }

    for neighbor in &data.neighbors {
        assert!(is_valid_ipv4(&neighbor.ip));
        assert_eq!(neighbor.device, data.interface);
    }
    assert_eq!(data.passive_notice, "Passive ARP cache — no active scan performed.");
}
