//! Kuma's **other** door: the socket.io channel an account logs in to.
//!
//! The read half of this adapter is one `GET /metrics` with an API key
//! ([`crate::http`]). Pausing a monitor is not on that door at all -- Uptime
//! Kuma v2 has no REST API for configuration, an API key authenticates
//! `/metrics` and nothing else, and everything the dashboard does it does over
//! socket.io. So a Kuma that can write is a Kuma knobas has an **account** for,
//! and this module is the whole of what that account is used for: log in, emit
//! an event, read its answer -- and keep the session for the next write.
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
//! GET  …&sid=…   -> `40{"sid":…}`, `42["info",{…}]`, `42["loginRequired"]`
//! POST …&sid=…   body `421["login",{…}]`        -> ok      (emit, ack id 1)
//! GET  …&sid=…   -> pushed `42[…]` events, and `431[{"ok":true,…}]`
//! POST …&sid=…   body `422["pauseMonitor",8]`   -> ok      (emit, ack id 2)
//! GET  …&sid=…   -> `432[{"ok":true,"msg":"successPaused"}]`
//! POST …&sid=…   body `41`                                 (close)
//! ```
//!
//! Several packets arrive in one polling response, separated by `\x1e`
//! ([`SEPARATOR`]) -- a login pushes about fifty-five of them (the monitor
//! list, the type list, uptimes, heartbeats) and they came back in nine reads.
//!
//! **The login must be invited, and that was measured too (#1472).** Kuma
//! 2.5.3's connection handler (`io.on("connection")` in `server/server.js`)
//! first awaits `sendInfo`, two settings reads, and only then registers
//! `login` and every other handler; socket.io drops an event nothing listens
//! for, without an ack. While Kuma's settings cache is warm those reads cost
//! a microtask and the race is never lost, but each cached setting expires a
//! minute or two after it was read, and a login POSTed straight after the
//! namespace `40` then reaches Kuma before `login` is registered: the session
//! stays up, answers its pings and never acknowledges, and the write runs out
//! its whole [`CALL_BUDGET`]. Replayed against the pinned container on
//! 2026-10-21 with the two POSTs 2.4 ms apart: dropped when the cache was
//! cold, Kuma's log showing the polling connection and no `Login by
//! username` line. So the login is sent only once Kuma pushes
//! `loginRequired` (or `autoLogin`), which the handler emits after the last
//! handler is registered ([`invites_login`]).
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
//! # One login, many writes (#983)
//!
//! Uptime Kuma 2.5.3 allows **twenty logins per fixed minute**, across all
//! clients (`server/rate-limiter.js`, `loginRateLimiter`), and answers the
//! twenty-first inside a `200` like any refused login. A session per write
//! therefore failed the twenty-first write of a minute -- pausing many monitors,
//! or flushing a long queue -- and it read as a wrong password.
//!
//! So a logged-in session is **held** after a write and reused by the next
//! one, if that one comes within [`IDLE_REUSE`]. Three things end a held
//! session, and each is met by logging in again *before the write is lost*:
//!
//! * **it was idle too long.** Nothing reads the session between writes, so
//!   nothing answers Kuma's pings; [`IDLE_REUSE`] is short enough that Kuma has
//!   not given up on it yet, and past it the session is not tried at all.
//! * **Kuma reaped it anyway** (a restart, a password change, which disconnects
//!   every socket of the account): the emit's POST answers `400 Session ID
//!   unknown`. The packet was refused, so nothing was done, and a fresh login
//!   sends it again.
//! * **the socket is no longer logged in**: Kuma answers the emit with *You are
//!   not logged in.* before acting on it, and the same retry applies.
//!
//! Any other failure on a held session drops it and is the write's failure:
//! an emit that was accepted and then went unanswered may have been acted on,
//! and sending a create twice would make two monitors. That failure, and the
//! deadline once the packet is out, is `Unconfirmed` (#1627), which the write
//! queue holds rather than sends again. So is an ack that arrived in a shape
//! knobas cannot read (#1721): Kuma took the packet, and nothing it said reads
//! as a refusal.
//!
//! The socket itself is **shared per instance and account across the
//! process** ([`KumaSocket::shared`]): the write queue builds an adapter per
//! flush, and every click on *Pause* is its own flush, so a session held by
//! one adapter value would have been one login per click all the same.
//!
//! A login Kuma refuses **for its rate limit** is told apart from a wrong
//! password ([`login_verdict`]): it is [`SourceError::Unreachable`], which the
//! write queue retries on its next tick, not `Unauthorized`, which would ask a
//! human to re-enter a credential that is right.
//!
//! # What it deliberately does not do
//!
//! **No upgrade to websocket**, although the handshake offers one: a session
//! is read only while a write waits for its answer, and an upgrade would buy
//! nothing but a second transport to get wrong. **No goodbye**: a held session
//! is left for Kuma to reap on its own ping timeout once the writes stop.
//! **No `add`, no `deleteMonitor`, nothing else** -- spec #427 gives this
//! channel pause, resume and create, and issue #452 is the first two.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, PoisonError};
use std::time::{Duration, Instant};

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

/// The ack id the login rides on. A session's writes count up from the next
/// one ([`LoggedIn::next_ack`]), which is why [`ack_of`] compares ids as numbers.
const LOGIN_ACK: u32 = 1;

/// How long a held session may sit unused and still be tried for a write.
///
/// **Under Kuma's own patience, with room.** Engine.io pings every 25 s and
/// closes a session whose pong has not come 20 s later, and nothing reads a
/// held session between writes, so the ping that goes unanswered can be one
/// sent just before the last write finished. Fifteen seconds after that write
/// is at most forty seconds after the ping, against the forty-five Kuma waits.
/// A session reaped anyway costs a refused POST and a fresh login (the module
/// note), so this bounds wasted requests rather than correctness.
///
/// Long enough for what it is for: a burst of clicks, or a queue flushed write
/// after write, never sits fifteen seconds between two of them.
const IDLE_REUSE: Duration = Duration::from_secs(15);

