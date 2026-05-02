use anyhow::{Error, Result};
use async_trait::async_trait;
use std::sync::Arc;

use client::{
    api::ClientApiClient,
    client::{Client, ClientState},
    client_info::ServerInfo,
    config::ServerConfig,
};
use crypto::config::ProtocolConfig;
use tarpc::context;

#[async_trait]
pub trait ClientBackend: Send + Sync + 'static {
    async fn get_state(&self) -> Result<ClientState>;
    async fn set_state(&self, state: ClientState) -> Result<()>;
    async fn get_direct_apps(&self) -> Result<Vec<String>>;
    async fn get_direct_domains(&self) -> Result<Vec<String>>;
    async fn get_servers(&self) -> Result<Vec<ServerInfo>>;
    async fn is_initialized(&self) -> Result<bool>;
    async fn is_working(&self) -> Result<bool>;
    async fn set_enabled(&self, host: String, value: bool) -> Result<()>;
    async fn get_server_protocol(&self, host: String, key: String) -> Result<ProtocolConfig>;
    async fn add_server(&self, config: ServerConfig) -> Result<()>;
    async fn update_server(&self, orig_host: String, config: ServerConfig) -> Result<()>;
    async fn del_server(&self, host: String) -> Result<()>;
    async fn set_domain(&self, domain: String, server_host: String) -> Result<()>;
    async fn remove_domain(&self, domain: String) -> Result<()>;
    async fn set_app(&self, app: String, server_host: String) -> Result<()>;
    async fn remove_app(&self, app: String) -> Result<()>;
    async fn get_ttfb(&self, host: String, domain: String) -> Result<usize>;
    async fn shutdown(&self) -> Result<()>;
}

pub struct LocalBackend(pub Arc<Client>);

#[async_trait]
impl ClientBackend for LocalBackend {
    async fn get_state(&self) -> Result<ClientState> {
        Ok(self.0.get_state().await)
    }
    async fn set_state(&self, state: ClientState) -> Result<()> {
        self.0.set_state(state).await;
        Ok(())
    }
    async fn get_direct_apps(&self) -> Result<Vec<String>> {
        Ok(self.0.get_direct_apps().await)
    }
    async fn get_direct_domains(&self) -> Result<Vec<String>> {
        Ok(self.0.get_direct_domains().await)
    }
    async fn get_servers(&self) -> Result<Vec<ServerInfo>> {
        Ok(self.0.get_servers().await)
    }
    async fn is_initialized(&self) -> Result<bool> {
        Ok(self.0.is_initialized())
    }
    async fn is_working(&self) -> Result<bool> {
        Ok(self.0.is_working())
    }
    async fn set_enabled(&self, host: String, value: bool) -> Result<()> {
        self.0.set_enabled(&host, value).await
    }
    async fn get_server_protocol(&self, host: String, key: String) -> Result<ProtocolConfig> {
        self.0.get_server_protocol(&host, &key).await
    }
    async fn add_server(&self, config: ServerConfig) -> Result<()> {
        self.0.add_server(config).await;
        Ok(())
    }
    async fn update_server(&self, orig_host: String, config: ServerConfig) -> Result<()> {
        self.0.update_server(&orig_host, config).await
    }
    async fn del_server(&self, host: String) -> Result<()> {
        self.0.del_server(&host).await
    }
    async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        self.0.set_domain(domain, server_host).await
    }
    async fn remove_domain(&self, domain: String) -> Result<()> {
        self.0.remove_domain(domain).await
    }
    async fn set_app(&self, app: String, server_host: String) -> Result<()> {
        self.0.set_app(app, server_host).await
    }
    async fn remove_app(&self, app: String) -> Result<()> {
        self.0.remove_app(app).await
    }
    async fn get_ttfb(&self, host: String, domain: String) -> Result<usize> {
        self.0.get_ttfb(&host, &domain).await
    }
    async fn shutdown(&self) -> Result<()> {
        self.0.shutdown().await;
        Ok(())
    }
}

pub struct RemoteBackend(pub ClientApiClient);

#[inline]
fn ctx() -> context::Context {
    context::current()
}

#[async_trait]
impl ClientBackend for RemoteBackend {
    async fn get_state(&self) -> Result<ClientState> {
        Ok(self.0.get_state(ctx()).await?)
    }
    async fn set_state(&self, state: ClientState) -> Result<()> {
        self.0.set_state(ctx(), state).await?.map_err(Error::msg)
    }
    async fn get_direct_apps(&self) -> Result<Vec<String>> {
        Ok(self.0.get_direct_apps(ctx()).await?)
    }
    async fn get_direct_domains(&self) -> Result<Vec<String>> {
        Ok(self.0.get_direct_domains(ctx()).await?)
    }
    async fn get_servers(&self) -> Result<Vec<ServerInfo>> {
        Ok(self.0.get_servers(ctx()).await?)
    }
    async fn is_initialized(&self) -> Result<bool> {
        Ok(self.0.is_initialized(ctx()).await?)
    }
    async fn is_working(&self) -> Result<bool> {
        Ok(self.0.is_working(ctx()).await?)
    }
    async fn set_enabled(&self, host: String, value: bool) -> Result<()> {
        self.0.set_enabled(ctx(), host, value).await?.map_err(Error::msg)
    }
    async fn get_server_protocol(&self, host: String, key: String) -> Result<ProtocolConfig> {
        self.0.get_server_protocol(ctx(), host, key).await?.map_err(Error::msg)
    }
    async fn add_server(&self, config: ServerConfig) -> Result<()> {
        self.0.add_server(ctx(), config).await?;
        Ok(())
    }
    async fn update_server(&self, orig_host: String, config: ServerConfig) -> Result<()> {
        self.0
            .update_server(ctx(), orig_host, config)
            .await?
            .map_err(Error::msg)
    }
    async fn del_server(&self, host: String) -> Result<()> {
        self.0.del_server(ctx(), host).await?.map_err(Error::msg)
    }
    async fn set_domain(&self, domain: String, server_host: String) -> Result<()> {
        self.0.set_domain(ctx(), domain, server_host).await?.map_err(Error::msg)
    }
    async fn remove_domain(&self, domain: String) -> Result<()> {
        self.0.remove_domain(ctx(), domain).await?.map_err(Error::msg)
    }
    async fn set_app(&self, app: String, server_host: String) -> Result<()> {
        self.0.set_app(ctx(), app, server_host).await?.map_err(Error::msg)
    }
    async fn remove_app(&self, app: String) -> Result<()> {
        self.0.remove_app(ctx(), app).await?.map_err(Error::msg)
    }
    async fn get_ttfb(&self, host: String, domain: String) -> Result<usize> {
        self.0.get_ttfb(ctx(), host, domain).await?.map_err(Error::msg)
    }
    async fn shutdown(&self) -> Result<()> {
        // do not shutdown, only tray app can do that
        Ok(())
    }
}
