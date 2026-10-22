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
    assert_eq!(authorization_of(Auth::None), None);
    assert_eq!(
        authorization_of(Auth::Bearer("t".to_owned())),
        Some("Bearer t".to_owned())
    );
    assert_eq!(
        authorization_of(Auth::GiteaToken("t".to_owned())),
        Some("token t".to_owned())
    );
    // Basic is base64("someone:secret").
    assert_eq!(
        authorization_of(Auth::Basic {
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
        // The port stays bound for the rest of the process, so no later
        // server in this binary is handed it: the rate limit is process-wide
        // and keyed by origin (#1630), and a later test on a reused port
        // would inherit whatever quota this one tightened it to.
        std::mem::forget(listener.try_clone().expect("hold the port"));
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
        elapsed < std::time::Duration::from_secs(30),
        "the call must end at once rather than serving out two 60s waits; \
         took {elapsed:?}"
    );
}

/// The budget the limiter tests run on: long enough for the second of a
/// 1-a-second queue to go (at t=1 s), short enough that the third (t=2 s)
/// cannot, and far short of the real 45 s.
const SHORT_BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);

/// `count` sends of `GET /x` started at once on `client`: what each ended in,
/// and how long it took.
async fn queued_sends(
    client: &Arc<HttpClient>,
    count: usize,
) -> Vec<(Result<u16, SourceError>, std::time::Duration)> {
    let mut sends = tokio::task::JoinSet::new();
    for _ in 0..count {
        let client = Arc::clone(client);
        sends.spawn(async move {
            let started = std::time::Instant::now();
            let outcome = client
                .send(client.request(Method::GET, "/x"))
                .await
                .map(|response| response.status().as_u16());
            (outcome, started.elapsed())
        });
    }
    sends.join_all().await
}

/// **A limiter wait that outlasts the budget is the local quota, not the
/// instance** (SPI-11, #1630).
///
/// Five sends queued at once behind one a second with a burst of one: the
/// first goes at once, the second at t=1 s, and the other three cannot go
/// before the 1.5 s budget runs out. Not one of the three reached the
/// instance, so `Unreachable` -- the class the scheduler backs a source off
/// on -- would blame it for a wait knobas imposed. Each is `Protocol`, naming
/// the quota it waited on.
#[tokio::test(flavor = "multi_thread")]
async fn a_limiter_wait_that_outlasts_the_budget_is_the_local_quota() {
    let server = CountingServer::always(200, "");
    let client = Arc::new(paced_client(&server, 1).with_send_budget(SHORT_BUDGET));

    let outcomes = queued_sends(&client, 5).await;

    let sent = outcomes
        .iter()
        .filter(|(outcome, _)| outcome.is_ok())
        .count();
    assert_eq!(sent, 2, "t=0 and t=1s fit the budget: {outcomes:?}");
    assert_eq!(
        server.hits(),
        2,
        "nothing past the budget reached the server"
    );
    for (outcome, _) in outcomes.iter().filter(|(outcome, _)| outcome.is_err()) {
        let Err(SourceError::Protocol {
            status: None,
            message,
        }) = outcome
        else {
            panic!("a wait on the local quota must be Protocol, never Unreachable: {outcome:?}");
        };
        assert!(
            message.contains("local rate limit") && message.contains("1 request a second"),
            "the message names the quota it waited on: {message}"
        );
    }
}

/// **The limiter wait never outlasts what is left of the budget** -- the
/// limiter's half of [`SEND_BUDGET`](knobas_http::SEND_BUDGET), beside
/// `a_retry_after_longer_than_the_budget_ends_the_call`'s half for the waits
/// between attempts.
///
/// Unbounded, the five queued sends above end at t=0, 1, 2, 3 and 4 s; bounded,
/// none ends later than the 1.5 s budget, plus the scheduling slack of a
/// loaded test machine. The retry half: a `503` at t=0 asks for a second
/// attempt, which cannot pass the limiter before the budget runs out, so the
/// call ends at the budget with the `503` it had -- what the source said --
/// and not with the quota, which only kept it from asking again.
#[tokio::test(flavor = "multi_thread")]
async fn a_limiter_wait_never_outlasts_the_budget() {
    let slack = std::time::Duration::from_millis(400);

    let server = CountingServer::always(200, "");
    let client = Arc::new(paced_client(&server, 1).with_send_budget(SHORT_BUDGET));
    for (outcome, took) in queued_sends(&client, 5).await {
        assert!(
            took <= SHORT_BUDGET + slack,
            "a send ended {took:?} in, past the {SHORT_BUDGET:?} budget: {outcome:?}"
        );
    }

    let busy = CountingServer::always(503, "");
    let client = paced_client(&busy, 1).with_send_budget(std::time::Duration::from_millis(800));
    let started = std::time::Instant::now();
    let error = client
        .send(client.request(Method::GET, "/x"))
        .await
        .expect_err("503, then no room for a second attempt");
    let took = started.elapsed();
    assert!(
        matches!(
            error,
            SourceError::Protocol {
                status: Some(503),
                ..
            }
        ),
        "the call ends with what the source said: {error:?}"
    );
    assert_eq!(
        busy.hits(),
        1,
        "the second attempt never passed the limiter"
    );
    assert!(
        took <= std::time::Duration::from_millis(800) + slack,
        "{took:?}"
    );
}

