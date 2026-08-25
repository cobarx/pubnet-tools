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

pub async fn exec_cmd(cmd: &[&str]) -> std::io::Result<ExecResult> {
    let (program, args) = cmd.split_first().expect("exec_cmd requires a non-empty command");
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
