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

/// A URL nothing answers on (#1815, #1831): port 1 on the loopback, refused at
/// once. A port bound to `0` and dropped is free again at once, and another
/// test's server can be handed it. Port 1 is below every ephemeral range and
/// privileged on Linux; a bound, unlistened port instead makes macOS connects
/// wait out their timeout. Checked refused once per process.
fn dead_url() -> String {
    static REFUSED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    REFUSED.get_or_init(|| {
        let refused = std::net::TcpStream::connect(("127.0.0.1", 1))
            .is_err_and(|error| error.kind() == std::io::ErrorKind::ConnectionRefused);
        assert!(
            refused,
            "something answers on 127.0.0.1:1, the port dead_url names"
        );
    });
    "http://127.0.0.1:1".to_owned()
}

/// §4.1: connect/DNS/TLS/timeout are `Unreachable` -- the class the scheduler
/// backs off on, as opposed to the one that needs a human.
#[tokio::test]
async fn a_refused_connection_is_unreachable() {
    let client = HttpClient::new(config(dead_url())).expect("client");
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
        // A password in the base URL itself (#1640): refused by `new`, and
        // still never printed by the config that carries it.
        format!(
            "{:?}",
            config(format!("https://someone:{SECRET}@example.test"))
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
    /// How many exchanges are over: answered, and the connection closed by
    /// the client once it had read the answer ([`Self::exchanges_over`]).
    over: tokio::sync::watch::Receiver<usize>,
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
        let (ended, over) = tokio::sync::watch::channel(0);
        let ended = Arc::new(ended);
        tauri_free_spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let counter = Arc::clone(&counter);
                let ended = Arc::clone(&ended);
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
                    // The client hangs up once it has read the answer (it
                    // was told `connection: close`): only then is nothing of
                    // the exchange left on the wire.
                    while socket.read(&mut buffer).await.is_ok_and(|read| read > 0) {}
                    ended.send_modify(|over| *over += 1);
                });
            }
        });
        Self { port, hits, over }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }

    /// Wait until `count` exchanges are over: each answered, and its
    /// connection closed by the client after reading the answer. Then
    /// nothing those requests started is still on the wire, and the
    /// [`HeldClock`] can be moved or let go without a timer firing under a
    /// request still in flight.
    async fn exchanges_over(&self, count: usize) {
        let mut over = self.over.clone();
        real_backstop(&format!("{count} exchanges with the server"), async {
            over.wait_for(|over| *over >= count)
                .await
                .expect("the server outlives its handle");
        })
        .await;
    }
}

/// tokio's paused clock, held still until this is dropped (#1925).
///
/// A quota or a budget is a duration, so a test of one asserts on a clock,
/// and the wall clock moves under load. These tests run on tokio's paused
/// clock instead, which the limiter, the budget and every timeout read
/// alike. Left to itself a paused clock jumps to the next timer whenever the
/// runtime is idle, and a runtime waiting on a socket is idle: a request in
/// flight would see its own timeout fire. A running blocking task stops the
/// jumps (`tokio::time::pause`, *Preventing auto-advance*), so while this is
/// held the clock moves only by `tokio::time::advance`, and a request on the
/// loopback takes no time at all on it.
///
/// Dropped, the clock jumps from timer to timer again: what a test does once
/// nothing is left on the wire ([`CountingServer::exchanges_over`]).
struct HeldClock {
    _release: std::sync::mpsc::Sender<()>,
}

impl HeldClock {
    fn hold() -> Self {
        let (release, held) = std::sync::mpsc::channel::<()>();
        tokio::task::spawn_blocking(move || {
            // Returns once the sender is dropped: that is the release.
            let _ = held.recv();
        });
        Self { _release: release }
    }
}

