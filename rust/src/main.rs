mod checks;
mod cli;
mod exec;
mod network;
mod output;
mod scoring;
mod types;

#[tokio::main]
async fn main() {
    cli::run().await;
}
