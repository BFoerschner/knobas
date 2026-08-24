//! Turning transport and status failures into the three faults knobas shows.
//!
//! One mapping, shared by every adapter (interfaces §4.1): 401 **and** 403 →
//! [`SourceError::Unauthorized`]; connect, DNS, TLS and timeout →
//! [`SourceError::Unreachable`]; everything else → [`SourceError::Protocol`].
//! The distinction is user-visible -- `Unauthorized` is what makes the sources
//! view offer *Re-enter password* -- so three adapters classifying a 403 three
//! different ways is three different behaviours for one problem.

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

/// The fault a non-success status means.
#[must_use]
pub fn status_error(status: StatusCode, body: &str) -> SourceError {
    match status.as_u16() {
        // 403 with 401: a Jira DC 403 after repeated failures is the CAPTCHA
        // lockout (`X-Authentication-Denied-Reason`), which a human must clear.
        401 | 403 => SourceError::Unauthorized,
        _ => SourceError::Protocol(format!("HTTP {status}: {}", excerpt(body))),
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
        status_error(status, "")
    } else {
        SourceError::Protocol(error.to_string())
    }
}

/// The fault a middleware-wrapped failure means.
#[must_use]
pub fn transport_error(error: &reqwest_middleware::Error) -> SourceError {
    match error {
        reqwest_middleware::Error::Reqwest(inner) => reqwest_error(inner),
        // The only middleware in this stack is the retrier, and what it fails
        // with is a transport failure it gave up on.
        reqwest_middleware::Error::Middleware(inner) => SourceError::Unreachable(inner.to_string()),
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
            status_error(StatusCode::UNAUTHORIZED, ""),
            SourceError::Unauthorized
        ));
        assert!(matches!(
            status_error(StatusCode::FORBIDDEN, ""),
            SourceError::Unauthorized
        ));
        for status in [
            StatusCode::NOT_FOUND,
            StatusCode::BAD_REQUEST,
            StatusCode::BAD_GATEWAY,
        ] {
            assert!(
                matches!(status_error(status, "body"), SourceError::Protocol(_)),
                "{status}"
            );
        }
    }

    /// An error message goes in a log line and on screen; a 4 MB HTML error
    /// page does not.
    #[test]
    fn a_body_excerpt_is_bounded() {
        let long = "x".repeat(10_000);
        let SourceError::Protocol(message) = status_error(reqwest::StatusCode::NOT_FOUND, &long)
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
        let SourceError::Protocol(message) = status_error(reqwest::StatusCode::NOT_FOUND, &long)
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
