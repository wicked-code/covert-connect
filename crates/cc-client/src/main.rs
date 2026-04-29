use std::{path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use client::{
    client::{Client, ClientState},
    config::ServerConfig,
};
use is_terminal::IsTerminal;
use tarpc::context;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use cfgmatic_paths::PathsBuilder;
use url::Url;

use crate::{
    client_api::{ClientApiClient, connect_client_api},
    client_controller::ClientController,
};

mod client_api;
mod client_controller;

/// Covert-Connect client
#[derive(Parser)]
#[command(version, about = "Covert-Connect client", long_about = None)]
struct Cli {
    /// config file path
    #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    config: Option<PathBuf>,
    /// commands
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliClientState {
    Smart,
    All,
    Off,
}

#[derive(Subcommand)]
enum Commands {
    /// Add server config
    #[command(visible_alias = "a")]
    Add {
        /// uri of the server, e.g. "cc://myurl.com?k=KEY"
        uri: String,
    },
    /// Delete server config
    #[command(visible_alias = "d")]
    Del {
        /// server host, e.g. "myurl.com"
        host: String,
    },
    /// List servers
    #[command(visible_alias = "l")]
    List,
    /// Monitor client status
    #[command(visible_alias = "m")]
    Monitor,
    /// Show or change client state
    #[command(visible_alias = "s")]
    State { state: Option<CliClientState> },
    /// Uninstall service
    #[command(visible_alias = "u")]
    Uninstall,
}

#[tokio::main]
async fn main() -> Result<()> {
    let logger = tracing_subscriber::fmt().with_env_filter(
        EnvFilter::builder()
            .with_default_directive(tracing::Level::INFO.into())
            .from_env_lossy()
            .add_directive("tarpc=warn".parse().unwrap())
    );

    if std::io::stdout().is_terminal() {
        logger.init();
    } else {
        logger.without_time().init();
    }

    let args: Cli = Cli::parse();

    tracing::info!(version = env!("CARGO_PKG_VERSION"));

    if let Some(command) = args.command {
        return process_command(command).await;
    }

    let cfg_path = match args.config {
        Some(path) => path,
        None => find_config_path()?.join("config.toml"),
    };
    
    let client = Client::new(cfg_path);
    client.initialize().await?;

    let cancel_token = CancellationToken::new();

    let client_clone = client.clone();
    let cancel_token_clone = cancel_token.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        stop_client(client_clone, cancel_token_clone).await;
    });

    let client_clone = client.clone();
    let cancel_token_clone = cancel_token.clone();
    tokio::spawn(async move {
        let client_controller = ClientController::new(client_clone.clone(), cancel_token_clone.clone());
        if let Err(err) = client_controller.run().await {
            tracing::error!("Client controller error: {:?}", err);
            stop_client(client_clone, cancel_token_clone).await;
        }
    });

    client.serve(cancel_token).await
}

async fn process_command(command: Commands) -> Result<()> {
    match command {
        Commands::Add { uri } => {
            add_server(uri).await?;
        }
        Commands::Del { host } => {
            let client = connect_client_api().await?;
            client
                .del_server(context::current(), host)
                .await?
                .map_err(anyhow::Error::msg)?;
            tracing::info!("Server deleted");
            list_servers(client).await?;
        }
        Commands::List => {
            let client = connect_client_api().await?;
            list_servers(client).await?;
        }
        Commands::Monitor => {
            println!("Not implemented yet");
        }
        Commands::State { state } => {
            show_or_set_state(state).await?;
        }
        Commands::Uninstall => {
            println!("Not implemented yet");
        }
    }
    Ok(())
}

async fn add_server(uri: String) -> Result<()> {
    let u = Url::parse(&uri)?;
    if u.scheme() != "cc" {
        bail!("Invalid scheme")
    }
    let host = match u.host_str() {
        Some(h) => h.to_string(),
        None => bail!("Host missing in uri"),
    };
    let host = match u.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    };

    let mut key = None;
    let mut caption = None;
    for (k, v) in u.query_pairs() {
        match k.as_ref() {
            "k" => key = Some(v.into_owned()),
            "n" => caption = Some(v.into_owned()),
            _ => {}
        }
    }
    let key = match key.filter(|s| !s.is_empty()) {
        Some(k) => k,
        None => bail!("Key missing in uri"),
    };
    let caption = caption.filter(|s| !s.is_empty());

    let client = connect_client_api().await?;
    let protocol = client
        .get_server_protocol(context::current(), host.clone(), key)
        .await?
        .map_err(anyhow::Error::msg)?;

    client
        .add_server(
            context::current(),
            ServerConfig {
                host,
                caption,
                protocol,
                weight: Default::default(),
                domains: Default::default(),
                apps: Default::default(),
                enabled: true,
            },
        )
        .await?;

    tracing::info!("Server added");
    list_servers(client).await?;
    Ok(())
}

async fn show_or_set_state(state: Option<CliClientState>) -> Result<()> {
    let client = connect_client_api().await?;
    if let Some(new_state) = state {
        let new_state = match new_state {
            CliClientState::Smart => ClientState::Smart,
            CliClientState::All => ClientState::All,
            CliClientState::Off => ClientState::Off,
        };
        client
            .set_state(context::current(), new_state)
            .await?
            .map_err(anyhow::Error::msg)?;
    }
    let current_state = client.get_state(context::current()).await?;
    tracing::info!("Current client state: {:?}", current_state);
    Ok(())
}

async fn list_servers(client: ClientApiClient) -> Result<()> {
    let servers = client.get_servers(context::current()).await?;
    if servers.is_empty() {
        tracing::info!("No servers configured");
    } else {
        tracing::info!("Servers:");
        for srv in servers {
            tracing::info!(
                "{} - {}",
                srv.config.host,
                match srv.connect_info {
                    Some(info) => info.address.to_string(),
                    None => "not connected".to_string(),
                }
            );
        }
    }
    Ok(())
}

fn find_config_path() -> Result<PathBuf> {
    let finder = PathsBuilder::new(env!("CARGO_PKG_NAME")).build();
    if let Some(path) = finder.system_dirs().into_iter().next() {
        Ok(path)
    } else {
        Err(anyhow!("No config file found"))
    }
}

async fn stop_client(client: Arc<Client>, cancel_token: CancellationToken) {
    cancel_token.cancel();
    if let Err(err) = client.set_state(ClientState::Off).await {
        tracing::error!("Unable to restore proxy settings: {:?}", err);
    } else {
        tracing::info!("proxy settings restored");
    }
}
