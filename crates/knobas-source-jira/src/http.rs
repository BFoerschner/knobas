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
//! could only ever recover what the excerpt had already kept. Also Jira's:
//! the contract a body of the wrong shape is measured against ([`SHAPE`]); the
//! excerpt of that body is the transport's (#877, #1313).
//!
//! There is no Jira client type. The rest of the crate holds the transport's
//! [`HttpClient`], re-exported from here, and reaches the wire through
//! [`get_json`] and [`post_json`], which only name the excerpt and the
//! once-sent write. Confining every mention of `knobas-http` to this one file
//! is on purpose: a change requested from the orchestrator then costs one file
//! to apply, not a sweep across the crate.

use knobas_http::{Auth, BodyExcerpt, CredentialRules, HttpConfig, Method, TokenSpelling};
use knobas_source::{AuthMethod, SourceError};

pub(crate) use knobas_http::{HttpClient, credential};

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

/// What a Jira read whose body is not the documented shape says it should have
/// been, with the body's start beside it (#877): "did not answer with the
/// shape" says nothing a person can act on without it. The excerpt is the
/// transport's ([`knobas_http::HttpClient::get_json_with`], #1313).
pub(crate) const SHAPE: BodyExcerpt = BodyExcerpt {
    contract: "the Jira REST v2 contract",
};

/// The client for one configured source.
///
/// # Errors
///
/// [`SourceError::Protocol`] if the base URL is not an http(s) URL or the
/// client cannot be built -- both are configuration failures, and both
/// surface when the source is saved rather than mid-sync.
pub(crate) fn client(
    base_url: &str,
    cfg: &crate::JiraConfig,
    auth: Auth,
) -> Result<HttpClient, SourceError> {
    HttpClient::new(HttpConfig {
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
        max_body: knobas_http::MAX_BODY,
    })
}

/// `GET <base>/<path>?<query>`, decoded as `T`, with [`SHAPE`]'s excerpt.
///
/// `path` is always one of the endpoints the WADL declares, and `query`
/// carries only parameters that endpoint declares -- `knobas-mockd` records
/// anything else as a violation. The values are owned because every one of
/// them is computed (`startAt`, a rendered JQL string, a joined field list).
///
/// # Errors
///
/// The [`SourceError`] the failure maps to, with Jira's own words in the
/// message where it sent any (see [`error_envelope`]).
pub(crate) async fn get_json<T: serde::de::DeserializeOwned>(
    http: &HttpClient,
    path: &str,
    query: &[(&str, String)],
) -> Result<T, SourceError> {
    http.get_json_with(path, query, Some(SHAPE)).await
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
    http: &HttpClient,
    path: &str,
    body: &serde_json::Value,
) -> Result<knobas_http::Response, SourceError> {
    http.send(http.request(Method::POST, path).json(body).no_retry())
        .await
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

    /// Spec §14: the secret is never displayed. `Debug` is the leak that gets
    /// forgotten -- a `dbg!` on the client, or a `#[derive(Debug)]` on a struct
    /// holding it, would put a PAT in a log line or a bug report.
    #[test]
    fn debug_never_prints_the_secret() {
        let shown = format!("{:?}", a_credential());
        assert!(!shown.contains(SECRET), "{shown}");

        let http = client(
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
        let http = client(
            &knobas_mockd::refused_url(),
            &crate::JiraConfig::default(),
            a_credential(),
        )
        .unwrap();
        let got = get_json::<serde_json::Value>(&http, "rest/api/2/serverInfo", &[])
            .await
            .unwrap_err();
        assert!(matches!(got, SourceError::Unreachable(_)), "{got:?}");
    }

    /// #877's message, word for word now that the transport builds it
    /// (#1313): a 200 that is not Jira's JSON -- an SSO proxy's login page --
    /// names the path, the documented contract, the decode error and the
    /// body's first 300 characters.
    #[tokio::test]
    async fn a_body_that_is_not_jiras_shape_names_its_start() {
        let page = format!("<html>{}</html>", "x".repeat(400));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let answer = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{page}",
            page.len()
        );
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for socket in listener.incoming() {
                let Ok(mut socket) = socket else { return };
                let mut buf = [0_u8; 8192];
                let _ = socket.read(&mut buf);
                let _ = socket.write_all(answer.as_bytes());
            }
        });
        let http = client(
            &format!("http://{addr}"),
            &crate::JiraConfig::default(),
            a_credential(),
        )
        .unwrap();

        let got = get_json::<serde_json::Value>(&http, "rest/api/2/serverInfo", &[])
            .await
            .unwrap_err();
        let start = format!("<html>{}", "x".repeat(294));
        assert!(
            matches!(&got, SourceError::Protocol { message, .. }
            if *message == format!(
                "rest/api/2/serverInfo did not answer with the shape the Jira REST v2 \
                 contract documents: expected value at line 1 column 1; it began \
                 {start:?}"
            )),
            "{got:?}"
        );
    }

    /// A base URL the user mistyped must fail when the source is saved, not
    /// mid-sync, and it must name the string it could not read.
    #[test]
    fn a_base_url_that_is_not_a_url_is_refused() {
        let e = client(
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
