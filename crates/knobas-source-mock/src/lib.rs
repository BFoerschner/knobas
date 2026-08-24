//! The mock adapter: knobas' reference [`Source`], serving the Tidewater
//! Freight dataset.
//!
//! Two jobs, both load-bearing for the milestones that follow:
//!
//! * **It is the app's backend until there is a real one.** Every one of the 25
//!   mockups was drawn against `mockups/shared/dataset.md` -- the same tickets,
//!   people, timestamps and prose. [`fixture()`] parses that dataset from
//!   `fixtures/tidewater/work.json`, so a screen built to a mockup can be run,
//!   demoed and tested end to end without Jira, Gitea, TeamCity or Confluence
//!   existing yet.
//! * **It is the SPI's worked example.** It passes the same
//!   [`contract::battery`](knobas_source::contract::battery) every real adapter
//!   must pass, so a new adapter has something correct to copy: how kinds are
//!   declared, how faults are classified, how undeclared writes are refused,
//!   how a sink failure aborts a sync.
//!
//! It talks to nothing. Faults are simulated with
//! [`MockSource::with_fault`], and writes are recorded in memory rather than
//! performed -- [`MockSource::written_ops`] is how a test asserts that the app
//! issued the write it claims to have issued.
//!
//! ## Fixture conventions
//!
//! The dataset is prose, so transcribing it fixes two things it leaves open:
//!
//! * Relative times ("today 11:48", "yesterday 16:05") resolve against the
//!   dataset's fictional now, **2026-08-22T14:32Z**, recorded as
//!   [`Fixture::today`].
//! * Values the dataset gives only as a date ("2026-08-18") become midnight
//!   UTC. Values it does not give at all are `null`; nothing is invented to
//!   fill a hole.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, NaiveDate, Utc};
use knobas_core::entity::EntityRef;
use knobas_source::contract::Fault;
use knobas_source::{
    Capability, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError, SyncItem, WriteOp,
};
use serde::{Deserialize, Serialize};

/// The dataset, compiled in: the mock is used from tests and from the packaged
/// app, neither of which can rely on a file being next to the binary.
const FIXTURE_JSON: &str = include_str!("../../../fixtures/tidewater/work.json");

/// The only cursor the mock ever hands out. The fixture is a frozen snapshot,
/// so the position within it is a version, not an offset -- bump the suffix and
/// every stored cursor stops matching, which is exactly the "re-sync from
/// scratch" the new data would need.
const CURSOR: &str = "tidewater-v1";

// -- the fixture ------------------------------------------------------------

/// The Tidewater Freight dataset: `mockups/shared/dataset.md`, machine-readable.
///
/// Every field is transcribed; nothing is generated. The structs
/// `deny_unknown_fields` so that a key renamed in the JSON fails the build
/// instead of silently reading back as `None`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    /// The dataset's fictional now, 2026-08-22T14:32Z, against which every
    /// relative time in the brief was resolved.
    pub today: DateTime<Utc>,
    pub people: Vec<Person>,
    pub tickets: Vec<Ticket>,
    pub prs: Vec<PullRequest>,
    pub builds: Vec<Build>,
    pub pages: Vec<Page>,
    pub notes: Vec<Note>,
    pub commits: Vec<Commit>,
    pub branches: Vec<Branch>,
    pub repos: Vec<Repo>,
}

impl Fixture {
    /// The ticket with `key`, if the dataset has one.
    pub fn ticket(&self, key: &str) -> Option<&Ticket> {
        self.tickets.iter().find(|t| t.key == key)
    }

