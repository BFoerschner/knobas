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
//! Deletions are simulated too, but only when asked for:
//! [`MockSource::with_tombstone`] emits one extra item marked `deleted`, so the
//! channel that tombstones an entity can be exercised -- and demoed -- from the
//! reference adapter instead of from a hand-rolled test source. The plain
//! [`MockSource::new`] never reports a deletion: the fixture is what the
//! mockups were drawn against, and a demo load must show exactly that.
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
//! * One hole is *made* rather than transcribed: [`UNPROJECTED_KEY`] carries no
//!   project, so the miss direction every project read has to pin (ADR-0010:
//!   absence, never a wrong room) is reachable from the demo profile.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use chrono::{DateTime, NaiveDate, Utc};
use knobas_core::entity::EntityRef;
use knobas_source::contract::Fault;
use knobas_source::instance::SourceInstance;
use knobas_source::{
    Capability, ConnectionInfo, Cursor, KindInfo, RefreshAnswer, Sink, Source, SourceDescriptor,
    SourceError, SyncItem, WriteOp, WriteReceipt,
};
use serde::{Deserialize, Serialize};

/// The dataset, compiled in: the mock is used from tests and from the packaged
/// app, neither of which can rely on a file being next to the binary.
const FIXTURE_JSON: &str = include_str!("../../../fixtures/tidewater/work.json");

/// The only cursor the mock ever hands out. The fixture is a snapshot, so the
/// position within it is a version, not an offset -- bump the suffix and every
/// stored cursor stops matching, which is exactly the "re-sync from scratch"
/// the new data would need.
///
/// **Widening what the fixture emits obliges a bump.** #234 is the worked
/// example: #230 gave the corpus its projects and left the suffix at `v1`, so
/// every profile created before it kept a matching cursor, never refetched,
/// and showed no project rooms with nothing in the app able to repair it.
/// `v3` is #537 paying the same debt: the corpus gained the fixture's repos
/// and branches, and a demo profile stored at `v2` has to be re-sent the
/// corpus to get them. `v4` is #1240: the corpus gained a pipeline per build
/// configuration, a daily protocol page and two monitors. `v5` is #2403: a
/// ticket gained its reporter, its created date, its time spent and its
/// worklog entries' seconds.
const CURSOR: &str = "tidewater-v5";

/// Where the fictional company's systems live. Nothing is served from here --
/// it exists so *Open in browser* has a shape to render and a stream building
/// the detail view can see the button (P5, `docs/contract.md` §8 -- the M1
/// interfaces document, re-homed there in `06ed97f`).
const MOCK_BASE: &str = "https://tidewater.example";

// -- the fixture ------------------------------------------------------------

/// The Tidewater Freight dataset: `mockups/shared/dataset.md`, machine-readable.
///
/// Every field is transcribed; nothing is generated. The structs
/// `deny_unknown_fields` so that a key renamed in the JSON fails the parse --
/// loudly, in [`fixture()`], on first use -- instead of silently reading back
/// as `None`. Not the build: `include_str!` embeds the bytes and nothing
/// reads them until then.
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
    /// The health checks of the two services the dataset gives a repository
    /// (#1240); see [`Monitor`].
    pub monitors: Vec<Monitor>,
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
    /// The [`Project`] this ticket belongs to, where the dataset places it in
    /// one -- and `None` for [`UNPROJECTED_KEY`], which it deliberately does
    /// not.
    ///
    /// Serialized by [`ticket_payload`] rather than by this struct: a project
    /// reaches the mirror where a *source* would have written it, which is not
    /// where the transcription happens to keep it. See there.
    #[serde(default, skip_serializing)]
    pub project: Option<Project>,
}

/// A source's own grouping of its items (`CONTEXT.md`, **Project**; ADR-0010),
/// in the source's own word: the dataset's `PAY` -- *Payments Platform* and
/// `OPS` -- *Operations*.
///
/// Key and name both, because they answer different questions: the key is what
/// a project is scoped by, the name is what a person reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub key: String,
    pub name: String,
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
    /// [`Person::id`] of whoever started it, where the dataset says. `None` is
    /// a build nothing in the dataset attributes to a person -- a VCS trigger
    /// -- and not a person the transcription lost.
    #[serde(default)]
    pub triggered_by: Option<String>,
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
    /// [`Page::id`] of the page above this one, where the dataset names one
    /// (#1240: *Standup protocols*, "parent of daily protocol pages").
    #[serde(default)]
    pub parent: Option<String>,
    pub comments: Vec<Comment>,
}

/// A health check on one of the dataset's services. The dataset has none: this
/// is the one record kind the fixture adds for a role (#1240, monitoring), one
/// per repository the dataset gives a runtime (`payout-service`, `ledger-api`;
/// `ops-runbooks` is Markdown and runs nothing), both healthy so the demo
/// invents no outage. The URLs are `{MOCK_BASE}`-style addresses nothing
/// serves.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Monitor {
    pub id: String,
    pub name: String,
    /// `http`, as Uptime Kuma names the check.
    #[serde(rename = "type")]
    pub kind: String,
    /// `up` | `down` | `pending` | `maintenance`.
    pub state: String,
    pub response_time_ms: f64,
    pub url: String,
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
    /// The organization team the repository belongs to (prototype-6's), which
    /// `testenv/seed-gitea.sh` creates and adds the repository to.
    pub team: String,
    /// Path of the local clone, or `None` if it is not cloned.
    pub clone: Option<String>,
    pub default_branch: Option<String>,
    pub commit_count: Option<u32>,
}

/// The parsed dataset, shared by every caller.
///
/// Parsed once into a [`OnceLock`]: nothing mutates a parsed fixture -- every
/// caller gets `&'static` -- and every test in the workspace wants the same
/// copy of it. (The *file* is versioned, not frozen; see [`CURSOR`].)
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

/// A [`Source`] serving [`fixture()`], optionally pretending to be broken or
/// to have lost an item upstream.
#[derive(Debug)]
pub struct MockSource {
    /// This instance's id, and therefore the namespace of every item it
    /// emits. `"mock"` unless [`build`] was given another one.
    id: String,
    fault: Fault,
    /// Whether a full sync also reports [`TOMBSTONED_KEY`] as deleted.
    tombstone: bool,
    /// Whether this instance reads one item by id ([`Source::refresh`],
    /// ADR-0031). On unless its config says `"refresh": false`, which is how
    /// a test gets a source that offers the run alone.
    refresh: bool,
    /// Whether this instance files tickets (`create_ticket`), which it then
    /// holds as its remote system would ([`filed`]). Off unless its config
    /// says `"files_tickets": true`: the demo files nothing.
    files_tickets: bool,
    written: Mutex<Vec<WriteOp>>,
}

