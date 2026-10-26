//! A proxy in front of a [`super::Fake`] that holds chosen requests open until
//! the test lets them go (#2159).
//!
//! The witness that a sync walks repositories side by side without a timer in
//! it: a held request is in flight until [`Held::release`] says otherwise, so
//! [`Held::most`] counts requests that really were outstanding together, on a
//! slow machine as on a fast one. Every request it does not hold, and every
//! held one once released, is forwarded to the fake as it came.
//!
//! wiremock answers from a synchronous `respond`, so it cannot hold an answer
//! open on a signal; that is why this is a socket of its own. One request per
//! connection (`Connection: close`), which is all the adapter's client needs.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, oneshot};

/// Which requests to hold: a key for a path to hold under, `None` to forward.
pub type Rule = fn(&str) -> Option<String>;

#[derive(Default)]
struct Seen {
    /// Held now, by key, with the switch that lets each go.
    held: BTreeMap<String, oneshot::Sender<()>>,
    /// The most held at the same time.
    most: usize,
    /// Every key ever held, in arrival order, beside what [`Held::walked`]
    /// last said when it arrived.
    arrived: Vec<(String, u64)>,
    /// How many units the run under test says it has finished.
    finished: u64,
    /// Keys whose client hung up while the request was held.
    abandoned: Vec<String>,
}

pub struct Held {
    url: String,
    seen: Mutex<Seen>,
    changed: Notify,
}

impl Held {
    /// Listen on a free port and forward to `upstream`, holding what `rule`
    /// names.
    pub async fn start(upstream: String, rule: Rule) -> Arc<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let held = Arc::new(Self {
            url: format!("http://{}", listener.local_addr().expect("bound")),
            seen: Mutex::new(Seen::default()),
            changed: Notify::new(),
        });
        let client = reqwest::Client::new();
        let proxy = Arc::clone(&held);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let proxy = Arc::clone(&proxy);
                let client = client.clone();
                let upstream = upstream.clone();
                tokio::spawn(async move { proxy.serve(socket, client, upstream, rule).await });
            }
        });
        held
    }

    pub fn base_url(&self) -> String {
        self.url.clone()
    }

    /// The most requests held open at the same time.
    pub fn most(&self) -> usize {
        self.seen().most
    }

    /// Every key held so far with how many units the run had finished when
    /// it arrived, in arrival order.
    pub fn arrivals(&self) -> Vec<(String, u64)> {
        self.seen().arrived.clone()
    }

    /// Record that the run under test has finished `done` units: a sink's
    /// walk reading, which each later arrival is stamped with.
    pub fn walked(&self, done: u64) {
        self.seen().finished = done;
        self.changed.notify_waiters();
    }

    /// Every key held so far, in the order the requests arrived.
    pub fn arrived(&self) -> Vec<String> {
        self.seen()
            .arrived
            .iter()
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// Keys whose request the client gave up on while it was held.
    pub fn abandoned(&self) -> Vec<String> {
        self.seen().abandoned.clone()
    }

    /// The keys held right now.
    pub fn holding(&self) -> Vec<String> {
        self.seen().held.keys().cloned().collect()
    }

    /// Let the request held under `key` through to the fake.
    pub fn release(&self, key: &str) {
        let go = self.seen().held.remove(key).expect("that key is held");
        let _ = go.send(());
        self.changed.notify_waiters();
    }

    /// Wait until `done` holds of what this proxy has seen.
    pub async fn until(&self, done: impl Fn(&Held) -> bool) {
        loop {
            let changed = self.changed.notified();
            if done(self) {
                return;
            }
            changed.await;
        }
    }

    fn seen(&self) -> std::sync::MutexGuard<'_, Seen> {
        self.seen.lock().expect("not poisoned")
    }

    async fn serve(
        &self,
        mut socket: TcpStream,
        client: reqwest::Client,
        upstream: String,
        rule: Rule,
    ) {
        let Some(head) = read_head(&mut socket).await else {
            return;
        };
        let target = head
            .lines()
            .next()
            .and_then(|line| line.split(' ').nth(1))
            .unwrap_or("/")
            .to_owned();
        let path = target.split('?').next().unwrap_or("/").to_owned();
        if let Some(key) = rule(&path) {
            let (go, wait) = oneshot::channel();
            {
                let mut seen = self.seen();
                seen.held.insert(key.clone(), go);
                let finished = seen.finished;
                seen.arrived.push((key.clone(), finished));
                seen.most = seen.most.max(seen.held.len());
            }
            self.changed.notify_waiters();
            let mut probe = [0u8; 1];
            tokio::select! {
                _ = wait => {}
                // The client sends nothing more on a GET, so a read that ends
                // is the client hanging up on a request it no longer wants.
                _ = socket.read(&mut probe) => {
                    let mut seen = self.seen();
                    seen.held.remove(&key);
                    seen.abandoned.push(key);
                    drop(seen);
                    self.changed.notify_waiters();
                    return;
                }
            }
        }
        let mut request = client.get(format!("{upstream}{target}"));
        for line in head.lines().skip(1) {
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("authorization")
            {
                request = request.header("Authorization", value.trim());
            }
        }
        let Ok(response) = request.send().await else {
            return;
        };
        let status = response.status();
        let mut out = format!(
            "HTTP/1.1 {} {}\r\n",
            status.as_u16(),
            status.canonical_reason().unwrap_or("")
        );
        for (name, value) in response.headers() {
            if matches!(
                name.as_str(),
                "content-length" | "transfer-encoding" | "connection"
            ) {
                continue;
            }
            out.push_str(&format!("{name}: {}\r\n", value.to_str().unwrap_or("")));
        }
        let body = response.bytes().await.unwrap_or_default();
        out.push_str(&format!(
            "content-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        ));
        let _ = socket.write_all(out.as_bytes()).await;
        let _ = socket.write_all(&body).await;
        let _ = socket.shutdown().await;
    }
}

/// The request line and headers, up to the blank line.
async fn read_head(socket: &mut TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = socket.read(&mut buf).await.ok()?;
        if n == 0 {
            return None;
        }
        head.extend_from_slice(&buf[..n]);
    }
    String::from_utf8(head).ok()
}
