use std::{
    io::{self, ErrorKind}, mem, pin::{Pin, pin}, task::{ Context, Poll }, cmp,
};
use pin_project_lite::pin_project;
use futures::{ready, Future};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};
use bytes::{BufMut, BytesMut};
use rand_core::{CryptoRng, RngCore};
use rand::Rng;
use super::{cipher::Cipher, DataPadding};

pub const MAX_PACKET_SIZE: usize = 0xFFFF; // max TCP packet size
pub const DEF_PACKET_SIZE: usize = 1534; // MTU default + 2xTagSize(16) + datalen(1)

pin_project! {
    /// A stream wrapper that add rnd padding  and encrypt data
    pub struct EncryptedStream<S, R> 
    {
        #[pin]
        inner: S,

        read_cipher: Cipher,
        read_buffer: BytesMut,
        read_state: ReadState,
        readed: usize,

        write_cipher: Cipher,
        write_buffer: BytesMut,
        write_pos: usize,
        written: usize,

        padding: DataPadding,
        enc_limit: usize,

        rng: R
    }
}

enum ReadState {
    Header,
    Padding{size: usize, data_size: usize},
    Data{size: usize},
    Ready{pos: usize},
}

impl<S, R> EncryptedStream<S, R>
where
    R: CryptoRng + RngCore + Rng,
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn from_stream(
        inner: S,
        read_cipher: Cipher,
        write_cipher: Cipher,
        padding: DataPadding,
        enc_limit: usize,
        rng: R,
    ) -> Self {
        Self { 
            inner,
            read_cipher,
            read_buffer: BytesMut::with_capacity(DEF_PACKET_SIZE),
            read_state: ReadState::Header,
            readed: 0,
            write_cipher,
            write_buffer: BytesMut::with_capacity(DEF_PACKET_SIZE),
            write_pos: 0,
            written: 0,
            enc_limit,
            padding,
            rng
        }
    }

    /// Borrow the inner type.
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// Mut borrow the inner type.
    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Consume this wrapper and get the inner type.
    pub fn into_inner(self) -> S {
        self.inner
    }

    fn poll_read_exact(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        size: usize
    ) -> Poll<io::Result<()>> {
        let mut this = self.as_mut().project();
        if this.read_buffer.capacity() < size {
            this.read_buffer.reserve(size - this.read_buffer.len());
        }
        while this.read_buffer.len() < size {
            // read header
            let mut limited = this.read_buffer.limit(size - this.read_buffer.len());
            let read_buff = this.inner.read_buf(&mut limited);
            let n = ready!(pin!(read_buff).poll(cx))?;
            if n == 0 {
                if !self.read_buffer.is_empty() {
                    return Err(ErrorKind::UnexpectedEof.into()).into();
                } else {
                    break;
                }
            }           

            this = self.as_mut().project();
        }

        Ok(()).into()
    }

    fn poll_write_buffer(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        no_pending: bool,
    ) -> Poll<io::Result<()>> {
        let mut this = self.as_mut().project();
        // write data from buffers
        while *this.write_pos < this.write_buffer.len() {
            match this.inner.as_mut().poll_write(cx, &this.write_buffer[*this.write_pos..]) {
                Poll::Pending => {
                    if no_pending {
                        return Poll::Ready(Ok(()));
                    } 
                    return Poll::Pending;
                },
                Poll::Ready(Ok(n)) => {
                    if n == 0 {
                        return Err(ErrorKind::UnexpectedEof.into()).into();                        
                    }
                    *this.write_pos += n;
                },
                Poll::Ready(Err(err)) => {
                    return Poll::Ready(Err(err))
                },
            }
        }

        *this.write_pos = 0;
        this.write_buffer.clear();

        Poll::Ready(Ok(()))
    }

    fn assemble_data_to_buffer(
        mut self: Pin<&mut Self>,
        buf: &[u8]
    ) {
        let this = self.as_mut().project();
        let tag_size = this.write_cipher.tag_size();

        let mut header_size = mem::size_of::<u16>();
        if this.padding.needed() {
            header_size += mem::size_of::<u16>();
        }
        if *this.written <= *this.enc_limit {
            header_size += tag_size;
        }

        this.write_buffer.reserve(header_size + buf.len() + tag_size);

        // header
        this.write_buffer.put_u16(buf.len() as u16);

        let mut padding = 0;
        if this.padding.needed() {
            let padding_max = cmp::min(
                this.padding.max,
                (((this.padding.rate as usize) * buf.len()) / 100) as u16
            );

            if padding_max > 0 {
                padding = this.rng.gen_range(0..padding_max);
            }

            this.write_buffer.put_u16(padding);
        }

        // encrypt header
        if *this.written <= *this.enc_limit {
            this.write_cipher.encrypt(this.write_buffer, 0);
            this.write_cipher.inc_nonce(cmp::max(padding, 1));
        }

        // padding
        if padding > 0 {
            this.write_buffer.resize(header_size + padding as usize, 0);
            this.rng.fill_bytes(&mut this.write_buffer[header_size..]);
        }

        // data
        this.write_buffer.extend_from_slice(buf);

        // encryp data
        if *this.written <= *this.enc_limit {
            this.write_cipher.encrypt(this.write_buffer, header_size + padding as usize);
            this.write_cipher.inc_nonce(1);
        }

        *this.written += buf.len();
    }
}

