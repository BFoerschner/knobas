//! The HTTP stack every knobas adapter shares.
//!
//! Read-only for M1 (interfaces §8 P8). What it guarantees, so that three
//! adapters cannot disagree about any of it:
//!
//! * **rustls with the platform's native roots** -- a corporate CA works
//!   without a bundled root store (roadmap §4).
//! * **Timeouts**: 10 s to connect, 30 s for the whole request. A sync that
//!   hangs is worse than one that fails: the scheduler retries a failure.
//!   A read too large to arrive in 30 s -- a run's download, its whole log --
//!   goes through [`HttpClient::send_streaming`] instead: the 30 s covers
//!   the headers, and the body may take as long as it takes as long as it
//!   never goes [`STALL_TIMEOUT`] without a byte (issue #1012).
//! * **Retries**: [`MAX_ATTEMPTS`] attempts in total -- the first one included
//!   -- exponential, transient statuses only, and `Retry-After` obeyed
//!   *before* the retry it applies to, up to [`classify::RETRY_AFTER_CAP`].
//!   A write that must not arrive twice opts out with [`Request::no_retry`].
//!   A `GET` or `HEAD` the server closed its connection on before answering
//!   -- a pooled keep-alive connection its idle timeout closed as the request
//!   went out -- is sent once more; a write never is
//!   ([`retry::closed_before_answering`], #1818). A write, or a request sent
//!   with [`Request::no_retry`], that the server hung up on is
//!   [`SourceError::Unconfirmed`] (#1821), and the write client lets an idle
//!   connection go after [`WRITE_POOL_IDLE`] so the race is rarer for writes.
//! * **Rate limiting**: per instance, so two Jiras do not share a budget, and
//!   **every attempt passes it**, retries included. The limiter lives in a
//!   process-wide map keyed by the base URL's origin (scheme, host, port), not
//!   in the client: every [`HttpClient`] built for one instance -- a sync
//!   run's, a live read's, a flush's -- waits on the same quota, as do the
//!   wasm host's requests for an external adapter's instance (#1631). Where
//!   configurations differ, the stricter of those a live client still holds
//!   governs, so an edited quota applies once the clients built at the old
//!   one are gone ([`InstanceLimiter`], #1630, #1728). A wait
//!   on it is bounded by [`SEND_BUDGET`]; when it runs the budget out before
//!   the first attempt, the call is `Protocol` naming the quota
//!   ([`LocalQuota`]), not `Unreachable`, since the instance was never asked
//!   (before a retry, it is what the source said the attempt before). A
//!   caller that has to tell that failure from a refusal runs the call
//!   under [`watch_local_quota`], which hands back the quota it stopped on
//!   (#1729).
//! * **One way out**: [`HttpClient::send`] (and its streamed twin
//!   [`HttpClient::send_streaming`], the same attempt loop) is the only thing
//!   that puts a request on the wire. [`HttpClient::request`] hands back a [`Request`],
//!   which has no `send` of its own -- see that type for why.
//! * **One fault mapping** ([`classify`]), because `Unauthorized` is the fault
//!   the user is asked to act on -- carrying the **status** it came from, so an
//!   adapter can tell a dead credential (401) from one object it may not read
//!   (403) without a second request (ADR-0004). An adapter whose 403 can also
//!   refuse the *request* rather than the credential says which through its
//!   [`ForbiddenReading`](classify::ForbiddenReading) (issue #722).
//! * **One place a failing body is read**: the caller's
//!   [`BodyMessage`](classify::BodyMessage), consulted inside the attempt loop
//!   both sends share, where the body still exists. A body that arrives but
//!   is not the shape asked for names its own start when the caller passes a
//!   [`BodyExcerpt`] to [`HttpClient::get_json_with`] (#877, #1313).
//! * **One credential mapping** ([`credential`]): an adapter states its
//!   [`CredentialRules`] and gets the same refusals, in the same order, as
//!   every other (issue #1310).
//! * **One page walk** ([`PageWalk`]): the loop, the cap and the rule that an
//!   empty page ends a listing and a short one does not, for page number,
//!   offset and next link (issue #1315).
//! * **`User-Agent: knobas/<version> (<adapter_kind>/<adapter_version>)`** --
//!   an admin reading their access log can tell what is calling them.
//! * **A cap on what one answer's body may hold** ([`HttpConfig::max_body`],
//!   [`MAX_BODY`] by default): checked against the declared length before the
//!   read, and counted as the body is decoded, so a gzip bomb is refused
//!   too. Every body [`HttpClient::send`] hands back is read through it, an
//!   adapter's own `response.json()` included; a streamed download is not
//!   (#1640).
//! * **A redirect is followed for a read only.** A `GET` or `HEAD` follows
//!   one as reqwest does; any other method answered with a redirect fails
//!   with `Protocol` naming the `Location`, rather than reqwest re-sending a
//!   write as a `GET` whose `200` would read as the write's success
//!   (ADR-0012, #1640). A caller that must hold every hop of a read to a
//!   rule of its own builds its client with
//!   [`HttpClient::with_redirect_check`]: a hop the check refuses is never
//!   requested (#1830). Every adapter's client is built with
//!   [`HttpClient::new`] and follows as reqwest does.
//! * **No credential in a base URL.** [`HttpClient::new`] refuses one that
//!   carries a username or a password ([`refuse_userinfo`]), and no message
//!   or `Debug` of this crate repeats one (#1640).

mod cap;
pub mod classify;
#[cfg(feature = "test-util")]
pub mod counted;
mod credential;
mod limiter;
mod redirect;
pub mod retry;
mod walk;

use std::time::Duration;

use knobas_source::SourceError;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use tokio::time::Instant;

pub use classify::{BodyMessage, ForbiddenReading, reqwest_error, response_error, status_error};
pub use credential::{CredentialRules, TokenSpelling, credential};
pub use limiter::{InstanceLimiter, LocalQuota, watch_local_quota};
pub use walk::{NextLink, Offset, PageNumber, PageWalk, Paged, Protocol, Walked};

