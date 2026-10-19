//! The only module in this crate that speaks HTTP.
//!
//! Everything else is pure -- the parser, the fold, the mapping and the cursor
//! all work on values -- so the interesting half of this adapter is tested
//! without a socket, and `tests/it/contract.rs` puts a server under it without a
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
//! What is Kuma's, and therefore here: **the [`CredentialRules`] its API key is
//! sent under**, and the one path this adapter reads.

use knobas_http::{Auth, CredentialRules, HttpClient, HttpConfig, Method, TokenSpelling};
use knobas_source::{AuthMethod, SourceError};

/// The one endpoint this adapter reads.
///
/// Uptime Kuma v2 has no REST API for reading monitors -- the dashboard runs on
/// socket.io -- and an API key opens exactly this (spec #427, *The Kuma
/// adapter*: "Reads poll `/metrics` with the API key").
pub(crate) const METRICS: &str = "/metrics";

pub(crate) use knobas_http::credential;

/// Which [`Auth`] a Kuma API key means, and what a source that cannot
/// authenticate reads as. The mapping itself, and the order it judges in, are
/// [`credential`]'s.
///
/// An API key goes as **HTTP Basic with an empty username**, the transport's
/// mapping for [`AuthMethod::ApiToken`], which is how Kuma authenticates
/// `/metrics` and how the environment's own README reads it back
/// (`curl -u ":$(cat kuma-api-key)"`). Not a bearer token: measured on the
/// pinned image, an unauthenticated request answers `401` with
/// `WWW-Authenticate: Basic` and an empty body.
pub(crate) const CREDENTIAL_RULES: CredentialRules = CredentialRules {
    // Kuma offers no personal access token; `methods` leaves `Pat` out, so
    // this spelling is never reached.
    pat: TokenSpelling::Bearer,
    // What `descriptor_template().auth_methods` declares and the Add-source
    // form offers; anything else is refused rather than guessed at.
    methods: &[AuthMethod::ApiToken],
    unconfigured: "this Uptime Kuma source has no authentication method configured; choose an \
                   API key",
    unsupported: |method| {
        format!(
            "{method:?} authentication is not supported for Uptime Kuma; /metrics takes an API \
             key, sent as HTTP Basic with an empty username"
        )
    },
};

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
                forbidden: None,
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

    /// The Add-source form is generated from the descriptor's `auth_methods`
    /// (spec §3a), and the transport refuses any method the rules leave out.
    /// The two lists drifting apart either offers a method that is then
    /// refused, or accepts one nobody can choose.
    #[test]
    fn the_credential_rules_speak_exactly_what_the_form_offers() {
        assert_eq!(
            CREDENTIAL_RULES.methods,
            crate::descriptor_template().auth_methods.as_slice()
        );
    }
}
