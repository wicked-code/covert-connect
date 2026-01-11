use std::{
    mem,
    str,
    ops::Range,
    net::{SocketAddr, IpAddr, Ipv4Addr},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{lookup_host, TcpListener, TcpStream, TcpSocket},
    time::timeout
};
use chrono::Utc;
use anyhow::{anyhow, Result};
use bytes::{BufMut, BytesMut};
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use crate::config::AppConfig;
use crypto::{
    cipher::{Cipher, CipherType}, config::ProtocolConfig, kdf::Kdf, stream::EncryptedStream,
    GET_PROTOCOL_MAX_CONNECT_DELAY, MIN_HOST_LEN
};

pub const LOCAL_HOST: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
pub const MAX_PACKET_SIZE: usize = 0xFFFF; // max TCP packet size

async fn start_tunnel(
    stream: &mut TcpStream,
    socket_addr: SocketAddr,
    connect_time: i64,
    cfg: &AppConfig,
    url_path: &str,
    upgrade_support: bool,
) -> Result<()> {
    let unauth_cooldown = cfg.unauth_cooldown.clone();
    if upgrade_support && socket_addr.ip() == LOCAL_HOST {
        if let Err(err) = process_http_upgrade(stream, url_path).await {
            terminate_slowly(stream, unauth_cooldown).await;
            return Err(err);
        }
    }

    let ProtocolConfig {
        key, kdf, cipher: cipher_type, ..
    } = &cfg.protocol;

    let key_size = cipher_type.key_size();
    let tag_size = cipher_type.tag_size();
    let nonce_size = cipher_type.nonce_size();

    let main_header_len = nonce_size
        + key_size              // salt
        + mem::size_of::<u16>() // padding
        + mem::size_of::<u8>()  // host len
        + tag_size;

    let max_header_len = main_header_len
        + u8::MAX as usize // max host len (saved as u8)
        + tag_size
        + cfg.protocol.header_padding.end as usize;

    let min_header_len = main_header_len
        + MIN_HOST_LEN
        + tag_size
        + cfg.protocol.header_padding.start as usize;

    // read header
    let mut data = BytesMut::with_capacity(max_header_len);
    data.resize(min_header_len, 0);

    let readed = stream.read(data.as_mut()).await?;
    if readed < min_header_len {
        if try_special_request(data, readed, connect_time, stream, cfg).await.is_ok() {
            return Ok(());
        }

        // header should be written in one call
        terminate_slowly(stream, unauth_cooldown).await;
        anyhow::bail!("wrong header packet size");
    }

    let mut header = data.split_to(main_header_len);
    
    let nonce = header.split_to(nonce_size);
    let mut header_key = BytesMut::zeroed(cipher_type.key_size());
    let timestamp_for_key = connect_time / (cfg.protocol.max_connect_delay as i64);

    kdf.derive_key_from_timestamp(key.as_bytes(), timestamp_for_key, &mut header_key)?;
    let mut header_cipher = Cipher::new_with_nonce(*cipher_type, &header_key, nonce.as_ref());

    let header_copy = header.clone();
    if !header_cipher.decrypt(&mut header) {
        // try one more time with timestamp_for_key - 1
        // it's possible that client sent data in a prev interval
        kdf.derive_key_from_timestamp(key.as_bytes(), timestamp_for_key - 1, &mut header_key)?;
        header_cipher = Cipher::new_with_nonce(*cipher_type, &header_key, nonce.as_ref());

        // restore header
        header = header_copy.clone();
        if !header_cipher.decrypt(&mut header) {
            // restore splited data
            let mut restored_data = nonce;
            restored_data.extend_from_slice(header_copy.as_ref());
            restored_data.extend_from_slice(data.as_ref());
            if try_special_request(restored_data, readed, connect_time, stream, cfg).await.is_ok() {
                return Ok(());
            }

            terminate_slowly(stream, unauth_cooldown).await;
            anyhow::bail!("decrypt header failed");
        }
    }

    let salt = header.split_to(key_size);

    let padding_bytes = header.split_to(mem::size_of::<u16>());
    let padding = u16::from_be_bytes(padding_bytes.as_ref().try_into().unwrap());
    
    let host_len_bytes = header.split_to(mem::size_of::<u8>());
    let host_len = u8::from_be_bytes(host_len_bytes.as_ref().try_into().unwrap()) as usize + MIN_HOST_LEN;

    // read the rest
    let rest_header_size = host_len
        + cipher_type.tag_size()
        + padding as usize;

    let readed = data.len();
    data.resize(rest_header_size, 0);

    let rest_readed = stream.try_read(&mut data[readed..])?;
    if readed + rest_readed < rest_header_size {
        // header should be written in one call
        terminate_slowly(stream, unauth_cooldown).await;
        anyhow::bail!("wrong header packet size");
    }

    // decrypt host
    let mut host_data = data.split_to(host_len + tag_size);

    header_cipher.inc_nonce(padding);
    if !header_cipher.decrypt(&mut host_data) {
        terminate_slowly(stream, unauth_cooldown).await;
        anyhow::bail!("decrypt host failed");
    }

    //
    let host = match str::from_utf8(&host_data[..host_len]) {
        Ok(host) => host,
        Err(err) => {
            terminate_slowly(stream, unauth_cooldown).await;
            anyhow::bail!(err.to_owned());
        }
    };

    // prefer ipv4
    let addr = lookup_host(host)
        .await?
        .reduce(|acc, val| if acc.is_ipv6() && val.is_ipv4() { val } else { acc })
        .ok_or_else(|| anyhow!("host {host} notfound"))?;

    let (client_cipher, server_cipher) =
        Cipher::new_client_server(*cipher_type, *kdf, key, &salt)?;

    let mut client = EncryptedStream::from_stream(
        stream,
        client_cipher,
        server_cipher,
        cfg.protocol.data_padding,
        cfg.protocol.encryption_limit,
        ChaCha20Rng::from_entropy()
    );

    let mut out_stream = match cfg.out_address {
        Some(out_addr) if out_addr.is_ipv4() == addr.is_ipv4() => {
            let socket = match out_addr {
                IpAddr::V4(_) => TcpSocket::new_v4()?,
                IpAddr::V6(_) => TcpSocket::new_v6()?,
            };

            socket.bind(SocketAddr::new(out_addr, 0))?;
            socket.connect(addr).await?
        },
        _ => TcpStream::connect(addr).await?
    };

    tracing::info!("CONNECT from {socket_addr} to {addr}");

    tokio::io::copy_bidirectional(&mut client, &mut out_stream).await?;

    Ok(())
}

