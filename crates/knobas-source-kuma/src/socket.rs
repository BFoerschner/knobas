//! Kuma's **other** door: the socket.io channel an account logs in to.
//!
//! The read half of this adapter is one `GET /metrics` with an API key
//! ([`crate::http`]). Pausing a monitor is not on that door at all -- Uptime
//! Kuma v2 has no REST API for configuration, an API key authenticates
//! `/metrics` and nothing else, and everything the dashboard does it does over
//! socket.io. So a Kuma that can write is a Kuma knobas has an **account** for,
//! and this module is the whole of what that account is used for: log in, emit
//! one event, read its answer, hang up.
//!
//! # Why the protocol is hand-written here
//!
//! Because the alternative is a second HTTP stack. A socket.io client crate
//! brings `rust_socketio` and, through it, a second `reqwest` major version and
//! `native-tls` beside the workspace's rustls -- and the workspace manifest
//! says in as many words that exactly one `reqwest` may be in the tree.
//! Measured, not assumed: adding `kuma-client` 2.1.0-rc.2 on 2026-09-07 locked
//! `reqwest 0.12.28` beside the pinned `0.13.4`.
//!
//! What is actually needed is small, because **engine.io's polling transport is
//! plain HTTP**: a `GET` opens a session, a `POST` sends a packet, and a `GET`
//! reads whatever the server has buffered. So this module speaks that, through
//! [`knobas_http`] like every other request this repo makes -- one rate
//! limiter, one retry budget, one status → [`SourceError`] mapping.
//!
//! # The packets, as the pinned image (2.5.3) answers them
//!
//! Read off the running container on 2026-09-07, not recalled:
//!
//! ```text
//! GET  /socket.io/?EIO=4&transport=polling
//!   -> 0{"sid":"…","upgrades":["websocket"],"pingInterval":25000,…}
//! POST …&sid=…   body `40`                      -> ok      (open the namespace)
//! POST …&sid=…   body `421["login",{…}]`        -> ok      (emit, ack id 1)
//! GET  …&sid=…   -> `40{"sid":…}`, pushed `42[…]` events, and `431[{"ok":true,…}]`
//! POST …&sid=…   body `422["pauseMonitor",8]`   -> ok      (emit, ack id 2)
//! GET  …&sid=…   -> `432[{"ok":true,"msg":"successPaused"}]`
//! POST …&sid=…   body `41`                                 (close)
//! ```
//!
//! Several packets arrive in one polling response, separated by `\x1e`
//! ([`SEPARATOR`]) -- a login pushes about fifty-five of them (the monitor
//! list, the type list, uptimes, heartbeats) and they came back in nine reads.
//!
//! **The login must be waited for, and that is measured too.** Emitting
//! `pauseMonitor` in the same breath as `login` -- three POSTs, then one poll
//! loop -- answers `{"ok":false,"msg":"You are not logged in."}`: Kuma's login
//! handler is asynchronous, so a call that arrives while it is still running is
//! refused as anonymous. A pipeline like that would have read as a permissions
//! problem for ever. So [`KumaSocket::call`] waits for the login's own ack
//! before it emits anything else.
//!
//! # What bounds one write
//!
//! **One wall-clock deadline over the whole exchange** ([`CALL_BUDGET`]), and
//! not a count of reads: a count bounds one wait and not their sum, and every
//! read here can cost twenty-five seconds without anything having gone wrong.
//! The constant's own note is where that arithmetic is written out.
//!
//! # What it deliberately does not do
//!
//! **No upgrade to websocket**, although the handshake offers one: the session
//! lives for one write and a few hundred milliseconds, and an upgrade would
//! buy nothing but a second transport to get wrong. **No session kept between
//! writes**: a queue flush is rare, a held-open socket is a ping loop to
//! maintain and a token to refresh, and Kuma's own answer to a stale session is
//! the same refusal a wrong password gets. **No `add`, no `deleteMonitor`,
//! nothing else** -- spec #427 gives this channel pause, resume and create, and
//! issue #452 is the first two.

