//! The Tidewater Freight fixture, transcribed into Jira Data Center shapes and
//! made mutable so a sync can be observed happening.
//!
//! ## The transcription rules
//!
//! `knobas_source_mock::fixture()` is prose made machine-readable; Jira needs a
//! few things the dataset leaves open. These rules are deterministic and are
//! asserted by value in `tests/state.rs`, because stream A derives its own
//! expectations from the same fixture — they are a cross-stream contract, not
//! an implementation detail.
//!
//! | Jira field | Rule |
//! |---|---|
//! | `id` | `10000 + index in fixture().tickets`. PAY-200 = 10000 … OPS-77 = 10006. |
//! | `project` | the key's prefix before `-` (`PAY`, `OPS`). |
//! | `assignee`/`author` | `fixture().person(id).username` (`"mara.lindqvist"`), never the short handle. |
//! | `reporter` | `person(assigned_by ?? assignee ?? "priya").username`. |
//! | `updated` | `ticket.updated` if set. PAY-200 is the fixture's only null, and Jira always has an `updated`, so it is synthesized as `fixture().today - 30 days` = **`2026-07-23T14:32:00Z`** (`2026-07-23T16:32:00.000+0200` on the wire). |
//! | `created` | `updated - 14 days`. |
//! | comment `id` | `20000 + running index over all comments in fixture order`. |
//! | worklog `id` | `30000 + running index`; `started` = the worklog's date at 09:00 UTC; `timeSpentSeconds` = `minutes * 60`. |
//! | `parent` | the ticket's `epic`, resolved to that issue. Absent where the fixture names none (PAY-200 is itself the epic; OPS-77 belongs to none). |
//! | [`jira::EPIC_LINK_FIELD`](crate::jira::EPIC_LINK_FIELD) | the same `epic`, in the classic Data Center spelling: the epic's bare **key**. `null` — not absent — where the fixture names none, which is how Jira serves a requested custom field with no value. |
//! | `issuelinks` | one link per `blocked_by` entry, served at **both** ends: `inwardIssue` on the blocked issue, `outwardIssue` on the blocker, sharing one `id` = `40000 + running index over all `blocked_by` entries in fixture order`. |
//! | `resolution` | `Done` (id `10000`) exactly when the status is in the `done` category, `null` otherwise — Jira sets a resolution when and only when an issue reaches a done status. |
//! | `timeoriginalestimate` | `estimate_h * 3600`, `null` where the fixture records no estimate. |
//! | `timespent` | the sum of the issue's worklog seconds, `null` where it has none. |
//! | `labels` | always `[]`. The dataset names no labels and mockd will not invent any (the #28 ruling: an invented value reaches `SyncItem::payload` and is indexed as if the dataset had said it). |
//! | clock | starts at `fixture().today` and advances **exactly one minute** per mutation (`touch_issue`, `add_comment`, `transition_issue`, `create_issue`, `queue_build`, and the build finishers) — JQL time resolution is one minute, so a smaller step would make two touches indistinguishable to the very query an adapter runs. |
//!
//! ## The server is deliberately not on UTC
//!
//! A real Jira DC instance runs in its operator's timezone, `serverInfo.serverTime`
//! is where an adapter learns which one, and **JQL date literals are interpreted
//! in that zone**. An adapter that formats its watermark in UTC and sends it to a
//! `+02:00` server silently re-fetches (or skips) two hours of issues on every
//! incremental run. A mock on UTC would let that bug through, so
//! [`DEFAULT_SERVER_OFFSET_SECS`] is `+02:00` and [`MockState::set_server_offset`]
//! lets a test move it.
//!
//! `serverInfo` cannot carry a separate timezone field: the WADL schema is
//! `additionalProperties: false` and declares none, so the offset on `serverTime`
//! genuinely is the only channel — exactly as on a real instance.

use std::sync::{Arc, Mutex, RwLock};

use chrono::{DateTime, Duration, FixedOffset, NaiveTime, TimeZone, Utc};
use knobas_source_mock::fixture;

use crate::tc_state::{TcBuild, TcBuildType, TcState, TcStatus};
use crate::validate::ViolationLog;

