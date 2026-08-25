//! The only module in this crate that names [`knobas_http`] or `reqwest`.
//!
//! The transport is the shared one and that crate is **read-only for M1**
//! (interfaces §8 P8): rustls with the platform's root store, the retry
//! budget, `Retry-After`, the per-instance rate limiter, the `User-Agent`, and
//! the one fault mapping every adapter must agree on (401 **and** 403 →
//! [`SourceError::Unauthorized`]; connect/DNS/TLS/timeout →
//! [`SourceError::Unreachable`]; everything else →
//! [`SourceError::Protocol`]). [`knobas_http::HttpClient::send`] is the only
//! way onto the wire -- the [`knobas_http::Request`] its builder hands back
//! deliberately has no `send` of its own.
//!
//! Two things are TeamCity's and therefore here: which [`Auth`] variant the
//! configured [`AuthMethod`] means, and the client's timeouts and rate limit.
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
        // credential at all (P6). TeamCity is not one of them, and neither
        // method the descriptor declares can be guessed at from a bare secret.
        return Err(SourceError::Protocol(
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
                SourceError::Protocol(
                    "user + password authentication needs a username in the source configuration"
                        .to_owned(),
                )
            }),
        // Declared in neither `descriptor_template().auth_methods` nor
        // reachable from the Add-source form; refused rather than guessed at.
        other => Err(SourceError::Protocol(format!(
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
    })
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
            Err(SourceError::Unauthorized)
        ));
        let err =
            credential(Some(AuthMethod::UserPassword), None, Some("pw")).expect_err("no username");
        assert!(
            matches!(&err, SourceError::Protocol(m) if m.contains("username")),
            "{err:?}"
        );
        // A blank username is the same mistake with whitespace in it.
        let err = credential(Some(AuthMethod::UserPassword), Some("  "), Some("pw"))
            .expect_err("blank username");
        assert!(
            matches!(&err, SourceError::Protocol(m) if m.contains("username")),
            "{err:?}"
        );
        // No method at all: a configuration problem, not a credential one.
        let err = credential(None, None, Some("tok")).expect_err("no method");
        assert!(matches!(err, SourceError::Protocol(_)), "{err:?}");
        // A method the descriptor does not declare.
        for other in [AuthMethod::OAuth, AuthMethod::ApiToken] {
            let err = credential(Some(other), None, Some("tok")).expect_err("undeclared");
            assert!(
                matches!(err, SourceError::Protocol(_)),
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
            matches!(&err, SourceError::Protocol(m) if m.contains("username")),
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
            assert!(matches!(err, SourceError::Protocol(_)), "{bad:?}: {err:?}");
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
