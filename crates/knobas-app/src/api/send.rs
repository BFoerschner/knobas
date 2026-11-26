//! Send: one request a person composed in the API explorer, from the app
//! process, once (ux31, sections 17 and 18, ADR-0057).
//!
//! # What is refused before anything is sent (an IPC error)
//!
//! - A method that is not one of OpenAPI's eight, and `TRACE` and `CONNECT`.
//! - An address that is not `http` or `https`, or has no host.
//! - **A request that changes something** (any method but `GET`, `HEAD` and
//!   `OPTIONS`) to a host that is not this computer, without the page's yes
//!   for that host ([`ApiSend::confirmed_host`]). The page asks in place; this
//!   is the second check, because a guard only in the page is one bug away
//!   from not being there.
//! - **A secret for another origin.** A held secret ([`super::held`]) is
//!   attached only to a request whose scheme, host and port are its own,
//!   whatever the spec or a redirect says.
//!
//! # What a send does
//!
//! One attempt, 10 s to connect, 60 s in all unless the page asks for 5 s to
//! 10 min, the first 64 MiB of the answer kept. A `GET` or `HEAD` follows up
//! to [`MAX_HOPS`] redirects that stay on its origin, and the answer lists the
//! hops; any other redirect is the answer itself, with the address it names in
//! [`ApiAnswer::Answered::follow`], which the page offers as *Follow* (a plain
//! `GET`, no secret). A request to this computer goes direct, never through a
//! proxy. A failure is an answer the page draws, never an IPC error.
//!
//! One tracing span a send names the method, the origin and the status; never
//! the path, the query, a header or a body.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::Duration;

use knobas_http::person::{self, Bytes, Client, ClientOptions, Failure};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;

use super::held::held;
use super::kept::{Sent, kept};
use crate::{IpcError, IpcErrorCode};

/// The methods knobas sends: OpenAPI's eight but `TRACE`.
pub const METHODS: [&str; 7] = ["GET", "PUT", "POST", "DELETE", "OPTIONS", "HEAD", "PATCH"];
/// How many same-origin redirects a `GET` or `HEAD` follows.
pub const MAX_HOPS: usize = 5;
/// The most of an answer that is read and kept.
pub const MAX_BODY: usize = 64 * 1024 * 1024;
/// How long a send may take in all, unless the page asks otherwise, and its bounds.
pub const DEFAULT_TIMEOUT_S: u32 = 60;
pub const MIN_TIMEOUT_S: u32 = 5;
pub const MAX_TIMEOUT_S: u32 = 600;

/// Where a held secret goes in a request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "api.ts")]
pub enum AuthPlace {
    /// `Authorization: Bearer <secret>`.
    #[default]
    Bearer,
    /// `Authorization: Basic <user:secret>`.
    Basic,
    /// A header of its own name (an API key).
    Header,
    /// A query parameter of its own name.
    Query,
    /// A cookie of its own name.
    Cookie,
}

/// Which held secret a request uses: the origin it was typed for, its scheme's
/// name, and where it goes. Never the secret itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct ApiAuth {
    pub origin: String,
    pub scheme: String,
    pub place: AuthPlace,
    /// The header's, the query parameter's or the cookie's name, for an API key.
    pub name: String,
}

/// A request as the page hands it over: what `outgoing` built, with no secret in it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct ApiSend {
    /// The page's number for this send, which *Stop* and the body's read name.
    pub id: u64,
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub auth: Option<ApiAuth>,
    /// The whole send's limit in seconds, 5 to 600; 60 where left out.
    pub timeout_s: Option<u32>,
    /// The host the page's question was answered *yes* for, for a request that changes something.
    pub confirmed_host: Option<String>,
    /// A saved server whose CA file to trust, for a request to its origin.
    pub server: Option<String>,
    /// The spec's file (`repository\npath`) and the operation, for *Sent since knobas started*.
    pub file: String,
    pub operation: String,
}

/// One redirect followed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct Hop {
    pub status: u16,
    pub url: String,
}

