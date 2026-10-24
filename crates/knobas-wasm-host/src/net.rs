//! An external adapter's one way out: `wasi:http` outgoing requests, held
//! to its allowlist, with the credential attached by the host (ADR-0028,
//! issue #1203).
//!
//! **The allowlist.** A request leaves only for a host [`Allowlist`] allows,
//! read from its resolved scheme, host and port. Any other is refused with
//! `connection-refused` before a socket opens, and the refusal is logged on
//! the host's side with the host it named. The component follows redirects
//! itself (the transport follows none), so a redirect to an undeclared host
//! is refused the same way.
//!
//! **A fixed name may not lead home** (#1244). A request the allowlist
//! [guards](Allowlist::guards) leaves through a client of its own, whose
//! resolver refuses the name when any address it resolves to is not
//! [public](crate::allow::is_public). The check is on the addresses the
//! connection is then made to, so a name that answers public once and
//! local the next time (DNS rebinding) cannot slip between a check and the
//! connect. The refusal reaches the component as `connection-refused`.
//!
//! **Through a system proxy** (#1283) the proxy resolves the name, and the
//! resolver sees only the proxy's own name, which it lets through. So
//! `send` resolves a guarded name itself first and refuses it the same way
//! when any answer is local; only then does the request go out, through the
//! proxy as before. That is a second lookup, so a name whose owner answers
//! public to knobas and local to the proxy (rebinding) still reaches the
//! proxy's side, which is the reader's loopback when the proxy runs on the
//! reader's machine. No credential goes there. A name knobas cannot resolve
//! goes on to the proxy too: a network whose only way out is the proxy may
//! answer no outside name, or none at all, so the lookup waits a fixed two
//! seconds at most (#1290) and a silent one counts as unresolved. The
//! component's own timeouts do not shorten that wait: a component that
//! could cut the lookup short could send a name that points home on to the
//! proxy. The check also runs on a direct connection, where the resolver's
//! check is still the one that holds against rebinding.
//!
//! **The credential.** It is attached only to a request whose origin
//! (scheme, host and port) is the source's address: `Authorization: Bearer
//! <secret>` for a token method (`Pat`, `ApiToken`), `Authorization: Basic`
//! with the config's `username` and the secret for `UserPassword`. Any
//! `Authorization` the component set on such a request is replaced. Every
//! response from the instance is redacted ([`crate::redact`]) before the
//! component reads it, and the component's `Accept-Encoding` is dropped from
//! those requests, so the body reaches the redactor decoded. A request to
//! any other declared host carries nothing of the host's.
//!
//! **Pacing** (#1631). A request for the instance waits, before it
//! leaves, on the instance's own rate limit: `knobas-http`'s
//! [`InstanceLimiter`] for the instance's origin, asked for at the http
//! crate's defaults (five a second, a burst of ten). The quota is the
//! instance's, not the adapter's: every built-in client for the same origin
//! waits on the same one, and where they ask for different quotas the
//! stricter of those still held governs. So a naive adapter that pages as
//! fast as it can sends no more than a built-in one would, and cannot lock
//! the reader's account. The wait is bounded by the call's deadline and
//! nothing shorter. A `Retry-After` is the component's to read (ruling A9 of
//! #1618). A request for another declared host is not paced: it carries
//! nothing of the reader's, and the instance's quota is not its to spend.
//!
//! **Bounds** (#1204). The call's deadline holds every request, since it
//! stops the whole call. Within it, the component's `request-options` are
//! honoured: its connect and first-byte timeouts together bound the wait
//! for the response's head, and its between-bytes timeout each wait for the
//! next frame of the body. An outgoing body is buffered whole before it
//! leaves, so it is refused (`HTTP-request-body-size`) past the adapter's
//! memory cap. A response body streams through to the component and is
//! never buffered here.
//!
//! **The transport** is the workspace's reqwest, rustls with the platform
//! verifier and the system proxy, as the built-in adapters' is. It follows
//! no redirect and keeps no cookie. A transport error reaches the component
//! as a bare `wasi:http` error code, with no text that could quote a request.

use std::collections::HashSet;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context, Poll};
use std::time::Duration;

use base64::Engine as _;
use bytes::Bytes;
use http_body_util::BodyExt;
use knobas_http::{HttpConfig, InstanceLimiter};
use knobas_source::instance::SourceInstance;
use knobas_source::{AuthMethod, SourceError};
use wasmtime_wasi_http::{Error as HttpError, RequestOptions, WasiBody, WasiHttpHooks};

