//! The only module in this crate that speaks HTTP.
//!
//! Everything else is pure: the mapping, the cursor and the JQL renderer all
//! work on values, and the sync run (task 5) is written against a trait it can
//! be faked through. That split is deliberate -- the interesting logic is
//! paging and cursor discipline, and neither needs a socket to be tested.
//!
//! # What lives here and what does not
//!
//! The transport is [`knobas_http`]'s, and that crate changes only through the
//! orchestrator (interfaces §8 P8, §10.8): rustls with the platform's root
//! store, the retry budget, `Retry-After`, the per-instance rate limiter, the
//! `User-Agent`, and the one fault mapping every adapter must agree on (401
//! **and** 403 → [`SourceError::Unauthorized`]; connect/DNS/TLS/timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`]),
//! each carrying the status it came from ([`SourceError::status`], ADR-0004).
//! `knobas_http::HttpClient::send` is the only way onto the wire -- the
//! [`knobas_http::Request`] its builder hands back deliberately has no `send`
//! of its own.
//!
//! What is Jira's and therefore here: which [`knobas_http::Auth`] variant the
//! configured [`AuthMethod`] means, and reading Jira's `errorMessages`
//! envelope out of a failing response body so the sources view shows a sentence
//! rather than a JSON document. That reading is handed to `knobas-http` as a
//! [`knobas_http::BodyMessage`] (ADR-0004) and runs where the body still
//! exists; it used to be a rewrite of the finished message afterwards, which
//! could only ever recover what the excerpt had already kept.
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
/// `missing_secret` state of interfaces §3, which the sources view offers
/// *Re-enter* for and the scheduler does not back off over.
/// [`SourceError::Protocol`] for a configuration that cannot authenticate at
/// all: no method chosen, a method Jira DC does not speak, or user+password
/// with no username beside it.
pub(crate) fn credential(
    auth: Option<AuthMethod>,
    username: Option<&str>,
    secret: Option<&str>,
) -> Result<Auth, SourceError> {
    // The configuration is judged **before** the secret, and the order is the
    // whole point. These are two different states with two different remedies
    // (§3, §10.2): `missing_secret` sends the user to *Re-enter*, a
    // misconfigured source sends them to the source's settings. A source that
    // could not authenticate even with a secret in hand has the second problem,
    // and reporting `Unauthorized` for it -- which is what checking the secret
    // first does -- sends the user to retype a token that was never the issue.
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
        // credential (P6). Jira DC is not one of them, and neither method the
        // descriptor declares can be guessed at from a bare secret.
        return Err(SourceError::protocol(
            "this Jira source has no authentication method configured; choose a personal access \
             token or user + password"
                .to_owned(),
        ));
    };
    match auth {
        // Jira DC >= 8.14: personal access tokens are Bearer tokens.
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
            "{other:?} authentication is not supported for Jira Data Center; use a personal \
             access token or user + password"
        ))),
    }
}

/// One Jira instance's HTTP access.
///
/// A thin wrapper and nothing more: it exists so that the rest of the crate
/// names Jira paths and Jira parameters, and so the `errorMessages` rewrite
/// happens in exactly one place.
#[derive(Debug)]
pub(crate) struct JiraHttp {
    client: HttpClient,
}

