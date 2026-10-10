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
//! 1), so `tests/it/mockd.rs` is where the guarantee is actually witnessed.
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

/// The sentence inside TeamCity's error body, for `knobas-http` to build the
/// message out of ([`knobas_http::BodyMessage`], ADR-0004).
///
/// **Two shapes, tried in that order**, because TeamCity has served two and
/// only one of them reaches this adapter.
///
/// # The JSON envelope: what this adapter actually receives
///
/// ```json
/// {"errors":[{"message":"No build found by id '6520690000'.",
///             "additionalMessage":"jetbrains.buildServer.server.rest.errors.NotFoundException: No build found by id '6520690000'.",
///             "statusText":"Responding with error, status code: 404 (Not Found).",
///             "stackTrace":null}]}
/// ```
///
/// [`knobas_http::HttpClient::new`] sets `Accept: application/json` on every
/// request it builds and no adapter can talk it out of that, so this is the
/// shape every error this adapter will ever see arrives in. Measured read-only
/// against JetBrains' public instance (2026.2 EAP, build 238763) on
/// 2026-08-29, on a 400 and a 404, and pinned live by
/// `a_rest_error_is_a_json_envelope_the_adapter_can_read`.
///
/// `message` and nothing else: `additionalMessage` repeats the sentence behind
/// a fully-qualified Java class name and `statusText` restates the status the
/// message already carries. An envelope naming more than one fault keeps all
/// of them, joined -- dropping the rest would hide the half a user needs.
///
/// # The plain-text form: kept, and this is the decision
///
/// ```text
/// Error has occurred during request processing (Not Found).
/// Error: jetbrains.buildServer.server.rest.errors.NotFoundException: No project found by name or internal/external id 'tidewatr'.
/// ```
///
/// This is what `error_message` used to require, on a doc comment asserting it
/// was "the shape a real TeamCity serves". It is not the shape 2026.2 serves
/// anyone. That server content-negotiates, measured read-only on 2026-08-29
/// (2026.2 EAP, build 238763) over `/app/rest/server`:
///
/// | `Accept` | answer |
/// |---|---|
/// | `application/json` -- what `knobas-http` sends | JSON; errors are the envelope above |
/// | `application/xml`, `*/*`, or no `Accept` at all | **XML** |
/// | one the server cannot satisfy (`text/plain`, `text/html`) | **406**, whose body is itself the JSON envelope |
///
/// So the plain-text form below reaches nobody, and requiring it made this
/// function return `None` on every real error -- issue #113. Note the middle
/// row: `*/*` is what reqwest sends by default, which is why
/// `knobas-mockd`'s deviation 1 refuses it, and why "any `Accept` other than
/// JSON gets a 406" would be the wrong reading of the same measurement.
///
/// It is kept anyway, deliberately, as a fallback **after** the JSON attempt:
///
/// * It cannot mis-fire. The branch only runs on a body whose first line is
///   literally that announcement, so no JSON body and no proxy page can reach
///   it, and the JSON attempt has already had its turn.
/// * The hook sees every failing body this client is handed, not only TeamCity's
///   own -- a gateway, an older server, or a non-`/app/rest` path is not a case
///   this adapter can enumerate, and the code to read the form is already here
///   and already tested.
/// * Deleting working code on one server's behaviour is the move this whole
///   suite exists to argue against. One server surprised us into this issue;
///   that is a reason to accept both forms, not to bet the other way.
///
/// # Neither shape
///
/// `None`, which keeps `knobas-http`'s bounded excerpt of the raw body -- an
/// HTML error page from a reverse proxy, an empty body, a JSON object shaped
/// like nothing above. A body this function did not understand is still the
/// most informative thing knobas has, and lifting a line out of one that never
/// claimed to be TeamCity's would put `<html>` on screen in its place.
///
/// What it does *not* do is check who sent the envelope: any body shaped
/// `{"errors":[{"message": …}]}` is read, whether a TeamCity wrote it or a
/// gateway in front of one did. That is the intent -- see the second bullet
/// above -- and not an accident to tighten later.
fn error_message(body: &str) -> Option<String> {
    json_error_message(body).or_else(|| plaintext_error_message(body))
}

