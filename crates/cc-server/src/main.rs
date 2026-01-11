use std::{
    net::SocketAddr,
    path::Path,
};
use anyhow::Result;
use is_terminal::IsTerminal;
use tracing_subscriber::EnvFilter;
use clap::Parser;
use colored::*;
use crypto::kdf::Kdf;

mod config;
mod server;

use config::AppConfig;
use server::LOCAL_HOST;

/// Covert-Connect server
#[derive(Parser)]
struct Cli {
    /// config file path
    #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    config: std::path::PathBuf,

    #[arg(short, long, help = "HTTPS server path that client use (GET host/<path>). Should be used in server (nginx) as location for proxy.")]
    url: bool,

    #[arg(short, long, help = "Generate new key, update config and exit. New key should be used in client after that.")]
    new_key: bool,
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
    let cfg = AppConfig::new(&args.config)?;

    tracing::info!(version = env!("CARGO_PKG_VERSION"));

    let url_path = Kdf::derive_url_path(&cfg.protocol.key)?;

    if args.url {
        show_proxy_path(cfg.address, &url_path);
        Ok(())
    } else if args.new_key {
        generate_new_key(&args.config, &cfg.protocol.key)
    } else {
        server::serve(cfg, url_path, true).await
    }
}

fn generate_new_key(cfg_path: &Path, old_key: &str) -> Result<()> {
    // generate new key and update config
    let new_key = Kdf::generate_new_key();
    print!("\n{}\n{}\n\n", "new key:".green(), new_key.bold());

    // probably we can use yaml_rust to preserve comments and format
    // but it's actually easier just find old key and replace with new one
    let config = std::fs::read_to_string(cfg_path)?;
    let config = config.replace(old_key, &new_key);
    std::fs::write(cfg_path, config)?;

    print!("Config file {} updated.\nUse new key in the client.\n",
        cfg_path.display().to_string().italic()
    );

    Ok(())
}

fn show_proxy_path(address: SocketAddr, url_path: &str) {
    if address.ip() != LOCAL_HOST {
        tracing::warn!("\n{} {} {} {}", address.ip().to_string().yellow().bold(),
                        "can be visible from outside.\naddress".yellow(), 
                        LOCAL_HOST.to_string().yellow().bold(),
                        "is recommended in https proxy mode.".yellow(),
        );
    }

    print!("\n{}\n{}\n\n", "client path:".green(), url_path.bold());
    print!("{}\n\
            location /{url_path} {{\n\
            \tproxy_pass http://{};\n\
            \tproxy_set_header Upgrade $http_upgrade;\n\
            \tproxy_set_header Connection \"Upgrade\";\n\
            }}\n\n",
            "nginx config example:".green(),
            &address
    );
}