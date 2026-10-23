//! Which failures are worth trying again, and how long to wait.
//!
//! Roadmap §4: three attempts, exponential, **429/502/503/504, connect errors
//! and timeouts only**. Not 500 (a deterministic server bug repeated three
//! times is three times the load and the same answer), and not 4xx (the
//! request is wrong). One more, for reads only: a `GET` or `HEAD` the server
//! closed its connection on before answering is sent once more
//! ([`closed_before_answering`], #1818). Writes retry too, since M2's
//! write-backs (issue #43): [`Request::json`](crate::Request::json) buffers
//! its body, so a retry
//! re-sends the same bytes. A write that must not arrive twice opts out with
//! [`Request::no_retry`](crate::Request::no_retry) (issue #1122).
//!
//! These are plain predicates rather than a `reqwest_retry` policy on purpose.
//! The retry loop lives in [`HttpClient::send`](crate::HttpClient::send)
//! because two of this crate's guarantees can only be kept from there:
//!
//! * **every attempt passes the rate limiter.** A middleware retrying inside
//!   one `send` is invisible to a limiter awaited outside it, so a source
//!   answering 503 would get three unthrottled requests in a burst -- from the
//!   component whose job is to not do that.
//! * **`Retry-After` is obeyed *before* retrying.** A retry policy never sees
//!   response headers, so a middleware retry is always the blind exponential
//!   one; honouring the header afterwards would mean the server's instruction
//!   arrives only after its advice has already been ignored twice.

use std::time::Duration;

/// The first retry's delay; each subsequent one doubles it.
pub const BACKOFF_BASE: Duration = Duration::from_millis(250);

/// Statuses that mean "momentarily busy", where a second attempt plausibly
/// gets a different answer.
#[must_use]
pub const fn status_is_transient(status: u16) -> bool {
    matches!(status, 429 | 502 | 503 | 504)
}

/// Transport failures worth another attempt: a connection that never
/// established, or one that took too long. DNS and TLS arrive as connect
/// errors too.
#[must_use]
pub fn error_is_transient(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

/// A request whose connection the server closed before the answer's status
/// line and headers were complete: hyper's `IncompleteMessage`, at the bottom
/// of reqwest's error chain.
///
/// This is what a pooled keep-alive connection looks like when the server's
/// idle timeout closes it just as the next request goes out on it -- Node's
/// default is 5 s, and Kuma's `live_kuma` suite met it between two syncs. The
/// request was written, so it may have been read: only a method that is safe
/// to repeat may be sent again on it, and [`HttpClient::send`] does that once,
/// for a `GET` or `HEAD` that did not opt out ([`Request::no_retry`]). A body
/// that breaks off after its headers fails the body's read, never `send`, so
/// it is not this.
///
/// [`HttpClient::send`]: crate::HttpClient::send
/// [`Request::no_retry`]: crate::Request::no_retry
#[must_use]
pub fn closed_before_answering(error: &reqwest::Error) -> bool {
    let mut cause = std::error::Error::source(error);
    while let Some(error) = cause {
        if error
            .downcast_ref::<hyper::Error>()
            .is_some_and(hyper::Error::is_incomplete_message)
        {
            return true;
        }
        cause = error.source();
    }
    false
}

/// How long to wait before attempt `attempt + 1`, counting from 1.
#[must_use]
pub fn backoff(attempt: u32) -> Duration {
    BACKOFF_BASE.saturating_mul(1u32 << attempt.min(16).saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The statuses that mean "the server is momentarily busy or behind a
    /// proxy that is".
    #[test]
    fn a_busy_server_is_tried_again() {
        for status in [429, 502, 503, 504] {
            assert!(status_is_transient(status), "{status}");
        }
    }

    /// 500 is deliberately **not** retried: a deterministic server-side bug
    /// answered three times is three times the load and the same answer. Nor
    /// is any 4xx -- the request itself is what is wrong -- and a 2xx is not a
    /// failure at all.
    #[test]
    fn a_broken_request_or_a_broken_server_is_not_tried_again() {
        for status in [200, 201, 304, 400, 401, 403, 404, 409, 422, 500, 501] {
            assert!(!status_is_transient(status), "{status}");
        }
    }

    /// Exponential, and bounded: the shift is clamped so a pathological
    /// attempt count cannot overflow it into a zero delay.
    #[test]
    fn the_delay_doubles_and_never_wraps() {
        assert_eq!(backoff(1), BACKOFF_BASE);
        assert_eq!(backoff(2), BACKOFF_BASE * 2);
        assert_eq!(backoff(3), BACKOFF_BASE * 4);
        assert!(backoff(u32::MAX) >= BACKOFF_BASE);
    }
}
