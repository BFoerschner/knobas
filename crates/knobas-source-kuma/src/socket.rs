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

/// How many polling reads one answer may take.
///
/// A login pushes the whole dashboard's state before its own ack, which is
/// about fifty-five packets and took nine reads against the pinned image, and
/// the call after it takes another three. The cap is what stops a server that
/// answers `2` (ping) for ever from turning one write into an unbounded loop;
/// it is deliberately far above what a working exchange needs, because a
/// *low* cap would fail exactly on the estate with the most monitors.
const MAX_POLLS: usize = 200;

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
                connect_timeout: std::time::Duration::from_secs(cfg.connect_timeout_secs),
                request_timeout: std::time::Duration::from_secs(cfg.request_timeout_secs),
                // Engine.io answers a bad session with express's own text and
                // a refused login inside a `200`; there is no error envelope
                // to lift, and the bounded excerpt is the honest reading.
                body_message: None,
            })?,
            account,
        })
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
    /// that may not touch it), carrying Kuma's own words; and whatever
    /// [`knobas_http`] classified a transport failure as.
    pub(crate) async fn call(
        &self,
        event: &str,
        argument: &serde_json::Value,
    ) -> Result<serde_json::Value, SourceError> {
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

        // Best-effort: a session left open is one Kuma reaps on its own ping
        // timeout, and a write that succeeded must not be reported as failed
        // because the goodbye did not arrive.
        if let Err(error) = session.send("41".to_owned()).await {
            tracing::debug!(%error, "closing the Uptime Kuma socket.io session");
        }
        verdict(&answered, event)
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
    /// Everything else in the stream is discarded on purpose: what a login
    /// pushes is the dashboard's whole state, and this module has no use for
    /// any of it -- reading a monitor is `/metrics`' job and stays so.
    async fn wait_for(&self, ack: u32) -> Result<String, SourceError> {
        for _ in 0..MAX_POLLS {
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
        Err(SourceError::protocol(format!(
            "Uptime Kuma did not answer within {MAX_POLLS} reads of its socket.io session"
        )))
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