impl<S, R> AsyncRead for EncryptedStream<S, R>
where
    R: CryptoRng + RngCore + Rng,
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut this = self.as_mut().project();
        let tag_size = this.read_cipher.tag_size();

        if let ReadState::Header = *this.read_state {
            let mut header_size = mem::size_of::<u16>();
            if this.padding.needed() {
                header_size += mem::size_of::<u16>()
            }
            if *this.readed <= *this.enc_limit {
                header_size += tag_size
            }

            ready!(self.as_mut().poll_read_exact(cx, header_size))?;
            this = self.as_mut().project();

            // process EOF correctly
            if this.read_buffer.len() < header_size {
                return Ok(()).into();
            }

            if *this.readed <= *this.enc_limit {
                // decrypt header
                if !this.read_cipher.decrypt(this.read_buffer) {
                    return Err(io::Error::new(ErrorKind::InvalidData, "decrypt data header failed")).into();
                }
            }

            let size = u16::from_be_bytes(this.read_buffer[..2].try_into().unwrap()) as usize;

            let mut padding = 0;
            if this.padding.needed() {
                padding = u16::from_be_bytes(this.read_buffer[2..4].try_into().unwrap());
                *this.read_state = ReadState::Padding { size: padding as usize, data_size: size };
            } else {
                *this.read_state = ReadState::Data { size };
            }

            if *this.readed <= *this.enc_limit {
                this.read_cipher.inc_nonce(cmp::max(padding, 1));
            }

            this.read_buffer.clear();
        }

        if let ReadState::Padding{size, data_size} = *this.read_state {
            ready!(self.as_mut().poll_read_exact(cx, size))?;
            this = self.as_mut().project();

            // process EOF correctly
            if this.read_buffer.len() < size {
                return Ok(()).into();
            }
                
            *this.read_state = ReadState::Data { size: data_size };
            this.read_buffer.clear();
        }

        if let ReadState::Data{size} = *this.read_state {
            let read_size = if *this.readed <= *this.enc_limit {
                size + tag_size
            } else {
                size
            };

            ready!(self.as_mut().poll_read_exact(cx, read_size))?;
            this = self.as_mut().project();

            // process EOF correctly
            if this.read_buffer.len() < size {
                return Ok(()).into();
            }

            if *this.readed <= *this.enc_limit {
                // decrypt data
                if !this.read_cipher.decrypt(this.read_buffer) {
                    return Err(io::Error::new(ErrorKind::InvalidData, "decrypt data failed")).into();
                }

                this.read_cipher.inc_nonce(1);

                this.read_buffer.truncate(size);
            }

            *this.readed += size;
            *this.read_state = ReadState::Ready { pos: 0 };
        }

        // return buffered data
        if let ReadState::Ready{ref mut pos} = *this.read_state {
            if *pos < this.read_buffer.len() {
                let buffered = &this.read_buffer[*pos..];

                let consumed = usize::min(buffered.len(), buf.remaining());
                buf.put_slice(&buffered[..consumed]);

                *pos += consumed;
            }

            if *pos >= this.read_buffer.len() {
                this.read_buffer.clear();
                *this.read_state = ReadState::Header;
            }
        }

        Ok(()).into()
    }
}

