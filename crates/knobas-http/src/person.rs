//! A person's request: the API explorer's way out (ux31, ADR-0057).
//!
//! A second, small client beside the adapters' [`crate::HttpClient`], so
//! reqwest stays named in one crate. It shares the TLS and proxy setup and
//! nothing else:
//!
//! * **One attempt.** No retry for any method, no limiter, no fault mapping:
//!   a request a person composed goes out once, and what happened is told as
//!   a [`Failure`] stage the page draws, never as an adapter's error.
//! * **No redirect is followed here.** The caller applies the explorer's rule
//!   hop by hop (same origin, `GET` and `HEAD`, five at most).
//! * **Limits:** a connect timeout, a total timeout for the whole answer, and
//!   a cap on the body: past it the body is cut and [`Answer::cut`] says so.
//! * **Certificates** are the platform verifier's, plus the roots of a CA
//!   file where the caller gives one (`tls_certs_merge`: the platform's stay).
//!   There is no switch that skips verification.
//! * **Proxies** are the system's and `HTTPS_PROXY`, `HTTP_PROXY`,
//!   `NO_PROXY`; a client built with `direct` uses none (this computer).

use std::time::{Duration, Instant};

pub use bytes::Bytes;
use bytes::BytesMut;
use reqwest::header::{HeaderName, HeaderValue};

/// How a client is built: its connect timeout, any roots to trust beside the
/// platform's, and whether it goes direct (no proxy).
#[derive(Clone, Debug)]
pub struct ClientOptions {
    pub connect_timeout: Duration,
    /// A PEM bundle of roots to trust beside the platform's, for one server.
    pub extra_roots_pem: Option<Vec<u8>>,
    /// No proxy at all: a request to this computer never goes through one.
    pub direct: bool,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            extra_roots_pem: None,
            direct: false,
        }
    }
}

/// The client a person's requests go out on.
#[derive(Clone, Debug)]
pub struct Client {
    inner: reqwest::Client,
}

/// One request as it goes out. Headers a person set win over the client's
/// own but `Host` and `Content-Length`, which the client refuses here.
#[derive(Clone, Debug)]
pub struct Request {
    pub method: reqwest::Method,
    pub url: reqwest::Url,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// What a server answered.
#[derive(Clone, Debug)]
pub struct Answer {
    pub status: u16,
    pub reason: String,
    pub version: String,
    /// Every header, in the server's order.
    pub headers: Vec<(String, String)>,
    /// The body as decoded, up to the cap.
    pub body: Bytes,
    /// The size the server said it sent (`Content-Length`), else the decoded size.
    pub wire_bytes: u64,
    /// The body was longer than the cap and was cut there.
    pub cut: bool,
    pub to_headers_ms: u64,
    pub total_ms: u64,
}

/// Where a request failed, and the words the page draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Resolve(String),
    Connect(String),
    Tls(String),
    Send(String),
    Read(String),
    Timeout,
    Stopped,
}

impl Failure {
    /// The stage's name, as the page reads it.
    #[must_use]
    pub fn stage(&self) -> &'static str {
        match self {
            Self::Resolve(_) => "resolve",
            Self::Connect(_) => "connect",
            Self::Tls(_) => "tls",
            Self::Send(_) => "send",
            Self::Read(_) => "read",
            Self::Timeout => "timeout",
            Self::Stopped => "stopped",
        }
    }
}

