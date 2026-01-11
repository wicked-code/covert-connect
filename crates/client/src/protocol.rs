use std::{
    mem, net::SocketAddr, sync::{atomic::{AtomicU64, Ordering}, Arc}
};
use anyhow::{bail, Result};
use tokio::io::{AsyncRead, AsyncWriteExt, AsyncReadExt};
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use chrono::Utc;
use bytes::{Buf, BufMut, BytesMut};
use crypto::{
    cipher::{Cipher, CipherType}, config::{ProtocolConfig, DataPadding}, stream::EncryptedStream, kdf::Kdf,
    MIN_HOST_LEN, GET_PROTOCOL_MAX_CONNECT_DELAY
};
use crate::config::ServerConfig;
use crate::monitor_stream::MonitorStream;

#[derive(Clone)]
pub struct Server {
    pub config: ServerConfig,
    pub state: Arc<ServerState>,
}

#[derive(Default)]
pub struct ServerState {
    pub rx_total: AtomicU64,
    pub tx_total: AtomicU64,
    pub err_count: AtomicU64,  // tunnels with errors i.e. zero data returned from server, used for check healthy connection
    pub succes_count: AtomicU64,  // tunnels with no zero data returned from server, used for check healthy connection
}


pub struct SelectedServer {
    pub host: String,
    pub address: SocketAddr,
    pub protocol: ProtocolConfig,
    pub url_path: Option<String>,
    pub state: Arc<ServerState>,
}

const MAX_GET_PROTOCOL_HEADER_PADDING: u16 = 4096;
const MIN_GET_PROTOCOL_HEADER_PADDING: u16 = 177;

pub async fn get_server_protocol(
    mut stream: impl AsyncWriteExt + Unpin + AsyncRead,
    key: &str
) -> Result<ProtocolConfig> {
    // protocol description in /doc/protocol.md

    let kdf = Kdf::Argon2;
    let cipher_type = CipherType::Aes256Gcm;
    let mut rng = ChaCha20Rng::from_entropy();

    // prepare header        
    let tag_size = cipher_type.tag_size();
    let key_size = cipher_type.key_size();
    let nonce_size = cipher_type.nonce_size();

    // generate salt
    let mut salt = BytesMut::zeroed(key_size);
    rng.fill_bytes(&mut salt);
    
    // create first packet
    let mut packet = BytesMut::with_capacity(
        nonce_size
        + salt.len()
        + mem::size_of::<u16>()
        + tag_size
        + tag_size
        + MAX_GET_PROTOCOL_HEADER_PADDING as usize
    );

    // header cipher
    let mut header_key = BytesMut::zeroed(key_size);
    let timestamp = Utc::now().timestamp_millis() / (GET_PROTOCOL_MAX_CONNECT_DELAY as i64);
    kdf.derive_key_from_timestamp(key.as_bytes(), timestamp, &mut header_key)?;

    let mut header_cipher_aes = Cipher::new(CipherType::Aes256Gcm, &header_key, &mut rng);
    let mut header_cipher_cha = Cipher::new_with_nonce(CipherType::ChaCha20Poly1305, &header_key, header_cipher_aes.nonce());
    
    // create first packet
    packet.put(header_cipher_aes.nonce());

    let padding_size = rng.gen_range(MIN_GET_PROTOCOL_HEADER_PADDING..MAX_GET_PROTOCOL_HEADER_PADDING);
    packet.put(salt.as_ref());
    packet.put_u16(padding_size);

    // encrypt main header part
    header_cipher_cha.encrypt(&mut packet, nonce_size);
    header_cipher_aes.encrypt(&mut packet, nonce_size);

    // add unencrypted padding
    let padding_pos = packet.len();
    packet.resize(padding_pos + padding_size as usize, 0);
    let (_, padding) = packet.as_mut().split_at_mut(padding_pos);

    rng.fill_bytes(padding);

    stream.write_all(packet.as_ref()).await?;
    stream.flush().await?;

    // read response
    let header_len = 
        mem::size_of::<u16>()
        + mem::size_of::<u16>()
        + tag_size
        + tag_size;

    let mut header = BytesMut::zeroed(header_len);
    if stream.read(header.as_mut()).await? != header_len {
        bail!("wrong header size");
    }

    let mut response_key = BytesMut::zeroed(key_size);
    kdf.derive_protocol_response_key(key.as_bytes(), &salt, &mut response_key)?;

    let mut cipher_aes = Cipher::new_with_nonce(CipherType::Aes256Gcm, &response_key, &salt[0..nonce_size]);
    let mut cipher_cha = Cipher::new_with_nonce(CipherType::ChaCha20Poly1305, &response_key, &salt[key_size - nonce_size..key_size]);
    
    if !cipher_cha.decrypt(&mut header) {
        bail!("can't decrypt header");
    }

    header.truncate(header.len() - tag_size);
    if !cipher_aes.decrypt(&mut header) {
        bail!("can't decrypt header");
    }

    cipher_aes.inc_nonce(1);
    cipher_cha.inc_nonce(1);

    let padding_bytes = header.split_to(mem::size_of::<u16>());
    let padding_start = u16::from_be_bytes(padding_bytes.as_ref().try_into().unwrap()) as usize;

    let padding_bytes = header.split_to(mem::size_of::<u16>());
    let padding_end = u16::from_be_bytes(padding_bytes.as_ref().try_into().unwrap()) as usize;

    let data_len = padding_start
        + mem::size_of::<u8>()  // kdf
        + mem::size_of::<u8>()  // cipher
        + mem::size_of::<u16>() // max_connect_delay
        + mem::size_of::<u16>() // header_padding.start
        + mem::size_of::<u16>() // header_padding.end
        + mem::size_of::<u16>() // data_padding.max
        + mem::size_of::<u8>()  // data_padding.rate
        + mem::size_of::<u64>()  // encryption_limit
        + padding_end
        + tag_size
        + tag_size;

    let mut data = BytesMut::zeroed(data_len);
    stream.read_exact(data.as_mut()).await?;
        
    if !cipher_cha.decrypt(&mut data) {
        bail!("can't decrypt data");
    }

    data.truncate(data.len() - tag_size);
    if !cipher_aes.decrypt(&mut data) {
        bail!("can't decrypt data");
    }

    let mut payload = data.split_off(padding_start);
    
    let kdf: Kdf = Kdf::try_from(payload.get_u8())?;
    let cipher: CipherType = CipherType::try_from(payload.get_u8())?;
    let max_connect_delay = payload.get_u16();
    let header_padding = payload.get_u16()..payload.get_u16();
    let data_padding = DataPadding {max: payload.get_u16(), rate: payload.get_u8()};
    let encryption_limit = payload.get_u64() as usize;
    
    let key = key.to_owned();
    Ok(ProtocolConfig{
        key,
        kdf,
        cipher,
        max_connect_delay,
        header_padding,
        data_padding,
        encryption_limit,
    })
}

