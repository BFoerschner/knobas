//! One login for many writes (#983), against a fake Kuma that keeps score.
//!
//! Uptime Kuma 2.5.3 allows twenty logins per fixed minute
//! (`server/rate-limiter.js`, `loginRateLimiter`) and answers the twenty-first
//! with `{"ok":false,"msg":"Too frequently, try again later."}`. The fake below
//! enforces the same limit, so a socket that logs in per write fails here the
//! way it failed `just kuma-live` (#864).
//!
//! What certifies the exchange is still the live suite against the real
//! container (ADR-0013); this fake is the witness for what the container
//! cannot be asked to do on cue -- reap a session between two writes, or
//! forget a login -- and for the count of logins, which the real server does
//! not report.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::*;

/// Kuma's own words for a refused emit on a socket that is not logged in
/// (`checkLogin` in `server/util-server.js`).
const ANONYMOUS: &str = "You are not logged in.";

#[derive(Default)]
struct Engine {
    minted: usize,
    /// Live sessions: whether each is logged in, and what it has to say.
    sessions: HashMap<String, (bool, VecDeque<String>)>,
    logins: usize,
    login_limit: usize,
    /// Every emit other than the login that Kuma acted on.
    delivered: Vec<String>,
    /// When set, an emit Kuma acted on is answered this late: the packet
    /// landed and the client's POST times out anyway (#1122).
    stall_after_acting: Option<Duration>,
    /// When set, the session an emit landed on is reaped right after Kuma
    /// acted on it, so a resent POST meets `400 Session ID unknown`.
    reap_after_acting: bool,
}

/// The stateful half of the fake, shared by every mock it mounts.
#[derive(Clone)]
struct Kuma(Arc<Mutex<Engine>>);

