use std::{
    io::{self, ErrorKind}, pin::Pin, task::{ Context, Poll }
};
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

const MAX_UPGRADE_RESPONSE: usize = u8::MAX as usize;

pin_project! {
    /// A stream wrapper that helps transfer the data through https proxy
    pub struct UgradeStream<S> 
    {
        #[pin]
        inner: S,

        state: UpgradeState,
        request: String,
    }
}

enum UpgradeState {
    SendRequest{pos: usize},
    WaitResponse{lf_in_row: usize, size: usize},
    Upgraded
}

impl<S> UgradeStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn from_stream(
        inner: S,
        host: &str,
        http_path: &str,
    ) -> Self {
        Self { 
            inner,
            state: UpgradeState::SendRequest{pos: 0},
            request: format!("\
                GET /{http_path} HTTP/1.1\r\n\
                Host: {host}\r\n\
                Connection: upgrade\r\n\
                Upgrade: websocket\r\n\
                \r\n\
            "),
        }
    }
}

impl<S> AsyncRead for UgradeStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut this = self.as_mut().project();

        if let UpgradeState::WaitResponse{lf_in_row, size} = this.state {

            let mut byte = [0u8];
            let mut byte_buff = ReadBuf::new(&mut byte);

            while *lf_in_row < 2 {
                match this.inner.as_mut().poll_read(cx, &mut byte_buff) {
                    Poll::Pending => {
                        return Poll::Pending
                    },
                    Poll::Ready(Ok(())) => {},
                    Poll::Ready(Err(err)) => {
                        return Poll::Ready(Err(err))
                    },                
                }

                *size += 1;
                if *size > MAX_UPGRADE_RESPONSE {
                    return Err(io::Error::new(ErrorKind::InvalidData, "uprade response too big")).into();
                }

                let filled = byte_buff.filled();
                if filled.is_empty() {
                    return Ok(()).into();
                }

                if filled[0] == b'\n' {
                    *lf_in_row += 1;
                } else if filled[0] != b'\r' {
                    *lf_in_row = 0;
                }

                byte_buff.clear();
            }
            
            *this.state = UpgradeState::Upgraded;
        }

        this.inner.poll_read(cx, buf)
    }
}

impl<S> AsyncWrite for UgradeStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let mut this = self.project();
        if let UpgradeState::SendRequest { pos } = this.state {
            while *pos < this.request.len() {
                match this.inner.as_mut().poll_write(cx, &this.request.as_bytes()[*pos..]) {
                    Poll::Pending => {
                        return Poll::Pending;
                    },
                    Poll::Ready(Ok(n)) => {
                        if n == 0 {
                            return Err(ErrorKind::UnexpectedEof.into()).into();
                        }
                        *pos += n;
                    },
                    Poll::Ready(Err(err)) => {
                        return Poll::Ready(Err(err))
                    },
                }
            }

            *this.state = UpgradeState::WaitResponse{lf_in_row:0, size: 0};
        }

        this.inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_shutdown(cx)
    }
}