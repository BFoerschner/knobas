//! The only module in this crate that speaks HTTP.
//!
//! Everything else is pure -- the parser, the fold, the mapping and the cursor
//! all work on values -- so the interesting half of this adapter is tested
//! without a socket, and `tests/contract.rs` puts a server under it without a
//! container.
//!
//! The transport is [`knobas_http`]'s, and that crate changes only through the
//! orchestrator (contract §10.8): rustls with the platform's root store, the
//! retry budget, `Retry-After`, the per-instance rate limiter, the
//! `User-Agent`, and the one fault mapping every adapter agrees on (401 **and**
//! 403 → [`SourceError::Unauthorized`]; connect/DNS/TLS/timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`]),
//! each carrying the status it came from (ADR-0004).
//!
//! What is Kuma's, and therefore here: **how an API key is sent**, and the one
//! path this adapter reads.

use knobas_http::{Auth, HttpClient, HttpConfig, Method};
use knobas_source::{AuthMethod, SourceError};

/// The one endpoint this adapter reads.
///
/// Uptime Kuma v2 has no REST API for reading monitors -- the dashboard runs on
/// socket.io -- and an API key opens exactly this (spec #427, *The Kuma
/// adapter*: "Reads poll `/metrics` with the API key").
pub(crate) const METRICS: &str = "/metrics";

/// The [`Auth`] a Kuma API key means.
///
/// **HTTP Basic with an empty username**, which is how Kuma authenticates
/// `/metrics` and how the environment's own README reads it back
/// (`curl -u ":$(cat kuma-api-key)"`). Not a bearer token: measured on the
/// pinned image, an unauthenticated request answers `401` with
/// `WWW-Authenticate: Basic` and an empty body.
///
/// # Errors
///
/// [`SourceError::Unauthorized`] when the keychain holds no key -- the
/// `missing_secret` state of contract §3, which the sources view offers
/// *Re-enter* for. [`SourceError::Protocol`] for a source configured with no
/// authentication method, or with one Kuma's `/metrics` does not speak.
pub(crate) fn credential(
    auth: Option<AuthMethod>,
    secret: Option<&str>,
) -> Result<Auth, SourceError> {
    // The configuration is judged before the secret, and the order matters:
    // these are two states with two remedies (§3). `missing_secret` sends the
    // user to *Re-enter*; a source that could not authenticate even with a key
    // in hand sends them to its settings, and reporting `Unauthorized` for
    // that would send them to retype a key that was never the problem.
    match auth {
        Some(AuthMethod::ApiToken) => {}
        None => {
            return Err(SourceError::protocol(
                "this Uptime Kuma source has no authentication method configured; choose an API \
                 key"
                .to_owned(),
            ));
        }
        Some(other) => {
            return Err(SourceError::protocol(format!(
                "{other:?} authentication is not supported for Uptime Kuma; /metrics takes an API \
                 key, sent as HTTP Basic with an empty username"
            )));
        }
    }
    let secret = secret.ok_or_else(SourceError::unauthorized)?;
    Ok(Auth::Basic {
        username: String::new(),
        password: secret.to_owned(),
    })
}

/// One Kuma instance's HTTP access.
#[derive(Debug)]
pub(crate) struct KumaHttp {
    client: HttpClient,
}

impl KumaHttp {
    /// Build the client for one configured source.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if the base URL is not an http(s) URL or the
    /// client cannot be built -- both are configuration failures, and both
    /// surface when the source is saved rather than mid-sync.
    pub(crate) fn new(
        base_url: &str,
        cfg: &crate::KumaConfig,
        auth: Auth,
    ) -> Result<Self, SourceError> {
        Ok(Self {
            client: HttpClient::new(HttpConfig {
                base_url: base_url.trim().to_owned(),
                adapter_kind: crate::ADAPTER_KIND.to_owned(),
                adapter_version: crate::ADAPTER_VERSION.to_owned(),
                auth,
                requests_per_second: cfg.rate_per_sec,
                burst: cfg.rate_burst,
                connect_timeout: std::time::Duration::from_secs(cfg.connect_timeout_secs),
                request_timeout: std::time::Duration::from_secs(cfg.request_timeout_secs),
                // No error envelope to lift: Kuma answers a refused key with an
                // empty body, and everything else on this path with express's
                // own HTML. Keeping the bounded excerpt is the honest reading of
                // a body this adapter cannot parse.
                body_message: None,
            })?,
        })
    }

    /// The whole `/metrics` document, as text.
    ///
    /// Text and not JSON: the Prometheus exposition format is a line protocol,
    /// and [`crate::metrics`] is what reads it.
    ///
    /// # Errors
    ///
    /// The mapped [`SourceError`] -- [`SourceError::Unauthorized`] for the
    /// `401` a wrong or missing API key gets, which is the credential-health
    /// path end to end.
    pub(crate) async fn metrics(&self) -> Result<String, SourceError> {
        let response = self
            .client
            .send(self.client.request(Method::GET, METRICS))
            .await?;
        response
            .text()
            .await
            .map_err(|error| SourceError::protocol(format!("reading {METRICS}: {error}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The API key goes out as Basic with an empty username, which is what
    /// Kuma's `/metrics` accepts and what the test environment's README reads
    /// it back with.
    #[test]
    fn an_api_key_is_basic_auth_with_no_username() {
        let auth = credential(Some(AuthMethod::ApiToken), Some("uk1_secret")).unwrap();
        match auth {
            Auth::Basic { username, password } => {
                assert_eq!(username, "");
                assert_eq!(password, "uk1_secret");
            }
            other => panic!("expected Basic, got {other:?}"),
        }
    }

    /// The two ways a source can be unusable, and the different remedies they
    /// send the user to. A missing key is `Unauthorized` -- *Re-enter* -- and a
    /// method this adapter cannot speak is a configuration fault that says so
    /// by name.
    #[test]
    fn a_missing_key_and_a_wrong_method_are_different_failures() {
        assert!(matches!(
            credential(Some(AuthMethod::ApiToken), None),
            Err(SourceError::Unauthorized { status: None })
        ));
        for wrong in [AuthMethod::Pat, AuthMethod::UserPassword, AuthMethod::OAuth] {
            let refused = credential(Some(wrong), Some("uk1_secret"));
            let message = match refused {
                Err(SourceError::Protocol { message, .. }) => message,
                other => panic!("expected Protocol, got {other:?}"),
            };
            assert!(message.contains("API key"), "{message}");
        }
        assert!(matches!(
            credential(None, Some("uk1_secret")),
            Err(SourceError::Protocol { .. })
        ));
    }
}
