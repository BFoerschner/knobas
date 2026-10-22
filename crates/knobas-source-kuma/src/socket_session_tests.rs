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
//! cannot be asked to do on cue -- reap a session between two writes, forget
//! a login, or lose the race a login sent too early can lose (#1472) -- and
//! for the count of logins, which the real server does not report.
//!
//! The lost acknowledgements of #1627 are here at the class level: an ack
//! poll that times out, a session reaped before its ack is read, an emit
//! never acknowledged, and a deadline with the emit still on the wire. The
//! first, which is the ticket's case, is also witnessed against the real
//! container (`live_kuma.rs`, `a_create_whose_ack_is_lost_is_unconfirmed_and_one_monitor`,
//! through a link that drops after the `add`, ADR-0013's *drop the
//! connection*). The other three need Kuma itself to reap or stall on cue
//! between one packet and the next, and the fake is their witness.
//!
//! So is an ack that arrives in a shape knobas cannot read (#1721): 2.5.3
//! always answers `[{"ok":…}]`, only a changed Kuma answers otherwise, and a
//! relay rewriting the real ack would be forging Kuma's answer rather than
//! staging a link. ADR-0013's carve-out for a fault the real instance cannot
//! produce is what makes the fake the witness there.

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
    /// Sessions whose namespace `40` has arrived and whose handlers are not
    /// registered yet (#1472). Kuma 2.5.3's connection handler awaits two
    /// settings reads (`sendInfo`) before it registers `login` or anything
    /// else, and socket.io drops an event nothing listens for, without an
    /// ack. The value counts the client's reads until the handlers are
    /// registered: the first read carries only the namespace's `40{…}`,
    /// which socket.io sends before the handler runs, and the second the
    /// handler's `info` and `loginRequired`. That is the worst case of the
    /// real race, a handler slower than the client's next request.
    registering: HashMap<String, usize>,
    logins: usize,
    /// Logins that arrived before Kuma listened for them, and were dropped.
    dropped: usize,
    login_limit: usize,
    /// Every emit other than the login that Kuma acted on.
    delivered: Vec<String>,
    /// When set, an emit Kuma acted on is answered this late: the packet
    /// landed and the client's POST times out anyway (#1122).
    stall_after_acting: Option<Duration>,
    /// When set, the session an emit landed on is reaped right after Kuma
    /// acted on it, so a resent POST meets `400 Session ID unknown`.
    reap_after_acting: bool,
    /// When set, the login is answered this late: the client's POST times
    /// out before the write's own packet is sent (#1125).
    stall_login: Option<Duration>,
    /// When set, every read after Kuma acted on an emit is answered this
    /// late: the packet landed and its ack is queued, and the poll that
    /// would carry it times out on the way back -- the Wi-Fi gone between
    /// the emit and its acknowledgement (#1627).
    stall_reads_after_acting: Option<Duration>,
    /// The same, from the moment the login's POST arrives: the login's ack
    /// is lost, and the write's own packet never goes out.
    stall_reads_after_login: Option<Duration>,
    /// How late every read is answered now, once one of the two above fired.
    reads_stalled: Option<Duration>,
    /// When set, Kuma acts on an emit and never acknowledges it: the session
    /// stays up and every read answers a no-op, until the deadline.
    swallow_acks: bool,
    /// When set, Kuma acts on an emit and acknowledges it with these
    /// arguments verbatim, in place of `[{"ok":true,…}]`: the ack arrives,
    /// in a shape knobas cannot read (#1721).
    ack_arguments: Option<String>,
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
            if let Some(stall) = engine.reads_stalled {
                return ResponseTemplate::new(200)
                    .set_body_string("6")
                    .set_delay(stall);
            }
            let registered = match engine.registering.get_mut(&sid) {
                Some(reads) if *reads > 1 => {
                    *reads -= 1;
                    false
                }
                Some(_) => {
                    engine.registering.remove(&sid);
                    true
                }
                None => false,
            };
            let (_, queued) = engine.sessions.get_mut(&sid).expect("checked above");
            if registered {
                // What the connection handler pushes once its awaits are
                // done: `info` just before the handlers are registered, and
                // `loginRequired` after the last of them.
                queued.push_back(r#"42["info",{"primaryBaseURL":null}]"#.to_owned());
                queued.push_back(r#"42["loginRequired"]"#.to_owned());
            }
            let body = queued.drain(..).collect::<Vec<_>>().join("\u{1e}");
            return ResponseTemplate::new(200).set_body_string(if body.is_empty() {
                "6".to_owned()
            } else {
                body
            });
        }
        let packet = String::from_utf8_lossy(&request.body).into_owned();
        let delivered_before = engine.delivered.len();
        let reply = engine.answer(&sid, &packet);
        if let Some(reply) = reply {
            let (_, queued) = engine.sessions.get_mut(&sid).expect("checked above");
            queued.push_back(reply);
        }
        let ok = ResponseTemplate::new(200).set_body_string("ok");
        if packet.contains(r#"["login""#) {
            if engine.stall_reads_after_login.is_some() {
                engine.reads_stalled = engine.stall_reads_after_login;
            }
            if let Some(stall) = engine.stall_login {
                return ok.set_delay(stall);
            }
        }
        if engine.delivered.len() == delivered_before {
            return ok;
        }
        if engine.stall_reads_after_acting.is_some() {
            engine.reads_stalled = engine.stall_reads_after_acting;
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
            // Registered on the second read: see the field's note.
            self.registering.insert(sid.to_owned(), 2);
            return Some(r#"40{"sid":"socket"}"#.to_owned());
        }
        let rest = packet.strip_prefix("42")?;
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let (ack, document) = rest.split_at(digits);
        let event: serde_json::Value = serde_json::from_str(document).ok()?;
        let name = event.get(0)?.as_str()?.to_owned();
        if self.registering.contains_key(sid) {
            // Nothing listens yet: socket.io drops the event, and no ack
            // ever comes.
            if name == "login" {
                self.dropped += 1;
            }
            return None;
        }
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
            if self.swallow_acks {
                return None;
            }
            if let Some(arguments) = &self.ack_arguments {
                return Some(format!("43{ack}{arguments}"));
            }
            serde_json::json!({"ok": true, "msg": "successPaused"})
        } else {
            serde_json::json!({"ok": false, "msg": ANONYMOUS})
        };
        Some(format!("43{ack}{}", serde_json::json!([answered])))
    }
}