/// What came back: an answer's head (its body is read apart, `api_answer_body`), or where it failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "api.ts")]
pub enum ApiAnswer {
    Answered {
        id: u64,
        status: u16,
        reason: String,
        version: String,
        headers: Vec<(String, String)>,
        /// The body's size as kept.
        bytes: u64,
        wire_bytes: u64,
        to_headers_ms: u64,
        total_ms: u64,
        hops: Vec<Hop>,
        /// The body was longer than 64 MiB and was cut there.
        cut: bool,
        /// A redirect not followed (another origin, or not a `GET` or `HEAD`): where it points.
        follow: Option<String>,
        content_type: String,
    },
    Failed {
        id: u64,
        /// `resolve`, `connect`, `tls`, `send`, `read`, `timeout` or `stopped`.
        stage: String,
        words: String,
    },
}

/// Whether `host` is this computer: `localhost`, a name ending `.localhost`,
/// `127.0.0.0/8` and `::1`. Read from the name, not from what it resolves to:
/// a public name that resolves to 127.0.0.1 is asked about, the safe side.
#[must_use]
pub fn is_this_computer(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if host == "localhost" || host.ends_with(".localhost") || bare == "::1" {
        return true;
    }
    bare.parse::<Ipv4Addr>()
        .is_ok_and(|ip| ip.octets()[0] == 127)
}

/// Whether `method` changes something: anything but `GET`, `HEAD` and `OPTIONS`.
#[must_use]
pub fn changes(method: &str) -> bool {
    !matches!(method, "GET" | "HEAD" | "OPTIONS")
}

/// Whether a send of `method` to `host` needs the page's yes first.
#[must_use]
pub fn needs_asking(method: &str, host: &str) -> bool {
    changes(method) && !is_this_computer(host)
}

/// The origin of `url` as the held secrets key it: scheme, host and the port where it is not the default.
#[must_use]
pub fn origin_of(url: &url::Url) -> String {
    url.origin().ascii_serialization()
}

/// What a send was refused for, by name, before anything left.
fn refuse(words: impl Into<String>) -> IpcError {
    IpcError::new(IpcErrorCode::Invalid, words.into())
}

/// Checks a send against the rules above and builds its request; the secret attached where the rules allow.
///
/// # Errors
/// A refusal, by name.
pub fn prepare(send: &ApiSend) -> Result<(person::Request, url::Url), IpcError> {
    let method = send.method.to_ascii_uppercase();
    if method == "TRACE" || method == "CONNECT" {
        return Err(refuse(format!("knobas does not send {method}.")));
    }
    if !METHODS.contains(&method.as_str()) {
        return Err(refuse(format!(
            "{} is not a method knobas sends.",
            send.method
        )));
    }
    let url = url::Url::parse(&send.url).map_err(|_| {
        refuse(format!(
            "{} is not an address knobas can send to.",
            send.url
        ))
    })?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(refuse(format!(
            "knobas sends only to http and https addresses, not {}.",
            url.scheme()
        )));
    }
    let Some(host) = url.host_str().map(str::to_owned) else {
        return Err(refuse("The address has no host."));
    };
    if needs_asking(&method, &host) && send.confirmed_host.as_deref() != Some(host.as_str()) {
        return Err(refuse(format!(
            "{method} changes something on {host}, and knobas was not told to send it."
        )));
    }
    let mut url = url;
    let mut headers: Vec<(String, String)> = Vec::new();
    for (name, value) in &send.headers {
        if name.contains(['\r', '\n']) || value.contains(['\r', '\n']) {
            return Err(refuse(format!(
                "The header {} holds a line break.",
                name.trim()
            )));
        }
        headers.push((name.clone(), value.clone()));
    }
    if let Some(auth) = &send.auth {
        let origin = origin_of(&url);
        if auth.origin != origin {
            return Err(refuse(format!(
                "This secret is kept for {}. The request goes to {}.",
                host_of(&auth.origin),
                host_of(&origin)
            )));
        }
        let secrets = held();
        let Some(secret) = secrets.get(&auth.origin, &auth.scheme) else {
            return Err(refuse(format!(
                "No secret is held for {} on {}: type it again.",
                auth.scheme,
                host_of(&auth.origin)
            )));
        };
        match auth.place {
            AuthPlace::Bearer => headers.push((
                "Authorization".into(),
                format!("Bearer {}", secret.secret()),
            )),
            AuthPlace::Basic => {
                use base64::Engine as _;
                let pair = format!("{}:{}", secret.user().unwrap_or(""), secret.secret());
                headers.push((
                    "Authorization".into(),
                    format!(
                        "Basic {}",
                        base64::engine::general_purpose::STANDARD.encode(pair)
                    ),
                ));
            }
            AuthPlace::Header => {
                let name = auth.name.trim();
                if name.is_empty() {
                    return Err(refuse("The API key header has no name."));
                }
                if name.contains(['\r', '\n']) {
                    return Err(refuse("The API key header holds a line break."));
                }
                headers.push((name.to_owned(), secret.secret().to_owned()));
            }
            AuthPlace::Query => {
                let name = auth.name.trim();
                if name.is_empty() {
                    return Err(refuse("The API key query parameter has no name."));
                }
                url.query_pairs_mut().append_pair(name, secret.secret());
            }
            AuthPlace::Cookie => headers.push((
                "Cookie".into(),
                format!("{}={}", auth.name, secret.secret()),
            )),
        }
    }
    let request = person::Request {
        method: knobas_http::Method::from_bytes(method.as_bytes())
            .map_err(|_| refuse("bad method"))?,
        url: url.clone(),
        headers,
        body: send.body.clone().map(String::into_bytes),
    };
    Ok((request, url))
}