/// What Kuma answers a login its rate limiter refused (`errorMessage` in
/// `server/rate-limiter.js`, 2.5.3), in place of a verdict on the password.
const RATE_LIMITED: &str = "Too frequently, try again later.";

/// What Kuma pushes once its connection handler has registered every event
/// handler, `login` among them (`server/server.js`, 2.5.3): `loginRequired`,
/// or `autoLogin` on an instance with authentication disabled. Either means
/// a login sent now is heard (#1472).
const LOGIN_INVITATIONS: [&str; 2] = ["loginRequired", "autoLogin"];

/// What Kuma answers an emit from a socket that is not logged in
/// (`checkLogin`, `server/util-server.js`), before acting on it.
const NOT_LOGGED_IN: &str = "You are not logged in.";

/// How long one whole write may take: the handshake, the login and the call's
/// own ack, measured as wall-clock time.
///
/// **A deadline over the exchange, not a count of reads**, and the difference
/// is what an unanswered ack costs. Each pass of [`Session::read_until`] is one
/// engine.io long-poll GET, and a session that is alive but never acks holds
/// each of those until Kuma's ping interval (25 s) before answering with a
/// bare ping -- so the request timeout never fires, every read is a legitimate
/// answer, and a cap of 200 reads is eighty-three minutes. Three times, because
/// a fresh login waits three times: for Kuma's invitation (#1472), the
/// login's ack and the call's. [`knobas_http::SEND_BUDGET`] makes the same
/// argument one level down: a per-part cap bounds one wait, not their sum.
///
/// The read count stays bounded without a count of its own, because the
/// instance's rate limiter (the source's quota, [`KumaSocket::new`]) is what
/// decides how many reads fit inside the deadline: at the default five a
/// second for sixty seconds, a server that pings instantly and for ever costs
/// at most 300 reads and a pong each -- against one that pings at Kuma's own
/// interval it is three -- so the deadline bounds the traffic as well as the
/// time.
///
/// **Sixty seconds, and a constant rather than a [`crate::KumaConfig`]
/// field.** The measured exchange is twelve reads in a few hundred
/// milliseconds, and a large estate adds packets to the login rather than idle
/// waits, so there is no deployment this is tight for and nothing here for a
/// form to tune. What tripping it costs is one minute of the write queue's
/// flush task -- it runs beside the ticker since #1629, so no sync waits on
/// it -- which is a price worth paying once for a bound that exists at all.
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
    /// What the socket was built from, so that [`KumaSocket::shared`] can tell
    /// a source saved with other timeouts from the one it already holds.
    cfg: crate::KumaConfig,
    /// [`CALL_BUDGET`] in production; overridden only by this module's own
    /// tests, which cannot wait a minute to watch a deadline trip.
    budget: Duration,
    /// [`IDLE_REUSE`] in production; overridden only by this module's tests.
    idle_reuse: Duration,
    /// The logged-in session the next write may reuse, if any. The lock is
    /// held for a whole write, so two writes never share a session at once --
    /// the write queue flushes one source at a time anyway.
    held: tokio::sync::Mutex<Option<LoggedIn>>,
}

/// A logged-in session kept between writes.
#[derive(Debug)]
struct LoggedIn {
    sid: String,
    /// The ack id the next emit on this session rides on.
    next_ack: u32,
    /// What the login pushed, kept current by every read since.
    pushed: Pushed,
    /// When the last write on it finished.
    idle_since: Instant,
}

/// How an emit on a session went.
enum Emitted {
    /// Kuma acknowledged it, with this payload (not yet checked for `ok`).
    Answered(String),
    /// Kuma refused it **without acting on it**, because the session is gone
    /// or no longer logged in: a fresh login may send it again.
    NotDelivered(SourceError),
}

/// The process's sockets, one per instance and account (#983).
type Registry = HashMap<(String, String), Arc<KumaSocket>>;
static SOCKETS: LazyLock<std::sync::Mutex<Registry>> = LazyLock::new(Default::default);

impl KumaSocket {
    /// The socket for this instance and account, shared by every adapter the
    /// process builds for them, so a held session outlives the adapter value
    /// (the module note says why that matters).
    ///
    /// Keyed by base URL and username, and **replaced** when the password or
    /// the configuration differs: a session logged in with the right password
    /// must never carry the write of a source saved with a wrong one, and the
    /// map holds one entry per instance and account rather than one per edit.
    ///
    /// # Errors
    ///
    /// [`KumaSocket::new`]'s.
    pub(crate) fn shared(
        base_url: &str,
        cfg: &crate::KumaConfig,
        account: Account,
    ) -> Result<Arc<Self>, SourceError> {
        let key = (base_url.trim().to_owned(), account.username.clone());
        let mut sockets = SOCKETS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(socket) = sockets.get(&key)
            && socket.account == account
            && socket.cfg == *cfg
        {
            return Ok(Arc::clone(socket));
        }
        let socket = Arc::new(Self::new(base_url, cfg, account)?);
        sockets.insert(key, Arc::clone(&socket));
        Ok(socket)
    }

