//! The GETs, through `knobas-http` (decisions D4, D6).
//!
//! rustls with the system's roots, the system proxy and `HTTPS_PROXY`, 10 s to connect, 30 s for
//! an answer. `User-Agent: knobas/<version> (update/<version>)` and `Accept: application/json`,
//! nothing else: no token, no cookie. Every hop of a redirect must be https, or http to a loopback
//! address when the reads started there (a test's feed).

use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Utc};
use knobas_http::{HttpClient, HttpConfig, Method};
use knobas_source::SourceError;

use crate::channel::{Endpoints, Fetch, FetchError, allowed, on_this_machine};

/// The `update` half of the user agent.
pub const AGENT: &str = "update";

/// Where a client's redirect check leaves the hop it refused.
pub type RefusedHop = Arc<Mutex<Option<String>>>;

/// A client for the origin of `url`, whose reads follow only allowed hops, and the path to ask
/// it for. The refused hop, if one was, lands in the returned slot.
///
/// # Errors
///
/// [`FetchError::Bad`] for a URL that is not one or not allowed.
pub fn client_for(
    url: &str,
    max_body: u64,
) -> Result<(HttpClient, String, RefusedHop), FetchError> {
    let parsed =
        url::Url::parse(url).map_err(|error| FetchError::Bad(format!("{url}: {error}")))?;
    let from_here = on_this_machine(&parsed);
    if !allowed(&parsed, from_here) {
        return Err(FetchError::Bad(format!("{url} is not https")));
    }
    let mut path = parsed.path().to_owned();
    if let Some(query) = parsed.query() {
        path = format!("{path}?{query}");
    }
    let refused = Arc::new(Mutex::new(None));
    let check = {
        let refused = Arc::clone(&refused);
        move |hop: &knobas_http::Url| {
            let ok = allowed(hop, from_here);
            if !ok {
                *refused.lock().unwrap_or_else(PoisonError::into_inner) = Some(hop.to_string());
            }
            ok
        }
    };
    let client = HttpClient::with_redirect_check(
        HttpConfig {
            base_url: parsed.origin().ascii_serialization(),
            adapter_kind: AGENT.to_owned(),
            adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
            max_body,
            ..HttpConfig::default()
        },
        check,
    )
    .map_err(|error| FetchError::Bad(error.to_string()))?;
    Ok((client, path, refused))
}

/// A failed send, as the check reads it.
#[must_use]
pub fn failure(error: &SourceError, refused: Option<String>) -> FetchError {
    if let Some(hop) = refused {
        return FetchError::Bad(format!("GitHub redirected to {hop}, which is not https"));
    }
    match error {
        SourceError::Unreachable(why) | SourceError::Unconfirmed { message: why, .. } => {
            FetchError::Offline(why.clone())
        }
        other => match other.status() {
            Some(404) => FetchError::NotFound,
            Some(status) => FetchError::Bad(format!("GitHub answered {status}")),
            None => FetchError::Bad(other.to_string()),
        },
    }
}

/// The app's [`Fetch`].
#[derive(Debug, Clone, Default)]
pub struct HttpFetch {
    pub endpoints: Endpoints,
}

#[async_trait::async_trait]
impl Fetch for HttpFetch {
    async fn get(&self, url: &str, cap: u64) -> Result<Vec<u8>, FetchError> {
        let (client, path, refused) = client_for(url, cap)?;
        let taken = || {
            refused
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
        };
        let response = match client.send(client.request(Method::GET, &path)).await {
            Ok(response) => response,
            Err(error) => {
                let status = error.status();
                // GitHub's primary limit answers 403 or 429 and says when it resets only in
                // that answer's headers, which the HTTP stack keeps to itself. `rate_limit` is
                // not counted against the limit and carries the same `x-ratelimit-*` headers.
                if matches!(status, Some(403 | 429)) && url.starts_with(&self.endpoints.api) {
                    return Err(self.rate_limit().await.unwrap_or_else(|| {
                        FetchError::Bad(format!("GitHub answered {}", status.unwrap_or_default()))
                    }));
                }
                return Err(failure(&error, taken()));
            }
        };
        let bytes = response
            .bytes()
            .await
            .map_err(|error| FetchError::Offline(format!("reading the answer: {error}")))?;
        Ok(bytes.to_vec())
    }
}

impl HttpFetch {
    /// `RateLimited` with GitHub's reset time when the address's limit is used up.
    async fn rate_limit(&self) -> Option<FetchError> {
        let url = format!("{}/rate_limit", self.endpoints.api);
        let (client, path, _) = client_for(&url, 64 * 1024).ok()?;
        let response = client.send(client.request(Method::GET, &path)).await.ok()?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<i64>().ok())
        };
        let remaining = header("x-ratelimit-remaining")?;
        (remaining == 0).then(|| FetchError::RateLimited {
            reset: header("x-ratelimit-reset")
                .and_then(|at| DateTime::<Utc>::from_timestamp(at, 0)),
        })
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn fetch(server: &MockServer) -> HttpFetch {
        HttpFetch {
            endpoints: Endpoints {
                web: server.uri(),
                api: server.uri(),
            },
        }
    }

    /// Decisions D6: a user agent naming knobas, and no `Authorization` or `Cookie`.
    #[tokio::test]
    async fn the_user_agent_names_knobas_and_nothing_else() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/x"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;
        fetch(&server)
            .get(&format!("{}/x", server.uri()), 1024)
            .await
            .unwrap();
        let requests = server.received_requests().await.unwrap();
        let headers = &requests[0].headers;
        let agent = headers.get("user-agent").unwrap().to_str().unwrap();
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(agent, format!("knobas/{version} (update/{version})"));
        assert!(headers.get("authorization").is_none());
        assert!(headers.get("cookie").is_none());
        assert_eq!(headers.get("accept").unwrap(), "application/json");
    }

    #[tokio::test]
    async fn a_404_is_not_found() {
        let server = MockServer::builder().start().await;
        assert_eq!(
            fetch(&server)
                .get(&format!("{}/none", server.uri()), 1024)
                .await,
            Err(FetchError::NotFound)
        );
    }

    /// Decisions D4: the rate-limited answer names when to try again, from GitHub's
    /// `x-ratelimit-reset`.
    #[tokio::test]
    async fn a_rate_limited_answer_names_when_to_try_again() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/repos/BFoerschner/knobas/releases"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("x-ratelimit-reset", "1800000000"),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/rate_limit"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("x-ratelimit-reset", "1800000000")
                    .set_body_string("{}"),
            )
            .mount(&server)
            .await;
        let url = format!(
            "{}/repos/BFoerschner/knobas/releases?per_page=10",
            server.uri()
        );
        assert_eq!(
            fetch(&server).get(&url, 1024).await,
            Err(FetchError::RateLimited {
                reset: DateTime::from_timestamp(1_800_000_000, 0)
            })
        );
    }

    #[tokio::test]
    async fn a_403_with_requests_left_is_a_bad_answer() {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/rate_limit"))
            .respond_with(ResponseTemplate::new(200).insert_header("x-ratelimit-remaining", "40"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/BFoerschner/knobas/releases"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        let url = format!(
            "{}/repos/BFoerschner/knobas/releases?per_page=10",
            server.uri()
        );
        assert!(matches!(
            fetch(&server).get(&url, 1024).await,
            Err(FetchError::Bad(_))
        ));
    }

    #[tokio::test]
    async fn plain_http_off_this_machine_is_never_asked() {
        let fetch = HttpFetch::default();
        assert!(matches!(
            fetch.get("http://example.com/latest.json", 1024).await,
            Err(FetchError::Bad(_))
        ));
    }
}