// Every type this crate's signatures name, re-exported: an adapter depends on
// `knobas-http` and on nothing else for its transport. Otherwise all three
// would list `reqwest` themselves just to spell `Method` -- three chances to
// pick a different version of the stack this crate exists to make singular,
// and three Cargo.toml edits for a bump that P8 says routes through the
// orchestrator.
//
// **Named types only, never `self`.** `pub use reqwest::{self, ..}` would
// re-export the whole crate under `knobas_http::reqwest`, and from there
// `Client::new()` is one line away -- a client with no rate limiter, no retry
// budget, no `Retry-After` and no `SourceError` mapping, reached without an
// adapter adding a single dependency. That is the same hole [`Request`] closes
// on the builder, left open one level up, so it is closed the same way: if a
// type is not named here, it cannot be reached through this crate.
// `RequestBuilder` and `Client` are both deliberately absent.
pub use reqwest::{Method, Response, StatusCode, Url, header};
// What a [`StreamedBody`] chunk is; reqwest's own, so no copy per chunk.
pub use bytes::Bytes;

/// Total attempts per request, the first one included.
///
/// Three *in total*, not three retries on top of a first try: a source that is
/// down should cost one sync three requests, not four.
pub const MAX_ATTEMPTS: u32 = 3;

/// The longest one [`HttpClient::send`] may take, retries and waits included.
///
/// Attempts, timeouts and `Retry-After` multiply: three 30 s timeouts plus two
/// 60 s capped waits is about 210 s for a single call. That is not an abstract
/// worry, because of where the call happens -- `knobas_sync::run_once` holds
/// an advisory-locked transaction across `Source::sync`, so every second a
/// request spends is a second that transaction stays open, blocking the same
/// source's next run and pinning one of the pool's five connections. Waiting
/// minutes inside a transaction is the exact thing
/// [`classify::RETRY_AFTER_CAP`] was introduced to prevent, and the cap alone
/// does not prevent it: it bounds one wait, not their sum.
///
/// So the whole call is bounded, not just its parts. Each attempt's timeout is
/// shortened to whatever is left, which makes this a real ceiling rather than
/// a check between attempts that a single slow attempt can still overshoot.
///
/// Well under a minute on purpose: a source that cannot answer in this long is
/// a source the scheduler should back off from, not one to keep a transaction
/// open for. The retries that matter -- 429 and 503, which answer immediately
/// -- all fit inside it with room to spare.
///
/// The structural fix is for the transaction not to span the network at all;
/// that is stream F's (interfaces §10.6).
pub const SEND_BUDGET: Duration = Duration::from_secs(45);

/// How long a connection the write client pooled may sit idle before it is
/// closed rather than reused (#1821).
///
/// Below the 5 s keep-alive timeout of Node (so Uptime Kuma, whose socket.io
/// packets go out on that client) and of Apache. A server closes an idle
/// connection when its timeout fires, and a write sent on it at that moment
/// is read and never answered: `Unconfirmed`, held until the user acts when
/// a second arrival would duplicate it. A write after a pause opens a fresh
/// connection instead. Reads keep reqwest's default idle timeout: one the
/// server hung up on is sent again (#1818).
pub const WRITE_POOL_IDLE: Duration = Duration::from_secs(4);

/// How long a streamed body may go without a byte before it is given up on
/// ([`HttpClient::send_streaming`], issue #1012).
///
/// The request timeout's 30 s, because it answers the same question -- "is
/// anyone still there?" -- about one read instead of about the whole body.
pub const STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// The most bytes of one answer's body a client reads by default
/// ([`HttpConfig::max_body`], #1640).
///
/// Tens of MiB on purpose: far over the largest page any adapter asks for (a
/// hundred Jira issues with their fields run to a few MiB), and far under
/// what the desktop app can hold beside its embedded Postgres. A download that
/// is large on purpose goes through [`HttpClient::send_streaming`], which is
/// not capped.
pub const MAX_BODY: u64 = 32 * 1024 * 1024;

/// How an adapter authenticates. The secret lives in the OS keychain and
/// arrives here per instance; it is never logged and never stored.
///
/// `Debug` is hand-written and prints the *method* only. It must never be
/// derived: the whole point of the keychain is that the token exists in
/// exactly one place, and a derived `Debug` is how it reaches a panic message
/// and from there a bug report.
#[derive(Clone)]
pub enum Auth {
    /// Anonymous. Public Gitea instances allow it; nothing else in M1 does.
    None,
    /// `Authorization: Bearer <token>` -- Jira DC ≥ 8.14 PATs, TeamCity tokens.
    Bearer(String),
    /// Username + password or PAT, sent as HTTP Basic.
    Basic { username: String, password: String },
    /// `Authorization: token <pat>` -- Gitea's own spelling.
    GiteaToken(String),
}

impl std::fmt::Debug for Auth {
    /// The method, never the secret.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let method = match self {
            Self::None => "None",
            Self::Bearer(_) => "Bearer(…)",
            Self::Basic { .. } => "Basic(…)",
            Self::GiteaToken(_) => "GiteaToken(…)",
        };
        formatter.write_str(method)
    }
}

/// What one adapter instance's client is configured with.
///
/// `Debug`-able because [`Auth`]'s own `Debug` redacts and the base URL is
/// printed without a username or password in it; a config that could not be
/// logged at all would be logged by hand, field by field, three times.
pub struct HttpConfig {
    /// The instance's base URL, path prefix included.
    pub base_url: String,
    /// For the `User-Agent`, e.g. `"jira"`.
    pub adapter_kind: String,
    /// For the `User-Agent`: the adapter crate's own version.
    pub adapter_version: String,
    /// How this instance authenticates.
    pub auth: Auth,
    /// Sustained requests per second (§4.1 defaults: Jira 5, Gitea 10,
    /// TeamCity 5). Overridable per source in its config. Shared with every
    /// other client for the same origin, and the lowest asked for by a client
    /// still alive where they differ ([`InstanceLimiter`]).
    pub requests_per_second: u32,
    /// Burst allowance (§4.1 defaults: Jira 10, Gitea 20, TeamCity 10).
    /// Shared and merged the same way.
    pub burst: u32,
    /// How long to wait for a connection before calling the source
    /// unreachable.
    pub connect_timeout: Duration,
    /// How long to wait for a whole request, connection included -- or, for
    /// [`HttpClient::send_streaming`], for its response headers.
    pub request_timeout: Duration,
    /// How this adapter reads its own error envelope out of a failing response
    /// body (ADR-0004), or `None` to keep the raw excerpt.
    ///
    /// The body is consumed inside [`HttpClient::send`] (or `send_streaming`),
    /// the only door onto the wire, so this is the adapter's one chance at it. Every source knobas
    /// reads has an envelope of its own -- Jira's `{"errorMessages":[…]}`,
    /// Gitea's `{"message":…}`, TeamCity's plain text -- and the commonest sync
    /// failure of all, a mistyped filter, is a 400 whose body is the exact
    /// sentence the user needs.
    pub body_message: Option<classify::BodyMessage>,
    /// How this adapter tells a 403 that refuses the request from one that
    /// refuses the credential, or `None` to read every 403 as the credential's
    /// ([`classify::ForbiddenReading`], issue #722). Gitea's only.
    pub forbidden: Option<classify::ForbiddenReading>,
    /// The most bytes of one answer's body this client reads, as decoded
    /// ([`MAX_BODY`] by default, #1640). Over it, the read fails with
    /// [`SourceError::Protocol`] naming the cap; a failing status keeps its
    /// class and names the cap where its body would be. Not applied to a
    /// [`HttpClient::send_streaming`] body.
    pub max_body: u64,
}

