use anyhow::Result;
use is_terminal::IsTerminal;
use tracing_subscriber::EnvFilter;
use clap::Parser;

mod config;

use config::AppConfig;
use client::proxy::{Proxy, ProxyState};

/// Covert-Connect client
#[derive(Parser)]
struct Cli {
    /// config file path
    #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    config: std::path::PathBuf,
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
    let cfg = AppConfig::new(args.config)?
        .init().await?;

    tracing::info!(version = env!("CARGO_PKG_VERSION"));

    let client = Proxy::new(cfg.proxy_port, ProxyState::All)?;
    client.update_pac_content().await;
    for srv in cfg.servers {
        client.add_server(srv).await;
    }

    let client_clone = client.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        if let Err(err) = client_clone.set_proxy_state(ProxyState::Off).await {
            tracing::error!("Unable to restore proxy settings: {:?}", err);
        } else {
            tracing::info!("proxy settings restored")
        }
    });    

    client.serve().await
}