use crate::allow::{Allowlist, HostPattern, Target, declared_keys, is_public};
use crate::limits::Limits;
use crate::redact::{self, Needles, RedactedBody};

/// What a source's calls may reach and attach.
pub struct Policy {
    /// The source id, for the host's log lines.
    source_id: String,
    allowlist: Allowlist,
    /// The source's address, when it is an `http(s)` URL.
    instance: Option<Target>,
    /// The instance origin's share of the rate limit, held for as long as
    /// the source is: what a request for the instance waits on.
    limiter: Option<InstanceLimiter>,
    credential: Option<http::HeaderValue>,
    needles: Needles,
}

impl std::fmt::Debug for Policy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Policy")
            .field("source_id", &self.source_id)
            .field("allowlist", &self.allowlist)
            .field("instance", &self.instance)
            .field("limiter", &self.limiter)
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "<redacted>"),
            )
            .finish_non_exhaustive()
    }
}

impl Policy {
    /// The policy of a call that reaches nothing: the descriptor probes at
    /// load, before any source exists.
    #[must_use]
    pub fn none(source_id: &str) -> Self {
        Self {
            source_id: source_id.to_owned(),
            allowlist: Allowlist::default(),
            instance: None,
            limiter: None,
            credential: None,
            needles: Needles::default(),
        }
    }

    /// One source's policy: `patterns` resolved over its address and
    /// config (only the keys `config_schema` declares), and its credential.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] for an auth method the host cannot attach
    /// (`OAuth`), `UserPassword` without a `username` in the config, or a
    /// secret that cannot be a header.
    pub fn for_source(
        instance: &SourceInstance,
        patterns: &[HostPattern],
        config_schema: &serde_json::Value,
    ) -> Result<Self, SourceError> {
        let allowlist = Allowlist::resolve(
            patterns,
            &instance.base_url,
            &instance.config,
            &declared_keys(config_schema),
        );
        let (credential, spellings) = credential(instance)?;
        let defaults = HttpConfig::default();
        Ok(Self {
            source_id: instance.id.clone(),
            allowlist,
            instance: Target::origin_of(&instance.base_url),
            // `None` exactly when `instance` is: an address that is not an
            // http(s) URL is no request's destination.
            limiter: InstanceLimiter::for_url(
                &instance.base_url,
                defaults.requests_per_second,
                defaults.burst,
            )
            .ok(),
            credential,
            needles: Needles::new(spellings),
        })
    }
}

/// The `Authorization` value for `instance`, and every spelling of the
/// secret it carries. `None` without a method or a secret: such a source
/// sends no credential, and the server's 401 says so.
fn credential(
    instance: &SourceInstance,
) -> Result<(Option<http::HeaderValue>, Vec<String>), SourceError> {
    let (Some(method), Some(secret)) = (instance.auth, instance.secret.as_deref()) else {
        return Ok((None, Vec::new()));
    };
    let (value, mut spellings) = match method {
        AuthMethod::Pat | AuthMethod::ApiToken => {
            (format!("Bearer {secret}"), vec![secret.to_owned()])
        }
        AuthMethod::UserPassword => {
            let username = instance
                .config
                .get("username")
                .and_then(serde_json::Value::as_str)
                .filter(|u| !u.is_empty())
                .ok_or_else(|| {
                    SourceError::protocol(
                        "user + password authentication needs a username in the source \
                         configuration",
                    )
                })?;
            let pair =
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{secret}"));
            (format!("Basic {pair}"), vec![secret.to_owned(), pair])
        }
        AuthMethod::OAuth => {
            return Err(SourceError::protocol(
                "OAuth is not supported for an external adapter; use a token or user + password",
            ));
        }
    };
    // The spellings an echo is likeliest to use: form-encoded (both cases of
    // its escapes) and JSON-escaped.
    let encoded: String = url::form_urlencoded::byte_serialize(secret.as_bytes()).collect();
    spellings.push(encoded.to_ascii_lowercase());
    spellings.push(encoded);
    let json = serde_json::to_string(secret).expect("a string serializes");
    spellings.push(json[1..json.len() - 1].to_owned());
    let mut header = http::HeaderValue::from_str(&value)
        .map_err(|_| SourceError::protocol("the secret cannot be sent in a header"))?;
    header.set_sensitive(true);
    Ok((Some(header), spellings))
}

