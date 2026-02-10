use std::{
    io,
    pin::Pin,
    task::{Context, Poll, Waker},
    sync::{Arc, atomic::{AtomicU64, Ordering}},
};
use chrono::Utc;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use parking_lot::Mutex;

const REQ: &str = "GET / HTTP/1.1\r\n\r\n";

pub struct TtfbStream
{
    start: i64,
    state: TtfbState,
    ttfb: Arc<AtomicU64>,
    waker: Option<Arc<Mutex<Waker>>>,
}

#[derive(PartialEq)]
enum TtfbState {
    Read{pos: usize},
    WaitResponse,
    Done
}

impl TtfbStream
{
    pub fn new(ttfb: Arc<AtomicU64>) -> Self {
        Self {
            start: Utc::now().timestamp_millis(),
            state: TtfbState::Read { pos: 0 },
            ttfb,
            waker: None,
        }
    }
}

impl AsyncRead for TtfbStream
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match &mut this.state {
            TtfbState::Read { pos } => {
                let consumed = usize::min(REQ.len() - *pos, buf.remaining());
                buf.put_slice(&REQ.as_bytes()[*pos..consumed]);
                if *pos + consumed < REQ.len() {
                    *pos += consumed;
                } else {
                    this.state = TtfbState::WaitResponse;
                }
        
                Poll::Ready(Ok(()))
            },
            TtfbState::WaitResponse => {
                if let Some(waker) = &this.waker {
                    let mut waker = waker.lock();
                    if !waker.will_wake(cx.waker()) {
                        *waker = cx.waker().clone();
                    }
                } else {
                    let waker = Arc::new(Mutex::new(cx.waker().clone()));
                    this.waker = Some(waker.clone());
                }

                Poll::Pending
            },
            TtfbState::Done => Poll::Ready(Ok(())),
        }
    }
}

impl AsyncWrite for TtfbStream
{
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        if self.state == TtfbState::WaitResponse {
            let ttfb = (Utc::now().timestamp_millis() - self.start).try_into().unwrap_or_default();
            let this = self.get_mut();
            this.ttfb.store(ttfb, Ordering::Relaxed);
            this.state = TtfbState::Done;

            if let Some(waker) = &this.waker {
                let waker = waker.lock();
                waker.wake_by_ref();
            }
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}
