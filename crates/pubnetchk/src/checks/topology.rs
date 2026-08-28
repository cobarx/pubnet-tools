//! Port of src/checks/topology.ts.
//! spec: topology-default-route-precondition

use crate::platform::PlatformProbe;
use crate::types::{CheckResult, CheckStatus, TopologyData};
use std::time::Instant;

const PASSIVE_NOTICE: &str = "Passive ARP cache — no active scan performed.";

/// spec: topology-default-route-precondition
/// Sequential: default route determines the interface every other lookup depends on.
pub async fn check_topology<P: PlatformProbe>(probe: &P) -> CheckResult<TopologyData> {
    let start = Instant::now();

    let Some(route) = probe.default_route().await else {
        return CheckResult {
            name: "topology".to_string(),
            status: CheckStatus::Skipped,
            data: None,
            errors: vec!["No default route found".to_string()],
            findings: vec![],
            duration_ms: start.elapsed().as_millis() as u64,
        };
    };

    let (addr, neighbors, kind) = tokio::join!(
        probe.interface_addr(&route.device),
        probe.arp_neighbors(&route.device, Some(&route.gateway)),
        probe.interface_type(&route.device),
    );

    let mut errors = Vec::new();
    if addr.is_none() {
        errors.push(format!(
            "Could not determine IP address for {}",
            route.device
        ));
    }

    let ip_cidr = addr
        .as_ref()
        .map(|a| format!("{}/{}", a.ip, a.prefix))
        .unwrap_or_default();

    let data = TopologyData {
        interface: route.device,
        interface_kind: kind,
        ip_cidr,
        gateway: route.gateway,
        neighbors,
        passive_notice: PASSIVE_NOTICE.to_string(),
    };

    CheckResult {
        name: "topology".to_string(),
        status: if addr.is_some() {
            CheckStatus::Ok
        } else {
            CheckStatus::Degraded
        },
        data: Some(data),
        errors,
        findings: vec![],
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{AddrInfo, PlatformProbe, RouteInfo, WifiInfo};
    use crate::types::{ArpNeighbor, BssEntry, DnsResolverInfo, InterfaceKind};

    struct MockProbe {
        route: Option<RouteInfo>,
        addr: Option<AddrInfo>,
        neighbors: Vec<ArpNeighbor>,
    }

    impl PlatformProbe for MockProbe {
        async fn default_route(&self) -> Option<RouteInfo> {
            self.route.clone()
        }
        async fn interface_addr(&self, _: &str) -> Option<AddrInfo> {
            self.addr.clone()
        }
        async fn arp_neighbors(&self, _: &str, _: Option<&str>) -> Vec<ArpNeighbor> {
            self.neighbors.clone()
        }
        async fn wifi_info(&self, _: &str, _: bool) -> Option<WifiInfo> {
            None
        }
        async fn dns_info(&self, _: &str) -> Option<DnsResolverInfo> {
            None
        }
        async fn system_egress_ip(&self) -> Option<String> {
            None
        }
        async fn interface_type(&self, _: &str) -> InterfaceKind {
            InterfaceKind::Ethernet
        }
        async fn scan_bss_list(&self) -> Option<Vec<BssEntry>> {
            None
        }
    }

    fn gateway_neighbor() -> ArpNeighbor {
        ArpNeighbor {
            ip: "192.168.5.1".to_string(),
            vendor: Some("TP-Link".to_string()),
            mac: Some("68:7f:f0:55:77:7b".to_string()),
            state: "REACHABLE".to_string(),
            device: "wlan0".to_string(),
            is_gateway: true,
        }
    }

    // spec: topology-default-route-precondition#S2
    #[tokio::test]
    async fn no_default_route_is_skipped() {
        let probe = MockProbe {
            route: None,
            addr: None,
            neighbors: vec![],
        };
        let result = check_topology(&probe).await;
        assert_eq!(result.status, CheckStatus::Skipped);
        assert!(result.data.is_none());
        assert!(!result.errors.is_empty());
    }

    #[tokio::test]
    async fn default_route_drives_addr_and_neigh_lookups() {
        let probe = MockProbe {
            route: Some(RouteInfo {
                gateway: "192.168.5.1".to_string(),
                device: "wlan0".to_string(),
            }),
            addr: Some(AddrInfo {
                ip: "192.168.5.151".to_string(),
                prefix: 24,
            }),
            neighbors: vec![gateway_neighbor()],
        };
        let result = check_topology(&probe).await;
        assert_eq!(result.status, CheckStatus::Ok);
        let data = result.data.unwrap();
        assert_eq!(data.interface, "wlan0");
        assert_eq!(data.ip_cidr, "192.168.5.151/24");
        assert_eq!(data.gateway, "192.168.5.1");
        assert_eq!(data.neighbors.len(), 1);
        assert!(data.neighbors[0].is_gateway);
        assert_eq!(data.neighbors[0].vendor, Some("TP-Link".to_string()));
        assert_eq!(
            data.passive_notice,
            "Passive ARP cache — no active scan performed."
        );
    }

    #[tokio::test]
    async fn route_found_but_addr_missing_is_degraded() {
        let probe = MockProbe {
            route: Some(RouteInfo {
                gateway: "192.168.5.1".to_string(),
                device: "wlan0".to_string(),
            }),
            addr: None,
            neighbors: vec![],
        };
        let result = check_topology(&probe).await;
        assert_eq!(result.status, CheckStatus::Degraded);
        assert!(result.data.is_some()); // data still present, ip_cidr just empty
        assert!(!result.errors.is_empty());
    }
}