/// The `wasi:http` hooks of one call's store.
pub struct Hooks {
    policy: Arc<Policy>,
    limits: Limits,
}

impl Hooks {
    pub fn new(policy: Arc<Policy>, limits: Limits) -> Self {
        Self { policy, limits }
    }
}

/// Where a request may go, decided before it leaves.
enum Verdict {
    Refused(String),
    Allowed {
        /// The request is for the source's own address.
        for_instance: bool,
        /// The request may not reach a local address
        /// ([`Allowlist::guards`]).
        guarded: bool,
    },
}

impl Policy {
    fn judge(&self, uri: &http::Uri) -> Verdict {
        let scheme = uri.scheme_str().unwrap_or_default();
        let authority = uri.authority().map_or("", http::uri::Authority::as_str);
        match Target::parse(scheme, authority) {
            None => Verdict::Refused(format!("{scheme}://{authority} is not an http(s) host")),
            Some(target) if !self.allowlist.allows(&target) => Verdict::Refused(format!(
                "{scheme}://{authority} is not a host the adapter's manifest declares"
            )),
            Some(target) => Verdict::Allowed {
                for_instance: self.instance.as_ref() == Some(&target),
                guarded: self.allowlist.guards(&target),
            },
        }
    }

    /// Put the credential on a request for the instance, in place of any
    /// the component set.
    fn attach(&self, headers: &mut http::HeaderMap) {
        headers.remove(http::header::AUTHORIZATION);
        headers.remove(http::header::ACCEPT_ENCODING);
        if let Some(credential) = &self.credential {
            headers.insert(http::header::AUTHORIZATION, credential.clone());
        }
    }
}

type SendResult = Result<
    (
        http::Response<WasiBody>,
        Box<dyn Future<Output = Result<(), HttpError>> + Send>,
    ),
    HttpError,
>;

impl WasiHttpHooks for Hooks {
    // The transport sets `Host` from the URL it was given.
    fn set_host_header(&mut self) -> bool {
        false
    }

    fn send_request(
        &mut self,
        request: http::Request<WasiBody>,
        options: Option<RequestOptions>,
        _fut: Box<dyn Future<Output = Result<(), HttpError>> + Send>,
    ) -> Box<dyn Future<Output = SendResult> + Send> {
        let policy = Arc::clone(&self.policy);
        let bounds = Bounds::new(options.as_ref(), self.limits);
        Box::new(async move { send(&policy, bounds, request).await })
    }
}

/// What one request is held to, besides the call's deadline.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    /// The longest wait for the response's head: the component's connect
    /// and first-byte timeouts together. `None` waits out the deadline.
    head: Option<Duration>,
    /// The longest wait for each next frame of the response body.
    between: Option<Duration>,
    /// The most an outgoing body may hold.
    body_bytes: usize,
    /// The longest wait on the instance's limiter: the call's whole
    /// deadline, which ends the call first, having started earlier.
    pacing_wait: Duration,
}

impl Bounds {
    fn new(options: Option<&RequestOptions>, limits: Limits) -> Self {
        let connect = options.and_then(|o| o.connect_timeout);
        let first_byte = options.and_then(|o| o.first_byte_timeout);
        let head = match (connect, first_byte) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or_default() + b.unwrap_or_default()),
        };
        Self {
            head,
            between: options.and_then(|o| o.between_bytes_timeout),
            body_bytes: usize::try_from(limits.memory_bytes()).unwrap_or(usize::MAX),
            pacing_wait: limits.deadline,
        }
    }
}