/// `errors[].message`, joined, or `None` if the body is not that envelope.
fn json_error_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let sentences: Vec<&str> = value
        .get("errors")?
        .as_array()?
        .iter()
        .filter_map(|error| error.get("message")?.as_str())
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .collect();
    // An `errors` array with nothing readable in it is not a message this
    // function found -- it is one it failed to find, and the excerpt of the
    // whole body says more than an empty string would.
    (!sentences.is_empty()).then(|| sentences.join("; "))
}

/// The sentence out of the older plain-text form, or `None` for a body that
/// does not announce itself as one. See [`error_message`] for why this is
/// still here.
fn plaintext_error_message(body: &str) -> Option<String> {
    let mut lines = body.lines().map(str::trim).filter(|line| !line.is_empty());
    // The first line is the status, which the message already carries -- and it
    // is what identifies the body as TeamCity's in the first place.
    if !lines
        .next()?
        .starts_with("Error has occurred during request processing")
    {
        return None;
    }
    let detail = lines.next()?;
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

    /// The error body a real TeamCity serves a client that asked for JSON --
    /// which is every request this adapter makes.
    ///
    /// Verbatim from JetBrains' public instance (2026.2 EAP, build 238763),
    /// read-only on 2026-08-29. `knobas-http` sets `Accept: application/json`
    /// on every request it builds and an adapter cannot talk it out of that,
    /// so this is the *only* error shape this adapter will ever be handed.
    /// `error_message` used to require a plain-text first line instead and so
    /// returned `None` on every one of them, putting a JSON blob on screen
    /// where a sentence was meant (issue #113).
    ///
    /// `additionalMessage` repeats the sentence behind a fully-qualified Java
    /// class name and `statusText` restates the status the message already
    /// carries; both are noise, for the same reason the plain-text reading
    /// drops its own first line and its own class name.
    fn live_404() -> &'static str {
        r#"{"errors":[{"additionalMessage":"jetbrains.buildServer.server.rest.errors.NotFoundException: No build found by id '999999999'.","statusText":"Responding with error, status code: 404 (Not Found).","stackTrace":null,"message":"No build found by id '999999999'."}]}"#
    }

    /// Issue #113: the shape the server actually sends becomes the sentence.
    ///
    /// Asserted through `knobas_http::status_error` -- the function that
    /// consults the hook -- so the `HTTP <status>: ` prefix this crate does not
    /// own is pinned where it comes from.
    #[test]
    fn teamcitys_json_error_envelope_becomes_the_message() {
        let error = knobas_http::status_error(
            knobas_http::StatusCode::NOT_FOUND,
            live_404(),
            Some(error_message),
        );
        assert!(
            matches!(&error, SourceError::Protocol { message, .. }
                     if message == "HTTP 404 Not Found: No build found by id '999999999'."),
            "the sentence, not the envelope around it: {error:?}"
        );

        // The 400 half, and the one a user is most likely to see: a locator
        // this adapter would never send, refused by name.
        let locator = knobas_http::status_error(
            knobas_http::StatusCode::BAD_REQUEST,
            r#"{"errors":[{"message":"Error processing locator 'order:(id:desc)': Locator dimension [order] is unknown.","statusText":"Responding with error, status code: 400 (Bad Request)."}]}"#,
            Some(error_message),
        );
        assert!(
            matches!(&locator, SourceError::Protocol { message, .. }
                     if message.contains("Locator dimension [order] is unknown")
                        && !message.contains("statusText")),
            "{locator:?}"
        );

        // An envelope naming more than one fault keeps all of them: dropping
        // the rest would hide the half a user needs.
        assert_eq!(
            error_message(r#"{"errors":[{"message":"first thing"},{"message":"second thing"}]}"#),
            Some("first thing; second thing".to_owned())
        );

        // A body that is JSON but not *this* envelope keeps the raw excerpt --
        // a proxy's own error object says more as itself than as nothing.
        for foreign in [
            r#"{"error":"upstream connect error"}"#,
            r#"{"errors":[]}"#,
            r#"{"errors":[{"statusText":"no sentence here"}]}"#,
            r#"{"errors":"not a list"}"#,
            "[1,2,3]",
        ] {
            assert_eq!(error_message(foreign), None, "{foreign}");
        }
    }

    /// The plain-text form, which is **kept as a fallback** and is no longer
    /// claimed to be what a real TeamCity serves.
    ///
    /// It was, once, and the doc comment on this test used to say it was the
    /// shape `knobas-mockd`'s `tc_error` serves "which is the shape a real
    /// TeamCity serves". Both halves were wrong at once: 2026.2 serves the
    /// JSON envelope to a JSON-accepting client and XML to one that sends no
    /// `Accept` at all, and mockd was teaching the shape that hid it (issue
    /// #113). mockd serves the envelope now.
    ///
    /// The reading stays because it cannot mis-fire -- it runs only on a body
    /// whose first line is literally TeamCity's announcement, after the JSON
    /// attempt has had its turn -- and because the hook sees every failing
    /// body this client is handed, gateways and older servers included. See
    /// [`error_message`].
    ///
    /// Asserted through `knobas_http::status_error` -- the function that
    /// actually consults the hook -- so the `HTTP <status>: ` prefix this crate
    /// does not own is pinned where it comes from.
    #[test]
    fn the_plaintext_error_form_is_still_read_where_one_arrives() {
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
        // The HTML cases are the reason the opening line is required: without
        // that, a multi-line error page reads as its own first line -- `<html>`
        // -- which is strictly less than the raw excerpt it would replace.
        assert_eq!(error_message(""), None);
        assert_eq!(error_message("   \n \n"), None);
        assert_eq!(error_message("<html><body>502</body></html>"), None);
        assert_eq!(
            error_message("<html>\n<head><title>502 Bad Gateway</title></head>\n</html>"),
            None
        );
    }

    /// Interfaces §4.1: 401 *and* 403 are `Unauthorized`, whatever the body
    /// says -- and ADR-0004 makes them tell-apart-able by the status they
    /// carry without collapsing that.
    ///
    /// **The property issue #113 must not break.** Reading the body is a
    /// legibility change and nothing else: the fault *class* comes off the
    /// status, so retryability, the `Unauthorized` mapping behind *Re-enter
    /// password* and ADR-0004's carried status are all decided before
    /// [`error_message`] is consulted -- and for `Unauthorized` it is not
    /// consulted at all, because that fault carries no message.
    ///
    /// So every body below -- the envelope, one that is half-written, one that
    /// is not JSON, and none at all -- produces the same class and the same
    /// status as the others of its code. A reading that could change either
    /// would be one a malformed body could talk out of retrying.
    #[test]
    fn a_refusal_keeps_its_class_whatever_the_body_says() {
        let bodies = [
            // The shape a real TeamCity serves.
            r#"{"errors":[{"message":"Authentication required"}]}"#,
            // Truncated mid-envelope, as a dropped connection leaves it.
            r#"{"errors":[{"message":"Authenti"#,
            // The plain-text form, and a proxy's page, and nothing at all.
            "Error has occurred during request processing (401).\nAuthentication required\n",
            "<html><body>401</body></html>",
            "",
        ];
        for status in [
            knobas_http::StatusCode::UNAUTHORIZED,
            knobas_http::StatusCode::FORBIDDEN,
        ] {
            for body in bodies {
                let error = knobas_http::status_error(status, body, Some(error_message));
                assert!(
                    matches!(error, SourceError::Unauthorized { .. }),
                    "{status} with body {body:?}: {error:?}"
                );
                assert_eq!(error.status(), Some(status.as_u16()), "{status}: {body:?}");
            }
        }

        // ...and the same on the message-carrying side, which is the half a
        // body reading could actually reach. The status is what
        // `retry::status_is_transient` and `client::presence` read, and it is
        // the same whatever came back in the body -- including a body that is
        // JSON but not an envelope, and one that is no shape at all.
        for status in [
            knobas_http::StatusCode::NOT_FOUND,
            knobas_http::StatusCode::BAD_REQUEST,
            knobas_http::StatusCode::TOO_MANY_REQUESTS,
            knobas_http::StatusCode::INTERNAL_SERVER_ERROR,
            knobas_http::StatusCode::SERVICE_UNAVAILABLE,
        ] {
            for body in bodies.into_iter().chain([r#"{"errors":[]}"#, "\u{0}\u{1}"]) {
                let error = knobas_http::status_error(status, body, Some(error_message));
                assert_eq!(
                    error.status(),
                    Some(status.as_u16()),
                    "{status} with body {body:?}: {error:?}"
                );
                assert!(
                    matches!(error, SourceError::Protocol { .. }),
                    "{status} with body {body:?}: {error:?}"
                );
            }
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