use std::time::Duration;

use knobas_http::{Auth, HttpClient, HttpConfig, Method};
use knobas_source::SourceError;
use knobas_source::instance::Account;

/// The one path engine.io serves. Trailing slash included: that is what
/// `socket.io-client` requests and what the server mounts.
pub(crate) const PATH: &str = "/socket.io/";

/// Engine.io protocol 4, which is what socket.io 4 (and Uptime Kuma 2) speaks.
const EIO: &str = "4";
/// Long-polling, never `websocket`: see the module note on upgrades.
const TRANSPORT: &str = "polling";

/// Engine.io v4's record separator between packets in one payload.
const SEPARATOR: char = '\u{1e}';

/// The ack id the login rides on, and the one the call rides on.
///
/// Fixed rather than counted: one session performs exactly one login and one
/// call, so a counter would be a variable with two values and one more thing
/// for a reader to hold.
const LOGIN_ACK: u32 = 1;
const CALL_ACK: u32 = 2;

/// How long one whole write may take: the handshake, the login and the call's
/// own ack, measured as wall-clock time.
///
/// **A deadline over the exchange, not a count of reads**, and the difference
/// is what an unanswered ack costs. Each pass of [`Session::wait_for`] is one
/// engine.io long-poll GET, and a session that is alive but never acks holds
/// each of those until Kuma's ping interval (25 s) before answering with a
/// bare ping -- so the request timeout never fires, every read is a legitimate
/// answer, and a cap of 200 reads is eighty-three minutes. Twice, because
/// there are two waits per write. [`knobas_http::SEND_BUDGET`] makes the same
/// argument one level down: a per-part cap bounds one wait, not their sum.
///
/// The read count stays bounded without a count of its own, because the
/// transport's rate limiter ([`POLL_RATE_PER_SEC`]) is what decides how many
/// reads fit inside the deadline: at 25 a second for sixty seconds, a server
/// that pings instantly and for ever costs at most 1,500 reads and a pong
/// each -- against one that pings at Kuma's own interval it is three -- so the
/// deadline bounds the traffic as well as the time.
///
/// **Sixty seconds, and a constant rather than a [`crate::KumaConfig`]
/// field.** The measured exchange is twelve reads in a few hundred
/// milliseconds, and a large estate adds packets to the login rather than idle
/// waits, so there is no deployment this is tight for and nothing here for a
/// form to tune. What tripping it costs is one minute of the scheduler --
/// `flush_all` is awaited inline in the tick -- which is a price worth paying
/// once for a bound that exists at all.
const CALL_BUDGET: Duration = Duration::from_secs(60);

/// Kuma's own event names, spelled as its `server.js` registers them.
pub(crate) const PAUSE_EVENT: &str = "pauseMonitor";
pub(crate) const RESUME_EVENT: &str = "resumeMonitor";
/// The create (issue #453). One word, and `server.js` really does register it
/// as `add` rather than `addMonitor` -- the odd one out among the three, which
/// is why it is spelled here beside its siblings instead of at the call site.
pub(crate) const ADD_EVENT: &str = "add";

/// One Kuma instance's socket.io access, as an account.
#[derive(Debug)]
pub(crate) struct KumaSocket {
    client: HttpClient,
    account: Account,
    /// [`CALL_BUDGET`] in production; overridden only by this module's own
    /// tests, which cannot wait a minute to watch a deadline trip.
    budget: Duration,
}

