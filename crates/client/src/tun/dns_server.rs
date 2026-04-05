use crate::tun::dns_mapper::DnsMapper;
use anyhow::{Result, anyhow, bail};
use hickory_proto::{
    op::{Header, LowerQuery, MessageType, OpCode, ResponseCode},
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

        Ok(match query.query_type() {
            RecordType::AAAA => {
                // IPv6 support just adds complexity and has no any additional value
                header.set_authoritative(true);

                let response = builder.build_no_records(header);
                response_handle.send_response(response).await?
            }
            RecordType::A => {
                header.set_authoritative(true);

                let ip_record = self
                    .dns_mapper
                    .resolve(query.name().to_string().as_str().trim_end_matches('.'));

                let records = [Record::from_rdata(
                    Name::from(query.name().clone()),
                    ip_record
                        .expire_time
                        .duration_since(std::time::Instant::now())
                        .as_secs() as u32,
                    RData::A(ip_record.ip.into()),
                )];
                let response = builder.build(header, &records, &[], &[], &[]);
                response_handle.send_response(response).await?
            }
            _ => self.forward_to_upstream(query, response_handle).await?,
        })
    }

    async fn forward_to_upstream<R: ResponseHandler>(
        &self,
        query: &LowerQuery,
        /*mut response_handle*/_: R,
    ) -> Result<ResponseInfo> {
        // TODO: ???
        // implment forwarding to upstream DNS server when query type is not A or AAAA
        // to make good quality we need Egress and Server selector here
        // if host should go direct we should use Egress
        // if specific server selected somehow use start_tunnel here
        // overwise just go through tun (no binding)
        // to implement queary take a look at:
        // dns_stream_builder in https://github.com/Watfaq/clash-rs/blob/c414fb750265e9c7f73beb5ab9b6709e4628b4cd/clash-lib/src/app/dns/dns_client.rs#L12

        //Ok(response_handle.send_response(response.into_message()).await?)
        bail!("unsupported query type: {}", query.query_type());
    }
}

#[async_trait::async_trait]
impl RequestHandler for DnsHandler {
    async fn handle_request<H: ResponseHandler>(&self, request: &Request, response_handle: H) -> ResponseInfo {
        self.handle(request, response_handle).await.unwrap_or_else(|e| {
            tracing::error!("dns request error: {}", e);
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
        let server = self.server.lock().take();
        if let Some(mut server) = server {
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