/// **A call the local quota stopped is marked beside its error** (#1729).
///
/// The error is a status-less `Protocol`, which is also what a refusal
/// knobas raised itself looks like, so the write queue cannot tell the two
/// apart from the error alone. [`knobas_http::watch_local_quota`] hands back
/// the quota a call inside it stopped on. One a second with a burst of one:
/// the first send goes, the second cannot pass the limiter within its
/// budget and is marked. A call that went is not marked, and a fresh watch
/// does not see the last one's mark.
#[tokio::test(flavor = "multi_thread")]
async fn a_watched_call_the_quota_stopped_is_marked() {
    let server = CountingServer::always(200, "");
    let client = paced_client(&server, 1).with_send_budget(std::time::Duration::from_millis(300));

    let (went, quota) =
        knobas_http::watch_local_quota(client.send(client.request(Method::GET, "/x"))).await;
    assert!(went.is_ok(), "the burst lets the first through: {went:?}");
    assert_eq!(quota, None, "a call that went is not marked");

    let (stopped, quota) =
        knobas_http::watch_local_quota(client.send(client.request(Method::GET, "/x"))).await;
    assert!(
        matches!(stopped, Err(SourceError::Protocol { status: None, .. })),
        "{stopped:?}"
    );
    let quota = quota.expect("the quota that stopped the call is handed back");
    assert!(quota.to_string().contains("1 request a second"), "{quota}");
    assert_eq!(server.hits(), 1, "the stopped call never reached the server");

    let ((), quota) = knobas_http::watch_local_quota(async {}).await;
    assert_eq!(quota, None, "each watch starts unmarked");
}

/// **A retry the quota stopped is not marked** (#1729): the call ends with
/// what the source said on the attempt before, a `503` here, and that answer
/// is what the write queue reads. Only a call that never reached the source
/// carries the mark.
#[tokio::test(flavor = "multi_thread")]
async fn a_retry_the_quota_stopped_is_not_marked() {
    let busy = CountingServer::always(503, "");
    let client = paced_client(&busy, 1).with_send_budget(std::time::Duration::from_millis(800));

    let (answered, quota) =
        knobas_http::watch_local_quota(client.send(client.request(Method::GET, "/x"))).await;

    assert!(
        matches!(
            answered,
            Err(SourceError::Protocol {
                status: Some(503),
                ..
            })
        ),
        "{answered:?}"
    );
    assert_eq!(quota, None, "the source was asked, so this is its answer");
}

