use std::sync::Arc;

use anyhow::Result;
use client::{
    client::{Client, ClientState},
    client_info::ServerInfo,
    config::ServerConfig,
    log::{LogLine, get_trace_log},
};
use crypto::config::ProtocolConfig;
use futures::StreamExt;
use service_manager::{ServiceLabel, ServiceManager, ServiceUninstallCtx};
use tarpc::{
    context, serde_transport,
    server::{self, Channel},
};
use tokio_serde::formats::Json;
use tokio_util::sync::CancellationToken;

use client::api::ClientApi;

#[cfg(debug_assertions)]
pub const SERVICE_NAME: &str = "com.wicked-code.cc-client-dbg";
#[cfg(not(debug_assertions))]
pub const SERVICE_NAME: &str = "com.wicked-code.cc-client";

pub fn service_label() -> ServiceLabel {
    SERVICE_NAME.parse().unwrap()
}

#[derive(Clone)]
pub struct ClientController {
    client: Arc<Client>,
    cancel_token: CancellationToken,
}

impl ClientController {
    pub fn new(client: Arc<Client>) -> Self {
        Self {
            client,
            cancel_token: CancellationToken::new(),
        }
    }

    /// Stop the API server loop without shutting down the inner `Client`.
    pub fn stop(&self) {
        self.cancel_token.cancel();
    }

    pub async fn run(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use std::fs;
            use std::os::unix::fs::PermissionsExt;
            use tokio::net::UnixListener;

            use client::api::get_api_socket_path;

            let socket_path = get_api_socket_path();
            let _ = fs::remove_file(&socket_path); // Remove existing socket

            if let Some(parent) = std::path::Path::new(&socket_path).parent() {
                fs::create_dir_all(parent)?;
            }

            let listener = UnixListener::bind(&socket_path)?;
            fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o666))?;
            tracing::info!("Server listening on UDS: {}", socket_path);

            loop {
                let (stream, _) = tokio::select! {
                    result = listener.accept() => result?,
                    _ = self.cancel_token.cancelled() => return Ok(()),
                };

                let transport = serde_transport::new(
                    tokio_util::codec::LengthDelimitedCodec::builder().new_framed(stream),
                    Json::default(),
                );

                let controller = self.clone();
                tokio::spawn(
                    server::BaseChannel::with_defaults(transport)
                        .execute(controller.serve())
                        .for_each_concurrent(None, |response| async move {
                            response.await;
                        }),
                );
            }
        }

        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            use windows::Win32::Foundation::FALSE;
            use windows::Win32::Security::{
                InitializeSecurityDescriptor, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR,
                SetSecurityDescriptorDacl,
            };
            use windows::Win32::System::SystemServices::SECURITY_DESCRIPTOR_REVISION;

            use client::api::get_api_pipe_name;

            let pipe_name = get_api_pipe_name();
            tracing::info!("Server listening on Named Pipe: {}", pipe_name);

            loop {
                let server = {
                    let mut sd = SECURITY_DESCRIPTOR::default();

                    unsafe {
                        let psd = PSECURITY_DESCRIPTOR(&mut sd as *mut _ as *mut _);

                        InitializeSecurityDescriptor(psd, SECURITY_DESCRIPTOR_REVISION)
                            .map_err(std::io::Error::other)?;

                        // TRUE enables DACL, but passing None for the ACL allows 'Everyone'
                        SetSecurityDescriptorDacl(psd, true, None, false).map_err(std::io::Error::other)?;
                    }

                    let mut sa = SECURITY_ATTRIBUTES {
                        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                        lpSecurityDescriptor: &mut sd as *mut _ as *mut _,
                        bInheritHandle: FALSE,
                    };

                    unsafe {
                        ServerOptions::new()
                            .write_dac(true)
                            .create_with_security_attributes_raw(&pipe_name, &mut sa as *mut _ as *mut _)?
                    }
                };

                tokio::select! {
                    _ = server.connect() => {}
                    _ = self.cancel_token.cancelled() => return Ok(()),
                }

                let transport = serde_transport::new(
                    tokio_util::codec::LengthDelimitedCodec::builder().new_framed(server),
                    Json::default(),
                );

                let controller = self.clone();
                tokio::spawn(
                    server::BaseChannel::with_defaults(transport)
                        .execute(controller.serve())
                        .for_each_concurrent(None, |response| async move {
                            response.await;
                        }),
                );
            }
        }
    }
}