impl std::fmt::Debug for HttpConfig {
    /// Every field, the base URL without its userinfo (#1640).
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Destructured without `..`, so a field added later cannot be left
        // out of this rendering without the compiler saying so.
        let Self {
            base_url,
            adapter_kind,
            adapter_version,
            auth,
            requests_per_second,
            burst,
            connect_timeout,
            request_timeout,
            body_message,
            forbidden,
            max_body,
        } = self;
        formatter
            .debug_struct("HttpConfig")
            .field(
                "base_url",
                &knobas_source::instance::redact_userinfo(base_url),
            )
            .field("adapter_kind", adapter_kind)
            .field("adapter_version", adapter_version)
            .field("auth", auth)
            .field("requests_per_second", requests_per_second)
            .field("burst", burst)
            .field("connect_timeout", connect_timeout)
            .field("request_timeout", request_timeout)
            .field("body_message", body_message)
            .field("forbidden", forbidden)
            .field("max_body", max_body)
            .finish()
    }
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            adapter_kind: "unknown".to_owned(),
            adapter_version: "0.0.0".to_owned(),
            auth: Auth::None,
            requests_per_second: 5,
            burst: 10,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            body_message: None,
            forbidden: None,
            max_body: MAX_BODY,
        }
    }
}

/// One adapter instance's HTTP client.
pub struct HttpClient {
    /// For `GET` and `HEAD`: follows a redirect as reqwest does, or as the
    /// check of [`HttpClient::with_redirect_check`] allows.
    inner: reqwest::Client,
    /// For every other method: follows none, so [`HttpClient::send`] sees the
    /// redirect and refuses it (#1640). reqwest's redirect policy cannot see
    /// the method, so the choice is made by picking the client.
    writes: reqwest::Client,
    /// [`HttpConfig::max_body`].
    max_body: u64,
    /// The per-request timeout, kept so [`HttpClient::send`] can shorten it to
    /// whatever is left of [`SEND_BUDGET`].
    request_timeout: Duration,
    /// [`SEND_BUDGET`], except in a test that cannot wait 45 s to see it run
    /// out ([`HttpClient::with_send_budget`], `test-util` only).
    send_budget: Duration,
    /// Trailing slash trimmed; see [`HttpClient::url_for`].
    base_url: String,
    auth: Auth,
    /// The instance origin's quota, shared with every other client for it.
    limiter: InstanceLimiter,
    /// The adapter's reading of a failing response body; see
    /// [`HttpConfig::body_message`].
    body_message: Option<classify::BodyMessage>,
    /// The adapter's reading of a 403; see [`HttpConfig::forbidden`].
    forbidden: Option<classify::ForbiddenReading>,
}

impl std::fmt::Debug for HttpClient {
    /// The instance it talks to and how it authenticates -- never the secret,
    /// and never the middleware chain, which has no readable form.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpClient")
            .field("base_url", &self.base_url)
            .field("auth", &self.auth)
            .finish_non_exhaustive()
    }
}

impl HttpClient {
    /// Build the client for one instance.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if the base URL is not an http(s) URL,
    /// carries a username or a password ([`refuse_userinfo`]), or the TLS
    /// backend cannot be built -- all configuration failures, and all must
    /// surface when the source is saved rather than mid-sync.
    pub fn new(config: HttpConfig) -> Result<Self, SourceError> {
        Self::build(config, reqwest::redirect::Policy::default())
    }

    /// Build a client whose `GET` and `HEAD` follow a redirect only when
    /// `check` allows its target (#1830). The check is asked about every hop
    /// before it is requested, the last one included; reqwest's limit of 10
    /// hops still holds. A client of its own, because reqwest's redirect
    /// policy belongs to the client and not to a request: what
    /// [`HttpClient::new`] builds is unchanged. Every other method follows no
    /// redirect, as on any client.
    ///
    /// A read redirected to a hop `check` refuses fails with
    /// [`SourceError::Protocol`] carrying the redirect's status and naming
    /// the hop, and is not sent again.
    ///
    /// # Errors
    ///
    /// As [`HttpClient::new`].
    pub fn with_redirect_check(
        config: HttpConfig,
        check: impl Fn(&Url) -> bool + Send + Sync + 'static,
    ) -> Result<Self, SourceError> {
        Self::build(config, redirect::policy(std::sync::Arc::new(check)))
    }

    /// [`HttpClient::new`], with `reads` as the policy a `GET` or `HEAD`
    /// follows redirects by.
    fn build(config: HttpConfig, reads: reqwest::redirect::Policy) -> Result<Self, SourceError> {
        let parsed = limiter::http_url(&config.base_url)?;

        let mut headers = HeaderMap::new();
        // TeamCity answers XML without this and every adapter wants JSON, so
        // it is a default rather than something three adapters remember.
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        let agent = format!(
            "knobas/{} ({}/{})",
            env!("CARGO_PKG_VERSION"),
            config.adapter_kind,
            config.adapter_version
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&agent)
                .map_err(|error| SourceError::protocol(format!("user agent: {error}")))?,
        );

