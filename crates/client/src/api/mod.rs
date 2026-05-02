use crate::{client::ClientState, client_info::ServerInfo, config::ServerConfig, log::LogLine};
use anyhow::Result;
use crypto::config::ProtocolConfig;
use tarpc::serde_transport;
use tokio_serde::formats::Json;

const API_APP_NAME: &str = "covert_connect";

#[cfg(debug_assertions)]
const API_CHANNEL_NAME: &str = "cc_client_debug_api";
#[cfg(not(debug_assertions))]
const API_CHANNEL_NAME: &str = "cc_client_api";

#[cfg(unix)]
pub fn get_api_socket_path() -> String {
    format!("/var/run/{}/{}.sock", API_APP_NAME, API_CHANNEL_NAME)
}
#[cfg(windows)]
pub fn get_api_pipe_name() -> String {
    format!(r"\\.\pipe\{}.{}", API_APP_NAME, API_CHANNEL_NAME)
}

#[tarpc::service]
pub trait ClientApi {
    async fn get_state() -> ClientState;
    async fn set_state(state: ClientState) -> Result<(), String>;
    async fn get_direct_apps() -> Vec<String>;
    async fn get_direct_domains() -> Vec<String>;
    async fn get_servers() -> Vec<ServerInfo>;
    async fn is_initialized() -> bool;
    async fn is_working() -> bool;
    async fn set_enabled(host: String, value: bool) -> Result<(), String>;
    async fn get_server_protocol(host: String, key: String) -> Result<ProtocolConfig, String>;
    async fn add_server(config: ServerConfig);
    async fn update_server(orig_host: String, config: ServerConfig) -> Result<(), String>;
    async fn del_server(host: String) -> Result<(), String>;
    async fn set_domain(domain: String, server_host: String) -> Result<(), String>;
    async fn remove_domain(domain: String) -> Result<(), String>;
    async fn set_app(app: String, server_host: String) -> Result<(), String>;
    async fn remove_app(app: String) -> Result<(), String>;
    async fn get_log(start: Option<u64>, end: Option<u64>, limit: usize) -> Result<Vec<LogLine>, String>;
    async fn get_ttfb(host: String, domain: String) -> Result<usize, String>;
    async fn shutdown();
    async fn uninstall_service();
}

pub async fn connect_client_api() -> Result<ClientApiClient> {
    #[cfg(unix)]
    let stream = {
        use tokio::net::UnixStream;
        UnixStream::connect(get_api_socket_path()).await?
    };

    #[cfg(windows)]
    let stream = {
        use std::time::Duration;
        use tokio::net::windows::named_pipe::ClientOptions;
        use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;

        let pipe_name = get_api_pipe_name();
        loop {
            match ClientOptions::new().open(&pipe_name) {
                Ok(client) => break client,
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
    };

    let transport = serde_transport::new(
        tokio_util::codec::LengthDelimitedCodec::builder().new_framed(stream),
        Json::default(),
    );

    let config = tarpc::client::Config::default();
    Ok(ClientApiClient::new(config, transport).spawn())
}
