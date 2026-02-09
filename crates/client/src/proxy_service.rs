use std::{
    net::SocketAddr,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::{
    net::TcpListener,
    select,
    sync::{Mutex, oneshot},
};

use axum::{
    body::Body,
    extract::Request,
    http::{Method, StatusCode, uri},
    response::{IntoResponse, Response},
};
use hyper::{body::Incoming, server::conn::http1};
use hyper_util::rt::TokioIo;
use tower::util::ServiceExt;

use anyhow::{Result, anyhow};

use crate::{
    pac_file_service::PacFileService,
    router::{Router, RouterState},
};

pub struct ProxyService {
    pac_service: Arc<PacFileService>,
    restart: Mutex<Option<oneshot::Sender<()>>>,
    enabled: AtomicBool,
    router: Weak<Router>,
}

impl ProxyService {
    pub fn new(proxy_port: u16, router: Weak<Router>) -> Arc<Self> {
        Arc::new(Self {
            router: router,
            pac_service: PacFileService::new(proxy_port),
            restart: Default::default(),
            enabled: Default::default(),
        })
    }

    pub async fn add_domains(&self, domains: &Vec<String>) {
        self.pac_service.add_domains(domains).await;
    }

    pub async fn add_domains_and_reset(&self, domains: &Vec<String>) -> Result<()> {
        self.pac_service.add_domains(domains).await;
        self.reset_proxy().await
    }

    pub async fn remove_domain(&self, domain: &str) -> Result<()> {
        self.pac_service.remove_domain(&domain).await?;
        self.reset_proxy().await
    }

    pub async fn get_domains(&self) -> Vec<String> {
        self.pac_service.get_domains().await
    }

    pub fn get_proxy_address(&self) -> SocketAddr {
        self.pac_service.get_proxy_address()
    }

    pub async fn set_proxy_port(&self, port: u16) -> Result<()> {
        self.pac_service.set_new_port(port);
        if self.enabled() {
            self.restart().await?;
            self.reset_proxy().await
        } else {
            Ok(())
        }
    }

    async fn reset_proxy(&self) -> Result<()> {
        if !self.enabled() {
            return Ok(());
        }

        let state = self.router.upgrade().unwrap().get_state().await;
        if state == RouterState::Smart {
            self.pac_service.update_content().await;
            return self.pac_service.set_proxy_pac().await;
        } else {
            Ok(())
        }
    }

    async fn restart(&self) -> Result<()> {
        self.restart
            .lock()
            .await
            .take()
            .ok_or_else(|| anyhow!("restart channel not initialized"))?
            .send(())
            .map_err(|_| anyhow!("receiver dropped"))
    }

    pub async fn stop(&self) -> Result<()> {
        self.enabled.store(false, Ordering::Relaxed);
        self.restart().await
    }

    pub async fn serve(&self, router_state: RouterState) -> Result<()> {
        let pac_router = self.pac_service.new_router().await?;

        let router = self.router.upgrade().unwrap();
        let handle_request = move |request: Request<Incoming>, client_addr: SocketAddr| {
            let pac_router = pac_router.clone();
            let req = request.map(Body::new);
            let router = router.clone();
            async move {
                if req.method() == Method::CONNECT {
                    ProxyService::serve_proxy_connection(req, router, client_addr).await
                } else if req.uri().scheme() == Some(&uri::Scheme::HTTP) {
                    let loc = req.uri().to_string().replace("http", "https");
                    Ok((StatusCode::TEMPORARY_REDIRECT, [("Location", loc.as_str())]).into_response())
                } else {
                    pac_router.oneshot(req).await.map_err(|err| match err {})
                }
            }
        };

        self.set_proxy_state(router_state).await?;

        let (restart_tx, mut restart_rx) = oneshot::channel();
        self.restart.lock().await.replace(restart_tx);

        let proxy_address = self.pac_service.get_proxy_address();
        let mut listener = TcpListener::bind(proxy_address).await?;

        self.enabled.store(true, Ordering::Relaxed);

        tracing::info!("proxy server started: {:?}", proxy_address);

        loop {
            select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, client_addr)) => {
                            let io = TokioIo::new(stream);
                            let handle_request = handle_request.clone();
                            tokio::task::spawn(async move {
                                let service =
                                    hyper::service::service_fn(move |request: Request<Incoming>| handle_request(request, client_addr));

                                if let Err(err) = http1::Builder::new()
                                    .preserve_header_case(true)
                                    .title_case_headers(true)
                                    .serve_connection(io, service)
                                    .with_upgrades()
                                    .await
                                {
                                    tracing::info!("Failed to serve connection: {:?}", err);
                                }
                            });
                        }
                        Err(error) => {
                            drop(listener);
                            tracing::error!("accept failed: {:?}", error);
                            listener = TcpListener::bind(proxy_address).await?;
                        }
                    }
                },
                _ = &mut restart_rx => {
                    if !self.enabled() {
                        drop(listener);
                        self.pac_service.restore_proxy().await.err().map(|e| tracing::error!("failed to restore proxy: {:?}", e));
                        tracing::info!("proxy server stopped");
                        return Ok(());
                    }

                    let (new_restart_tx, new_restart_rx) = oneshot::channel();
                    restart_rx = new_restart_rx;
                    self.restart.lock().await.replace(new_restart_tx);

                    let proxy_address = self.pac_service.get_proxy_address();
                    listener = TcpListener::bind(proxy_address).await?;
                }
            }
        }
    }

    pub async fn serve_proxy_connection(
        req: Request,
        router: Arc<Router>,
        client_addr: SocketAddr,
    ) -> Result<Response, hyper::Error> {
        if let Some(host_addr) = req.uri().authority().map(|auth| auth.to_string()) {
            tokio::task::spawn(async move {
                match hyper::upgrade::on(req).await {
                    Ok(upgraded) => {
                        let client = TokioIo::new(upgraded);
                        if let Err(e) = router.start_tunnel(client, host_addr, client_addr).await {
                            if let Some(io_err) = e.downcast_ref::<std::io::Error>()
                                && io_err.kind() == std::io::ErrorKind::UnexpectedEof
                            {
                                // suppress logging of unexpected eof errors
                                // https://docs.rs/rustls/latest/rustls/manual/_03_howto/index.html#unexpected-eof
                                return;
                            }

                            tracing::warn!("server io error: {}", e);
                        };
                    }
                    Err(e) => tracing::warn!("upgrade error: {}", e),
                }
            });

            Ok(Response::new(Body::empty()))
        } else {
            tracing::warn!("CONNECT host is not socket addr: {:?}", req.uri());
            Ok(StatusCode::BAD_REQUEST.into_response())
        }
    }

    pub async fn set_proxy_state(&self, state: RouterState) -> Result<()> {
        match state {
            RouterState::All => self.pac_service.set_proxy_all().await,
            RouterState::Smart => self.pac_service.set_proxy_pac().await,
            RouterState::Off => self.pac_service.restore_proxy().await,
        }
    }

    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}
