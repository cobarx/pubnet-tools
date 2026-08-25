//! Port of src/cli.ts: clap setup, single-spinner orchestration ("Analyzing...",
//! "All checks passed" / only-the-issues summary), --json/--save/--only/--strict,
//! and the `record` subcommand. Not yet ported — placeholder below.

use clap::Parser;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name = "conncheck", version = VERSION, about = "Audit the public WiFi or network you just joined.")]
struct Cli {}

pub async fn run() {
    let _cli = Cli::parse();
    println!("conncheck {VERSION} (rust port scaffold — checks not yet ported)");
}
