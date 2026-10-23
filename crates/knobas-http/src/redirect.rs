//! A read that follows only the redirects its caller allows (#1830).
//!
//! Every adapter's `GET` follows a redirect as reqwest does. A caller that
//! has to hold every hop to a rule of its own (an external adapter's release
//! fetch, where each hop must be https) builds its client with
//! [`HttpClient::with_redirect_check`](crate::HttpClient::with_redirect_check):
//! the check is asked about each hop before it is requested, and a hop it
//! refuses is never sent. reqwest's redirect policy belongs to the client, not
//! to a request, so this is a client of its own and not a request option.

use knobas_source::SourceError;
use reqwest::{StatusCode, Url};

/// The check a client's reads ask about each redirect's target.
pub(crate) type Check = std::sync::Arc<dyn Fn(&Url) -> bool + Send + Sync>;

/// A redirect the check refused: its status and where it pointed.
#[derive(Debug)]
struct RefusedHop {
    status: StatusCode,
    url: Url,
}

impl std::fmt::Display for RefusedHop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "HTTP {}: the read was redirected to {}, which this client does not follow",
            self.status, self.url
        )
    }
}

impl std::error::Error for RefusedHop {}

/// reqwest's own policy (at most 10 hops) for every hop `check` allows, and
/// a [`RefusedHop`] error for one it does not.
pub(crate) fn policy(check: Check) -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(move |attempt| {
        if check(attempt.url()) {
            reqwest::redirect::Policy::default().redirect(attempt)
        } else {
            let refused = RefusedHop {
                status: attempt.status(),
                url: attempt.url().clone(),
            };
            attempt.error(refused)
        }
    })
}

/// `Protocol` with the redirect's status, naming the hop, when `error` is a
/// hop a check refused; `None` for any other failure.
pub(crate) fn refused(error: &reqwest::Error) -> Option<SourceError> {
    let mut cause = std::error::Error::source(error);
    while let Some(error) = cause {
        if let Some(hop) = error.downcast_ref::<RefusedHop>() {
            return Some(SourceError::Protocol {
                status: Some(hop.status.as_u16()),
                message: hop.to_string(),
            });
        }
        cause = error.source();
    }
    None
}