impl KumaSocket {
    /// Build the channel for one configured source.
    ///
    /// **A client of its own, beside [`crate::http::KumaHttp`]**, and the two
    /// differences are the reason:
    ///
    /// * **No `Auth`.** The API key is Basic auth on `/metrics` and means
    ///   nothing here; sending it on every polling read would be a credential
    ///   on the wire for no purpose.
    /// * **A much higher rate.** A poll is not a request to a remote corpus,
    ///   it is how this transport *receives*, and one write makes a dozen of
    ///   them. At the read half's five per second a pause would take three
    ///   seconds of pure rate limiting. The budget being protected is Kuma's,
    ///   and a dozen reads of a buffer it has already filled is not traffic
    ///   worth spacing out.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] if the base URL is not an http(s) URL or the
    /// client cannot be built -- both configuration failures, surfaced when
    /// the source is saved rather than mid-write.
    pub(crate) fn new(
        base_url: &str,
        cfg: &crate::KumaConfig,
        account: Account,
    ) -> Result<Self, SourceError> {
        Ok(Self {
            client: HttpClient::new(HttpConfig {
                base_url: base_url.trim().to_owned(),
                adapter_kind: crate::ADAPTER_KIND.to_owned(),
                adapter_version: crate::ADAPTER_VERSION.to_owned(),
                auth: Auth::None,
                requests_per_second: POLL_RATE_PER_SEC,
                burst: POLL_BURST,
                connect_timeout: Duration::from_secs(cfg.connect_timeout_secs),
                request_timeout: Duration::from_secs(cfg.request_timeout_secs),
                // Engine.io answers a bad session with express's own text and
                // a refused login inside a `200`; there is no error envelope
                // to lift, and the bounded excerpt is the honest reading.
                body_message: None,
            })?,
            account,
            budget: CALL_BUDGET,
        })
    }

    /// The same socket with a shorter deadline, for this module's tests.
    ///
    /// Test-only on purpose: [`CALL_BUDGET`] is a constant and not a
    /// configuration field (see its note), so the only thing that may shorten
    /// it is a test that would otherwise have to wait a minute.
    #[cfg(test)]
    fn with_budget(mut self, budget: Duration) -> Self {
        self.budget = budget;
        self
    }

    /// Log in, emit `event` with `argument`, and answer what Kuma said.
    ///
    /// The answer is Kuma's own acknowledgement object, already checked for
    /// `ok` -- `{"ok":true,"msg":"successPaused"}` for a pause, and
    /// `{"ok":true,"msg":"successAdded","monitorID":31}` for a create, whose
    /// caller reads that id out as the write's receipt. Handing the object
    /// back rather than `()` is what lets a call that *made* something name it
    /// without this module growing a second entry point per event.
    ///
    /// # Errors
    ///
    /// [`SourceError::Unauthorized`] when the account is refused -- which is
    /// what puts *Re-enter* on screen and what makes the write queue **wait**
    /// rather than record a refusal (ADR-0004). [`SourceError::Protocol`] when
    /// Kuma refuses the call itself (a monitor that is not there, an account
    /// that may not touch it), carrying Kuma's own words; when the session
    /// stays open past [`CALL_BUDGET`] without acknowledging anything; and
    /// whatever [`knobas_http`] classified a transport failure as.
    ///
    /// The deadline's error is **status-less `Protocol`, which the write queue
    /// classes as a permanent refusal of this write** (`retryable`), and that
    /// is the deliberate reading: a session that answers its pings but never
    /// acks is a Kuma whose protocol has moved, not a Kuma that is down, and
    /// an `Unreachable` would spend a minute of every tick waiting for the
    /// drift to fix itself.
    pub(crate) async fn call(
        &self,
        event: &str,
        argument: &serde_json::Value,
    ) -> Result<serde_json::Value, SourceError> {
        let Ok(exchanged) = tokio::time::timeout(self.budget, self.exchange(event, argument)).await
        else {
            // `{:?}` and not `as_secs()`, which is how `knobas_http` prints
            // its own budget: this one is shortened to milliseconds by a test,
            // and `as_secs()` would render that deadline as `0s`.
            return Err(SourceError::protocol(format!(
                "Uptime Kuma held its socket.io session open for {:?} without acknowledging \
                 {event}: the session stayed up and answered its pings, so this reads as a change \
                 in Kuma's protocol rather than an outage",
                self.budget
            )));
        };
        let (session, answered) = exchanged?;

        // Best-effort, and **outside the deadline** on purpose: a session left
        // open is one Kuma reaps on its own ping timeout, and a write that
        // succeeded must not be reported as failed -- or hurried -- because
        // the goodbye did not arrive.
        if let Err(error) = session.send("41".to_owned()).await {
            tracing::debug!(%error, "closing the Uptime Kuma socket.io session");
        }
        verdict(&answered, event)
    }

