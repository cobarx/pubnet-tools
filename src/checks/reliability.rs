//! spec: reliability-check-resilience

use crate::exec::{cmd, ExecResult};
use crate::network::{parse_ping_output, stddev};
use crate::types::{CheckResult, CheckStatus, Finding, PingTargetLabel, PingTargetResult, ReliabilityData, Severity};
use std::future::Future;
use std::time::Instant;

const EXTERNAL_TARGETS: &[(&str, PingTargetLabel)] =
    &[("8.8.8.8", PingTargetLabel::GoogleDns), ("1.1.1.1", PingTargetLabel::CloudflareDns)];

const PING_COUNT: &str = "10";

/// `ping` has no portable argument set. Linux/macOS take `-c <count>`;
/// Linux additionally allows `-i 0.2` to compress 10 packets into ~2s
/// (non-root floor is 200ms). Windows `ping` uses `-n <count>`, has no
/// sub-second interval flag at all (it waits ~1s between echoes, so 10
/// packets is ~10s), and `-i` there sets the TTL — passing `-i 0.2` would
/// break it outright. `-w 2000` caps the per-reply wait. See
/// docs/decisions/2026-08-27-windows-platform-support.md.
#[cfg(windows)]
fn ping_cmd(host: &str) -> Vec<String> {
    cmd(&["ping", "-n", PING_COUNT, "-w", "2000", host])
}

#[cfg(not(windows))]
fn ping_cmd(host: &str) -> Vec<String> {
    cmd(&["ping", "-c", PING_COUNT, "-i", "0.2", host])
}

async fn ping_target<F, Fut>(exec: &F, host: &str, label: PingTargetLabel) -> PingTargetResult
where
    F: Fn(Vec<String>) -> Fut,
    Fut: Future<Output = std::io::Result<ExecResult>>,
{
    let stdout = match exec(ping_cmd(host)).await {
        Ok(r) => r.stdout,
        Err(_) => String::new(),
    };
    let summary = parse_ping_output(&stdout);

    let packet_loss_pct = if summary.transmitted > 0 {
        (summary.transmitted - summary.received) as f64 / summary.transmitted as f64 * 100.0
    } else {
        100.0
    };
    let reachable = summary.received > 0;

    PingTargetResult {
        host: host.to_string(),
        label,
        transmitted: summary.transmitted,
        received: summary.received,
        packet_loss_pct,
        min_ms: summary.rtts.iter().cloned().fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.min(v)))
        }),
        avg_ms: if summary.rtts.is_empty() {
            None
        } else {
            Some(summary.rtts.iter().sum::<f64>() / summary.rtts.len() as f64)
        },
        max_ms: summary.rtts.iter().cloned().fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        }),
        jitter_ms: if summary.rtts.is_empty() { None } else { Some(stddev(&summary.rtts)) },
        rtts: summary.rtts,
        reachable,
    }
}

fn findings_for(targets: &[PingTargetResult]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let gateway = targets.iter().find(|t| t.label == PingTargetLabel::Gateway);
    let internet_up = targets.iter().any(|t| t.label != PingTargetLabel::Gateway && t.reachable);

    if let Some(g) = gateway
        && !g.reachable
    {
        findings.push(Finding {
            id: "reliability.gateway-unreachable".to_string(),
            severity: Severity::Alert,
            points: 30,
            title: "Gateway unreachable".to_string(),
            detail: None,
        });
    }
    if !internet_up {
        findings.push(Finding {
            id: "reliability.internet-unreachable".to_string(),
            severity: Severity::Alert,
            points: 25,
            title: "Internet unreachable".to_string(),
            detail: None,
        });
    }
    for target in targets {
        let label_str = target.label.as_str();
        if target.packet_loss_pct > 10.0 {
            findings.push(Finding {
                id: format!("reliability.packet-loss.{label_str}"),
                severity: Severity::Warn,
                points: 10,
                title: format!("Packet loss > 10% to {}", target.host),
                detail: Some(format!("{:.1}% loss", target.packet_loss_pct)),
            });
        }
        if let Some(avg) = target.avg_ms
            && avg > 200.0
        {
            findings.push(Finding {
                id: format!("reliability.high-latency.{label_str}"),
                severity: Severity::Warn,
                points: 5,
                title: format!("Average RTT > 200ms to {}", target.host),
                detail: Some(format!("{avg:.1}ms avg")),
            });
        }
        if let Some(jitter) = target.jitter_ms
            && jitter > 30.0
        {
            findings.push(Finding {
                id: format!("reliability.jitter.{label_str}"),
                severity: Severity::Warn,
                points: 5,
                title: format!("Jitter > 30ms to {}", target.host),
                detail: Some(format!("{jitter:.1}ms jitter")),
            });
        }
    }
    findings
}