pub async fn try_special_request(
    mut data: BytesMut,
    readed: usize,
    connect_time: i64,
    stream: &mut TcpStream,
    cfg: &AppConfig
) -> Result<()> {
    // protocol description in /doc/protocol.md

    let range = 77..777;
    let kdf = Kdf::Argon2;
    let cipher_type = CipherType::Aes256Gcm;
    let timestamp_for_key = connect_time / (GET_PROTOCOL_MAX_CONNECT_DELAY as i64);
    let key = &cfg.protocol.key;

    let key_size = cipher_type.key_size();
    let tag_size = cipher_type.tag_size();
    let nonce_size = cipher_type.nonce_size();    

    let header_len = nonce_size
        + key_size              // salt
        + mem::size_of::<u16>() // padding
        + tag_size
        + tag_size;

    if readed < header_len {
        anyhow::bail!("not enough readed");
    }

    let mut header = data.split_to(header_len);

    let nonce = header.split_to(nonce_size);
    let mut header_key = BytesMut::zeroed(cipher_type.key_size());

    kdf.derive_key_from_timestamp(key.as_bytes(), timestamp_for_key, &mut header_key)?;
    let mut header_cipher = Cipher::new_with_nonce(cipher_type, &header_key, nonce.as_ref());

    let header_copy = header.clone();
    if !header_cipher.decrypt(&mut header) {
        // try one more time with timestamp_for_key - 1
        // it's possible that client sent data in a prev interval
        kdf.derive_key_from_timestamp(key.as_bytes(), timestamp_for_key - 1, &mut header_key)?;
        header_cipher = Cipher::new_with_nonce(cipher_type, &header_key, nonce.as_ref());

        // restore header
        header = header_copy.clone();
        if !header_cipher.decrypt(&mut header) {
            anyhow::bail!("decrypt error");
        }
    }

    header.truncate(header.len() - tag_size);
    header_cipher = Cipher::new_with_nonce(CipherType::ChaCha20Poly1305, &header_key, nonce.as_ref());
    if !header_cipher.decrypt(&mut header) {
        anyhow::bail!("decrypt error");
    }

    let salt = header.split_to(key_size);

    let padding_bytes = header.split_to(mem::size_of::<u16>());
    let padding = u16::from_be_bytes(padding_bytes.as_ref().try_into().unwrap()) as usize;

    // read the rest
    data.resize(header_len + padding, 0);
    let readed_rest = stream.try_read(&mut data[readed..])?;
    if readed + readed_rest != header_len + padding {
        anyhow::bail!("not enough readed (padding)");
    }

    // response with config
    let mut rng = ChaCha20Rng::from_entropy();
    let padding_begin: u16 = rng.gen_range(range.clone());
    let padding_end: u16 = rng.gen_range(range);

    let mut response_key = BytesMut::zeroed(key_size);
    kdf.derive_protocol_response_key(cfg.protocol.key.as_bytes(), &salt, &mut response_key)?;

    let mut cipher_aes = Cipher::new_with_nonce(CipherType::Aes256Gcm, &response_key, &salt[0..nonce_size]);
    let mut cipher_cha = Cipher::new_with_nonce(CipherType::ChaCha20Poly1305, &response_key, &salt[key_size - nonce_size..key_size]);

    // prepare data
    let mut response = BytesMut::with_capacity(MAX_PACKET_SIZE);

    // header
    response.put_u16(padding_begin);
    response.put_u16(padding_end);

    cipher_aes.encrypt(&mut response, 0);
    cipher_cha.encrypt(&mut response, 0);

    cipher_aes.inc_nonce(1);
    cipher_cha.inc_nonce(1);

    let payload_start = response.len();

    // padding begin
    response.resize(payload_start + (padding_begin as usize), 0);
    let (_, padding) = response.as_mut().split_at_mut(payload_start);

    rng.fill_bytes(padding);

    // data
    response.put_u8(cfg.protocol.kdf as u8);
    response.put_u8(cfg.protocol.cipher as u8);
    response.put_u16(cfg.protocol.max_connect_delay);
    response.put_u16(cfg.protocol.header_padding.start);
    response.put_u16(cfg.protocol.header_padding.end);
    response.put_u16(cfg.protocol.data_padding.max);
    response.put_u8(cfg.protocol.data_padding.rate);
    response.put_u64(cfg.protocol.encryption_limit as u64);

    // padding end
    let padding_end_start = response.len();
    response.resize(padding_end_start + (padding_end as usize), 0);
    let (_, padding) = response.as_mut().split_at_mut(padding_end_start);

    rng.fill_bytes(padding);

    // encrypt
    cipher_aes.encrypt(&mut response, payload_start);
    cipher_cha.encrypt(&mut response, payload_start);

    stream.write_all(response.as_ref()).await?;
    stream.flush().await?;

    Ok(())
}