impl Kuma {
    async fn start(login_limit: usize) -> (MockServer, Self) {
        let server = MockServer::builder().start().await;
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

    fn dropped(&self) -> usize {
        self.engine().dropped
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

/// A socket to the fake, saved at a quota well above the default's five a
/// second: the polls wait on it (#1723), the pacing is not what these tests
/// are about, and twenty-five writes at the default would spend ten seconds
/// of `just check` on the refill.
fn socket_to(server: &MockServer) -> KumaSocket {
    KumaSocket::new(
        &server.uri(),
        &crate::KumaConfig {
            rate_per_sec: 100,
            rate_burst: 100,
            ..crate::KumaConfig::default()
        },
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

/// **#1472.** A login POSTed straight after the namespace `40` can reach
/// Kuma before its connection handler has registered `login`, and socket.io
/// drops it without an ack: the session stays up, answers its pings, and the
/// write runs out its whole budget. The login waits for `loginRequired`,
/// which Kuma pushes once every handler is registered.
#[tokio::test]
async fn the_login_waits_until_kuma_listens_for_it() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server).with_budget(Duration::from_secs(5));
    pause(&socket)
        .await
        .expect("the write lands on a login Kuma was listening for");
    assert_eq!(kuma.dropped(), 0, "no login went out before Kuma listened");
    assert_eq!(kuma.logins(), 1);
    assert_eq!(kuma.delivered(), 1);
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
    // Unconfirmed (#1125): the write queue holds it rather than resending.
    assert!(
        matches!(error, SourceError::Unconfirmed { .. }),
        "{error:?}"
    );
    assert_eq!(kuma.delivered(), 1, "exactly one create reached Kuma");
}

/// **#1125:** a login whose POST times out has sent nothing of the write, so
/// the failure is plain `Unreachable` and the queue may send the write again
/// later. `Unconfirmed` here would hold a create Kuma never saw.
#[tokio::test]
async fn a_login_that_timed_out_before_the_write_was_emitted_is_unreachable() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = stalling(&server, &kuma, Duration::ZERO, false);
    kuma.engine().stall_login = Some(Duration::from_millis(1500));
    let error = add(&socket).await.expect_err("the login's POST timed out");
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
    assert_eq!(kuma.delivered(), 0, "the create never reached Kuma");
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
    assert!(
        matches!(error, SourceError::Unconfirmed { .. }),
        "{error:?}"
    );
    assert_eq!(kuma.delivered(), 1, "exactly one create reached Kuma");
    assert_eq!(kuma.logins(), 1, "no fresh login to resend it on");
}

/// **#1627, the ticket's case.** Kuma accepts the `add`'s POST and acts on it,
/// and every poll for its ack times out on the way back. The create may have
/// landed, so it is `Unconfirmed`, which the write queue holds; `Unreachable`
/// would have it sent again on the next tick and make a second monitor.
#[tokio::test]
async fn a_create_whose_ack_was_lost_after_landing_is_unconfirmed() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = stalling(&server, &kuma, Duration::ZERO, false);
    kuma.engine().stall_reads_after_acting = Some(Duration::from_millis(1500));
    let error = add(&socket).await.expect_err("the ack never arrived");
    assert!(
        matches!(error, SourceError::Unconfirmed { status: None, .. }),
        "{error:?}"
    );
    assert_eq!(kuma.delivered(), 1, "exactly one create reached Kuma");
    assert_eq!(kuma.logins(), 1, "no fresh login to resend it on");
}

/// **#1627, the other shape of a lost ack.** Kuma acts on the `add` and reaps
/// the session before its ack is read, so the poll answers `400 Session ID
/// unknown`. That `400` is about the read, not the packet, which was
/// accepted: `Unconfirmed` and not a `Protocol` the queue records as refused
/// while the monitor exists.
#[tokio::test]
async fn a_create_whose_session_was_reaped_before_its_ack_is_unconfirmed() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = stalling(&server, &kuma, Duration::ZERO, true);
    let error = add(&socket)
        .await
        .expect_err("the session went with the ack");
    assert!(
        matches!(
            error,
            SourceError::Unconfirmed {
                status: Some(400),
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(kuma.delivered(), 1, "exactly one create reached Kuma");
    assert_eq!(kuma.logins(), 1, "no fresh login to resend it on");
}

/// **#1627, the control.** The login's ack is what is lost: the write's own
/// packet never went out, so the failure stays `Unreachable` and the queue may
/// send the create later. `Unconfirmed` here would hold a create Kuma never saw.
#[tokio::test]
async fn a_login_whose_ack_was_lost_is_unreachable() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = stalling(&server, &kuma, Duration::ZERO, false);
    kuma.engine().stall_reads_after_login = Some(Duration::from_millis(1500));
    let error = add(&socket)
        .await
        .expect_err("the login's ack never arrived");
    assert!(matches!(error, SourceError::Unreachable(_)), "{error:?}");
    assert_eq!(kuma.delivered(), 0, "the create never reached Kuma");
}

/// **#1627, the deadline after the emit.** Kuma acts on the `add` and never
/// acknowledges it while the session stays up. The deadline that ends the
/// wait is `Unconfirmed`: the create landed, and a refusal would record it as
/// never made.
#[tokio::test]
async fn a_create_left_unacknowledged_past_the_deadline_is_unconfirmed() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server).with_budget(Duration::from_millis(500));
    kuma.engine().swallow_acks = true;
    let error = add(&socket).await.expect_err("Kuma never acknowledged it");
    match error {
        SourceError::Unconfirmed {
            status: None,
            message,
        } => assert!(message.contains("500ms"), "{message}"),
        other => panic!("expected Unconfirmed, got {other:?}"),
    }
    assert_eq!(kuma.delivered(), 1, "exactly one create reached Kuma");
}