        let build = |redirect: reqwest::redirect::Policy, idle: Option<Duration>| {
            let builder = reqwest::Client::builder();
            // reqwest's own idle timeout unless one is given.
            let builder = match idle {
                Some(idle) => builder.pool_idle_timeout(idle),
                None => builder,
            };
            builder
                // Explicit, and it must stay explicit. reqwest picks its backend
                // from the *union* of the features every crate in the workspace
                // asks for, and `TlsBackend::default()` prefers native-tls the
                // moment anything turns it on -- `postgresql_embedded` does, via
                // `postgresql_archive/tls-native-tls`. Left implicit, this client
                // would quietly validate against OpenSSL's root store instead of
                // the platform verifier, and the corporate-CA guarantee (roadmap
                // §4) would hold on nobody's machine.
                .use_rustls_tls()
                .connect_timeout(config.connect_timeout)
                // No client-wide `.timeout`: a request's timeout is set per
                // attempt in `send_with`, and a client-wide one would cut every
                // streamed body off at the request timeout, which is #1012's bug.
                .default_headers(headers.clone())
                .redirect(redirect)
                .build()
                .map_err(|error| {
                    SourceError::protocol(format!("building the http client: {error}"))
                })
        };
        // A write's connection is let go before the commonest keep-alive
        // timeout closes it under the write ([`WRITE_POOL_IDLE`], #1821).
        let client = build(reads, None)?;
        let writes = build(reqwest::redirect::Policy::none(), Some(WRITE_POOL_IDLE))?;

        let limiter = InstanceLimiter::for_origin(
            &parsed,
            limiter::Rate::new(config.requests_per_second, config.burst),
        );

