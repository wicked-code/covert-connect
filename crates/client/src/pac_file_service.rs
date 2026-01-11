use std::{ net::{SocketAddr, IpAddr, Ipv4Addr}, sync::{atomic::{AtomicU16, Ordering}, Arc}};
use anyhow::{Result, anyhow};
use rand::Rng;
use tokio::sync::RwLock;
use axum::{
    Router,
    routing::get,
    extract::State,
    http::{StatusCode, HeaderValue, header},
    response::{Response, IntoResponse},
};
use const_format::concatcp;

use sys_proxy::SystemProxy;

const HTTP: &str = "http://";
const PAC_ROUTE: &str = "/pac/";
const PAC_ROUTE_FULL: &str = concatcp!(PAC_ROUTE, ":key");
const PAC_CONTENT: &str = r#"
function FindProxyForURL(url, host) {
    if (/^[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}$/.test(host)) {
      if (IsInNet(host, "10.0.0.0", "255.0.0.0")
        || IsInNet(host, "127.0.0.0", "255.0.0.0")
        || IsInNet(host, "172.16.0.0", "255.240.0.0")
        || IsInNet(host, "192.168.0.0", "255.255.0.0")
      ) {
        return "DIRECT"
      }
    }
  
    if (isPlainHostName(host) || shExpMatch(host, "*.local")) {
      return "DIRECT"
    }
  
    list = [__ARRAY__]
  
    parts = host.split('.')
    for (let i = 0; i < list.length; i++) {
      part_count = i + 1;
      if (part_count > parts.length) {
        break
      }
  
      phost = parts.slice(parts.length - part_count).join(".")
      if (list[i].indexOf(phost) != -1) {
        return 'DIRECT;'
      }
    }

    return __PROXY__
}
"#;

pub struct PacFileService {
    pac_content: RwLock<Option<String>>,
    domains: RwLock<Vec<String>>,
    system_proxy: Arc<SystemProxy>,
    proxy_port: AtomicU16,
}

impl PacFileService {
    pub fn new(proxy_port: u16) -> Result<Arc<Self>> {
        Ok(Arc::new(Self {
            pac_content: Default::default(),
            domains: Default::default(),
            system_proxy: Arc::new(SystemProxy::new()?),
            proxy_port: AtomicU16::new(proxy_port),
        }))
    }

    pub fn get_proxy_address(&self) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), self.proxy_port.load(Ordering::Relaxed))
    }

    pub fn set_new_port(&self, port: u16) {
        self.proxy_port.store(port, Ordering::Relaxed);
    }

    pub async fn add_domains(&self, domains: &Vec<String>) {
        self.domains.write().await.extend_from_slice(domains.as_slice());
    }

    pub async fn remove_domain(&self, domain: &str) -> Result<()> {
        let mut wr_domains = self.domains.write().await;
        if let Some(idx) = wr_domains.iter().position(|d| d == domain) {
            wr_domains.remove(idx);
            Ok(())
        } else {
            Err(anyhow!("domain not found"))
        }
    }

    pub async fn get_domains(&self) -> Vec<String> {
        self.domains.read().await.clone()
    }

    pub async fn update_content(&self) {
        let proxy_address_str = &self.get_proxy_address().to_string();
        let proxy = "'PROXY ".to_owned() + proxy_address_str + "'";

        let mut hosts_by_dots : Vec<Vec<String>> = Vec::new();
        let hosts = self.domains.read().await;
        for host in &*hosts {
            let dots_cnt = host.chars().fold(0, |acc, c| acc + (c == '.') as usize) + 1;
           
            if hosts_by_dots.len() < dots_cnt {
                for _ in hosts_by_dots.len()..dots_cnt {
                    hosts_by_dots.push(Vec::new());
                }
            }

            hosts_by_dots[dots_cnt - 1].push(host.clone());
        }

        drop(hosts);
        
        let mut hosts_by_dots_str = String::new();
        for hosts in &hosts_by_dots {
            hosts_by_dots_str.push('[');
            for host in hosts {
                hosts_by_dots_str.push('"');
                hosts_by_dots_str.push_str(host);
                hosts_by_dots_str.push_str("\",");
            }
            hosts_by_dots_str.push_str("],");
        }

        let pac_with_hosts = PAC_CONTENT.replacen("__ARRAY__", &hosts_by_dots_str, usize::MAX);

        *self.pac_content.write().await = Some(pac_with_hosts.replacen("__PROXY__", &proxy, usize::MAX));
    }

    pub async fn set_proxy_all(&self) -> Result<()> {
        self.system_proxy.set_proxy(&(HTTP.to_owned() + &self.get_proxy_address().to_string()))
    }

    pub async fn set_proxy_pac(&self) -> Result<()> {
        let rand_part = format!("{:X}", rand::thread_rng().gen::<u128>());

        let addr_str = &self.get_proxy_address().to_string();
        self.system_proxy.set_pac(&(HTTP.to_owned() + addr_str + PAC_ROUTE + &rand_part))?;
        Ok(())
    }

    pub async fn restore_proxy(&self) -> Result<()> {
        self.system_proxy.restore()
    }

    pub async fn new_router(self: Arc<Self>) -> Result<Router> {
        Ok(Router::new()
            .route(PAC_ROUTE_FULL, get(pac_hander))
            .with_state(self.clone())
        )
    }

}

async fn pac_hander(
    State(state): State<Arc<PacFileService>>
) -> Response {
    let content = state.pac_content.read().await.clone();
    if let Some(content) = content {
        let mut res = content.into_response();
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/x-ns-proxy-autoconfig")
        );
        res
    } else {
        StatusCode::SERVICE_UNAVAILABLE.into_response()
    }
}