/// **#1721, the ticket's case.** Kuma acts on the `add` and acknowledges it,
/// and the ack's arguments are not the array 2.5.3 sends: a Kuma whose ack
/// shape changed. The monitor exists, so the write is `Unconfirmed`, which the
/// queue holds; a `Protocol` would record as refused a create that landed.
#[tokio::test]
async fn a_create_acknowledged_in_a_shape_knobas_cannot_read_is_unconfirmed() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server);
    kuma.engine().ack_arguments =
        Some(r#"{"ok":true,"msg":"successAdded","monitorID":31}"#.to_owned());
    let error = add(&socket).await.expect_err("the ack cannot be read");
    match error {
        SourceError::Unconfirmed {
            status: None,
            message,
        } => assert!(message.contains(ADD_EVENT), "{message}"),
        other => panic!("expected Unconfirmed, got {other:?}"),
    }
    assert_eq!(kuma.delivered(), 1, "exactly one create reached Kuma");
    assert_eq!(kuma.logins(), 1);
}

/// **#1721, every ack that carries no verdict.** An empty ack, and an answer
/// with no boolean `ok`, say neither that Kuma acted nor that it refused, so
/// they are `Unconfirmed` too. Kuma's own refusal, `ok: false`, stays the
/// refusal it is.
#[tokio::test]
async fn an_ack_with_no_verdict_is_unconfirmed_and_a_refusal_is_not() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server);
    for arguments in [
        "[]",
        "[null]",
        r#"[{"msg":"who knows"}]"#,
        r#"[{"ok":"true"}]"#,
    ] {
        kuma.engine().ack_arguments = Some(arguments.to_owned());
        let error = add(&socket).await.expect_err("no verdict");
        assert!(
            matches!(error, SourceError::Unconfirmed { status: None, .. }),
            "{arguments}: {error:?}"
        );
    }
    kuma.engine().ack_arguments = Some(r#"[{"ok":false,"msg":"Invalid URL"}]"#.to_owned());
    let refused = add(&socket).await.expect_err("Kuma refused it");
    assert!(
        matches!(refused, SourceError::Protocol { status: None, .. }),
        "{refused:?}"
    );
    assert_eq!(kuma.delivered(), 5);
    assert_eq!(kuma.logins(), 1, "an ack that arrived keeps the session");
}