fn host_of(origin: &str) -> String {
    url::Url::parse(origin)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| origin.to_owned())
}

/// The clients, by whether they go direct and the CA file they trust: built once each.
/// A client by whether it goes direct and the CA file it trusts.
type Clients = HashMap<(bool, Option<String>), Client>;
static CLIENTS: LazyLock<Mutex<Clients>> = LazyLock::new(Mutex::default);

fn client(direct: bool, ca_file: Option<&str>) -> Result<Client, String> {
    let key = (direct, ca_file.map(str::to_owned));
    let mut clients = CLIENTS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(c) = clients.get(&key) {
        return Ok(c.clone());
    }
    let extra_roots_pem = match ca_file {
        Some(path) => Some(
            std::fs::read(path)
                .map_err(|error| format!("The CA file {path} could not be read: {error}"))?,
        ),
        None => None,
    };
    let c = Client::new(&ClientOptions {
        extra_roots_pem,
        direct,
        ..ClientOptions::default()
    })?;
    clients.insert(key, c.clone());
    Ok(c)
}

/// The sends running, by id, so *Stop* can end one.
static RUNNING: LazyLock<Mutex<HashMap<u64, CancellationToken>>> = LazyLock::new(Mutex::default);

/// Ends send `id` at once; nothing where it already ended.
pub fn cancel(id: u64) {
    if let Some(token) = RUNNING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&id)
    {
        token.cancel();
    }
}

/// Sends `send` once, following same-origin redirects for `GET` and `HEAD`, and keeps it with its answer.
///
/// # Errors
/// A refusal before anything was sent ([`prepare`]).
pub async fn send(send: ApiSend, ca_file: Option<String>) -> Result<ApiAnswer, IpcError> {
    let (request, url) = prepare(&send)?;
    let direct = url.host_str().is_some_and(is_this_computer);
    let token = CancellationToken::new();
    RUNNING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(send.id, token.clone());
    let total = Duration::from_secs(u64::from(
        send.timeout_s
            .unwrap_or(DEFAULT_TIMEOUT_S)
            .clamp(MIN_TIMEOUT_S, MAX_TIMEOUT_S),
    ));
    let span = tracing::info_span!("api_send", method = %request.method, origin = %origin_of(&url), status = tracing::field::Empty);
    let answered = exchange(request, url, direct, ca_file.as_deref(), total, &token)
        .instrument(span.clone())
        .await;
    RUNNING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&send.id);
    let (answer, body) = match answered {
        Ok((answer, hops, follow)) => {
            span.record("status", answer.status);
            let content_type = answer
                .headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            (
                ApiAnswer::Answered {
                    id: send.id,
                    status: answer.status,
                    reason: answer.reason,
                    version: answer.version,
                    headers: answer.headers,
                    bytes: answer.body.len() as u64,
                    wire_bytes: answer.wire_bytes,
                    to_headers_ms: answer.to_headers_ms,
                    total_ms: answer.total_ms,
                    hops,
                    cut: answer.cut,
                    follow,
                    content_type,
                },
                Some(answer.body),
            )
        }
        Err(failure) => (
            ApiAnswer::Failed {
                id: send.id,
                stage: failure.stage().to_owned(),
                words: words(&failure),
            },
            None,
        ),
    };
    let at_ms = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis()),
    )
    .unwrap_or(0);
    kept().keep(Sent {
        at_ms,
        send,
        answer: answer.clone(),
        body,
    });
    Ok(answer)
}