/// `wait`, or a panic naming `what` once twenty seconds of *real* time have
/// gone by. A held clock never fires a timer on its own, so a wait for
/// something a broken quota never lets happen would hang the run instead of
/// failing it. A hang backstop, not a bound: nothing a correct quota does
/// takes real time worth naming.
async fn real_backstop<F: std::future::Future>(what: &str, wait: F) -> F::Output {
    let (fired, backstop) = tokio::sync::oneshot::channel::<()>();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(20));
        let _ = fired.send(());
    });
    tokio::select! {
        output = wait => output,
        _ = backstop => panic!("{what} never came, twenty seconds of real time on"),
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

/// **The request counter sees what the server sees** (#2239): a call the
/// retry budget repeats counts every attempt, a call outside a counted
/// scope is in no count, and two scopes count apart.
#[tokio::test(flavor = "multi_thread")]
async fn the_request_counter_counts_what_reached_the_server() {
    let server = CountingServer::always(503, "");
    let client = HttpClient::new(config(server.url())).expect("client");

    let (_, outside) = knobas_http::counted::counted(async {}).await;
    let _ = client.get_json::<serde_json::Value>("/thing", &[]).await;
    let (answer, sent) =
        knobas_http::counted::counted(client.get_json::<serde_json::Value>("/thing", &[])).await;
    assert!(answer.is_err(), "503 every time");
    assert_eq!(outside, 0);
    assert_eq!(
        sent,
        knobas_http::MAX_ATTEMPTS as u64,
        "one count per attempt"
    );
    assert_eq!(
        server.hits(),
        2 * knobas_http::MAX_ATTEMPTS as usize,
        "the uncounted call reached the server too"
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
const SHORT_BUDGET: Duration = Duration::from_millis(1500);

/// What one send ended in, and when on the [`HeldClock`].
type Ended = (Result<u16, SourceError>, Duration);

/// `per_client` sends of `GET /x` started at once on each of `clients`, on
/// the held clock. Every one of them has been polled once it returns, so each
/// started at the same instant: what the quota hands out at once has gone to
/// the wire, and the rest wait on the limiter.
async fn start_sends(
    clients: Vec<Arc<HttpClient>>,
    per_client: usize,
) -> tokio::task::JoinSet<Ended> {
    let mut sends = tokio::task::JoinSet::new();
    for client in clients {
        for _ in 0..per_client {
            let client = Arc::clone(&client);
            sends.spawn(async move {
                let started = tokio::time::Instant::now();
                let outcome = client
                    .send(client.request(Method::GET, "/x"))
                    .await
                    .map(|response| response.status().as_u16());
                (outcome, started.elapsed())
            });
        }
    }
    tokio::task::yield_now().await;
    sends
}

/// What every send ended in, once `clock` is let go. Called once nothing is
/// left on the wire, so the clock jumps only between timers no request is
/// under.
async fn ended(clock: HeldClock, sends: tokio::task::JoinSet<Ended>) -> Vec<Ended> {
    drop(clock);
    real_backstop("the end of every send", sends.join_all()).await
}

/// How many of `outcomes` went: answered `200`.
fn went(outcomes: &[Ended]) -> usize {
    outcomes
        .iter()
        .filter(|(outcome, _)| matches!(outcome, Ok(200)))
        .count()
}

/// Every one of `outcomes` that did not go stopped on the local quota: a
/// status-less `Protocol` naming it, which no request that reached the
/// instance ends in.
fn the_rest_stopped_on_the_quota(outcomes: &[Ended], quota: &str) {
    for (outcome, _) in outcomes.iter().filter(|(outcome, _)| outcome.is_err()) {
        let Err(SourceError::Protocol {
            status: None,
            message,
        }) = outcome
        else {
            panic!("a wait on the local quota must be Protocol, never Unreachable: {outcome:?}");
        };
        assert!(
            message.contains("local rate limit") && message.contains(quota),
            "the message names the quota it waited on: {message}"
        );
    }
}

/// `count` sends queued at once on a client for `server` at one a second
/// with a burst of one and [`SHORT_BUDGET`], on the held clock: the first
/// goes at t=0, the second at t=1 s, and the budget runs out at 1.5 s.
async fn queued_behind_one_a_second(server: &CountingServer, count: usize) -> Vec<Ended> {
    let clock = HeldClock::hold();
    let client = Arc::new(paced_client(server, 1).with_send_budget(SHORT_BUDGET));
    let sends = start_sends(vec![client], count).await;
    // Past the token at 1 s and short of the budget, off both instants, so
    // neither timer fires on the one the other does.
    const SECOND_GOES: Duration = Duration::from_millis(1200);
    server.exchanges_over(1).await;
    tokio::time::advance(SECOND_GOES).await;
    server.exchanges_over(2).await;
    tokio::time::advance(SHORT_BUDGET - SECOND_GOES).await;
    ended(clock, sends).await
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
#[tokio::test(start_paused = true)]
async fn a_limiter_wait_that_outlasts_the_budget_is_the_local_quota() {
    let server = CountingServer::always(200, "");

    let outcomes = queued_behind_one_a_second(&server, 5).await;

    assert_eq!(
        went(&outcomes),
        2,
        "t=0 and t=1s fit the budget: {outcomes:?}"
    );
    assert_eq!(
        server.hits(),
        2,
        "nothing past the budget reached the server"
    );
    the_rest_stopped_on_the_quota(&outcomes, "1 request a second");
}

/// **The limiter wait never outlasts what is left of the budget** -- the
/// limiter's half of [`SEND_BUDGET`](knobas_http::SEND_BUDGET), beside
/// `a_retry_after_longer_than_the_budget_ends_the_call`'s half for the waits
/// between attempts.
///
/// Twelve sends queued at once behind one a second. Unbounded, they end at
/// t=0, 1, 2, ... 11 s; bounded, none ends later than the 1.5 s budget. The
/// subject is a duration, and the clock is tokio's, held (#1925): the bound
/// is the budget itself, with no slack, because load cannot move that clock.
///
/// The retry half: a `503` at t=0 asks for a second attempt, which cannot
/// pass the limiter before the budget runs out, so the call ends with the
/// `503` it had -- what the source said -- and not with the quota, which only
/// kept it from asking again. It is not timed: an unbounded wait there ends
/// at t=1 s and then finds no budget left to send in, which ends the call
/// with the same `503`. The bound on that wait is the first half's, since
/// every attempt waits through the same call with what is left of its budget.
#[tokio::test(start_paused = true)]
async fn a_limiter_wait_never_outlasts_the_budget() {
    let server = CountingServer::always(200, "");
    for (outcome, took) in queued_behind_one_a_second(&server, 12).await {
        assert!(
            took <= SHORT_BUDGET,
            "a send ended {took:?} in, past the {SHORT_BUDGET:?} budget: {outcome:?}"
        );
    }

    let busy = CountingServer::always(503, "");
    let clock = HeldClock::hold();
    let client = paced_client(&busy, 1).with_send_budget(Duration::from_millis(800));
    let (error, ()) = tokio::join!(client.send(client.request(Method::GET, "/x")), async {
        busy.exchanges_over(1).await;
        drop(clock);
    });
    let error = error.expect_err("503, then no room for a second attempt");
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
#[tokio::test(start_paused = true)]
async fn a_watched_call_the_quota_stopped_is_marked() {
    let server = CountingServer::always(200, "");
    let clock = HeldClock::hold();
    let client = paced_client(&server, 1).with_send_budget(Duration::from_millis(300));

    let (went, quota) =
        knobas_http::watch_local_quota(client.send(client.request(Method::GET, "/x"))).await;
    assert!(went.is_ok(), "the burst lets the first through: {went:?}");
    assert_eq!(quota, None, "a call that went is not marked");
    drop(went);
    server.exchanges_over(1).await;
    drop(clock);

    let (stopped, quota) =
        knobas_http::watch_local_quota(client.send(client.request(Method::GET, "/x"))).await;
    assert!(
        matches!(stopped, Err(SourceError::Protocol { status: None, .. })),
        "{stopped:?}"
    );
    let quota = quota.expect("the quota that stopped the call is handed back");
    assert!(quota.to_string().contains("1 request a second"), "{quota}");
    assert_eq!(
        server.hits(),
        1,
        "the stopped call never reached the server"
    );

    let ((), quota) = knobas_http::watch_local_quota(async {}).await;
    assert_eq!(quota, None, "each watch starts unmarked");
}

/// **A retry the quota stopped is not marked** (#1729): the call ends with
/// what the source said on the attempt before, a `503` here, and that answer
/// is what the write queue reads. Only a call that never reached the source
/// carries the mark.
#[tokio::test(start_paused = true)]
async fn a_retry_the_quota_stopped_is_not_marked() {
    let busy = CountingServer::always(503, "");
    let clock = HeldClock::hold();
    let client = paced_client(&busy, 1).with_send_budget(Duration::from_millis(800));

    let ((answered, quota), ()) = tokio::join!(
        knobas_http::watch_local_quota(client.send(client.request(Method::GET, "/x"))),
        async {
            busy.exchanges_over(1).await;
            drop(clock);
        }
    );

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

/// The budget the burst tests run on: shorter than the second in which a
/// one-a-second quota adds a token, so what goes is the burst and nothing
/// the refill adds.
const UNDER_A_REFILL: Duration = Duration::from_millis(500);

/// A client for `server` at one request a second and the given burst.
fn paced_client(server: &CountingServer, burst: u32) -> HttpClient {
    client_at(server, 1, burst)
}

/// **Every client built for one instance shares its quota** (SPI-2, #1630).
///
/// A sync run, a live read and a flush each build their own client, and they
/// run at once. Two clients for one origin at a burst of ten, ten sends each,
/// inside a budget shorter than one refill: the instance sees ten, not
/// twenty, and the other ten stop on the quota.
#[tokio::test(start_paused = true)]
async fn two_clients_for_one_instance_share_one_burst() {
    let server = CountingServer::always(200, "");
    let clock = HeldClock::hold();
    let clients = [paced_client(&server, 10), paced_client(&server, 10)]
        .map(|client| Arc::new(client.with_send_budget(UNDER_A_REFILL)));

    let sends = start_sends(clients.into(), 10).await;
    server.exchanges_over(10).await;
    let outcomes = ended(clock, sends).await;

    assert_eq!(
        went(&outcomes),
        10,
        "{outcomes:?}: each client had a limiter of its own"
    );
    assert_eq!(server.hits(), 10, "the instance saw only the shared burst");
    the_rest_stopped_on_the_quota(&outcomes, "burst 10");
}

/// Ten sends on `loose` at once, inside a budget shorter than one refill:
/// what went, once the `burst` that governs has gone to `server`.
async fn ten_on_the_loose_client(
    server: &CountingServer,
    loose: HttpClient,
    burst: usize,
    clock: HeldClock,
) -> Vec<Ended> {
    let sends = start_sends(vec![Arc::new(loose.with_send_budget(UNDER_A_REFILL))], 10).await;
    server.exchanges_over(burst).await;
    ended(clock, sends).await
}

/// **Where two configurations name different quotas for one instance, the
/// stricter governs**, in either order (#1630).
///
/// A burst of ten and a burst of two at one origin: the instance sees two
/// within a budget shorter than one refill, whichever client the sends go
/// through and whichever was built first. Built loose-first, the stricter one
/// tightens the quota under the client that already holds it -- which is why
/// the sends go through the loose client in both halves.
#[tokio::test(start_paused = true)]
async fn the_stricter_of_two_quotas_for_one_instance_governs() {
    let strict_first = CountingServer::always(200, "");
    let clock = HeldClock::hold();
    let _strict = paced_client(&strict_first, 2);
    let loose = paced_client(&strict_first, 10);
    let outcomes = ten_on_the_loose_client(&strict_first, loose, 2, clock).await;
    assert_eq!(
        went(&outcomes),
        2,
        "strict first: {outcomes:?}; the looser burst of 10 won"
    );
    assert_eq!(strict_first.hits(), 2, "strict first");
    the_rest_stopped_on_the_quota(&outcomes, "burst 2");

    let loose_first = CountingServer::always(200, "");
    let clock = HeldClock::hold();
    let loose = paced_client(&loose_first, 10);
    let _strict = paced_client(&loose_first, 2);
    let outcomes = ten_on_the_loose_client(&loose_first, loose, 2, clock).await;
    assert_eq!(
        went(&outcomes),
        2,
        "loose first: {outcomes:?}; the stricter burst of 2 was ignored"
    );
    assert_eq!(loose_first.hits(), 2, "loose first");
    the_rest_stopped_on_the_quota(&outcomes, "burst 2");
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
/// one. Built right after the strict client, the loose limiter starts
/// drained (the test below), so 525 ms in it has refilled ten at twenty a
/// second. Held to the old quota, one request a second would go.
#[tokio::test(start_paused = true)]
async fn a_quota_loosens_once_the_stricter_client_is_gone() {
    let server = CountingServer::always(200, "");
    let clock = HeldClock::hold();
    drop(client_at(&server, 1, 1));
    const BUDGET: Duration = Duration::from_secs(1);
    // Ten refills of 50 ms and half of the eleventh, off every refill
    // instant.
    const TEN_REFILLED: Duration = Duration::from_millis(525);
    let loose = Arc::new(client_at(&server, 20, 20).with_send_budget(BUDGET));

    let sends = start_sends(vec![loose], 20).await;
    tokio::time::advance(TEN_REFILLED).await;
    server.exchanges_over(10).await;
    tokio::time::advance(BUDGET - TEN_REFILLED).await;
    let outcomes = ended(clock, sends).await;

    assert_eq!(
        went(&outcomes),
        10,
        "{outcomes:?}: the dropped client's one a second still governs"
    );
    the_rest_stopped_on_the_quota(&outcomes, "20 requests a second");
}

/// The budget the two-a-second tests run on: past the one refill at 500 ms,
/// short of the second at 1 s.
const TWO_A_SECOND_WINDOW: Duration = Duration::from_millis(900);

/// Where those tests move the clock first: past the refill at 500 ms and
/// short of the budget, off both instants.
const ONE_REFILLED: Duration = Duration::from_millis(700);

/// **A loosening hands out no fresh burst** (#1728).
///
/// Two sources on one server at different quotas, syncing one after the
/// other, would otherwise get a new burst on every alternation. The limiter
/// built for a loosening starts with what its rate refilled since the
/// origin's last request (#1771), which right after the strict client is
/// nothing: at two a second and a burst of twenty, what reaches the instance
/// in the first 900 ms is the one request the refill allows at 500 ms, not
/// twenty. That the quota loosened at all is the test above's; this one is
/// that the loosening sent no burst.
#[tokio::test(start_paused = true)]
async fn a_loosening_right_away_starts_drained() {
    const BUDGET: Duration = TWO_A_SECOND_WINDOW;
    let server = CountingServer::always(200, "");
    let clock = HeldClock::hold();
    drop(client_at(&server, 1, 1));
    let loose = Arc::new(client_at(&server, 2, 20).with_send_budget(BUDGET));

    let sends = start_sends(vec![loose], 20).await;
    tokio::time::advance(ONE_REFILLED).await;
    server.exchanges_over(1).await;
    tokio::time::advance(BUDGET - ONE_REFILLED).await;
    let outcomes = ended(clock, sends).await;

    assert_eq!(
        went(&outcomes),
        1,
        "{outcomes:?} right after a loosening: the new limiter handed out its burst"
    );
    assert_eq!(server.hits(), 1);
    the_rest_stopped_on_the_quota(&outcomes, "2 requests a second");
}

/// **A tightening after a loosening hands out no fresh burst either**
/// (#1740).
///
/// Two sources on one server at two a second (burst six) and four a second
/// (burst twelve), their clients never alive at once: the strict one spends
/// its burst, the loose one is built and dropped, and the strict one is
/// built again. Had the strict quota governed throughout, the 900 ms after
/// that would see what two a second refills, the one request at 500 ms.
/// Built with its own burst, the strict limiter sends six more.
#[tokio::test(start_paused = true)]
async fn a_tightening_after_a_loosening_starts_with_what_was_left() {
    const BUDGET: Duration = TWO_A_SECOND_WINDOW;
    let server = CountingServer::always(200, "");
    let clock = HeldClock::hold();
    let strict = client_at(&server, 2, 6);
    for _ in 0..6 {
        let _ = strict.send(strict.request(Method::GET, "/x")).await;
    }
    drop(strict);
    drop(client_at(&server, 4, 12));
    let spent = server.hits();
    server.exchanges_over(spent).await;

    let strict_again = Arc::new(client_at(&server, 2, 6).with_send_budget(BUDGET));
    let sends = start_sends(vec![strict_again], 6).await;
    tokio::time::advance(ONE_REFILLED).await;
    server.exchanges_over(spent + 1).await;
    tokio::time::advance(BUDGET - ONE_REFILLED).await;
    let outcomes = ended(clock, sends).await;

    assert_eq!(spent, 6, "the strict burst went out before the alternation");
    assert_eq!(
        went(&outcomes),
        1,
        "{outcomes:?} after the strict quota came back: the tightening handed out a \
         fresh burst"
    );
    assert_eq!(server.hits() - spent, 1);
    the_rest_stopped_on_the_quota(&outcomes, "2 requests a second");
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
        (error, server.hits().await)
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
    let client = HttpClient::new(config(dead_url())).expect("client");

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
    // The one dead URL that must be rebound, so it cannot be port 1.
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

/// A server that accepts connections and never replies, reporting each one's
/// peer in the order it accepted them.
struct SilentServer {
    port: u16,
    accepted: tokio::sync::watch::Receiver<Vec<std::net::SocketAddr>>,
    _keep: tokio::task::JoinHandle<()>,
}

impl SilentServer {
    fn new() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
        let (report, accepted) = tokio::sync::watch::channel(Vec::new());
        let keep = tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, peer)) = listener.accept().await {
                report.send_modify(|peers| peers.push(peer));
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

    /// Connections opened before this call, counted once the server has
    /// accepted every one of them (#1939). A timed-out request's connection
    /// is never reused, so this is the number of attempts.
    ///
    /// The client's connections wait in the listen queue until the accept
    /// loop takes them, which can be after the client has timed out and
    /// returned. A probe opened now joins that queue behind them, and the
    /// queue hands connections out in arrival order: once the server
    /// reports the probe accepted, it has accepted everything before it,
    /// and the probe's place in the order is the count.
    async fn hits(&self) -> usize {
        let probe_stream = tokio::net::TcpStream::connect(("127.0.0.1", self.port))
            .await
            .expect("the probe connects");
        let probe = probe_stream.local_addr().expect("probe address");
        let mut accepted = self.accepted.clone();
        let peers = real_backstop(
            "the server's accept of the probe",
            accepted.wait_for(|peers| peers.contains(&probe)),
        )
        .await
        .expect("the server outlives its handle");
        peers
            .iter()
            .position(|peer| *peer == probe)
            .expect("waited for the probe")
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
///
/// Its gaps and its stall are `tokio::time::sleep`s on the test's runtime, so
/// they pass on the same paused clock as the timeouts they are measured
/// against (#1935), and it says what it is doing ([`Dribbled`]): the test
/// moves the clock only while the server holds a byte back and every byte
/// it wrote has been read ([`DribblingServer::quiet_after`]).
struct DribblingServer {
    port: u16,
    dribbled: tokio::sync::watch::Receiver<Dribbled>,
    _keep: tokio::task::JoinHandle<()>,
}

/// What the [`DribblingServer`] has done so far.
#[derive(Clone, Copy, Default)]
struct Dribbled {
    /// The holds it began: a gap before each piece, and the stall.
    holds: usize,
    /// When the current hold ends, on tokio's clock.
    until: Option<tokio::time::Instant>,
    /// Body bytes written.
    written: usize,
}

impl DribblingServer {
    fn new(pieces: usize, gap: Duration, stall: Option<Duration>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        listener.set_nonblocking(true).expect("nonblocking");
        let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
        let (said, dribbled) = tokio::sync::watch::channel(Dribbled::default());
        let said = Arc::new(said);
        let keep = tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let said = Arc::clone(&said);
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let hold = |length: Duration| {
                        let until = tokio::time::Instant::now() + length;
                        said.send_modify(|dribbled| {
                            dribbled.holds += 1;
                            dribbled.until = Some(until);
                        });
                        tokio::time::sleep_until(until)
                    };
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
                    let last = stall.map(|stall| (stall, b"y"));
                    for (wait, byte) in std::iter::repeat_n((gap, b"x"), pieces).chain(last) {
                        hold(wait).await;
                        if socket.write_all(byte).await.is_err() {
                            return;
                        }
                        said.send_modify(|dribbled| dribbled.written += 1);
                    }
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self {
            port,
            dribbled,
            _keep: keep,
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Wait for a hold past the first `seen`, with every byte written before
    /// it already `received` by the client: then the server is holding the
    /// next byte back, nothing is on the wire, and the clock can be moved.
    /// Answers how many holds have begun and when this one ends.
    async fn quiet_after(
        &self,
        seen: usize,
        received: &tokio::sync::watch::Receiver<usize>,
    ) -> (usize, tokio::time::Instant) {
        let now = *self
            .dribbled
            .clone()
            .wait_for(|dribbled| dribbled.holds > seen)
            .await
            .expect("the server outlives its handle");
        // A reader that ended dropped its sender: the caller sees it end.
        let _ = received
            .clone()
            .wait_for(|received| *received >= now.written)
            .await;
        (now.holds, now.until.expect("a hold has an end"))
    }
}

/// Read a body off the [`DribblingServer`] on the held clock, and how long
/// that took on it since `started`.
///
/// The clock is moved one hold at a time, each time the server holds a byte
/// back and the client has read every byte before it: so no timer fires with
/// a byte on the wire, and the only order left is the one the code
/// produces. A hold the `deadline` falls inside is moved in two steps, the
/// first to the deadline, and `read` is polled there: a timeout that fires
/// on time ends the read at the deadline exactly, one that fires early ends
/// it at an earlier hold, and one that fires late does not end it there.
async fn on_the_held_clock<T>(
    server: &DribblingServer,
    started: tokio::time::Instant,
    deadline: Duration,
    received: tokio::sync::watch::Receiver<usize>,
    read: impl std::future::Future<Output = T>,
) -> (T, Duration) {
    tokio::pin!(read);
    let at = started + deadline;
    let mut seen = 0;
    let read = real_backstop("the end of the body", async {
        loop {
            tokio::select! {
                biased;
                read = &mut read => break read,
                (holds, until) = server.quiet_after(seen, &received) => {
                    seen = holds;
                    let now = tokio::time::Instant::now();
                    if now < at && at < until {
                        tokio::time::advance(at - now).await;
                        let at_the_deadline =
                            std::future::poll_fn(|cx| Poll::Ready(read.as_mut().poll(cx))).await;
                        if let Poll::Ready(read) = at_the_deadline {
                            break read;
                        }
                    }
                    tokio::time::advance(until - tokio::time::Instant::now()).await;
                }
            }
        }
    })
    .await;
    (read, started.elapsed())
}

/// Every chunk of `body` until it ends or breaks off, with the bytes kept so
/// far published to `received` as they arrive.
async fn read_streamed(
    mut body: knobas_http::StreamedBody,
    received: tokio::sync::watch::Sender<usize>,
) -> Result<Vec<u8>, (knobas_http::BodyBrokeOff, Vec<u8>)> {
    let mut kept = Vec::new();
    loop {
        match body.chunk().await {
            Ok(Some(chunk)) => {
                kept.extend_from_slice(&chunk);
                received.send_replace(kept.len());
            }
            Ok(None) => return Ok(kept),
            Err(broke_off) => return Err((broke_off, kept)),
        }
    }
}

use std::task::Poll;
use std::time::Duration;

/// A client whose request timeout is 300 ms.
fn quick_client(url: String) -> HttpClient {
    HttpClient::new(HttpConfig {
        request_timeout: REQUEST_TIMEOUT,
        connect_timeout: Duration::from_millis(300),
        ..config(url)
    })
    .expect("client")
}

const REQUEST_TIMEOUT: Duration = Duration::from_millis(300);

/// Ten bytes 80 ms apart: 800 ms in all, well past the 300 ms request
/// timeout, and never 80 ms without a byte. None of the three falls on
/// another's multiple, so no two timers are ever due at one instant.
const PIECES: usize = 10;
const GAP: Duration = Duration::from_millis(80);
const STALL: Duration = Duration::from_millis(250);

/// The fixture's own premise, checked: the plain `send` breaks this body off.
/// Without it, the test below would stay green against a server that is not
/// actually slow (the lesson of a stub that is wrong about the tool).
///
/// On the held clock (#1935) it breaks off at the request timeout exactly,
/// in the fourth gap, after the three bytes before it.
#[tokio::test(start_paused = true)]
async fn a_dribbled_body_outlasts_the_request_timeout() {
    let server = DribblingServer::new(PIECES, GAP, None);
    let client = quick_client(server.url());
    let clock = HeldClock::hold();

    let started = tokio::time::Instant::now();
    let mut response = client
        .send(client.request(Method::GET, "/big.zip"))
        .await
        .expect("the headers arrive at once");
    let (count, received) = tokio::sync::watch::channel(0);
    let read = async move {
        let mut kept = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    kept.extend_from_slice(&chunk);
                    count.send_replace(kept.len());
                }
                Ok(None) => return Ok(kept),
                Err(error) => return Err((error, kept)),
            }
        }
    };
    let (read, took) = on_the_held_clock(&server, started, REQUEST_TIMEOUT, received, read).await;
    drop(clock);

    let (error, kept) = read.expect_err("the total timeout cuts the body");
    assert!(error.is_timeout(), "{error:?}");
    assert_eq!(took, REQUEST_TIMEOUT, "cut off the request timeout");
    assert_eq!(kept, b"xxx", "the bytes before the timeout arrived");
}

/// Issue #1012: a body slower in total than the request timeout, but never
/// stalling, arrives whole through the streamed send.
///
/// On the held clock (#1935) it takes the ten gaps exactly, the request
/// timeout passing in the fourth.
#[tokio::test(start_paused = true)]
async fn a_streamed_body_slower_than_the_request_timeout_arrives_whole() {
    let server = DribblingServer::new(PIECES, GAP, None);
    let client = quick_client(server.url());
    let clock = HeldClock::hold();

    let started = tokio::time::Instant::now();
    let body = client
        .send_streaming(client.request(Method::GET, "/big.zip"), STALL)
        .await
        .expect("the headers arrive at once");
    let (count, received) = tokio::sync::watch::channel(0);
    let (read, took) = on_the_held_clock(
        &server,
        started,
        REQUEST_TIMEOUT,
        received,
        read_streamed(body, count),
    )
    .await;
    drop(clock);

    let kept = read.unwrap_or_else(|(broke_off, kept)| panic!("{broke_off} after {kept:?}"));
    assert_eq!(kept, b"x".repeat(PIECES));
    assert_eq!(
        took,
        GAP * u32::try_from(PIECES).expect("ten"),
        "the body took its ten gaps"
    );
}

/// Issue #1012: a body that goes silent for longer than the stall timeout is
/// given up on, even though it would have finished. The adapter reports it as
/// `unreachable` ("… broke off: no bytes for 30s").
///
/// On the held clock (#1935) it breaks off one stall timeout after the third
/// byte, exactly, and the last byte never comes.
#[tokio::test(start_paused = true)]
async fn a_streamed_body_that_stalls_breaks_off_as_unreachable() {
    // Three bytes, then twice the stall timeout of silence, then the last.
    let server = DribblingServer::new(3, GAP, Some(STALL * 2));
    let client = quick_client(server.url());
    let clock = HeldClock::hold();

    let started = tokio::time::Instant::now();
    let body = client
        .send_streaming(client.request(Method::GET, "/big.zip"), STALL)
        .await
        .expect("the headers arrive at once");
    let (count, received) = tokio::sync::watch::channel(0);
    let (read, took) = on_the_held_clock(
        &server,
        started,
        GAP * 3 + STALL,
        received,
        read_streamed(body, count),
    )
    .await;
    drop(clock);

    let (broke_off, kept) = read.expect_err("the stalled body read as whole");
    assert_eq!(broke_off, knobas_http::BodyBrokeOff::Stalled(STALL));
    assert_eq!(kept, b"xxx", "what came before the stall did arrive");
    assert_eq!(
        took,
        GAP * 3 + STALL,
        "one stall timeout after the third byte"
    );
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

// ---- Bounds: the body cap, a redirected write, a URL with userinfo (#1640) --

mod bounds {
    use std::io::Write as _;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use knobas_http::{Auth, BodyExcerpt, HttpClient, HttpConfig, InstanceLimiter, Method};
    use knobas_source::SourceError;

    /// What [`ScriptedServer`] sends for one request.
    struct Answer {
        bytes: Vec<u8>,
        /// Keep the connection open after `bytes`, sending nothing more: a
        /// body shorter than its declared length.
        then_hold: bool,
    }

    impl Answer {
        fn sent(bytes: Vec<u8>) -> Self {
            Self {
                bytes,
                then_hold: false,
            }
        }
    }

    /// `status` with `headers` and `body`, `content-length` set from the body.
    fn response(status: u16, headers: &str, body: &[u8]) -> Vec<u8> {
        let mut bytes = format!(
            "HTTP/1.1 {status} X\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        bytes.extend_from_slice(body);
        bytes
    }

    /// A server that answers each request with what `answer` makes of its
    /// head, and keeps every head it was sent, in order.
    struct ScriptedServer {
        port: u16,
        heads: Arc<Mutex<Vec<String>>>,
    }

    impl ScriptedServer {
        fn new(answer: fn(&str) -> Answer) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            // Held for the process, as `CountingServer`'s is: the limiter is
            // keyed by origin, and a reused port would inherit its quota.
            std::mem::forget(listener.try_clone().expect("hold the port"));
            listener.set_nonblocking(true).expect("nonblocking");
            let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
            let heads = Arc::new(Mutex::new(Vec::new()));
            let kept = Arc::clone(&heads);
            tokio::spawn(async move {
                while let Ok((mut socket, _)) = listener.accept().await {
                    let kept = Arc::clone(&kept);
                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        let mut read = Vec::new();
                        let mut buffer = [0u8; 4096];
                        while !read.windows(4).any(|w| w == b"\r\n\r\n") {
                            match socket.read(&mut buffer).await {
                                Ok(0) | Err(_) => return,
                                Ok(n) => read.extend_from_slice(&buffer[..n]),
                            }
                        }
                        let head = String::from_utf8_lossy(&read).into_owned();
                        let answer = answer(&head);
                        kept.lock().expect("heads").push(head);
                        let _ = socket.write_all(&answer.bytes).await;
                        if answer.then_hold {
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                        let _ = socket.shutdown().await;
                    });
                }
            });
            Self { port, heads }
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        /// Each request's line, e.g. `"POST /a HTTP/1.1"`.
        fn request_lines(&self) -> Vec<String> {
            self.heads
                .lock()
                .expect("heads")
                .iter()
                .map(|head| head.lines().next().unwrap_or_default().to_owned())
                .collect()
        }

        fn heads(&self) -> Vec<String> {
            self.heads.lock().expect("heads").clone()
        }
    }

    /// The cap the body tests run at: small enough that twice it is cheap.
    const CAP: u64 = 64 * 1024;

    fn capped_client(url: String) -> HttpClient {
        HttpClient::new(HttpConfig {
            base_url: url,
            adapter_kind: "test".to_owned(),
            adapter_version: "0.1.0".to_owned(),
            auth: Auth::None,
            max_body: CAP,
            request_timeout: Duration::from_secs(5),
            ..HttpConfig::default()
        })
        .expect("client")
    }

    /// A JSON string `len` bytes long, quotes included.
    fn json_of_length(len: usize) -> Vec<u8> {
        let mut body = vec![b'a'; len];
        body[0] = b'"';
        body[len - 1] = b'"';
        body
    }

    fn twice_the_cap(_: &str) -> Answer {
        Answer::sent(response(200, "", &json_of_length(2 * CAP as usize)))
    }

    fn names_the_cap(error: &SourceError) -> bool {
        matches!(error, SourceError::Protocol { status: None, message } if message.contains("64 KiB"))
    }

    /// SPI-4: a body twice the cap is refused, naming the cap, through both
    /// reads this crate does for an adapter.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_body_over_the_cap_is_refused_naming_the_cap() {
        let server = ScriptedServer::new(twice_the_cap);
        let client = capped_client(server.url());

        let plain = client
            .get_json::<serde_json::Value>("/listing", &[])
            .await
            .expect_err("over the cap");
        assert!(names_the_cap(&plain), "{plain:?}");

        let excerpted = client
            .get_json_with::<serde_json::Value, [(&str, &str)]>(
                "/listing",
                &[],
                Some(BodyExcerpt {
                    contract: "the test contract",
                }),
            )
            .await
            .expect_err("over the cap");
        assert!(names_the_cap(&excerpted), "{excerpted:?}");
    }

    /// The declared length is read before the body is: a server that
    /// declares twice the cap and then sends almost nothing is refused at
    /// once, not after the request timeout spent waiting for the rest.
    ///
    /// The refusal naming the cap is the witness, not a clock (#1920). The
    /// server sends five bytes of the declared 128 KiB and then holds the
    /// connection for 30 s, so a read that waited for the body could never
    /// reach the cap: it would end at the 5 s request timeout, as a timeout.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_declared_length_over_the_cap_is_refused_before_the_read() {
        fn declares_twice(_: &str) -> Answer {
            let mut bytes = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                2 * CAP
            )
            .into_bytes();
            bytes.extend_from_slice(b"\"aaaa");
            Answer {
                bytes,
                then_hold: true,
            }
        }
        let server = ScriptedServer::new(declares_twice);
        let client = capped_client(server.url());

        let error = client
            .get_json::<serde_json::Value>("/listing", &[])
            .await
            .expect_err("over the cap");
        assert!(names_the_cap(&error), "{error:?}");
    }

    /// What reqwest inflates counts, not what arrived: a gzip body far under
    /// the cap that inflates to twice it is refused.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_gzip_body_that_inflates_past_the_cap_is_refused() {
        fn gzipped(_: &str) -> Answer {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            encoder
                .write_all(&json_of_length(2 * CAP as usize))
                .expect("gzip");
            let compressed = encoder.finish().expect("gzip");
            // The fixture's premise: it arrives small.
            assert!((compressed.len() as u64) < CAP / 16, "{}", compressed.len());
            Answer::sent(response(200, "content-encoding: gzip\r\n", &compressed))
        }
        let server = ScriptedServer::new(gzipped);
        let client = capped_client(server.url());

        let error = client
            .get_json::<serde_json::Value>("/listing", &[])
            .await
            .expect_err("inflates over the cap");
        assert!(names_the_cap(&error), "{error:?}");
        // And the server did gzip it: reqwest asked for it.
        assert!(
            server.heads()[0]
                .to_ascii_lowercase()
                .contains("accept-encoding: gzip"),
            "{:?}",
            server.heads()
        );
    }

    /// A body at the cap is read whole, and so is one under it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_body_at_or_under_the_cap_is_read() {
        fn at_the_cap(head: &str) -> Answer {
            let len = if head.starts_with("GET /at ") {
                CAP
            } else {
                CAP / 2
            };
            Answer::sent(response(200, "", &json_of_length(len as usize)))
        }
        let server = ScriptedServer::new(at_the_cap);
        let client = capped_client(server.url());
        for path in ["/at", "/under"] {
            let read = client
                .get_json::<serde_json::Value>(path, &[])
                .await
                .unwrap_or_else(|error| panic!("{path}: {error:?}"));
            assert!(read.is_string(), "{path}");
        }
    }

    /// An adapter that reads the answer itself is bounded too: the cap is on
    /// the body `send` hands back, not only on this crate's own reads.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_adapters_own_read_of_a_sent_answer_is_bounded() {
        let server = ScriptedServer::new(twice_the_cap);
        let client = capped_client(server.url());
        let answer = client
            .send(client.request(Method::GET, "/listing"))
            .await
            .expect("the status is a success");
        let read = answer.bytes().await;
        assert!(
            read.is_err(),
            "read {} bytes",
            read.map(|b| b.len()).unwrap_or(0)
        );
    }

    /// A reader that stops at a cap of its own reads the start of a body
    /// declared over this one: only a read of the whole body is refused on
    /// the declared length (Gitea's `raw_capped`, #711).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_body_declared_over_the_cap_still_yields_its_start() {
        let server = ScriptedServer::new(twice_the_cap);
        let client = capped_client(server.url());
        let mut answer = client
            .send(client.request(Method::GET, "/raw/big.txt"))
            .await
            .expect("the status is a success");
        let first = answer.chunk().await.expect("the start").expect("a chunk");
        assert!(!first.is_empty());
    }

    /// A download is not capped: `send_streaming` is for the bodies that are
    /// large on purpose (#1012).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_streamed_body_over_the_cap_arrives_whole() {
        let server = ScriptedServer::new(twice_the_cap);
        let client = capped_client(server.url());
        let mut body = client
            .send_streaming(
                client.request(Method::GET, "/big.zip"),
                Duration::from_secs(5),
            )
            .await
            .expect("headers");
        let mut total = 0;
        while let Some(chunk) = body.chunk().await.expect("whole") {
            total += chunk.len();
        }
        assert_eq!(total as u64, 2 * CAP);
    }

    /// A failing status keeps its class with a body over the cap, and its
    /// message names the cap where the body's excerpt would be.
    #[tokio::test(flavor = "multi_thread")]
    async fn an_error_body_over_the_cap_keeps_its_status_and_names_the_cap() {
        fn refused(_: &str) -> Answer {
            Answer::sent(response(400, "", &json_of_length(2 * CAP as usize)))
        }
        let server = ScriptedServer::new(refused);
        let client = capped_client(server.url());
        let error = client
            .get_json::<serde_json::Value>("/listing", &[])
            .await
            .expect_err("400");
        assert!(
            matches!(&error, SourceError::Protocol { status: Some(400), message } if message.contains("64 KiB")),
            "{error:?}"
        );
    }

    /// SPI-7: a write answered with a redirect is refused, naming where it
    /// was sent, and the GET the redirect asks for is never made -- for every
    /// redirect status, retried write or once-sent.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_redirected_write_is_refused_naming_the_location() {
        fn moves(head: &str) -> Answer {
            if head.starts_with("GET ") {
                return Answer::sent(response(200, "", b"[]"));
            }
            let status = head
                .split_whitespace()
                .nth(1)
                .and_then(|path| path.rsplit('/').next())
                .and_then(|last| last.parse().ok())
                .unwrap_or(301);
            Answer::sent(response(
                status,
                "location: /rest/api/2/issue/PAY-1/transitions/\r\n",
                b"",
            ))
        }
        let server = ScriptedServer::new(moves);
        let client = capped_client(server.url());

        for status in [301, 302, 303, 307, 308] {
            for once in [false, true] {
                let request = client
                    .request(Method::POST, &format!("/rest/api/2/issue/PAY-1/{status}"))
                    .json(&serde_json::json!({ "transition": { "id": "31" } }));
                let request = if once { request.no_retry() } else { request };
                let error = client.send(request).await.expect_err("redirected");
                assert!(
                    matches!(&error, SourceError::Protocol { status: Some(s), message }
                        if *s == status && message.contains("/rest/api/2/issue/PAY-1/transitions/")),
                    "{status} once={once}: {error:?}"
                );
            }
        }
        let lines = server.request_lines();
        assert_eq!(lines.len(), 10, "{lines:?}");
        assert!(
            lines.iter().all(|line| line.starts_with("POST ")),
            "{lines:?}"
        );
        // The client a write goes out on is configured as the read's is.
        assert!(
            server
                .heads()
                .iter()
                .all(|head| head.contains("user-agent: knobas/")),
            "{:?}",
            server.heads()
        );
    }

    /// A read still follows a redirect.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_redirected_read_is_followed() {
        fn moves(head: &str) -> Answer {
            if head.starts_with("GET /old ") {
                Answer::sent(response(301, "location: /new\r\n", b""))
            } else {
                Answer::sent(response(200, "", br#"{"moved":true}"#))
            }
        }
        let server = ScriptedServer::new(moves);
        let client = capped_client(server.url());
        let read: serde_json::Value = client.get_json("/old", &[]).await.expect("followed");
        assert_eq!(read["moved"], true);
        assert_eq!(
            server.request_lines(),
            ["GET /old HTTP/1.1", "GET /new HTTP/1.1"]
        );
        // The answer, read through the cap, still says where it came from.
        let response = client
            .send(client.request(Method::GET, "/old"))
            .await
            .expect("followed");
        assert_eq!(response.url().path(), "/new");
    }

    /// `/start` to `/mid` to `/end`, each a redirect but the last; `/loop`
    /// redirects to itself.
    fn hops(head: &str) -> Answer {
        let next = |to: &str| Answer::sent(response(302, &format!("location: {to}\r\n"), b""));
        if head.starts_with("GET /start ") {
            next("/mid")
        } else if head.starts_with("GET /mid ") {
            next("/end")
        } else if head.starts_with("GET /loop ") {
            next("/loop")
        } else {
            Answer::sent(response(200, "", br#"{"end":true}"#))
        }
    }

    /// A client built with a redirect check, recording each hop it is asked
    /// about, and refusing the ones whose path is `refused`.
    fn checked_client(url: String, refused: &'static str) -> (HttpClient, Arc<Mutex<Vec<String>>>) {
        let asked = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&asked);
        let client = HttpClient::with_redirect_check(
            HttpConfig {
                base_url: url,
                request_timeout: Duration::from_secs(5),
                ..HttpConfig::default()
            },
            move |hop: &knobas_http::Url| {
                seen.lock().expect("hops").push(hop.path().to_owned());
                hop.path() != refused
            },
        )
        .expect("client");
        (client, asked)
    }

    /// #1830: a read on a client built with a redirect check asks it about
    /// every hop, not only the last. A hop it refuses is never requested:
    /// the read fails with `Protocol`, the redirect's status, naming the hop,
    /// and is not sent again.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_checked_read_is_refused_at_the_first_hop_its_check_refuses() {
        let server = ScriptedServer::new(hops);
        let (client, asked) = checked_client(server.url(), "/mid");
        let error = client
            .send(client.request(Method::GET, "/start"))
            .await
            .expect_err("a refused hop");
        assert!(
            matches!(&error, SourceError::Protocol { status: Some(302), message }
                if message.contains(&format!("{}/mid", server.url()))),
            "{error:?}"
        );
        assert_eq!(server.request_lines(), ["GET /start HTTP/1.1"]);
        assert_eq!(*asked.lock().unwrap(), ["/mid"]);
    }

    /// A hop the check allows is followed, and so is the next: the check
    /// sees each one.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_checked_read_follows_every_hop_its_check_allows() {
        let server = ScriptedServer::new(hops);
        let (client, asked) = checked_client(server.url(), "/elsewhere");
        let response = client
            .send(client.request(Method::GET, "/start"))
            .await
            .expect("followed");
        assert_eq!(response.url().path(), "/end");
        assert_eq!(*asked.lock().unwrap(), ["/mid", "/end"]);
        assert_eq!(
            server.request_lines(),
            [
                "GET /start HTTP/1.1",
                "GET /mid HTTP/1.1",
                "GET /end HTTP/1.1"
            ]
        );
    }

    /// A checked read stops where reqwest's own policy does: after 10 hops.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_checked_read_follows_at_most_ten_hops() {
        let server = ScriptedServer::new(hops);
        let (client, _) = checked_client(server.url(), "/elsewhere");
        let error = client
            .send(client.request(Method::GET, "/loop"))
            .await
            .expect_err("too many redirects");
        assert!(matches!(error, SourceError::Protocol { .. }), "{error:?}");
        assert_eq!(server.request_lines().len(), 11);
    }

    const PASSWORD: &str = "pw-from-a-browser-form";

    /// SPI-10: a base URL carrying a username or a password is refused when
    /// the client is built, and the refusal does not repeat the password.
    #[test]
    fn a_base_url_with_userinfo_is_refused_without_naming_it() {
        for url in [
            format!("https://me:{PASSWORD}@jira.example"),
            format!("https://:{PASSWORD}@jira.example"),
            "https://me@jira.example".to_owned(),
            format!("https:me:{PASSWORD}@jira.example"),
        ] {
            let error = HttpClient::new(HttpConfig {
                base_url: url.clone(),
                ..HttpConfig::default()
            })
            .expect_err(&url);
            assert!(
                matches!(&error, SourceError::Protocol { status: None, message }
                    if message.contains("username or password") && !message.contains(PASSWORD)),
                "{url}: {error:?}"
            );
            let limiter = InstanceLimiter::for_url(&url, 5, 10).expect_err(&url);
            assert!(!limiter.to_string().contains(PASSWORD), "{limiter}");
        }
    }

    /// No refusal of a base URL repeats a password in it, malformed or not.
    #[test]
    fn a_refused_base_url_never_repeats_its_password() {
        for url in [
            format!("ftp://me:{PASSWORD}@jira.example"),
            format!("https://me:{PASSWORD}@jira example"),
        ] {
            let error = HttpClient::new(HttpConfig {
                base_url: url.clone(),
                ..HttpConfig::default()
            })
            .expect_err(&url);
            assert!(!error.to_string().contains(PASSWORD), "{url}: {error}");
        }
    }

    /// `HttpConfig`'s `Debug` prints the base URL without its password.
    #[test]
    fn a_configs_debug_never_prints_a_password_in_its_url() {
        for url in [
            format!("https://me:{PASSWORD}@jira.example/prefix"),
            format!("https:me:{PASSWORD}@jira.example"),
            format!("https://me:{PASSWORD}@jira example"),
        ] {
            let printed = format!(
                "{:?}",
                HttpConfig {
                    base_url: url,
                    ..HttpConfig::default()
                }
            );
            assert!(!printed.contains(PASSWORD), "{printed}");
            assert!(
                printed.contains("jira"),
                "the host stays readable: {printed}"
            );
        }
    }
}

