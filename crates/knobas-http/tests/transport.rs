//! The transport, exercised without a network.
//!
//! No server is spun up here: `knobas-mockd` (stream T) is what adapters test
//! against, and this crate's own risk is the *classification* -- getting 403
//! wrong is a source that looks broken instead of one that needs a password.

// `Method` comes from `knobas_http`, not from `reqwest`: an adapter depends on
// this crate alone for its transport, and this import is what proves it can.
use knobas_http::{Auth, HttpClient, HttpConfig, Method};
use knobas_source::SourceError;

fn config(base_url: String) -> HttpConfig {
    HttpConfig {
        base_url,
        adapter_kind: "test".to_owned(),
        adapter_version: "0.1.0".to_owned(),
        auth: Auth::Bearer("token".to_owned()),
        ..HttpConfig::default()
    }
}

/// §4.1: connect/DNS/TLS/timeout are `Unreachable` -- the class the scheduler
/// backs off on, as opposed to the one that needs a human.
#[tokio::test]
async fn a_refused_connection_is_unreachable() {
    // Bind and drop: the port is real, closed, and nobody else's.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);

    let client = HttpClient::new(config(format!("http://127.0.0.1:{port}"))).expect("client");
    let error = client
        .get_json::<serde_json::Value>("/rest/api/2/myself", &[])
        .await
        .expect_err("nothing is listening");
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
}

/// A base URL that is not a URL is a configuration mistake, and it must fail
/// at construction rather than on the first request of the first sync.
#[tokio::test]
async fn a_bad_base_url_is_refused_up_front() {
    let error = HttpClient::new(config("not a url".to_owned())).expect_err("bad base url");
    assert!(matches!(error, SourceError::Protocol(_)), "{error:?}");
}

/// Each `Auth` variant's exact wire spelling. Gitea's is the one that bites:
/// it is `token <pat>`, not `Bearer <pat>`, and a Gitea that does not
/// recognise the scheme answers 401 -- which knobas then correctly reports as
/// *Re-enter password* for a password that was right all along.
#[test]
fn each_auth_variant_has_its_own_wire_spelling() {
    fn authorization(auth: Auth) -> Option<String> {
        let client = HttpClient::new(HttpConfig {
            auth,
            ..config("https://example.test".to_owned())
        })
        .expect("client");
        let request = client
            .request(Method::GET, "/api/v1/user")
            .build()
            .expect("a GET with no body always builds");
        request
            .headers()
            .get("authorization")
            .map(|value| value.to_str().expect("ascii").to_owned())
    }

    assert_eq!(authorization(Auth::None), None);
    assert_eq!(
        authorization(Auth::Bearer("t".to_owned())),
        Some("Bearer t".to_owned())
    );
    assert_eq!(
        authorization(Auth::GiteaToken("t".to_owned())),
        Some("token t".to_owned())
    );
    // Basic is base64("someone:secret").
    assert_eq!(
        authorization(Auth::Basic {
            username: "someone".to_owned(),
            password: "secret".to_owned(),
        }),
        Some("Basic c29tZW9uZTpzZWNyZXQ=".to_owned())
    );
}

/// The secret reaches the wire and nothing else. `Auth` and `HttpClient` both
/// carry it, both are `Debug`, and a `tracing` field or a panic message is one
/// `{:?}` away -- so the redaction is a test, not a convention.
#[test]
fn a_secret_never_reaches_a_debug_rendering() {
    const SECRET: &str = "s3cr3t-token-value";

    let renderings = [
        format!("{:?}", Auth::Bearer(SECRET.to_owned())),
        format!("{:?}", Auth::GiteaToken(SECRET.to_owned())),
        format!(
            "{:?}",
            Auth::Basic {
                username: "someone".to_owned(),
                password: SECRET.to_owned(),
            }
        ),
        format!(
            "{:?}",
            HttpConfig {
                auth: Auth::Bearer(SECRET.to_owned()),
                ..config("https://example.test".to_owned())
            }
        ),
        format!(
            "{:?}",
            HttpClient::new(HttpConfig {
                auth: Auth::Bearer(SECRET.to_owned()),
                ..config("https://example.test".to_owned())
            })
            .expect("client")
        ),
    ];

    for rendering in renderings {
        assert!(!rendering.contains(SECRET), "{rendering}");
    }
}

