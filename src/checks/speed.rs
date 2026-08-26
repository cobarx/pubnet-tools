//! Hand-rolled NDT7 (M-Lab) client over tokio-tungstenite.
//! See docs/decisions/2026-08-24-cloudflare-speedtest-not-node-compatible.md
//!
//! Scope note: unlike the other checks, this module does not inject a fake
//! WebSocket transport for unit-testing the send/receive orchestration -
//! doing so faithfully would mean a trait abstraction over
//! Stream<Item=Message>+Sink<Message> purely to replicate coverage a real
//! contract test (see rust/tests/speed.rs) against live M-Lab servers
//! already provides. Pure helpers (`mbps`, `extract_rtt_ms`) are unit
//! tested directly; `locate` stays injectable since faking a failed HTTP
//! call is cheap and worth covering without touching the network.

use crate::network::stddev;
use crate::types::{CheckStatus, CheckResult, Finding, Severity, SpeedData};
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

const NDT7_SUBPROTOCOL: &str = "net.measurementlab.ndt.v7";
const LOCATE_URL: &str = "https://locate.measurementlab.net/v2/nearest/ndt/ndt7";
/// spec: docs/decisions/2026-08-25-configurable-speed-duration.md
/// Per-direction window when the caller doesn't override it via
/// --speed-duration/--quick. Matches ndt7-js's own default.
pub const DEFAULT_TEST_DURATION: Duration = Duration::from_secs(10);
const UPLOAD_CHUNK_BYTES: usize = 8192;

pub struct NdtServer {
    pub download_url: String,
    pub upload_url: String,
}