// ---- A keep-alive connection the server closed as it was reused (#1818) ---

mod hung_up {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use knobas_http::{Auth, HttpClient, HttpConfig, Method};
    use knobas_source::SourceError;

    /// When [`HangUpServer`] closes a connection on a request it has read.
    #[derive(Clone, Copy)]
    enum HangUp {
        /// On any request after a connection's first. The first is answered
        /// and the connection kept alive, so the client pools it; the next
        /// request on it meets what a server's keep-alive timeout firing as
        /// the request went out looks like: read whole, and never answered.
        OnReuse,
        /// On every request, fresh connection or not.
        Always,
    }

    /// What [`HangUpServer`] was sent, in arrival order.
    #[derive(Default)]
    struct Log {
        /// Each request's line, e.g. `"GET /a HTTP/1.1"`.
        lines: Vec<String>,
        /// The lines of the requests it hung up on.
        hung_up: Vec<String>,
    }

    /// A server that reads each request whole and either answers it `200 {}`
    /// with the connection kept alive, or closes the connection without a
    /// byte of answer. It keeps every request line and every hang-up.
    struct HangUpServer {
        port: u16,
        log: Arc<Mutex<Log>>,
    }

    impl HangUpServer {
        fn new(when: HangUp) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            // Held for the process, as `CountingServer`'s is: the limiter is
            // keyed by origin, and a reused port would inherit its quota.
            std::mem::forget(listener.try_clone().expect("hold the port"));
            listener.set_nonblocking(true).expect("nonblocking");
            let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
            let log = Arc::new(Mutex::new(Log::default()));
            let kept = Arc::clone(&log);
            tokio::spawn(async move {
                while let Ok((socket, _)) = listener.accept().await {
                    tokio::spawn(serve(socket, when, Arc::clone(&kept)));
                }
            });
            Self { port, log }
        }

