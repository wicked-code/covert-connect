use anyhow::{Result, anyhow, bail};
use hickory_proto::{
    op::{Header, MessageType, OpCode, ResponseCode},
    rr::{Name, RData, Record, RecordType},
};
use hickory_server::{
    ServerFuture,
    authority::MessageResponseBuilder,
    server::{Request, RequestHandler, ResponseHandler, ResponseInfo},
};
use parking_lot::Mutex;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::net::{TcpListener, UdpSocket};

static DEFAULT_DNS_SERVER_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct DnsHandler {
    dns_mapper: Arc<DnsMapper>,
}

use crate::tun::dns_mapper::DnsMapper;
pub struct DnsServer {
    dns_mapper: Arc<DnsMapper>,
    server: Mutex<Option<ServerFuture<DnsHandler>>>,
}

impl DnsHandler {
    async fn handle<H: ResponseHandler>(&self, request: &Request, mut response_handle: H) -> Result<ResponseInfo> {
        if request.op_code() != OpCode::Query {
            bail!("invalid OP code: {}", request.op_code());
        }

        if request.message_type() != MessageType::Query {
            bail!("invalid message type: {}", request.message_type());
        }

        // ignore multiple queries
        let query = request.queries().first().ok_or_else(|| anyhow!("no query"))?;

        let builder = MessageResponseBuilder::from_message_request(request);
        let mut header = Header::response_from_request(request.header());

        if query.query_type() == RecordType::AAAA {
            header.set_authoritative(true);

            let response = builder.build_no_records(header);
            return Ok(response_handle.send_response(response).await?);
        }

        // TODO: ??? redirect record types other than A to outbound DNS server

        let ip_record = self.dns_mapper.resolve(query.name().to_string().as_str().trim_end_matches('.'));

        let records = [Record::from_rdata(
            Name::from(query.name().clone()),
            ip_record
                .expire_time
                .duration_since(std::time::Instant::now())
                .as_secs() as u32,
            RData::A(ip_record.ip.into()),
        )];
        let response = builder.build(header, &records, &[], &[], &[]);
        Ok(response_handle.send_response(response).await?)
    }
}

#[async_trait::async_trait]
impl RequestHandler for DnsHandler {
    async fn handle_request<H: ResponseHandler>(&self, request: &Request, response_handle: H) -> ResponseInfo {
        // TODO: remove trace
        tracing::info!(
            "got dns request [{}][{:?}][{:?}] from {}",
            request.protocol(),
            request.queries().first().map(|x| x.query_type()),
            request.queries().first().map(|x| x.name()),
            request.src()
        );

        self.handle(request, response_handle).await.unwrap_or_else(|e| {
            // TODO: remove trace or replace to debug
            tracing::info!("dns request error: {}", e);
            let mut h = Header::new();
            h.set_response_code(ResponseCode::ServFail);
            h.into()
        })
    }
}

impl DnsServer {
    pub fn new(dns_mapper: Arc<DnsMapper>) -> Arc<Self> {
        Arc::new(Self {
            dns_mapper,
            server: Mutex::new(None),
        })
    }

    pub async fn stop(self: &Arc<Self>) -> Result<()> {
        self.dns_mapper.stop().await;
        if let Some(mut server) = self.server.lock().take() {
            server.shutdown_gracefully().await?;
        }

        Ok(())
    }

    pub async fn start(self: &Arc<Self>, addr: Ipv4Addr) -> Result<()> {
        self.dns_mapper.start().await?;

        let handler = DnsHandler {
            dns_mapper: self.dns_mapper.clone(),
        };
        let mut s = ServerFuture::new(handler);

        let mut has_server = UdpSocket::bind(SocketAddr::new(IpAddr::V4(addr), 53))
            .await
            .map(|x| {
                tracing::info!("UDP dns server listening on: {}", addr);
                s.register_socket(x);
            })
            .inspect_err(|x| {
                tracing::error!("failed to listen UDP DNS server on {}: {}", addr, x);
            })
            .is_ok();

        has_server |= TcpListener::bind(SocketAddr::new(IpAddr::V4(addr), 53))
            .await
            .map(|x| {
                tracing::info!("TCP dns server listening on: {}", addr);
                s.register_listener(x, DEFAULT_DNS_SERVER_TIMEOUT);
            })
            .inspect_err(|x| {
                tracing::error!("failed to listen TCP DNS server on {}: {}", addr, x);
            })
            .is_ok();

        if has_server {
            *self.server.lock() = Some(s);
            Ok(())
        } else {
            bail!("failed to start DNS server on {}", addr);
        }
    }
}