/// The tickets each mock instance has filed, by instance id: the mock's
/// remote system, which outlives the instance that filed them as a real
/// server outlives an adapter. The app builds an adapter per flush, per run
/// and per refresh, so a ticket filed by one must be there for the next.
fn filed() -> std::sync::MutexGuard<'static, BTreeMap<String, Vec<Ticket>>> {
    static FILED: OnceLock<Mutex<BTreeMap<String, Vec<Ticket>>>> = OnceLock::new();
    FILED
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
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
            id: SOURCE_ID.to_owned(),
            fault,
            tombstone: false,
            refresh: true,
            files_tickets: false,
            written: Mutex::new(Vec::new()),
        }
    }

    /// A healthy mock whose **full sync also reports one item as deleted**:
    /// [`TOMBSTONED_KEY`], a ticket the dataset does not otherwise contain.
    ///
    /// Opt-in, because the fixture is the dataset the mockups were drawn
    /// against and a demo load has to match it. What this buys is the one part
    /// of the sync contract the fixture cannot express: `SyncItem::deleted`,
    /// which tombstones the entity while leaving its mirror row (and therefore
    /// its last-known title) in place. Without it, the deletion channel is
    /// reachable only from a test-local adapter or from a real one with a live
    /// server behind it (Gitea's `branch_tombstone` emits them) -- neither of
    /// which a demo load or a credential-free test has.
    ///
    /// Deterministic: the same key, title and body on every run and every
    /// sync, so re-syncing is idempotent and the tombstone can be asserted on
    /// by value.
    pub fn with_tombstone() -> Self {
        Self {
            tombstone: true,
            ..Self::new()
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

    /// Every live item this instance's remote system holds: the fixture,
    /// and what it filed ([`filed`]), which a full sync that left out would
    /// sweep.
    fn world(&self) -> Vec<SyncItem> {
        let mut world = items(&self.id);
        if self.files_tickets
            && let Some(tickets) = filed().get(&self.id)
        {
            world.extend(tickets.iter().map(|t| ticket_item(&self.id, t)));
        }
        world
    }

    /// The error this instance should report instead of doing the work.
    fn fault_error(&self) -> Option<SourceError> {
        match self.fault {
            Fault::None => None,
            Fault::Unauthorized => Some(SourceError::unauthorized()),
            Fault::Unreachable => Some(SourceError::Unreachable(
                "simulated: connection refused".into(),
            )),
        }
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
// One positional argument per `SyncItem` field the fixture fills, which is the
// point: adding a field to the SPI must not compile until all eight call sites
// below -- the seven kinds in `items` and the tombstone in `tombstoned_item`
// -- have decided what to put in it. A parameter struct would take a
// `..Default::default()` instead and let one kind silently keep the old value.
#[allow(clippy::too_many_arguments)]
fn item(
    source_id: &str,
    kind: &str,
    key: String,
    title: String,
    body: String,
    author: Option<String>,
    updated_at: Option<DateTime<Utc>>,
    payload: &impl Serialize,
    web_url: Option<String>,
) -> SyncItem {
    SyncItem {
        entity: EntityRef::new(source_id, &key),
        kind: kind.to_owned(),
        title,
        body_text: body,
        author,
        updated_at,
        payload: serde_json::to_value(payload).expect("a fixture record must serialize"),
        web_url,
        deleted: false,
    }
}

/// Descriptor id, adapter kind, and therefore the [`EntityRef`] namespace of
/// every item this adapter emits.
const SOURCE_ID: &str = "mock";

/// The item [`MockSource::with_tombstone`] reports as deleted.
///
/// Deliberately *not* one of the fixture's own keys: an item that is both
/// emitted live and tombstoned in one sync would come down to which one the
/// engine wrote last, and the fixture's tickets are the ones every mockup
/// refers to by name.
pub const TOMBSTONED_KEY: &str = "PAY-198";

/// The one fixture ticket that names no [`Project`].
///
/// Every other ticket in the dataset carries one, so a corpus that syncs
/// cleanly still contains the case ADR-0010 pins the project rooms on: a
/// record with no readable project belongs to no project room and is still in
/// *All work* and in its source's room. Without it the miss direction would be
/// reachable only from a hand-written payload, and the demo profile could not
/// show it at all.
///
/// A leaf story rather than the epic or the lone `OPS` ticket: removing either
/// of those would take a whole project out of the dataset, and the two named
/// projects are what make a project room worth drawing here.
pub const UNPROJECTED_KEY: &str = "PAY-236";

/// The tombstoned item itself: an entity that has been withdrawn upstream but
/// whose title the UI still has to be able to show.
fn tombstoned_item(source_id: &str) -> SyncItem {
    SyncItem {
        deleted: true,
        ..item(
            source_id,
            "ticket",
            TOMBSTONED_KEY.to_owned(),
            "Legacy payout reconciliation (withdrawn)".to_owned(),
            "Withdrawn upstream: superseded by the SEPA retry work.".to_owned(),
            None,
            None,
            // `labels` is on every ticket (see [`ticket_payload`]), a withdrawn
            // one included: a record without the key would read as the path
            // naming a key the records do not have.
            &serde_json::json!({ "key": TOMBSTONED_KEY, "status": "Deleted", "labels": [] }),
            // An item withdrawn upstream has no page left to open.
            None,
        )
    }
}

/// The payload the mirror stores for a ticket: the fixture record, with its
/// project moved to **the place a source would have written it**.
///
/// A Jira Data Center issue carries its project at `fields.project`, key and
/// name -- `knobas-source-jira` keeps the record verbatim and has requested
/// `project` in its base field list since M1 (ADR-0010) -- so that is where
/// this puts it. The demo profile then exercises the same payload read
/// (ADR-0007) a real Jira corpus does, instead of a third spelling that exists
/// nowhere but here. Nothing else about the record moves: everything the
/// fixture transcribes stays flat, which is also where the mock's `status` and
/// `priority` already are.
///
/// A ticket the fixture leaves outside every project contributes **no**
/// `fields.project` -- not an empty object, not a key inferred from the issue
/// key. Absence is the miss direction, and a value invented here would take it
/// away from every reader downstream. Pinned by
/// `a_demo_ticket_with_no_project_syncs_and_its_payload_names_none` in
/// `knobas-app/tests/it/demo.rs`.
fn ticket_payload(t: &Ticket) -> serde_json::Value {
    let mut payload = serde_json::to_value(t).expect("a fixture record must serialize");
    if let Some(project) = &t.project {
        payload["fields"]["project"] =
            serde_json::to_value(project).expect("a project must serialize");
    }
    // The two shapes the ticket-system role's declared paths need (#1229) and
    // the fixture's flat record does not have. `labels` is the list the
    // dataset has no entry for, so it is empty on every ticket, which is what
    // Jira sends for a ticket with none; and the epic is a *parent object*
    // (`{"key"}`), where the flat `epic` is the bare key. A ticket with no
    // epic gets no `parent`: absence stays absence.
    payload["labels"] = serde_json::json!([]);
    if let Some(epic) = &t.epic {
        payload["parent"] = serde_json::json!({ "key": epic });
    }
    // The card's Reporter and Logged rows (#2403), by the rule the Jira mock
    // (`knobas-mockd`'s `state.rs`) serves the same dataset with, so the demo
    // and the mock Jira agree: the reporter is whoever assigned the ticket,
    // else its assignee, else priya; it was filed two weeks before its last
    // update (30 days before the dataset's now where it has none); the time
    // spent is its worklogs' sum in seconds, none where it has none. Each
    // worklog entry gains its `seconds`, the unit the worklog path reads.
    payload["reporter"] = serde_json::Value::from(
        t.assigned_by
            .as_deref()
            .or(t.assignee.as_deref())
            .unwrap_or("priya"),
    );
    let updated = t
        .updated
        .unwrap_or_else(|| fixture().today - chrono::Duration::days(30));
    payload["created"] = serde_json::to_value(updated - chrono::Duration::days(14))
        .expect("an instant must serialize");
    if !t.worklogs.is_empty() {
        let seconds: u64 = t.worklogs.iter().map(|w| u64::from(w.minutes) * 60).sum();
        payload["time_spent_seconds"] = serde_json::Value::from(seconds);
    }
    if let Some(entries) = payload["worklogs"].as_array_mut() {
        for (entry, worklog) in entries.iter_mut().zip(&t.worklogs) {
            entry["seconds"] = serde_json::Value::from(u64::from(worklog.minutes) * 60);
        }
    }
    payload
}

/// A build's payload: the record, as the Builds frame's *run* (#1240). The
/// dataset's build is a TeamCity build of a build configuration, which is a run
/// of a pipeline, so the record gains the keys the run's declared paths read:
/// the configuration as its `pipeline`, the number as text, and `finished`
/// only for a build the dataset gives a duration (`4 m 12 s`) -- a running
/// build has not finished, and a success with no duration is not guessed at.
/// `when` is the start, as the dataset's own running build says ("started
/// today 11:45").
fn run_payload(b: &Build) -> serde_json::Value {
    let mut payload = serde_json::to_value(b).expect("a fixture record must serialize");
    payload["pipeline"] = serde_json::Value::from(b.cfg.as_str());
    payload["number"] = serde_json::Value::from(b.num.to_string());
    payload["started"] = serde_json::to_value(b.when).expect("an instant must serialize");
    if let Some(secs) = b.duration.as_deref().and_then(duration_secs) {
        payload["finished"] = serde_json::to_value(b.when + chrono::Duration::seconds(secs))
            .expect("an instant must serialize");
    }
    payload
}

/// `4 m 12 s` as seconds; `None` for any other shape.
fn duration_secs(text: &str) -> Option<i64> {
    let mut words = text.split_whitespace();
    let (m, unit_m, s, unit_s) = (words.next()?, words.next()?, words.next()?, words.next()?);
    (unit_m == "m" && unit_s == "s" && words.next().is_none())
        .then(|| Some(m.parse::<i64>().ok()? * 60 + s.parse::<i64>().ok()?))
        .flatten()
}

/// A build configuration's pipeline (#1240): derived from the `cfg` the
/// dataset's builds name, nothing else.
fn pipeline_payload(cfg: &str) -> serde_json::Value {
    serde_json::json!({ "key": cfg, "id": cfg, "name": cfg })
}

/// The Docs projection (#1240, `knobas_source::docs::DocProjection`): the
/// page the wiki role draws. The dataset gives a page body as one line of
/// `## Heading — text` runs, so each run becomes a heading and a paragraph;
/// the comments are the page's own, as plain paragraphs. No version, labels
/// or history: the dataset records none, and none is invented.
#[must_use]
pub fn doc_content(
    kind: &str,
    payload: &serde_json::Value,
) -> Option<knobas_source::docs::DocContent> {
    use knobas_source::docs::{DocBlock, DocComment, DocContent, Inline};
    if kind != knobas_source::docs::PAGE_KIND {
        return None;
    }
    let text = |text: &str| {
        vec![Inline::Text {
            text: text.trim().to_owned(),
            strong: false,
            emphasis: false,
            code: false,
        }]
    };
    let mut blocks = Vec::new();
    for run in payload["body"].as_str().unwrap_or_default().split("## ") {
        let (head, rest) = run.split_once(" \u{2014} ").unwrap_or((run, ""));
        if head.trim().is_empty() {
            continue;
        }
        blocks.push(DocBlock::Heading {
            level: 2,
            text: text(head),
        });
        if !rest.trim().is_empty() {
            blocks.push(DocBlock::Paragraph { text: text(rest) });
        }
    }
    let comments = payload["comments"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| DocComment {
            id: None,
            author: c["who"].as_str().map(str::to_owned),
            author_name: None,
            at: c["when"].as_str().map(str::to_owned),
            blocks: vec![DocBlock::Paragraph {
                text: text(c["text"].as_str().unwrap_or_default()),
            }],
        })
        .collect();
    Some(DocContent {
        version: None,
        editor_name: None,
        labels: Vec::new(),
        blocks,
        comments,
        sections: Vec::new(),
        history: Vec::new(),
    })
}

/// A page's payload: the record, with the page above it as a parent object
/// (`{"title"}`, which is what the docs tree matches a parent on) where the
/// dataset names one.
fn page_payload(p: &Page) -> serde_json::Value {
    let mut payload = serde_json::to_value(p).expect("a fixture record must serialize");
    payload.as_object_mut().expect("an object").remove("parent");
    // The space is the project (#1240): the dataset names `ENG` as
    // *Engineering*, the one space it has.
    payload["project"] = serde_json::json!({
        "key": p.space,
        "name": if p.space == "ENG" { serde_json::json!("Engineering") } else { serde_json::Value::Null },
    });
    if let Some(parent) = p
        .parent
        .as_deref()
        .and_then(|id| fixture().pages.iter().find(|q| q.id == id))
    {
        payload["parent"] = serde_json::json!({ "id": parent.id, "title": parent.title });
    }
    payload
}

/// The newest commit the fixture records on `branch` of `repo`: the head a
/// branch or a pull request from it stands at.
fn head_commit(repo: &str, branch: &str) -> Option<&'static str> {
    fixture()
        .commits
        .iter()
        .filter(|c| c.repo == repo && c.branch == branch)
        .max_by_key(|c| c.when)
        .map(|c| c.sha.as_str())
}

/// `payload` with the repository it sits in as a parent object (`{"name"}`),
/// which the code-host role's declared paths read (#1229): the fixture's flat
/// `repo` is the bare name, and a parent is an object.
fn in_repo(mut payload: serde_json::Value, repo: &str) -> serde_json::Value {
    payload["repository"] = serde_json::json!({ "name": repo });
    payload
}

/// A pull request's payload: the record, in its repository, with the head
/// commit its branch stands at and a merged *flag* (the record's own `merged`
/// is a timestamp, a different fact; see [`payload_paths`]).
fn pr_payload(p: &PullRequest) -> serde_json::Value {
    let mut payload = in_repo(
        serde_json::to_value(p).expect("a fixture record must serialize"),
        &p.repo,
    );
    payload["is_merged"] = serde_json::Value::Bool(p.merged.is_some());
    if let Some(sha) = p.from.as_deref().and_then(|b| head_commit(&p.repo, b)) {
        payload["head_sha"] = serde_json::Value::from(sha);
    }
    payload
}

/// A branch's payload: the record, in its repository, with the commit it
/// stands at and its repository's default branch, where the fixture knows them.
fn branch_payload(b: &Branch) -> serde_json::Value {
    let mut payload = in_repo(
        serde_json::to_value(b).expect("a fixture record must serialize"),
        &b.repo,
    );
    if let Some(sha) = head_commit(&b.repo, &b.name) {
        payload["head_sha"] = serde_json::Value::from(sha);
    }
    if let Some(default) = fixture()
        .repos
        .iter()
        .find(|r| r.name == b.repo)
        .and_then(|r| r.default_branch.as_deref())
    {
        payload["default_branch"] = serde_json::Value::from(default);
    }
    payload
}

/// A ticket as the sync emits it, a filed one ([`filed`]) included.
fn ticket_item(source_id: &str, t: &Ticket) -> SyncItem {
    item(
        source_id,
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
        &ticket_payload(t),
        Some(format!("{MOCK_BASE}/browse/{}", t.key)),
    )
}

/// The key the mock gives the `n`th ticket an instance files, from 1. Its
/// own prefix, so it never names a fixture ticket.
fn filed_key(n: usize) -> String {
    format!("NEW-{n}")
}

/// Every work item in the fixture, in the order the mock emits them.
fn items(source_id: &str) -> Vec<SyncItem> {
    let f = fixture();
    let mut out = Vec::new();
    for t in &f.tickets {
        out.push(ticket_item(source_id, t));
    }
    for p in &f.prs {
        out.push(item(
            source_id,
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
            &pr_payload(p),
            Some(format!("{MOCK_BASE}/tidewater/{}/pulls/{}", p.repo, p.num)),
        ));
    }
    for b in &f.builds {
        let title = format!("{} #{}", b.cfg, b.num);
        out.push(item(
            source_id,
            "build",
            format!("{}#{}", b.cfg, b.num),
            title.clone(),
            body_text(
                [title, b.status.clone(), b.branch.clone()]
                    .into_iter()
                    .chain(b.log.clone()),
            ),
            b.triggered_by.clone(),
            Some(b.when),
            &run_payload(b),
            Some(format!(
                "{MOCK_BASE}/teamcity/viewLog.html?buildId={}",
                b.num
            )),
        ));
    }
    // One pipeline per build configuration the builds name, ahead of its runs.
    let mut pipelines: Vec<&str> = f.builds.iter().map(|b| b.cfg.as_str()).collect();
    pipelines.sort_unstable();
    pipelines.dedup();
    for cfg in pipelines {
        out.push(item(
            source_id,
            "pipeline",
            cfg.to_owned(),
            cfg.to_owned(),
            body_text([cfg.to_owned()]),
            None,
            None,
            &pipeline_payload(cfg),
            Some(format!(
                "{MOCK_BASE}/teamcity/viewType.html?buildTypeId={cfg}"
            )),
        ));
    }
    for m in &f.monitors {
        out.push(item(
            source_id,
            "monitor",
            m.id.clone(),
            m.name.clone(),
            body_text([m.name.clone(), m.url.clone()]),
            None,
            None,
            m,
            Some(m.url.clone()),
        ));
    }
    for p in &f.pages {
        out.push(item(
            source_id,
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
            &page_payload(p),
            Some(format!("{MOCK_BASE}/wiki/{}/{}", p.space, p.id)),
        ));
    }
    for c in &f.commits {
        out.push(item(
            source_id,
            "commit",
            c.sha.clone(),
            c.msg.clone(),
            body_text([c.msg.clone(), c.branch.clone()]),
            Some(c.by.clone()),
            Some(c.when),
            &in_repo(
                serde_json::to_value(c).expect("a fixture record must serialize"),
                &c.repo,
            ),
            Some(format!("{MOCK_BASE}/tidewater/{}/commit/{}", c.repo, c.sha)),
        ));
    }
    // Repositories before their branches, because that is the order a reader
    // needs them in: `knobas_app::checkout`'s `repo_of` answers a branch by
    // finding the repo whose id its own extends, and a corpus that emitted the
    // branches first would read as three branches belonging to nothing until
    // the sink caught up. Nothing in the engine depends on the order -- the
    // upsert is per item -- so this is for whoever reads a sync log.
    for r in &f.repos {
        out.push(item(
            source_id,
            "repo",
            r.name.clone(),
            r.name.clone(),
            body_text([r.name.clone(), r.lang.clone()]),
            // No owner and no timestamp in the dataset, and neither is
            // invented. The brief's repository table has a name, a language, a
            // local clone and a note (`payout-service`'s becomes
            // `default_branch` and `commit_count`) -- and nobody to attribute
            // it to, nor a moment to date it at. Absence is what the fixture
            // conventions above promise for a fact the brief does not give.
            None,
            None,
            r,
            Some(repo_url(&r.name)),
        ));
    }
    for b in &f.branches {
        out.push(item(
            source_id,
            "branch",
            branch_key(&b.repo, &b.name),
            b.name.clone(),
            body_text([b.name.clone(), b.repo.clone()]),
            None,
            None,
            &branch_payload(b),
            Some(format!("{}/src/branch/{}", repo_url(&b.repo), b.name)),
        ));
    }
    out
}

/// A repository's page, and the URL a clone's `origin` is matched against.
///
/// `knobas_core::checkout` reduces a remote to host + owner/repo, so this is
/// the value the scan compares a `.git/config` against -- and `tidewater` is
/// the owner segment this fixture's *other repository links* already carry
/// (`{MOCK_BASE}/tidewater/{repo}/pulls/{n}`, `.../commit/{sha}`; the ticket,
/// build and page links are their own systems' shapes and carry no owner). One
/// function, so the repo item's URL and the branch items' base cannot drift
/// apart.
fn repo_url(repo: &str) -> String {
    format!("{MOCK_BASE}/tidewater/{repo}")
}

/// A branch's key: its repository's, plus the ref.
///
/// **The prefix is load-bearing** (§4.2 fixes the key grammar per source).
/// `knobas_app::checkout`'s `repo_of` finds a branch's repository as the
/// longest repo id in the same source that the branch id starts with, which is
/// how it costs no knowledge of any adapter's spelling. `knobas-source-gitea`
/// earns that with `gitea:owner/repo` and `gitea:owner/repo@refs/heads/name`;
/// this is the same shape, without the owner the mock's keys have never
/// carried (a pull request here is `payout-service#142`). A key that did not
/// extend its repo's would leave every branch detail answering *no checkout*
/// with the repo, the clone and the setting all correct.
fn branch_key(repo: &str, branch: &str) -> String {
    format!("{repo}@refs/heads/{branch}")
}

/// Where the fixture's records keep what knobas reads (#277, #1229).
///
/// Only what the transcription really carries, in the spelling the payload
/// builders ([`ticket_payload`], [`pr_payload`], [`branch_payload`]) leave it
/// in: a ticket's `status` and `priority` are flat, because that is where the
/// dataset has them, and its project is at `fields.project` because that is
/// where a source would have written it.
///
/// **The two roles the mock claims** (#1229, ADR-0029) set what must be
/// declared here: `knobas_source::role::MINIMUMS` names the paths of a
/// `code-host` and a `ticket-system`, and a descriptor that claims a role and
/// leaves one out fails the battery's clause 10. So the paths below are the
/// minimum's, each pointing at a fact the dataset has, in the shape the
/// reader wants:
///
/// * a *parent* is an object, and the fixture's flat `repo` and `epic` are
///   bare names, so the builders add `repository` and `parent`;
/// * a pull request's *merged* is a boolean, and [`PullRequest::merged`] is a
///   timestamp, so the builder adds `is_merged` and leaves the timestamp be;
/// * a *commit* a branch or a pull request stands at is the newest commit the
///   fixture records on the branch (`head_sha`), absent where it records none;
/// * *reviewers* and *assignee* point at the person **ids** the fixture
///   records ([`Person::id`], `mara`), not the username the inbox matches
///   against ([`Person::username`]). The minimum forces the declaration, and
///   it changes nothing the demo read before: an id never equals an identity,
///   so the inbox rules these feed still do not fire on the mock's records.
///
/// [`Person::id`]: Person::id
/// [`Person::username`]: Person::username
fn payload_paths() -> Vec<knobas_source::KindPaths> {
    use knobas_source::{
        Frame, KindPaths, ListPath, ParentPath, PayloadPath, StatePath, StateValue, Tone,
    };
    let in_repo = || {
        vec![ParentPath {
            kind: "repo".to_owned(),
            at: PayloadPath::of(["repository"]),
            names: vec![PayloadPath::of(["name"])],
        }]
    };
    vec![
        // No blocked-like and no done-like set (#1501): the dataset's statuses
        // are drawn as written, and a done set here would archive the demo's
        // finished jobs out from under the mockups they were drawn for.
        KindPaths {
            kind: "ticket".to_owned(),
            status_name: vec![PayloadPath::of(["status"])],
            priority: vec![PayloadPath::of(["priority"])],
            assignee: vec![PayloadPath::of(["assignee"])],
            issue_type: vec![PayloadPath::of(["type"])],
            labels: vec![ListPath {
                at: PayloadPath::of(["labels"]),
                entry: PayloadPath::default(),
            }],
            parent: vec![ParentPath {
                kind: "ticket".to_owned(),
                at: PayloadPath::of(["parent"]),
                names: vec![PayloadPath::of(["key"])],
            }],
            project_key: vec![PayloadPath::of(["fields", "project", "key"])],
            project_name: vec![PayloadPath::of(["fields", "project", "name"])],
            // The dataset's descriptions are Markdown (#2308), so the demo's
            // ticket readers draw them with the Markdown component.
            description: vec![PayloadPath::of(["description"])],
            markup: knobas_source::Markup::Markdown,
            // What `ticket_payload` adds for the card's Reporter and Logged
            // rows (#2403); the worklog's authors are person ids, as the
            // assignee is.
            reporter: vec![PayloadPath::of(["reporter"])],
            created: vec![PayloadPath::of(["created"])],
            time_spent: vec![PayloadPath::of(["time_spent_seconds"])],
            worklog: vec![knobas_source::WorklogPath {
                at: PayloadPath::of(["worklogs"]),
                author: vec![PayloadPath::of(["who"])],
                seconds: vec![PayloadPath::of(["seconds"])],
            }],
            ..KindPaths::default()
        },
        KindPaths {
            kind: "pr".to_owned(),
            parent: in_repo(),
            state: vec![StatePath {
                at: PayloadPath::of(["state"]),
                values: vec![
                    StateValue::of("open", Tone::Info),
                    StateValue::of("merged", Tone::Ok),
                ],
            }],
            head: vec![PayloadPath::of(["from"])],
            // The target branch (#1227), where the dataset names one.
            base: vec![PayloadPath::of(["to"])],
            commit: vec![PayloadPath::of(["head_sha"])],
            reviewers: vec![ListPath {
                at: PayloadPath::of(["review_requested_from"]),
                entry: PayloadPath::of(["who"]),
            }],
            merged: vec![PayloadPath::of(["is_merged"])],
            ..KindPaths::default()
        },
        KindPaths {
            kind: "branch".to_owned(),
            parent: in_repo(),
            commit: vec![PayloadPath::of(["head_sha"])],
            default_branch: vec![PayloadPath::of(["default_branch"])],
            ..KindPaths::default()
        },
        KindPaths {
            kind: "commit".to_owned(),
            parent: in_repo(),
            commit: vec![PayloadPath::of(["sha"])],
            ..KindPaths::default()
        },
        // The Builds frame (#1240): the dataset's build is a run of the
        // pipeline its build configuration names.
        KindPaths {
            kind: "pipeline".to_owned(),
            frame: Some(Frame::Pipeline),
            name: vec![PayloadPath::of(["name"])],
            pipeline: vec![PayloadPath::of(["id"])],
            ..KindPaths::default()
        },
        KindPaths {
            kind: "build".to_owned(),
            frame: Some(Frame::Run),
            pipeline: vec![PayloadPath::of(["pipeline"])],
            number: vec![PayloadPath::of(["number"])],
            branch: vec![PayloadPath::of(["branch"])],
            started: vec![PayloadPath::of(["started"])],
            finished: vec![PayloadPath::of(["finished"])],
            state: vec![StatePath {
                at: PayloadPath::of(["status"]),
                values: vec![
                    StateValue::of("running", Tone::Info),
                    StateValue::of("failed", Tone::Err),
                    StateValue::of("success", Tone::Ok),
                ],
            }],
            ..KindPaths::default()
        },
        // The Docs role (#1240): the space is the project, and a page's parent
        // is the page above it, named by title as the docs tree matches it.
        KindPaths {
            kind: "page".to_owned(),
            project_key: vec![PayloadPath::of(["project", "key"])],
            project_name: vec![PayloadPath::of(["project", "name"])],
            parent: vec![ParentPath {
                kind: "page".to_owned(),
                at: PayloadPath::of(["parent"]),
                names: vec![PayloadPath::of(["title"])],
            }],
            ..KindPaths::default()
        },
        // The Monitors role (#1240), in Uptime Kuma's own words.
        KindPaths {
            kind: "monitor".to_owned(),
            status_name: vec![PayloadPath::of(["state"])],
            state: vec![StatePath {
                at: PayloadPath::of(["state"]),
                values: vec![
                    StateValue::of("up", Tone::Ok),
                    StateValue::of("down", Tone::Err),
                    StateValue::of("pending", Tone::Warn),
                ],
            }],
            monitor_type: vec![PayloadPath::of(["type"])],
            response_time: vec![PayloadPath::of(["response_time_ms"])],
            ..KindPaths::default()
        },
    ]
}

#[async_trait::async_trait]
impl Source for MockSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            // The instance, which `build` may have renamed; the kind below is
            // the adapter and never moves.
            id: self.id.clone(),
            adapter_kind: SOURCE_ID.to_owned(),
            name: "Tidewater (mock)".to_owned(),
            // P12: `Search` means server-side search, which the fixture has no
            // entry point for. `Write` stays -- it and `write_ops` are two
            // signals for one fact, and the battery needs a declared op to
            // exercise the write path against.
            // `Refresh` unless this instance was configured to offer the run
            // alone: the read below answers every kind it emits.
            capabilities: if self.refresh {
                vec![Capability::Write, Capability::Refresh]
            } else {
                vec![Capability::Write]
            },
            adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
            // Nothing to authenticate against: the fixture is compiled in.
            auth_methods: Vec::new(),
            accepts_account: false,
            min_sync_interval_secs: knobas_source::DEFAULT_MIN_SYNC_INTERVAL_SECS,
            // The roles the fixture's corpus fills (#1229, #1240): the shell
            // draws each surface from the sources that claim its role, and
            // the demo's only source is this one. Builds read the `build`
            // records as runs of a pipeline per build configuration, Docs the
            // pages under their parent, Monitors the two health checks.
            //
            // A claim states the role's minimum, reads included, and the
            // minimum is the same for a real adapter and a fixture. The mock
            // answers none of them live (`view` is `None`, there is no
            // workflow to read: see `reachable_transitions`), so its claim is
            // "my records fill the role", which `payload_paths` and the
            // battery's clauses 6 and 10 hold it to.
            roles: {
                use knobas_source::role::{
                    CI, CODE_HOST, MONITORING, RoleClaim, TICKET_SYSTEM, WIKI, minimum,
                };
                [CODE_HOST, TICKET_SYSTEM, CI, WIKI, MONITORING]
                    .map(|role| {
                        let reads = minimum(role).map_or(&[][..], |m| m.reads);
                        RoleClaim::new(role, reads.iter().copied())
                    })
                    .to_vec()
            },
            source_actions: Vec::new(),
            // The code host's clone URLs (#1197) are required of its claim.
            // The demo source has an empty base URL, so this names no address
            // and a clone is built from each repository's own page, as before.
            clone_from: vec![knobas_source::clone_urls::CloneFrom::BaseUrl],
            url_forms: Vec::new(),
            code_pages: None,
            write_ops: if self.files_tickets {
                vec!["comment".to_owned(), "create_ticket".to_owned()]
            } else {
                vec!["comment".to_owned()]
            },
            // The UI renders this source's launcher groups and chips from these
            // alone, so every kind `sync` emits is declared here.
            entity_kinds: vec![
                KindInfo {
                    id: "ticket".to_owned(),
                    label: "Ticket".to_owned(),
                    plural: "Tickets".to_owned(),
                    monogram: "TK".to_owned(),
                    full_sync_exhaustive: true,
                    refreshes: true,
                },
                KindInfo {
                    id: "pr".to_owned(),
                    label: "Pull request".to_owned(),
                    plural: "Pull requests".to_owned(),
                    monogram: "PR".to_owned(),
                    full_sync_exhaustive: true,
                    refreshes: true,
                },
                KindInfo {
                    id: "build".to_owned(),
                    label: "Build".to_owned(),
                    plural: "Builds".to_owned(),
                    monogram: "BU".to_owned(),
                    full_sync_exhaustive: true,
                    refreshes: true,
                },
                KindInfo {
                    id: "page".to_owned(),
                    label: "Page".to_owned(),
                    plural: "Pages".to_owned(),
                    monogram: "PG".to_owned(),
                    full_sync_exhaustive: true,
                    refreshes: true,
                },
                KindInfo {
                    id: "commit".to_owned(),
                    label: "Commit".to_owned(),
                    plural: "Commits".to_owned(),
                    monogram: "CM".to_owned(),
                    full_sync_exhaustive: true,
                    refreshes: true,
                },
                // The two the corpus gained in #537. `RP` and `BR` are the
                // monograms `app/src/lib/shell/kinds.ts` already draws for
                // these kinds -- that table is the fallback for a kind no
                // descriptor declared, so matching it keeps the demo drawing
                // one glyph per kind whether the descriptor reached the
                // frontend or not. It is not repo-wide agreement and does not
                // claim to be: `knobas-source-gitea` declares `RE` for its own
                // `repo`, pinned that way in §4.2, and a descriptor always
                // wins over the fallback for its own source.
                KindInfo {
                    id: "repo".to_owned(),
                    label: "Repository".to_owned(),
                    plural: "Repositories".to_owned(),
                    monogram: "RP".to_owned(),
                    full_sync_exhaustive: true,
                    refreshes: true,
                },
                KindInfo {
                    id: "branch".to_owned(),
                    label: "Branch".to_owned(),
                    plural: "Branches".to_owned(),
                    monogram: "BR".to_owned(),
                    full_sync_exhaustive: true,
                    refreshes: true,
                },
                // The Builds frame's pipeline (#1240); a `build` is its run.
                KindInfo {
                    id: "pipeline".to_owned(),
                    label: "Pipeline".to_owned(),
                    plural: "Pipelines".to_owned(),
                    monogram: "PL".to_owned(),
                    full_sync_exhaustive: true,
                    refreshes: true,
                },
                KindInfo {
                    id: "monitor".to_owned(),
                    label: "Monitor".to_owned(),
                    plural: "Monitors".to_owned(),
                    monogram: "MO".to_owned(),
                    full_sync_exhaustive: true,
                    refreshes: true,
                },
            ],
            // Every kind above declares `full_sync_exhaustive: true`: the
            // fixture is the entire world this source has, so a full sync
            // emits all of it and the engine's sweep is safe for each kind.
            // Nothing to configure, so the Add-source form for the mock is empty.
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            payload_paths: payload_paths(),
        }
    }

    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError> {
        if let Some(err) = self.fault_error() {
            return Err(err);
        }
        // The mock authenticates nothing, but the Add-source flow (M1 stream
        // D) was built against it -- "the mock source is the frontend's
        // backend", `docs/agents/working-model.md`, extracted from roadmap §3
        // -- so it reports what a real source would: the fixture's owner, and
        // its own version as the server's.
        Ok(ConnectionInfo {
            account: Some(
                fixture()
                    .person("mara")
                    .map_or_else(|| "mara".to_owned(), |p| p.username.clone()),
            ),
            server_version: Some(format!("knobas-source-mock {}", env!("CARGO_PKG_VERSION"))),
            // Nothing to expire: the fixture is compiled in.
            secret_expires_at: None,
            detail: Some("compiled-in fixture; nothing was contacted".to_owned()),
            discovered: std::collections::BTreeMap::new(),
        })
    }

    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError> {
        if let Some(err) = self.fault_error() {
            return Err(err);
        }
        // A caller already at the current version has nothing to fetch. Any
        // other cursor is from an older fixture and gets a full sync -- which
        // is the point of versioning the cursor (the bump rule is on
        // [`CURSOR`] itself).
        if cursor.as_deref() == Some(CURSOR) {
            return Ok(CURSOR.to_owned());
        }
        let mut fixture = self.world();
        if self.tombstone {
            // Last, and still inside the sync: a deletion is an item like any
            // other, carrying the last-known title so the UI can render what
            // vanished.
            fixture.push(tombstoned_item(&self.id));
        }
        // The walk reading (#1379): the mock's outer unit is the item itself,
        // reported before each one and once after the last, so the demo load
        // draws a determinate bar too.
        let of = u64::try_from(fixture.len()).unwrap_or(u64::MAX);
        for (done, it) in (0u64..).zip(fixture) {
            sink.walked(done, Some(of), "items").await;
            // Not the adapter's failure to swallow: a sink that rejected an
            // item wants the sync abandoned, not the remaining items pushed at
            // it and a fresh cursor handed back over the gap.
            sink.item(it).await?;
        }
        sink.walked(of, Some(of), "items").await;
        Ok(CURSOR.to_owned())
    }

    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError> {
        // The SPI's own mapping, not a copy of it: a per-adapter table drifts
        // from the identifiers descriptors are validated against.
        let id = op.identifier();
        // Refused locally, before the fault check: an op this source never
        // advertised is a caller bug, and stays one whether or not the remote
        // system happens to be reachable.
        if !self.descriptor().write_ops.iter().any(|w| w == id) {
            return Err(SourceError::protocol(format!("unsupported write op: {id}")));
        }
        if let Some(err) = self.fault_error() {
            return Err(err);
        }
        // A filed ticket is held and named by its key, which is its entity's
        // local part (`WriteOp::receipt_names_an_entity`).
        let receipt = match &op {
            WriteOp::CreateTicket {
                title,
                body,
                ticket_type,
                ..
            } => {
                let mut filed = filed();
                let tickets = filed.entry(self.id.clone()).or_default();
                let key = filed_key(tickets.len() + 1);
                tickets.push(Ticket {
                    key: key.clone(),
                    epic: None,
                    summary: title.clone(),
                    kind: ticket_type.clone(),
                    status: "To Do".to_owned(),
                    priority: None,
                    assignee: None,
                    assigned_by: None,
                    updated: None,
                    description: Some(body.clone()),
                    estimate_h: None,
                    spent_week_m: None,
                    blocked_by: Vec::new(),
                    comments: Vec::new(),
                    worklogs: Vec::new(),
                    project: None,
                });
                WriteReceipt::id(key)
            }
            // A comment creates no row with an id of its own, so there is
            // nothing to hand back -- see `WriteReceipt`.
            _ => WriteReceipt::none(),
        };
        self.lock().push(op);
        Ok(receipt)
    }

    /// The item its full sync emits for `entity`, or *gone* for an id it
    /// does not emit live, [`TOMBSTONED_KEY`] among them (ADR-0031). Not
    /// offered by an instance configured with `"refresh": false`.
    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError> {
        if !self.refresh {
            return Ok(RefreshAnswer::NotOffered);
        }
        if let Some(err) = self.fault_error() {
            return Err(err);
        }
        Ok(self
            .world()
            .into_iter()
            .find(|item| item.entity.to_string() == entity)
            .map_or(RefreshAnswer::Gone, RefreshAnswer::Item))
    }

    /// Refused: the fixture has statuses but no workflow (#498).
    ///
    /// The distinction is the whole reason this refuses. `fixtures/tidewater`
    /// gives every ticket a status and the demo board draws columns from them,
    /// so it is tempting to answer with the other statuses the fixture uses --
    /// and that answer would be a **corpus**, which is exactly the guess the
    /// select made before this read existed and exactly what a *reachable
    /// transition* is not: an answer about now, from a source that knows what
    /// its workflow offers from where the ticket stands. The fixture knows no
    /// such thing, and a mock certifies nothing anyway (ADR-0013). So the mock
    /// refuses, the demo ticket draws no select and makes no read (the detail
    /// gates it on the descriptor's `transition` op), and the corpus-observed
    /// offer stays where it belongs -- in the shell, as the fallback for a
    /// source that *has* a workflow and could not be read, labelled *offer
    /// unverified*.
    ///
    /// Consistent with the descriptor either way: the mock declares `comment`
    /// and no `transition`, so the contract battery requires this refusal.
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError> {
        Err(SourceError::protocol(format!(
            "the mock adapter has no workflow -- its fixture has statuses, not a set of moves -- \
             so there are no reachable transitions to read for {entity:?}"
        )))
    }

    /// No source view (ADR-0020): the mock draws nothing for any kind, so the
    /// demo's details are the generic projection.
    async fn view(
        &self,
        _entity: &str,
        _section: Option<&str>,
    ) -> Result<Option<knobas_source::view::SourceView>, SourceError> {
        Ok(None)
    }
}

