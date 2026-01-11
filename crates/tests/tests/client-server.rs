use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::time::{sleep, Duration};

use anyhow::Result;
use crypto::{cipher::CipherType, config::ProtocolConfig, kdf::Kdf, DataPadding};
use client::proxy::{ProxyState, Proxy};

const KEY: &str = r#"ZrDj5S25tK0wVXFnlEC_yNBemc6yLsa4iYnf1vRB_7A"#;

#[tokio::test]
async fn get_server_protocol() -> Result<()> {

    let protocol = ProtocolConfig {
        key: KEY.to_owned(),
        kdf: Kdf::Blake3,
        cipher: CipherType::ChaCha20Poly1305,
        max_connect_delay: 10000,
        header_padding: 50..777,
        encryption_limit: 1024,
        data_padding: DataPadding { 
            max: 250,
            rate: 10
        }
    };

    let srv_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8383); 
    let proxy_port: u16 = 1085;

    // start server
    let srv_protocol = protocol.clone();
    tokio::task::spawn(async move {
        let cfg = cc_server::config::AppConfig {
            address: srv_address,
            protocol: srv_protocol,
            out_address: None,
            unauth_cooldown: 55..777,
        };

        let url_path = Kdf::derive_url_path(&cfg.protocol.key)?;
    
        cc_server::server::serve(cfg, url_path, false).await
    });

    let srv_cfg = client::config::ServerConfig {
        caption: None,
        host: srv_address.to_string(),
        weight: None,
        domains: None,
        enabled: true,
        protocol: protocol.clone(),
        address: srv_address,
        url_path: None,
    };

    let client = Proxy::new(proxy_port, ProxyState::Off)?;
    client.update_pac_content().await;
    client.add_server(srv_cfg.clone()).await;

    let srv_protocol = client.get_server_protocol(&srv_cfg.host, KEY).await?;
    assert_eq!(protocol, srv_protocol);

    Ok(())
}

#[tokio::test]
async fn simple_connect() -> Result<()> {
    let protocol = ProtocolConfig {
        key: KEY.to_owned(),
        kdf: Kdf::Blake3,
        cipher: CipherType::ChaCha20Poly1305,
        max_connect_delay: 10000,
        header_padding: 50..777,
        encryption_limit: 1024,
        data_padding: DataPadding { 
            max: 250,
            rate: 10
        }
    };

    let srv_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8385); 
    let proxy_port: u16 = 1087;

    // start server
    let srv_protocol = protocol.clone();
    tokio::task::spawn(async move {
        let cfg = cc_server::config::AppConfig {
            address: srv_address,
            protocol: srv_protocol,
            out_address: None,
            unauth_cooldown: 55..777,
        };

        let url_path = Kdf::derive_url_path(&cfg.protocol.key)?;
    
        cc_server::server::serve(cfg, url_path, false).await
    });

    let srv_protocol = protocol.clone();
    tokio::task::spawn(async move {

        let srv_cfg = client::config::ServerConfig {
            caption: None,
            host: srv_address.to_string(),
            weight: None,
            domains: None,
            enabled: true,
            protocol: srv_protocol,
            address: srv_address,
            url_path: None,
        };

        let client = Proxy::new(proxy_port, ProxyState::Off)?;
        client.update_pac_content().await;
        client.add_server(srv_cfg.clone()).await;

        client.serve().await
    });

    // wait for servers start
    sleep(Duration::from_millis(300)).await;

    let proxy_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), proxy_port);
    let body = reqwest::Client::builder()
        .proxy(reqwest::Proxy::https(proxy_address.to_string())?)
        .build()?
        .get("https://www.google.com")
        .send()
        .await?
        .text()
        .await?;

    assert!(body.starts_with("<!doctype html>"));
    assert!(body.ends_with("</body></html>"));
    assert!(body.contains("google"));
    assert!(body.len() > 10000);

    Ok(())
}