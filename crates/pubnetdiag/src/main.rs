use clap::Parser;

#[derive(Parser)]
#[command(
    name = "pubnetdiag",
    version,
    about = "Scan visible Wi-Fi APs and flag WPA2+WPA3 transition-mode issues."
)]
struct Cli {}

fn main() {
    let _ = Cli::parse();
    eprintln!("not yet implemented");
    std::process::exit(1);
}