        Ok(Self {
            inner: client,
            writes,
            max_body: config.max_body,
            request_timeout: config.request_timeout,
            send_budget: SEND_BUDGET,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            auth: config.auth,
            limiter,
            body_message: config.body_message,
            forbidden: config.forbidden,
        })
    }

    /// The absolute URL for `path`, with or without a leading slash.
    ///
    /// Deliberately string concatenation and not [`url::Url::join`]: joining
    /// `"/api/v1/user"` onto `"https://gitea.example/prefix"` **discards**
    /// `prefix`, and a source behind a path prefix is the setup that only
    /// breaks in someone's real deployment.
    #[must_use]
    pub fn url_for(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }

    /// A request with the base URL, auth and default headers applied.
    ///
    /// Returns a [`Request`], which is a builder with **no way to send
    /// itself**: [`HttpClient::send`] (or `send_streaming`) is the only thing
    /// that puts it on the wire. That is the point -- see [`Request`].
    #[must_use]
    pub fn request(&self, method: Method, path: &str) -> Request {
        // A read follows a redirect; anything else is sent on the client
        // that follows none (#1640).
        let follows = method == Method::GET || method == Method::HEAD;
        let client = if follows { &self.inner } else { &self.writes };
        let request = client.request(method, self.url_for(path));
        let request = match &self.auth {
            Auth::None => request,
            Auth::Bearer(token) => request.bearer_auth(token),
            Auth::Basic { username, password } => request.basic_auth(username, Some(password)),
            Auth::GiteaToken(token) => request.header("Authorization", format!("token {token}")),
        };
        Request {
            inner: request,
            retry: true,
            follows,
        }
    }

    /// Send a request: rate-limited, retried, `Retry-After`-aware, classified,
    /// and bounded.
    ///
    /// [`MAX_ATTEMPTS`] attempts at most, the first included. Every attempt --
    /// retries too -- waits for the rate limiter first. A retryable answer is
    /// followed by the delay the server asked for (`Retry-After`, capped at
    /// [`classify::RETRY_AFTER_CAP`]) if it asked for one, and by the
    /// exponential [`retry::backoff`] if it did not. The whole call, waits
    /// included, is bounded by [`SEND_BUDGET`]. A `GET` or `HEAD` the server
    /// closed its connection on before answering is sent once more, through
    /// the limiter and inside the budget like any retry
    /// ([`retry::closed_before_answering`], #1818).
    ///
    /// A request that cannot be cloned is never retried -- a retry has to
    /// re-send the same bytes, and guessing is worse than one attempt -- but
    /// it still fails with *its own* classification, not with a message about
    /// cloning. Nothing in M1 builds such a request (they are all GETs with no
    /// body).
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to (see [`classify`]). A retryable
    /// failure that exhausts the attempts, or the budget, surfaces as the
    /// *last* failure rather than as a retry-specific one: what the caller
    /// needs to know is what the source finally said. A first attempt the
    /// limiter holds past the budget is [`SourceError::Protocol`] with the
    /// [`LocalQuota`]'s message: the source said nothing, and was not asked.
    /// A request other than a `GET` or `HEAD` answered with a redirect is
    /// [`SourceError::Protocol`] with the redirect's status, naming its
    /// `Location`, and is not followed (#1640). A request the server closed
    /// its connection on before answering, and that is not sent again for it
    /// (a write, or one sent with [`Request::no_retry`]), is
    /// [`SourceError::Unconfirmed`] with no status: it reached the source and
    /// got no answer (#1821).
    ///
    /// The body of a success is read through [`HttpConfig::max_body`]: a read
    /// of it past the cap fails, which [`over_the_cap`] tells apart.
    pub async fn send(&self, request: Request) -> Result<reqwest::Response, SourceError> {
        self.send_with(request, Deadline::Whole).await
    }

    /// Send a read whose body may be too large to arrive within the request
    /// timeout -- a run's download or its whole log (issue #1012).
    ///
    /// Everything [`send`](Self::send) guarantees holds up to the response
    /// headers: the limiter, the retries, `Retry-After`, the classification
    /// and [`SEND_BUDGET`], with each attempt's timeout covering the wait for
    /// the headers only. The body then has no total deadline; it is bounded
    /// by `stall` instead -- [`StreamedBody::chunk`] gives up when no byte
    /// arrives for that long. A 1 GB artifact over a slow tunnel downloads
    /// whole; a server that stops sending is still noticed.
    ///
    /// # Errors
    ///
    /// As [`send`](Self::send); headers that do not arrive within the
    /// attempt's timeout are [`SourceError::Unreachable`], retried like any
    /// timeout.
    pub async fn send_streaming(
        &self,
        request: Request,
        stall: Duration,
    ) -> Result<StreamedBody, SourceError> {
        let response = self.send_with(request, Deadline::Headers).await?;
        Ok(StreamedBody { response, stall })
    }

    /// [`send`](Self::send)'s attempt loop, with `deadline` saying what each
    /// attempt's timeout covers.
    async fn send_with(
        &self,
        request: Request,
        deadline: Deadline,
    ) -> Result<reqwest::Response, SourceError> {
        // tokio's clock, which the limiter and every timeout here read too
        // (#1925): the monotonic clock, unless a test paused it.
        let started = Instant::now();
        // A request that may not be repeated ([`Request::no_retry`]) is
        // retried only after a connect error: the connection never opened, so
        // no byte of it reached the source.
        let retry = request.retry;
        let follows = request.follows;
        let mut request = request.inner;
        // What the attempt before this one ended in, once there was one: what
        // the call reports if the budget runs out before the next goes.
        let mut previous: Option<SourceError> = None;
        // Whether a read has been sent again after the server closed its
        // connection before answering: that happens once per call (#1818).
        let mut resent_after_close = false;

        for attempt in 1..=MAX_ATTEMPTS {
            // Cloned before sending, because sending consumes it. `None` for a
            // streaming body, which M1 never builds.
            let next = request.try_clone();
            let last = attempt == MAX_ATTEMPTS;

            // Every attempt, not just the first: a retry that skips the
            // limiter is the burst this crate exists to prevent. Inside the
            // budget, because waiting for a token is part of what the call
            // costs, and bounded by what is left of it (SPI-11, #1630).
            let left = self.budget_left(started).unwrap_or_default();
            let ready = self.limiter.until_ready_within(left).await;
            // Out of budget in the wait, or with none left once it ended:
            // either way this attempt never left. A retry reports what the
            // source said the attempt before; a first attempt has only the
            // wait to report, and it is knobas' own quota -- `Protocol`,
            // never `Unreachable`, since the source was never asked. The
            // error alone does not keep a backoff off, since a caller backs
            // off on a `Protocol` too; a caller tells a quota stop apart by
            // the mark below, under `watch_local_quota` (#1729, #1980).
            let attempt_timeout = match (ready, self.attempt_timeout(started)) {
                (Ok(()), Some(timeout)) => timeout,
                (ready, _) => {
                    let quota = match ready {
                        Err(ran_out) => ran_out,
                        Ok(()) => self.limiter.ran_out(left),
                    };
                    // Only a call that never reached the source is marked
                    // (#1729): a retry ends with what the source said.
                    return Err(previous.unwrap_or_else(|| {
                        limiter::stopped_on(&quota);
                        SourceError::protocol(quota.to_string())
                    }));
                }
            };

            // Set when this attempt is a read the server closed its
            // connection on before answering.
            let mut closed = false;
            // A transport failure, mapped, and whether it is transient: worth
            // another attempt.
            let classify_transport = |error: reqwest::Error| {
                // A hop the client's redirect check refused (#1830): the
                // check's answer, so another attempt would meet it again.
                if let Some(refused) = redirect::refused(&error) {
                    return (refused, false);
                }
                let hung_up = retry::closed_before_answering(&error);
                if retry && follows {
                    // A pooled keep-alive connection the server timed out as
                    // the request went out on it (#1818). Safe to repeat for a
                    // read (`follows` is a `GET` or `HEAD`), and once is what
                    // that race needs: the pool has dropped the closed
                    // connection, so the next attempt goes out on another
                    // one; a server that hangs up on that too is not racing.
                    closed = hung_up;
                    let again = closed && !resent_after_close;
                    let transient = again || retry::error_is_transient(&error);
                    (classify::reqwest_error(&error), transient)
                } else if hung_up {
                    // A write, or a request sent once, the server read whole
                    // and closed on without answering. It may have been acted
                    // on, so it is never sent again on this (#1818). It
                    // reached the source and got no answer, which is what
                    // `once_sent` reads as `Unconfirmed` (#1821): the write
                    // queue holds a write a second arrival would duplicate
                    // rather than refuse it.
                    let unanswered = SourceError::Unreachable(error.to_string());
                    (classify::once_sent(unanswered, true), false)
                } else if retry {
                    let transient = retry::error_is_transient(&error);
                    (classify::reqwest_error(&error), transient)
                } else {
                    // Sent once: past a connect error, it may have landed.
                    let mapped = classify::reqwest_error(&error);
                    let connected = !error.is_connect();
                    (classify::once_sent(mapped, connected), !connected)
                }
            };
            // The request counter of a test that holds a read to a budget
            // (#2239): this attempt goes on the wire.
            #[cfg(feature = "test-util")]
            counted::sent();
            let answered = match deadline {
                Deadline::Whole => request
                    .timeout(attempt_timeout)
                    .send()
                    .await
                    .map_err(classify_transport),
                // reqwest's own timeout would cover the body as well, so the
                // wait for the headers is bounded from outside instead. Late
                // headers are a timeout, and transient like one.
                Deadline::Headers => {
                    match tokio::time::timeout(attempt_timeout, request.send()).await {
                        Ok(answered) => answered.map_err(classify_transport),
                        Err(_) => {
                            let late = SourceError::Unreachable(format!(
                                "no response headers within {attempt_timeout:?}"
                            ));
                            if retry {
                                Err((late, true))
                            } else {
                                Err((classify::once_sent(late, true), false))
                            }
                        }
                    }
                }
            };
            // `delay` is how long to wait before the next attempt; `give_up`
            // is what to report if there is not going to be one. Carrying both
            // is what keeps a 503-that-cannot-be-retried a 503.
            let (delay, give_up) = match answered {
                Ok(response) if response.status().is_success() => {
                    // A download is large on purpose and is bounded by its
                    // stall timeout instead (#1012); every other body is
                    // read through the cap (#1640).
                    return Ok(match deadline {
                        Deadline::Whole => cap::capped(response, self.max_body),
                        Deadline::Headers => response,
                    });
                }
                // A write that met a redirect: refused, never re-sent. The
                // client it went out on follows none, so this is the answer
                // to the write itself, and nothing has been sent since.
                Ok(response) if !follows && response.status().is_redirection() => {
                    return Err(redirected(&response));
                }
                Ok(response) => {
                    let status = response.status();
                    let transient = retry && retry::status_is_transient(status.as_u16());
                    // Read before the body is consumed to build the message.
                    let asked_for = classify::parse_retry_after(response.headers());
                    // Bounded here too: a streamed send's response carries no
                    // deadline of its own, and an error body that dribbles
                    // must not hold the call open.
                    let body =
                        tokio::time::timeout(attempt_timeout, body_of(response, self.max_body))
                            .await
                            .unwrap_or_default();
                    let error =
                        classify::response_error(status, &body, self.body_message, self.forbidden);
                    if !retry {
                        return Err(classify::once_sent(error, true));
                    }
                    if last || !transient {
                        return Err(error);
                    }
                    // The server's own instruction wins over our guess, and it
                    // is read *before* the wait it applies to.
                    (asked_for.unwrap_or_else(|| retry::backoff(attempt)), error)
                }
                Err((mapped, transient)) => {
                    if last || !transient {
                        return Err(mapped);
                    }
                    (retry::backoff(attempt), mapped)
                }
            };

            // Unretryable in practice: re-send what, exactly? Reported as the
            // failure that actually happened.
            let Some(next) = next else {
                return Err(give_up);
            };
            // A wait that would run past the budget is a wait not worth
            // starting: the answer would arrive after the caller has given up.
            // This is the branch a `Retry-After` longer than the budget takes
            // -- a source asking for a minute is telling us to come back on
            // the next schedule, not to hold a transaction open for it.
            if self.budget_left(started).is_none_or(|left| delay >= left) {
                return Err(give_up);
            }
            tracing::debug!(attempt, ?delay, "retrying");
            tokio::time::sleep(delay).await;
            request = next;
            previous = Some(give_up);
            resent_after_close |= closed;
        }

        // `MAX_ATTEMPTS` is a non-zero constant, so the loop always returns.
        unreachable!("the attempt loop returns on its last attempt")
    }

    /// What is left of [`SEND_BUDGET`]. `None` once it is gone.
    ///
    /// This is the *call's* remaining time and nothing else -- deliberately
    /// not clamped to the request timeout, because it also has to answer "is
    /// there room to wait this long before retrying?", where the per-request
    /// timeout is irrelevant.
    fn budget_left(&self, started: Instant) -> Option<Duration> {
        self.send_budget
            .checked_sub(started.elapsed())
            .filter(|left| !left.is_zero())
    }

    /// How long the next attempt may take: the configured per-request timeout,
    /// or the rest of the budget if that is shorter.
    fn attempt_timeout(&self, started: Instant) -> Option<Duration> {
        Some(self.budget_left(started)?.min(self.request_timeout))
    }

    /// GET a JSON document.
    ///
    /// # Errors
    ///
    /// The mapped [`SourceError`], or [`SourceError::Protocol`] if the body is
    /// not the JSON the adapter expected -- which is a contract violation, not
    /// a connectivity problem -- or is over [`HttpConfig::max_body`], declared
    /// or as read: `<path>: the answer is larger than <cap>, the most knobas
    /// reads of one answer` (#1640).
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, SourceError> {
        self.get_json_with(path, query, None).await
    }

    /// GET a JSON document, with the body's start in the message when it is
    /// not the shape `T` asked for and `excerpt` says so (#877, #1313).
    ///
    /// With `excerpt` at `None` this is [`HttpClient::get_json`], message for
    /// message. With it set, a body that does not decode as `T` reads
    /// `<path> did not answer with the shape <contract> documents: <serde
    /// error>; it began "<the first BODY_EXCERPT_CHARS characters>"`, and a
    /// body that breaks off reads `<path>: the answer's body broke off:
    /// <error>`. A failing status is [`status_error`]'s either way: its body
    /// already reaches the message there, read by the caller's
    /// [`BodyMessage`] where it has one. A body over [`HttpConfig::max_body`]
    /// reads as [`HttpClient::get_json`] says, excerpt or not.
    ///
    /// `query` is anything [`Request::query`] takes, so a caller whose values
    /// are computed can hand over owned `String`s.
    ///
    /// # Errors
    ///
    /// As [`HttpClient::get_json`].
    pub async fn get_json_with<T, Q>(
        &self,
        path: &str,
        query: &Q,
        excerpt: Option<BodyExcerpt>,
    ) -> Result<T, SourceError>
    where
        T: serde::de::DeserializeOwned,
        Q: serde::Serialize + ?Sized,
    {
        let response = self
            .send(self.request(Method::GET, path).query(query))
            .await?;
        if let Some(over) = cap::declared_over(&response, self.max_body) {
            return Err(SourceError::protocol(format!("{path}: {over}")));
        }
        let Some(BodyExcerpt { contract }) = excerpt else {
            return response.json::<T>().await.map_err(|error| {
                read_error(path, &error, |error| format!("decoding {path}: {error}"))
            });
        };
        let body = response.text().await.map_err(|error| {
            read_error(path, &error, |error| {
                format!("{path}: the answer's body broke off: {error}")
            })
        })?;
        serde_json::from_str::<T>(&body).map_err(|error| {
            let start: String = body.chars().take(BODY_EXCERPT_CHARS).collect();
            SourceError::protocol(format!(
                "{path} did not answer with the shape {contract} documents: {error}; it began \
                 {start:?}"
            ))
        })
    }
}