/// What reaches `server` within the first 900 ms of `per_client` sends started
/// at once on each of `clients` -- under the second at which a one-a-second
/// quota adds a token, so the burst is all that can pass. The sends still
/// queued are abandoned.
async fn hits_within_the_burst(
    server: &CountingServer,
    clients: Vec<HttpClient>,
    per_client: usize,
) -> usize {
    let mut sends = tokio::task::JoinSet::new();
    for client in clients {
        let client = Arc::new(client);
        for _ in 0..per_client {
            let client = Arc::clone(&client);
            sends.spawn(async move {
                let _ = client.send(client.request(Method::GET, "/x")).await;
            });
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(900)).await;
    let hits = server.hits();
    sends.abort_all();
    hits
}

/// A client for `server` at one request a second and the given burst.
fn paced_client(server: &CountingServer, burst: u32) -> HttpClient {
    client_at(server, 1, burst)
}

/// **Every client built for one instance shares its quota** (SPI-2, #1630).
///
/// A sync run, a live read and a flush each build their own client, and they
/// run at once. Two clients for one origin at a burst of ten, ten sends each:
/// the instance sees ten in the first second, not twenty.
#[tokio::test(flavor = "multi_thread")]
async fn two_clients_for_one_instance_share_one_burst() {
    let server = CountingServer::always(200, "");
    let clients = vec![paced_client(&server, 10), paced_client(&server, 10)];

    let hits = hits_within_the_burst(&server, clients, 10).await;

    assert!(
        hits <= 10,
        "{hits} requests reached the instance within its burst of 10: each client \
         had a limiter of its own"
    );
    assert!(hits >= 1, "the shared burst let nothing through");
}

/// **Where two configurations name different quotas for one instance, the
/// stricter governs**, in either order (#1630).
///
/// A burst of ten and a burst of two at one origin: the instance sees two in
/// the first second, whichever client the sends go through and whichever was
/// built first. Built loose-first, the stricter one tightens the quota under
/// the client that already holds it -- which is why the sends go through the
/// loose client in both halves.
#[tokio::test(flavor = "multi_thread")]
async fn the_stricter_of_two_quotas_for_one_instance_governs() {
    let strict_first = CountingServer::always(200, "");
    let _strict = paced_client(&strict_first, 2);
    let loose = paced_client(&strict_first, 10);
    let hits = hits_within_the_burst(&strict_first, vec![loose], 10).await;
    assert!(
        hits <= 2,
        "strict first: {hits} reached the instance; the looser burst of 10 won"
    );
    assert!(
        hits >= 1,
        "strict first: the burst of 2 let nothing through"
    );

    let loose_first = CountingServer::always(200, "");
    let loose = paced_client(&loose_first, 10);
    let _strict = paced_client(&loose_first, 2);
    let hits = hits_within_the_burst(&loose_first, vec![loose], 10).await;
    assert!(
        hits <= 2,
        "loose first: {hits} reached the instance; the stricter burst of 2 was ignored"
    );
    assert!(
        hits >= 1,
        "loose first: the tightened quota let nothing through"
    );
}

/// A client for `server` at `per_second` and `burst`.
fn client_at(server: &CountingServer, per_second: u32, burst: u32) -> HttpClient {
    HttpClient::new(HttpConfig {
        requests_per_second: per_second,
        burst,
        ..config(server.url())
    })
    .expect("client")
}

/// **A quota loosens once no live client asks for the stricter one**
/// (#1728).
///
/// A source edited from one a second to twenty: the run that built its
/// client at the old quota has ended, so the next client is paced by the new
/// one. Held to the old quota, one request would reach the instance in the
/// first 900 ms; at twenty a second, started drained, about seventeen do.
#[tokio::test(flavor = "multi_thread")]
async fn a_quota_loosens_once_the_stricter_client_is_gone() {
    let server = CountingServer::always(200, "");
    drop(client_at(&server, 1, 1));
    let loose = client_at(&server, 20, 20);

    let hits = hits_within_the_burst(&server, vec![loose], 20).await;

    assert!(
        hits >= 5,
        "{hits} reached the instance in 900 ms at twenty a second: the dropped \
         client's one a second still governs"
    );
}

/// **A loosening hands out no fresh burst** (#1728).
///
/// Two sources on one server at different quotas, syncing one after the
/// other, would otherwise get a new burst on every alternation. The limiter
/// built for a loosening starts drained: at two a second and a burst of
/// twenty, what reaches the instance in the first 900 ms is the one request
/// the refill allows at 500 ms, not twenty. That the quota loosened at all
/// is the test above's; this one is that the loosening sent no burst.
#[tokio::test(flavor = "multi_thread")]
async fn a_loosened_quota_starts_drained() {
    let server = CountingServer::always(200, "");
    drop(client_at(&server, 1, 1));
    let loose = client_at(&server, 2, 20);

    let hits = hits_within_the_burst(&server, vec![loose], 20).await;

    assert!(
        hits <= 2,
        "{hits} reached the instance in 900 ms right after a loosening: the new \
         limiter handed out its burst"
    );
    assert!(hits >= 1, "the drained limiter never refilled");
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

/// A write that may already have landed is sent **once** when its caller says
/// so (issue #1122). The server here takes the request in and never answers --
/// from the client's side exactly a POST that reached the source and timed out
/// on the way back -- and the request is not put on the wire a second time.
///
/// The control half is what makes the count mean something: the same request
/// without [`Request::no_retry`](knobas_http::Request::no_retry), against the
/// same silence, is sent [`MAX_ATTEMPTS`](knobas_http::MAX_ATTEMPTS) times.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_that_opted_out_of_retries_is_sent_once_when_it_times_out() {
    async fn post(no_retry: bool) -> (SourceError, usize) {
        let server = SilentServer::new();
        let client = HttpClient::new(HttpConfig {
            request_timeout: std::time::Duration::from_millis(300),
            ..config(server.url())
        })
        .expect("client");
        let request = client
            .request(Method::POST, "/rest/api/2/issue")
            .json(&serde_json::json!({ "fields": { "summary": "once" } }));
        let request = if no_retry {
            request.no_retry()
        } else {
            request
        };
        let error = client.send(request).await.expect_err("nothing answers");
        (error, server.hits())
    }

    // Unconfirmed, not Unreachable (#1125): the request reached the source,
    // so the write queue must not take the timeout for "nothing happened".
    let (error, hits) = post(true).await;
    assert!(
        matches!(error, SourceError::Unconfirmed { status: None, .. }),
        "{error:?}"
    );
    assert_eq!(hits, 1, "a timed-out create must not be sent again");

    let (error, hits) = post(false).await;
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
    assert_eq!(
        hits,
        knobas_http::MAX_ATTEMPTS as usize,
        "the same silence retries a request that did not opt out"
    );
}

/// The opt-out covers a busy answer too: a `502` or `504` from a proxy says
/// nothing about whether the source behind it acted, so a write that may not
/// be repeated is not repeated on any answer.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_that_opted_out_of_retries_is_not_retried_on_a_busy_answer() {
    let server = CountingServer::always(504, "");
    let client = HttpClient::new(config(server.url())).expect("client");

    let error = client
        .send(
            client
                .request(Method::POST, "/rest/api/content")
                .json(&serde_json::json!({ "type": "comment" }))
                .no_retry(),
        )
        .await
        .expect_err("504");
    assert!(
        matches!(
            error,
            SourceError::Unconfirmed {
                status: Some(504),
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(server.hits(), 1);
}

/// A busy answer that *does* say the source did nothing stays what it was
/// (#1125): a `503` or a `429` is the source turning the request away, so the
/// write queue may send it again later. Only a gateway's `502`/`504` is
/// unconfirmed.
#[tokio::test(flavor = "multi_thread")]
async fn a_once_sent_write_turned_away_as_busy_is_not_unconfirmed() {
    for status in [503, 429] {
        let server = CountingServer::always(status, "");
        let client = HttpClient::new(config(server.url())).expect("client");
        let error = client
            .send(
                client
                    .request(Method::POST, "/rest/api/content")
                    .json(&serde_json::json!({ "type": "comment" }))
                    .no_retry(),
            )
            .await
            .expect_err("busy");
        assert!(
            matches!(error, SourceError::Protocol { status: Some(s), .. } if s == status),
            "{status}: {error:?}"
        );
        assert_eq!(server.hits(), 1, "{status}");
    }
}

/// A once-sent write whose connection never opened, on every attempt, is
/// plain `Unreachable` (#1125): nothing reached the source, so the queue may
/// send it again on a later tick.
#[tokio::test(flavor = "multi_thread")]
async fn a_once_sent_write_that_never_connected_is_unreachable() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    let client = HttpClient::new(config(format!("http://127.0.0.1:{port}"))).expect("client");

    let error = client
        .send(
            client
                .request(Method::POST, "/rest/api/2/issue")
                .json(&serde_json::json!({ "fields": { "summary": "once" } }))
                .no_retry(),
        )
        .await
        .expect_err("nothing listens");
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
}

/// A connect error is the one failure a write that opted out is still retried
/// on: the connection never opened, so nothing reached the source (#1122
/// review). The port refuses the first attempt and is listening by the second,
/// [`BACKOFF_BASE`](knobas_http::retry::BACKOFF_BASE) later, which answers
/// `201` and counts.
#[tokio::test(flavor = "multi_thread")]
async fn a_write_that_opted_out_of_retries_is_retried_after_a_refused_connection() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Bind and drop: the port is real and closed until the server takes it.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);

    // Built first: building a client can take longer than the server's delay,
    // and the first attempt must meet the closed port.
    let client = HttpClient::new(config(format!("http://127.0.0.1:{port}"))).expect("client");

    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    let server = tokio::spawn(async move {
        // Well inside the first backoff (250 ms), well after the first attempt.
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .expect("rebind");
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut buffer = [0u8; 2048];
            let _ = socket.read(&mut buffer).await;
            counter.fetch_add(1, Ordering::SeqCst);
            let _ = socket
                .write_all(
                    b"HTTP/1.1 201 Created\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                )
                .await;
            let _ = socket.shutdown().await;
        }
    });

    let sent = client
        .send(
            client
                .request(Method::POST, "/rest/api/2/issue")
                .json(&serde_json::json!({ "fields": { "summary": "once" } }))
                .no_retry(),
        )
        .await;
    server.abort();

    assert!(sent.is_ok(), "{sent:?}");
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

/// A server that accepts connections and never replies, counting them.
struct SilentServer {
    port: u16,
    accepted: Arc<AtomicUsize>,
    _keep: tokio::task::JoinHandle<()>,
}

impl SilentServer {
    fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&accepted);
        let keep = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
                // Held, never answered, never closed.
                held.push(socket);
            }
        });
        Self {
            port,
            accepted,
            _keep: keep,
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Connections accepted. A timed-out request's connection is never
    /// reused, so this is the number of attempts.
    fn hits(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
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

// -- the body's start in a decode failure (#877, #1313) ------------------------

/// A body that is not the JSON the caller asked for names its own start when
/// the caller asks for that, bounded to [`knobas_http::BODY_EXCERPT_CHARS`]
/// characters, and stays out of the message when it does not.
///
/// "Did not answer with the shape" says nothing a person can act on without
/// the body: an SSO proxy's login page and a gateway's plain text both arrive
/// as a 200 that is not JSON.
#[tokio::test(flavor = "multi_thread")]
async fn a_body_that_is_not_the_shape_asked_for_names_its_start_when_asked() {
    // Characters, not bytes: the bound must not split a multi-byte character.
    let page: &'static str =
        Box::leak(format!("<html>{}</html>", "ä".repeat(400)).into_boxed_str());
    let server = CountingServer::always_with_body(200, "content-type: text/html\r\n", page);
    let client = HttpClient::new(config(server.url())).expect("client");
    let excerpt = knobas_http::BodyExcerpt {
        contract: "the Test REST contract",
    };

    let error = client
        .get_json_with::<serde_json::Value, _>("/thing", &[("start", "0")], Some(excerpt))
        .await
        .expect_err("not JSON");
    let SourceError::Protocol { message, status } = &error else {
        panic!("a body of the wrong shape is a protocol fault: {error:?}");
    };
    let start = format!("<html>{}", "ä".repeat(294));
    assert_eq!(
        message,
        &format!(
            "/thing did not answer with the shape the Test REST contract documents: expected \
             value at line 1 column 1; it began {start:?}"
        ),
    );
    assert_eq!(*status, None, "the status was a 200; the fault is the body");

    // Not asked: the same answer keeps the bare decode message.
    let error = client
        .get_json_with::<serde_json::Value, _>("/thing", &[("start", "0")], None)
        .await
        .expect_err("not JSON");
    let SourceError::Protocol { message, .. } = &error else {
        panic!("a body of the wrong shape is a protocol fault: {error:?}");
    };
    assert!(message.starts_with("decoding /thing: "), "{message}");
    assert!(!message.contains("<html>"), "{message}");
    assert!(!message.contains("began"), "{message}");

    // A body shorter than the bound is quoted whole.
    let server = CountingServer::always_with_body(200, "", r#"{"values":"#);
    let client = HttpClient::new(config(server.url())).expect("client");
    let error = client
        .get_json_with::<Vec<u32>, _>("/thing", &[] as &[(&str, &str)], Some(excerpt))
        .await
        .expect_err("cut short");
    assert!(
        matches!(&error, SourceError::Protocol { message, .. }
                 if message.ends_with(r#"; it began "{\"values\":""#)),
        "{error:?}"
    );
}

/// The excerpt is about a body of the wrong shape, not a failing status: a
/// 400 still reads the way [`knobas_http::status_error`] words it, and a body
/// of the right shape decodes.
#[tokio::test(flavor = "multi_thread")]
async fn the_excerpt_leaves_a_failing_status_and_a_good_body_alone() {
    let excerpt = Some(knobas_http::BodyExcerpt {
        contract: "the Test REST contract",
    });
    let server = CountingServer::always_with_body(400, "", "no such field");
    let client = HttpClient::new(config(server.url())).expect("client");
    let error = client
        .get_json_with::<serde_json::Value, _>("/thing", &[] as &[(&str, &str)], excerpt)
        .await
        .expect_err("400");
    assert!(
        matches!(&error, SourceError::Protocol { message, status: Some(400) }
                 if message == "HTTP 400 Bad Request: no such field"),
        "{error:?}"
    );

    let server = CountingServer::always_with_body(200, "", "[1,2,3]");
    let client = HttpClient::new(config(server.url())).expect("client");
    let got = client
        .get_json_with::<Vec<u32>, _>("/thing", &[] as &[(&str, &str)], excerpt)
        .await
        .expect("the right shape");
    assert_eq!(got, vec![1, 2, 3]);
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

/// A server that answers its headers at once and then **dribbles** the body:
/// `pieces` bytes, one every `gap`, then `stall` of silence before the rest
/// (when `stall` is set), on a socket it never closes early.
///
/// Hand-rolled rather than `wiremock`, which can delay a response but cannot
/// write a body slowly: a fixture that answered late but whole would let a
/// total timeout and a stall timeout look the same, and this pair of tests
/// is about telling them apart (issue #1012).
struct DribblingServer {
    port: u16,
    _keep: tokio::task::JoinHandle<()>,
}

impl DribblingServer {
    fn new(pieces: usize, gap: Duration, stall: Option<Duration>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
        let keep = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buffer = [0u8; 2048];
                    let _ = socket.read(&mut buffer).await;
                    // One byte more than the dribble when it stalls: the byte
                    // that only arrives after the silence.
                    let length = pieces + usize::from(stall.is_some());
                    let head = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {length}\r\nconnection: close\r\n\r\n"
                    );
                    if socket.write_all(head.as_bytes()).await.is_err() {
                        return;
                    }
                    for _ in 0..pieces {
                        tokio::time::sleep(gap).await;
                        if socket.write_all(b"x").await.is_err() {
                            return;
                        }
                    }
                    if let Some(stall) = stall {
                        tokio::time::sleep(stall).await;
                        let _ = socket.write_all(b"y").await;
                    }
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self { port, _keep: keep }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

use std::time::Duration;

/// A client whose request timeout is 300 ms.
fn quick_client(url: String) -> HttpClient {
    HttpClient::new(HttpConfig {
        request_timeout: Duration::from_millis(300),
        connect_timeout: Duration::from_millis(300),
        ..config(url)
    })
    .expect("client")
}

/// Ten bytes 80 ms apart: 800 ms in all, well past the 300 ms request
/// timeout, and never 80 ms without a byte.
const PIECES: usize = 10;
const GAP: Duration = Duration::from_millis(80);
const STALL: Duration = Duration::from_millis(250);

/// The fixture's own premise, checked: the plain `send` breaks this body off.
/// Without it, the test below would stay green against a server that is not
/// actually slow (the lesson of a stub that is wrong about the tool).
#[tokio::test(flavor = "multi_thread")]
async fn a_dribbled_body_outlasts_the_request_timeout() {
    let server = DribblingServer::new(PIECES, GAP, None);
    let client = quick_client(server.url());

    let response = client
        .send(client.request(Method::GET, "/big.zip"))
        .await
        .expect("the headers arrive at once");
    let error = response
        .bytes()
        .await
        .expect_err("the total timeout cuts the body");
    assert!(error.is_timeout(), "{error:?}");
}

/// Issue #1012: a body slower in total than the request timeout, but never
/// stalling, arrives whole through the streamed send.
#[tokio::test(flavor = "multi_thread")]
async fn a_streamed_body_slower_than_the_request_timeout_arrives_whole() {
    let server = DribblingServer::new(PIECES, GAP, None);
    let client = quick_client(server.url());

    let started = std::time::Instant::now();
    let mut body = client
        .send_streaming(client.request(Method::GET, "/big.zip"), STALL)
        .await
        .expect("the headers arrive at once");
    let mut kept = Vec::new();
    while let Some(chunk) = body.chunk().await.expect("never stalls") {
        kept.extend_from_slice(&chunk);
    }
    assert_eq!(kept, b"x".repeat(PIECES));
    assert!(
        started.elapsed() > Duration::from_millis(300),
        "the body took longer than the request timeout: {:?}",
        started.elapsed()
    );
}

/// Issue #1012: a body that goes silent for longer than the stall timeout is
/// given up on, even though it would have finished. The adapter reports it as
/// `unreachable` ("… broke off: no bytes for 30s").
#[tokio::test(flavor = "multi_thread")]
async fn a_streamed_body_that_stalls_breaks_off_as_unreachable() {
    // Three bytes, then twice the stall timeout of silence, then the last.
    let server = DribblingServer::new(3, GAP, Some(STALL * 2));
    let client = quick_client(server.url());

    let mut body = client
        .send_streaming(client.request(Method::GET, "/big.zip"), STALL)
        .await
        .expect("the headers arrive at once");
    let mut kept = Vec::new();
    let broke_off = loop {
        match body.chunk().await {
            Ok(Some(chunk)) => kept.extend_from_slice(&chunk),
            Ok(None) => panic!("the stalled body read as whole: {kept:?}"),
            Err(broke_off) => break broke_off,
        }
    };
    assert_eq!(broke_off, knobas_http::BodyBrokeOff::Stalled(STALL));
    assert_eq!(kept, b"xxx", "what came before the stall did arrive");
}

/// The streamed send still bounds the wait for the headers: a server that
/// never answers is `unreachable`, not a call that hangs.
#[tokio::test(flavor = "multi_thread")]
async fn a_streamed_send_with_no_headers_is_unreachable() {
    let server = SilentServer::new();
    let client = quick_client(server.url());

    // Bounded from outside, so a streamed send that lost its headers timeout
    // fails here instead of hanging the suite: three 300 ms attempts and
    // their backoff end well inside 10 s.
    let error = tokio::time::timeout(
        Duration::from_secs(10),
        client.send_streaming(client.request(Method::GET, "/big.zip"), STALL),
    )
    .await
    .expect("the headers timeout ends the call")
    .expect_err("nothing ever answers");
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
}

/// The `Authorization` header a client built over `auth` sends, if any.
fn authorization_of(auth: Auth) -> Option<String> {
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

/// What `credential` should hand back: the header the client then sends, or
/// the refusal's class and its whole message.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Header(String),
    Unauthorized,
    Protocol(String),
}

fn outcome(result: Result<Auth, SourceError>) -> Outcome {
    match result {
        Ok(auth) => {
            Outcome::Header(authorization_of(auth).expect("every credential is sent as a header"))
        }
        Err(SourceError::Unauthorized { status: None }) => Outcome::Unauthorized,
        Err(SourceError::Protocol {
            status: None,
            message,
        }) => Outcome::Protocol(message),
        Err(other) => panic!("a credential is refused as Unauthorized or Protocol: {other:?}"),
    }
}

/// Spec #1302, issue #1310: one mapping from a source's configured method,
/// username and keychain secret to the header the transport sends, for every
/// built-in adapter. Each row is a literal taken from the adapter copy it
/// replaces, so the wording a credential health line shows cannot drift.
#[test]
fn a_configured_method_maps_to_one_credential_or_one_refusal() {
    use knobas_http::{CredentialRules, TokenSpelling, credential};
    use knobas_source::AuthMethod::{ApiToken, OAuth, Pat, UserPassword};

    // Jira Data Center's rules, as `knobas-source-jira` states them.
    const JIRA: CredentialRules = CredentialRules {
        pat: TokenSpelling::Bearer,
        methods: &[Pat, UserPassword],
        unconfigured: "this Jira source has no authentication method configured; choose a \
                       personal access token or user + password",
        unsupported: |method| {
            format!(
                "{method:?} authentication is not supported for Jira Data Center; use a \
                 personal access token or user + password"
            )
        },
    };
    // Gitea's: a PAT only, sent in Gitea's own `token` scheme.
    const GITEA: CredentialRules = CredentialRules {
        pat: TokenSpelling::Token,
        methods: &[Pat],
        unconfigured: "gitea: this source has no authentication method configured; choose a \
                       personal access token",
        unsupported: |method| {
            format!(
                "gitea: {method:?} authentication is not supported; this adapter authenticates \
                 with a personal access token"
            )
        },
    };
    // Uptime Kuma's: an API key, which `/metrics` takes as Basic with an
    // empty username.
    const KUMA: CredentialRules = CredentialRules {
        pat: TokenSpelling::Bearer,
        methods: &[ApiToken],
        unconfigured: "this Uptime Kuma source has no authentication method configured; choose \
                       an API key",
        unsupported: |method| {
            format!(
                "{method:?} authentication is not supported for Uptime Kuma; /metrics takes an \
                 API key, sent as HTTP Basic with an empty username"
            )
        },
    };
    // Rules that list every method, OAuth included, which has no wire form.
    const EVERY: CredentialRules = CredentialRules {
        pat: TokenSpelling::Bearer,
        methods: &[Pat, UserPassword, ApiToken, OAuth],
        unconfigured: "no method",
        unsupported: |method| format!("{method:?} has no wire form"),
    };

    let header = |value: &str| Outcome::Header(value.to_owned());
    let protocol = |message: &str| Outcome::Protocol(message.to_owned());
    let no_username =
        protocol("user + password authentication needs a username in the source configuration");
    let jira_unconfigured = protocol(JIRA.unconfigured);
    let jira_unsupported = |method| Outcome::Protocol((JIRA.unsupported)(method));

    #[rustfmt::skip]
    let table = [
        // rules, method, username, secret => outcome
        // Every method that maps, in both token spellings.
        (&JIRA,  Some(Pat),          None,         Some("t"),  header("Bearer t")),
        (&GITEA, Some(Pat),          None,         Some("t"),  header("token t")),
        (&JIRA,  Some(UserPassword), Some("mara"), Some("pw"), header("Basic bWFyYTpwdw==")),
        (&KUMA,  Some(ApiToken),     None,         Some("k"),  header("Basic Oms=")),
        // An API key's Basic username is empty whatever the configuration
        // holds; a PAT's username beside it is ignored, not sent.
        (&KUMA,  Some(ApiToken),     Some("mara"), Some("k"),  header("Basic Oms=")),
        (&JIRA,  Some(Pat),          Some("mara"), Some("t"),  header("Bearer t")),
        // Refusal one: a well-configured source with no secret is the
        // `missing_secret` state, the one the sources view offers Re-enter for.
        (&JIRA,  Some(Pat),          None,         None,       Outcome::Unauthorized),
        (&GITEA, Some(Pat),          None,         None,       Outcome::Unauthorized),
        (&JIRA,  Some(UserPassword), Some("mara"), None,       Outcome::Unauthorized),
        (&KUMA,  Some(ApiToken),     None,         None,       Outcome::Unauthorized),
        // Refusal two: a method the adapter does not speak, by name. OAuth has
        // no wire form at all, and is refused even by rules that list it.
        (&JIRA,  Some(OAuth),        Some("mara"), Some("t"),  jira_unsupported(OAuth)),
        (&JIRA,  Some(ApiToken),     Some("mara"), Some("t"),  jira_unsupported(ApiToken)),
        (&GITEA, Some(UserPassword), Some("mara"), Some("t"),  protocol("gitea: UserPassword authentication is not supported; this adapter authenticates with a personal access token")),
        (&KUMA,  Some(Pat),          None,         Some("k"),  protocol("Pat authentication is not supported for Uptime Kuma; /metrics takes an API key, sent as HTTP Basic with an empty username")),
        (&EVERY, Some(OAuth),        Some("mara"), Some("t"),  protocol("OAuth has no wire form")),
        // Refusal three: a misconfigured source -- no method, or user +
        // password with no username.
        (&JIRA,  None,               None,         Some("t"),  jira_unconfigured.clone()),
        (&GITEA, None,               None,         Some("t"),  protocol(GITEA.unconfigured)),
        (&JIRA,  Some(UserPassword), None,         Some("pw"), no_username.clone()),
        // The configuration outranks the secret: retyping a token cannot fix a
        // source with no method chosen, so Re-enter must not be offered for it.
        (&JIRA,  None,               None,         None,       jira_unconfigured),
        (&JIRA,  Some(OAuth),        Some("mara"), None,       jira_unsupported(OAuth)),
        (&JIRA,  Some(UserPassword), None,         None,       no_username),
    ];

    for (rules, method, username, secret, expected) in table {
        assert_eq!(
            outcome(credential(rules, method, username, secret)),
            expected,
            "{method:?} / {username:?} / secret {:?}",
            secret.is_some(),
        );
    }
}

// ---- The page walk (#1315) ------------------------------------------------

mod walk {
    use std::cell::RefCell;

    use knobas_http::{NextLink, Offset, PageNumber, PageWalk, Paged, Walked};
    use knobas_source::SourceError;

    /// A page as a stand-in server answers it: its rows, and what it claims
    /// about the rest (a total, a continuation link).
    #[derive(Debug, Clone, PartialEq)]
    struct Page {
        rows: Vec<u32>,
        total: Option<usize>,
        next: Option<String>,
    }

    impl Paged for Page {
        fn rows(&self) -> usize {
            self.rows.len()
        }
        fn total(&self) -> Option<usize> {
            self.total
        }
        fn next_link(&self) -> Option<&str> {
            self.next.as_deref()
        }
    }

    fn rows(rows: &[u32]) -> Page {
        Page {
            rows: rows.to_vec(),
            total: None,
            next: None,
        }
    }

    /// Every page the walk hands out, then how it ended.
    async fn drain<P, F, Fut>(mut walk: PageWalk<P, F>) -> (Vec<Vec<u32>>, String)
    where
        P: knobas_http::Protocol,
        F: FnMut(P::At) -> Fut,
        Fut: std::future::Future<Output = Result<Page, SourceError>>,
    {
        let mut read = Vec::new();
        loop {
            match walk.next().await {
                Ok(Walked::Page(page)) => read.push(page.rows),
                Ok(Walked::End) => return (read, "end".to_owned()),
                Ok(Walked::Capped) => return (read, "capped".to_owned()),
                Err(error) => return (read, error.to_string()),
            }
        }
    }

    /// Page number plus limit (Gitea): a short page is not the end -- a
    /// server may cap a page below the limit asked for -- and an empty one is.
    #[tokio::test]
    async fn a_page_number_walk_continues_past_a_short_page_and_ends_on_an_empty_one() {
        let pages = [rows(&[1, 2]), rows(&[3]), rows(&[4, 5]), rows(&[])];
        let asked = RefCell::new(Vec::new());
        let walk = PageWalk::new(PageNumber, 10, |page: u32| {
            asked.borrow_mut().push(page);
            let answer = pages[page as usize - 1].clone();
            async move { Ok(answer) }
        });
        let (read, end) = drain(walk).await;
        assert_eq!(read, vec![vec![1, 2], vec![3], vec![4, 5]]);
        assert_eq!(end, "end");
        assert_eq!(
            *asked.borrow(),
            vec![1, 2, 3, 4],
            "one request for the empty page"
        );
    }

    /// The cap: the walk hands out at most `cap` pages, and when it would ask
    /// for one more it answers `Capped`, not `End` -- a walk promising a whole
    /// corpus must be able to tell the two apart. A listing that ends exactly
    /// at the cap ends, rather than reading as capped.
    #[tokio::test]
    async fn a_walk_answers_capped_when_it_would_ask_past_its_cap() {
        let asked = RefCell::new(0_u32);
        let endless = PageWalk::new(PageNumber, 3, |page: u32| {
            *asked.borrow_mut() += 1;
            async move { Ok(rows(&[page])) }
        });
        let (read, end) = drain(endless).await;
        assert_eq!(read, vec![vec![1], vec![2], vec![3]]);
        assert_eq!(end, "capped");
        assert_eq!(*asked.borrow(), 3, "no request past the cap");

        let exact = PageWalk::new(PageNumber, 2, |page: u32| async move {
            Ok(if page <= 2 { rows(&[page]) } else { rows(&[]) })
        });
        assert_eq!(
            drain(exact).await.1,
            "capped",
            "a full last page is not proof of the end"
        );

        let total = PageWalk::new(PageNumber, 2, |page: u32| async move {
            Ok(Page {
                total: Some(2),
                ..rows(&[page])
            })
        });
        assert_eq!(
            drain(total).await,
            (vec![vec![1], vec![2]], "end".to_owned())
        );
    }

    /// Offset plus total (Jira): each request starts where the records read so
    /// far end, a short page continues, and reaching the reported total ends
    /// the walk without asking for the empty page.
    #[tokio::test]
    async fn an_offset_walk_starts_each_page_after_the_records_read_and_stops_at_the_total() {
        let asked = RefCell::new(Vec::new());
        let walk = PageWalk::new(Offset, 10, |start: u32| {
            asked.borrow_mut().push(start);
            let page = match start {
                0 => vec![1, 2],
                2 => vec![3],
                3 => vec![4, 5],
                _ => vec![],
            };
            async move {
                Ok(Page {
                    total: Some(5),
                    ..rows(&page)
                })
            }
        });
        let (read, end) = drain(walk).await;
        assert_eq!(read, vec![vec![1, 2], vec![3], vec![4, 5]]);
        assert_eq!(end, "end");
        assert_eq!(*asked.borrow(), vec![0, 2, 3]);

        // A total the server inflates still ends on the empty page.
        let inflated = PageWalk::new(Offset, 10, |start: u32| async move {
            Ok(Page {
                total: Some(99),
                ..rows(if start == 0 { &[1] } else { &[] })
            })
        });
        assert_eq!(drain(inflated).await, (vec![vec![1]], "end".to_owned()));
    }

    /// Next link (Confluence): the first request carries no link, each later
    /// one the link the page before offered, an empty page with a link is
    /// followed, and a page without one ends the walk.
    #[tokio::test]
    async fn a_next_link_walk_follows_the_link_through_an_empty_page() {
        let asked = RefCell::new(Vec::new());
        let walk = PageWalk::new(NextLink { product: "Wiki" }, 10, |link: Option<String>| {
            asked.borrow_mut().push(link.clone());
            let page = match link.as_deref() {
                None => Page {
                    next: Some("/p2".to_owned()),
                    ..rows(&[1])
                },
                Some("/p2") => Page {
                    next: Some("/p3".to_owned()),
                    ..rows(&[])
                },
                _ => rows(&[2]),
            };
            async move { Ok(page) }
        });
        let (read, end) = drain(walk).await;
        assert_eq!(read, vec![vec![1], vec![], vec![2]]);
        assert_eq!(end, "end");
        assert_eq!(
            *asked.borrow(),
            vec![None, Some("/p2".to_owned()), Some("/p3".to_owned())]
        );
    }

    /// An absolute continuation link is refused, not followed and not read as
    /// the end -- after the page that offered it is handed out.
    #[tokio::test]
    async fn an_absolute_next_link_is_refused_after_its_page() {
        let asked = RefCell::new(0_u32);
        let walk = PageWalk::new(NextLink { product: "Wiki" }, 10, |_: Option<String>| {
            *asked.borrow_mut() += 1;
            async move {
                Ok(Page {
                    next: Some("https://elsewhere.example/p2".to_owned()),
                    ..rows(&[1])
                })
            }
        });
        let (read, end) = drain(walk).await;
        assert_eq!(read, vec![vec![1]]);
        assert_eq!(
            end,
            "protocol: Wiki answered with a continuation link this adapter cannot follow: \
             \"https://elsewhere.example/p2\" is not a path rooted at the instance. Refusing \
             rather than reporting a walk that stopped early as a completed one."
        );
        assert_eq!(*asked.borrow(), 1);
    }

    /// A fetch's failure is the walk's.
    #[tokio::test]
    async fn a_failing_fetch_fails_the_walk() {
        let walk = PageWalk::new(PageNumber, 10, |_: u32| async move {
            Err::<Page, _>(SourceError::protocol("down"))
        });
        assert_eq!(drain(walk).await, (vec![], "protocol: down".to_owned()));
    }
}
