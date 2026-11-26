//! **The delaying proxy** the slow-source rigs time reads through
//! (`slow_forge_live.rs`, `slow_sources_live.rs`; PR #2479's smoothness
//! passes): it forwards bytes between knobas and a testenv product and holds
//! the first chunk of each request for the mode's delay. [`HANG`] holds it
//! forever, so the HTTP client's own timeouts decide (30 s a try, 45 s a
//! call), and [`DOWN`] closes every connection as it opens. It runs in the
//! test's process, on loopback: nothing is installed on the machine the
//! product runs on.

// Compiled into each rig, which may not use all of it.
#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Hold every request forever.
pub const HANG: u64 = u64::MAX;
/// Close every connection as it opens.
pub const DOWN: u64 = u64::MAX - 1;

pub struct Proxy {
    pub url: String,
    delay: Arc<AtomicU64>,
}

impl Proxy {
    /// Hold each request `ms` milliseconds, or [`HANG`] or [`DOWN`].
    pub fn set(&self, ms: u64) {
        self.delay.store(ms, Ordering::SeqCst);
    }
}

/// A proxy on loopback in front of `upstream` (`http://host:port`).
pub async fn start(upstream: &str) -> Proxy {
    let target = upstream
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().unwrap().port();
    let delay = Arc::new(AtomicU64::new(0));
    let shared = delay.clone();
    tokio::spawn(async move {
        loop {
            let Ok((client, _)) = listener.accept().await else {
                return;
            };
            let (target, delay) = (target.clone(), shared.clone());
            tokio::spawn(async move {
                let _ = pipe(client, &target, delay).await;
            });
        }
    });
    Proxy {
        url: format!("http://127.0.0.1:{port}"),
        delay,
    }
}

async fn pipe(client: TcpStream, target: &str, delay: Arc<AtomicU64>) -> std::io::Result<()> {
    if delay.load(Ordering::SeqCst) == DOWN {
        return Ok(());
    }
    let server = TcpStream::connect(target).await?;
    let (mut from_client, mut to_client) = client.into_split();
    let (mut from_server, mut to_server) = server.into_split();
    // The next bytes from the client start a request.
    let fresh = Arc::new(AtomicBool::new(true));
    let seen = fresh.clone();
    let up = async move {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = from_client.read(&mut buf).await?;
            if n == 0 {
                let _ = to_server.shutdown().await;
                return Ok::<_, std::io::Error>(());
            }
            if seen.swap(false, Ordering::SeqCst) {
                match delay.load(Ordering::SeqCst) {
                    HANG => std::future::pending::<()>().await,
                    0 => {}
                    ms => tokio::time::sleep(Duration::from_millis(ms)).await,
                }
            }
            to_server.write_all(&buf[..n]).await?;
        }
    };
    let down = async move {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = from_server.read(&mut buf).await?;
            if n == 0 {
                let _ = to_client.shutdown().await;
                return Ok::<_, std::io::Error>(());
            }
            fresh.store(true, Ordering::SeqCst);
            to_client.write_all(&buf[..n]).await?;
        }
    };
    let _ = tokio::join!(up, down);
    Ok(())
}
