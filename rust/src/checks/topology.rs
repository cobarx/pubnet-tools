//! Port of src/checks/topology.ts.
//! spec: topology-default-route-precondition

use crate::exec::{cmd, ExecResult};
use crate::network::{parse_ip_addr, parse_ip_neigh, parse_ip_route};
use crate::types::{CheckResult, CheckStatus, TopologyData};
use std::future::Future;
use std::time::Instant;

const PASSIVE_NOTICE: &str = "Passive ARP cache — no active scan performed.";

fn failed(start: Instant, message: String) -> CheckResult<TopologyData> {
    CheckResult {
        name: "topology".to_string(),
        status: CheckStatus::Failed,
        data: None,
        errors: vec![message],
        findings: vec![],
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// spec: topology-default-route-precondition
/// Sequential: default route determines the interface every other lookup
/// (and every downstream check's gateway) depends on.
///
/// `exec` is generic rather than a trait object — this check (like every
/// other one) only ever has one concrete exec implementation active at a
/// time, so static dispatch is enough and avoids `dyn`/boxed-future
/// ceremony. A spawn failure (exec returning `Err`) is caught here and
/// turned into a `Failed` status, matching the CheckResult contract's
/// "checks never throw" rule — TS's version does not actually implement
/// this catch despite documenting the intent; this is a deliberate,
/// small correction made during the port, not a divergence to hide.
pub async fn check_topology<F, Fut>(exec: &F) -> CheckResult<TopologyData>
where
    F: Fn(Vec<String>) -> Fut,
    Fut: Future<Output = std::io::Result<ExecResult>>,
{
    let start = Instant::now();

    let route_stdout = match exec(cmd(&["ip", "route", "show", "default"])).await {
        Ok(r) => r.stdout,
        Err(e) => return failed(start, format!("Failed to run `ip route`: {e}")),
    };

    let Some(route) = parse_ip_route(&route_stdout) else {
        return CheckResult {
            name: "topology".to_string(),
            status: CheckStatus::Skipped,
            data: None,
            errors: vec!["No default route found".to_string()],
            findings: vec![],
            duration_ms: start.elapsed().as_millis() as u64,
        };
    };

    let (addr_res, neigh_res) = tokio::join!(
        exec(cmd(&["ip", "addr", "show", &route.device])),
        exec(cmd(&["ip", "neigh", "show", "dev", &route.device])),
    );

    let addr_stdout = match addr_res {
        Ok(r) => r.stdout,
        Err(e) => return failed(start, format!("Failed to run `ip addr`: {e}")),
    };
    let neigh_stdout = match neigh_res {
        Ok(r) => r.stdout,
        Err(e) => return failed(start, format!("Failed to run `ip neigh`: {e}")),
    };

    let addr = parse_ip_addr(&addr_stdout);
    let neighbors = parse_ip_neigh(&neigh_stdout, &route.device, Some(&route.gateway));

    let mut errors = Vec::new();
    if addr.is_none() {
        errors.push(format!("Could not determine IP address for {}", route.device));
    }

    let data = TopologyData {
        interface: route.device,
        ip_cidr: addr.as_ref().map(|a| format!("{}/{}", a.ip, a.prefix)).unwrap_or_default(),
        gateway: route.gateway,
        neighbors,
        passive_notice: PASSIVE_NOTICE.to_string(),
    };

    CheckResult {
        name: "topology".to_string(),
        status: if addr.is_some() { CheckStatus::Ok } else { CheckStatus::Degraded },
        data: Some(data),
        errors,
        findings: vec![],
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn exec_result(stdout: &str) -> ExecResult {
        ExecResult { stdout: stdout.to_string(), stderr: String::new(), exit_code: Some(0) }
    }

    // spec: topology-default-route-precondition#S2
    #[tokio::test]
    async fn no_default_route_is_skipped_and_stops_after_one_call() {
        let call_count = AtomicUsize::new(0);
        let calls: Mutex<Vec<Vec<String>>> = Mutex::new(Vec::new());
        let exec = |c: Vec<String>| {
            call_count.fetch_add(1, Ordering::SeqCst);
            calls.lock().unwrap().push(c);
            async { Ok(exec_result("")) }
        };

        let result = check_topology(&exec).await;

        assert_eq!(result.status, CheckStatus::Skipped);
        assert!(result.data.is_none());
        assert!(!result.errors.is_empty());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert_eq!(calls.lock().unwrap()[0], vec!["ip", "route", "show", "default"]);
    }

    #[tokio::test]
    async fn default_route_drives_addr_and_neigh_lookups() {
        let calls: Mutex<Vec<Vec<String>>> = Mutex::new(Vec::new());
        let exec = |c: Vec<String>| {
            calls.lock().unwrap().push(c.clone());
            async move {
                if c[1] == "route" {
                    Ok(exec_result(
                        "default via 192.168.5.1 dev wlan0 proto dhcp src 192.168.5.151 metric 600 ",
                    ))
                } else if c[1] == "addr" {
                    Ok(exec_result("    inet 192.168.5.151/24 brd 192.168.5.255 scope global wlan0"))
                } else {
                    Ok(exec_result("192.168.5.1 lladdr 68:7f:f0:55:77:7b REACHABLE "))
                }
            }
        };

        let result = check_topology(&exec).await;

        assert_eq!(result.status, CheckStatus::Ok);
        let data = result.data.unwrap();
        assert_eq!(data.interface, "wlan0");
        assert_eq!(data.ip_cidr, "192.168.5.151/24");
        assert_eq!(data.gateway, "192.168.5.1");
        assert_eq!(data.neighbors.len(), 1);
        assert!(data.neighbors[0].is_gateway);
        assert_eq!(data.neighbors[0].vendor, Some("TP-Link".to_string()));
        assert_eq!(data.passive_notice, "Passive ARP cache — no active scan performed.");

        let recorded = calls.lock().unwrap();
        assert_eq!(recorded[1], vec!["ip", "addr", "show", "wlan0"]);
        assert_eq!(recorded[2], vec!["ip", "neigh", "show", "dev", "wlan0"]);
    }
}