/// How many characters of a body [`HttpClient::get_json_with`] quotes when it
/// is not the shape asked for: Jira's bound since #877.
///
/// Not [`classify::BODY_EXCERPT`], which bounds a *failing status's* body, in
/// bytes. Each keeps the bound its message had before it moved here.
pub const BODY_EXCERPT_CHARS: usize = 300;

/// The option that puts a body's start into [`HttpClient::get_json_with`]'s
/// message when the body is not the shape asked for (#877, #1313).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyExcerpt {
    /// Whose documentation the shape is, read into the message as "the shape
    /// `<contract>` documents", e.g. `"the Jira REST v2 contract"`.
    pub contract: &'static str,
}

/// What one attempt's timeout covers.
#[derive(Clone, Copy)]
enum Deadline {
    /// The whole exchange, body included ([`HttpClient::send`]).
    Whole,
    /// The response headers only ([`HttpClient::send_streaming`]).
    Headers,
}

/// A successful response whose body is read chunk by chunk, with no total
/// deadline and a stall timeout instead ([`HttpClient::send_streaming`]).
#[derive(Debug)]
pub struct StreamedBody {
    response: reqwest::Response,
    stall: Duration,
}

impl StreamedBody {
    /// The next bytes of the body, `None` at its end.
    ///
    /// # Errors
    ///
    /// [`BodyBrokeOff`] when no byte arrived within the stall timeout, or the
    /// connection failed mid-body. Either way the body is not whole, and the
    /// caller must not keep what it has as if it were.
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, BodyBrokeOff> {
        match tokio::time::timeout(self.stall, self.response.chunk()).await {
            Ok(Ok(chunk)) => Ok(chunk),
            Ok(Err(error)) => Err(BodyBrokeOff::Transport(error.to_string())),
            Err(_) => Err(BodyBrokeOff::Stalled(self.stall)),
        }
    }
}

