use anyhow::{Context, Result};
use std::time::Duration;

use sntpc::{NtpContext, StdTimestampGen, get_time};
use sntpc_net_tokio::UdpSocketWrapper;
use tokio::{
    net::{UdpSocket, lookup_host},
    process::Command,
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

const UPDATE_TIME_INTERVAL_MIN: Duration = Duration::from_mins(30);
const UPDATE_TIME_WAIT: Duration = Duration::from_secs(5);
const SERVERS: &[&str] = &[
    "time.google.com:123",
    "time.cloudflare.com:123",
    "time.windows.com:123",
    "pool.ntp.org:123",
];
const MAX_OFFSET: Duration = Duration::from_secs(2);

pub struct WindowsTimeSync {
    cancel_token: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl WindowsTimeSync {
    pub fn new() -> Self {
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();

        let task = tokio::spawn(async move {
            loop {
                check_and_sync().await;
                tokio::select! {
                    _ = token_clone.cancelled() => break,
                    _ = sleep(UPDATE_TIME_INTERVAL_MIN) => {},
                }
            }
        });

        Self { cancel_token, task }
    }

    pub async fn stop(&mut self) {
        self.cancel_token.cancel();
        if timeout(UPDATE_TIME_WAIT, &mut self.task).await.is_err() {
            tracing::warn!("Time sync task did not stop within {:?}, aborting", UPDATE_TIME_WAIT);
            self.task.abort();
        }
    }
}

async fn check_and_sync() {
    for &server in SERVERS {
        if let Some(offset) = try_get_offset(server).await {
            if offset > MAX_OFFSET {
                if let Err(err) = sync_time().await {
                    tracing::error!("Failed to sync time: {:?}", err);
                }
            }
            return;
        }
    }
    tracing::error!("Failed to get time offset from all servers");
}

async fn try_get_offset(server: &str) -> Option<Duration> {
    let socket = UdpSocket::bind("0.0.0.0:0").await.ok()?;
    let socket = UdpSocketWrapper::from(socket);
    let ntp_context = NtpContext::new(StdTimestampGen::default());

    for addr in lookup_host(server).await.ok()? {
        match timeout(Duration::from_secs(2), get_time(addr, &socket, ntp_context)).await {
            Ok(Ok(resp)) => return Some(Duration::from_micros(resp.offset().unsigned_abs())),
            Ok(Err(e)) => {
                tracing::debug!("Failed to query NTP server {}: {:?}", server, e);
            }
            Err(_) => {
                tracing::debug!("Timeout querying NTP server {}", server);
            }
        }
    }

    None
}

async fn sync_time() -> Result<()> {
    let was_running = is_service_running().await?;
    if !was_running {
        start_service().await?;
    }
 
    run_command("w32tm", &["/resync", "/force"])
        .await
        .with_context(|| "w32tm /resync /force")?;
 
    if !was_running {
        stop_service().await.with_context(|| "stop time service")?;
    }
 
    Ok(())
}

async fn is_service_running() -> Result<bool> {
    let output = Command::new("sc").args(["query", "w32time"]).output().await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().any(|l| l.contains("STATE") && l.contains(" 4 ")))
}

async fn start_service() -> Result<()> {
    run_command("net", &["start", "w32time"])
        .await
        .with_context(|| "start time service")
}

async fn stop_service() -> Result<()> {
    run_command("net", &["stop", "w32time"])
        .await
        .with_context(|| "stop time service")
}

async fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    let result = Command::new(cmd).args(args).output().await?;
    if !result.status.success() {
        anyhow::bail!(
            "Command failed with code {:?} : {}",
            result.status.code(),
            String::from_utf8_lossy(if result.stderr.is_empty() {
                &result.stdout
            } else {
                &result.stderr
            })
            .trim()
        );
    }
    Ok(())
}