fn gone() -> ResponseTemplate {
    // What engine.io answered for an unknown `sid`, read off the pinned image.
    ResponseTemplate::new(400).set_body_string(r#"{"code":1,"message":"Session ID unknown"}"#)
}

impl Respond for Kuma {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let mut engine = self.0.lock().expect("the fake's lock");
        let sid = request
            .url
            .query_pairs()
            .find(|(key, _)| key == "sid")
            .map(|(_, value)| value.into_owned());
        let Some(sid) = sid else {
            engine.minted += 1;
            let sid = format!("s{}", engine.minted);
            engine
                .sessions
                .insert(sid.clone(), (false, VecDeque::new()));
            return ResponseTemplate::new(200)
                .set_body_string(format!(r#"0{{"sid":"{sid}","pingInterval":25000}}"#));
        };
        if !engine.sessions.contains_key(&sid) {
            return gone();
        }
        if request.method.as_str() == "GET" {
            let (_, queued) = engine.sessions.get_mut(&sid).expect("checked above");
            let body = queued.drain(..).collect::<Vec<_>>().join("\u{1e}");
            return ResponseTemplate::new(200).set_body_string(if body.is_empty() {
                "6".to_owned()
            } else {
                body
            });
        }
        let packet = String::from_utf8_lossy(&request.body).into_owned();
        let acted = engine.delivered.len();
        let reply = engine.answer(&sid, &packet);
        if let Some(reply) = reply {
            let (_, queued) = engine.sessions.get_mut(&sid).expect("checked above");
            queued.push_back(reply);
        }
        let ok = ResponseTemplate::new(200).set_body_string("ok");
        if engine.delivered.len() == acted {
            return ok;
        }
        if engine.reap_after_acting {
            engine.sessions.clear();
        }
        match engine.stall_after_acting {
            Some(stall) => ok.set_delay(stall),
            None => ok,
        }
    }
}

impl Engine {
    /// What Kuma pushes back for one posted packet, if anything.
    fn answer(&mut self, sid: &str, packet: &str) -> Option<String> {
        if packet == "40" {
            return Some(r#"40{"sid":"socket"}"#.to_owned());
        }
        let rest = packet.strip_prefix("42")?;
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let (ack, document) = rest.split_at(digits);
        let event: serde_json::Value = serde_json::from_str(document).ok()?;
        let name = event.get(0)?.as_str()?.to_owned();
        let answered = if name == "login" {
            self.logins += 1;
            if self.logins > self.login_limit {
                serde_json::json!({"ok": false, "msg": "Too frequently, try again later."})
            } else {
                self.sessions.get_mut(sid)?.0 = true;
                serde_json::json!({"ok": true, "token": "ey.J.W.T"})
            }
        } else if self.sessions.get(sid)?.0 {
            self.delivered.push(name);
            serde_json::json!({"ok": true, "msg": "successPaused"})
        } else {
            serde_json::json!({"ok": false, "msg": ANONYMOUS})
        };
        Some(format!("43{ack}{}", serde_json::json!([answered])))
    }
}

impl Kuma {
    async fn start(login_limit: usize) -> (MockServer, Self) {
        let server = MockServer::start().await;
        let kuma = Self(Arc::new(Mutex::new(Engine {
            login_limit,
            ..Engine::default()
        })));
        Mock::given(path(PATH))
            .and(method("GET"))
            .respond_with(kuma.clone())
            .mount(&server)
            .await;
        Mock::given(path(PATH))
            .and(method("POST"))
            .respond_with(kuma.clone())
            .mount(&server)
            .await;
        (server, kuma)
    }

    fn engine(&self) -> std::sync::MutexGuard<'_, Engine> {
        self.0.lock().expect("the fake's lock")
    }

    fn logins(&self) -> usize {
        self.engine().logins
    }

    fn delivered(&self) -> usize {
        self.engine().delivered.len()
    }

    /// Kuma reaped every session, as its ping timeout does to an idle one.
    fn reap(&self) {
        self.engine().sessions.clear();
    }

    /// Every session is still open but no longer logged in.
    fn forget_logins(&self) {
        for (logged_in, _) in self.engine().sessions.values_mut() {
            *logged_in = false;
        }
    }
}

fn socket_to(server: &MockServer) -> KumaSocket {
    KumaSocket::new(
        &server.uri(),
        &crate::KumaConfig::default(),
        Account {
            username: "knobas".to_owned(),
            password: "knobas-dev".to_owned(),
        },
    )
    .expect("the socket builds against an http url")
}

async fn pause(socket: &KumaSocket) -> Result<serde_json::Value, SourceError> {
    socket.call(PAUSE_EVENT, &serde_json::json!(8)).await
}

/// **The ticket's own case**: more writes in a minute than Kuma allows
/// logins, and every one lands -- on one login.
#[tokio::test]
async fn more_writes_than_kuma_allows_logins_all_land_on_one_login() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server);
    for write in 1..=25 {
        pause(&socket)
            .await
            .unwrap_or_else(|error| panic!("write {write} was refused: {error:?}"));
    }
    assert_eq!(kuma.delivered(), 25);
    assert_eq!(kuma.logins(), 1, "one session carries every write");
}

/// A held session Kuma has reaped (its POST answers `400 Session ID
/// unknown`) costs one fresh login, and the write still lands -- once.
#[tokio::test]
async fn a_reaped_session_is_replaced_and_the_write_lands_once() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server);
    pause(&socket).await.expect("the first write");
    kuma.reap();
    pause(&socket)
        .await
        .expect("the write after the reap lands on a fresh login");
    assert_eq!(kuma.logins(), 2);
    assert_eq!(kuma.delivered(), 2, "neither write was lost or doubled");
}

/// A held session that is open but no longer logged in is refused by Kuma
/// before it acts, and the write goes again on a fresh login.
#[tokio::test]
async fn a_session_that_lost_its_login_logs_in_again() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server);
    pause(&socket).await.expect("the first write");
    kuma.forget_logins();
    pause(&socket)
        .await
        .expect("the write lands on a fresh login, not as `You are not logged in.`");
    assert_eq!(kuma.logins(), 2);
    assert_eq!(kuma.delivered(), 2);
}

/// Past the idle window a held session is not tried at all: Kuma may have
/// stopped waiting for the pongs nobody sent.
#[tokio::test]
async fn a_session_idle_past_its_window_is_not_reused() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server).with_idle_reuse(Duration::from_millis(50));
    pause(&socket).await.expect("the first write");
    pause(&socket).await.expect("a write inside the window");
    assert_eq!(kuma.logins(), 1, "inside the window the session is reused");
    tokio::time::sleep(Duration::from_millis(80)).await;
    pause(&socket).await.expect("a write after the window");
    assert_eq!(kuma.logins(), 2, "past the window a fresh login is spent");
}

