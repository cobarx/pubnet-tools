//! UniFFI surface for the pubnetchk audit engine.
//!
//! One call: a `HostSnapshot` JSON string in, the `Report` JSON string out. The
//! report crosses the FFI as a string rather than as generated records — the
//! JSON schema is already a maintained contract shared with the web/HTML report,
//! and this keeps `pubnet_tools::types` free of `uniffi` derives.
//!
//! See docs/epics/pubnet-android/ and docs/specs/android-host-snapshot.md.

use pubnet_platform::platform::snapshot::{HostSnapshot, SnapshotProbe};
use pubnet_tools::audit::{CheckName, RunAuditOptions, VERSION, run_audit_with_probe};
use serde::Deserialize;
use std::time::Duration;

uniffi::setup_scaffolding!();

#[derive(Debug, uniffi::Error)]
pub enum AuditError {
    /// `snapshot_json` did not parse as a `HostSnapshot`.
    // The field is `reason`, not `message`: UniFFI 0.29's Kotlin backend also
    // emits an `override val message` on the generated exception class, and a
    // constructor property literally named `message` collides with it.
    BadSnapshot { reason: String },
    /// `options_json` did not parse.
    BadOptions { reason: String },
    /// The tokio runtime could not be built, or the report could not be
    /// serialized. Not expected in practice.
    Internal { reason: String },
}

impl std::fmt::Display for AuditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditError::BadSnapshot { reason } => write!(f, "invalid host snapshot: {reason}"),
            AuditError::BadOptions { reason } => write!(f, "invalid options: {reason}"),
            AuditError::Internal { reason } => write!(f, "internal error: {reason}"),
        }
    }
}

impl std::error::Error for AuditError {}

/// Options from the Kotlin side. All fields optional; the default runs the
/// checks that work on Android today (topology + security).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AndroidOptions {
    /// Check names to run: any of `topology`, `security`, `reliability`,
    /// `speed`. Unknown names are ignored.
    only: Vec<String>,
    /// Seconds per direction for the speed test (ignored unless `speed` runs).
    speed_duration_secs: u64,
    /// Read Wi-Fi channel/signal detail (the snapshot already carries it, so
    /// this only affects finding text).
    wifi_detail: bool,
}

impl Default for AndroidOptions {
    fn default() -> Self {
        Self {
            only: vec!["topology".to_string(), "security".to_string()],
            speed_duration_secs: 10,
            wifi_detail: true,
        }
    }
}

fn check_name(s: &str) -> Option<CheckName> {
    match s {
        "topology" => Some(CheckName::Topology),
        "security" => Some(CheckName::Security),
        "reliability" => Some(CheckName::Reliability),
        "speed" => Some(CheckName::Speed),
        _ => None,
    }
}

/// Run the audit against a caller-supplied `HostSnapshot`.
///
/// - `snapshot_json` — a `HostSnapshot` (see `docs/specs/android-host-snapshot.md`).
/// - `options_json` — an `AndroidOptions` object, or `"{}"` for the default
///   (topology + security).
///
/// Returns the `Report` as a JSON string (same schema as `pubnetchk --json`).
/// Blocks for the duration of the audit — call it off the main thread.
#[uniffi::export]
fn run_audit_json(snapshot_json: String, options_json: String) -> Result<String, AuditError> {
    let snapshot: HostSnapshot = serde_json::from_str(&snapshot_json)
        .map_err(|e| AuditError::BadSnapshot { reason: e.to_string() })?;
    let opts: AndroidOptions = serde_json::from_str(&options_json)
        .map_err(|e| AuditError::BadOptions { reason: e.to_string() })?;

    let only: Vec<CheckName> = opts.only.iter().filter_map(|s| check_name(s)).collect();
    if only.is_empty() {
        return Err(AuditError::BadOptions {
            reason: "`only` selected no known checks".to_string(),
        });
    }

    let run_options = RunAuditOptions {
        only: Some(only),
        quiet: true,
        exclude_targets: Vec::new(),
        speed_duration: Duration::from_secs(opts.speed_duration_secs.max(1)),
        wifi_detail: opts.wifi_detail,
    };

    let probe = SnapshotProbe::new(snapshot);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| AuditError::Internal { reason: e.to_string() })?;
    let report = runtime.block_on(run_audit_with_probe(&probe, run_options));

    serde_json::to_string(&report).map_err(|e| AuditError::Internal { reason: e.to_string() })
}

/// The engine version — matches the `version` field of the report JSON.
#[uniffi::export]
fn report_schema_version() -> String {
    VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAPSHOT: &str = r#"{
        "defaultRoute":  { "gateway": "192.168.1.1", "device": "wlan0" },
        "interfaceAddr": { "ip": "192.168.1.34", "prefix": 24 },
        "arpNeighbors": [ { "ip": "192.168.1.1", "mac": "a4:2b:b0:11:22:33", "isGateway": true } ],
        "wifi": { "ssid": "CoffeeWiFi", "ssidHidden": false, "encryption": "WPA2",
                  "channel": 6, "frequencyMhz": 2437, "signalPercent": 72 },
        "dns": { "servers": ["192.168.1.1"], "currentServer": "192.168.1.1" },
        "interfaceKind": "wifi"
    }"#;

    #[test]
    fn topology_only_runs_offline_and_returns_a_report() {
        // topology needs no network — this exercises the whole snapshot ->
        // probe -> audit -> JSON pipeline without hitting the wire.
        let json = run_audit_json(SNAPSHOT.to_string(), r#"{ "only": ["topology"] }"#.to_string())
            .expect("audit runs");
        let report: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(report["topology"]["status"], "ok");
        assert_eq!(report["topology"]["data"]["gateway"], "192.168.1.1");
        assert_eq!(report["topology"]["data"]["interface"], "wlan0");
        assert_eq!(report["security"]["status"], "skipped");
        assert!(report["score"]["level"].is_string());
        assert_eq!(report["version"], report_schema_version());
    }

    #[test]
    fn bad_snapshot_is_a_typed_error() {
        let err = run_audit_json("not json".to_string(), "{}".to_string()).unwrap_err();
        assert!(matches!(err, AuditError::BadSnapshot { .. }));
    }

    #[test]
    fn unknown_check_names_are_rejected() {
        let err = run_audit_json(SNAPSHOT.to_string(), r#"{ "only": ["bogus"] }"#.to_string())
            .unwrap_err();
        assert!(matches!(err, AuditError::BadOptions { .. }));
    }
}
