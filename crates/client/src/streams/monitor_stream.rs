use std::{
    io,
    sync::{Arc, atomic::Ordering},
    pin::Pin,
    task::{ Context, Poll },
};
use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use crate::protocol::ServerState;

pin_project! {
    /// A stream wrapper that add rnd padding  and encrypt data
    pub struct MonitorStream<S> 
    {
        #[pin]
        inner: S,

        success: bool,
        state: Arc<ServerState>,
    }
}

impl<S> MonitorStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn from_stream(
        inner: S,
        state: Arc<ServerState>,
    ) -> Self {
        Self { 
            inner,
            state,
            success: false,
        }
    }

    pub fn is_success(&self) -> bool {
        self.success
    }
}

impl<S> AsyncRead for MonitorStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.project();
        match this.inner.poll_read(cx, buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(())) => {
                if !*this.success {
                    this.state.succes_count.fetch_add(1, Ordering::Relaxed);
                    *this.success = true;
                }

                this.state.rx_total.fetch_add(buf.filled().len() as u64, Ordering::Relaxed);

                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
        }
    }
}

impl<S> AsyncWrite for MonitorStream<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        let this = self.project();
        match this.inner.poll_write(cx, buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Ok(n)) => {
                this.state.tx_total.fetch_add(n as u64, Ordering::Relaxed);
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_shutdown(cx)
    }
}
