use std::{ffi::OsString, path::PathBuf, sync::atomic::Ordering, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use client::{
    client::{Client, ClientState},
    config::ServerConfig,
    log::init_trace_log,
};
use service_manager::{
    RestartPolicy, ServiceInstallCtx, ServiceManager, ServiceStartCtx, ServiceStatus, ServiceStatusCtx, ServiceStopCtx,
    ServiceUninstallCtx,
};
use tarpc::context;
use tracing_subscriber::EnvFilter;

use cfgmatic_paths::PathsBuilder;
use url::Url;

use crate::client_controller::{ClientController, service_label};
#[cfg(windows)]
use crate::windows_service_runtime::set_failure_and_description;
use client::api::{ClientApiClient, connect_client_api};
use tokio::sync::oneshot;

mod client_controller;
#[cfg(windows)]
mod windows_service_runtime;

/// Covert-Connect client
#[derive(Parser)]
#[command(version, about = "Covert-Connect client", long_about = None)]
struct Cli {
    /// config file path
    #[arg(short, long, value_name = "PATH", value_hint = clap::ValueHint::DirPath)]
    config: Option<PathBuf>,
    /// commands
    #[command(subcommand)]
    command: Commands,
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
    /// Install service
    #[command(visible_alias = "i")]
    Install,
    /// Uninstall service
    #[command(visible_alias = "u")]
    Uninstall,
    /// Start service
    #[command()]
    Start,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Cli = Cli::parse();

    let cfg_path = match args.config {
        Some(path) => path,
        None => find_config_path()?.join("config.toml"),
    };

    #[cfg(windows)]
    if matches!(args.command, Commands::Start)
        && windows_service_runtime::run_as_windows_service_if_needed(cfg_path.clone())?
    {
        return Ok(());
    }

    match args.command {
        Commands::Start => start_client(cfg_path, None, None).await,
        _ => process_command(args.command, cfg_path).await,
    }
}

async fn process_command(command: Commands, cfg_path: PathBuf) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(tracing::Level::INFO.into())
                .from_env_lossy()
                .add_directive("tarpc=warn".parse().unwrap()),
        )
        .without_time()
        .init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"));

    match command {
        Commands::Add { uri } => add_server(uri).await,
        Commands::Del { host } => {
            let client = connect_client_api().await?;
            client
                .del_server(context::current(), host)
                .await?
                .map_err(anyhow::Error::msg)?;
            tracing::info!("Server deleted");
            list_servers(client).await
        }
        Commands::List => {
            let client = connect_client_api().await?;
            list_servers(client).await
        }
        Commands::Monitor => monitor_client().await,
        Commands::State { state } => show_or_set_state(state).await,
        Commands::Install => install(cfg_path).await,
        Commands::Uninstall => uninstall().await,
        Commands::Start => {
            panic!("Start command should be handled in main");
        }
    }
}

pub(crate) async fn start_client(
    cfg_path: PathBuf,
    shutdown_signal: Option<oneshot::Receiver<()>>,
    on_started: Option<Box<dyn FnOnce() -> Result<()> + Send>>,
) -> Result<()> {
    init_trace_log("cc-client")?;
    tracing::info!(version = env!("CARGO_PKG_VERSION"));

    let client = Client::new(cfg_path);
    client.initialize().await?;

    let client_controller = ClientController::new(client.clone());

    let shutdown_listener = match shutdown_signal {
        Some(stop_rx) => {
            let client_clone = client.clone();
            let controller_clone = client_controller.clone();
            tokio::spawn(async move {
                let _ = stop_rx.await;
                controller_clone.stop();
                client_clone.shutdown().await;
            })
        }
        None => {
            let client_clone = client.clone();
            let controller_clone = client_controller.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.unwrap();
                controller_clone.stop();
                client_clone.shutdown().await;
            })
        }
    };

    let client_clone = client.clone();
    let controller_clone = client_controller.clone();
    tokio::spawn(async move {
        if let Err(err) = controller_clone.run().await {
            tracing::error!("Client controller error: {:?}", err);
            client_clone.shutdown().await;
        }
        tracing::info!("API server stopped");
    });

    if let Some(on_started) = on_started {
        on_started()?;
    }

    client.serve().await?;
    tracing::info!("Client stopped");

    client_controller.stop();
    shutdown_listener.abort();
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