/// Jira DC serialises timestamps as `2026-08-22T13:48:00.000+0200` — **not**
/// RFC 3339 (no colon in the offset).
/// `chrono::DateTime::parse_from_rfc3339` rejects it.
pub const JIRA_DATE_FMT: &str = "%Y-%m-%dT%H:%M:%S%.3f%z";

/// The server zone mockd reports and interprets JQL literals in.
/// `+02:00` = Europe/Berlin in August, which is when the fixture lives.
pub const DEFAULT_SERVER_OFFSET_SECS: i32 = 2 * 3600;

/// The default page size cap, standing in for `jira.search.views.default.max`.
const DEFAULT_MAX_RESULTS_CAP: u32 = 100;

/// Renders `t` **in the server's zone**, not in UTC.
pub fn jira_date(t: DateTime<Utc>, off: FixedOffset) -> String {
    t.with_timezone(&off).format(JIRA_DATE_FMT).to_string()
}

/// One issue, in the shape the Jira serialisers need.
#[derive(Debug, Clone)]
pub struct JiraIssue {
    pub id: u64,
    pub key: String,
    pub project: String,
    pub summary: String,
    pub description: Option<String>,
    pub issue_type: String,
    pub status: String,
    pub priority: Option<String>,
    pub assignee: Option<String>,
    pub reporter: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub comments: Vec<JiraComment>,
    pub worklogs: Vec<JiraWorklog>,
    /// The epic this belongs to, as `fields.parent`.
    pub parent: Option<JiraIssueRef>,
    /// `fields.issuelinks`, this issue's end of each of them.
    pub links: Vec<JiraLink>,
    /// `fields.timeoriginalestimate`, in seconds.
    pub original_estimate_secs: Option<u64>,
}

impl JiraIssue {
    /// `fields.timespent`: the sum of what has been logged, or `None` when
    /// nothing has. `None` rather than `0`, because Jira reports zero for an
    /// issue whose logged time was deleted and null for one that never had
    /// any.
    #[must_use]
    pub fn time_spent_secs(&self) -> Option<u64> {
        (!self.worklogs.is_empty())
            .then(|| self.worklogs.iter().map(|w| w.time_spent_seconds).sum())
    }
}

/// The other end of a `parent` or an `issuelinks` entry.
///
/// Jira nests a whole (abbreviated) issue there, so mockd resolves the
/// fixture's `epic` / `blocked_by` keys into one of these when the state is
/// built. Resolved once rather than on every request because nothing mockd
/// serves can change what it holds: `touch_issue` moves `updated` and
/// `add_comment` appends a comment, and neither is a field of this shape.
#[derive(Debug, Clone)]
pub struct JiraIssueRef {
    pub id: u64,
    pub key: String,
    pub summary: String,
    pub issue_type: String,
    pub status: String,
    pub priority: Option<String>,
}

/// One `issuelinks` entry, from this issue's side of it.
#[derive(Debug, Clone)]
pub struct JiraLink {
    /// Shared by both ends: in Jira one link is one object seen from two
    /// sides, not two links.
    pub id: u64,
    /// `true` when this issue is the blocked one, so the other end is served
    /// as `inwardIssue` ("is blocked by"); `false` when it is the blocker, so
    /// the other end is `outwardIssue` ("blocks").
    pub inward: bool,
    pub other: JiraIssueRef,
}

#[derive(Debug, Clone)]
pub struct JiraComment {
    pub id: u64,
    pub author: String,
    pub body: String,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct JiraWorklog {
    pub id: u64,
    pub author: String,
    pub comment: String,
    pub started: DateTime<Utc>,
    pub time_spent_seconds: u64,
}

/// The failure a mock server should exhibit instead of answering normally.
///
/// Serde-tagged so `POST /__mock/fault` can carry one as
/// `{"kind":"rate_limited","retry_after_secs":5}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MockFault {
    #[default]
    None,
    Unauthorized,
    Timeout {
        hang_ms: u64,
    },
    ServerError,
    RateLimited {
        retry_after_secs: u32,
    },
}

#[derive(Debug)]
struct Inner {
    issues: Vec<JiraIssue>,
    next_comment_id: u64,
    /// Advances one minute per mutation; see the module docs.
    clock: DateTime<Utc>,
    max_results_cap: u32,
    build_types: Vec<TcBuildType>,
    /// Ascending by id, and kept that way by `queue_build`.
    builds: Vec<TcBuild>,
}