    /// The session itself: open, log in, emit, and read the call's ack.
    ///
    /// Split out from [`KumaSocket::call`] so that one deadline can span the
    /// whole of it -- there is no point bounding the poll loop alone when a
    /// handshake that never answers costs the same. It hands the session back
    /// with the ack because the goodbye is sent *after* the deadline, so the
    /// caller needs the session the cancelled future was holding.
    async fn exchange(
        &self,
        event: &str,
        argument: &serde_json::Value,
    ) -> Result<(Session<'_>, String), SourceError> {
        let opened = self.open().await?;
        let session = Session {
            socket: self,
            sid: sid_of(&opened)?,
        };
        session.send("40".to_owned()).await?;

        session
            .send(emit(LOGIN_ACK, "login", &self.login_argument()))
            .await?;
        let login = session.wait_for(LOGIN_ACK).await?;
        login_verdict(&login, &self.account.username)?;

        session.send(emit(CALL_ACK, event, argument)).await?;
        let answered = session.wait_for(CALL_ACK).await?;
        Ok((session, answered))
    }

    /// The `login` payload. `token` is the two-factor code, empty because
    /// knobas has nowhere to ask for one -- Kuma answers an account with 2FA
    /// on by refusing, which surfaces as [`SourceError::Unauthorized`] like
    /// any other refused credential.
    fn login_argument(&self) -> serde_json::Value {
        serde_json::json!({
            "username": self.account.username,
            "password": self.account.password,
            "token": "",
        })
    }

    async fn open(&self) -> Result<String, SourceError> {
        let request = self
            .client
            .request(Method::GET, PATH)
            .query(&[("EIO", EIO), ("transport", TRANSPORT)]);
        text_of(self.client.send(request).await?).await
    }
}

/// Requests per second for the polling transport. See [`KumaSocket::new`].
const POLL_RATE_PER_SEC: u32 = 25;
const POLL_BURST: u32 = 50;

/// One open engine.io session: the socket plus the id the server minted.
struct Session<'a> {
    socket: &'a KumaSocket,
    sid: String,
}

impl Session<'_> {
    fn query(&self) -> [(&str, &str); 3] {
        [
            ("EIO", EIO),
            ("transport", TRANSPORT),
            ("sid", self.sid.as_str()),
        ]
    }

    /// POST one packet. The body is the packet verbatim, which is what
    /// [`knobas_http::Request::text`] exists for.
    async fn send(&self, packet: String) -> Result<(), SourceError> {
        let request = self
            .socket
            .client
            .request(Method::POST, PATH)
            .query(&self.query())
            .text(packet);
        self.socket.client.send(request).await?;
        Ok(())
    }

    /// Read until the ack numbered `ack` arrives, answering pings on the way.
    ///
    /// **No bound of its own, and that is the design**: what stops this loop
    /// when the ack never comes is [`CALL_BUDGET`], the deadline
    /// [`KumaSocket::call`] holds over the whole exchange, which cancels this
    /// future wherever it happens to be waiting. A write waits here twice, and
    /// the constant's note says why bounding each wait separately would not
    /// have bounded the write.
    ///
    /// Everything else in the stream is discarded on purpose: what a login
    /// pushes is the dashboard's whole state, and this module has no use for
    /// any of it -- reading a monitor is `/metrics`' job and stays so.
    async fn wait_for(&self, ack: u32) -> Result<String, SourceError> {
        loop {
            let request = self
                .socket
                .client
                .request(Method::GET, PATH)
                .query(&self.query());
            let body = text_of(self.socket.client.send(request).await?).await?;
            for packet in body.split(SEPARATOR) {
                // Engine.io's heartbeat. Unanswered, the server closes the
                // session after its `pingTimeout` -- which a slow exchange
                // could reach.
                if packet == "2" {
                    self.send("3".to_owned()).await?;
                } else if let Some(payload) = ack_of(packet, ack) {
                    return Ok(payload.to_owned());
                }
            }
        }
    }
}

