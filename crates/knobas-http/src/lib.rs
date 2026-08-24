//! The HTTP stack every knobas adapter shares.
//!
//! Read-only for M1 (interfaces §8 P8). What it guarantees, so that three
//! adapters cannot disagree about any of it:
//!
//! * **rustls with the platform's native roots** -- a corporate CA works
//!   without a bundled root store (roadmap §4).
//! * **Timeouts**: 10 s to connect, 30 s for the whole request. A sync that
//!   hangs is worse than one that fails: the scheduler retries a failure.
//! * **Retries**: [`MAX_ATTEMPTS`] attempts in total -- the first one included
//!   -- exponential, transient statuses only, and `Retry-After` obeyed
//!   *before* the retry it applies to, up to [`classify::RETRY_AFTER_CAP`].
//! * **Rate limiting**: per instance, so two Jiras do not share a budget, and
//!   **every attempt passes it**, retries included.
//! * **One way out**: [`HttpClient::send`] is the only thing that puts a
//!   request on the wire. [`HttpClient::request`] hands back a [`Request`],
//!   which has no `send` of its own -- see that type for why.
//! * **One fault mapping** ([`classify`]), because `Unauthorized` is the fault
//!   the user is asked to act on.
//! * **`User-Agent: knobas/<version> (<adapter_kind>/<adapter_version>)`** --
//!   an admin reading their access log can tell what is calling them.

pub mod classify;
pub mod retry;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use knobas_source::SourceError;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};

pub use classify::{reqwest_error, status_error};

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
pub use reqwest::{Method, Response, StatusCode, header};

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

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

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
/// `Debug`-able because [`Auth`]'s own `Debug` redacts; a config that could
/// not be logged at all would be logged by hand, field by field, three times.
#[derive(Debug)]
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
    /// TeamCity 5). Overridable per source in its config.
    pub requests_per_second: u32,
    /// Burst allowance (§4.1 defaults: Jira 10, Gitea 20, TeamCity 10).
    pub burst: u32,
    /// How long to wait for a connection before calling the source
    /// unreachable.
    pub connect_timeout: Duration,
    /// How long to wait for a whole request, connection included.
    pub request_timeout: Duration,
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
        }
    }
}