/// The mutable fixture one mock server serves.
#[derive(Debug)]
pub struct MockState {
    inner: RwLock<Inner>,
    violations: ViolationLog,
    fault: Mutex<MockFault>,
    /// One entry per served API (`"jira"`, `"teamcity"`).
    ///
    /// Per-API and not a single string because [`spawn_all`](crate::spawn_all)
    /// mounts both routers over **one** state on **two** ports: a shared field
    /// would make whichever server bound last own every `self` link in the
    /// other one's bodies.
    base_urls: RwLock<std::collections::HashMap<String, String>>,
    server_offset: RwLock<FixedOffset>,
}

impl MockState {
    /// A fresh state holding the fixture, unmutated.
    pub fn from_fixture() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(Inner::fresh()),
            violations: ViolationLog::default(),
            fault: Mutex::new(MockFault::None),
            base_urls: RwLock::new(std::collections::HashMap::new()),
            server_offset: RwLock::new(default_server_offset()),
        })
    }

    /// Throws away every mutation, restoring the fixture exactly.
    ///
    /// The violation log, the fault and the base URL are deliberately left
    /// alone: they belong to the server, not to the data.
    pub fn reset(&self) {
        *self.write() = Inner::fresh();
    }

    /// Called once `api`'s listener has bound, so its `self` links can name
    /// the port it actually got.
    pub fn set_base_url(&self, api: &str, url: &str) {
        self.base_urls
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(api.to_owned(), url.to_owned());
    }

    /// `api`'s public base URL, or the empty string before it has bound.
    pub fn base_url(&self, api: &str) -> String {
        self.base_urls
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(api)
            .cloned()
            .unwrap_or_default()
    }

    pub fn violations(&self) -> &ViolationLog {
        &self.violations
    }

    pub fn fault(&self) -> MockFault {
        self.fault.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_fault(&self, f: MockFault) {
        *self.fault.lock().unwrap_or_else(|e| e.into_inner()) = f;
    }

    /// Bumps `key`'s `updated` to the next tick of the clock, leaving every
    /// other issue alone.
    ///
    /// A **complete** no-op for a key the fixture does not have: the clock does
    /// not move either.
    pub fn touch_issue(&self, key: &str) {
        let mut inner = self.write();
        // Resolve the issue *before* ticking: the clock is what
        // `serverInfo.serverTime` reports and what every incremental query
        // orders by, so moving it for a mutation that did not happen would
        // make a failed write observable as a phantom sync tick.
        let Some(idx) = inner.issues.iter().position(|i| i.key == key) else {
            return;
        };
        let now = inner.tick();
        inner.issues[idx].updated = now;
    }

    /// Appends a comment and bumps the issue's `updated`; returns the new
    /// comment's id, or `None` if there is no such issue.
    pub fn add_comment(&self, key: &str, author: &str, body: &str) -> Option<u64> {
        let mut inner = self.write();
        // Same ordering rule as `touch_issue`: no issue, no tick, and no id
        // consumed either.
        let idx = inner.issues.iter().position(|i| i.key == key)?;
        let now = inner.tick();
        let id = inner.next_comment_id;
        let issue = &mut inner.issues[idx];
        issue.comments.push(JiraComment {
            id,
            author: author.to_owned(),
            body: body.to_owned(),
            created: now,
            updated: now,
        });
        issue.updated = now;
        inner.next_comment_id += 1;
        Some(id)
    }

    /// The workflow mockd pretends to have, as `(transition id, name, target
    /// status)` reachable from `from`.
    ///
    /// A real Jira's workflow is per-project and per-issue-type configuration,
    /// which is exactly why interfaces §5 says the available set is *fetched*
    /// and never hard-coded. What matters for a test double is that it is a
    /// **workflow rather than a list of statuses**: from `In Progress` you can
    /// reach `In Review` and go back to `To Do`, and you cannot jump straight
    /// to `Done`. An adapter that assumed "any status is reachable" passes
    /// against a mock that offers everything and fails against a real Jira.
    #[must_use]
    pub fn jira_transitions(from: &str) -> &'static [(&'static str, &'static str, &'static str)] {
        match from {
            "To Do" => &[("11", "Start Progress", "In Progress")],
            "In Progress" => &[
                ("21", "Send to Review", "In Review"),
                ("41", "Stop Progress", "To Do"),
            ],
            "In Review" => &[
                ("31", "Done", "Done"),
                ("41", "Back to In Progress", "In Progress"),
            ],
            "Done" => &[("51", "Reopen", "To Do")],
            _ => &[],
        }
    }

    /// Move an issue by transition id, returning the status it landed on.
    ///
    /// `None` if there is no such issue; `Some(Err(..))` if the transition is
    /// not one the issue's current status offers -- which is the 400 a real
    /// Jira answers, and the failure story 2 exists to keep out of the UI.
    ///
    /// Same ordering rule as [`Self::add_comment`]: the clock does not tick
    /// for a move that did not happen.
    pub fn transition_issue(&self, key: &str, transition_id: &str) -> Option<Result<String, ()>> {
        let mut inner = self.write();
        let idx = inner.issues.iter().position(|i| i.key == key)?;
        let Some((_, _, to)) = Self::jira_transitions(&inner.issues[idx].status)
            .iter()
            .find(|(id, _, _)| *id == transition_id)
        else {
            return Some(Err(()));
        };
        let to = (*to).to_owned();
        let now = inner.tick();
        inner.issues[idx].status.clone_from(&to);
        inner.issues[idx].updated = now;
        Some(Ok(to))
    }

    /// Append a new issue to `project` and return it.
    ///
    /// `None` if `project` is not one the fixture has -- a real Jira answers
    /// a create into an unknown project with a 400 naming the project, and a
    /// mock that invented the project instead would let an adapter ship a
    /// typo.
    pub fn create_issue(
        &self,
        project: &str,
        summary: &str,
        description: Option<&str>,
        issue_type: &str,
        reporter: &str,
    ) -> Option<JiraIssue> {
        let mut inner = self.write();
        if !inner.issues.iter().any(|i| i.project == project) {
            return None;
        }
        let now = inner.tick();
        let id = inner.issues.iter().map(|i| i.id).max().unwrap_or(10_000) + 1;
        // Jira numbers issue keys per project, from the highest that project
        // has ever had.
        let next = inner
            .issues
            .iter()
            .filter(|i| i.project == project)
            .filter_map(|i| {
                i.key
                    .rsplit_once('-')
                    .and_then(|(_, n)| n.parse::<u64>().ok())
            })
            .max()
            .unwrap_or(0)
            + 1;
        let issue = JiraIssue {
            id,
            key: format!("{project}-{next}"),
            project: project.to_owned(),
            summary: summary.to_owned(),
            description: description.map(str::to_owned),
            issue_type: issue_type.to_owned(),
            // A new issue starts at the workflow's first status.
            status: "To Do".to_owned(),
            priority: None,
            assignee: None,
            reporter: reporter.to_owned(),
            created: now,
            updated: now,
            comments: Vec::new(),
            worklogs: Vec::new(),
            parent: None,
            links: Vec::new(),
            original_estimate_secs: None,
        };
        inner.issues.push(issue.clone());
        Some(issue)
    }

    pub fn issue(&self, key: &str) -> Option<JiraIssue> {
        self.read().issues.iter().find(|i| i.key == key).cloned()
    }

    /// Every issue, in fixture order.
    pub fn issues(&self) -> Vec<JiraIssue> {
        self.read().issues.clone()
    }

    /// The mock's own clock: `fixture().today` plus one minute per mutation.
    ///
    /// This is what `serverInfo.serverTime` reports, so the server's idea of
    /// "now" is always at least as new as the newest `updated` it will serve —
    /// and never the wall clock, which would make responses irreproducible.
    pub fn now(&self) -> DateTime<Utc> {
        self.read().clock
    }

    // -- the TeamCity half --------------------------------------------------

    /// Every build configuration, ascending by id.
    pub fn build_types(&self) -> Vec<TcBuildType> {
        self.read().build_types.clone()
    }

    /// Every build, ascending by id.
    pub fn builds(&self) -> Vec<TcBuild> {
        self.read().builds.clone()
    }

    pub fn build(&self, id: u64) -> Option<TcBuild> {
        self.read().builds.iter().find(|b| b.id == id).cloned()
    }

    /// Moves a `running`/`queued` build to `finished` with `status`, setting
    /// `finish_date` to the ticked clock.
    ///
    /// # Panics
    ///
    /// If there is no such build. This is a test-driver API: a silent no-op
    /// would make a caller's test pass for the wrong reason.
    pub fn finish_build(&self, id: u64, status: TcStatus) {
        finish(&mut self.write(), "finish_build", id, status);
    }

    /// Cancels a build: `finished`, [`TcStatus::Unknown`], `statusText:
    /// "Canceled"`, and marked so the default filter hides it.
    ///
    /// The class no fixture build has, made expressible -- the counterpart of
    /// `describe_build_type`. Without it nothing can serve the one shape issue
    /// #105 is about: a build the mirror already holds as running, which the
    /// server then stops.
    ///
    /// # Panics
    ///
    /// If there is no such build. This is a test-driver API, and a silent
    /// no-op would make a caller's test pass for the wrong reason.
    /// The finish and the flag are set under **one** lock, deliberately.
    /// mockd serves requests concurrently, so two locks would leave a window
    /// in which a `state:finished` page carried this build as an *ordinary*
    /// one -- `UNKNOWN`, `statusText: "Canceled"`, `canceled: false` -- which
    /// is precisely the fidelity gap issue #105 closed.
    pub fn cancel_build(&self, id: u64) {
        let mut inner = self.write();
        let b = finish(&mut inner, "cancel_build", id, TcStatus::Unknown);
        b.canceled = true;
    }

    /// Terminates a build as **failed to start**: `finished`, `FAILURE`, its
    /// own `statusText`, and `failedToStart: true`, which the default filter
    /// hides on.
    ///
    /// The other class the default filter removes, and the one a *queued*
    /// build reaches without ever running.
    ///
    /// The flag stays server-side: mockd does not serve a `failedToStart` key,
    /// because nothing in knobas reads one and `BUILD_FIELDS` asking for a name
    /// no reader looks at is its own defect. The class is observable exactly
    /// where it matters -- through the `failedToStart:` locator dimension and
    /// the default filter -- which is how `personal` would work too if the
    /// fixture had any.
    ///
    /// # Panics
    ///
    /// If there is no such build.
    /// One lock, for the reason [`Self::cancel_build`] gives.
    pub fn fail_build_to_start(&self, id: u64) {
        let mut inner = self.write();
        let b = finish(&mut inner, "fail_build_to_start", id, TcStatus::Failure);
        b.failed_to_start = true;
        b.status_text = "Failed to start: no agent could run this build".to_owned();
    }

    /// Appends a `queued` build with `id = max(existing ids) + 1` and returns
    /// it. Ids stay monotonic, which is what makes `sinceBuild` meaningful.
    ///
    /// # Panics
    ///
    /// If `build_type_id` is not one of the fixture's build configurations.
    pub fn queue_build(&self, build_type_id: &str, branch: &str) -> u64 {
        let mut inner = self.write();
        if !inner.build_types.iter().any(|t| t.id == build_type_id) {
            panic!("queue_build: no build type {build_type_id:?} in the fixture");
        }
        let id = inner.builds.iter().map(|b| b.id).max().unwrap_or(0) + 1;
        let now = inner.tick();
        inner.builds.push(TcBuild {
            id,
            build_type_id: build_type_id.to_owned(),
            number: id.to_string(),
            status: TcStatus::Success,
            state: TcState::Queued,
            branch_name: branch.to_owned(),
            start_date: now,
            finish_date: None,
            status_text: "Queued".to_owned(),
            percentage_complete: None,
            current_stage_text: None,
            // A build the test harness queued is nobody's: the mutator is
            // given a configuration and a branch, which is a VCS trigger.
            triggered_by: None,
            canceled: false,
            failed_to_start: false,
            default_branch: crate::tc_state::is_default_branch(branch),
        });
        id
    }

    /// Gives a build configuration a `description`.
    ///
    /// No fixture configuration has one, because the dataset describes none.
    /// That makes the whole `description` wire path -- selector asks, mockd
    /// serves, the adapter maps it into the search blob -- untestable from the
    /// fixture alone: with the value `null` everywhere, a selector that asks
    /// for it and one that does not produce identical output. This is the
    /// counterpart of `finish_build`/`queue_build`: the one thing a frozen
    /// fixture cannot express, made expressible, so the test can fail.
    ///
    /// # Panics
    ///
    /// If no configuration has `id` -- a test naming one that is not there is
    /// asserting against nothing.
    pub fn describe_build_type(&self, id: &str, description: &str) {
        let mut inner = self.write();
        let Some(t) = inner.build_types.iter_mut().find(|t| t.id == id) else {
            panic!("describe_build_type: no build type {id:?} in the fixture");
        };
        t.description = Some(description.to_owned());
    }

    pub fn set_max_results_cap(&self, cap: u32) {
        self.write().max_results_cap = cap;
    }

    pub fn max_results_cap(&self) -> u32 {
        self.read().max_results_cap
    }

    /// The zone this server reports its timestamps in and interprets JQL date
    /// literals in. Default `+02:00`, deliberately not UTC.
    pub fn server_offset(&self) -> FixedOffset {
        *self.server_offset.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_server_offset(&self, off: FixedOffset) {
        *self
            .server_offset
            .write()
            .unwrap_or_else(|e| e.into_inner()) = off;
    }

    /// The same poisoning rationale as [`ViolationLog`]: a test that already
    /// panicked should fail on its own assertion, not on a second one.
    fn read(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        self.inner.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(|e| e.into_inner())
    }
}