/// Why a [`StreamedBody`] ended before its end. Its `Display` is the detail
/// an adapter puts after its own "the download of … broke off: ", in a
/// [`SourceError::Unreachable`]: the source failed to answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyBrokeOff {
    /// No byte arrived for this long.
    Stalled(Duration),
    /// The connection failed; the transport's own words.
    Transport(String),
}

impl std::fmt::Display for BodyBrokeOff {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stalled(stall) => write!(formatter, "no bytes for {stall:?}"),
            Self::Transport(detail) => formatter.write_str(detail),
        }
    }
}

impl std::error::Error for BodyBrokeOff {}

/// A request under construction, bound to the client that built it.
///
/// **This type exists to have no `send`.** A bare
/// `reqwest::RequestBuilder` carries an inherent `.send()`, and an adapter
/// holding one can reach the network directly -- skipping the rate limiter,
/// the retry budget, `Retry-After`, and the status → [`SourceError`] mapping
/// that are the entire reason three adapters share this crate. The bypass is
/// one character shorter than the correct call
/// (`client.send(req)` vs `req.send()`), it compiles, and it works, so it
/// would be found in review or not at all. Wrapping the builder makes
/// [`HttpClient::send`] (and `send_streaming`, the same loop) the only door.
///
/// The builder surface is deliberately narrow: what an adapter needs on a
/// GET, plus [`Request::json`] for M2's ratified write set (issue #43) --
/// nothing else that could carry a write. Anything more routes through the
/// orchestrator (P8).
pub struct Request {
    inner: reqwest::RequestBuilder,
    /// `false` once [`Request::no_retry`] said so.
    retry: bool,
    /// A `GET` or `HEAD`, built on the client that follows a redirect
    /// (#1640). Also what makes a request safe to send again after the
    /// server hung up on it (#1818).
    follows: bool,
}

impl std::fmt::Debug for Request {
    /// Never the headers: `Authorization` is in them.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Request").finish_non_exhaustive()
    }
}

#[cfg(feature = "test-util")]
impl HttpClient {
    /// The same client with a shorter [`SEND_BUDGET`]. **Tests only**: a test
    /// that watches the budget run out should not take 45 s to do it, and
    /// nothing outside a test may give a call more or less than the budget.
    #[must_use]
    pub fn with_send_budget(self, budget: Duration) -> Self {
        Self {
            send_budget: budget,
            ..self
        }
    }
}

#[cfg(feature = "test-util")]
impl Request {
    /// The request as `reqwest` would send it. **Tests only.**
    ///
    /// Behind `test-util` for the same reason [`Request`] exists at all: a
    /// built `reqwest::Request` can be handed to any `reqwest::Client`, which
    /// is a second route to the network past the limiter and the retry
    /// budget. Tests need it to assert what a request *carries* (the
    /// `Authorization` spelling per [`Auth`] variant, which is the one thing
    /// no adapter can be trusted to get right by inspection), and the feature
    /// is off in every build an adapter compiles against.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if the builder cannot produce a request --
    /// a malformed header value, in practice.
    pub fn build(self) -> Result<reqwest::Request, SourceError> {
        self.inner
            .build()
            .map_err(|error| SourceError::protocol(format!("building the request: {error}")))
    }
}

impl Request {
    /// Append query parameters, as `reqwest`'s own `query` does.
    #[must_use]
    pub fn query<T: serde::Serialize + ?Sized>(self, query: &T) -> Self {
        Self {
            inner: self.inner.query(query),
            ..self
        }
    }

    /// Set one header. The client's defaults (`Accept`, `User-Agent`) and its
    /// `Authorization` are already applied.
    ///
    /// Typed rather than generic over `TryInto`: a header built from a bad
    /// string then fails at *send* time, where it reads as a transport fault.
    /// Both types are re-exported as [`header`], so an adapter constructs them
    /// -- and handles a malformed one -- where the mistake actually is.
    #[must_use]
    pub fn header(
        self,
        name: reqwest::header::HeaderName,
        value: reqwest::header::HeaderValue,
    ) -> Self {
        Self {
            inner: self.inner.header(name, value),
            ..self
        }
    }

    /// Send `body` as the JSON request body, with `Content-Type:
    /// application/json`.
    ///
    /// Added for M2's write-backs (issue #43): every write in the ratified set
    /// -- a Jira transition, a Gitea pull request, a TeamCity build trigger --
    /// is a `POST` carrying a JSON document, and until this existed there was
    /// no way to put one on the wire without building a second `reqwest`
    /// client beside the limiter and the retry budget. Which is the one thing
    /// this crate exists to prevent, so the method belongs here rather than in
    /// an adapter.
    ///
    /// **The body is buffered, not streamed**, and that is load-bearing rather
    /// than incidental: [`HttpClient::send`] retries by `try_clone`, which
    /// answers `None` for a streaming body and turns a retryable failure into
    /// a single attempt. A serialized `serde_json::Value` clones, so a write
    /// gets the same retry budget every read has.
    ///
    /// A value that cannot be serialized -- which for `serde_json` means a map
    /// with non-string keys or a type whose `Serialize` fails -- surfaces at
    /// [`HttpClient::send`] as the transport fault `reqwest` reports, the same
    /// as any other request that could not be built. Adapters build their
    /// bodies from `serde_json::json!`, which cannot produce one.
    #[must_use]
    pub fn json<T: serde::Serialize + ?Sized>(self, body: &T) -> Self {
        Self {
            inner: self.inner.json(body),
            ..self
        }
    }

