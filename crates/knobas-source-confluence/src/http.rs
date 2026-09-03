//! The only module in this crate that speaks HTTP.
//!
//! Everything else is pure: the mapping, the stripper, the cursor and the CQL
//! renderer all work on values, and the sync run is written against a trait it
//! can be faked through. That split is deliberate -- the interesting logic is
//! paging and cursor discipline, and neither needs a socket to be tested.
//!
//! # What lives here and what does not
//!
//! The transport is [`knobas_http`]'s, and that crate changes only through the
//! orchestrator (contract §10.8): rustls with the platform's root store, the
//! retry budget, `Retry-After`, the per-instance rate limiter, the
//! `User-Agent`, and the one fault mapping every adapter must agree on (401
//! **and** 403 → [`SourceError::Unauthorized`]; connect/DNS/TLS/timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`]),
//! each carrying the status it came from ([`SourceError::status`], ADR-0004).
//!
//! What is Confluence's and therefore here: which [`knobas_http::Auth`]
//! variant the configured [`AuthMethod`] means, and reading Confluence's
//! `{"statusCode":…,"message":…}` envelope out of a failing response body so
//! the sources view shows a sentence rather than a JSON document.
//!
//! Confining every mention of `knobas-http` to this one file is on purpose: a
//! change requested from the orchestrator then costs one file to apply, not a
//! sweep across the crate.

use knobas_http::{Auth, HttpClient, HttpConfig, Method};
use knobas_source::{AuthMethod, SourceError};

/// The [`Auth`] the configured method means.
///
/// # Errors
///
/// [`SourceError::Unauthorized`] when the keychain holds no secret -- the
/// `missing_secret` state of contract §3, which the sources view offers
/// *Re-enter* for and the scheduler does not back off over.
/// [`SourceError::Protocol`] for a configuration that cannot authenticate at
/// all: no method chosen, a method Confluence DC does not speak, or
/// user+password with no username beside it.
pub(crate) fn credential(
    auth: Option<AuthMethod>,
    username: Option<&str>,
    secret: Option<&str>,
) -> Result<Auth, SourceError> {
    // The configuration is judged **before** the secret, and the order is the
    // whole point. These are two different states with two different remedies
    // (§3, §10.2): `missing_secret` sends the user to *Re-enter*, a
    // misconfigured source sends them to the source's settings. A source that
    // could not authenticate even with a secret in hand has the second
    // problem, and reporting `Unauthorized` for it sends the user to retype a
    // token that was never the issue.
    let scheme = scheme(auth, username)?;
    let secret = secret.ok_or_else(SourceError::unauthorized)?;
    Ok(match scheme {
        Scheme::Bearer => Auth::Bearer(secret.to_owned()),
        Scheme::Basic(username) => Auth::Basic {
            username: username.to_owned(),
            password: secret.to_owned(),
        },
    })
}

/// How this source authenticates, decided from configuration alone.
enum Scheme<'a> {
    Bearer,
    Basic(&'a str),
}

fn scheme(auth: Option<AuthMethod>, username: Option<&str>) -> Result<Scheme<'_>, SourceError> {
    let Some(auth) = auth else {
        // `SourceInstance::auth` is optional because some sources need no
        // credential (P6). Confluence DC is not one of them, and neither
        // method the descriptor declares can be guessed at from a bare secret.
        return Err(SourceError::protocol(
            "this Confluence source has no authentication method configured; choose a personal \
             access token or user + password"
                .to_owned(),
        ));
    };
    match auth {
        // Confluence DC >= 7.9: personal access tokens are Bearer tokens.
        AuthMethod::Pat => Ok(Scheme::Bearer),
        AuthMethod::UserPassword => username.map(Scheme::Basic).ok_or_else(|| {
            SourceError::protocol(
                "user + password authentication needs a username in the source configuration"
                    .to_owned(),
            )
        }),
        // Declared in neither `descriptor_template().auth_methods` nor
        // reachable from the Add-source form; refused rather than guessed at.
        other => Err(SourceError::protocol(format!(
            "{other:?} authentication is not supported for Confluence Data Center; use a \
             personal access token or user + password"
        ))),
    }
}

/// One Confluence instance's HTTP access.
///
/// A thin wrapper and nothing more: it exists so the rest of the crate names
/// Confluence paths and Confluence parameters, and so the error-envelope
/// reading happens in exactly one place.
#[derive(Debug)]
pub(crate) struct ConfluenceHttp {
    client: HttpClient,
}