async fn send(policy: &Policy, bounds: Bounds, request: http::Request<WasiBody>) -> SendResult {
    let (for_instance, guarded) = match policy.judge(request.uri()) {
        Verdict::Refused(why) => {
            tracing::warn!(source = %policy.source_id, "an external adapter's request is refused: {why}");
            return Err(HttpError::ConnectionRefused);
        }
        Verdict::Allowed {
            for_instance,
            guarded,
        } => (for_instance, guarded),
    };
    if guarded && let Some(name) = request.uri().host() {
        register_guarded(name);
        // A system proxy resolves the name itself, and the resolver below
        // never sees it: judge it here first.
        // A lookup that never answers is a failed one: the request goes on to the proxy.
        if let Ok(Ok(addrs)) = tokio::time::timeout(LOOKUP_WAIT, lookup(name)).await
            && refuse_unless_public(name, &addrs)
        {
            return Err(HttpError::ConnectionRefused);
        }
    }
    let (mut parts, body) = request.into_parts();
    if for_instance {
        policy.attach(&mut parts.headers);
    }
    let body: Bytes = http_body_util::Limited::new(body, bounds.body_bytes)
        .collect()
        .await
        .map_err(|error| match error.downcast::<HttpError>() {
            Ok(error) => *error,
            Err(_) => HttpError::HttpRequestBodySize(None),
        })?
        .to_bytes();
    let request = reqwest::Request::try_from(http::Request::from_parts(parts, body))
        .map_err(|_| HttpError::HttpRequestUriInvalid)?;
    // Last before the wire, so the quota is spent on a request that leaves.
    if for_instance && let Some(limiter) = &policy.limiter {
        limiter
            .until_ready_within(bounds.pacing_wait)
            .await
            .map_err(|quota| {
                tracing::warn!(source = %policy.source_id, "{quota}");
                HttpError::ConnectionLimitReached
            })?;
    }
    let sent = client(guarded)?.execute(request);
    let sent = match bounds.head {
        None => sent.await,
        Some(wait) => tokio::time::timeout(wait, sent)
            .await
            .map_err(|_| HttpError::ConnectionReadTimeout)?,
    };
    let response = sent.map_err(|error| {
        tracing::debug!(source = %policy.source_id, %error, "an external adapter's request failed");
        transport_error(&error)
    })?;
    let mut response: http::Response<reqwest::Body> = response.into();
    let body = std::mem::replace(response.body_mut(), reqwest::Body::from(Bytes::new()));
    let body = body.map_err(|_| HttpError::HttpResponseIncomplete);
    let body = BetweenBytes::new(body.boxed_unsync(), bounds.between);
    let (mut parts, ()) = response.map(|_| ()).into_parts();
    let body: WasiBody = if for_instance {
        // A body reqwest did not decode could hide the secret from the redactor.
        if parts
            .headers
            .get(http::header::CONTENT_ENCODING)
            .is_some_and(|coding| coding != "identity")
        {
            return Err(HttpError::HttpResponseContentCoding(None));
        }
        redact::headers(&mut parts.headers, &policy.needles);
        // Redaction changes the length.
        parts.headers.remove(http::header::CONTENT_LENGTH);
        RedactedBody::new(body, policy.needles.clone()).boxed_unsync()
    } else {
        body.boxed_unsync()
    };
    let done: Box<dyn Future<Output = Result<(), HttpError>> + Send> = Box::new(async { Ok(()) });
    Ok((http::Response::from_parts(parts, body), done))
}

/// A response body that fails `connection-read-timeout` when the next frame
/// is longer coming than `gap`.
///
/// The wait is timed from the first poll that finds no frame, not from the
/// last frame: time the component spends elsewhere between reads (on the
/// sink, say) is its own, and a server held back by it has not stalled.
struct BetweenBytes {
    inner: WasiBody,
    gap: Option<(Duration, Pin<Box<tokio::time::Sleep>>)>,
    /// Whether the timer runs: set by a poll that found no frame, cleared by
    /// one that found one.
    waiting: bool,
}

impl BetweenBytes {
    fn new(inner: WasiBody, gap: Option<Duration>) -> Self {
        Self {
            inner,
            gap: gap.map(|gap| (gap, Box::pin(tokio::time::sleep(gap)))),
            waiting: false,
        }
    }
}