/// One adapter instance's HTTP client.
pub struct HttpClient {
    inner: reqwest::Client,
    /// The per-request timeout, kept so [`HttpClient::send`] can shorten it to
    /// whatever is left of [`SEND_BUDGET`].
    request_timeout: Duration,
    /// Trailing slash trimmed; see [`HttpClient::url_for`].
    base_url: String,
    auth: Auth,
    limiter: Arc<Limiter>,
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
    /// [`SourceError::Protocol`] if the base URL is not a URL or the TLS
    /// backend cannot be built -- both are configuration failures, and both
    /// must surface when the source is saved rather than mid-sync.
    pub fn new(config: HttpConfig) -> Result<Self, SourceError> {
        let parsed = url::Url::parse(&config.base_url).map_err(|error| {
            SourceError::Protocol(format!("base url {:?}: {error}", config.base_url))
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(SourceError::Protocol(format!(
                "base url {:?} is not http(s)",
                config.base_url
            )));
        }

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
                .map_err(|error| SourceError::Protocol(format!("user agent: {error}")))?,
        );

        let client = reqwest::Client::builder()
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
            .timeout(config.request_timeout)
            .default_headers(headers)
            .build()
            .map_err(|error| SourceError::Protocol(format!("building the http client: {error}")))?;

        let quota = Quota::per_second(nonzero(config.requests_per_second))
            .allow_burst(nonzero(config.burst));

        Ok(Self {
            inner: client,
            request_timeout: config.request_timeout,
            base_url: config.base_url.trim_end_matches('/').to_owned(),
            auth: config.auth,
            limiter: Arc::new(RateLimiter::direct(quota)),
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
    /// itself**: [`HttpClient::send`] is the only thing that puts it on the
    /// wire. That is the point -- see [`Request`].
    #[must_use]
    pub fn request(&self, method: Method, path: &str) -> Request {
        let request = self.inner.request(method, self.url_for(path));
        let request = match &self.auth {
            Auth::None => request,
            Auth::Bearer(token) => request.bearer_auth(token),
            Auth::Basic { username, password } => request.basic_auth(username, Some(password)),
            Auth::GiteaToken(token) => request.header("Authorization", format!("token {token}")),
        };
        Request { inner: request }
    }

    /// Send a request: rate-limited, retried, `Retry-After`-aware, classified,
    /// and bounded.
    ///
    /// [`MAX_ATTEMPTS`] attempts at most, the first included. Every attempt --
    /// retries too -- waits for the rate limiter first. A retryable answer is
    /// followed by the delay the server asked for (`Retry-After`, capped at
    /// [`classify::RETRY_AFTER_CAP`]) if it asked for one, and by the
    /// exponential [`retry::backoff`] if it did not. The whole call, waits
    /// included, is bounded by [`SEND_BUDGET`].
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
    /// needs to know is what the source finally said.
    pub async fn send(&self, request: Request) -> Result<reqwest::Response, SourceError> {
        let started = Instant::now();
        let mut request = request.inner;

        for attempt in 1..=MAX_ATTEMPTS {
            // Cloned before sending, because sending consumes it. `None` for a
            // streaming body, which M1 never builds.
            let next = request.try_clone();
            let last = attempt == MAX_ATTEMPTS;

            // Every attempt, not just the first: a retry that skips the
            // limiter is the burst this crate exists to prevent. Before the
            // budget is measured out, because waiting for a token is part of
            // what the call costs.
            self.limiter.until_ready().await;

            let Some(remaining) = self.remaining(started) else {
                return Err(SourceError::Unreachable(format!(
                    "no answer within {}s",
                    SEND_BUDGET.as_secs()
                )));
            };

            // `delay` is how long to wait before the next attempt; `give_up`
            // is what to report if there is not going to be one. Carrying both
            // is what keeps a 503-that-cannot-be-retried a 503.
            let (delay, give_up) = match request.timeout(remaining).send().await {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let transient = retry::status_is_transient(status.as_u16());
                    // Read before the body is consumed to build the message.
                    let asked_for = classify::parse_retry_after(response.headers());
                    let error = classify::status_error(status, &body_of(response).await);
                    if last || !transient {
                        return Err(error);
                    }
                    // The server's own instruction wins over our guess, and it
                    // is read *before* the wait it applies to.
                    (asked_for.unwrap_or_else(|| retry::backoff(attempt)), error)
                }
                Err(error) => {
                    let mapped = classify::reqwest_error(&error);
                    if last || !retry::error_is_transient(&error) {
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
            // starting: the answer would arrive after the caller gave up.
            if self.remaining(started).is_none_or(|left| delay >= left) {
                return Err(give_up);
            }
            tracing::debug!(attempt, ?delay, "retrying");
            tokio::time::sleep(delay).await;
            request = next;
        }

        // `MAX_ATTEMPTS` is a non-zero constant, so the loop always returns.
        unreachable!("the attempt loop returns on its last attempt")
    }

    /// How long the next attempt may take: what is left of [`SEND_BUDGET`],
    /// never more than the configured per-request timeout. `None` once the
    /// budget is gone.
    fn remaining(&self, started: Instant) -> Option<Duration> {
        let left = SEND_BUDGET.checked_sub(started.elapsed())?;
        (!left.is_zero()).then(|| left.min(self.request_timeout))
    }

    /// GET a JSON document.
    ///
    /// # Errors
    ///
    /// The mapped [`SourceError`], or [`SourceError::Protocol`] if the body is
    /// not the JSON the adapter expected -- which is a contract violation, not
    /// a connectivity problem.
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, SourceError> {
        let response = self
            .send(self.request(Method::GET, path).query(query))
            .await?;
        response
            .json::<T>()
            .await
            .map_err(|error| SourceError::Protocol(format!("decoding {path}: {error}")))
    }
}

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
/// [`HttpClient::send`] the only door.
///
/// The builder surface is deliberately narrow: what a read-only M1 adapter
/// needs on a GET, and nothing that could carry a write. Anything more routes
/// through the orchestrator (P8: this crate is read-only for M1).
pub struct Request {
    inner: reqwest::RequestBuilder,
}

impl std::fmt::Debug for Request {
    /// Never the headers: `Authorization` is in them.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Request").finish_non_exhaustive()
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
            .map_err(|error| SourceError::Protocol(format!("building the request: {error}")))
    }
}

impl Request {
    /// Append query parameters, as `reqwest`'s own `query` does.
    #[must_use]
    pub fn query<T: serde::Serialize + ?Sized>(self, query: &T) -> Self {
        Self {
            inner: self.inner.query(query),
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
        }
    }
}

/// A response body, or an empty string if it cannot be read -- this is only
/// ever used to build an error message.
async fn body_of(response: reqwest::Response) -> String {
    response.text().await.unwrap_or_default()
}

/// Quotas cannot be zero; a misconfigured `0/s` means "one", not "never".
fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
}
