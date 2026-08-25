//! Port of src/utils/exec.ts: spawn wrapper, no shell injection (array args),
//! never fails the caller on non-zero exit — only a real spawn failure
//! (binary not found) surfaces as an `Err`.

use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

/// Takes owned `Vec<String>` (rather than `&[&str]`) so this same signature
/// works as the generic `Fn(Vec<String>) -> impl Future<...>` bound every
/// check is written against — a fake exec function in tests can then own
/// its captured expectations without fighting borrow lifetimes.
pub async fn exec_cmd(cmd: Vec<String>) -> std::io::Result<ExecResult> {
    let mut iter = cmd.into_iter();
    let program = iter.next().expect("exec_cmd requires a non-empty command");
    let args: Vec<String> = iter.collect();
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .await?;

    Ok(ExecResult {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
    })
}

/// Small helper for building command vectors at call sites:
/// `cmd(&["ip", "route", "show", "default"])`.
pub fn cmd(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}
