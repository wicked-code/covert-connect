use crypto::config::ProtocolConfig;
use rustc_hash::FxHashMap;
use std::{net::SocketAddr, sync::Arc};

use crate::client_info::{ClientInfo, ServerInfo, ServerState};

pub enum RouteResult {
    Direct,
    Servers(Vec<Arc<ServerContext>>),
    NoRoute,
}

pub struct ServerContext {
    pub host: String,
    pub address: SocketAddr,
    pub protocol: ProtocolConfig,
    pub url_path: Option<String>,
    pub state: Arc<ServerState>,
    pub weight: Option<u8>,
}

pub struct RouterTable {
    servers: Vec<Arc<ServerContext>>,
    apps: FxHashMap<String, Option<Vec<Arc<ServerContext>>>>,
    domains: FxHashMap<String, Option<Vec<Arc<ServerContext>>>>,
}

impl RouterTable {
    pub fn new() -> Arc<Self> {
        Arc::new(RouterTable {
            servers: Default::default(),
            apps: Default::default(),
            domains: Default::default(),
        })
    }

    pub async fn from(info: Arc<ClientInfo>) -> Arc<Self> {
        // add direct first (direct has priority)
        let mut apps = FxHashMap::default();
        for app in info.get_direct_apps().await.iter() {
            apps.insert(app.clone(), None::<Vec<Arc<ServerContext>>>);
        }

        let mut domains = FxHashMap::default();
        for domain in info.get_direct_domains().await.iter() {
            domains.insert(domain.clone(), None::<Vec<Arc<ServerContext>>>);
        }

        //
        let mut servers = Vec::new();
        for srv in info.get_servers().await.iter() {
            if !srv.config.enabled {
                continue;
            }

            let srv_ctx = Arc::new(ServerContext::from(srv));
            for app in srv.config.apps.iter().flatten() {
                match apps.get_mut(app) {
                    Some(Some(list)) => list.push(srv_ctx.clone()),
                    Some(None) => {} // direct app, keep priority
                    None => {
                        apps.insert(app.clone(), Some(vec![srv_ctx.clone()]));
                    }
                }
            }

            for domain in srv.config.domains.iter().flatten() {
                match domains.get_mut(domain) {
                    Some(Some(list)) => list.push(srv_ctx.clone()),
                    Some(None) => {} // direct domain, keep priority
                    None => {
                        domains.insert(domain.clone(), Some(vec![srv_ctx.clone()]));
                    }
                }
            }

            servers.push(srv_ctx);
        }

        Arc::new(RouterTable { servers, apps, domains })
    }

    pub fn servers_by_process(&self, process_name: &str) -> RouteResult {
        match self.apps.get(process_name) {
            Some(Some(servers)) => RouteResult::Servers(servers.clone()),
            Some(None) => RouteResult::Direct, // direct app, no server
            None => RouteResult::NoRoute,
        }
    }

    pub fn servers_by_domain(&self, domain: &str) -> RouteResult {
        match self.domains.get(domain) {
            Some(Some(servers)) => RouteResult::Servers(servers.clone()),
            Some(None) => RouteResult::Direct, // direct domain, no server
            None => RouteResult::NoRoute,
        }
    }

    pub fn servers(&self) -> Vec<Arc<ServerContext>> {
        self.servers.clone()
    }

    pub fn server_by_host(&self, host: &str) -> Option<Arc<ServerContext>> {
        self.servers.iter().find(|s| s.host == host).cloned()
    }
}

impl From<&ServerInfo> for ServerContext {
    fn from(srv: &ServerInfo) -> Self {
        ServerContext {
            host: srv.config.host.clone(),
            address: srv.config.address,
            protocol: srv.config.protocol.clone(),
            url_path: srv.config.url_path.clone(),
            state: srv.state.clone(),
            weight: srv.config.weight,
        }
    }
}