        fn url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        fn lines(&self) -> Vec<String> {
            self.log.lock().expect("log").lines.clone()
        }

        fn hung_up(&self) -> Vec<String> {
            self.log.lock().expect("log").hung_up.clone()
        }

        /// How many requests arrived with `line`.
        fn count(&self, line: &str) -> usize {
            self.lines().iter().filter(|seen| *seen == line).count()
        }
    }

    /// One connection: each request read whole (head and `content-length`
    /// body), then answered or hung up on.
    async fn serve(mut socket: tokio::net::TcpStream, when: HangUp, log: Arc<Mutex<Log>>) {
        use tokio::io::AsyncWriteExt;
        let mut read = Vec::new();
        for nth in 0usize.. {
            let end = loop {
                if let Some(at) = read.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
                if !read_more(&mut socket, &mut read).await {
                    return;
                }
            };
            let head = String::from_utf8_lossy(&read[..end]).into_owned();
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            while read.len() < end + length {
                if !read_more(&mut socket, &mut read).await {
                    return;
                }
            }
            read.drain(..end + length);
            let line = head.lines().next().unwrap_or_default().to_owned();
            let hang_up = matches!(when, HangUp::Always) || nth > 0;
            {
                let mut log = log.lock().expect("log");
                log.lines.push(line.clone());
                if hang_up {
                    log.hung_up.push(line);
                }
            }
            if hang_up {
                // Everything sent has been read, so this is a FIN and not a
                // reset: the client reads the end of the stream where it
                // waited for a status line.
                let _ = socket.shutdown().await;
                return;
            }
            let answer = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                          content-length: 2\r\nconnection: keep-alive\r\n\r\n{}";
            if socket.write_all(answer.as_bytes()).await.is_err() {
                return;
            }
        }
    }

    /// Append what the socket has next to `read`; `false` once it has ended.
    async fn read_more(socket: &mut tokio::net::TcpStream, read: &mut Vec<u8>) -> bool {
        use tokio::io::AsyncReadExt;
        let mut buffer = [0u8; 4096];
        match socket.read(&mut buffer).await {
            Ok(0) | Err(_) => false,
            Ok(n) => {
                read.extend_from_slice(&buffer[..n]);
                true
            }
        }
    }

    fn client(server: &HangUpServer) -> HttpClient {
        client_at(server, 5, 10)
    }

    fn client_at(server: &HangUpServer, per_second: u32, burst: u32) -> HttpClient {
        HttpClient::new(HttpConfig {
            base_url: server.url(),
            adapter_kind: "test".to_owned(),
            adapter_version: "0.1.0".to_owned(),
            auth: Auth::None,
            requests_per_second: per_second,
            burst,
            request_timeout: Duration::from_secs(5),
            ..HttpConfig::default()
        })
        .expect("client")
    }

    /// The read that leaves a kept-alive connection in the client's pool.
    async fn pool_a_connection(client: &HttpClient) {
        client
            .get_json::<serde_json::Value>("/first", &[])
            .await
            .expect("the first request on a connection is answered");
    }

    /// The same for a write, which goes out on a client of its own (#1640)
    /// and so on a pool of its own.
    async fn pool_a_write_connection(client: &HttpClient) {
        let response = client
            .send(client.request(Method::POST, "/first").no_retry())
            .await
            .expect("the first request on a connection is answered");
        response
            .bytes()
            .await
            .expect("read whole, so the connection is pooled");
    }

    /// **A read the server hung up on as it reused a pooled connection is
    /// sent again, and answered** (#1818).
    ///
    /// The `live_kuma` flake: Kuma's (Node's) 5 s keep-alive timeout closed
    /// the connection the incremental sync's `GET /metrics` went out on, and
    /// the sync failed `Protocol` on a read that was always safe to repeat.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_the_server_hung_up_on_is_sent_again() {
        for method in [Method::GET, Method::HEAD] {
            let server = HangUpServer::new(HangUp::OnReuse);
            let client = client(&server);
            pool_a_connection(&client).await;

            let answered = client.send(client.request(method.clone(), "/second")).await;

            let line = format!("{method} /second HTTP/1.1");
            assert!(answered.is_ok(), "{method}: {answered:?}");
            assert_eq!(
                server.hung_up(),
                [line.as_str()],
                "{method}: the request went out on the pooled connection and was hung up on"
            );
            assert_eq!(server.count(&line), 2, "{method}: {:?}", server.lines());
        }
    }

    /// **A write the server hung up on is never sent again**, whether or not
    /// it opted out of retries (ADR-0012), **and is `Unconfirmed`** (#1821).
    ///
    /// The server read it whole; whether it acted on it before closing is
    /// what the client cannot know. That is `classify::once_sent`'s reading
    /// of a request that reached the source and got no answer, so the write
    /// queue holds a write a second arrival would duplicate rather than
    /// refuse it.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_write_the_server_hung_up_on_is_not_sent_again() {
        for no_retry in [true, false] {
            let server = HangUpServer::new(HangUp::OnReuse);
            let client = client(&server);
            pool_a_write_connection(&client).await;

            let request = client
                .request(Method::POST, "/rest/api/2/issue")
                .json(&serde_json::json!({ "fields": { "summary": "once" } }));
            let request = if no_retry {
                request.no_retry()
            } else {
                request
            };
            let error = client.send(request).await.expect_err("never answered");

            let line = "POST /rest/api/2/issue HTTP/1.1";
            assert_eq!(server.hung_up(), [line], "no_retry {no_retry}");
            assert_eq!(server.count(line), 1, "no_retry {no_retry}: sent again");
            assert!(
                matches!(error, SourceError::Unconfirmed { status: None, .. }),
                "no_retry {no_retry}: {error:?}"
            );
        }
    }

    /// **A write does not go out on a connection idle for
    /// [`knobas_http::WRITE_POOL_IDLE`]** (#1821): the write client lets it
    /// go before a server's 5 s keep-alive timeout (Node's, so Kuma's) can
    /// close it as the write goes out, and opens a fresh one.
    ///
    /// The server hangs up on any request after a connection's first, so a
    /// write sent on the pooled connection would be hung up on, as
    /// `a_write_the_server_hung_up_on_is_not_sent_again` shows for one sent
    /// at once. This one is answered.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_write_after_the_pool_idle_timeout_goes_out_on_a_fresh_connection() {
        let server = HangUpServer::new(HangUp::OnReuse);
        let client = client(&server);
        pool_a_write_connection(&client).await;

        tokio::time::sleep(knobas_http::WRITE_POOL_IDLE + Duration::from_millis(500)).await;
        let answered = client
            .send(client.request(Method::POST, "/second").no_retry())
            .await;

        assert!(answered.is_ok(), "{answered:?}");
        assert!(server.hung_up().is_empty(), "{:?}", server.hung_up());
        assert_eq!(server.count("POST /second HTTP/1.1"), 1);
    }

    /// **A read sent with `no_retry` is not sent again either**: the
    /// caller asked for one sending, and a `GET` is no exception to that.
    /// Sent once, it reads as every once-sent request the server hung up on
    /// does: `Unconfirmed` (#1821).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_sent_with_no_retry_is_not_sent_again() {
        let server = HangUpServer::new(HangUp::OnReuse);
        let client = client(&server);
        pool_a_connection(&client).await;

        let error = client
            .send(client.request(Method::GET, "/second").no_retry())
            .await
            .expect_err("never answered");

        let line = "GET /second HTTP/1.1";
        assert_eq!(server.hung_up(), [line]);
        assert_eq!(server.count(line), 1, "sent again: {:?}", server.lines());
        assert!(
            matches!(error, SourceError::Unconfirmed { status: None, .. }),
            "{error:?}"
        );
    }

    /// A read is sent again **once**: a server that hangs up on every
    /// request is broken, not racing its keep-alive timeout, and is told so
    /// after two requests rather than [`knobas_http::MAX_ATTEMPTS`].
    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_the_server_keeps_hanging_up_on_is_sent_twice_only() {
        let server = HangUpServer::new(HangUp::Always);
        let client = client(&server);

        let error = client
            .get_json::<serde_json::Value>("/thing", &[])
            .await
            .expect_err("never answered");

        assert!(
            matches!(error, SourceError::Protocol { status: None, .. }),
            "{error:?}"
        );
        assert_eq!(
            server.count("GET /thing HTTP/1.1"),
            2,
            "{:?}",
            server.lines()
        );
    }

    /// The second sending is a request against the instance's quota, so it
    /// waits for the rate limiter as every attempt does (#1630).
    ///
    /// One a second with a burst of two: `/first` and `/second` take the
    /// burst, and `/second` sent again waits for the next token, about a
    /// second -- not just the 250 ms backoff.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_sent_again_waits_for_the_rate_limiter() {
        let server = HangUpServer::new(HangUp::OnReuse);
        let client = client_at(&server, 1, 2);
        pool_a_connection(&client).await;

        let started = Instant::now();
        client
            .get_json::<serde_json::Value>("/second", &[])
            .await
            .expect("answered the second time");
        let took = started.elapsed();

        assert_eq!(server.count("GET /second HTTP/1.1"), 2);
        assert!(
            took >= Duration::from_millis(700),
            "sent again after {took:?}: the retry skipped the limiter"
        );
    }

    /// The second sending fits in [`knobas_http::SEND_BUDGET`] or is not
    /// made: a call whose budget is shorter than the backoff ends with the
    /// hang-up rather than outliving the caller's patience.
    ///
    /// The budget is a duration, so the clock is tokio's, held for the whole
    /// test (#1925): nothing here waits on a timer, and the 200 ms budget is
    /// spent on no request however long the loopback takes. A call that
    /// waited out the backoff anyway would wait for ever on a held clock, so
    /// the real-time backstop fails it instead.
    #[tokio::test(start_paused = true)]
    async fn a_read_sent_again_stays_inside_the_send_budget() {
        let _clock = super::HeldClock::hold();
        let server = HangUpServer::new(HangUp::OnReuse);
        let client = client(&server).with_send_budget(Duration::from_millis(200));
        pool_a_connection(&client).await;

        let error = super::real_backstop(
            "the end of a call with no room to send again",
            client.get_json::<serde_json::Value>("/second", &[]),
        )
        .await
        .expect_err("no room to send it again");

        assert!(
            matches!(error, SourceError::Protocol { status: None, .. }),
            "{error:?}"
        );
        assert_eq!(server.count("GET /second HTTP/1.1"), 1);
    }
}