impl<S, R> AsyncWrite for EncryptedStream<S, R>
where
    R: CryptoRng + RngCore + Rng,
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {

        if buf.len() > MAX_PACKET_SIZE {
            buf = &buf[..MAX_PACKET_SIZE];
        }

        // flush buffer
        ready!(self.as_mut().poll_write_buffer(cx, false))?;

        // assemble data
        self.as_mut().assemble_data_to_buffer(buf);

        // try to flush buffer
        ready!(self.as_mut().poll_write_buffer(cx, true))?;

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        // write if buffer not empty
        ready!(self.as_mut().poll_write_buffer(cx, false))?;

        self.as_mut().project().inner.poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        // write if buffer not empty
        ready!(self.as_mut().poll_write_buffer(cx, false))?;

        self.as_mut().project().inner.poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {

    use super::{Cipher, EncryptedStream, DataPadding, MAX_PACKET_SIZE};
    use crate::cipher::CipherType;
    use crate::kdf::Kdf;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
    use rand_chacha::ChaCha20Rng;
    use rand::prelude::*;
    use bytes::{BufMut, BytesMut};
    use anyhow::Result;
    use std::{io, pin::Pin, task::{ Context, Poll }, cmp::min};
    
    #[tokio::test]
    async fn encrypted_stream() {
        let enc_limit = usize::MAX;
        let padding = DataPadding {max: 250, rate: 10};
        check_stream_with_params(padding, enc_limit).await;
    }

    #[tokio::test]
    async fn encrypted_stream_enc_limit() {
        let enc_limit = 1024;
        let padding = DataPadding {max: 250, rate: 10};
        check_stream_with_params(padding, enc_limit).await;

        let enc_limit = 500;
        let padding = DataPadding {max: 250, rate: 10};
        check_stream_with_params(padding, enc_limit).await;

        let enc_limit = 100;
        let padding = DataPadding {max: 250, rate: 10};
        check_stream_with_params(padding, enc_limit).await;
    }

    #[tokio::test]
    async fn encrypted_stream_no_padding() {
        let enc_limit = usize::MAX;
        let padding = DataPadding {max: 0, rate: 0};
        check_stream_with_params(padding, enc_limit).await;
    }

    #[tokio::test]
    async fn encrypted_stream_enc_limit_no_padding() {
        let enc_limit = 1024;
        let padding = DataPadding {max: 0, rate: 0};
        check_stream_with_params(padding, enc_limit).await;

        let enc_limit = 500;
        let padding = DataPadding {max: 0, rate: 0};
        check_stream_with_params(padding, enc_limit).await;

        let enc_limit = 100;
        let padding = DataPadding {max: 0, rate: 0};
        check_stream_with_params(padding, enc_limit).await;
    }
   
    async fn check_stream_with_params(padding: DataPadding, enc_limit: usize) {
        let fake_stream = FakeStream::new();

        let pass = "QrD15a25tK0wVXdnlECwyNBemc6yLsa4iYnf1vRBx7A";
        let salt = "QrD15a25tK0wVXdnlECwyNBemc6yLsa4iYnf1vRBx5A".as_bytes();
        let rng = ChaCha20Rng::from_entropy();
        
        let (read_cipher, write_cipher) =
            new_client_server(CipherType::Aes256Gcm, Kdf::Blake3, pass, &salt).unwrap();

        let mut stream =
            EncryptedStream::from_stream(fake_stream, read_cipher, write_cipher, padding, enc_limit, rng);

        let u8val = 55;
        stream.write_u8(u8val).await.unwrap();
        assert_eq!(stream.read_u8().await.unwrap(), u8val);

        let u128val = 155;
        stream.write_u128(u128val).await.unwrap();
        assert_eq!(stream.read_u128().await.unwrap(), u128val);

        stream.write_all(pass.as_bytes()).await.unwrap();
        let mut readed_str = String::new();
        stream.read_to_string(&mut readed_str).await.unwrap();
        assert_eq!(readed_str, pass);

        // uneven read write
        let u8val = 0x0a;
        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();

        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();
        stream.write_u8(u8val).await.unwrap();

        let u128val = 155;
        stream.write_u128(u128val).await.unwrap();
        assert_eq!(stream.read_u32().await.unwrap(), 0x0a0a0a0a);
        assert_eq!(stream.read_u64().await.unwrap(), 0x0a0a0a0a0a0a0a0a);
        assert_eq!(stream.read_u128().await.unwrap(), u128val);

        // uneven read write chunks
        let mut rng = ChaCha20Rng::from_entropy();

        let data_size = 4096;
        let mut data = BytesMut::zeroed(data_size);
        rng.fill_bytes(data.as_mut());

        stream.write_all(&data[..100]).await.unwrap();
        stream.write_all(&data[100..500]).await.unwrap();
        stream.write_all(&data[500..1100]).await.unwrap();
        stream.write_all(&data[1100..2300]).await.unwrap();
        stream.write_all(&data[2300..4096]).await.unwrap();

        let mut data_readed = BytesMut::zeroed(data_size);
        stream.read_exact(data_readed.as_mut()).await.unwrap();

        assert_eq!(data, data_readed);

        // uneven read write chunks mix
        let data_size = 4096;
        let mut data = BytesMut::zeroed(data_size);
        rng.fill_bytes(data.as_mut());

        stream.write_all(&data[..100]).await.unwrap();
        stream.write_all(&data[100..500]).await.unwrap();
        stream.write_all(&data[500..1100]).await.unwrap();
        stream.write_all(&data[1100..2300]).await.unwrap();
        stream.write_all(&data[2300..4096]).await.unwrap();

        let mut data_readed = BytesMut::zeroed(data_size);
        stream.read_exact(&mut data_readed.as_mut()[..50]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[50..125]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[125..1101]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[1101..1102]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[1102..1157]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[1157..1700]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[1700..4096]).await.unwrap();

        assert_eq!(data, data_readed);

        // big chunk
        let data_size = 16384;
        let mut data = BytesMut::zeroed(data_size);
        rng.fill_bytes(data.as_mut());

        stream.write_all(&data[..1]).await.unwrap();
        stream.write_all(&data[1..5]).await.unwrap();
        stream.write_all(&data[5..5100]).await.unwrap();
        stream.write_all(&data[5100..11501]).await.unwrap();
        stream.write_all(&data[11501..16384]).await.unwrap();

        let mut data_readed = BytesMut::zeroed(data_size);
        stream.read_exact(&mut data_readed.as_mut()[..50]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[50..125]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[125..1101]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[1101..1102]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[1102..1157]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[1157..1700]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[1700..4096]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[4096..16384]).await.unwrap();

        assert_eq!(data, data_readed);

        // huge chunks
        let data_size = MAX_PACKET_SIZE * 2;
        let mut data = BytesMut::zeroed(data_size);
        rng.fill_bytes(data.as_mut());

        stream.write_all(&data[..MAX_PACKET_SIZE + 1]).await.unwrap();
        stream.write_all(&data[MAX_PACKET_SIZE + 1..MAX_PACKET_SIZE * 2]).await.unwrap();

        let mut data_readed = BytesMut::zeroed(data_size);
        stream.read_exact(&mut data_readed.as_mut()[..50]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[50..125]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[125..16384]).await.unwrap();
        stream.read_exact(&mut data_readed.as_mut()[16384..MAX_PACKET_SIZE * 2]).await.unwrap();

        assert_eq!(data, data_readed);
    }

    fn new_client_server(cipher: CipherType, kdf: Kdf, pass: &str, salt: &[u8]) -> Result<(Cipher, Cipher)> {
        let key_size = cipher.key_size();
        let nonce_size = cipher.nonce_size();

        // client stream cipher
        let mut client_key = BytesMut::zeroed(key_size);
        kdf.derive_client_key(pass.as_bytes(), salt, &mut client_key)?;
        let client_cipher = Cipher::new_with_nonce(cipher, &client_key, &salt[0..nonce_size]);

        // server stream cipher
        let server_key = client_key.clone();
        let server_cipher = Cipher::new_with_nonce(cipher, &server_key, &salt[0..nonce_size]);

        Ok((client_cipher, server_cipher))
    }

    pub struct FakeStream
    {
        buffer: BytesMut,
    }
    
    impl FakeStream
    {
        /// 
        pub fn new() -> Self {
            Self { 
                buffer: BytesMut::new(),
            }
        }
    }
    
    impl AsyncRead for FakeStream
    {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let buffer_len = self.buffer.len();

            let read_len = min(buf.remaining(), buffer_len);
            buf.put(&self.buffer[..read_len]);

            self.buffer.copy_within(read_len.., 0);

            let new_len = buffer_len - read_len;
            self.buffer.truncate(new_len);
            
            Ok(()).into()
        }
    }
    
    impl AsyncWrite for FakeStream
    {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            self.buffer.extend_from_slice(buf);
            Poll::Ready(Ok(buf.len()))
        }
    
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }
    
        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            Poll::Ready(Ok(()))
        }
    }
}