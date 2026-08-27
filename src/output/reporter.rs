//! Port of src/output/reporter.ts: saves JSON report to
//! ~/.pubnetchk/reports/<timestamp>.json, only when --save is passed
//! (see docs/decisions/2026-08-25-save-off-by-default.md).

use crate::types::Report;
use std::path::{Path, PathBuf};

pub fn default_reports_dir() -> PathBuf {
    dirs_home().join(".pubnetchk").join("reports")
}

fn dirs_home() -> PathBuf {
    // No `dirs` crate dependency for one lookup. $HOME covers Linux/macOS
    // (and Git Bash on Windows); %USERPROFILE% is the native-Windows home.
    home_from(std::env::var("HOME").ok(), std::env::var("USERPROFILE").ok())
}

fn home_from(home: Option<String>, userprofile: Option<String>) -> PathBuf {
    home.filter(|s| !s.is_empty())
        .or(userprofile.filter(|s| !s.is_empty()))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub async fn save_report(report: &Report, reports_dir: &Path) -> std::io::Result<PathBuf> {
    tokio::fs::create_dir_all(reports_dir).await?;
    let filename = format!("{}.json", report.timestamp.replace(':', "-"));
    let path = reports_dir.join(filename);
    let json = serde_json::to_string_pretty(report).expect("Report serialization should never fail");
    tokio::fs::write(&path, json).await?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn empty<T>(name: &str) -> CheckResult<T> {
        CheckResult {
            name: name.to_string(),
            status: CheckStatus::Ok,
            data: None,
            errors: vec![],
            findings: vec![],
            duration_ms: 1,
        }
    }

    fn fake_report() -> Report {
        Report {
            version: "0.1.0".to_string(),
            timestamp: "2026-08-24T12:34:56.789Z".to_string(),
            security: empty("security"),
            speed: empty("speed"),
            reliability: empty("reliability"),
            topology: empty("topology"),
            score: ScoreResult { total: 0, level: RiskLevel::Low, findings: vec![] },
        }
    }

    #[tokio::test]
    async fn writes_report_as_json_named_from_timestamp_colons_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let report = fake_report();

        let path = save_report(&report, dir.path()).await.unwrap();

        assert_eq!(path, dir.path().join("2026-08-24T12-34-56.789Z.json"));
        let written = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(written.contains("\"version\""));
    }

    #[test]
    fn home_wins_over_userprofile_and_both_over_cwd() {
        assert_eq!(
            home_from(Some("/home/x".into()), Some("C:\\Users\\x".into())),
            PathBuf::from("/home/x")
        );
        assert_eq!(
            home_from(None, Some("C:\\Users\\x".into())),
            PathBuf::from("C:\\Users\\x")
        );
        assert_eq!(home_from(Some(String::new()), None), PathBuf::from("."));
        assert_eq!(home_from(None, None), PathBuf::from("."));
    }

    #[tokio::test]
    async fn creates_reports_directory_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested").join("reports");
        let report = fake_report();

        let path = save_report(&report, &nested).await.unwrap();

        let written = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(written.contains("\"version\""));
    }
}