pub async fn default_locate() -> Result<NdtServer, String> {
    let res = reqwest::get(LOCATE_URL).await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("NDT7 locate API returned HTTP {}", res.status()));
    }
    let body: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let first = body.get("results").and_then(|r| r.get(0));
    let download_url = first
        .and_then(|f| f.get("urls"))
        .and_then(|u| u.get("wss:///ndt/v7/download"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let upload_url = first
        .and_then(|f| f.get("urls"))
        .and_then(|u| u.get("wss:///ndt/v7/upload"))
        .and_then(|v| v.as_str())
        .map(String::from);

    match (download_url, upload_url) {
        (Some(d), Some(u)) => Ok(NdtServer { download_url: d, upload_url: u }),
        _ => Err("NDT7 locate API returned no usable server".to_string()),
    }
}

pub fn extract_rtt_ms(json: &str) -> Option<f64> {
    let parsed: serde_json::Value = serde_json::from_str(json).ok()?;
    let rtt = parsed.get("TCPInfo")?.get("RTT")?.as_f64()?;
    Some(rtt / 1000.0)
}

pub fn mbps(bytes: u64, elapsed_ms: u64) -> f64 {
    if elapsed_ms == 0 {
        return 0.0;
    }
    (bytes as f64 * 8.0) / (elapsed_ms as f64 / 1000.0) / 1_000_000.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Download,
    Upload,
}

struct DirectionResult {
    bytes_transferred: u64,
    elapsed_ms: u64,
    rtt_samples_ms: Vec<f64>,
}

async fn measure_direction(url: &str, mode: Mode, test_duration: Duration) -> Result<DirectionResult, String> {
    let (ws_stream, _) = tokio_tungstenite::connect_async(
        tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(url)
            .map_err(|e| e.to_string())
            .and_then(|mut req| {
                req.headers_mut().insert(
                    "Sec-WebSocket-Protocol",
                    NDT7_SUBPROTOCOL.parse().map_err(|_| "invalid subprotocol header".to_string())?,
                );
                Ok(req)
            })?,
    )
    .await
    .map_err(|e| e.to_string())?;

    let started_at = Instant::now();
    let (mut write, mut read) = ws_stream.split();

    let chunk = if mode == Mode::Upload {
        let mut buf = vec![0u8; UPLOAD_CHUNK_BYTES];
        rand::rng().fill_bytes(&mut buf);
        Some(buf)
    } else {
        None
    };

    let mut bytes: u64 = 0;
    let mut rtts: Vec<f64> = Vec::new();
    let deadline = tokio::time::sleep(test_duration);
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            _ = &mut deadline => break,
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        if mode == Mode::Download {
                            bytes += data.len() as u64;
                        }
                    }
                    Some(Ok(Message::Text(text))) => {
                        if let Some(rtt) = extract_rtt_ms(&text) {
                            rtts.push(rtt);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => return Err(e.to_string()),
                    _ => {}
                }
            }
            send_result = write.send(Message::Binary(chunk.clone().unwrap_or_default().into())), if mode == Mode::Upload => {
                match send_result {
                    Ok(()) => bytes += chunk.as_ref().map(|c| c.len()).unwrap_or(0) as u64,
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
    }

    let _ = write.close().await;
    Ok(DirectionResult { bytes_transferred: bytes, elapsed_ms: started_at.elapsed().as_millis() as u64, rtt_samples_ms: rtts })
}

fn failed(start: Instant, message: String) -> CheckResult<SpeedData> {
    CheckResult {
        name: "speed".to_string(),
        status: CheckStatus::Failed,
        data: None,
        errors: vec![message],
        findings: vec![Finding {
            id: "speed.failed".to_string(),
            severity: Severity::Warn,
            points: 5,
            title: "Speed check failed".to_string(),
            detail: None,
        }],
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

pub async fn check_speed<L, LFut>(locate: &L, test_duration: Duration) -> CheckResult<SpeedData>
where
    L: Fn() -> LFut,
    LFut: std::future::Future<Output = Result<NdtServer, String>>,
{
    let start = Instant::now();

    let server = match locate().await {
        Ok(s) => s,
        Err(e) => return failed(start, e),
    };

    let download = match measure_direction(&server.download_url, Mode::Download, test_duration).await {
        Ok(d) => d,
        Err(e) => return failed(start, e),
    };
    let upload = match measure_direction(&server.upload_url, Mode::Upload, test_duration).await {
        Ok(u) => u,
        Err(e) => return failed(start, e),
    };

    let mut rtts = download.rtt_samples_ms;
    rtts.extend(upload.rtt_samples_ms);

    let data = SpeedData {
        download_mbps: mbps(download.bytes_transferred, download.elapsed_ms),
        upload_mbps: mbps(upload.bytes_transferred, upload.elapsed_ms),
        latency_ms: rtts.iter().cloned().fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |a| a.min(v)))).unwrap_or(0.0),
        jitter_ms: if rtts.is_empty() { 0.0 } else { stddev(&rtts) },
        source: "ndt7".to_string(),
    };

    let findings = if data.download_mbps < 1.0 {
        vec![Finding {
            id: "speed.slow-download".to_string(),
            severity: Severity::Warn,
            points: 10,
            title: "Download speed below 1 Mbps".to_string(),
            detail: None,
        }]
    } else {
        vec![]
    };

    CheckResult {
        name: "speed".to_string(),
        status: CheckStatus::Ok,
        data: Some(data),
        errors: vec![],
        findings,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mbps_converts_bytes_and_elapsed_ms() {
        // 12,500,000 bytes in 1000ms = 100,000,000 bits/sec = 100 Mbps
        assert!((mbps(12_500_000, 1000) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn mbps_zero_elapsed_returns_zero() {
        assert_eq!(mbps(1000, 0), 0.0);
    }

    #[test]
    fn extract_rtt_ms_reads_tcpinfo_rtt_microseconds_as_ms() {
        let json = r#"{"TCPInfo":{"RTT":41580,"BytesAcked":123}}"#;
        assert!((extract_rtt_ms(json).unwrap() - 41.58).abs() < 1e-9);
    }

    #[test]
    fn extract_rtt_ms_malformed_json_returns_none() {
        assert!(extract_rtt_ms("not json").is_none());
    }

    #[test]
    fn extract_rtt_ms_missing_field_returns_none() {
        assert!(extract_rtt_ms(r#"{"BBRInfo":{"BW":1000}}"#).is_none());
    }

    #[tokio::test]
    async fn locate_failure_is_reported_as_failed_not_panicking() {
        let locate = || async { Err::<NdtServer, String>("no server available".to_string()) };

        let result = check_speed(&locate, DEFAULT_TEST_DURATION).await;

        assert_eq!(result.status, CheckStatus::Failed);
        assert!(result.data.is_none());
        assert!(result.errors[0].contains("no server available"));
    }
}
