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
//! What is Jira's and therefore here: the [`CredentialRules`] the transport's
//! [`credential`] maps the configured [`AuthMethod`] under, and reading Jira's `errorMessages`
//! envelope out of a failing response body so the sources view shows a sentence
//! rather than a JSON document. That reading is handed to `knobas-http` as a
//! [`knobas_http::BodyMessage`] (ADR-0004) and runs where the body still
//! exists; it used to be a rewrite of the finished message afterwards, which
//! could only ever recover what the excerpt had already kept.
//!
//! Confining every mention of `knobas-http` to this one file is on purpose: a
//! change requested from the orchestrator then costs one file to apply, not a
//! sweep across the crate.

use knobas_http::{Auth, CredentialRules, HttpClient, HttpConfig, Method, TokenSpelling};
use knobas_source::{AuthMethod, SourceError};

pub(crate) use knobas_http::credential;

/// Which [`Auth`] each configured method means for Jira Data Center, and what
/// a source that cannot authenticate reads as. The mapping itself, and the
/// order it judges in, are [`credential`]'s.
pub(crate) const CREDENTIAL_RULES: CredentialRules = CredentialRules {
    // Jira DC >= 8.14: personal access tokens are Bearer tokens.
    pat: TokenSpelling::Bearer,
    // What `descriptor_template().auth_methods` declares and the Add-source
    // form offers; anything else is refused rather than guessed at.
    methods: &[AuthMethod::Pat, AuthMethod::UserPassword],
    unconfigured: "this Jira source has no authentication method configured; choose a personal \
                   access token or user + password",
    unsupported: |method| {
        format!(
            "{method:?} authentication is not supported for Jira Data Center; use a personal \
             access token or user + password"
        )
    },
};

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
            forbidden: None,
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
        let body = response.text().await.map_err(|error| {
            SourceError::protocol(format!("{path}: the answer's body broke off: {error}"))
        })?;
        // The body's start rides in the message: "did not answer with the
        // shape" says nothing a person can act on without it (#877).
        serde_json::from_str::<T>(&body).map_err(|error| {
            let start: String = body.chars().take(300).collect();
            SourceError::protocol(format!(
                "{path} did not answer with the shape the Jira REST v2 contract documents: \
                 {error}; it began {start:?}"
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
    /// **Sent once, never retried by the transport** (#1122). A comment, a
    /// create and a worklog are each done again if they arrive twice, and a
    /// POST that timed out may have landed. A transition is mostly refused
    /// the second time (the issue has moved and no longer offers it), but not
    /// on a workflow whose transition loops back to its own status, so it
    /// goes once too. A failure comes back as the write's error, and the
    /// write queue decides whether to send it again.
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
        let request = self
            .client
            .request(Method::POST, path)
            .json(body)
            .no_retry();
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
        credential(&CREDENTIAL_RULES, Some(AuthMethod::Pat), None, Some(SECRET))
            .expect("a PAT is enough")
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