impl JiraHttp {
    /// Build the client for one configured source.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if the base URL is not an http(s) URL or the
    /// client cannot be built -- both are configuration failures, and both
    /// surface when the source is saved rather than mid-sync.
    pub(crate) fn new(
        base_url: &str,
        cfg: &crate::JiraConfig,
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
    /// `path` is always one of the six endpoints the WADL declares for M1, and
    /// `query` carries only parameters that endpoint declares -- `knobas-mockd`
    /// records anything else as a violation.
    ///
    /// The query values are owned because every one of them is computed
    /// (`startAt`, a rendered JQL string, a joined field list); borrowing would
    /// only move the temporaries to the call sites.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to, with Jira's own words in the
    /// message where it sent any (see [`error_envelope`]).
    pub(crate) async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, SourceError> {
        let request = self.client.request(Method::GET, path).query(query);
        let response = self.client.send(request).await?;
        response.json::<T>().await.map_err(|error| {
            SourceError::protocol(format!(
                "{path} did not answer with the shape the Jira REST v2 contract documents: {error}"
            ))
        })
    }

    /// `POST <base>/<path>` with `body` as JSON, keeping the response.
    ///
    /// M2's write-back (issue #43). The response is handed back rather than
    /// decoded here because the three writes answer three different ways: a
    /// transition is **204 with no body**, a comment is 201 with the created
    /// comment, and a create is 201 with `{id, key, self}`. A helper that
    /// insisted on JSON would have to invent a body for the first.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to, with Jira's own words in the
    /// message where it sent any (see [`error_envelope`]).
    pub(crate) async fn post_json(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<knobas_http::Response, SourceError> {
        let request = self.client.request(Method::POST, path).json(body);
        self.client.send(request).await
    }
}

/// The sentence inside Jira's error envelope, for `knobas-http` to build the
/// message out of ([`knobas_http::BodyMessage`], ADR-0004).
///
/// A failing Jira answers `{"errorMessages":[…],"errors":{…}}`, which is
/// correct, bounded and unreadable on screen. The commonest sync failure by far
/// is a mistyped `jql_filter`, whose 400 carries the exact sentence the user
/// needs; showing them raw JSON instead is the difference between a fixable
/// error and a support question.
///
/// `None` for anything that is not the envelope -- an SSO proxy's HTML login
/// page, a gateway's plain text -- which keeps the raw excerpt `knobas-http`
/// would have built. So the worst case is the message it already produced.
///
/// This used to run *after* the fact, on the finished `HTTP <status>: <body
/// excerpt>` message, because `knobas-http` was read-only for M1 and the body
/// was consumed inside its `send`. It could therefore only recover what the
/// excerpt had already kept -- an envelope the 400-character cut had truncated
/// was no longer JSON, and fell through unread. Running here, on the whole
/// body, it does not have that hole.
fn error_envelope(body: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope {
        #[serde(default)]
        error_messages: Vec<String>,
        #[serde(default)]
        errors: serde_json::Map<String, serde_json::Value>,
    }

    let envelope = serde_json::from_str::<Envelope>(body.trim()).ok()?;
    let mut parts = envelope.error_messages;
    parts.extend(envelope.errors.iter().map(|(field, detail)| {
        let detail = detail
            .as_str()
            .map_or_else(|| detail.to_string(), str::to_owned);
        format!("{field}: {detail}")
    }));
    // A well-formed envelope that says nothing is less useful than the raw body
    // it came in: keep what we were given.
    (!parts.is_empty()).then(|| parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_http::Auth;
    use knobas_source::AuthMethod;

    const SECRET: &str = "NjE2NTM3-super-secret-pat";

    fn a_credential() -> Auth {
        credential(Some(AuthMethod::Pat), None, Some(SECRET)).expect("a PAT is enough")
    }

    /// Jira DC >= 8.14 personal access tokens are Bearer tokens. The header
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

    /// A configured source with no secret in the keychain (interfaces §3
    /// "missing_secret") must read as unauthorized, not as a protocol bug:
    /// that is what makes the sources view offer *Re-enter*.
    #[test]
    fn a_missing_secret_is_unauthorized() {
        assert!(matches!(
            credential(Some(AuthMethod::Pat), None, None),
            Err(SourceError::Unauthorized { .. })
        ));
    }

    /// `SourceInstance::auth` is `Option` because a source may need no
    /// credential at all (P6) -- but Jira DC is not such a source, and the
    /// Add-source form only ever offers the two methods the descriptor
    /// declares. Anonymous here is a misconfiguration, not a missing secret.
    #[test]
    fn a_jira_with_no_auth_method_is_refused_rather_than_tried_anonymously() {
        let e = credential(None, None, Some(SECRET)).unwrap_err();
        assert!(
            matches!(&e, SourceError::Protocol { message: m, .. }
                     if m.contains("personal access token") && m.contains("password")),
            "{e:?}"
        );
    }

    /// A source that is *both* misconfigured and missing its secret must
    /// report the configuration problem. `Unauthorized` would put *Re-enter*
    /// in front of the user (§3 `missing_secret`), and retyping a token cannot
    /// fix a source with no auth method chosen -- they would be sent round that
    /// loop forever. The two states have different remedies, so the more
    /// fundamental one wins.
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
        // But a *well*-configured source with no secret is exactly
        // `missing_secret`, and still reads as unauthorized.
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
    /// forgotten -- a `dbg!` on the client, or a `#[derive(Debug)]` on a struct
    /// holding it, would put a PAT in a log line or a bug report.
    #[test]
    fn debug_never_prints_the_secret() {
        let shown = format!("{:?}", a_credential());
        assert!(!shown.contains(SECRET), "{shown}");

        let http = JiraHttp::new(
            "https://jira.tidewater.example",
            &crate::JiraConfig::default(),
            a_credential(),
        )
        .unwrap();
        let shown = format!("{http:?}");
        assert!(!shown.contains(SECRET), "{shown}");
        assert!(shown.contains("jira.tidewater.example"), "{shown}");
    }

    /// Jira's error envelope, exactly as the WADL documents it, read out of the
    /// failing body and handed back as the sentence the message is built from.
    ///
    /// Asserted through `knobas_http::status_error` -- the function that
    /// actually consults the hook -- rather than on the hook alone, so the
    /// `HTTP <status>: ` prefix this crate does *not* own is pinned where it
    /// comes from.
    #[test]
    fn the_jira_error_envelope_becomes_the_message() {
        let error = knobas_http::status_error(
            knobas_http::StatusCode::BAD_REQUEST,
            r#"{"errorMessages":["Error in the JQL Query: 'nope' is an unknown field"],"errors":{}}"#,
            Some(error_envelope),
        );
        assert!(
            matches!(&error, SourceError::Protocol { message, .. }
                     if message == "HTTP 400 Bad Request: Error in the JQL Query: 'nope' is an \
                                    unknown field"),
            "{error:?}"
        );

        // The `errors` map is the other half of the envelope, and the one a
        // bad JQL usually arrives in.
        assert_eq!(
            error_envelope(r#"{"errorMessages":[],"errors":{"jql":"Unable to parse the query"}}"#),
            Some("jql: Unable to parse the query".to_owned())
        );
        // Both halves, in the order Jira documents them.
        assert_eq!(
            error_envelope(r#"{"errorMessages":["first"],"errors":{"jql":"second"}}"#),
            Some("first; jql: second".to_owned())
        );
    }

    /// A DC instance behind an SSO proxy answers with an HTML login page, and a
    /// 4 MB one at that. Reading nothing out of it is what keeps
    /// `knobas-http`'s bounded excerpt of the real body on screen.
    #[test]
    fn a_body_that_is_not_jiras_envelope_is_read_as_nothing() {
        assert_eq!(
            error_envelope(&format!("<html>{}</html>", "x".repeat(4000))),
            None
        );
        assert_eq!(error_envelope(""), None);
        assert_eq!(error_envelope("Service Unavailable"), None);
        // A well-formed envelope that says nothing: the raw body it came in is
        // more use than an empty sentence.
        assert_eq!(error_envelope(r#"{"errorMessages":[],"errors":{}}"#), None);
        // And a body that is JSON but not Jira's envelope reads as nothing
        // rather than as an empty message.
        assert_eq!(error_envelope(r#"{"detail":"nope"}"#), None);
    }

    /// Interfaces §4.1: 401 *and* 403 are `Unauthorized`, and no reading of a
    /// body may change that -- it is the one fault the user is asked to act on.
    /// ADR-0004 makes them tell-apart-able by status without collapsing that.
    #[test]
    fn a_refusal_keeps_its_class_whatever_the_body_says() {
        for status in [
            knobas_http::StatusCode::UNAUTHORIZED,
            knobas_http::StatusCode::FORBIDDEN,
        ] {
            let error = knobas_http::status_error(
                status,
                r#"{"errorMessages":["You do not have permission"],"errors":{}}"#,
                Some(error_envelope),
            );
            assert!(
                matches!(error, SourceError::Unauthorized { .. }),
                "{status}: {error:?}"
            );
            assert_eq!(error.status(), Some(status.as_u16()), "{status}");
        }
    }

    /// Interfaces §4.1: connect failures are `Unreachable`, which is what makes
    /// the sources view say "can't reach it" instead of "re-enter password".
    /// End to end through this crate's own seam, so a mis-wired client is
    /// caught here rather than by a real instance.
    #[tokio::test]
    async fn a_refused_connection_is_unreachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let http = JiraHttp::new(
            &format!("http://127.0.0.1:{port}"),
            &crate::JiraConfig::default(),
            a_credential(),
        )
        .unwrap();
        let got = http
            .get_json::<serde_json::Value>("rest/api/2/serverInfo", &[])
            .await
            .unwrap_err();
        assert!(matches!(got, SourceError::Unreachable(_)), "{got:?}");
    }

    /// A base URL the user mistyped must fail when the source is saved, not
    /// mid-sync, and it must name the string it could not read.
    #[test]
    fn a_base_url_that_is_not_a_url_is_refused() {
        let e = JiraHttp::new(
            "jira.example.com",
            &crate::JiraConfig::default(),
            a_credential(),
        )
        .unwrap_err();
        assert!(
            matches!(&e, SourceError::Protocol { message: m, .. } if m.contains("jira.example.com")),
            "{e:?}"
        );
    }
}
