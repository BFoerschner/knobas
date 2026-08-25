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
//! | clock | starts at `fixture().today` and advances **exactly one minute** per `touch_issue`/`add_comment` — JQL time resolution is one minute, so a smaller step would make two touches indistinguishable to the very query an adapter runs. |
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
    base_url: RwLock<String>,
    server_offset: RwLock<FixedOffset>,
}

impl MockState {
    /// A fresh state holding the fixture, unmutated.
    pub fn from_fixture() -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(Inner::fresh()),
            violations: ViolationLog::default(),
            fault: Mutex::new(MockFault::None),
            base_url: RwLock::new(String::new()),
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

    /// Called once the listener has bound, so `self` links can name the port.
    pub fn set_base_url(&self, url: &str) {
        *self.base_url.write().unwrap_or_else(|e| e.into_inner()) = url.to_owned();
    }

    pub fn base_url(&self) -> String {
        self.base_url
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
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
        let mut inner = self.write();
        // Resolve before ticking, exactly as `touch_issue` does: a mutation
        // that did not happen must not move the clock.
        let Some(idx) = inner.builds.iter().position(|b| b.id == id) else {
            panic!("finish_build: no build {id} in the fixture");
        };
        let now = inner.tick();
        let b = &mut inner.builds[idx];
        b.state = TcState::Finished;
        b.status = status;
        b.finish_date = Some(now);
        b.status_text = match status {
            TcStatus::Success => "Success".to_owned(),
            TcStatus::Failure => "Failure".to_owned(),
        };
        b.percentage_complete = None;
        b.current_stage_text = None;
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
        });
        id
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
        });
    }
    (issues, next_comment_id)
}
