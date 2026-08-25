//! Contract level: verifies our NDT7 protocol implementation against the
//! real M-Lab locate API and real download/upload WebSocket servers.
//! Asserts shape, not exact values - real networks and server load vary.

use conncheck::checks::speed::{check_speed, default_locate};
use conncheck::types::CheckStatus;

#[tokio::test]
async fn returns_data_or_fails_gracefully() {
    let result = check_speed(&default_locate).await;

    if result.status == CheckStatus::Ok {
        let data = result.data.unwrap();
        assert!(data.download_mbps > 0.0);
        assert!(data.upload_mbps > 0.0);
        assert!(data.latency_ms > 0.0);
        assert!(data.jitter_ms >= 0.0);
        assert_eq!(data.source, "ndt7");
    } else {
        assert_eq!(result.status, CheckStatus::Failed);
        assert!(result.data.is_none());
        assert!(!result.errors.is_empty());
    }
}