impl ConfluenceHttp {
    /// Build the client for one configured source.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if the base URL is not an http(s) URL or the
    /// client cannot be built -- both are configuration failures, and both
    /// surface when the source is saved rather than mid-sync.
    pub(crate) fn new(
        base_url: &str,
        cfg: &crate::ConfluenceConfig,
        auth: Auth,
    ) -> Result<Self, SourceError> {
        let client = HttpClient::new(HttpConfig {
            base_url: base_url.trim().to_owned(),
            adapter_kind: crate::ADAPTER_KIND.to_owned(),
            adapter_version: crate::ADAPTER_VERSION.to_owned(),
            auth,
            requests_per_second: cfg.rate_per_sec,
            burst: cfg.rate_burst,
            connect_timeout: std::time::Duration::from_secs(cfg.connect_timeout_secs),
            request_timeout: std::time::Duration::from_secs(cfg.request_timeout_secs),
            body_message: Some(error_envelope),
        })?;
        Ok(Self { client })
    }

    /// `GET <base>/<path>?<query>`, decoded as `T`.
    ///
    /// The query values are owned because every one of them is computed (a
    /// rendered CQL string, a limit, a start offset); borrowing would only
    /// move the temporaries to the call sites.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to, with Confluence's own words in
    /// the message where it sent any (see [`error_envelope`]).
    pub(crate) async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, SourceError> {
        let request = self.client.request(Method::GET, path).query(query);
        let response = self.client.send(request).await?;
        response.json::<T>().await.map_err(|error| {
            SourceError::protocol(format!(
                "{path} did not answer with the shape the Confluence REST v1 API documents: \
                 {error}"
            ))
        })
    }

    /// `GET <base><path_and_query>` -- a link **Confluence itself built**.
    ///
    /// Used for `_links.next` and for nothing else. The query is not
    /// re-composed here on purpose: the continuation carries the server's own
    /// `cql`, `expand`, `limit` and `start`, and an adapter that rebuilt them
    /// would be paging a query subtly different from the one it started.
    ///
    /// # Errors
    ///
    /// As [`Self::get_json`], plus [`SourceError::Protocol`] for a link that
    /// is not a path rooted at the instance -- an absolute URL here would name
    /// a host this client was not built for, and appending it to the base URL
    /// would produce a nonsense request rather than a refusal.
    pub(crate) async fn get_link<T: serde::de::DeserializeOwned>(
        &self,
        path_and_query: &str,
    ) -> Result<T, SourceError> {
        if !path_and_query.starts_with('/') {
            return Err(SourceError::protocol(format!(
                "Confluence answered with a continuation link this adapter cannot follow: \
                 {path_and_query:?} is not a path rooted at the instance. Refusing rather than \
                 reporting a walk that stopped early as a completed one."
            )));
        }
        let request = self.client.request(Method::GET, path_and_query);
        let response = self.client.send(request).await?;
        response.json::<T>().await.map_err(|error| {
            SourceError::protocol(format!(
                "the continuation link {path_and_query} did not answer with the shape the \
                 Confluence REST v1 API documents: {error}"
            ))
        })
    }

    /// `POST <base>/<path>` with `body` as JSON, keeping the response.
    ///
    /// The response is handed back rather than decoded here because the
    /// creates want the id Confluence assigned and the update wants nothing at
    /// all -- a helper that insisted on a shape would have to invent one for
    /// the second (issue #286).
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to, with Confluence's own words in
    /// the message where it sent any (see [`error_envelope`]).
    pub(crate) async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<knobas_http::Response, SourceError> {
        let request = self.client.request(Method::POST, path).json(body);
        self.client.send(request).await
    }

    /// `PUT <base>/<path>` with `body` as JSON, keeping the response.
    ///
    /// A separate verb rather than a parameter on [`Self::post_json`] because
    /// the two are not interchangeable here: Confluence's content `PUT`
    /// **replaces** the record and is the one request in this adapter that can
    /// destroy something. A call site reads which of the two it is.
    ///
    /// # Errors
    ///
    /// As [`Self::post_json`]. In particular the 409 a version conflict
    /// answers arrives as [`SourceError::Protocol`] carrying Confluence's own
    /// sentence, which is what makes it a refusal rather than a retry.
    pub(crate) async fn put_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<knobas_http::Response, SourceError> {
        let request = self.client.request(Method::PUT, path).json(body);
        self.client.send(request).await
    }
}

