//! `knobas-mockd` — faithful, stateful HTTP mocks of the source APIs knobas
//! cannot self-host, backed by the Tidewater Freight fixture.
//!
//! Two ways in, one behaviour:
//!
//! * **In-process** (`spawn_mock_jira()`, `spawn_mock_teamcity()`,
//!   `spawn_all()`): binds `127.0.0.1:0`, returns a guard that shuts the server
//!   down on `Drop`. This is what adapter integration tests use — fast,
//!   deterministic, no Docker, green in CI.
//! * **As a container** (`src/bin/mockd.rs`): the same routers on the fixed
//!   ports of the interfaces doc §5, for the `testenv/` compose environment.
//!
//! Every request is checked against a path/verb/query allowlist **generated at
//! build time from `testenv/specs/jira-dc-rest.wadl`**. A request the contract
//! does not define gets a real error shape *and* a [`Violation`], so an adapter
//! that invents an endpoint fails its own test suite instead of passing against
//! a lenient mock.
//!
//! ## Documented deviations from the real APIs
//!
//! Ruled acceptable by the orchestrator (interfaces doc §8, P11). Do not
//! "fix" these — each one is here because the alternative buys nothing:
//!
//! 1. **TeamCity without `Accept: application/json` gets 406 + `X-Mockd-Hint`**,
//!    where real TeamCity would serve XML. Writing an XML serializer to reward a
//!    bug is waste; the adapter fails either way, and this way it fails legibly.
//!    `Accept: */*` counts as "not JSON" — reqwest sends that by default, so
//!    this is precisely the guard that forces the header to be explicit.
//! 2. **Jira request validation is a WADL-derived path/verb/query allowlist**,
//!    not schema validation of request bodies: Atlassian publishes no
//!    machine-readable DC request schema (`testenv/specs/README.md`).
//! 3. **Unknown query parameters are refused with 400.** Real Jira ignores
//!    them. mockd is stricter on purpose: a parameter that does nothing is a
//!    bug the adapter author must see.
//! 4. **Every Jira endpoint requires an `Authorization` header**, including
//!    `/rest/api/2/serverInfo`, which a real anonymous-browsing instance would
//!    serve without one. Same reason: credentials must be on every request.
//! 5. **`fields=` and `expand=` values are validated against a closed set**;
//!    real Jira ignores names it does not know. A typo that silently drops a
//!    field from a sync is worth a 400.
//! 6. **TeamCity `fields=` is mandatory on the collections, strict about
//!    names, and only `$long` of its presets is honoured.** Real TeamCity has a
//!    default projection and silently drops names it does not know; mockd
//!    answers 400 + [`ViolationKind::UnknownField`]. `$short` and `$locator`
//!    are refused rather than approximated — a mock that guesses at a preset's
//!    contents teaches the adapter a shape the real server does not serve.
//! 7. **No wiki rendering.** `expand=renderedFields` returns the description
//!    verbatim rather than the HTML a real instance would render, because
//!    nothing in knobas reads the rendering — only that the field is there.
//! 8. **[`MockFault`] carries payloads and a `None` variant**, where the SPI's
//!    `knobas_source::contract::Fault` does not. mockd has to reproduce a
//!    concrete `Retry-After` value and a concrete hang duration, which a
//!    payload-free enum cannot express.
//! 9. **A 501 carries an `Allow` header naming mockd's routing table.** Both
//!    routers send a declared-but-unserved verb to their `unimplemented`
//!    fallback, and axum attaches `Allow` to that response downstream of the
//!    handler. The header therefore describes what mockd routes rather than
//!    what the contract declares. Removing it inside the handler does **not**
//!    work (verified in PR #14's review — axum adds it after the handler
//!    returns), and both real fixes are worse than the wart: a `map_response`
//!    layer that strips `Allow` from every 501 would also strip a legitimate
//!    one, and dropping `method_not_allowed_fallback` reinstates the bare 405
//!    with no violation at all, which is the failure mode the fallback exists
//!    to remove. The status code, the body and the violation are all correct;
//!    only the header is noise.
//! 10. **TeamCity's `/app/rest/users/current` reports the same e-mail address
//!     Jira's `myself` does** (`mara.lindqvist@tidewater.example`). One fake
//!     company has one seat, and a cross-source test over [`spawn_all`] sees a
//!     single coherent person rather than two spellings of Mara.
//! 11. **A TeamCity object's absent keys are omitted, never `null`.** The
//!     serialisers build every key an object can have so that `fields=` has a
//!     stable set of known names to validate against; the projection then drops
//!     the nulls, which is TeamCity's own wire shape. The consequence is that
//!     `fields=` accepts a name that is legal for the *type* even when this
//!     particular instance does not carry it — asking a finished build for
//!     `running-info` is a 200 without the key, not a 400.
//! 12. **A build's `triggered` comes from the fixture and from nowhere else.**
//!     `fixtures/tidewater/work.json` gives build 1188 a `triggered_by` and the
//!     other two none, so 1188 is `type: "user"` with that person's `username`
//!     and `name`, and the rest are `type: "vcs"` with no `user` at all — which
//!     is exactly what a real server serves for a branch build. This is a
//!     deviation only in that mockd could invent a person and refuses to: an
//!     invented one would reach `SyncItem::author` and be indexed and searched
//!     as if the dataset had said it. The consequence of a `null` `user` is
//!     deviation 11 one level down — a null carries no key set, so a *sub*-name
//!     of it cannot be validated: `triggered(user(nosuchfield))` is a 400
//!     against an answer containing 1188 and a 200 against one that does not.
//!
//! ## The shared credentials
//!
//! Adapter tests send [`JIRA_TOKEN`] and [`TEAMCITY_TOKEN`]. mockd accepts any
//! non-empty `Bearer`/`Basic` credential; the constants exist so no test
//! hard-codes a string that silently stops meaning anything.