/// **#1627, the deadline with the emit on the wire.** Kuma acted on the `add`
/// and its answer to the POST is still on the way when the deadline drops
/// it. A packet already sent may have landed, so this is `Unconfirmed` too.
#[tokio::test]
async fn a_create_still_on_the_wire_at_the_deadline_is_unconfirmed() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server).with_budget(Duration::from_millis(500));
    kuma.engine().stall_after_acting = Some(Duration::from_millis(1500));
    let error = add(&socket).await.expect_err("the deadline came first");
    assert!(
        matches!(error, SourceError::Unconfirmed { status: None, .. }),
        "{error:?}"
    );
    assert_eq!(kuma.delivered(), 1, "exactly one create reached Kuma");
}

/// **#1627, the deadline before the emit.** The held session is gone, so the
/// create's first packet was refused unread (`400`), and the deadline falls
/// on the fresh login that would carry it. Nothing of this write reached
/// Kuma, so the deadline keeps its own class: status-less `Protocol`.
#[tokio::test]
async fn a_deadline_on_the_login_after_a_reaped_session_keeps_its_class() {
    let (server, kuma) = Kuma::start(20).await;
    let socket = socket_to(&server).with_budget(Duration::from_millis(500));
    add(&socket).await.expect("the first create");
    kuma.reap();
    kuma.engine().stall_login = Some(Duration::from_millis(1500));
    let error = add(&socket)
        .await
        .expect_err("the fresh login outlasted the deadline");
    assert!(
        matches!(error, SourceError::Protocol { status: None, .. }),
        "{error:?}"
    );
    assert_eq!(kuma.delivered(), 1, "only the first create reached Kuma");
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
