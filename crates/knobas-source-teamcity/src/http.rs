//! The only module in this crate that names [`knobas_http`] or `reqwest`.
//!
//! The transport is the shared one and that crate changes only through the
//! orchestrator (interfaces §8 P8, §10.8): rustls with the platform's root
//! store, the retry budget, `Retry-After`, the per-instance rate limiter, the
//! `User-Agent`, and the one fault mapping every adapter must agree on (401
//! **and** 403 → [`SourceError::Unauthorized`]; connect/DNS/TLS/timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`]),
//! each carrying the status it came from ([`SourceError::status`], ADR-0004).
//! [`knobas_http::HttpClient::send`] is the only way onto the wire -- the
//! [`knobas_http::Request`] its builder hands back deliberately has no `send`
//! of its own.
//!
//! Three things are TeamCity's and therefore here: which [`Auth`] variant the
//! configured [`AuthMethod`] means, the client's timeouts and rate limit, and
//! reading TeamCity's own error text out of a failing body ([`error_message`],
//! a [`knobas_http::BodyMessage`] per ADR-0004).
//!
//! `Accept: application/json` is **not** here, because it is not this
//! adapter's to remember: [`knobas_http::HttpClient::new`] sets it as a
//! default header on every request it builds. That is what makes "always" a
//! structural property rather than a habit -- and `knobas-mockd` answers a
//! TeamCity request without it with 406 plus a recorded violation (deviation
//! 1), so `tests/mockd.rs` is where the guarantee is actually witnessed.
//!
//! Confining every mention of `knobas-http` to this one file is on purpose: a
//! change requested from the orchestrator then costs one file to apply, not a
//! sweep across the crate.

use std::time::Duration;

use knobas_http::{Auth, HttpClient, HttpConfig};
use knobas_source::{AuthMethod, SourceError};

/// Interfaces §4.1: 10 s to connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Interfaces §4.1: 30 s for a whole request.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The [`Auth`] the configured method means.
///
/// # Errors
///
/// [`SourceError::Unauthorized`] when the keychain holds no secret -- the
/// `missing_secret` state of interfaces §3, which the sources view offers
/// *Re-enter* for. [`SourceError::Protocol`] for a configuration that cannot
/// authenticate at all: no method chosen, a method this adapter does not
/// speak, or user + password with no username beside it.
pub(crate) fn credential(
    auth: Option<AuthMethod>,
    username: Option<&str>,
    secret: Option<&str>,
) -> Result<Auth, SourceError> {
    // The configuration is judged **before** the secret, and the order is the
    // point. These are two different states with two different remedies:
    // `missing_secret` sends the user to *Re-enter*, a misconfigured source
    // sends them to the source's settings. Checking the secret first would
    // report `Unauthorized` for a source that could not authenticate even
    // with a secret in hand, and send the user to retype a token that was
    // never the problem.
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
        // credential at all (P6). TeamCity is not one of them, and neither
        // method the descriptor declares can be guessed at from a bare secret.
        return Err(SourceError::protocol(
            "this TeamCity source has no authentication method configured; choose an access \
             token or user + password"
                .to_owned(),
        ));
    };
    match auth {
        // A TeamCity access token (2019.1+) is a Bearer token.
        AuthMethod::Pat => Ok(Scheme::Bearer),
        AuthMethod::UserPassword => username
            .map(str::trim)
            .filter(|u| !u.is_empty())
            .map(Scheme::Basic)
            .ok_or_else(|| {
                SourceError::protocol(
                    "user + password authentication needs a username in the source configuration"
                        .to_owned(),
                )
            }),
        // Declared in neither `descriptor_template().auth_methods` nor
        // reachable from the Add-source form; refused rather than guessed at.
        other => Err(SourceError::protocol(format!(
            "{other:?} authentication is not supported for TeamCity; use an access token or \
             user + password"
        ))),
    }
}

/// The shared client for one instance.
///
/// # Errors
///
/// [`SourceError::Protocol`] if the base URL is not an http(s) URL or the TLS
/// backend cannot be built -- both are configuration failures, and both must
/// surface when the source is saved rather than mid-sync.
pub(crate) fn client(
    base_url: &str,
    cfg: &crate::TeamCityConfig,
    auth: Auth,
) -> Result<HttpClient, SourceError> {
    HttpClient::new(HttpConfig {
        base_url: base_url.trim().to_owned(),
        adapter_kind: crate::ADAPTER_KIND.to_owned(),
        adapter_version: crate::ADAPTER_VERSION.to_owned(),
        auth,
        requests_per_second: cfg.rate_limit_per_sec,
        burst: cfg.burst(),
        connect_timeout: CONNECT_TIMEOUT,
        request_timeout: REQUEST_TIMEOUT,
        body_message: Some(error_message),
    })
}