// -- construction, the shape every adapter crate exposes ---------------------

/// The mock's descriptor template: one per adapter kind, `id == adapter_kind`
/// (§4.2). This is what `list_adapters` serves the Add-source form.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    MockSource::new().descriptor()
}

/// Build a mock instance from its stored configuration.
///
/// The same signature every adapter crate exposes, so stream F's registry has
/// one shape to call and something to exercise it against before a real
/// adapter exists.
///
/// The fixture is compiled in, so `base_url`, `auth` and `secret` are ignored --
/// every other adapter uses all three. **`config` is not**: it carries the two
/// knobs [`MockSource`] has, and carrying them is what lets a caller drive a
/// *registry-built* adapter that fails. Until it did, the only faulted mock in
/// existence was one a test constructed by hand, so nothing going through the
/// real registry could be made to fail at all (the narrow remainder of PR #24's
/// finding, carried on the M0/M1 ledger as #48).
///
/// ```json
/// { "fault": "none" | "unauthorized" | "unreachable", "tombstone": true,
///   "refresh": false, "files_tickets": true }
/// ```
///
/// Every key is optional and each defaults to the healthy fixture, refreshing
/// and filing nothing, so a source added through the Add-source form -- whose
/// generated config carries none -- is exactly the mock it has always been.
///
/// # Errors
///
/// [`SourceError::Protocol`] if the instance is not this adapter's to build, if
/// its id cannot be an entity namespace, or if `config` is anything
/// [`MockConfig`] cannot read -- an unknown key, a misspelled one, a value of
/// the wrong type, or a blob that is not an object. All three are configuration
/// mistakes, and all three are worth catching before a sync writes rows under a
/// namespace nothing can address -- the third especially, since the alternative
/// is a source that silently builds healthy and syncs when the caller asked for
/// one that fails.
pub fn build(instance: SourceInstance) -> Result<Box<dyn Source>, SourceError> {
    if instance.kind != SOURCE_ID {
        return Err(SourceError::protocol(format!(
            "knobas-source-mock cannot build an instance of kind {:?}",
            instance.kind
        )));
    }
    knobas_source::instance::validate_instance_id(&instance.id)
        .map_err(|error| SourceError::protocol(error.to_string()))?;
    // Both knobs, independently: a faulted mock never reaches the tombstone and
    // a healthy one always does, so picking one over the other would be a silent
    // precedence rule where there is no reason for one.
    let config = MockConfig::from_json(&instance.config)?;
    Ok(Box::new(MockSource {
        id: instance.id,
        tombstone: config.tombstone,
        refresh: config.refresh,
        files_tickets: config.files_tickets,
        ..MockSource::with_fault(config.fault.into())
    }))
}