/// Moves a build to `finished` with `status` under a lock the caller already
/// holds, and hands the build back so a caller with a *facet* to set as well
/// -- [`MockState::cancel_build`], [`MockState::fail_build_to_start`] -- can
/// set it before anything else reads the build.
///
/// The lock is the caller's for that reason: mockd serves requests
/// concurrently, and a finish that released before the flag was set would
/// leave a window in which a `state:finished` page carried a canceled build
/// as an ordinary one.
///
/// # Panics
///
/// If there is no build `id`. This is a test-driver API, and a silent no-op
/// would make a caller's test pass for the wrong reason.
fn finish<'a>(
    inner: &'a mut Inner,
    caller: &str,
    id: u64,
    status: TcStatus,
) -> &'a mut crate::tc_state::TcBuild {
    // Resolve before ticking, exactly as `touch_issue` does: a mutation
    // that did not happen must not move the clock.
    let Some(idx) = inner.builds.iter().position(|b| b.id == id) else {
        panic!("{caller}: no build {id} in the fixture");
    };
    let now = inner.tick();
    let b = &mut inner.builds[idx];
    b.state = TcState::Finished;
    b.status = status;
    b.finish_date = Some(now);
    b.status_text = match status {
        TcStatus::Success => "Success".to_owned(),
        TcStatus::Failure => "Failure".to_owned(),
        // Reachable only through `cancel_build`, which sets `canceled` in the
        // same critical section; a caller that finished a build UNKNOWN
        // without it would have served a build the default filter treats as
        // ordinary.
        TcStatus::Unknown => "Canceled".to_owned(),
    };
    b.percentage_complete = None;
    b.current_stage_text = None;
    b
}