/// The words a failure draws: what failed and what to check.
fn words(failure: &Failure) -> String {
    match failure {
        Failure::Resolve(w) => format!("The server's name could not be found: {w}"),
        Failure::Connect(w) => format!("No answer: the connection could not be made. {w}"),
        Failure::Tls(w) => format!("The certificate is not trusted: {w}"),
        Failure::Send(w) => format!("The request could not be sent: {w}"),
        Failure::Read(w) => format!("The server hung up before it had answered: {w}"),
        Failure::Timeout => {
            "No answer in time. The server may still have received the request.".to_owned()
        }
        Failure::Stopped => "Stopped. The server may still have received the request.".to_owned(),
    }
}

type Exchanged = (person::Answer, Vec<Hop>, Option<String>);

async fn exchange(
    first: person::Request,
    url: url::Url,
    direct: bool,
    ca_file: Option<&str>,
    total: Duration,
    token: &CancellationToken,
) -> Result<Exchanged, Failure> {
    let client = client(direct, ca_file).map_err(Failure::Send)?;
    let follows = matches!(first.method.as_str(), "GET" | "HEAD");
    let origin = origin_of(&url);
    let mut request = first;
    let mut hops = Vec::new();
    loop {
        let answer = client
            .send(request.clone(), total, MAX_BODY, token.cancelled())
            .await?;
        let location = if (300..400).contains(&answer.status) {
            answer
                .headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("location"))
                .and_then(|(_, v)| request.url.join(v).ok())
        } else {
            None
        };
        let Some(next) = location else {
            return Ok((answer, hops, None));
        };
        // Only a GET or HEAD, only on its own origin, only so many times; anything else is the answer, with where it points.
        if !follows || origin_of(&next) != origin || hops.len() >= MAX_HOPS {
            return Ok((answer, hops, Some(next.to_string())));
        }
        hops.push(Hop {
            status: answer.status,
            url: next.to_string(),
        });
        request = person::Request {
            url: next,
            ..request
        };
    }
}

