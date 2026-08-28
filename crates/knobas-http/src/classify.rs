//! Turning transport and status failures into the three faults knobas shows.
//!
//! One mapping, shared by every adapter (interfaces §4.1): 401 **and** 403 →
//! [`SourceError::Unauthorized`]; connect, DNS, TLS and timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`].
//! The distinction is user-visible -- `Unauthorized` is what makes the sources
//! view offer *Re-enter password* -- so three adapters classifying a 403 three
//! different ways is three different behaviours for one problem.
//!
//! **The status the source answered with rides on the error** (ADR-0004). The
//! fault class above is the *user's* axis and is unchanged; an adapter's is
//! finer, and used to have no home. 401 and 403 arrived as one indistinguishable
//! `Unauthorized`, so Gitea re-ran an identity probe to guess which it had, and
//! a 404 or a 409 was recovered by parsing the message this module had just
//! built. [`SourceError::status`] is now the one way to ask.
//!
//! **The caller maps the failing body into the message** ([`BodyMessage`]).
//! Every source knobas reads answers a failure with its own error envelope --
//! Jira's `{"errorMessages":[…]}`, Gitea's `{"message":…}`, TeamCity's plain
//! text -- and the body is consumed inside [`crate::HttpClient::send`], the only
//! door onto the wire, so an adapter could not reach it. Jira used to
//! reconstruct the sentence out of the finished message afterwards. The hook is
//! that same rewrite done where the body still exists, and it is bounded like
//! everything else here: what it returns is excerpted too.

use std::time::Duration;

use knobas_source::SourceError;
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, RETRY_AFTER};

/// How much of an error body reaches a message.
pub const BODY_EXCERPT: usize = 400;

/// Longest `Retry-After` knobas obeys. Beyond it the run fails and the
/// scheduler's backoff takes over -- a source asking us to wait an hour is
/// telling us to come back on the next schedule, not to hold a connection.
pub const RETRY_AFTER_CAP: Duration = Duration::from_secs(60);

/// How a caller turns a failing response body into the message the error
/// carries.
///
/// Returning `None` keeps the bounded excerpt of the raw body -- which is what
/// a hook must do for anything it does not recognise, because a body it could
/// not parse is still the most informative thing knobas has. Whatever it does
/// return is excerpted in turn, so a hook cannot widen [`BODY_EXCERPT`].
///
/// A plain `fn` and not a boxed closure: this is a pure reading of one string,
/// fixed per adapter and known at compile time, and a closure would invite one
/// that captures the client it is configured on.
pub type BodyMessage = fn(body: &str) -> Option<String>;

/// The fault a non-success status means.
///
/// `body_message` is the caller's [`BodyMessage`], or `None` to keep the raw
/// excerpt. It is consulted only for the message-carrying fault:
/// [`SourceError::Unauthorized`] has no message, deliberately -- it is the one
/// fault the user is asked to act on, and *Re-enter password* is the whole of
/// what it has to say.
#[must_use]
pub fn status_error(
    status: StatusCode,
    body: &str,
    body_message: Option<BodyMessage>,
) -> SourceError {
    match status.as_u16() {
        // 403 with 401: a Jira DC 403 after repeated failures is the CAPTCHA
        // lockout (`X-Authentication-Denied-Reason`), which a human must clear.
        // One fault class, two statuses -- and the status is carried, because
        // "this credential is dead" and "this credential may not read *that*"
        // are the same sentence to the user and opposite instructions to an
        // adapter.
        code @ (401 | 403) => SourceError::Unauthorized { status: Some(code) },
        code => SourceError::Protocol {
            status: Some(code),
            message: format!("HTTP {status}: {}", message_of(body, body_message)),
        },
    }
}

/// The body as the message should carry it: the caller's reading of it where
/// there was one, the raw excerpt otherwise, bounded either way.
fn message_of(body: &str, body_message: Option<BodyMessage>) -> String {
    match body_message.and_then(|read| read(body)) {
        Some(lifted) => excerpt(&lifted),
        None => excerpt(body),
    }
}

