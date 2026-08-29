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
    assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
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
        Self::always_with_body(status, extra, "")
    }

    /// The same, with a body -- what a source's own error envelope arrives in.
    fn always_with_body(status: u16, extra: &'static str, body: &'static str) -> Self {
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
                        "HTTP/1.1 {status} X\r\n{extra}content-length: {}\r\nconnection: \
                         close\r\n\r\n{body}",
                        body.len()
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
    assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
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
    assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
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
    assert!(
        matches!(error, SourceError::Unauthorized { .. }),
        "{error:?}"
    );
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

    assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
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

/// **A `Retry-After` longer than the budget is refused, not waited out.**
///
/// The discriminating test for [`SEND_BUDGET`], and the one that also pins
/// finding 4's real shape: two capped waits. A server answering `429` with
/// `retry-after: 60` asks for 60 s, `RETRY_AFTER_CAP` caps each wait at
/// exactly that, and there are two of them between three attempts -- so
/// without a budget one call costs **120 s and 3 requests** while the caller's
/// advisory-locked transaction stays open. With it, the first wait is seen to
/// be longer than the whole call may take, and the call ends immediately:
/// **1 request, ~2 ms**.
///
/// The gap between those two outcomes is the entire point, which is why this
/// test asserts the request *count* and a millisecond-scale bound rather than
/// "faster than 45 s". An earlier version asserted `elapsed < SEND_BUDGET`
/// with a 300 ms `request_timeout`, which proved nothing at all: the timeout
/// was already shorter than the budget, so deleting the budget mechanism
/// entirely left it green.
#[tokio::test(flavor = "multi_thread")]
async fn a_retry_after_longer_than_the_budget_ends_the_call() {
    let server = CountingServer::always(429, "retry-after: 60\r\n");
    let client = HttpClient::new(config(server.url())).expect("client");

    let started = std::time::Instant::now();
    let error = client
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("429 every time");
    let elapsed = started.elapsed();

    // The 429 itself, classified -- not a budget-specific error. What the
    // caller needs to know is what the source said.
    assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
    assert_eq!(
        server.hits(),
        1,
        "a wait longer than the whole budget must not be started, so there is \
         no second attempt"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "the call must end at once rather than serving out two 60s waits; \
         took {elapsed:?}"
    );
}

/// A server that accepts the connection and then says nothing is
/// **`Unreachable`, not `Protocol`** -- which is the class the scheduler backs
/// off on, so the classification is the whole assertion.
///
/// Named for what it checks. It used to be called
/// `a_silent_server_is_unreachable_within_the_attempt_budget`, but it asserts
/// no budget and must not: with a 300 ms `request_timeout` the call is already
/// far inside [`SEND_BUDGET`](knobas_http::SEND_BUDGET), so an `elapsed <
/// SEND_BUDGET` bound here would stay green with the budget mechanism deleted
/// -- exactly the vacuity the test above spells out. The budget's
/// discriminating case is `a_retry_after_longer_than_the_budget_ends_the_call`.
#[tokio::test(flavor = "multi_thread")]
async fn a_silent_server_is_unreachable_rather_than_a_protocol_error() {
    let server = SilentServer::new();
    let client = HttpClient::new(HttpConfig {
        request_timeout: std::time::Duration::from_millis(300),
        connect_timeout: std::time::Duration::from_millis(300),
        ..config(server.url())
    })
    .expect("client");

    let error = client
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("nothing ever answers");

    // A timeout is `Unreachable`, which is the class the scheduler backs off
    // on -- not `Protocol`, which would read as knobas' own bug.
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
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

/// The body → message hook is reached from `send`, and only `send` can reach
/// it: the response body is consumed there, inside the one door onto the wire.
///
/// End to end on purpose. The hook travels `HttpConfig` → `HttpClient` →
/// `send`, and `classify`'s own tests prove none of that wiring -- a client
/// that dropped the hook on the way would leave every adapter's error envelope
/// unread with the whole unit suite still green.
#[tokio::test(flavor = "multi_thread")]
async fn the_callers_reading_of_a_failing_body_reaches_the_message() {
    fn lift(body: &str) -> Option<String> {
        let value: serde_json::Value = serde_json::from_str(body).ok()?;
        Some(value.get("message")?.as_str()?.to_owned())
    }

    let server = CountingServer::always_with_body(
        400,
        "content-type: application/json\r\n",
        r#"{"message":"the jql_filter names a field that does not exist"}"#,
    );
    let client = HttpClient::new(HttpConfig {
        body_message: Some(lift),
        ..config(server.url())
    })
    .expect("client");

    let error = client
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("400");
    let SourceError::Protocol { message, status } = &error else {
        panic!("a 400 is a protocol fault: {error:?}");
    };
    assert_eq!(
        message, "HTTP 400 Bad Request: the jql_filter names a field that does not exist",
        "the sentence the source sent is what the user reads"
    );
    // The status the response carried, structurally -- not read back out of
    // the message above (ADR-0004).
    assert_eq!(*status, Some(400));

    // Without a hook the same answer keeps its raw body, so the assertion
    // above is about the hook and not about the message format.
    let bare = HttpClient::new(config(server.url())).expect("client");
    let error = bare
        .get_json::<serde_json::Value>("/thing", &[])
        .await
        .expect_err("400");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. }
                 if message.contains("{\"message\":")),
        "{error:?}"
    );
}

// -- a request that carries a body (issue #43) --------------------------------

/// M2's write-backs are `POST`s carrying JSON, and until `Request::json`
/// existed there was no way to put a body on the wire through this crate at
/// all. The body and its content type both have to arrive: a Jira transition
/// sent without `Content-Type: application/json` is a 415, and one sent with
/// the header and no body is a 400 -- two different bugs that a test asserting
/// only the path would miss.
#[test]
fn a_json_body_and_its_content_type_both_reach_the_request() {
    let client = HttpClient::new(config("https://jira.example".to_owned())).expect("client");
    let request = client
        .request(Method::POST, "/rest/api/2/issue/PAY-231/transitions")
        .json(&serde_json::json!({ "transition": { "id": "31" } }))
        .build()
        .expect("a json body builds");

    assert_eq!(request.method(), &Method::POST);
    assert_eq!(
        request
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/json"),
    );
    let body = request
        .body()
        .and_then(reqwest::Body::as_bytes)
        .expect("a buffered body");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(body).expect("the body is the json"),
        serde_json::json!({ "transition": { "id": "31" } }),
    );
}

/// The body is buffered rather than streamed, which is what lets `send` retry
/// it: `try_clone` answers `None` for a streaming body, and a write that
/// silently got one attempt where every read gets three would fail on the first
/// 503 a source served.
///
/// Asserted through the retry count rather than by inspecting the body,
/// because that is the behaviour that would actually be lost.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_with_a_body_is_retried_like_any_other() {
    let server = CountingServer::always(503, "");
    let client = HttpClient::new(config(server.url())).expect("client");

    let error = client
        .send(
            client
                .request(Method::POST, "/app/rest/buildQueue")
                .json(&serde_json::json!({ "buildType": { "id": "Payout_Build" } })),
        )
        .await
        .expect_err("503 every time");
    assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
    assert_eq!(
        server.hits(),
        knobas_http::MAX_ATTEMPTS as usize,
        "a request with a body must get the documented attempts, not one"
    );
}
