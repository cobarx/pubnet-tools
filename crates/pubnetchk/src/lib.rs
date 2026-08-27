// Re-export pubnet-platform modules so that all existing `crate::exec`,
// `crate::network`, and `crate::platform` references throughout this crate
// continue to resolve without changes.
pub use pubnet_platform::{exec, network, platform};

pub mod checks;
pub mod cli;
pub mod output;
pub mod scoring;
pub mod types;