/// A response body as text, or the protocol fault of not being readable.
async fn text_of(response: knobas_http::Response) -> Result<String, SourceError> {
    response
        .text()
        .await
        .map_err(|error| SourceError::protocol(format!("reading {PATH}: {error}")))
}

/// One socket.io emit with an ack id: `42<ack>["<event>",<argument>]`.
fn emit(ack: u32, event: &str, argument: &serde_json::Value) -> String {
    format!("42{ack}{}", serde_json::json!([event, argument]))
}

/// The session id out of engine.io's `open` packet (`0{…}`).
fn sid_of(open: &str) -> Result<String, SourceError> {
    let body = open.strip_prefix('0').ok_or_else(|| {
        SourceError::protocol(
            "Uptime Kuma did not answer the socket.io handshake with an open packet".to_owned(),
        )
    })?;
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|handshake| handshake.get("sid")?.as_str().map(str::to_owned))
        .ok_or_else(|| {
            SourceError::protocol("Uptime Kuma's socket.io handshake carried no session id")
        })
}

/// The payload of `packet` if it is the ack numbered `ack`.
///
/// `43` is socket.io's ACK packet type, the digits after it are the ack id,
/// and the rest is the arguments array. The id is **read out and compared as a
/// number**, not matched as a text prefix: `"431"` is a prefix of `"4312"`, so
/// a prefix match would read ack 12's answer as ack 1's -- and it would do it
/// only from the tenth emit of a session, which is not a bug anybody would
/// meet before a user did.
fn ack_of(packet: &str, ack: u32) -> Option<&str> {
    let rest = packet.strip_prefix("43")?;
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let (id, payload) = rest.split_at(digits);
    (id.parse::<u32>().ok()? == ack).then_some(payload)
}

/// What Kuma answered, as one object: `[{"ok":true,"msg":"successPaused"}]`.
fn answer(payload: &str) -> Result<serde_json::Value, SourceError> {
    let arguments: Vec<serde_json::Value> = serde_json::from_str(payload)
        .map_err(|_| SourceError::protocol("Uptime Kuma's answer was not a socket.io ack"))?;
    arguments
        .into_iter()
        .next()
        .ok_or_else(|| SourceError::protocol("Uptime Kuma acknowledged with no answer at all"))
}

fn said(answered: &serde_json::Value) -> String {
    answered
        .get("msg")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("no reason given")
        .to_owned()
}

fn went_well(answered: &serde_json::Value) -> bool {
    answered.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
}

/// The login's verdict.
///
/// A refused account is [`SourceError::Unauthorized`] and not a protocol
/// fault, and the difference is what the write queue does next: `Unauthorized`
/// **waits** for a human to re-enter a credential, everything else records the
/// write as refused (ADR-0004, `knobas_sync::write_queue::retryable`). A wrong
/// password is the first of those, however cheerfully Kuma answers it with a
/// `200`.
///
/// `status: None`, because nothing answered with one: Kuma refuses a login
/// inside a successful HTTP response, which is the `missing_secret`-shaped
/// case that field exists to distinguish.
fn login_verdict(payload: &str, username: &str) -> Result<(), SourceError> {
    let answered = answer(payload)?;
    if went_well(&answered) {
        return Ok(());
    }
    tracing::warn!(
        %username,
        reason = %said(&answered),
        "Uptime Kuma refused the account"
    );
    Err(SourceError::unauthorized())
}

