//! The HTTP stack every knobas adapter shares.
//!
//! Read-only for M1 (interfaces §8 P8). What it guarantees, so that three
//! adapters cannot disagree about any of it:
//!
//! * **rustls with the platform's native roots** -- a corporate CA works
//!   without a bundled root store (roadmap §4).
//! * **Timeouts**: 10 s to connect, 30 s for the whole request. A sync that
//!   hangs is worse than one that fails: the scheduler retries a failure.
//! * **Retries**: three attempts, exponential, transient statuses only, and
//!   `Retry-After` obeyed up to [`classify::RETRY_AFTER_CAP`].
//! * **Rate limiting**: per instance, so two Jiras do not share a budget.
//! * **One fault mapping** ([`classify`]), because `Unauthorized` is the fault
//!   the user is asked to act on.
//! * **`User-Agent: knobas/<version> (<adapter_kind>/<adapter_version>)`** --
//!   an admin reading their access log can tell what is calling them.

pub mod classify;
pub mod retry;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use knobas_source::SourceError;
use reqwest::header::{ACCEPT, HeaderMap, HeaderValue, USER_AGENT};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::RetryTransientMiddleware;
use reqwest_retry::policies::ExponentialBackoff;

pub use classify::{reqwest_error, status_error, transport_error};

// Every type this crate's signatures name, re-exported: an adapter depends on
// `knobas-http` and on nothing else for its transport. Otherwise all three
// would list `reqwest` and `reqwest-middleware` themselves just to spell
// `Method` and `RequestBuilder` -- three chances to pick a different version
// of the stack this crate exists to make singular, and three Cargo.toml edits
// for a bump that P8 says routes through the orchestrator.
pub use reqwest::{self, Method, Response, StatusCode, header};
pub use reqwest_middleware::{self, RequestBuilder};

/// Total attempts per request, the first one included.
pub const MAX_ATTEMPTS: u32 = 3;

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
    inner: ClientWithMiddleware,
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

        let backoff =
            ExponentialBackoff::builder().build_with_max_retries(MAX_ATTEMPTS.saturating_sub(1));
        let inner = ClientBuilder::new(client)
            .with(RetryTransientMiddleware::new_with_policy_and_strategy(
                backoff,
                retry::TransientOnly,
            ))
            .build();

        let quota = Quota::per_second(nonzero(config.requests_per_second))
            .allow_burst(nonzero(config.burst));

        Ok(Self {
            inner,
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
    /// No `#[must_use]`: `RequestBuilder` already carries one.
    pub fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let request = self.inner.request(method, self.url_for(path));
        match &self.auth {
            Auth::None => request,
            Auth::Bearer(token) => request.bearer_auth(token),
            Auth::Basic { username, password } => request.basic_auth(username, Some(password)),
            Auth::GiteaToken(token) => request.header("Authorization", format!("token {token}")),
        }
    }

    /// Send a request: rate-limited, retried, `Retry-After`-aware, classified.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to (see [`classify`]).
    pub async fn send(&self, request: RequestBuilder) -> Result<reqwest::Response, SourceError> {
        // Cloned before the first send so a `Retry-After` can be honoured:
        // the retry middleware's policy never sees response headers, so this
        // is the only place that can read one.
        let retry = request.try_clone();
        let response = self.dispatch(request).await?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if let (Some(delay), Some(retry)) = (classify::parse_retry_after(response.headers()), retry)
        {
            tracing::debug!(?delay, %status, "honouring Retry-After");
            tokio::time::sleep(delay).await;
            let response = self.dispatch(retry).await?;
            if response.status().is_success() {
                return Ok(response);
            }
            let status = response.status();
            return Err(classify::status_error(status, &body_of(response).await));
        }
        Err(classify::status_error(status, &body_of(response).await))
    }

    /// One `limit → send` pass.
    async fn dispatch(&self, request: RequestBuilder) -> Result<reqwest::Response, SourceError> {
        self.limiter.until_ready().await;
        request
            .send()
            .await
            .map_err(|error| classify::transport_error(&error))
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

/// A response body, or an empty string if it cannot be read -- this is only
/// ever used to build an error message.
async fn body_of(response: reqwest::Response) -> String {
    response.text().await.unwrap_or_default()
}

/// Quotas cannot be zero; a misconfigured `0/s` means "one", not "never".
fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap_or(NonZeroU32::MIN)
}