pub async fn process_tunnel(
    mut server: impl AsyncWriteExt + Unpin + AsyncRead,
    mut client: impl AsyncWriteExt + Unpin + AsyncRead,
    host: String,
    mut rng: impl CryptoRng + Rng,
    selected_server: SelectedServer,
) -> Result<()> 
{
    let ProtocolConfig {
        key, kdf, cipher: cipher_type, header_padding, ..
    } = &selected_server.protocol;

    // prepare header        
    let key_size = cipher_type.key_size();
    let nonce_size = cipher_type.nonce_size();

    // generate salt
    let mut salt = BytesMut::zeroed(key_size);
    rng.fill_bytes(&mut salt);

    // create first packet
    let mut packet = BytesMut::with_capacity(
        nonce_size
        + salt.len()
        + mem::size_of::<u16>()
        + mem::size_of::<u8>()
        + cipher_type.tag_size()
        + u8::MAX as usize // max host len (saved as u8)
        + cipher_type.tag_size()
        + header_padding.end as usize
    );

    // header cipher
    let mut header_key = BytesMut::zeroed(key_size);
    let timestamp = Utc::now().timestamp_millis() / (selected_server.protocol.max_connect_delay as i64);
    kdf.derive_key_from_timestamp(key.as_bytes(), timestamp, &mut header_key)?;

    let mut header_cipher = Cipher::new(*cipher_type, &header_key, &mut rng);
    
    // create first packet
    packet.put(header_cipher.nonce());

    let padding_size = rng.gen_range(header_padding.start..header_padding.end);
    packet.put(salt.as_ref());
    packet.put_u16(padding_size);
    packet.put_u8((host.len() - MIN_HOST_LEN) as u8);

    // encrypt main header part
    header_cipher.encrypt(&mut packet, nonce_size);
    header_cipher.inc_nonce(padding_size);

    // add host and encrypt
    let header_main_size = packet.len();
    packet.put(host.as_bytes());
    header_cipher.encrypt(&mut packet, header_main_size);

    // add unencrypted padding
    let padding_pos = packet.len();
    packet.resize(padding_pos + padding_size as usize, 0);
    let (_, padding) = packet.as_mut().split_at_mut(padding_pos);

    rng.fill_bytes(padding);

    server.write_all(packet.as_ref()).await?;
    server.flush().await?;

    let (client_cipher, server_cipher) =
        Cipher::new_client_server(*cipher_type, *kdf, key, &salt)?;

    let server = EncryptedStream::from_stream(
        server,
        server_cipher,
        client_cipher,
        selected_server.protocol.data_padding,
        selected_server.protocol.encryption_limit,
        rng
    );
    
    let mut server = MonitorStream::from_stream(server, selected_server.state.clone());
    
    let result = tokio::io::copy_bidirectional(&mut client, &mut server).await;

    if !server.is_success() {
        selected_server.state.err_count.fetch_add(1, Ordering::Relaxed);
    }

    result?;
    Ok(())
}

impl From<&Server> for SelectedServer {
    fn from(srv: &Server) -> Self {
        SelectedServer {
            host: srv.config.host.clone(),
            address: srv.config.address,
            protocol: srv.config.protocol.clone(),
            url_path: srv.config.url_path.clone(),
            state: srv.state.clone(),
        }
    }
}