/// The call's verdict: Kuma's own answer, or [`SourceError::Protocol`]
/// carrying Kuma's own words.
///
/// Protocol and not `Unauthorized`, even for *You do not own this monitor* --
/// the account logged in, so re-entering it changes nothing, and a write that
/// waits for a credential that is already right would wait for ever. It is a
/// refusal of this write, which is what the queue records and shows.
fn verdict(payload: &str, event: &str) -> Result<serde_json::Value, SourceError> {
    let answered = answer(payload)?;
    if went_well(&answered) {
        return Ok(answered);
    }
    Err(SourceError::protocol(format!(
        "Uptime Kuma refused {event}: {}",
        said(&answered)
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// **A Kuma that stays up and never acknowledges anything.**
    ///
    /// The one fake in this adapter, and it does **not** stand on a standing
    /// exception, because ADR-0013 has none: "no mock is a witness for any
    /// acceptance or exit criterion", and what certifies this adapter --
    /// this write path included -- is still `just kuma-live` against the real
    /// container. This is issue #486's own ruling for one fault, made for one
    /// reason: a released Kuma cannot be asked to withhold an ack, since what
    /// would produce one is a *future* release that renames an event. So the
    /// bound below is the only thing in this module with no live counterpart,
    /// and the ticket says so in as many words.
    ///
    /// It is faithful to the exchange in every other respect: the handshake
    /// mints a session, `40`, the login emit and every pong are accepted, and
    /// each poll answers a bare `2` after `hold` -- which is how a *healthy*
    /// idle engine.io session behaves, at Kuma's own 25 s ping interval. That
    /// is the point: nothing here is an error the transport could classify.
    struct PingingKuma {
        server: MockServer,
    }

    impl PingingKuma {
        /// `hold` stands in for the ping interval, scaled down so a test can
        /// watch several polls go by inside a sub-second deadline.
        async fn start(hold: Duration) -> Self {
            let server = MockServer::start().await;
            // The handshake: the one GET that carries no session yet.
            Mock::given(method("GET"))
                .and(path(PATH))
                .and(query_param_is_missing("sid"))
                .respond_with(ResponseTemplate::new(200).set_body_string(
                    r#"0{"sid":"never-answers","upgrades":[],"pingInterval":25000,"pingTimeout":20000}"#,
                ))
                .mount(&server)
                .await;
            // Every read of the session: a heartbeat, for ever.
            Mock::given(method("GET"))
                .and(path(PATH))
                .and(query_param("sid", "never-answers"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_string("2")
                        .set_delay(hold),
                )
                .mount(&server)
                .await;
            // `40`, the emits and the pongs: engine.io answers a POST `ok`.
            Mock::given(method("POST"))
                .and(path(PATH))
                .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
                .mount(&server)
                .await;
            Self { server }
        }

        fn base_url(&self) -> String {
            self.server.uri()
        }

        /// How many reads of the session the client actually made -- the
        /// handshake excluded, since it carries no `sid`.
        async fn polls(&self) -> usize {
            self.server
                .received_requests()
                .await
                .expect("the fake records what it was asked")
                .iter()
                .filter(|request| {
                    request.method.as_str() == "GET"
                        && request.url.query().is_some_and(|q| q.contains("sid="))
                })
                .count()
        }
    }

    fn socket_to(base_url: &str, budget: Duration) -> KumaSocket {
        KumaSocket::new(
            base_url,
            &crate::KumaConfig::default(),
            Account {
                username: "knobas".to_owned(),
                password: "knobas-dev".to_owned(),
            },
        )
        .expect("the socket builds against an http url")
        .with_budget(budget)
    }

    /// One run against the fake: how long [`KumaSocket::call`] took, how many
    /// reads it made, and what it finally said.
    ///
    /// The outer `tokio::time::timeout` is ten times the budget, so the code
    /// *without* its deadline -- [`Session::wait_for`] loops for ever -- fails
    /// here rather than hanging `just check`, and it is loose enough that the
    /// assertions on the deadline itself are what report a run that overshot.
    async fn until_it_gives_up(budget: Duration, hold: Duration) -> (Duration, usize, SourceError) {
        let kuma = PingingKuma::start(hold).await;
        let socket = socket_to(&kuma.base_url(), budget);

        let started = std::time::Instant::now();
        let refused =
            tokio::time::timeout(budget * 10, socket.call(PAUSE_EVENT, &serde_json::json!(8)))
                .await
                .expect("the call is stopped by its own deadline, not by this one")
                .expect_err("a session that never acknowledges cannot answer a write");
        (started.elapsed(), kuma.polls().await, refused)
    }

    /// **One write is bounded by time, and the assertion is the clock.**
    ///
    /// Issue #486: the loop used to run 200 reads, and against a session that
    /// pings rather than acks each read costs a ping interval -- so the bound
    /// was really eighty-three minutes of the scheduler, twice.
    ///
    /// What tells a deadline from a count is that **the budget is the only
    /// thing that changes between the two runs below**: the same fake, the
    /// same hold, four times the time, and the write both gives up later and
    /// reads more. A bound of N reads would have given up after the same reads
    /// after the same seconds both times, whatever the budget said.
    ///
    /// Not in `tests/contract.rs`, where issue #486 placed it, and the
    /// deviation is recorded rather than taken quietly: [`KumaSocket`] is
    /// `pub(crate)`, so the test-only deadline override the same paragraph
    /// asks for is unreachable from an integration test -- which leaves a test
    /// that waits the production minute, or this.
    #[tokio::test]
    async fn a_session_that_only_pings_is_given_up_on_when_the_time_is_gone() {
        const HOLD: Duration = Duration::from_millis(100);
        const SHORT: Duration = Duration::from_millis(500);
        const LONG: Duration = Duration::from_millis(2_000);

        let (elapsed, polls, refused) = until_it_gives_up(SHORT, HOLD).await;

        let (message, status) = match refused {
            SourceError::Protocol { message, status } => (message, status),
            other => panic!("expected Protocol, got {other:?}"),
        };
        // Status-less, which is what makes it a permanent refusal of this
        // write rather than something the queue retries every tick.
        assert_eq!(status, None, "{message}");
        assert!(
            message.contains("500ms"),
            "the message has to name the deadline it tripped: {message}"
        );
        assert!(message.contains(PAUSE_EVENT), "{message}");

        assert!(elapsed >= SHORT, "gave up before the deadline: {elapsed:?}");
        assert!(
            elapsed < SHORT * 8,
            "gave up long after the deadline: {elapsed:?}"
        );

        let (longer, more, _) = until_it_gives_up(LONG, HOLD).await;
        assert!(
            longer >= LONG,
            "the longer deadline was given up on early, after {longer:?}"
        );
        assert!(
            more > polls,
            "{polls} reads in {elapsed:?} and {more} in {longer:?}: the reads one write \
             makes have to be whatever fit inside its own deadline, not a fixed count"
        );
    }

    /// The emit's bytes, which are not JSON and cannot be built by a JSON
    /// body: `42` is the packet type, the digit after it is the ack id, and
    /// only what follows is a document. A client that posted
    /// `"42[\"login\",…]"` -- the JSON *string* -- would send a packet of type
    /// `"`, which engine.io discards silently.
    #[test]
    fn an_emit_is_a_packet_type_an_ack_id_and_a_document() {
        assert_eq!(
            emit(1, "login", &serde_json::json!({ "username": "knobas" })),
            r#"421["login",{"username":"knobas"}]"#
        );
        assert_eq!(
            emit(2, PAUSE_EVENT, &serde_json::json!(8)),
            r#"422["pauseMonitor",8]"#
        );
        assert_eq!(
            emit(2, RESUME_EVENT, &serde_json::json!(8)),
            r#"422["resumeMonitor",8]"#
        );
    }

    /// The handshake, and the two ways it can fail to carry a session.
    #[test]
    fn the_session_id_comes_out_of_the_open_packet() {
        assert_eq!(
            sid_of(r#"0{"sid":"PJkLq4xzdzRQm9Dv","upgrades":["websocket"],"pingInterval":25000}"#)
                .unwrap(),
            "PJkLq4xzdzRQm9Dv"
        );
        // A `40` here is the *namespace* connect packet, not the handshake:
        // reading it as one would take a socket id for a session id and every
        // request after it would 400.
        assert!(sid_of(r#"40{"sid":"LrIRTqRu89BciQdd"}"#).is_err());
        assert!(sid_of(r#"0{"upgrades":["websocket"]}"#).is_err());
        assert!(sid_of("<!DOCTYPE html>").is_err());
    }

    /// An ack is matched on its id as a **number**, not as a text prefix.
    ///
    /// The mistake this forbids is `strip_prefix("43{ack}")`, which is what
    /// this function was first written as: with acks 1 and 2 in flight it
    /// works, and it silently reads ack 12's answer as ack 1's -- payload
    /// `"2[…]"` and all -- the day a session emits more than nine times. The
    /// assertion below is the one that caught it.
    #[test]
    fn an_ack_is_matched_on_the_whole_id() {
        assert_eq!(ack_of(r#"431[{"ok":true}]"#, 1), Some(r#"[{"ok":true}]"#));
        assert_eq!(ack_of(r#"432[{"ok":true}]"#, 1), None);
        assert_eq!(ack_of(r#"4312[{"ok":true}]"#, 1), None);
        assert_eq!(ack_of(r#"4312[{"ok":true}]"#, 12), Some(r#"[{"ok":true}]"#));
        // An ACK with no id at all is nobody's answer.
        assert_eq!(ack_of(r#"43[{"ok":true}]"#, 1), None);
        // Pushed events and pings are not acks of anything.
        assert_eq!(ack_of(r#"42["monitorList",{}]"#, 1), None);
        assert_eq!(ack_of("2", 1), None);
    }

    /// The two verdicts, and the fault class each takes -- which is the whole
    /// of what the write queue does next.
    ///
    /// Every payload below is one the pinned image (2.5.3) actually answered
    /// on 2026-09-07.
    #[test]
    fn a_refused_account_waits_and_a_refused_write_does_not() {
        verdict(
            r#"[{"ok":true,"msg":"successPaused","msgi18n":true}]"#,
            PAUSE_EVENT,
        )
        .unwrap();
        verdict(
            r#"[{"ok":true,"msg":"successResumed","msgi18n":true}]"#,
            RESUME_EVENT,
        )
        .unwrap();
        login_verdict(r#"[{"ok":true,"token":"ey.J.W.T"}]"#, "knobas").unwrap();

        // A wrong password: the credential is what is wrong, so the queue
        // waits for a human rather than recording a refusal.
        assert!(matches!(
            login_verdict(
                r#"[{"ok":false,"msg":"authIncorrectCreds","msgi18n":true}]"#,
                "knobas"
            ),
            Err(SourceError::Unauthorized { status: None })
        ));

        // A monitor this account may not touch: the login worked, so
        // re-entering it would change nothing and the write is refused.
        let refused = verdict(
            r#"[{"ok":false,"msg":"You do not own this monitor."}]"#,
            PAUSE_EVENT,
        );
        let message = match refused {
            Err(SourceError::Protocol { message, .. }) => message,
            other => panic!("expected Protocol, got {other:?}"),
        };
        assert!(
            message.contains("You do not own this monitor."),
            "{message}"
        );
        assert!(message.contains(PAUSE_EVENT), "{message}");
    }

    /// An answer knobas cannot read is a protocol fault by name, never a
    /// silent success -- a write reported as landed because its ack was
    /// unparseable is the one failure a reader could not recover from.
    #[test]
    fn an_unreadable_answer_is_a_fault_rather_than_a_success() {
        for unreadable in ["", "null", "[]", "{\"ok\":true}", "not json"] {
            assert!(
                verdict(unreadable, PAUSE_EVENT).is_err(),
                "{unreadable:?} must not read as a successful pause"
            );
            assert!(
                login_verdict(unreadable, "knobas").is_err(),
                "{unreadable:?} must not read as a successful login"
            );
        }
        // An ack with no `ok` at all is a refusal, not an assumption.
        assert!(verdict(r#"[{"msg":"who knows"}]"#, PAUSE_EVENT).is_err());
    }
}