pub mod admin;
pub mod allowlist;
pub mod jira;
pub mod jql;
pub mod state;
pub mod tc_fields;
pub mod tc_state;
pub mod teamcity;
pub mod validate;

use std::net::SocketAddr;
use std::sync::Arc;

pub use state::{MockFault, MockState};
pub use tc_state::{TC_DATE_FMT, TcBuild, TcBuildType, TcState, TcStatus};
pub use validate::{Violation, ViolationKind, ViolationLog};

/// The credential adapter tests should send.
///
/// mockd accepts **any** non-empty `Bearer`/`Basic` credential — rejection is
/// what [`MockFault::Unauthorized`] is for — but a shared constant means no
/// test hard-codes a string that silently stops meaning anything.
pub const JIRA_TOKEN: &str = "mockd-jira-token";

/// Same, for the TeamCity side.
pub const TEAMCITY_TOKEN: &str = "mockd-teamcity-token";

/// A running mock Jira on `127.0.0.1:0`, shut down when the guard drops.
///
/// # Examples
///
/// ```
/// # tokio::runtime::Runtime::new().unwrap().block_on(async {
/// let jira = knobas_mockd::spawn_mock_jira().await;
/// let body = reqwest::Client::new()
///     .get(format!("{}/rest/api/2/serverInfo", jira.base_url()))
///     .header("Authorization", format!("Bearer {}", knobas_mockd::JIRA_TOKEN))
///     .send().await.unwrap()
///     .json::<serde_json::Value>().await.unwrap();
/// assert_eq!(body["deploymentType"], "Server");
/// jira.assert_no_violations();
/// # });
/// ```
pub async fn spawn_mock_jira() -> MockServer {
    let state = MockState::from_fixture();
    serve(jira::router(state.clone()), state, "jira").await
}

