use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::Parser;
use client::client::{Client, ClientState};
use is_terminal::IsTerminal;
use tracing_subscriber::EnvFilter;

use cfgmatic_paths::PathsBuilder;

/// Covert-Connect client
#[derive(Parser)]
struct Cli {
    /// config file path
    #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let logger = tracing_subscriber::fmt().with_env_filter(
        EnvFilter::builder()
            .with_default_directive(tracing::Level::INFO.into())
            .from_env_lossy(),
    );

    if std::io::stdout().is_terminal() {
        logger.init();
    } else {
        logger.without_time().init();
    }

    let args: Cli = Cli::parse();

    tracing::info!(version = env!("CARGO_PKG_VERSION"));

    let cfg_path = match args.config {
        Some(path) => path,
        None => find_config_path()?.join("config.toml"),
    };
    tracing::info!("config path: {:?}", cfg_path);

    let client = Client::new(cfg_path);
    client.initialize().await?;

    let client_clone = client.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        if let Err(err) = client_clone.set_state(ClientState::Off).await {
            tracing::error!("Unable to restore proxy settings: {:?}", err);
        } else {
            tracing::info!("proxy settings restored")
        }
    });

    client.serve().await
}

fn find_config_path() -> Result<PathBuf> {
    let finder = PathsBuilder::new(env!("CARGO_PKG_NAME")).build();
    if let Some(path) = finder.system_dirs().into_iter().next() {
        Ok(path)
    } else {
        Err(anyhow!("No config file found"))
    }
}
