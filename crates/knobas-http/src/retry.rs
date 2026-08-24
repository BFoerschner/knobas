//! Which failures are worth trying again.
//!
//! Roadmap §4: three attempts, exponential, **429/502/503/504 and connect
//! errors only**. Not 500 (a deterministic server bug repeated three times is
//! three times the load and the same answer), not 4xx (the request is wrong),
//! and never a write -- M1 issues none.

use reqwest_middleware::Error;
use reqwest_retry::{Retryable, RetryableStrategy};

/// The classifier the retry middleware runs on every attempt.
pub struct TransientOnly;

impl RetryableStrategy for TransientOnly {
    fn handle(&self, result: &Result<reqwest::Response, Error>) -> Option<Retryable> {
        match result {
            Ok(response) => match response.status().as_u16() {
                429 | 502 | 503 | 504 => Some(Retryable::Transient),
                _ => None,
            },
            Err(Error::Reqwest(error)) if error.is_timeout() || error.is_connect() => {
                Some(Retryable::Transient)
            }
            Err(_) => Some(Retryable::Fatal),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16) -> Result<reqwest::Response, Error> {
        let response = http::Response::builder()
            .status(status)
            .body("")
            .expect("a status and an empty body are always a valid response");
        Ok(reqwest::Response::from(response))
    }

    /// The statuses that mean "the server is momentarily busy or behind a
    /// proxy that is": a second attempt plausibly gets a different answer.
    #[test]
    fn a_busy_server_is_tried_again() {
        for status in [429, 502, 503, 504] {
            assert!(
                matches!(
                    TransientOnly.handle(&response(status)),
                    Some(Retryable::Transient)
                ),
                "{status}"
            );
        }
    }

    /// 500 is deliberately **not** retried: a deterministic server-side bug
    /// answered three times is three times the load and the same answer. Nor
    /// is any 4xx -- the request itself is what is wrong -- and a 2xx is not a
    /// failure at all.
    #[test]
    fn a_broken_request_or_a_broken_server_is_not_tried_again() {
        for status in [200, 201, 304, 400, 401, 403, 404, 409, 422, 500, 501] {
            assert!(
                TransientOnly.handle(&response(status)).is_none(),
                "{status}"
            );
        }
    }
}
