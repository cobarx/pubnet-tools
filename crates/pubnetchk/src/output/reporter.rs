//! Port of src/output/reporter.ts: saves JSON report to
//! <data dir>/reports/<timestamp>.json, only when --save is passed
//! (see docs/decisions/2026-08-25-save-off-by-default.md). The data dir is
//! the platform's per-user one (docs/decisions/2026-09-24-output-locations.md).

use crate::types::Report;
use std::path::{Path, PathBuf};

pub fn default_reports_dir() -> PathBuf {
    data_dir().join("reports")
}

#[cfg(target_os = "macos")]
fn data_dir() -> PathBuf {
    macos_data_dir(&dirs_home())
}

#[cfg(windows)]
fn data_dir() -> PathBuf {
    windows_data_dir(std::env::var("LOCALAPPDATA").ok(), &dirs_home())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn data_dir() -> PathBuf {
    xdg_data_dir(std::env::var("XDG_DATA_HOME").ok(), &dirs_home())
}

/// XDG Base Directory spec 0.8: `$XDG_DATA_HOME`, else `~/.local/share`. A relative
/// value "should [be considered] invalid and ignore[d]".
#[cfg(any(not(any(target_os = "macos", windows)), test))]
fn xdg_data_dir(xdg_data_home: Option<String>, home: &Path) -> PathBuf {
    xdg_data_home
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local").join("share"))
        .join("pubnet-tools")
}

/// Apple: Application Support, in a subdirectory named by bundle identifier.
#[cfg(any(target_os = "macos", test))]
fn macos_data_dir(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join("com.cobarx.pubnet-tools")
}

/// Microsoft KNOWNFOLDERID: `FOLDERID_LocalAppData`, default
/// `%USERPROFILE%\AppData\Local`. Local, not Roaming: a report is about this machine.
#[cfg(any(windows, test))]
fn windows_data_dir(local_appdata: Option<String>, home: &Path) -> PathBuf {
    local_appdata
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("AppData").join("Local"))
        .join("pubnet-tools")
}

fn dirs_home() -> PathBuf {
    // No `dirs` crate (see docs/decisions/2026-09-24-output-locations.md). $HOME covers Linux/macOS
    // (and Git Bash on Windows); %USERPROFILE% is the native-Windows home.
    home_from(
        std::env::var("HOME").ok(),
        std::env::var("USERPROFILE").ok(),
    )
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
    let json =
        serde_json::to_string_pretty(report).expect("Report serialization should never fail");
    tokio::fs::write(&path, json).await?;
    Ok(path)
}

/// Writes the plain-language HTML report to <dir>/pubnetchk-<timestamp>.html
/// and returns the path; the CLI passes the current directory
/// (docs/decisions/2026-09-24-output-locations.md). Unlike the
/// JSON report this is always self-contained (inline CSS, no assets), so the
/// returned path can be handed straight to `xdg-open`.
pub async fn save_html_report(report: &Report, dir: &Path) -> std::io::Result<PathBuf> {
    let filename = format!("pubnetchk-{}.html", report.timestamp.replace(':', "-"));
    let path = dir.join(filename);
    let html = crate::output::html::render_html(report);
    tokio::fs::write(&path, html).await?;
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
            score: ScoreResult {
                total: 0,
                level: RiskLevel::Low,
                findings: vec![],
            },
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

    // docs/decisions/2026-09-24-output-locations.md: the per-user data directory.
    // "/data" is not absolute on Windows (no drive), so the XDG cases are Unix-only.
    #[cfg(unix)]
    #[test]
    fn linux_uses_xdg_data_home_when_absolute() {
        assert_eq!(
            xdg_data_dir(Some("/data".into()), Path::new("/home/x")),
            PathBuf::from("/data/pubnet-tools")
        );
    }

    #[cfg(unix)]
    #[test]
    fn linux_ignores_an_unset_empty_or_relative_xdg_data_home() {
        let default = PathBuf::from("/home/x/.local/share/pubnet-tools");
        assert_eq!(xdg_data_dir(None, Path::new("/home/x")), default);
        assert_eq!(
            xdg_data_dir(Some(String::new()), Path::new("/home/x")),
            default
        );
        assert_eq!(
            xdg_data_dir(Some("data".into()), Path::new("/home/x")),
            default
        );
    }

    #[test]
    fn macos_uses_application_support_by_bundle_id() {
        assert_eq!(
            macos_data_dir(Path::new("/Users/x")),
            PathBuf::from("/Users/x/Library/Application Support/com.cobarx.pubnet-tools")
        );
    }

    #[test]
    fn windows_uses_local_appdata_else_its_default_under_the_profile() {
        assert_eq!(
            windows_data_dir(
                Some("C:\\Users\\x\\AppData\\Local".into()),
                Path::new("C:\\Users\\x")
            ),
            PathBuf::from("C:\\Users\\x\\AppData\\Local").join("pubnet-tools")
        );
        assert_eq!(
            windows_data_dir(None, Path::new("C:\\Users\\x")),
            PathBuf::from("C:\\Users\\x")
                .join("AppData")
                .join("Local")
                .join("pubnet-tools")
        );
    }

    #[test]
    fn reports_live_in_the_data_dir() {
        assert!(
            default_reports_dir().ends_with("pubnet-tools/reports")
                || default_reports_dir().ends_with("com.cobarx.pubnet-tools/reports")
        );
    }

    // docs/decisions/2026-09-24-output-locations.md: a document lands where the person
    // is, named for the tool that wrote it.
    #[tokio::test]
    async fn writes_html_report_into_the_given_dir_named_for_the_tool() {
        let dir = tempfile::tempdir().unwrap();

        let path = save_html_report(&fake_report(), dir.path()).await.unwrap();

        assert_eq!(
            path,
            dir.path().join("pubnetchk-2026-08-24T12-34-56.789Z.html")
        );
        assert!(
            tokio::fs::read_to_string(&path)
                .await
                .unwrap()
                .contains("<html")
        );
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