/// Paths are appended, not `Url::join`ed: joining `"/api/v1/user"` onto
/// `"https://gitea.example/prefix"` silently drops `prefix`, and a source
/// behind a path prefix is exactly the setup nobody tests until production.
#[test]
fn a_path_is_appended_to_the_whole_base_url() {
    let client =
        HttpClient::new(config("https://example.test/prefix/".to_owned())).expect("client");
    assert_eq!(
        client.url_for("/api/v1/user"),
        "https://example.test/prefix/api/v1/user"
    );
    assert_eq!(
        client.url_for("api/v1/user"),
        "https://example.test/prefix/api/v1/user"
    );
}

// ---------------------------------------------------------------------------
// The retry budget, the limiter and `Retry-After` -- against a real socket.
// ---------------------------------------------------------------------------

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A one-response HTTP server that counts what it was asked.
///
/// Hand-rolled rather than `wiremock`: the whole assertion is *how many
/// requests arrived and how far apart*, which needs a counter and a socket and
/// nothing else. It answers every request with the same canned status until it
/// is dropped.
struct CountingServer {
    port: u16,
    hits: Arc<AtomicUsize>,
}

impl CountingServer {
    /// Serve `status` (with optional extra headers) forever, counting requests.
    fn always(status: u16, extra: &'static str) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");

        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        tauri_free_spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let counter = Arc::clone(&counter);
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    // Read the request head; the body is never used.
                    let mut buffer = [0u8; 2048];
                    let _ = socket.read(&mut buffer).await;
                    counter.fetch_add(1, Ordering::SeqCst);
                    let response = format!(
                        "HTTP/1.1 {status} X\r\n{extra}content-length: 0\r\nconnection: close\r\n\r\n"
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self { port, hits }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

fn tauri_free_spawn<F: std::future::Future<Output = ()> + Send + 'static>(future: F) {
    tokio::spawn(future);
}

/// A transient status costs exactly [`MAX_ATTEMPTS`] requests -- not one more.
///
/// The bug this pins: with the retry living in middleware *inside* one send,
/// `send` saw only the middleware's final answer and then retried once more
/// itself, so a 503 cost four requests while the crate documented three.
#[tokio::test(flavor = "multi_thread")]
async fn a_transient_failure_costs_exactly_the_documented_attempts() {
    let server = CountingServer::always(503, "");
    let client = HttpClient::new(config(server.url())).expect("client");

    let error = client
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("503 every time");
    assert!(matches!(error, SourceError::Protocol(_)), "{error:?}");
    assert_eq!(
        server.hits(),
        knobas_http::MAX_ATTEMPTS as usize,
        "a transient failure must cost MAX_ATTEMPTS requests in total"
    );
}

/// A non-transient failure is not retried at all: one request, one answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_deterministic_failure_is_asked_once() {
    let server = CountingServer::always(500, "");
    let client = HttpClient::new(config(server.url())).expect("client");

    let error = client
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("500");
    assert!(matches!(error, SourceError::Protocol(_)), "{error:?}");
    assert_eq!(server.hits(), 1, "500 is deliberately not retried");
}

/// 401 short-circuits the budget too, and keeps its class: this is the failure
/// the sources view offers *Re-enter password* for.
#[tokio::test(flavor = "multi_thread")]
async fn an_unauthorized_answer_is_not_retried_and_keeps_its_class() {
    let server = CountingServer::always(401, "");
    let client = HttpClient::new(config(server.url())).expect("client");

    let error = client
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("401");
    assert!(matches!(error, SourceError::Unauthorized), "{error:?}");
    assert_eq!(server.hits(), 1, "a credential does not improve on retry");
}

/// `Retry-After` is obeyed **before** the retry it applies to.
///
/// The bug this pins: the header was read only after the retry budget was
/// already spent, so a server asking for a one-second pause was ignored twice
/// first -- which is the opposite of what the header is for. One second is the
/// smallest value the `delay-seconds` form can express, so it is also the
/// cheapest way to prove the wait happened.
#[tokio::test(flavor = "multi_thread")]
async fn retry_after_is_waited_out_before_the_retry() {
    let server = CountingServer::always(429, "retry-after: 1\r\n");
    let client = HttpClient::new(config(server.url())).expect("client");

    let started = std::time::Instant::now();
    let error = client
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("429 every time");
    let elapsed = started.elapsed();

    assert!(matches!(error, SourceError::Protocol(_)), "{error:?}");
    assert_eq!(server.hits(), knobas_http::MAX_ATTEMPTS as usize);
    // Two waits of one second each between three attempts. Compared against
    // the exponential it replaces (250 ms + 500 ms), so this cannot pass by
    // accident if the header is ignored.
    assert!(
        elapsed >= std::time::Duration::from_millis(1900),
        "Retry-After was not waited out: {elapsed:?}"
    );
}

/// **Every attempt passes the rate limiter, retries included.**
///
/// This is the guarantee that justified writing the retry loop instead of
/// using `reqwest-retry`, and it needs a quota tight enough to see. The other
/// tests here run at the default `burst: 10`, which swallows three attempts
/// whole -- so hoisting `until_ready()` out of the loop leaves every one of
/// them green while the guarantee is gone.
///
/// At one request per second with a burst of one: attempt 1 goes immediately,
/// attempt 2 cannot start before t=1 s, attempt 3 not before t=2 s. The
/// exponential backoff between them (250 ms + 500 ms) is far shorter, so the
/// limiter is what sets the pace and ~2 s is what it costs. Ungated, the same
/// three attempts cost only the backoff: ~0.75 s. The assertion sits between
/// the two and cannot be satisfied by the backoff alone.
#[tokio::test(flavor = "multi_thread")]
async fn every_attempt_waits_for_the_rate_limiter() {
    let server = CountingServer::always(503, "");
    let client = HttpClient::new(HttpConfig {
        requests_per_second: 1,
        burst: 1,
        ..config(server.url())
    })
    .expect("client");

    let started = std::time::Instant::now();
    client
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("503 every time");
    let elapsed = started.elapsed();

    assert_eq!(server.hits(), knobas_http::MAX_ATTEMPTS as usize);
    assert!(
        elapsed >= std::time::Duration::from_millis(1900),
        "three attempts at 1/s must take ~2s; {elapsed:?} means the retries \
         skipped the limiter (the backoff alone is 750ms)"
    );
}

/// The whole call is bounded, and the bound is real even when a single attempt
/// is the thing that is slow.
///
/// A server that accepts the connection and then says nothing would otherwise
/// cost `MAX_ATTEMPTS` × the request timeout. The per-attempt timeout is
/// shortened to whatever is left of [`SEND_BUDGET`], so the ceiling holds; here
/// the budget is squeezed by a short `request_timeout` so the test costs a
/// second rather than forty-five.
#[tokio::test(flavor = "multi_thread")]
async fn a_silent_server_cannot_outlast_the_budget() {
    let server = SilentServer::new();
    let client = HttpClient::new(HttpConfig {
        request_timeout: std::time::Duration::from_millis(300),
        connect_timeout: std::time::Duration::from_millis(300),
        ..config(server.url())
    })
    .expect("client");

    let started = std::time::Instant::now();
    let error = client
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("nothing ever answers");
    let elapsed = started.elapsed();

    // A timeout is `Unreachable`, which is the class the scheduler backs off
    // on -- not `Protocol`, which would read as knobas' own bug.
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
    assert!(
        elapsed < knobas_http::SEND_BUDGET,
        "the call must be bounded by SEND_BUDGET, took {elapsed:?}"
    );
}

/// A server that accepts connections and never replies.
struct SilentServer {
    port: u16,
    _keep: tokio::task::JoinHandle<()>,
}

impl SilentServer {
    fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
        let keep = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                // Held, never answered, never closed.
                held.push(socket);
            }
        });
        Self { port, _keep: keep }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}