    /// Send `body` verbatim, with `Content-Type: text/plain;charset=UTF-8`.
    ///
    /// Added for M4.1's Uptime Kuma write half (issue #452), and for a
    /// protocol rather than for a preference: **engine.io's polling transport
    /// posts a packet, not a document**. A socket.io emit goes out as the
    /// bytes `42["pauseMonitor",8]`, which is not JSON -- the leading `42` is
    /// the packet type and the ack id -- so [`Request::json`] cannot carry it
    /// in any spelling. Handed a `String` it would send `"42[…]"`, quotes
    /// included, which engine.io reads as a packet of type `"`.
    ///
    /// The alternative was a second `reqwest` client inside the adapter, past
    /// the rate limiter, the retry budget and the status → [`SourceError`]
    /// mapping -- which is the one thing this crate exists to prevent, and the
    /// reason [`Request`] has no `send` of its own. So the body method belongs
    /// here, ten lines beside `json`, rather than a transport belonging to an
    /// adapter.
    ///
    /// Buffered, not streamed, for `json`'s reason: a `String` body clones, so
    /// [`HttpClient::send`] can still retry it.
    ///
    /// **A caller must know what a retry means for its protocol.** `send`
    /// retries a transient failure, and a retried emit is a second emit. A
    /// packet that must not arrive twice -- Kuma's `add` is one -- says so
    /// with [`Request::no_retry`] (issue #1122).
    #[must_use]
    pub fn text(self, body: String) -> Self {
        Self {
            inner: self
                .inner
                .header(reqwest::header::CONTENT_TYPE, "text/plain;charset=UTF-8")
                .body(body),
            ..self
        }
    }

    /// Send this request **once**: [`HttpClient::send`] returns the failure of
    /// the attempt that reached the source as it is, timeouts and busy answers
    /// included (issue #1122). Only a connect error is retried as before: the
    /// connection never opened, so nothing was sent.
    ///
    /// For a write that does something new each time it arrives -- a create,
    /// a comment, a queued run -- and has no precondition the source checks.
    /// A timed-out request may have reached the source before the answer was
    /// lost; resending it blindly is how one create becomes two. The same
    /// goes for a `502` or `504`, which come from a proxy that cannot say
    /// whether the source behind it acted. The failure goes back to the
    /// caller, and the write queue, which can look before it writes again,
    /// decides what happens next.
    ///
    /// Not for reads, and not for a write the source refuses the second time
    /// on its own (a `sha` or version precondition, a name that must be
    /// unique): those keep the retries every other request gets.
    #[must_use]
    pub fn no_retry(self) -> Self {
        Self {
            retry: false,
            ..self
        }
    }
}

/// A response body, or an empty string if it cannot be read -- this is only
/// ever used to build an error message.
///
/// Read through the cap (#1640). Over it, the "body" is the sentence naming
/// the cap, so the status keeps its class -- a `503` is still retried, a
/// `401` still asks for the credential -- and the message says why it
/// carries no excerpt.
async fn body_of(response: reqwest::Response, max_body: u64) -> String {
    if let Some(over) = cap::declared_over(&response, max_body) {
        return over.to_string();
    }
    match cap::capped(response, max_body).text().await {
        Ok(body) => body,
        Err(error) => cap::ran_into(&error)
            .map(|over| over.to_string())
            .unwrap_or_default(),
    }
}

/// Whether a failed read of a body [`HttpClient::send`] handed back failed on
/// the cap ([`HttpConfig::max_body`], #1640): for a caller that reads the body
/// itself and says so in its own words.
#[must_use]
pub fn over_the_cap(error: &reqwest::Error) -> bool {
    cap::ran_into(error).is_some()
}

/// A failed read of `path`'s answer: the cap's sentence when the cap is why,
/// `otherwise`'s reading of the error when it is not.
fn read_error(
    path: &str,
    error: &reqwest::Error,
    otherwise: impl FnOnce(&reqwest::Error) -> String,
) -> SourceError {
    match cap::ran_into(error) {
        Some(over) => SourceError::protocol(format!("{path}: {over}")),
        None => SourceError::protocol(otherwise(error)),
    }
}

/// A write answered with a redirect (#1640, SPI-7).
///
/// `Protocol` with the redirect's status, which the write queue records as a
/// refusal: a proxy that moves a write is a base URL the reader has to fix,
/// and guessing whether to re-send the write elsewhere, or as which method,
/// is not this crate's to do. Under ADR-0012 a write that reads as success
/// must have happened, and following the redirect as reqwest does turns a
/// `POST` into a `GET` whose `200` reads as exactly that.
fn redirected(response: &reqwest::Response) -> SourceError {
    let status = response.status();
    let location = response
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|location| location.to_str().ok())
        .unwrap_or("nowhere it named");
    SourceError::Protocol {
        status: Some(status.as_u16()),
        message: format!(
            "HTTP {status}: the write was redirected to {location}, and knobas does not follow a \
             redirect on a write; the source's address may need to be the one it redirects to"
        ),
    }
}

/// Refuse a base URL that carries a username or a password (#1640, SPI-10).
///
/// reqwest would send one as HTTP Basic beside the credential the source is
/// configured with, and the URL is stored in `knobas.source_config` and
/// printed wherever it is. [`HttpClient::new`] calls this, and so does the
/// source save, so the Add-source dialog refuses what every sync would. A
/// string that is not a URL passes: [`HttpClient::new`] refuses it on its own
/// grounds.
///
/// # Errors
///
/// [`SourceError::Protocol`], whose message does not repeat the URL.
pub fn refuse_userinfo(base_url: &str) -> Result<(), SourceError> {
    match url::Url::parse(base_url) {
        Ok(parsed) => limiter::refuse_userinfo_in(&parsed),
        Err(_) => Ok(()),
    }
}