/// The mock's `source_config.config`: the two knobs [`MockSource`] has.
///
/// `deny_unknown_fields`, the same as every real adapter's config
/// (`GiteaConfig`, `JiraConfig`, `TeamCityConfig`) and for a sharper reason
/// here: a key-by-key reader accepts `{"faultt": "unauthorized"}` in silence and
/// hands back a **healthy** source, which is a test passing for the wrong
/// reason -- the exact failure honouring the config exists to prevent. It also
/// closes the other half of that hole, a `config` that is not an object at all:
/// `Value::get` answers `None` for a string or an array just as it does for an
/// absent key.
///
/// `default` on both, so `{}` -- what the Add-source form generates, this
/// adapter's `config_schema` declaring no properties -- is the healthy fixture
/// it has always been. **The schema is deliberately not widened to declare
/// these**: they drive tests and fixtures, and a form offering a person a
/// *Simulate a 401* checkbox would be offering them a broken source.
#[derive(Debug, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MockConfig {
    fault: ConfiguredFault,
    /// Whether a full sync also reports [`TOMBSTONED_KEY`] as deleted.
    tombstone: bool,
    /// Whether the instance reads one item by id. On by default.
    refresh: bool,
    /// Whether the instance files tickets. Off by default.
    files_tickets: bool,
}