/// A login Kuma refused for its rate limit is not a wrong password: it is
/// `Unreachable`, which the write queue retries, and never `Unauthorized`,
/// which asks a human to re-enter a credential that is right.
#[tokio::test]
async fn a_login_refused_for_the_rate_limit_is_not_a_wrong_password() {
    let (server, kuma) = Kuma::start(0).await;
    let refused = pause(&socket_to(&server))
        .await
        .expect_err("Kuma allowed no login");
    match refused {
        SourceError::Unreachable(message) => {
            assert!(message.contains("rate limit"), "{message}");
        }
        other => panic!("expected Unreachable, got {other:?}"),
    }
    assert_eq!(kuma.delivered(), 0);
}

/// A socket whose POSTs time out after one second, against a fake whose emits
/// land and then answer `stall` late -- past that timeout (#1122).
fn stalling(server: &MockServer, kuma: &Kuma, stall: Duration, reap: bool) -> KumaSocket {
    {
        let mut engine = kuma.engine();
        engine.stall_after_acting = Some(stall);
        engine.reap_after_acting = reap;
    }
    KumaSocket::new(
        &server.uri(),
        &crate::KumaConfig {
            request_timeout_secs: 1,
            ..crate::KumaConfig::default()
        },
        Account {
            username: "knobas".to_owned(),
            password: "knobas-dev".to_owned(),
        },
    )
    .expect("the socket builds against an http url")
}

async fn add(socket: &KumaSocket) -> Result<serde_json::Value, SourceError> {
    socket
        .call(ADD_EVENT, &serde_json::json!({ "name": "payout" }))
        .await
}

/// **#1122, on a live session.** Kuma acts on the `add` and its answer to the
/// POST never arrives in time. The create is not sent again: one monitor, and
/// the timeout comes back as the write's error for the write queue to rule on.
#[tokio::test]
async fn a_create_whose_emit_timed_out_after_landing_is_not_sent_again() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = stalling(&server, &kuma, Duration::from_millis(1500), false);
    let error = add(&socket).await.expect_err("the emit's POST timed out");
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
    assert_eq!(kuma.delivered(), 1, "exactly one create reached Kuma");
}

/// **#1122, across a reap.** The `add` lands, its POST times out, and Kuma
/// reaps the session before anything else arrives. A resent POST would meet
/// `400 Session ID unknown`, which reads as *not delivered* and buys a fresh
/// login and a third `add`. Nothing is resent, so none of that happens.
#[tokio::test]
async fn a_create_that_landed_before_a_reap_is_not_sent_on_a_fresh_login() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = stalling(&server, &kuma, Duration::from_millis(1500), true);
    let error = add(&socket).await.expect_err("the emit's POST timed out");
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
    assert_eq!(kuma.delivered(), 1, "exactly one create reached Kuma");
    assert_eq!(kuma.logins(), 1, "no fresh login to resend it on");
}

/// Adapters built for the same instance and account share one socket, and so
/// one held session; another password never borrows it.
#[tokio::test]
async fn adapters_for_one_account_share_a_socket_and_another_password_does_not() {
    let (server, _kuma) = Kuma::start(20).await;
    let cfg = crate::KumaConfig::default();
    let account = |password: &str| Account {
        username: "knobas".to_owned(),
        password: password.to_owned(),
    };
    let first = KumaSocket::shared(&server.uri(), &cfg, account("knobas-dev")).unwrap();
    let again = KumaSocket::shared(&server.uri(), &cfg, account("knobas-dev")).unwrap();
    assert!(
        Arc::ptr_eq(&first, &again),
        "one socket per instance and account"
    );
    let wrong = KumaSocket::shared(&server.uri(), &cfg, account("not-it")).unwrap();
    assert!(
        !Arc::ptr_eq(&first, &wrong),
        "a session logged in with the right password must not carry a wrong one's write"
    );
}