    /// The person with `id` (`"mara"`, `"priya"`, …), if the dataset has one.
    pub fn person(&self, id: &str) -> Option<&Person> {
        self.people.iter().find(|p| p.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    /// Short handle used as the `assignee`/`who`/`by` foreign key everywhere
    /// else in the fixture.
    pub id: String,
    pub name: String,
    pub initials: String,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ticket {
    pub key: String,
    /// Key of the epic this belongs to; `None` for the epic itself and OPS-77.
    pub epic: Option<String>,
    pub summary: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    pub priority: Option<String>,
    /// [`Person::id`] of the assignee.
    pub assignee: Option<String>,
    /// [`Person::id`] of whoever assigned it, where the dataset says.
    #[serde(default)]
    pub assigned_by: Option<String>,
    pub updated: Option<DateTime<Utc>>,
    pub description: Option<String>,
    pub estimate_h: Option<u32>,
    /// Time spent this week, in minutes.
    pub spent_week_m: Option<u32>,
    /// Keys of tickets blocking this one.
    #[serde(default)]
    pub blocked_by: Vec<String>,
    pub comments: Vec<Comment>,
    pub worklogs: Vec<Worklog>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comment {
    /// [`Person::id`] of the author.
    pub who: String,
    pub when: DateTime<Utc>,
    pub text: String,
}

/// Time already logged against a ticket. The dataset gives worklogs a day, not
/// a clock time, so this carries a date.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Worklog {
    /// [`Person::id`] of whoever logged it.
    pub who: String,
    pub date: NaiveDate,
    pub minutes: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PullRequest {
    pub num: u32,
    pub title: String,
    pub repo: String,
    /// Source branch, where the dataset names it.
    pub from: Option<String>,
    /// Target branch, where the dataset names it.
    pub to: Option<String>,
    /// `open` | `merged`.
    pub state: String,
    /// [`Person::id`] of the author.
    pub by: String,
    pub opened: Option<DateTime<Utc>>,
    #[serde(default)]
    pub merged: Option<DateTime<Utc>>,
    /// Given/required, e.g. `"1/2"`.
    pub approvals: Option<String>,
    #[serde(default)]
    pub approved_by: Vec<String>,
    #[serde(default)]
    pub review_requested_from: Vec<ReviewRequest>,
    /// [`Build::num`]s attached to this PR.
    pub checks: Vec<u32>,
    pub ticket: Option<String>,
    pub comments: Vec<Comment>,
    pub files: Vec<FileChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRequest {
    /// [`Person::id`] the review was requested from.
    pub who: String,
    pub when: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileChange {
    pub path: String,
    pub added: u32,
    pub removed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    pub num: u32,
    /// Build configuration, e.g. `"Payout_IntegrationTests"`.
    pub cfg: String,
    /// `running` | `failed` | `success`.
    pub status: String,
    pub branch: String,
    pub when: DateTime<Utc>,
    pub ticket: Option<String>,
    /// Progress of a running build, e.g. ``"step 3/5 `cargo test`"``.
    pub step: Option<String>,
    pub duration: Option<String>,
    /// Log excerpt, for the builds the dataset gives one.
    pub log: Option<String>,
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: String,
    pub title: String,
    pub space: String,
    pub edited: DateTime<Utc>,
    /// The section of the last edit, where the dataset names it.
    pub section: Option<String>,
    /// [`Person::id`] of the last editor.
    pub by: String,
    pub body: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    pub comments: Vec<Comment>,
}

/// A knobas-local note -- not synced from anywhere, which is why the mock does
/// not emit these as [`SyncItem`]s.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body_md: String,
    pub edited: DateTime<Utc>,
    /// `[[…]]` targets written in the note body.
    pub links: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub sha: String,
    pub when: DateTime<Utc>,
    pub msg: String,
    pub branch: String,
    pub repo: String,
    /// [`Person::id`] of the author.
    pub by: String,
    pub ticket: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Branch {
    pub name: String,
    pub repo: String,
    pub ticket: Option<String>,
    /// `default` | `open` | `merged`.
    pub state: String,
    /// Commits ahead of the default branch, where the dataset says.
    pub ahead: Option<u32>,
    pub checked_out: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repo {
    pub name: String,
    pub lang: String,
    /// Path of the local clone, or `None` if it is not cloned.
    pub clone: Option<String>,
    pub default_branch: Option<String>,
    pub commit_count: Option<u32>,
}

/// The parsed dataset, shared by every caller.
///
/// Parsed once and leaked into a [`OnceLock`] because the fixture is immutable
/// and every test in the workspace wants the same copy of it.
///
/// # Panics
///
/// If `fixtures/tidewater/work.json` does not parse. The file is compiled in,
/// so this is a build-time mistake that surfaces on first use, not a runtime
/// condition a caller could handle.
pub fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        serde_json::from_str(FIXTURE_JSON)
            .expect("fixtures/tidewater/work.json must parse as a Fixture")
    })
}

// -- the adapter ------------------------------------------------------------

/// A [`Source`] serving [`fixture()`], optionally pretending to be broken.
#[derive(Debug)]
pub struct MockSource {
    fault: Fault,
    written: Mutex<Vec<WriteOp>>,
}

impl Default for MockSource {
    fn default() -> Self {
        Self::new()
    }
}

impl MockSource {
    /// A healthy mock.
    pub fn new() -> Self {
        Self::with_fault(Fault::None)
    }

    /// A mock that reports `fault` from every operation that would touch the
    /// remote system -- [`Source::test_connection`], [`Source::sync`] and
    /// [`Source::write`] alike.
    ///
    /// [`Fault::None`] is a healthy one, same as [`MockSource::new`].
    pub fn with_fault(fault: Fault) -> Self {
        Self {
            fault,
            written: Mutex::new(Vec::new()),
        }
    }

    /// The writes this instance was asked to perform, oldest first.
    ///
    /// The mock has no remote system to check afterwards, so this is how a test
    /// asserts that the app issued the write it claims to have issued.
    pub fn written_ops(&self) -> Vec<WriteOp> {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<WriteOp>> {
        // A panic while holding the lock leaves recorded writes intact -- they
        // are a plain Vec with no invariant to violate -- so poisoning is not
        // worth turning an assertion failure into a second, less informative
        // panic in the test that inspects them.
        self.written.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The error this instance should report instead of doing the work.
    fn fault_error(&self) -> Option<SourceError> {
        match self.fault {
            Fault::None => None,
            Fault::Unauthorized => Some(SourceError::Unauthorized),
            Fault::Unreachable => Some(SourceError::Unreachable(
                "simulated: connection refused".into(),
            )),
        }
    }
}

/// The identifier a descriptor declares for `op`.
///
/// No wildcard arm, for the same reason [`knobas_source::contract`] has none:
/// when `WriteOp` grows, the compiler is the reminder that this mock has to
/// decide whether it supports the new op.
fn write_op_identifier(op: &WriteOp) -> &'static str {
    match op {
        WriteOp::Comment { .. } => "comment",
    }
}

/// Join the non-empty parts into the blob FTS indexes.
fn body_text(parts: impl IntoIterator<Item = String>) -> String {
    parts
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// One [`SyncItem`], with `payload` carrying the fixture record verbatim so a
/// later milestone can re-map it without re-reading the fixture.
fn item(
    kind: &str,
    key: String,
    title: String,
    body: String,
    author: Option<String>,
    updated_at: Option<DateTime<Utc>>,
    payload: &impl Serialize,
) -> SyncItem {
    SyncItem {
        entity: EntityRef::new(SOURCE_ID, &key),
        kind: kind.to_owned(),
        title,
        body_text: body,
        author,
        updated_at,
        payload: serde_json::to_value(payload).expect("a fixture record must serialize"),
        deleted: false,
    }
}

/// Descriptor id, adapter kind, and therefore the [`EntityRef`] namespace of
/// every item this adapter emits.
const SOURCE_ID: &str = "mock";

/// Every work item in the fixture, in the order the mock emits them.
fn items() -> Vec<SyncItem> {
    let f = fixture();
    let mut out = Vec::new();
    for t in &f.tickets {
        out.push(item(
            "ticket",
            t.key.clone(),
            t.summary.clone(),
            body_text(
                [t.summary.clone()]
                    .into_iter()
                    .chain(t.description.clone())
                    .chain(t.comments.iter().map(|c| c.text.clone())),
            ),
            t.assignee.clone(),
            t.updated,
            t,
        ));
    }
    for p in &f.prs {
        out.push(item(
            "pr",
            format!("{}#{}", p.repo, p.num),
            p.title.clone(),
            body_text(
                [p.title.clone()]
                    .into_iter()
                    .chain(p.comments.iter().map(|c| c.text.clone())),
            ),
            Some(p.by.clone()),
            p.merged.or(p.opened),
            p,
        ));
    }
    for b in &f.builds {
        let title = format!("{} #{}", b.cfg, b.num);
        out.push(item(
            "build",
            format!("{}#{}", b.cfg, b.num),
            title.clone(),
            body_text(
                [title, b.status.clone(), b.branch.clone()]
                    .into_iter()
                    .chain(b.log.clone()),
            ),
            None,
            Some(b.when),
            b,
        ));
    }
    for p in &f.pages {
        out.push(item(
            "page",
            p.id.clone(),
            p.title.clone(),
            body_text(
                [p.title.clone()]
                    .into_iter()
                    .chain(p.body.clone())
                    .chain(p.comments.iter().map(|c| c.text.clone())),
            ),
            Some(p.by.clone()),
            Some(p.edited),
            p,
        ));
    }
    for c in &f.commits {
        out.push(item(
            "commit",
            c.sha.clone(),
            c.msg.clone(),
            body_text([c.msg.clone(), c.branch.clone()]),
            Some(c.by.clone()),
            Some(c.when),
            c,
        ));
    }
    out
}

#[async_trait::async_trait]
impl Source for MockSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: SOURCE_ID.to_owned(),
            adapter_kind: SOURCE_ID.to_owned(),
            name: "Tidewater (mock)".to_owned(),
            // `Write` and `write_ops` are two signals for one fact; the SPI
            // requires them to agree.
            capabilities: vec![Capability::Search, Capability::Write],
            adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
            // Nothing to authenticate against: the fixture is compiled in.
            auth_methods: Vec::new(),
            write_ops: vec!["comment".to_owned()],
            // The UI renders this source's launcher groups and chips from these
            // alone, so every kind `sync` emits is declared here.
            entity_kinds: vec![
                KindInfo {
                    id: "ticket".to_owned(),
                    label: "Ticket".to_owned(),
                    plural: "Tickets".to_owned(),
                    monogram: "TK".to_owned(),
                },
                KindInfo {
                    id: "pr".to_owned(),
                    label: "Pull request".to_owned(),
                    plural: "Pull requests".to_owned(),
                    monogram: "PR".to_owned(),
                },
                KindInfo {
                    id: "build".to_owned(),
                    label: "Build".to_owned(),
                    plural: "Builds".to_owned(),
                    monogram: "BU".to_owned(),
                },
                KindInfo {
                    id: "page".to_owned(),
                    label: "Page".to_owned(),
                    plural: "Pages".to_owned(),
                    monogram: "PG".to_owned(),
                },
                KindInfo {
                    id: "commit".to_owned(),
                    label: "Commit".to_owned(),
                    plural: "Commits".to_owned(),
                    monogram: "CM".to_owned(),
                },
            ],
            // Nothing to configure, so the Add-source form for the mock is empty.
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
        }
    }

    async fn test_connection(&self) -> Result<(), SourceError> {
        match self.fault_error() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        if let Some(err) = self.fault_error() {
            return Err(err);
        }
        // The fixture is frozen, so a caller already at the current version has
        // nothing to fetch. Any other cursor is from an older fixture and gets
        // a full sync -- which is the point of versioning the cursor.
        if cursor.as_deref() == Some(CURSOR) {
            return Ok(CURSOR.to_owned());
        }
        for it in items() {
            // Not the adapter's failure to swallow: a sink that rejected an
            // item wants the sync abandoned, not the remaining items pushed at
            // it and a fresh cursor handed back over the gap.
            sink.item(it).await?;
        }
        Ok(CURSOR.to_owned())
    }

    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
        let id = write_op_identifier(&op);
        // Refused locally, before the fault check: an op this source never
        // advertised is a caller bug, and stays one whether or not the remote
        // system happens to be reachable.
        if !self.descriptor().write_ops.iter().any(|w| w == id) {
            return Err(SourceError::Protocol(format!("unsupported write op: {id}")));
        }
        if let Some(err) = self.fault_error() {
            return Err(err);
        }
        self.lock().push(op);
        Ok(())
    }
}