/// The sentence inside Confluence's error envelope, for `knobas-http` to build
/// the message out of ([`knobas_http::BodyMessage`], ADR-0004).
///
/// A failing Confluence answers `{"statusCode":400,"message":"…","reason":"…"}`,
/// which is correct, bounded and unreadable on screen. The commonest sync
/// failure by far is a CQL the server will not parse, whose 400 carries the
/// exact sentence the user needs; showing them raw JSON instead is the
/// difference between a fixable error and a support question.
///
/// `None` for anything that is not the envelope -- an SSO proxy's HTML login
/// page, a gateway's plain text -- which keeps the raw excerpt `knobas-http`
/// would have built. So the worst case is the message it already produced.
fn error_envelope(body: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Envelope {
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        reason: Option<String>,
    }

    let envelope = serde_json::from_str::<Envelope>(body.trim()).ok()?;
    let parts: Vec<String> = [envelope.message, envelope.reason]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .collect();
    // A well-formed envelope that says nothing is less useful than the raw
    // body it came in: keep what we were given.
    (!parts.is_empty()).then(|| parts.join(" -- "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::AuthMethod;

    const SECRET: &str = "NjE2NTM3-super-secret-pat";

    fn a_credential() -> Auth {
        credential(Some(AuthMethod::Pat), None, Some(SECRET)).expect("a PAT is enough")
    }

    /// Confluence DC personal access tokens are Bearer tokens. The header
    /// *spelling* is `knobas-http`'s business and is pinned there; what this
    /// crate owns is picking the right variant for the configured method.
    #[test]
    fn a_pat_becomes_a_bearer_credential() {
        assert!(
            matches!(a_credential(), Auth::Bearer(token) if token == SECRET),
            "a PAT must map to Bearer"
        );
    }

    #[test]
    fn user_and_password_become_a_basic_credential() {
        let got = credential(
            Some(AuthMethod::UserPassword),
            Some("mara"),
            Some("hunter2"),
        )
        .unwrap();
        assert!(
            matches!(got, Auth::Basic { username, password }
                     if username == "mara" && password == "hunter2"),
            "user+password must map to Basic with the pair in that order"
        );
    }

    #[test]
    fn basic_auth_without_a_username_is_a_configuration_error() {
        let e = credential(Some(AuthMethod::UserPassword), None, Some(SECRET)).unwrap_err();
        assert!(
            matches!(&e, SourceError::Protocol { message: m, .. } if m.contains("username")),
            "{e:?}"
        );
    }

    /// A configured source with no secret in the keychain (§3
    /// "missing_secret") must read as unauthorized, not as a protocol bug:
    /// that is what makes the sources view offer *Re-enter*.
    #[test]
    fn a_missing_secret_is_unauthorized() {
        assert!(matches!(
            credential(Some(AuthMethod::Pat), None, None),
            Err(SourceError::Unauthorized { .. })
        ));
    }

    /// A source that is *both* misconfigured and missing its secret must
    /// report the configuration problem: retyping a token cannot fix a source
    /// with no auth method chosen, and `Unauthorized` would send the user
    /// round that loop forever.
    #[test]
    fn a_configuration_problem_outranks_a_missing_secret() {
        for (auth, username) in [
            (None, None),
            (Some(AuthMethod::OAuth), Some("mara")),
            (Some(AuthMethod::UserPassword), None),
        ] {
            let e = credential(auth, username, None).unwrap_err();
            assert!(
                matches!(&e, SourceError::Protocol { .. }),
                "{auth:?}/{username:?} with no secret should name the configuration: {e:?}"
            );
        }
        assert!(matches!(
            credential(Some(AuthMethod::UserPassword), Some("mara"), None),
            Err(SourceError::Unauthorized { .. })
        ));
    }

    #[test]
    fn unsupported_auth_methods_are_refused_by_name() {
        for method in [AuthMethod::OAuth, AuthMethod::ApiToken] {
            let e = credential(Some(method), Some("mara"), Some(SECRET)).unwrap_err();
            assert!(matches!(e, SourceError::Protocol { .. }), "{method:?}");
        }
    }

    /// Spec §14: the secret is never displayed. `Debug` is the leak that gets
    /// forgotten -- a `dbg!` on the client would put a PAT in a bug report.
    #[test]
    fn debug_never_prints_the_secret() {
        let shown = format!("{:?}", a_credential());
        assert!(!shown.contains(SECRET), "{shown}");

        let http = ConfluenceHttp::new(
            "https://wiki.tidewater.example",
            &crate::ConfluenceConfig::default(),
            a_credential(),
        )
        .unwrap();
        let shown = format!("{http:?}");
        assert!(!shown.contains(SECRET), "{shown}");
        assert!(shown.contains("wiki.tidewater.example"), "{shown}");
    }

    /// Confluence's error envelope, read out of the failing body and handed
    /// back as the sentence the message is built from.
    ///
    /// Asserted through `knobas_http::status_error` -- the function that
    /// actually consults the hook -- rather than on the hook alone, so the
    /// `HTTP <status>: ` prefix this crate does *not* own is pinned where it
    /// comes from.
    #[test]
    fn the_confluence_error_envelope_becomes_the_message() {
        let error = knobas_http::status_error(
            knobas_http::StatusCode::BAD_REQUEST,
            r#"{"statusCode":400,"message":"Could not parse cql : type = pages"}"#,
            Some(error_envelope),
        );
        assert!(
            matches!(&error, SourceError::Protocol { message, .. }
                     if message == "HTTP 400 Bad Request: Could not parse cql : type = pages"),
            "{error:?}"
        );
        // Both halves, in the order Confluence sends them.
        assert_eq!(
            error_envelope(r#"{"statusCode":404,"message":"No content","reason":"Not Found"}"#),
            Some("No content -- Not Found".to_owned())
        );
    }

    /// An instance behind an SSO proxy answers with an HTML login page.
    /// Reading nothing out of it is what keeps `knobas-http`'s bounded excerpt
    /// of the real body on screen.
    #[test]
    fn a_body_that_is_not_confluences_envelope_is_read_as_nothing() {
        assert_eq!(
            error_envelope(&format!("<html>{}</html>", "x".repeat(4000))),
            None
        );
        assert_eq!(error_envelope(""), None);
        assert_eq!(error_envelope("Service Unavailable"), None);
        assert_eq!(error_envelope(r#"{"statusCode":500}"#), None);
        assert_eq!(error_envelope(r#"{"message":"   "}"#), None);
    }

    /// Contract §4.1: 401 *and* 403 are `Unauthorized`, and no reading of a
    /// body may change that -- it is the one fault the user is asked to act
    /// on. ADR-0004 makes them tell-apart-able by status without collapsing
    /// that.
    #[test]
    fn a_refusal_keeps_its_class_whatever_the_body_says() {
        for status in [
            knobas_http::StatusCode::UNAUTHORIZED,
            knobas_http::StatusCode::FORBIDDEN,
        ] {
            let error = knobas_http::status_error(
                status,
                r#"{"statusCode":401,"message":"No space with key ENG"}"#,
                Some(error_envelope),
            );
            assert!(
                matches!(error, SourceError::Unauthorized { .. }),
                "{status}: {error:?}"
            );
            assert_eq!(error.status(), Some(status.as_u16()), "{status}");
        }
    }

    /// Contract §4.1: connect failures are `Unreachable`, which is what makes
    /// the sources view say "can't reach it" instead of "re-enter password".
    /// End to end through this crate's own seam, so a mis-wired client is
    /// caught here rather than by a real instance.
    #[tokio::test]
    async fn a_refused_connection_is_unreachable() {
        let http = ConfluenceHttp::new(
            &dead_url(),
            &crate::ConfluenceConfig::default(),
            a_credential(),
        )
        .unwrap();
        let got = http
            .get_json::<serde_json::Value>("rest/api/user/current", &[])
            .await
            .unwrap_err();
        assert!(matches!(got, SourceError::Unreachable(_)), "{got:?}");
    }

    /// A continuation link that is not a path is refused rather than pasted
    /// onto the base URL. A walk that stopped early must never be reported as
    /// a completed one: the `page` kind claims `full_sync_exhaustive`, so the
    /// engine would tombstone everything past the break.
    #[tokio::test]
    async fn a_continuation_link_that_is_not_a_path_is_refused_by_name() {
        let http = ConfluenceHttp::new(
            &dead_url(),
            &crate::ConfluenceConfig::default(),
            a_credential(),
        )
        .unwrap();
        let e = http
            .get_link::<serde_json::Value>("https://elsewhere.example/rest/api/content/search")
            .await
            .unwrap_err();
        assert!(
            matches!(&e, SourceError::Protocol { message: m, status: None }
                     if m.contains("elsewhere.example")),
            "{e:?}"
        );
    }

    /// A base URL the user mistyped must fail when the source is saved, not
    /// mid-sync, and it must name the string it could not read.
    #[test]
    fn a_base_url_that_is_not_a_url_is_refused() {
        let e = ConfluenceHttp::new(
            "wiki.example.com",
            &crate::ConfluenceConfig::default(),
            a_credential(),
        )
        .unwrap_err();
        assert!(
            matches!(&e, SourceError::Protocol { message: m, .. } if m.contains("wiki.example.com")),
            "{e:?}"
        );
    }

    /// A port nothing listens on: bound to learn the number, then dropped.
    fn dead_url() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        format!("http://127.0.0.1:{port}")
    }
}