/// spec: reliability-check-resilience
/// One target's failure never aborts the others - every target is pinged
/// independently and its result reported regardless of the others' outcome.
///
/// `exclude` drops specific external targets (e.g. `--exclude-target`)
/// from the ping set entirely - the gateway is never excludable this way.
/// Validating "don't exclude every external target" is the CLI layer's
/// job (cli.rs), not this function's - this trusts its input the same way
/// the rest of the check trusts a well-formed `gateway_ip`.
pub async fn check_reliability<F, Fut>(
    gateway_ip: Option<&str>,
    exec: &F,
    exclude: &[PingTargetLabel],
) -> CheckResult<ReliabilityData>
where
    F: Fn(Vec<String>) -> Fut,
    Fut: Future<Output = std::io::Result<ExecResult>>,
{
    let start = Instant::now();

    let Some(gateway_ip) = gateway_ip else {
        return CheckResult {
            name: "reliability".to_string(),
            status: CheckStatus::Skipped,
            data: None,
            errors: vec!["No gateway IP available (topology check found no default route)".to_string()],
            findings: vec![],
            duration_ms: start.elapsed().as_millis() as u64,
        };
    };

    let mut futures = vec![ping_target(exec, gateway_ip, PingTargetLabel::Gateway)];
    for (host, label) in EXTERNAL_TARGETS.iter().filter(|(_, l)| !exclude.contains(l)) {
        futures.push(ping_target(exec, host, *label));
    }
    let targets = futures_util::future::join_all(futures).await;

    let gateway_reachable =
        targets.iter().find(|t| t.label == PingTargetLabel::Gateway).map(|t| t.reachable).unwrap_or(false);
    let internet_reachable = targets.iter().any(|t| t.label != PingTargetLabel::Gateway && t.reachable);

    let findings = findings_for(&targets);
    let data = ReliabilityData { targets, gateway_reachable, internet_reachable };
    let status = if gateway_reachable && internet_reachable { CheckStatus::Ok } else { CheckStatus::Degraded };

    CheckResult {
        name: "reliability".to_string(),
        status,
        data: Some(data),
        errors: vec![],
        findings,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn exec_result(stdout: &str) -> ExecResult {
        ExecResult { stdout: stdout.to_string(), stderr: String::new(), exit_code: Some(0) }
    }

    fn ping_output(transmitted: u32, received: u32, rtts: &[f64]) -> String {
        let mut lines: Vec<String> = rtts
            .iter()
            .enumerate()
            .map(|(i, t)| format!("64 bytes from x: icmp_seq={} time={t} ms", i + 1))
            .collect();
        lines.push(format!("{transmitted} packets transmitted, {received} received, 0% packet loss"));
        lines.join("\n")
    }

    fn reachable_output() -> String {
        ping_output(10, 10, &[10.0, 12.0, 11.0, 9.0, 10.0, 11.0, 10.0, 12.0, 9.0, 11.0])
    }

    fn unreachable_output() -> String {
        ping_output(10, 0, &[])
    }

    // spec: reliability-check-resilience#S4
    #[tokio::test]
    async fn no_gateway_ip_means_no_pings_attempted() {
        let call_count = AtomicUsize::new(0);
        let exec = |_: Vec<String>| {
            call_count.fetch_add(1, Ordering::SeqCst);
            async { Ok(exec_result("")) }
        };

        let result = check_reliability(None, &exec, &[]).await;

        assert_eq!(result.status, CheckStatus::Skipped);
        assert!(result.data.is_none());
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    // spec: reliability-check-resilience#S1
    #[tokio::test]
    async fn all_three_reachable_is_ok() {
        let exec = |_: Vec<String>| async { Ok(exec_result(&reachable_output())) };

        let result = check_reliability(Some("192.168.5.1"), &exec, &[]).await;

        assert_eq!(result.status, CheckStatus::Ok);
        let data = result.data.unwrap();
        assert!(data.gateway_reachable);
        assert!(data.internet_reachable);
        assert_eq!(data.targets.len(), 3);
        for target in &data.targets {
            assert_eq!(target.transmitted, 10);
            assert!(target.reachable);
            assert!(target.jitter_ms.is_some());
        }
    }

    // spec: reliability-check-resilience#S2
    #[tokio::test]
    async fn gateway_down_internet_up_is_degraded_not_aborted() {
        let exec = |c: Vec<String>| async move {
            let host = c.last().unwrap().clone();
            if host == "192.168.5.1" {
                Ok(exec_result(&unreachable_output()))
            } else {
                Ok(exec_result(&reachable_output()))
            }
        };

        let result = check_reliability(Some("192.168.5.1"), &exec, &[]).await;

        assert_eq!(result.status, CheckStatus::Degraded);
        let data = result.data.unwrap();
        assert!(!data.gateway_reachable);
        assert!(data.internet_reachable);
        assert_eq!(data.targets.len(), 3);
        let gateway_target = data.targets.iter().find(|t| t.label == PingTargetLabel::Gateway).unwrap();
        assert_eq!(gateway_target.packet_loss_pct, 100.0);
        assert!(!gateway_target.reachable);
    }

    // spec: reliability-check-resilience#S3
    #[tokio::test]
    async fn no_target_reachable_is_degraded_not_failed() {
        let exec = |_: Vec<String>| async { Ok(exec_result(&unreachable_output())) };

        let result = check_reliability(Some("192.168.5.1"), &exec, &[]).await;

        assert_eq!(result.status, CheckStatus::Degraded);
        let data = result.data.unwrap();
        assert!(!data.gateway_reachable);
        assert!(!data.internet_reachable);
        assert_eq!(data.targets.len(), 3);
        for target in &data.targets {
            assert_eq!(target.packet_loss_pct, 100.0);
            assert!(!target.reachable);
        }
    }

    #[tokio::test]
    async fn excluded_external_target_is_not_pinged_at_all() {
        let calls: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        let exec = |c: Vec<String>| {
            calls.lock().unwrap().push(c.last().unwrap().clone());
            async { Ok(exec_result(&reachable_output())) }
        };

        let result = check_reliability(Some("192.168.5.1"), &exec, &[PingTargetLabel::GoogleDns]).await;

        let data = result.data.unwrap();
        assert_eq!(data.targets.len(), 2);
        assert!(!data.targets.iter().any(|t| t.label == PingTargetLabel::GoogleDns));
        assert!(data.targets.iter().any(|t| t.label == PingTargetLabel::CloudflareDns));
        assert!(!calls.lock().unwrap().contains(&"8.8.8.8".to_string()));
    }

    #[tokio::test]
    async fn internet_reachable_reflects_only_the_remaining_external_target() {
        let exec = |c: Vec<String>| {
            let host = c.last().unwrap().clone();
            async move {
                let output = if host == "1.1.1.1" { unreachable_output() } else { reachable_output() };
                Ok(exec_result(&output))
            }
        };

        let result =
            check_reliability(Some("192.168.5.1"), &exec, &[PingTargetLabel::CloudflareDns]).await;

        let data = result.data.unwrap();
        assert_eq!(data.targets.len(), 2);
        // Only google-dns remains as the external target, and it's reachable
        // in this fixture, so internet_reachable should be true regardless
        // of what the excluded cloudflare-dns target would have reported.
        assert!(data.internet_reachable);
    }
}