impl ClientApi for ClientController {
    async fn get_state(self, _: context::Context) -> ClientState {
        self.client.get_state().await
    }

    async fn set_state(self, _: context::Context, state: ClientState) -> Result<(), String> {
        self.client.set_state(state).await;
        Ok(())
    }

    async fn get_direct_apps(self, _: context::Context) -> Vec<String> {
        self.client.get_direct_apps().await
    }

    async fn get_direct_domains(self, _: context::Context) -> Vec<String> {
        self.client.get_direct_domains().await
    }

    async fn get_servers(self, _: context::Context) -> Vec<ServerInfo> {
        self.client.get_servers().await
    }

    async fn is_initialized(self, _: context::Context) -> bool {
        self.client.is_initialized()
    }

    async fn is_working(self, _: context::Context) -> bool {
        self.client.is_working()
    }

    async fn set_enabled(self, _: context::Context, host: String, value: bool) -> Result<(), String> {
        self.client.set_enabled(&host, value).await.map_err(|e| e.to_string())
    }

    async fn get_server_protocol(
        self,
        _: context::Context,
        host: String,
        key: String,
    ) -> Result<ProtocolConfig, String> {
        self.client
            .get_server_protocol(&host, &key)
            .await
            .map_err(|e| e.to_string())
    }

    async fn add_server(self, _: context::Context, config: ServerConfig) {
        self.client.add_server(config).await;
    }

    async fn update_server(self, _: context::Context, orig_host: String, config: ServerConfig) -> Result<(), String> {
        self.client
            .update_server(&orig_host, config)
            .await
            .map_err(|e| e.to_string())
    }

    async fn del_server(self, _: context::Context, host: String) -> Result<(), String> {
        self.client.del_server(&host).await.map_err(|e| e.to_string())
    }

    async fn set_domain(self, _: context::Context, domain: String, server_host: String) -> Result<(), String> {
        self.client
            .set_domain(domain, server_host)
            .await
            .map_err(|e| e.to_string())
    }

    async fn remove_domain(self, _: context::Context, domain: String) -> Result<(), String> {
        self.client.remove_domain(domain).await.map_err(|e| e.to_string())
    }

    async fn set_app(self, _: context::Context, app: String, server_host: String) -> Result<(), String> {
        self.client.set_app(app, server_host).await.map_err(|e| e.to_string())
    }

    async fn remove_app(self, _: context::Context, app: String) -> Result<(), String> {
        self.client.remove_app(app).await.map_err(|e| e.to_string())
    }

    async fn get_ttfb(self, _: context::Context, host: String, domain: String) -> Result<usize, String> {
        self.client.get_ttfb(&host, &domain).await.map_err(|e| e.to_string())
    }

    async fn get_log(
        self,
        _: context::Context,
        start: Option<u64>,
        end: Option<u64>,
        limit: usize,
    ) -> Result<Vec<LogLine>, String> {
        get_trace_log(start, end, limit).await.map_err(|e| e.to_string())
    }

    async fn shutdown(self, _: context::Context) {
        self.cancel_token.cancel();
        self.client.shutdown().await;
    }

    async fn uninstall_service(self, _: context::Context) -> Result<(), String> {
        let label = service_label();

        let manager = <dyn ServiceManager>::native().map_err(|e| format!("Failed to detect management platform: {e}"))?;

        manager
            .uninstall(ServiceUninstallCtx { label: label.clone() })
            .map_err(|e| format!("Failed to uninstall service: {e}"))
    }
}