/// The sentence inside TeamCity's error text, for `knobas-http` to build the
/// message out of ([`knobas_http::BodyMessage`], ADR-0004).
///
/// TeamCity answers errors as `text/plain` even to a client that asked for
/// JSON, in the shape
///
/// ```text
/// Error has occurred during request processing (Not Found).
/// Error: jetbrains.buildServer.server.rest.errors.NotFoundException: No project found by name or internal/external id 'tidewatr'.
/// ```
///
/// The first line restates the status `knobas-http` has already put in the
/// message, and the fully-qualified Java class name in front of the second is
/// noise to everyone who is not reading TeamCity's source. What is left is the
/// sentence that says which project was not found.
///
/// `None` for anything that is not that shape -- an HTML error page from a
/// reverse proxy, an empty body -- which keeps the raw excerpt.
fn error_message(body: &str) -> Option<String> {
    let detail = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        // The first line is the status, which the message already carries.
        .find(|line| !line.starts_with("Error has occurred during request processing"))?;
    // `Error: <fully.qualified.Exception>: <sentence>` -- keep the sentence.
    let detail = detail
        .strip_prefix("Error: ")
        .and_then(|rest| {
            let (class, sentence) = rest.split_once(": ")?;
            // Only when it really is a class name, so a plain `Error: nope`
            // keeps its text instead of being split on the first colon.
            (class.contains('.') && !class.contains(' ')).then_some(sentence.trim())
        })
        .unwrap_or(detail);
    (!detail.is_empty()).then(|| detail.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A TeamCity access token is a Bearer token; user + password is Basic.
    /// The two spellings are the one thing no adapter can be trusted to get
    /// right by inspection, so they are asserted per variant.
    #[test]
    fn each_auth_method_maps_to_its_own_scheme() {
        let bearer = credential(Some(AuthMethod::Pat), None, Some("tok")).expect("pat");
        assert!(
            matches!(&bearer, Auth::Bearer(t) if t == "tok"),
            "{bearer:?}"
        );
        let basic = credential(Some(AuthMethod::UserPassword), Some("mara"), Some("pw"))
            .expect("user+password");
        assert!(
            matches!(&basic, Auth::Basic { username, password } if username == "mara" && password == "pw"),
            "{basic:?}"
        );
    }

    /// Missing secret and missing username are two different mistakes with two
    /// different remedies, and only one of them is the user's credential.
    #[test]
    fn construction_refuses_credentials_it_cannot_use() {
        assert!(matches!(
            credential(Some(AuthMethod::Pat), None, None),
            Err(SourceError::Unauthorized { .. })
        ));
        let err =
            credential(Some(AuthMethod::UserPassword), None, Some("pw")).expect_err("no username");
        assert!(
            matches!(&err, SourceError::Protocol { message: m, .. } if m.contains("username")),
            "{err:?}"
        );
        // A blank username is the same mistake with whitespace in it.
        let err = credential(Some(AuthMethod::UserPassword), Some("  "), Some("pw"))
            .expect_err("blank username");
        assert!(
            matches!(&err, SourceError::Protocol { message: m, .. } if m.contains("username")),
            "{err:?}"
        );
        // No method at all: a configuration problem, not a credential one.
        let err = credential(None, None, Some("tok")).expect_err("no method");
        assert!(matches!(err, SourceError::Protocol { .. }), "{err:?}");
        // A method the descriptor does not declare.
        for other in [AuthMethod::OAuth, AuthMethod::ApiToken] {
            let err = credential(Some(other), None, Some("tok")).expect_err("undeclared");
            assert!(
                matches!(err, SourceError::Protocol { .. }),
                "{other:?}: {err:?}"
            );
        }
    }

    /// The configuration is judged before the secret: a source that could not
    /// authenticate even with a secret must not send the user to *Re-enter*.
    #[test]
    fn a_misconfigured_source_is_not_reported_as_a_missing_secret() {
        let err =
            credential(Some(AuthMethod::UserPassword), None, None).expect_err("both are wrong");
        assert!(
            matches!(&err, SourceError::Protocol { message: m, .. } if m.contains("username")),
            "the actionable half is the configuration, not the credential: {err:?}"
        );
    }

    /// A TeamCity behind a path prefix is the normal deployment; a base URL
    /// that loses the prefix 404s every request.
    #[test]
    fn the_base_url_may_carry_a_path_prefix_with_or_without_a_trailing_slash() {
        let cfg = crate::TeamCityConfig::default();
        for base in [
            "https://ci.example.com/teamcity",
            "https://ci.example.com/teamcity/",
            " https://ci.example.com/teamcity ",
        ] {
            let c = client(base, &cfg, Auth::Bearer("tok".to_owned())).expect("client builds");
            assert_eq!(
                c.url_for("app/rest/server"),
                "https://ci.example.com/teamcity/app/rest/server",
                "base {base:?}"
            );
        }
        let c = client("https://ci.example.com", &cfg, Auth::Bearer("t".to_owned())).expect("c");
        assert_eq!(
            c.url_for("app/rest/server"),
            "https://ci.example.com/app/rest/server"
        );
    }

    #[test]
    fn a_base_url_that_is_not_a_url_is_refused_at_build_time() {
        let cfg = crate::TeamCityConfig::default();
        for bad in ["not a url", "ci.example.com", "ftp://ci.example.com"] {
            let err = client(bad, &cfg, Auth::Bearer("tok".to_owned()))
                .err()
                .unwrap_or_else(|| panic!("{bad:?} must be refused"));
            assert!(
                matches!(err, SourceError::Protocol { .. }),
                "{bad:?}: {err:?}"
            );
        }
    }

    /// TeamCity's error text, read where the body still exists (ADR-0004).
    ///
    /// Asserted through `knobas_http::status_error` -- the function that
    /// actually consults the hook -- so the `HTTP <status>: ` prefix this crate
    /// does not own is pinned where it comes from. The body is the shape
    /// `knobas-mockd`'s `tc_error` serves, which is the shape a real TeamCity
    /// serves.
    #[test]
    fn teamcitys_error_text_becomes_the_message() {
        let error = knobas_http::status_error(
            knobas_http::StatusCode::NOT_FOUND,
            "Error has occurred during request processing (Not Found).\nError: \
             jetbrains.buildServer.server.rest.errors.NotFoundException: No project found by \
             name or internal/external id 'tidewatr'.\n",
            Some(error_message),
        );
        assert!(
            matches!(&error, SourceError::Protocol { message, .. }
                     if message == "HTTP 404 Not Found: No project found by name or \
                                    internal/external id 'tidewatr'."),
            "the status line and the Java class name are noise the message already \
             carries or nobody can use: {error:?}"
        );

        // The first line alone -- what `tc_error` serves for a fault with no
        // detail -- says only what the status already said, so there is nothing
        // to lift and the raw excerpt is kept.
        assert_eq!(
            error_message("Error has occurred during request processing (400).\n"),
            None
        );
        // A detail line that is not `Error: <class>: <sentence>` is kept whole
        // rather than split on its first colon.
        assert_eq!(
            error_message("Error has occurred during request processing (400).\nError: nope: 1\n"),
            Some("Error: nope: 1".to_owned())
        );
        assert_eq!(
            error_message("Error has occurred during request processing (400).\nlocator is bad\n"),
            Some("locator is bad".to_owned())
        );
        // And nothing at all is read out of an empty body or a proxy's HTML.
        assert_eq!(error_message(""), None);
        assert_eq!(error_message("   \n \n"), None);
        assert_eq!(
            error_message("<html><body>502</body></html>"),
            Some("<html><body>502</body></html>".to_owned())
        );
    }

    /// Interfaces §4.1: 401 *and* 403 are `Unauthorized`, whatever the body
    /// says -- and ADR-0004 makes them tell-apart-able by the status they
    /// carry without collapsing that.
    #[test]
    fn a_refusal_keeps_its_class_whatever_the_body_says() {
        for status in [
            knobas_http::StatusCode::UNAUTHORIZED,
            knobas_http::StatusCode::FORBIDDEN,
        ] {
            let error = knobas_http::status_error(
                status,
                "Error has occurred during request processing (401).\nAuthentication required\n",
                Some(error_message),
            );
            assert!(
                matches!(error, SourceError::Unauthorized { .. }),
                "{status}: {error:?}"
            );
            assert_eq!(error.status(), Some(status.as_u16()), "{status}");
        }
    }

    /// A secret that reaches a log is a secret that leaks, and `Debug` is how
    /// it would get there.
    #[test]
    fn debug_never_prints_the_secret() {
        let auth =
            credential(Some(AuthMethod::Pat), None, Some("hunter2-the-real-token")).expect("pat");
        assert!(!format!("{auth:?}").contains("hunter2"), "{auth:?}");
        let c = client(
            "https://ci.example.com",
            &crate::TeamCityConfig::default(),
            credential(
                Some(AuthMethod::UserPassword),
                Some("mara"),
                Some("hunter2-the-real-token"),
            )
            .expect("basic"),
        )
        .expect("client builds");
        let printed = format!("{c:?}");
        assert!(!printed.contains("hunter2"), "{printed}");
        assert!(
            printed.contains("ci.example.com"),
            "everything else stays readable: {printed}"
        );
    }
}