pub(crate) fn find_config_path() -> Result<PathBuf> {
    let finder = PathsBuilder::new(env!("CARGO_PKG_NAME")).build();
    if let Some(path) = finder.system_dirs().into_iter().next() {
        Ok(path)
    } else {
        Err(anyhow!("No config file found"))
    }
}

async fn monitor_client() -> Result<()> {
    let client = connect_client_api().await?;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\n");
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
        }
        let state = client.get_state(context::current()).await?;
        println!("State: {:?}          ", state);
        let servers = client.get_servers(context::current()).await?;
        for (idx, srv) in servers.iter().enumerate() {
            print!(
                "{} ({}) In: {} \tOut: {} \tSuccess: {} \tErrors: {}                    {}",
                srv.config.host,
                match &srv.connect_info {
                    Some(info) => info.address.to_string(),
                    None => "not connected".to_string(),
                },
                srv.state.rx_total.load(Ordering::Relaxed),
                srv.state.tx_total.load(Ordering::Relaxed),
                srv.state.success_count.load(Ordering::Relaxed),
                srv.state.err_count.load(Ordering::Relaxed),
                if idx < servers.len() - 1 { "\n" } else { "" }
            );
        }
        print!("\r\x1B[{}F", servers.len());
    }

    Ok(())
}

async fn install(cfg_path: PathBuf) -> Result<()> {
    let label = service_label();

    let manager = <dyn ServiceManager>::native().with_context(|| "Failed to detect management platform")?;

    let exe = std::env::current_exe()?;
    let working_dir = exe.parent().map(|p| p.to_path_buf());

    // Uninstall if existing
    let status = manager.status(ServiceStatusCtx { label: label.clone() })?;
    if status != ServiceStatus::NotInstalled {
        tracing::info!("Service already exists, uninstalling first");
        if status == ServiceStatus::Running {
            manager
                .stop(ServiceStopCtx { label: label.clone() })
                .with_context(|| "Failed to stop existing service")?;
        }

        manager
            .uninstall(ServiceUninstallCtx { label: label.clone() })
            .with_context(|| "Failed to uninstall existing service")?;
    }

    manager
        .install(ServiceInstallCtx {
            label: label.clone(),
            program: exe,
            args: vec![
                OsString::from("--config"),
                cfg_path.into_os_string(),
                OsString::from("start"),
            ],
            contents: None,
            username: None,
            working_directory: working_dir,
            environment: None,
            autostart: true,
            restart_policy: RestartPolicy::OnFailure {
                delay_secs: Some(5),
                max_retries: Some(10),
                reset_after_secs: Some(60),
            },
        })
        .with_context(|| "Failed to install service")?;

    manager
        .start(ServiceStartCtx { label: label.clone() })
        .with_context(|| "Failed to start service")?;

    #[cfg(windows)]
    set_failure_and_description(&label.to_qualified_name(), "Covert-Connect client backend engine").await;

    Ok(())
}

async fn uninstall() -> Result<()> {
    let client = match connect_client_api().await {
        Ok(c) => c,
        Err(err) => {
            let manager = <dyn ServiceManager>::native().with_context(|| "Failed to detect management platform")?;
            match manager.status(ServiceStatusCtx { label: service_label() })? {
                ServiceStatus::NotInstalled => {
                    tracing::info!("Service not installed");
                    return Ok(());
                }
                ServiceStatus::Stopped(_) => {
                    bail!("service stopped, connect error: {:?}", err);
                }
                ServiceStatus::Running => {
                    bail!("service running, connect error: {:?}", err);
                }
            }
        }
    };

    client
        .uninstall_service(context::current())
        .await?
        .map_err(anyhow::Error::msg)?;
    client.shutdown(context::current()).await?;
    Ok(())
}