/// A running mock TeamCity on `127.0.0.1:0`, shut down when the guard drops.
///
/// # Examples
///
/// ```
/// # tokio::runtime::Runtime::new().unwrap().block_on(async {
/// let tc = knobas_mockd::spawn_mock_teamcity().await;
/// let body = reqwest::Client::new()
///     .get(format!("{}/app/rest/server", tc.base_url()))
///     .header("Accept", "application/json")
///     .header("Authorization", format!("Bearer {}", knobas_mockd::TEAMCITY_TOKEN))
///     .send().await.unwrap()
///     .json::<serde_json::Value>().await.unwrap();
/// assert_eq!(body["role"], "main_node");
/// tc.assert_no_violations();
/// # });
/// ```
pub async fn spawn_mock_teamcity() -> MockServer {
    let state = MockState::from_fixture();
    serve(teamcity::router(state.clone()), state, "teamcity").await
}

/// Both mock servers over **one** [`MockState`], each on its own ephemeral
/// port.
///
/// One state and not two because a cross-source test — a TeamCity build whose
/// branch carries a Jira issue key — has to see one coherent company. Each
/// server still owns its own base URL, so the links in a Jira body point at the
/// Jira port and the links in a TeamCity body point at the TeamCity one.
pub async fn spawn_all() -> MockCluster {
    let state = MockState::from_fixture();
    let jira = serve(jira::router(state.clone()), state.clone(), "jira").await;
    let teamcity = serve(teamcity::router(state.clone()), state, "teamcity").await;
    MockCluster { jira, teamcity }
}

/// Every mock server in one handle. Dropping it shuts both down.
#[derive(Debug)]
pub struct MockCluster {
    pub jira: MockServer,
    pub teamcity: MockServer,
}

impl MockCluster {
    /// The state behind **both** servers.
    pub fn state(&self) -> Arc<MockState> {
        self.jira.state()
    }

    /// # Panics
    ///
    /// If either server recorded a contract violation. The two share one log,
    /// so this reports everything once.
    pub fn assert_no_violations(&self) {
        self.jira.assert_no_violations();
    }

    pub fn violations(&self) -> Vec<Violation> {
        self.jira.violations()
    }

    /// Shuts both servers down and waits for their tasks to finish.
    pub async fn stop(self) {
        self.jira.stop().await;
        self.teamcity.stop().await;
    }
}

async fn serve(app: axum::Router, state: Arc<MockState>, api: &'static str) -> MockServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind 127.0.0.1:0");
    let addr = listener
        .local_addr()
        .expect("a bound listener has an address");
    state.set_base_url(api, &format!("http://{addr}"));
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await
            .ok();
    });
    MockServer {
        addr,
        state,
        api,
        shutdown: Some(tx),
        handle: Some(handle),
    }
}

/// A handle on one running mock server. Dropping it shuts the server down.
#[derive(Debug)]
pub struct MockServer {
    addr: SocketAddr,
    state: Arc<MockState>,
    /// Which API this is, for [`MockServer::assert_no_violations`]'s message.
    api: &'static str,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl MockServer {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `"http://127.0.0.1:<port>"`, with no trailing slash.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn state(&self) -> Arc<MockState> {
        self.state.clone()
    }

    pub fn violations(&self) -> Vec<Violation> {
        self.state.violations().snapshot()
    }

    /// # Panics
    ///
    /// If this server recorded any contract violation.
    pub fn assert_no_violations(&self) {
        self.state.violations().assert_empty(self.api);
    }

    pub fn set_fault(&self, fault: MockFault) {
        self.state.set_fault(fault);
    }

    pub fn touch_issue(&self, key: &str) {
        self.state.touch_issue(key);
    }

    /// Shuts the server down and waits for the task to finish.
    ///
    /// [`Drop`] does the same thing minus the wait — it cannot await — so this
    /// is the version to use when the next assertion is that the port is dead.
    pub async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.await;
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

/// A URL on a port nothing is listening on — the deterministic way to make an
/// adapter classify `SourceError::Unreachable` without waiting out a timeout.
///
/// Binds an ephemeral port, reads its number and closes it again. The OS may
/// hand that port to something else before the caller connects; the window is
/// microseconds, and no test in this workspace binds ephemeral ports
/// concurrently with an assertion on this URL.
pub fn refused_url() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let addr = l.local_addr().expect("a bound listener has an address");
    drop(l);
    format!("http://{addr}")
}