impl Client {
    /// A client for a person's requests.
    ///
    /// # Errors
    /// The CA file does not hold a certificate, or reqwest cannot build the
    /// client.
    pub fn new(options: &ClientOptions) -> Result<Self, String> {
        let agent = format!("knobas/{}", env!("CARGO_PKG_VERSION"));
        let mut builder = reqwest::Client::builder()
            // As the adapters' client: rustls with the platform verifier, never
            // the native-tls another crate's feature would make the default.
            .use_rustls_tls()
            .connect_timeout(options.connect_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(agent)
            // Cookies are not kept between sends; none is ever stored.
            .pool_max_idle_per_host(2);
        if options.direct {
            builder = builder.no_proxy();
        }
        if let Some(pem) = &options.extra_roots_pem {
            let roots = reqwest::Certificate::from_pem_bundle(pem).map_err(|error| {
                format!("The CA file holds no certificate knobas can read: {error}")
            })?;
            if roots.is_empty() {
                return Err("The CA file holds no certificate.".to_owned());
            }
            builder = builder.tls_certs_merge(roots);
        }
        let inner = builder
            .build()
            .map_err(|error| format!("building the client: {error}"))?;
        Ok(Self { inner })
    }

    /// Sends `request` once and reads its answer, up to `max_body` bytes,
    /// within `total`; `stop` ends it at once.
    ///
    /// # Errors
    /// Where it failed, as a [`Failure`].
    pub async fn send(
        &self,
        request: Request,
        total: Duration,
        max_body: usize,
        stop: impl std::future::Future<Output = ()>,
    ) -> Result<Answer, Failure> {
        let started = Instant::now();
        let work = self.exchange(request, max_body, started);
        tokio::select! {
            answer = tokio::time::timeout(total, work) => answer.unwrap_or(Err(Failure::Timeout)),
            () = stop => Err(Failure::Stopped),
        }
    }

    async fn exchange(
        &self,
        request: Request,
        max_body: usize,
        started: Instant,
    ) -> Result<Answer, Failure> {
        let mut builder = self.inner.request(request.method, request.url);
        for (name, value) in &request.headers {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                Failure::Send(format!("The header name {name} is not one HTTP takes."))
            })?;
            if name == reqwest::header::HOST || name == reqwest::header::CONTENT_LENGTH {
                return Err(Failure::Send(format!("knobas sets {name} itself.")));
            }
            let value = HeaderValue::from_str(value).map_err(|_| {
                Failure::Send(format!("The value of {name} is not one HTTP takes."))
            })?;
            // A person's header wins over the client's own (`User-Agent`, `Accept-Encoding`).
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let mut response = builder.send().await.map_err(classify)?;
        let to_headers_ms = millis(started);
        let status = response.status();
        let version = format!("{:?}", response.version());
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    String::from_utf8_lossy(value.as_bytes()).into_owned(),
                )
            })
            .collect();
        let said = response.content_length();
        let mut body = BytesMut::new();
        let mut cut = false;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| Failure::Read(words_of(&error)))?
        {
            let room = max_body.saturating_sub(body.len());
            if chunk.len() > room {
                body.extend_from_slice(&chunk[..room]);
                cut = true;
                break;
            }
            body.extend_from_slice(&chunk);
        }
        let body = body.freeze();
        Ok(Answer {
            status: status.as_u16(),
            reason: status.canonical_reason().unwrap_or("").to_owned(),
            version,
            headers,
            wire_bytes: said.unwrap_or(body.len() as u64),
            body,
            cut,
            to_headers_ms,
            total_ms: millis(started),
        })
    }
}

fn millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// The words of an error and every error under it, joined.
fn words_of(error: &(dyn std::error::Error + 'static)) -> String {
    let mut words = error.to_string();
    let mut at = error.source();
    while let Some(next) = at {
        let more = next.to_string();
        if !words.contains(&more) {
            words.push_str(": ");
            words.push_str(&more);
        }
        at = next.source();
    }
    words
}

/// Which stage a transport error stopped at, by what reqwest and the errors
/// under it say.
fn classify(error: reqwest::Error) -> Failure {
    let words = words_of(&error);
    let lower = words.to_lowercase();
    if error.is_timeout() {
        return Failure::Timeout;
    }
    if lower.contains("certificate")
        || lower.contains("tls")
        || lower.contains("handshake")
        || lower.contains("unknownissuer")
    {
        return Failure::Tls(words);
    }
    if lower.contains("dns")
        || lower.contains("lookup")
        || lower.contains("resolve")
        || lower.contains("name or service not known")
        || lower.contains("nodename nor servname")
    {
        return Failure::Resolve(words);
    }
    if error.is_connect() {
        return Failure::Connect(words);
    }
    if error.is_request() || error.is_body() || error.is_builder() {
        return Failure::Send(words);
    }
    Failure::Read(words)
}