/// The body of send `id` as kept: its bytes, or JSON indented where `pretty` and it parses (up to 8 MiB).
#[must_use]
pub fn body(id: u64, pretty: bool) -> Option<Bytes> {
    let kept = kept();
    let bytes = kept.body(id)?.clone();
    if pretty
        && bytes.len() <= 8 * 1024 * 1024
        && let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes)
        && let Ok(text) = serde_json::to_vec_pretty(&value)
    {
        return Some(Bytes::from(text));
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::{ApiAuth, ApiSend, AuthPlace, changes, is_this_computer, needs_asking, prepare};
    use crate::api::held::{Held, held};

    #[test]
    fn this_computer_is_read_from_the_name() {
        for host in [
            "localhost",
            "LOCALHOST",
            "api.localhost",
            "127.0.0.1",
            "127.8.9.10",
            "::1",
            "[::1]",
            "localhost.",
        ] {
            assert!(is_this_computer(host), "{host}");
        }
        for host in [
            "127.evil.example",
            "localhost.example.com",
            "128.0.0.1",
            "10.0.0.1",
            "api.example.com",
            "::2",
            "1127.0.0.1",
        ] {
            assert!(!is_this_computer(host), "{host}");
        }
    }

    #[test]
    fn a_change_to_another_host_is_asked_about_and_reading_never_is() {
        assert!(!changes("GET") && !changes("HEAD") && !changes("OPTIONS"));
        assert!(changes("POST") && changes("PUT") && changes("PATCH") && changes("DELETE"));
        assert!(needs_asking("POST", "api.example.com"));
        assert!(!needs_asking("POST", "localhost"));
        assert!(!needs_asking("GET", "api.example.com"));
    }

    fn send(method: &str, url: &str) -> ApiSend {
        ApiSend {
            id: 1,
            method: method.into(),
            url: url.into(),
            ..ApiSend::default()
        }
    }

    #[test]
    fn refused_before_anything_is_sent() {
        assert!(
            prepare(&send("TRACE", "http://localhost/"))
                .unwrap_err()
                .message
                .contains("does not send TRACE")
        );
        assert!(
            prepare(&send("BREW", "http://localhost/"))
                .unwrap_err()
                .message
                .contains("not a method")
        );
        assert!(
            prepare(&send("GET", "file:///etc/passwd"))
                .unwrap_err()
                .message
                .contains("only to http and https")
        );
        let post = prepare(&send("POST", "https://api.example.com/x")).unwrap_err();
        assert!(
            post.message
                .contains("POST changes something on api.example.com"),
            "{}",
            post.message
        );
        // A yes for another host is no yes for this one.
        let mut other = send("POST", "https://api.example.com/x");
        other.confirmed_host = Some("api.example.co".into());
        assert!(prepare(&other).is_err());
        other.confirmed_host = Some("api.example.com".into());
        assert!(prepare(&other).is_ok());
        assert!(prepare(&send("POST", "http://127.0.0.1:9/x")).is_ok());
        let mut broken = send("GET", "http://localhost/");
        broken.headers = vec![("X-A".into(), "b\r\nX-Evil: 1".into())];
        assert!(prepare(&broken).unwrap_err().message.contains("line break"));
    }

    #[test]
    fn a_secret_goes_only_to_its_own_origin() {
        held().hold(
            "https://teamcity.example.com",
            "bearerAuth",
            Held::new("tok-123".into(), None),
        );
        let auth = ApiAuth {
            origin: "https://teamcity.example.com".into(),
            scheme: "bearerAuth".into(),
            place: AuthPlace::Bearer,
            name: String::new(),
        };
        let mut ok = send("GET", "https://teamcity.example.com/app/rest/builds");
        ok.auth = Some(auth.clone());
        let (request, _) = prepare(&ok).unwrap();
        assert!(
            request
                .headers
                .contains(&("Authorization".into(), "Bearer tok-123".into()))
        );
        for to in [
            "https://other.example.com/",
            "http://teamcity.example.com/",
            "https://teamcity.example.com:8443/",
        ] {
            let mut away = send("GET", to);
            away.auth = Some(auth.clone());
            let refused = prepare(&away).unwrap_err().message;
            assert!(
                refused.starts_with("This secret is kept for teamcity.example.com."),
                "{to}: {refused}"
            );
        }
        // A key in the query, and one in a header, by their own names.
        held().hold(
            "https://teamcity.example.com",
            "key",
            Held::new("k 1".into(), None),
        );
        let mut query = send("GET", "https://teamcity.example.com/x?a=1");
        query.auth = Some(ApiAuth {
            scheme: "key".into(),
            place: AuthPlace::Query,
            name: "api_key".into(),
            ..auth.clone()
        });
        assert_eq!(
            prepare(&query).unwrap().0.url.as_str(),
            "https://teamcity.example.com/x?a=1&api_key=k+1"
        );
        held().hold(
            "https://teamcity.example.com",
            "basic",
            Held::new("pw".into(), Some("mara".into())),
        );
        let mut basic = send("GET", "https://teamcity.example.com/");
        basic.auth = Some(ApiAuth {
            scheme: "basic".into(),
            place: AuthPlace::Basic,
            ..auth
        });
        assert!(
            prepare(&basic)
                .unwrap()
                .0
                .headers
                .contains(&("Authorization".into(), "Basic bWFyYTpwdw==".into()))
        );
        held().forget("https://teamcity.example.com");
    }
}