impl Default for MockConfig {
    fn default() -> Self {
        Self {
            fault: ConfiguredFault::default(),
            tombstone: false,
            refresh: true,
            files_tickets: false,
        }
    }
}

/// [`Fault`], as it is spelled in a config.
///
/// A mirror rather than a `Deserialize` on `Fault` itself: `Fault` lives in
/// `knobas-source`, which §10.8 freezes, and it is a *battery* input with no
/// business carrying a wire format. The `From` below has no wildcard arm, so a
/// fault added there stops this file compiling until it has a spelling.
#[derive(Debug, Default, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConfiguredFault {
    #[default]
    None,
    Unauthorized,
    Unreachable,
}

impl From<ConfiguredFault> for Fault {
    fn from(configured: ConfiguredFault) -> Self {
        match configured {
            ConfiguredFault::None => Fault::None,
            ConfiguredFault::Unauthorized => Fault::Unauthorized,
            ConfiguredFault::Unreachable => Fault::Unreachable,
        }
    }
}

impl MockConfig {
    /// Parse `source_config.config`, refusing anything this adapter's schema
    /// does not describe -- the same door, and the same words, as
    /// [`knobas_source_gitea::GiteaConfig::from_json`].
    fn from_json(value: &serde_json::Value) -> Result<Self, SourceError> {
        serde_json::from_value(value.clone()).map_err(|e| {
            SourceError::protocol(format!("knobas-source-mock: invalid source config: {e}"))
        })
    }
}
