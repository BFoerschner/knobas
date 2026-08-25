//! The only module in this crate that speaks HTTP.
//!
//! Everything else is pure: the mapping, the cursor and the JQL renderer all
//! work on values, and the sync run (task 5) is written against a trait it can
//! be faked through. That split is deliberate -- the interesting logic is
//! paging and cursor discipline, and neither needs a socket to be tested.
//!
//! # What lives here and what does not
//!
//! The transport is [`knobas_http`]'s, and that crate is **read-only for M1**
//! (interfaces §8 P8): rustls with the platform's root store, the retry budget,
//! `Retry-After`, the per-instance rate limiter, the `User-Agent`, and the one
//! fault mapping every adapter must agree on (401 **and** 403 →
//! [`SourceError::Unauthorized`]; connect/DNS/TLS/timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`]).
//! `knobas_http::HttpClient::send` is the only way onto the wire -- the
//! [`knobas_http::Request`] its builder hands back deliberately has no `send`
//! of its own.
//!
//! What is Jira's and therefore here: which [`knobas_http::Auth`] variant the
//! configured [`AuthMethod`] means, and lifting Jira's `errorMessages`
//! envelope out of an error body so the sources view shows a sentence rather
//! than a JSON document.
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
    let secret = secret.ok_or(SourceError::Unauthorized)?;
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
        return Err(SourceError::Protocol(
            "this Jira source has no authentication method configured; choose a personal access \
             token or user + password"
                .to_owned(),
        ));
    };
    match auth {
        // Jira DC >= 8.14: personal access tokens are Bearer tokens.
        AuthMethod::Pat => Ok(Scheme::Bearer),
        AuthMethod::UserPassword => username.map(Scheme::Basic).ok_or_else(|| {
            SourceError::Protocol(
                "user + password authentication needs a username in the source configuration"
                    .to_owned(),
            )
        }),
        // Declared in neither `descriptor_template().auth_methods` nor
        // reachable from the Add-source form; refused rather than guessed at.
        other => Err(SourceError::Protocol(format!(
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
    /// message where it sent any (see [`humanize`]).
    pub(crate) async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, SourceError> {
        let request = self.client.request(Method::GET, path).query(query);
        let response = self.client.send(request).await.map_err(humanize)?;
        response.json::<T>().await.map_err(|error| {
            SourceError::Protocol(format!(
                "{path} did not answer with the shape the Jira REST v2 contract documents: {error}"
            ))
        })
    }
}

/// Replace an embedded Jira error envelope with the sentence inside it.
///
/// `knobas-http` builds a protocol message as `HTTP <status>: <body excerpt>`,
/// and for Jira that body is `{"errorMessages":[…],"errors":{…}}` -- correct,
/// bounded, and unreadable on screen. The commonest sync failure by far is a
/// mistyped `jql_filter`, whose 400 carries the exact sentence the user needs;
/// showing them raw JSON instead is the difference between a fixable error and
/// a support question.
///
/// Deliberately a rewrite of the *message* and not of the response: the body is
/// consumed inside `HttpClient::send`, which is the only door onto the wire.
/// Cleaner would be a body → message hook in `knobas-http`; that crate is
/// read-only for M1, so this is the local half and the hook is worth
/// requesting later. It degrades to the identity for anything it cannot parse,
/// including an envelope the excerpt truncated, and it never changes the
/// variant -- so the worst case is the message `knobas-http` already produced.
fn humanize(error: SourceError) -> SourceError {
    match error {
        SourceError::Protocol(message) => SourceError::Protocol(jira_message(&message)),
        // `Unauthorized` carries nothing, and `Unreachable`/`Sink` never carry
        // a response body. Rewriting either would only risk reclassifying the
        // one fault the user is asked to act on.
        other => other,
    }
}

/// The message with any embedded Jira error envelope replaced by its contents.
///
/// Everything before the envelope (`HTTP 400: `) is kept, so the status stays
/// on screen; anything that is not the envelope is returned unchanged.
fn jira_message(message: &str) -> String {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope {
        #[serde(default)]
        error_messages: Vec<String>,
        #[serde(default)]
        errors: serde_json::Map<String, serde_json::Value>,
    }

    let Some(start) = message.find('{') else {
        return message.to_owned();
    };
    let Ok(envelope) = serde_json::from_str::<Envelope>(&message[start..]) else {
        return message.to_owned();
    };
    let mut parts = envelope.error_messages;
    parts.extend(envelope.errors.iter().map(|(field, detail)| {
        let detail = detail
            .as_str()
            .map_or_else(|| detail.to_string(), str::to_owned);
        format!("{field}: {detail}")
    }));
    if parts.is_empty() {
        // A well-formed envelope that says nothing is less useful than the raw
        // body it came in: keep what we were given.
        return message.to_owned();
    }
    format!("{}{}", &message[..start], parts.join("; "))
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
            matches!(&e, SourceError::Protocol(m) if m.contains("username")),
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
            Err(SourceError::Unauthorized)
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
            matches!(&e, SourceError::Protocol(m)
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
                matches!(&e, SourceError::Protocol(_)),
                "{auth:?}/{username:?} with no secret should name the configuration: {e:?}"
            );
        }
        // But a *well*-configured source with no secret is exactly
        // `missing_secret`, and still reads as unauthorized.
        assert!(matches!(
            credential(Some(AuthMethod::UserPassword), Some("mara"), None),
            Err(SourceError::Unauthorized)
        ));
    }

    #[test]
    fn unsupported_auth_methods_are_refused_by_name() {
        for method in [AuthMethod::OAuth, AuthMethod::ApiToken] {
            let e = credential(Some(method), Some("mara"), Some(SECRET)).unwrap_err();
            assert!(matches!(e, SourceError::Protocol(_)), "{method:?}");
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

    /// Jira's error envelope, exactly as the WADL documents it, lifted out of
    /// the `HTTP <status>: <body>` message `knobas-http` builds.
    #[test]
    fn the_jira_error_envelope_becomes_the_message() {
        let got = humanize(SourceError::Protocol(
            r#"HTTP 400 Bad Request: {"errorMessages":["Error in the JQL Query: 'nope' is an unknown field"],"errors":{}}"#
                .to_owned(),
        ));
        let SourceError::Protocol(message) = got else {
            panic!("a 400 stays a protocol error");
        };
        assert_eq!(
            message,
            "HTTP 400 Bad Request: Error in the JQL Query: 'nope' is an unknown field"
        );

        let got = humanize(SourceError::Protocol(
            r#"HTTP 400: {"errorMessages":[],"errors":{"jql":"Unable to parse the query"}}"#
                .to_owned(),
        ));
        let SourceError::Protocol(message) = got else {
            panic!("a 400 stays a protocol error");
        };
        assert_eq!(message, "HTTP 400: jql: Unable to parse the query");
    }

    /// A DC instance behind an SSO proxy answers with an HTML login page, and
    /// a 4 MB one at that. `knobas-http` already excerpts it; lifting the Jira
    /// envelope must not put any of it back.
    #[test]
    fn a_body_that_is_not_jiras_envelope_is_left_exactly_as_it_arrived() {
        let excerpt = format!("HTTP 401: <html>{}…", "x".repeat(400));
        let got = humanize(SourceError::Protocol(excerpt.clone()));
        let SourceError::Protocol(message) = got else {
            panic!("still a protocol error");
        };
        assert_eq!(message, excerpt);

        // An envelope that the excerpt cut in half is not valid JSON, so it
        // falls through untouched rather than being half-parsed.
        let cut = r#"HTTP 400: {"errorMessages":["Error in the JQL Que…"#;
        let got = humanize(SourceError::Protocol(cut.to_owned()));
        assert!(
            matches!(&got, SourceError::Protocol(m) if m == cut),
            "{got:?}"
        );
    }

    /// Interfaces §4.1: 401 *and* 403 are `Unauthorized`, and that survives
    /// this crate's message rewriting -- `Unauthorized` is the one fault the
    /// user is asked to act on, and it carries no body to prettify.
    #[test]
    fn humanizing_never_reclassifies_a_fault() {
        assert!(matches!(
            humanize(SourceError::Unauthorized),
            SourceError::Unauthorized
        ));
        assert!(matches!(
            humanize(SourceError::Unreachable("refused".to_owned())),
            SourceError::Unreachable(m) if m == "refused"
        ));
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
            matches!(&e, SourceError::Protocol(m) if m.contains("jira.example.com")),
            "{e:?}"
        );
    }
}