    /// Build the channel for one configured source.
    ///
    /// **A client of its own, beside [`crate::http::KumaHttp`]**, because the
    /// API key is Basic auth on `/metrics` and means nothing here: sending it
    /// on every polling read would be a credential on the wire for no
    /// purpose.
    ///
    /// **The read half's quota, not one of its own** (#1723). Every client for
    /// one origin waits on one limiter, and the stricter of the quotas its
    /// live clients ask for governs (#1630, spec #1618): one knobas sends a
    /// Kuma at most what its source was saved with, polls included. So the
    /// socket asks for exactly that. A rate of its own could only ever
    /// tighten: the 25 a second this asked for before never sped a poll up,
    /// since every write's adapter holds the read half's client beside it, and
    /// it held a source saved at 100 down to 25, syncs and all, for as long as
    /// the process kept the socket. Measured through a relay in front of the
    /// pinned container (2026-10-22, default 5/s, burst 10): a pause on a
    /// fresh login is ten or eleven requests in 0.1 to 0.2 s, inside the
    /// burst, and a write straight after it on the held session pays 0.2 s a
    /// request for the refill.
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
                requests_per_second: cfg.rate_per_sec,
                burst: cfg.rate_burst,
                connect_timeout: Duration::from_secs(cfg.connect_timeout_secs),
                request_timeout: Duration::from_secs(cfg.request_timeout_secs),
                // Engine.io answers a bad session with express's own text and
                // a refused login inside a `200`; there is no error envelope
                // to lift, and the bounded excerpt is the honest reading.
                body_message: None,
                forbidden: None,
                max_body: knobas_http::MAX_BODY,
            })?,
            account,
            cfg: cfg.clone(),
            budget: CALL_BUDGET,
            idle_reuse: IDLE_REUSE,
            held: tokio::sync::Mutex::new(None),
        })
    }

    /// The same socket with a shorter idle window, for this module's tests.
    #[cfg(test)]
    fn with_idle_reuse(mut self, idle_reuse: Duration) -> Self {
        self.idle_reuse = idle_reuse;
        self
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

    /// Emit `event` with `argument` on a logged-in session -- the held one if
    /// it is still good, a fresh login if not -- and answer what Kuma said.
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
    /// rather than record a refusal (ADR-0004). [`SourceError::Unreachable`]
    /// when Kuma refused the login for its rate limit, which the queue retries
    /// on a later tick. [`SourceError::Protocol`] when
    /// Kuma refuses the call itself (a monitor that is not there, an account
    /// that may not touch it), carrying Kuma's own words; when the session
    /// stays open past [`CALL_BUDGET`] without acknowledging anything **before
    /// the write's own packet went out**; and whatever [`knobas_http`]
    /// classified a transport failure on the way there as.
    /// [`SourceError::Unconfirmed`] for **every failure after that packet
    /// went out** and before its ack was read -- a lost poll, a reaped
    /// session, the deadline (#1627): Kuma may have acted on it, and the write
    /// queue holds a create rather than make a second monitor. And for an
    /// ack that arrived and says neither that Kuma acted nor that it refused
    /// (#1721, [`verdict`]).
    ///
    /// The deadline before the emit is **status-less `Protocol`, which the
    /// write queue classes as a permanent refusal of this write**
    /// (`retryable`), and that is the deliberate reading: a session that
    /// answers its pings but never acknowledges a login is a Kuma whose
    /// protocol has moved, not a Kuma that is down, and an `Unreachable`
    /// would spend a minute of every tick waiting for the drift to fix
    /// itself. After the emit the same deadline cannot be that refusal,
    /// because the write may have landed.
    pub(crate) async fn call(
        &self,
        event: &str,
        argument: &serde_json::Value,
    ) -> Result<serde_json::Value, SourceError> {
        self.call_with(event, |_| Ok(argument.clone())).await
    }

    /// [`KumaSocket::call`], with the argument built from what the login
    /// pushed (#864): a Docker check names one of the account's Docker hosts,
    /// and the only place Kuma says which exist is the `dockerHostList` it
    /// pushes before it acknowledges the login. `build` runs after the login
    /// and before the emit; its error is the call's, and nothing is emitted.
    /// It is `Fn` because a held session that turns out to be gone is
    /// replaced by a fresh login, and the argument is built again from what
    /// that login pushed.
    ///
    /// # Errors
    ///
    /// [`KumaSocket::call`]'s, and whatever `build` refuses with.
    pub(crate) async fn call_with(
        &self,
        event: &str,
        build: impl Fn(&Pushed) -> Result<serde_json::Value, SourceError>,
    ) -> Result<serde_json::Value, SourceError> {
        let mut held = self.held.lock().await;
        let mut emitted = false;
        let Ok(answered) = tokio::time::timeout(
            self.budget,
            self.exchange(&mut held, &mut emitted, event, &build),
        )
        .await
        else {
            // `{:?}` and not `as_secs()`, which is how `knobas_http` prints
            // its own budget: this one is shortened to milliseconds by a test,
            // and `as_secs()` would render that deadline as `0s`.
            if emitted {
                // The write's own packet went out, so Kuma may have acted
                // on it (#1627): held by the queue, never sent again by it.
                return Err(SourceError::Unconfirmed {
                    status: None,
                    message: format!(
                        "Uptime Kuma was sent {event} and did not acknowledge it within {:?}, so it \
                         may have acted on it",
                        self.budget
                    ),
                });
            }
            return Err(SourceError::protocol(format!(
                "Uptime Kuma held its socket.io session open for {:?} without acknowledging \
                 {event}: the session stayed up and answered its pings, so this reads as a change \
                 in Kuma's protocol rather than an outage",
                self.budget
            )));
        };
        verdict(&answered?, event)
    }

    /// The write itself: on the held session if it is still good, else on a
    /// fresh login, and the ack's payload back.
    ///
    /// Split out from [`KumaSocket::call`] so that one deadline can span the
    /// whole of it -- there is no point bounding the poll loop alone when a
    /// handshake that never answers costs the same.
    ///
    /// **The held session is taken out of its slot first** and put back only
    /// once an ack has arrived on it, so every way this can end early -- an
    /// error, or the deadline dropping this future where it stood -- leaves
    /// nothing behind to reuse. A refusal Kuma acknowledged (*You do not own
    /// this monitor*) still puts the session back: the session answered, and
    /// [`verdict`] is what turns the answer into the write's error.
    async fn exchange(
        &self,
        held: &mut Option<LoggedIn>,
        emitted: &mut bool,
        event: &str,
        build: &impl Fn(&Pushed) -> Result<serde_json::Value, SourceError>,
    ) -> Result<String, SourceError> {
        let mut reused = None;
        if let Some(mut session) = held
            .take()
            .filter(|session| session.idle_since.elapsed() < self.idle_reuse)
        {
            match self.emit_on(&mut session, emitted, event, build).await? {
                Emitted::Answered(answered) => reused = Some((session, answered)),
                Emitted::NotDelivered(reason) => {
                    tracing::debug!(%reason, "the held Uptime Kuma session is gone; logging in again");
                }
            }
        }
        let (session, answered) = match reused {
            Some(done) => done,
            None => {
                let mut session = self.log_in().await.map_err(not_yet_emitted)?;
                match self.emit_on(&mut session, emitted, event, build).await? {
                    Emitted::Answered(answered) => (session, answered),
                    Emitted::NotDelivered(reason) => return Err(reason),
                }
            }
        };
        *held = Some(session);
        Ok(answered)
    }

    /// Open a session and log in on it.
    async fn log_in(&self) -> Result<LoggedIn, SourceError> {
        let sid = sid_of(&self.open().await?)?;
        let session = self.session(&sid);
        session.send("40".to_owned()).await?;
        let mut pushed = Pushed::default();
        // Not before Kuma listens for it: a login sent sooner can be dropped
        // without an ack (#1472, the module note).
        session
            .read_until(&mut pushed, |packet| invites_login(packet).then_some(()))
            .await?;
        session
            .send(emit(LOGIN_ACK, "login", &self.login_argument()))
            .await?;
        let login = session.wait_for(LOGIN_ACK, &mut pushed).await?;
        login_verdict(&login, &self.account.username)?;
        Ok(LoggedIn {
            sid,
            next_ack: LOGIN_ACK + 1,
            pushed,
            idle_since: Instant::now(),
        })
    }

    fn session<'a>(&'a self, sid: &'a str) -> Session<'a> {
        Session { socket: self, sid }
    }

    /// Build the argument, emit `event` on `logged_in`, and read its ack.
    /// An ack restarts the session's idle clock.
    ///
    /// [`Emitted::NotDelivered`] only for the two refusals that prove Kuma did
    /// nothing: engine.io's `400` to the emit's own POST (the session is
    /// unknown, so the packet was dropped), and *You are not logged in.*
    /// (Kuma checks the login before it acts). Everything else is an error:
    /// an emit that was accepted and then went unanswered may have landed,
    /// so a failure while its ack is awaited is [`once_emitted`]'s
    /// `Unconfirmed`.
    ///
    /// `emitted` is set as the packet goes out and cleared by either refusal,
    /// so that [`KumaSocket::call_with`] can tell a deadline before the write
    /// was sent from one after (#1627).
    ///
    /// The `400` proves only that *this* POST was dropped, which is why
    /// [`Session::send`] never resends a POST that may have landed (#1122): the `400` can then only be the first POST's.
    async fn emit_on(
        &self,
        logged_in: &mut LoggedIn,
        emitted: &mut bool,
        event: &str,
        build: &impl Fn(&Pushed) -> Result<serde_json::Value, SourceError>,
    ) -> Result<Emitted, SourceError> {
        let argument = build(&logged_in.pushed)?;
        let ack = logged_in.next_ack;
        logged_in.next_ack += 1;
        let session = self.session(&logged_in.sid);
        // Set before the POST, not after it: a deadline that drops the POST
        // in flight drops a packet that may already have arrived.
        *emitted = true;
        match session.send(emit(ack, event, &argument)).await {
            Ok(()) => {}
            Err(
                error @ SourceError::Protocol {
                    status: Some(400), ..
                },
            ) => {
                *emitted = false;
                return Ok(Emitted::NotDelivered(error));
            }
            Err(error) => return Err(error),
        }
        let payload = session
            .wait_for(ack, &mut logged_in.pushed)
            .await
            .map_err(|error| once_emitted(error, event))?;
        if answer(&payload).is_ok_and(|answered| {
            !went_well(&answered)
                && answered.get("msg").and_then(serde_json::Value::as_str) == Some(NOT_LOGGED_IN)
        }) {
            *emitted = false;
            return Ok(Emitted::NotDelivered(SourceError::protocol(format!(
                "Uptime Kuma refused {event}: {NOT_LOGGED_IN}"
            ))));
        }
        logged_in.idle_since = Instant::now();
        Ok(Emitted::Answered(payload))
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

/// A failure on the way to the emit, before the write's own packet went out
/// (#1125): an unanswered login POST says nothing about the write, which was
/// never sent, so it is plain [`SourceError::Unreachable`] and the write
/// queue sends the write again on its backoff ladder rather than holding it.
fn not_yet_emitted(error: SourceError) -> SourceError {
    match error {
        SourceError::Unconfirmed { message, .. } => SourceError::Unreachable(message),
        other => other,
    }
}

/// A failure on the way to the write's ack, after its own packet was
/// accepted (#1627): whatever the transport says about the poll, nothing says
/// whether Kuma acted on the packet, so it is [`SourceError::Unconfirmed`] --
/// `knobas_http::classify::once_sent`'s reading of a request that went out and
/// got no answer. The write queue holds a write a second arrival would
/// duplicate (a create) and sends one that would not (a pause) again later.
///
/// Every class, not only `Unreachable`: a `400 Session ID unknown` on the poll
/// says the session went *after* the packet was taken, and a `401` from a proxy
/// in front of Kuma says nothing about the packet either. The poll's status is
/// kept, so the message still says what the transport saw.
fn once_emitted(error: SourceError, event: &str) -> SourceError {
    let lost = |status, message: &dyn std::fmt::Display| SourceError::Unconfirmed {
        status,
        message: format!(
            "Uptime Kuma accepted {event} and its acknowledgement was lost, so it may have \
             acted on it: {message}"
        ),
    };
    match error {
        SourceError::Unreachable(message) => lost(None, &message),
        SourceError::Protocol { status, message }
        | SourceError::Unconfirmed { status, message } => lost(status, &message),
        SourceError::Unauthorized { status } => lost(status, &"unauthorized"),
        other @ (SourceError::Sink(_) | SourceError::Moved { .. }) => other,
    }
}

/// One open engine.io session: the socket plus the id the server minted.
struct Session<'a> {
    socket: &'a KumaSocket,
    sid: &'a str,
}

impl Session<'_> {
    fn query(&self) -> [(&str, &str); 3] {
        [("EIO", EIO), ("transport", TRANSPORT), ("sid", self.sid)]
    }

    /// POST one packet. The body is the packet verbatim, which is what
    /// [`knobas_http::Request::text`] exists for.
    ///
    /// **Sent once, never retried by the transport** (#1122), unless the
    /// connection never opened. A POST that
    /// timed out may have delivered its packet, and a resent `add` is a second
    /// monitor. Worse, if Kuma reaped the session in between, the resend meets
    /// `400 Session ID unknown`, which [`KumaSocket::emit_on`] rightly reads
    /// as *this* packet dropped -- and the write would go out a third time on
    /// a fresh login. Every packet rather than `add` alone: no emit here
    /// needs the transport's retry, and a failed one comes back as the write's
    /// error for the write queue to rule on.
    async fn send(&self, packet: String) -> Result<(), SourceError> {
        let request = self
            .socket
            .client
            .request(Method::POST, PATH)
            .query(&self.query())
            .text(packet)
            .no_retry();
        self.socket.client.send(request).await?;
        Ok(())
    }

    /// Read until the ack numbered `ack` arrives ([`Session::read_until`]).
    async fn wait_for(&self, ack: u32, pushed: &mut Pushed) -> Result<String, SourceError> {
        self.read_until(pushed, |packet| ack_of(packet, ack).map(str::to_owned))
            .await
    }

    /// Read until `wanted` picks a packet out of the stream: an ack, or
    /// Kuma's invitation to log in (#1472). Pings are answered on the way.
    ///
    /// **No bound of its own, and that is the design**: what stops this loop
    /// when the packet never comes is [`CALL_BUDGET`], the deadline
    /// [`KumaSocket::call`] holds over the whole exchange, which cancels this
    /// future wherever it happens to be waiting. A write on a fresh login
    /// waits three times, and the constant's note says why bounding each wait
    /// separately would not have bounded the write.
    ///
    /// Everything else in the stream is discarded on purpose, but for the
    /// Docker host list ([`Pushed`]): what a login pushes is the dashboard's
    /// whole state, and this module has no use for the rest of it -- reading a
    /// monitor is `/metrics`' job and stays so. Kuma pushes the list again
    /// when it changes, so a held session's copy stays current.
    async fn read_until<T>(
        &self,
        pushed: &mut Pushed,
        wanted: impl Fn(&str) -> Option<T>,
    ) -> Result<T, SourceError> {
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
                } else if let Some(found) = wanted(packet) {
                    return Ok(found);
                } else if let Some(hosts) = docker_hosts_of(packet) {
                    pushed.docker_hosts = Some(hosts);
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

/// What a login pushed that a call may need (#864). Everything else a login
/// pushes is still discarded ([`Session::read_until`]'s note).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Pushed {
    /// The account's Docker hosts by id, lowest first, as `dockerHostList`
    /// named them; `None` when the login pushed no list at all.
    pub(crate) docker_hosts: Option<Vec<i64>>,
}

/// The Docker host ids out of a pushed `42["dockerHostList",[{"id":1,…}]]`,
/// lowest first, or `None` for any other packet. Kuma 2.5.3's `afterLogin`
/// awaits this push before the login's callback runs, so it is in the stream
/// before the login's ack.
fn docker_hosts_of(packet: &str) -> Option<Vec<i64>> {
    let body = packet.strip_prefix("42")?;
    let event: serde_json::Value = serde_json::from_str(body).ok()?;
    let event = event.as_array()?;
    if event.first()?.as_str()? != "dockerHostList" {
        return None;
    }
    let mut ids: Vec<i64> = event
        .get(1)?
        .as_array()?
        .iter()
        .filter_map(|host| host.get("id")?.as_i64())
        .collect();
    ids.sort_unstable();
    Some(ids)
}

/// Whether `packet` is Kuma's invitation to log in, `42["loginRequired"]` or
/// `42["autoLogin"]` ([`LOGIN_INVITATIONS`]).
fn invites_login(packet: &str) -> bool {
    packet
        .strip_prefix("42")
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(|event| {
            event
                .get(0)?
                .as_str()
                .map(|name| LOGIN_INVITATIONS.contains(&name))
        })
        .unwrap_or(false)
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

/// What Kuma answered, as one object: `[{"ok":true,"msg":"successPaused"}]`,
/// or why the ack cannot be read as one.
///
/// **The reason, not the error class**, because the class depends on what
/// was acknowledged (#1721): [`login_verdict`] and [`verdict`] each say which
/// they make of it, and why.
fn answer(payload: &str) -> Result<serde_json::Value, &'static str> {
    let arguments: Vec<serde_json::Value> = serde_json::from_str(payload)
        .map_err(|_| "Uptime Kuma's answer was not a socket.io ack")?;
    arguments
        .into_iter()
        .next()
        .ok_or("Uptime Kuma acknowledged with no answer at all")
}

fn said(answered: &serde_json::Value) -> String {
    answered
        .get("msg")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("no reason given")
        .to_owned()
}

fn went_well(answered: &serde_json::Value) -> bool {
    ok_of(answered) == Some(true)
}

/// Kuma's verdict in an answer, `ok`, if it is the boolean 2.5.3 sends.
fn ok_of(answered: &serde_json::Value) -> Option<bool> {
    answered.get("ok").and_then(serde_json::Value::as_bool)
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
///
/// **Except a refusal for Kuma's rate limit** (#983), which says nothing about
/// the password: it is [`SourceError::Unreachable`], which the write queue
/// retries on a later tick, rather than `Unauthorized`, which would put
/// *Re-enter* on a credential that is right.
///
/// **An ack it cannot read ([`answer`]) is a refusal of the write**, status-less
/// `Protocol` (#1721): nothing of the write was sent, and a Kuma whose ack
/// shape moved answers every login so, which is the reading the deadline
/// before the emit takes too ([`KumaSocket::call`]).
fn login_verdict(payload: &str, username: &str) -> Result<(), SourceError> {
    let answered = answer(payload).map_err(SourceError::protocol)?;
    if went_well(&answered) {
        return Ok(());
    }
    if said(&answered) == RATE_LIMITED {
        tracing::warn!(%username, "Uptime Kuma refused the login for its rate limit");
        return Err(SourceError::Unreachable(format!(
            "Uptime Kuma refused the login for its rate limit (twenty a minute): {RATE_LIMITED}"
        )));
    }
    tracing::warn!(
        %username,
        reason = %said(&answered),
        "Uptime Kuma refused the account"
    );
    Err(SourceError::unauthorized())
}

/// The call's verdict: Kuma's own answer, [`SourceError::Protocol`] carrying
/// Kuma's own words, or [`SourceError::Unconfirmed`] for an ack that says
/// neither (#1721).
///
/// Protocol and not `Unauthorized`, even for *You do not own this monitor* --
/// the account logged in, so re-entering it changes nothing, and a write that
/// waits for a credential that is already right would wait for ever. It is a
/// refusal of this write, which is what the queue records and shows.
///
/// **Only `ok: false` is a refusal.** An ack knobas cannot read ([`answer`]),
/// or one with no boolean `ok`, arrived after Kuma took the packet and says
/// nothing about what it did with it, so it is `Unconfirmed`: the queue holds
/// a create for the reader and sends a pause or resume again on its ladder.
/// A create recorded as refused would leave a monitor knobas says it never
/// made, and a reader who sends it again makes a second one.
/// The pinned 2.5.3 never answers that way; a Kuma whose ack shape changed
/// does, on every write.
fn verdict(payload: &str, event: &str) -> Result<serde_json::Value, SourceError> {
    let unread = |why: &str| SourceError::Unconfirmed {
        status: None,
        message: format!(
            "Uptime Kuma acknowledged {event} in a shape knobas cannot read, so it may have \
             acted on it: {why} ({})",
            payload.chars().take(ACK_EXCERPT).collect::<String>()
        ),
    };
    let answered = answer(payload).map_err(unread)?;
    match ok_of(&answered) {
        Some(true) => Ok(answered),
        Some(false) => Err(SourceError::protocol(format!(
            "Uptime Kuma refused {event}: {}",
            said(&answered)
        ))),
        None => Err(unread("its answer carries no `ok`")),
    }
}

/// How much of an unreadable ack its error quotes: enough to name the shape
/// a changed Kuma answers in, and no more of a document that may be large.
const ACK_EXCERPT: usize = 120;

#[cfg(test)]
#[path = "socket_session_tests.rs"]
mod session_tests;

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;

    /// **A Kuma that stays up and never acknowledges anything.**
    ///
    /// The one fake in this adapter. It stands on ADR-0013's consequence for
    /// a fault the real instance cannot produce (ruled 2026-09-07, after PR
    /// #488): a released Kuma cannot be asked to withhold an ack, since what
    /// would produce one is a *future* release that renames an event, so no
    /// recipe can drive the pinned container into it. For that one criterion
    /// this fake is the witness; what certifies the adapter -- this write path
    /// included -- is still `just kuma-live` against the real container, and
    /// the bound below is the only thing in this module with no live
    /// counterpart.
    ///
    /// It is faithful to the exchange in every other respect: the handshake
    /// mints a session, `40` is accepted, the first read invites the login
    /// (#1472), the login emit and every pong are accepted, and each poll
    /// after that answers a bare `2` after `hold` -- which is how a *healthy*
    /// idle engine.io session behaves, at Kuma's own 25 s ping interval. That
    /// is the point: nothing here is an error the transport could classify.
    ///
    /// Hand-rolled on the test's own runtime rather than wiremock, whose
    /// server keeps the wall clock on a thread of its own: `hold` is a
    /// `tokio::time::sleep`, so it passes on the same paused clock as the
    /// deadline it is measured against (#1925).
    struct PingingKuma {
        port: u16,
        /// The session reads that arrived, the handshake excluded: the
        /// first is answered at once, and every later one is held.
        polls: tokio::sync::watch::Receiver<usize>,
    }

    impl PingingKuma {
        /// `hold` stands in for the ping interval, scaled down so a test can
        /// watch several polls go by inside a sub-second deadline.
        async fn start(hold: Duration) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let port = listener.local_addr().expect("addr").port();
            let (arrived, polls) = tokio::sync::watch::channel(0);
            let arrived = Arc::new(arrived);
            tokio::spawn(async move {
                while let Ok((socket, _)) = listener.accept().await {
                    tokio::spawn(Self::answer(socket, hold, Arc::clone(&arrived)));
                }
            });
            Self { port, polls }
        }

        /// One request on `socket`, answered as Kuma would, then the
        /// connection closed.
        async fn answer(
            mut socket: tokio::net::TcpStream,
            hold: Duration,
            arrived: Arc<tokio::sync::watch::Sender<usize>>,
        ) {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut request = Vec::new();
            let mut chunk = [0u8; 4096];
            let head_end = loop {
                if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break end + 4;
                }
                match socket.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(read) => request.extend_from_slice(&chunk[..read]),
                }
            };
            let head = String::from_utf8_lossy(&request[..head_end]).into_owned();
            // The body is read whole before answering, so closing the
            // connection never resets it under an unread POST.
            let length = head
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            while request.len() < head_end + length {
                match socket.read(&mut chunk).await {
                    Ok(0) | Err(_) => return,
                    Ok(read) => request.extend_from_slice(&chunk[..read]),
                }
            }
            let target = head.split(' ').nth(1).unwrap_or_default();
            let body = if head.starts_with("POST ") {
                // `40`, the emits and the pongs: engine.io answers `ok`.
                "ok"
            } else if !target.contains("sid=") {
                // The handshake: the one GET that carries no session yet.
                r#"0{"sid":"never-answers","upgrades":[],"pingInterval":25000,"pingTimeout":20000}"#
            } else {
                let mut first = false;
                arrived.send_modify(|polls| {
                    *polls += 1;
                    first = *polls == 1;
                });
                if first {
                    // The first read of the session: the namespace opened and
                    // Kuma listening for the login, as its connection handler
                    // ends.
                    "40{\"sid\":\"socket\"}\u{1e}42[\"loginRequired\"]"
                } else {
                    // Every read after it: a heartbeat, for ever.
                    tokio::time::sleep(hold).await;
                    "2"
                }
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
        }

        fn base_url(&self) -> String {
            format!("http://127.0.0.1:{}", self.port)
        }

        /// How many reads of the session the client actually made -- the
        /// handshake excluded, since it carries no `sid`.
        fn polls(&self) -> usize {
            *self.polls.borrow()
        }

        /// Wait for a read past the first `seen`: one the fake now holds.
        async fn held_after(&self, seen: usize) {
            let mut polls = self.polls.clone();
            polls
                .wait_for(|polls| *polls > seen)
                .await
                .expect("the fake outlives its handle");
        }
    }

    /// A socket for the fake, with `budget` for its deadline. Its quota is
    /// far past what a run sends, so no read waits on the limiter: the
    /// reads that fit inside the deadline are the fake's `hold` apart and
    /// nothing else.
    fn socket_to(base_url: &str, budget: Duration) -> KumaSocket {
        KumaSocket::new(
            base_url,
            &crate::KumaConfig {
                rate_per_sec: 1_000,
                rate_burst: 2_000,
                ..crate::KumaConfig::default()
            },
            Account {
                username: "knobas".to_owned(),
                password: "knobas-dev".to_owned(),
            },
        )
        .expect("the socket builds against an http url")
        .with_budget(budget)
    }

    /// tokio's paused clock, held still until this is dropped (#1925): a
    /// running blocking task stops it jumping to the next timer while the
    /// runtime waits on a socket (`tokio::time::pause`, *Preventing
    /// auto-advance*), so it moves only by `tokio::time::advance`, and a
    /// request on the loopback takes no time on it.
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

    /// One run against the fake: how long [`KumaSocket::call`] took on
    /// tokio's paused clock, how many reads it made, and what it finally
    /// said.
    ///
    /// The clock is held, and moved by the run itself: each time the fake
    /// holds a read, by `hold`, which answers it -- in two steps if the
    /// deadline falls inside it, the first to the deadline. Between
    /// two moves the client is waiting on the read the fake holds and nothing
    /// else -- every read and POST of a session goes one after the other --
    /// so no timer fires under a request in flight, and the run is the same
    /// every time: the deadline at `budget` exactly, after as many reads as
    /// fit inside it.
    ///
    /// The **real-time** backstop is twenty seconds of wall time, so the
    /// code *without* its deadline -- [`Session::read_until`] loops for ever,
    /// moved on by this loop -- or a call that stops reading to wait on a
    /// timer the held clock never fires fails here rather than hanging `just
    /// check`. It is a hang backstop and nothing more: the run itself takes no
    /// wall time worth naming.
    async fn until_it_gives_up(budget: Duration, hold: Duration) -> (Duration, usize, SourceError) {
        let kuma = PingingKuma::start(hold).await;
        let clock = HeldClock::hold();
        let socket = socket_to(&kuma.base_url(), budget);
        let (fired, backstop) = tokio::sync::oneshot::channel::<()>();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(20));
            let _ = fired.send(());
        });
        tokio::pin!(backstop);

        let started = tokio::time::Instant::now();
        let monitor = serde_json::json!(8);
        let call = socket.call(PAUSE_EVENT, &monitor);
        tokio::pin!(call);
        let mut seen = 1;
        let refused = loop {
            tokio::select! {
                biased;
                answered = &mut call => {
                    break answered.expect_err("a session that never acknowledges cannot answer a write");
                }
                _ = &mut backstop => {
                    panic!(
                        "still running after {} reads, twenty seconds of real time on: the call \
                         has no deadline",
                        kuma.polls()
                    );
                }
                () = kuma.held_after(seen) => {
                    seen = kuma.polls();
                    // The deadline, if it falls inside this hold: the call
                    // ends there, or it has failed to fire and the rest of
                    // the hold answers the read. Past the budget, the
                    // session is moved on by whole holds until the backstop.
                    let left = budget.saturating_sub(started.elapsed());
                    if !left.is_zero() && left < hold {
                        tokio::time::advance(left).await;
                        let at_the_deadline =
                            std::future::poll_fn(|cx| Poll::Ready(call.as_mut().poll(cx))).await;
                        if let Poll::Ready(answered) = at_the_deadline {
                            break answered
                                .expect_err("a session that never acknowledges cannot answer a write");
                        }
                        tokio::time::advance(hold - left).await;
                    } else {
                        tokio::time::advance(hold).await;
                    }
                }
            }
        };
        let took = started.elapsed();
        drop(clock);
        (took, kuma.polls(), refused)
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
    /// Not in `tests/it/contract.rs`, where issue #486 placed it, and the
    /// deviation is recorded rather than taken quietly: [`KumaSocket`] is
    /// `pub(crate)`, so the test-only deadline override the same paragraph
    /// asks for is unreachable from an integration test -- which leaves a test
    /// that waits the production minute, or this.
    #[tokio::test(start_paused = true)]
    async fn a_session_that_only_pings_is_given_up_on_when_the_time_is_gone() {
        // Not a divisor of either budget, so the deadline never falls on the
        // instant a hold ends, where which of the two fires first would
        // decide the count.
        const HOLD: Duration = Duration::from_millis(120);
        const SHORT: Duration = Duration::from_millis(500);
        const LONG: Duration = Duration::from_millis(2_000);

        let (elapsed, polls, refused) = until_it_gives_up(SHORT, HOLD).await;

        let (message, status) = match refused {
            SourceError::Protocol { message, status } => (message, status),
            other => panic!("expected Protocol, got {other:?}"),
        };
        // Status-less, which is what makes it a permanent refusal of this
        // write rather than something the queue retries on its ladder.
        assert_eq!(status, None, "{message}");
        assert!(
            message.contains("500ms"),
            "the message has to name the deadline it tripped: {message}"
        );
        assert!(message.contains(PAUSE_EVENT), "{message}");

        // On tokio's paused clock (#1925) both runs end at their deadline
        // exactly, which is the floor and the ceiling at once: a deadline
        // short of what it was given, or one with a wait of its own on top,
        // is off by that much. The reads are exact too: the invitation, then
        // one held read every 120 ms the deadline leaves room to start, the
        // last of them still held when it fires.
        assert_eq!(elapsed, SHORT, "gave up off the deadline");
        assert_eq!(polls, 1 + 5, "the invitation and the reads at 0..=480 ms");

        let (longer, more, _) = until_it_gives_up(LONG, HOLD).await;
        assert_eq!(longer, LONG, "the longer deadline was given up on off it");
        assert_eq!(
            more,
            1 + 17,
            "the invitation and the reads at 0..=1920 ms: the reads one write makes have to \
             be whatever fit inside its own deadline, not a fixed count"
        );
    }

    /// Kuma's two invitations open the way for the login; nothing pushed
    /// before them does, `info` included, which Kuma sends *before* it
    /// registers `login` (#1472).
    #[test]
    fn only_kuma_s_invitation_lets_the_login_go() {
        assert!(invites_login(r#"42["loginRequired"]"#));
        assert!(invites_login(r#"42["autoLogin"]"#));
        for not_yet in [
            r#"40{"sid":"socket"}"#,
            r#"42["info",{"primaryBaseURL":null}]"#,
            r#"42["loginRequiredSoon"]"#,
            r#"431[{"ok":true}]"#,
            "2",
        ] {
            assert!(!invites_login(not_yet), "{not_yet}");
        }
    }

    /// The Docker hosts a login pushes, read by id; any other push is not
    /// mistaken for them, and an empty list is a list (#864).
    #[test]
    fn the_docker_hosts_come_out_of_the_pushed_list() {
        assert_eq!(
            docker_hosts_of(
                r#"42["dockerHostList",[{"id":4,"name":"b","dockerDaemon":"/var/run/docker.sock"},{"id":2,"name":"a"}]]"#
            ),
            Some(vec![2, 4])
        );
        assert_eq!(docker_hosts_of(r#"42["dockerHostList",[]]"#), Some(vec![]));
        assert_eq!(docker_hosts_of(r#"42["monitorList",{}]"#), None);
        assert_eq!(docker_hosts_of(r#"431[{"ok":true}]"#), None);
        assert_eq!(docker_hosts_of("2"), None);
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

    /// **#1723.** The polls ask the instance for the quota its source was
    /// saved with, the read half's, so the socket neither tightens a quota a
    /// reader raised nor claims a rate of its own. Read back through a second
    /// holder of the same origin, whose looser ask the socket's then governs.
    #[test]
    fn the_polls_ask_for_the_sources_own_quota() {
        for (origin, per_second, burst) in [
            ("http://quota-raised.kuma.test", 100, 200),
            ("http://quota-default.kuma.test", 5, 10),
        ] {
            let _socket = KumaSocket::new(
                origin,
                &crate::KumaConfig {
                    rate_per_sec: per_second,
                    rate_burst: burst,
                    ..crate::KumaConfig::default()
                },
                Account {
                    username: "knobas".to_owned(),
                    password: "knobas-dev".to_owned(),
                },
            )
            .expect("the socket builds against an http url");
            let probe =
                knobas_http::InstanceLimiter::for_url(origin, 1_000, 2_000).expect("an http url");
            let governs = format!("{probe:?}");
            assert!(
                governs.contains(&format!("per_second: {per_second},"))
                    && governs.contains(&format!("burst: {burst} ")),
                "{origin}: {governs}"
            );
        }
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

    /// An answer knobas cannot read is never a silent success -- a write
    /// reported as landed because its ack was unparseable is the one failure a
    /// reader could not recover from. To the write's own emit it is
    /// `Unconfirmed`, because Kuma took the packet (#1721); to the login it is
    /// a protocol fault, because nothing of the write was sent.
    #[test]
    fn an_unreadable_answer_is_a_fault_rather_than_a_success() {
        for unreadable in ["", "null", "[]", "{\"ok\":true}", "not json"] {
            assert!(
                matches!(
                    verdict(unreadable, PAUSE_EVENT),
                    Err(SourceError::Unconfirmed { status: None, .. })
                ),
                "{unreadable:?} must not read as a successful pause, nor as a refused one"
            );
            assert!(
                matches!(
                    login_verdict(unreadable, "knobas"),
                    Err(SourceError::Protocol { status: None, .. })
                ),
                "{unreadable:?} must not read as a successful login"
            );
        }
        // An ack with no `ok` at all is no verdict, not an assumption either way.
        assert!(matches!(
            verdict(r#"[{"msg":"who knows"}]"#, PAUSE_EVENT),
            Err(SourceError::Unconfirmed { .. })
        ));
    }
}
