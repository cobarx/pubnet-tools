//! Contract level: verifies our NDT7 protocol implementation against the
//! real M-Lab locate API and real download/upload WebSocket servers.
//! Asserts shape, not exact values - real networks and server load vary.

use conncheck::checks::speed::{check_speed, default_locate};
use conncheck::types::CheckStatus;
use std::time::Duration;

#[tokio::test]
async fn returns_data_or_fails_gracefully() {
    // spec: docs/decisions/2026-08-25-configurable-speed-duration.md
    // A short window here is about this test's own runtime and M-Lab
    // rate-limit pressure from repeated CI/local runs - not a claim
    // about what duration the real default should be. That's still
    // DEFAULT_TEST_DURATION (10s) in speed.rs, unchanged by this test.
    let result = check_speed(&default_locate, Duration::from_secs(3)).await;

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
