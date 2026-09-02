// Re-export pubnet-platform modules so that all existing `crate::exec`,
// `crate::network`, and `crate::platform` references throughout this crate
// continue to resolve without changes.
pub use pubnet_platform::{exec, network, platform};

pub mod audit;
pub mod checks;
pub mod output;
pub mod scoring;
pub mod types;

// The clap CLI and the desktop `run` / `record` entry points. Not built for
// Android — the app drives `audit::run_audit_with_probe` directly.
#[cfg(not(target_os = "android"))]
pub mod cli;