impl http_body::Body for BetweenBytes {
    type Data = Bytes;
    type Error = HttpError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Bytes>, HttpError>>> {
        let this = &mut *self;
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(frame) => {
                this.waiting = false;
                Poll::Ready(frame)
            }
            Poll::Pending => {
                let Some((gap, timer)) = this.gap.as_mut() else {
                    return Poll::Pending;
                };
                if !this.waiting {
                    this.waiting = true;
                    timer.as_mut().reset(tokio::time::Instant::now() + *gap);
                }
                let stalled = timer.as_mut().poll(cx).is_ready();
                if stalled {
                    Poll::Ready(Some(Err(HttpError::ConnectionReadTimeout)))
                } else {
                    Poll::Pending
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }
}

fn transport_error(error: &reqwest::Error) -> HttpError {
    if error.is_timeout() {
        HttpError::ConnectionTimeout
    } else if error.is_connect() {
        HttpError::ConnectionRefused
    } else {
        HttpError::HttpProtocolError
    }
}

/// The process's two clients for external adapters: one for guarded
/// requests, one for the rest. Two, so a connection the second pooled to a
/// name the reader typed never carries a guarded request.
fn client(guarded: bool) -> Result<&'static reqwest::Client, HttpError> {
    static PLAIN: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    static GUARDED: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    let cell = if guarded { &GUARDED } else { &PLAIN };
    cell.get_or_init(|| {
        let builder = reqwest::Client::builder()
            // Explicit, for `knobas-http`'s reason: the platform verifier.
            .use_rustls_tls()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(30))
            .dns_resolver(Resolver { guarded });
        // In place of the system proxy, which a test must not depend on.
        #[cfg(feature = "test-util")]
        let builder = builder.proxy(reqwest::Proxy::custom(|url| {
            locked(fake_proxies())
                .get(&url.host_str()?.to_ascii_lowercase())
                .cloned()
        }));
        builder
            .build()
            .inspect_err(|error| tracing::error!(%error, "the external adapters' http client"))
            .ok()
    })
    .as_ref()
    .ok_or(HttpError::ConfigurationError)
}

/// The names a guarded request went to. The guarded client's resolver
/// refuses only these when they resolve home: the other names it is asked
/// for are the system proxy's, which may well be on the reader's network.
fn guarded_names() -> &'static Mutex<HashSet<String>> {
    static NAMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    NAMES.get_or_init(Mutex::default)
}

/// Add `name` to the [guarded names](guarded_names).
fn register_guarded(name: &str) {
    locked(guarded_names()).insert(name.to_ascii_lowercase());
}

fn is_guarded(name: &str) -> bool {
    locked(guarded_names()).contains(&name.to_ascii_lowercase())
}

/// A lock a panic elsewhere left poisoned still holds whole values.
fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The clients' name lookup: the system's, and on the guarded client a
/// refusal of a [guarded](register_guarded) name any of whose addresses is not public. An
/// address in the URL never comes here; the connector reads it as it is.
struct Resolver {
    guarded: bool,
}