/// The fault a `reqwest` failure means.
#[must_use]
pub fn reqwest_error(error: &reqwest::Error) -> SourceError {
    if error.is_timeout() || error.is_connect() {
        // DNS and TLS failures arrive as connect errors too, which is exactly
        // the class §4.1 puts here.
        SourceError::Unreachable(error.to_string())
    } else if let Some(status) = error.status() {
        // No body to read, so no hook to consult: `reqwest` raised this
        // *instead of* handing back a response.
        status_error(status, "", None)
    } else {
        SourceError::protocol(error.to_string())
    }
}

/// How long the server asked us to wait, capped, and only in the
/// `delay-seconds` form.
///
/// The HTTP-date form is deliberately not parsed: it is rare, its timezone
/// handling is a trap, and a misparse would sleep for hours. Falling through
/// to the exponential backoff is the safe reading of an unreadable header.
#[must_use]
pub fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    let seconds: u64 = headers
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(Duration::from_secs(seconds).min(RETRY_AFTER_CAP))
}

fn excerpt(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.len() <= BODY_EXCERPT {
        return trimmed.to_owned();
    }
    // Char boundary, not byte: an error body is arbitrary UTF-8.
    let end = trimmed
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= BODY_EXCERPT)
        .last()
        .unwrap_or(0);
    format!("{}…", &trimmed[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mapping every adapter must agree on (interfaces §4.1). 403 is
    /// `Unauthorized` and not `Protocol` on purpose: a Jira DC 403 after
    /// repeated failures is the CAPTCHA lockout
    /// (`X-Authentication-Denied-Reason`), and the user must act.
    #[test]
    fn statuses_map_to_the_fault_the_user_sees() {
        use reqwest::StatusCode;
        assert!(matches!(
            status_error(StatusCode::UNAUTHORIZED, "", None),
            SourceError::Unauthorized { .. }
        ));
        assert!(matches!(
            status_error(StatusCode::FORBIDDEN, "", None),
            SourceError::Unauthorized { .. }
        ));
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_GATEWAY,
        ] {
            assert!(
                matches!(
                    status_error(status, "body", None),
                    SourceError::Protocol { .. }
                ),
                "{status}"
            );
        }
    }

    /// ADR-0004: one fault class, and the status that says *which* refusal it
    /// was. 401 is a credential that is gone and must end a run; 403 is one
    /// object this credential may not read, and the run survives it. They
    /// arrived here as the same value, which is the whole reason Gitea had to
    /// re-run an identity probe to guess between them.
    #[test]
    fn a_refusal_carries_the_status_that_says_which_refusal_it_was() {
        use reqwest::StatusCode;
        assert_eq!(
            status_error(StatusCode::UNAUTHORIZED, "", None).status(),
            Some(401)
        );
        assert_eq!(
            status_error(StatusCode::FORBIDDEN, "", None).status(),
            Some(403)
        );
        // Every other status is readable too, without parsing a message back
        // apart: 404 is "gone", 409 is Gitea's empty repository.
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::CONFLICT,
            StatusCode::BAD_REQUEST,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            assert_eq!(
                status_error(status, "body", None).status(),
                Some(status.as_u16()),
                "{status}"
            );
        }
        // A fault that never came from a response claims no status: a DNS blip
        // read as a 404 would be swallowed as one skipped repository.
        assert_eq!(SourceError::Unreachable("dns".to_owned()).status(), None);
    }

    /// The body → message hook: the caller reads its own error envelope where
    /// the body still exists, instead of reconstructing the sentence out of the
    /// finished message afterwards.
    #[test]
    fn the_caller_maps_a_failing_body_into_the_message() {
        use reqwest::StatusCode;
        // Gitea's envelope; Jira and TeamCity each have one of their own.
        fn lift(body: &str) -> Option<String> {
            let value: serde_json::Value = serde_json::from_str(body).ok()?;
            Some(value.get("message")?.as_str()?.to_owned())
        }

        let error = status_error(
            StatusCode::NOT_FOUND,
            r#"{"message":"user redirect does not exist [name: tidewater]","url":"x"}"#,
            Some(lift),
        );
        let SourceError::Protocol { message, status } = &error else {
            panic!("a 404 is a protocol fault: {error:?}");
        };
        assert_eq!(
            message,
            "HTTP 404 Not Found: user redirect does not exist [name: tidewater]"
        );
        // The hook maps the message and nothing else: the status it came from
        // is still the status it came from.
        assert_eq!(*status, Some(404));

        // A body the hook cannot read keeps the raw excerpt -- a body it did
        // not understand is still the most informative thing knobas has.
        let error = status_error(StatusCode::NOT_FOUND, "<html>login</html>", Some(lift));
        assert!(
            matches!(&error, SourceError::Protocol { message, .. }
                     if message == "HTTP 404 Not Found: <html>login</html>"),
            "{error:?}"
        );

        // A hook cannot widen the excerpt: an SSO proxy's 4 MB login page,
        // lifted whole into a "message", is what BODY_EXCERPT exists for.
        fn firehose(_: &str) -> Option<String> {
            Some("x".repeat(10_000))
        }
        let error = status_error(StatusCode::BAD_GATEWAY, "small", Some(firehose));
        let SourceError::Protocol { message, .. } = &error else {
            panic!("a 502 is a protocol fault: {error:?}");
        };
        assert!(message.len() < BODY_EXCERPT + 100, "{}", message.len());

        // And it never reaches the one fault the user is asked to act on:
        // `Unauthorized` carries no message, so there is nothing for a hook to
        // rewrite and no way for one to turn a refusal into something else.
        fn never_called(_: &str) -> Option<String> {
            panic!("Unauthorized carries no message; the hook must not be consulted");
        }
        for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
            assert!(
                matches!(
                    status_error(status, r#"{"message":"nope"}"#, Some(never_called)),
                    SourceError::Unauthorized { .. }
                ),
                "{status}"
            );
        }
    }

    /// An error message goes in a log line and on screen; a 4 MB HTML error
    /// page does not.
    #[test]
    fn a_body_excerpt_is_bounded() {
        let long = "x".repeat(10_000);
        let SourceError::Protocol { message, .. } =
            status_error(reqwest::StatusCode::NOT_FOUND, &long, None)
        else {
            panic!("404 is a protocol error");
        };
        assert!(message.len() < BODY_EXCERPT + 100, "{}", message.len());
    }

    /// The excerpt is cut on a character boundary: an error body is arbitrary
    /// UTF-8, and slicing a multi-byte character in half panics.
    #[test]
    fn a_body_excerpt_survives_multibyte_text() {
        let long = "ä".repeat(10_000);
        let SourceError::Protocol { message, .. } =
            status_error(reqwest::StatusCode::NOT_FOUND, &long, None)
        else {
            panic!("404 is a protocol error");
        };
        assert!(message.contains('ä'), "{message}");
    }

    #[test]
    fn retry_after_is_read_in_its_delay_seconds_form() {
        use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("7"));
        assert_eq!(
            parse_retry_after(&headers),
            Some(std::time::Duration::from_secs(7))
        );

        // The HTTP-date form is not parsed: the exponential backoff covers it,
        // and a wrong date parse would sleep for hours.
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT"),
        );
        assert_eq!(parse_retry_after(&headers), None);

        // A server asking for a week is not obeyed.
        headers.insert(RETRY_AFTER, HeaderValue::from_static("604800"));
        assert_eq!(parse_retry_after(&headers), Some(RETRY_AFTER_CAP));
    }

    /// No header at all is the common case and must not be an error.
    #[test]
    fn no_retry_after_header_is_no_delay() {
        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }
}