fn default_server_offset() -> FixedOffset {
    FixedOffset::east_opt(DEFAULT_SERVER_OFFSET_SECS).expect("+02:00 is a valid offset")
}

impl Inner {
    fn fresh() -> Self {
        let (issues, next_comment_id) = build_issues();
        Self {
            issues,
            next_comment_id,
            clock: fixture().today,
            max_results_cap: DEFAULT_MAX_RESULTS_CAP,
            build_types: crate::tc_state::build_types(),
            builds: crate::tc_state::builds(),
        }
    }

    /// One minute per mutation — see the module docs on why not less.
    fn tick(&mut self) -> DateTime<Utc> {
        self.clock += Duration::minutes(1);
        self.clock
    }
}

/// The fixture in Jira shapes, plus the next free comment id.
///
/// `reset()` and `from_fixture()` both go through here, so the two cannot
/// drift apart.
fn build_issues() -> (Vec<JiraIssue>, u64) {
    let fx = fixture();
    let username = |id: &str| {
        fx.person(id)
            .unwrap_or_else(|| panic!("fixture person {id:?}"))
            .username
            .clone()
    };
    // PAY-200 is the fixture's only null `updated`; Jira always has one.
    let synthesized_updated = fx.today - Duration::days(30);
    let mut next_comment_id = 20_000;
    let mut next_worklog_id = 30_000;
    let mut issues = Vec::with_capacity(fx.tickets.len());

    for (idx, t) in fx.tickets.iter().enumerate() {
        let updated = t.updated.unwrap_or(synthesized_updated);
        let comments = t
            .comments
            .iter()
            .map(|c| {
                let id = next_comment_id;
                next_comment_id += 1;
                JiraComment {
                    id,
                    author: username(&c.who),
                    body: c.text.clone(),
                    created: c.when,
                    updated: c.when,
                }
            })
            .collect();
        let worklogs = t
            .worklogs
            .iter()
            .map(|w| {
                let id = next_worklog_id;
                next_worklog_id += 1;
                let at_nine = w
                    .date
                    .and_time(NaiveTime::from_hms_opt(9, 0, 0).expect("09:00 is a valid time"));
                JiraWorklog {
                    id,
                    author: username(&w.who),
                    comment: w.text.clone(),
                    started: Utc.from_utc_datetime(&at_nine),
                    time_spent_seconds: u64::from(w.minutes) * 60,
                }
            })
            .collect();
        issues.push(JiraIssue {
            id: 10_000 + idx as u64,
            key: t.key.clone(),
            project: t
                .key
                .split_once('-')
                .map(|(p, _)| p.to_owned())
                .unwrap_or_else(|| t.key.clone()),
            summary: t.summary.clone(),
            description: t.description.clone(),
            issue_type: t.kind.clone(),
            status: t.status.clone(),
            priority: t.priority.clone(),
            assignee: t.assignee.as_deref().map(&username),
            reporter: username(
                t.assigned_by
                    .as_deref()
                    .or(t.assignee.as_deref())
                    .unwrap_or("priya"),
            ),
            created: updated - Duration::days(14),
            updated,
            comments,
            worklogs,
            // Both need every issue to exist first; filled in below.
            parent: None,
            links: Vec::new(),
            original_estimate_secs: t.estimate_h.map(|h| u64::from(h) * 3600),
        });
    }
    resolve_references(&mut issues, fx);
    (issues, next_comment_id)
}