impl reqwest::dns::Resolve for Resolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let guarded = self.guarded;
        let name = name.as_str().to_owned();
        Box::pin(async move {
            let addrs = lookup(&name).await?;
            if guarded && is_guarded(&name) && refuse_unless_public(&name, &addrs) {
                return Err(format!("{name} resolves to an address that is not public").into());
            }
            Ok(Box::new(addrs.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Whether a guarded `name` is refused for resolving to `addrs`: any of
/// them not [public](is_public). A refusal is logged with the address.
fn refuse_unless_public(name: &str, addrs: &[SocketAddr]) -> bool {
    let Some(refused) = addrs.iter().find(|addr| !is_public(addr.ip())) else {
        return false;
    };
    tracing::warn!(
        "an external adapter's request is refused: {name}, a host its manifest fixes, \
         resolves to {}, which is not a public address",
        refused.ip()
    );
    true
}

/// The longest the lookup before hand-off waits. Fixed, not the component's
/// head bound: a component that set a short timeout would otherwise time
/// the lookup out and send a name that points home on to the proxy.
const LOOKUP_WAIT: Duration = Duration::from_secs(2);

async fn lookup(name: &str) -> std::io::Result<Vec<SocketAddr>> {
    #[cfg(feature = "test-util")]
    {
        let delay = locked(slow_names())
            .get(&name.to_ascii_lowercase())
            .copied();
        match delay {
            Some(None) => return std::future::pending().await,
            Some(Some(delay)) => tokio::time::sleep(delay).await,
            None => {}
        }
    }
    #[cfg(feature = "test-util")]
    if let Some(ips) = locked(fake_answers()).get(&name.to_ascii_lowercase()) {
        return Ok(ips.iter().map(|ip| SocketAddr::new(*ip, 0)).collect());
    }
    // Port 0: the connector sets the request's.
    Ok(tokio::net::lookup_host((name, 0)).await?.collect())
}

#[cfg(feature = "test-util")]
type FakeAnswers = Mutex<std::collections::HashMap<String, Vec<std::net::IpAddr>>>;

#[cfg(feature = "test-util")]
fn fake_answers() -> &'static FakeAnswers {
    static ANSWERS: OnceLock<FakeAnswers> = OnceLock::new();
    ANSWERS.get_or_init(Mutex::default)
}

/// Answer every later lookup of `name` by the external adapters' clients
/// with `ips`, in order, in place of the system's. For tests: a name the
/// test points at its own server.
#[cfg(feature = "test-util")]
pub fn fake_dns(name: &str, ips: &[std::net::IpAddr]) {
    locked(fake_answers()).insert(name.to_ascii_lowercase(), ips.to_vec());
}

/// Names whose lookup waits before it answers; `None` never answers.
#[cfg(feature = "test-util")]
type SlowNames = Mutex<std::collections::HashMap<String, Option<Duration>>>;

#[cfg(feature = "test-util")]
fn slow_names() -> &'static SlowNames {
    static NAMES: OnceLock<SlowNames> = OnceLock::new();
    NAMES.get_or_init(Mutex::default)
}

/// Never answer a later lookup of `name` by the external adapters' clients,
/// as a network whose DNS drops outside queries. For tests.
#[cfg(feature = "test-util")]
pub fn fake_dns_hangs(name: &str) {
    locked(slow_names()).insert(name.to_ascii_lowercase(), None);
}

/// Answer a later lookup of `name` by the external adapters' clients only
/// after `delay`, as a slow resolver. For tests.
#[cfg(feature = "test-util")]
pub fn fake_dns_slow(name: &str, delay: Duration) {
    locked(slow_names()).insert(name.to_ascii_lowercase(), Some(delay));
}

#[cfg(feature = "test-util")]
type FakeProxies = Mutex<std::collections::HashMap<String, String>>;

#[cfg(feature = "test-util")]
fn fake_proxies() -> &'static FakeProxies {
    static PROXIES: OnceLock<FakeProxies> = OnceLock::new();
    PROXIES.get_or_init(Mutex::default)
}

/// Send every later request of the external adapters' clients for `name`
/// through the proxy at `proxy` (`http://127.0.0.1:<port>`), as a system
/// proxy would. For tests: in a build with `test-util`, the clients use
/// these in place of the system's proxy, and send any other name direct.
#[cfg(feature = "test-util")]
pub fn fake_proxy(name: &str, proxy: &str) {
    locked(fake_proxies()).insert(name.to_ascii_lowercase(), proxy.to_owned());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body whose frames arrive when the test sends them.
    struct Gated(tokio::sync::mpsc::Receiver<Bytes>);

    impl http_body::Body for Gated {
        type Data = Bytes;
        type Error = HttpError;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Result<http_body::Frame<Bytes>, HttpError>>> {
            self.0
                .poll_recv(cx)
                .map(|bytes| bytes.map(|b| Ok(http_body::Frame::data(b))))
        }
    }

    const GAP: Duration = Duration::from_millis(100);

    fn gated() -> (tokio::sync::mpsc::Sender<Bytes>, BetweenBytes) {
        let (frames, rx) = tokio::sync::mpsc::channel(4);
        (
            frames,
            BetweenBytes::new(Gated(rx).boxed_unsync(), Some(GAP)),
        )
    }

    #[tokio::test]
    async fn a_body_that_stalls_past_its_gap_fails_read_timeout() {
        let (_frames, mut body) = gated();
        let next = body.frame().await;
        assert!(
            matches!(next, Some(Err(HttpError::ConnectionReadTimeout))),
            "{next:?}"
        );
    }

    /// The component's own time between reads is not the server's stall: a
    /// frame that comes inside the gap of the read that waits for it passes,
    /// however long ago the last read was.
    ///
    /// On tokio's paused clock (#1935), where the gap and the frame's delay
    /// run alike and nothing is on a wire: the frame comes a millisecond
    /// inside the gap, which a gap counted from the last read had long run
    /// out.
    #[tokio::test(start_paused = true)]
    async fn a_reader_that_pauses_longer_than_the_gap_is_not_timed_out() {
        let (frames, mut body) = gated();
        frames.send(Bytes::from_static(b"one")).await.unwrap();
        assert!(matches!(body.frame().await, Some(Ok(_))));
        tokio::time::sleep(GAP * 3).await;
        let later = tokio::spawn(async move {
            tokio::time::sleep(GAP - Duration::from_millis(1)).await;
            frames.send(Bytes::from_static(b"two")).await.unwrap();
        });
        let next = body.frame().await;
        assert!(matches!(next, Some(Ok(_))), "{next:?}");
        later.await.unwrap();
    }
}