pub async fn serve(cfg: AppConfig, url_path: String, upgrade_support: bool) -> Result<()> {
    let listener = TcpListener::bind(&cfg.address).await?;

    tracing::info!("server started: {:?}", cfg.address);

    loop {
        let (mut stream, socket_addr) = listener.accept().await?;

        // replay protection
        let timestamp = Utc::now().timestamp_millis();

        let cfg = cfg.clone();
        let url_path = url_path.clone();
        tokio::spawn(async move {
            if let Err(err) = start_tunnel(&mut stream, socket_addr, timestamp, &cfg, &url_path, upgrade_support).await {
                tracing::error!("{:?}", err);
            }
        });
    }
}

async fn terminate_slowly(stream: &mut TcpStream, cooldown: Range<u16>) {
    // avoid testing for required header size
    let mut rng = ChaCha20Rng::from_entropy();

    let max_read : u16 = rng.gen();
    let max_time_ms = rng.gen_range(cooldown) as u64;

    let mut data = BytesMut::with_capacity(max_read as usize);
    timeout(Duration::from_millis(max_time_ms), stream.read_exact(&mut data)).await.ok();
}

async fn process_http_upgrade(stream: &mut TcpStream, url_path: &str) -> Result<()> {
    // should be from proxy check for Uprage header and skip it
    let mut req_bytes = [0u8;u8::MAX as usize];
    let mut readed = 0;
    let mut lf_in_row = 0;
    while lf_in_row < 2 {
        let byte = stream.read_u8().await?;
        if byte == b'\n' {
            lf_in_row += 1;
        } else if byte != b'\r' {
            lf_in_row = 0;
        }

        req_bytes[readed] = byte;
        readed += 1;

        if readed >= u8::MAX as usize {
            anyhow::bail!("response header with upgrade is too big");    
        }
    }

    let req = String::from_utf8_lossy(&req_bytes[..readed]);
    if req.find(url_path).is_none() || !req.ends_with('\n') {
        // wrong request
        anyhow::bail!("unexpected header from localhost");
    }

    // reply with upgrade
    stream.write_all(
        "\
            HTTP/1.1 101 Switching Protocols\r\n\
            Upgrade: cconnect\r\n\
            Connection: Upgrade\r\n\
            \r\n\
        "
        .as_bytes(),
    ).await?;
    stream.flush().await?;

    Ok(())
}