/// The second pass: `parent` and `issuelinks`, which point at *other* issues
/// and therefore cannot be built in the first.
///
/// A fixture key that names no ticket is a broken fixture, not a missing
/// reference, so it panics rather than being skipped -- a silently dropped
/// epic would show up as an adapter that stopped reading epic membership.
fn resolve_references(issues: &mut [JiraIssue], fx: &knobas_source_mock::Fixture) {
    let reference = |key: &str| -> JiraIssueRef {
        let i = issues
            .iter()
            .find(|i| i.key == key)
            .unwrap_or_else(|| panic!("fixture ticket {key:?} is referenced but does not exist"));
        JiraIssueRef {
            id: i.id,
            key: i.key.clone(),
            summary: i.summary.clone(),
            issue_type: i.issue_type.clone(),
            status: i.status.clone(),
            priority: i.priority.clone(),
        }
    };

    let parents: Vec<Option<JiraIssueRef>> = fx
        .tickets
        .iter()
        .map(|t| t.epic.as_deref().map(&reference))
        .collect();

    // One link object, two ends. The blocked issue gets the inward end and the
    // blocker the outward one, both under the same id.
    let mut next_link_id = 40_000;
    let mut links: Vec<Vec<JiraLink>> = vec![Vec::new(); fx.tickets.len()];
    for (blocked_idx, t) in fx.tickets.iter().enumerate() {
        for blocker_key in &t.blocked_by {
            let id = next_link_id;
            next_link_id += 1;
            // `reference` already panics if the key names no ticket, and says
            // so; this only needs the *position*, to hang the other end on.
            let blocker = reference(blocker_key);
            let blocker_idx = fx
                .tickets
                .iter()
                .position(|o| &o.key == blocker_key)
                .expect("`reference` accepted the key, so a ticket has it");
            links[blocked_idx].push(JiraLink {
                id,
                inward: true,
                other: blocker,
            });
            links[blocker_idx].push(JiraLink {
                id,
                inward: false,
                other: reference(&t.key),
            });
        }
    }

    for (issue, (parent, link)) in issues.iter_mut().zip(parents.into_iter().zip(links)) {
        issue.parent = parent;
        issue.links = link;
    }
}
