mod cli;
mod config;
mod error;
mod ssh;
mod state;
mod sync;
mod util;

mod aws;
mod tui;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing - only show nydus logs by default, hide AWS SDK logs
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("nydus=info,warn")),
        )
        .init();

    let args = cli::Cli::parse();

    cli::run(args).await
}
