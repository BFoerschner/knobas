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
    Capability, ConnectionInfo, Cursor, KindInfo, Sink, Source, SourceDescriptor, SourceError,
    SyncItem, WriteOp,
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
const CURSOR: &str = "tidewater-v2";

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
            id: SOURCE_ID.to_owned(),
            fault,
            tombstone: false,
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
// point: adding a field to the SPI must not compile until all six call sites
// below -- the five kinds in `items` and the tombstone in `tombstoned_item`
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
            &serde_json::json!({ "key": TOMBSTONED_KEY, "status": "Deleted" }),
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
/// `knobas-app/tests/demo.rs`.
fn ticket_payload(t: &Ticket) -> serde_json::Value {
    let mut payload = serde_json::to_value(t).expect("a fixture record must serialize");
    if let Some(project) = &t.project {
        payload["fields"]["project"] =
            serde_json::to_value(project).expect("a project must serialize");
    }
    payload
}

/// Every work item in the fixture, in the order the mock emits them.
fn items(source_id: &str) -> Vec<SyncItem> {
    let f = fixture();
    let mut out = Vec::new();
    for t in &f.tickets {
        out.push(item(
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
        ));
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
            p,
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
            b,
            Some(format!(
                "{MOCK_BASE}/teamcity/viewLog.html?buildId={}",
                b.num
            )),
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
            p,
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
            c,
            Some(format!("{MOCK_BASE}/tidewater/{}/commit/{}", c.repo, c.sha)),
        ));
    }
    out
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
            capabilities: vec![Capability::Write],
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
                    full_sync_exhaustive: true,
                },
                KindInfo {
                    id: "pr".to_owned(),
                    label: "Pull request".to_owned(),
                    plural: "Pull requests".to_owned(),
                    monogram: "PR".to_owned(),
                    full_sync_exhaustive: true,
                },
                KindInfo {
                    id: "build".to_owned(),
                    label: "Build".to_owned(),
                    plural: "Builds".to_owned(),
                    monogram: "BU".to_owned(),
                    full_sync_exhaustive: true,
                },
                KindInfo {
                    id: "page".to_owned(),
                    label: "Page".to_owned(),
                    plural: "Pages".to_owned(),
                    monogram: "PG".to_owned(),
                    full_sync_exhaustive: true,
                },
                KindInfo {
                    id: "commit".to_owned(),
                    label: "Commit".to_owned(),
                    plural: "Commits".to_owned(),
                    monogram: "CM".to_owned(),
                    full_sync_exhaustive: true,
                },
            ],
            // Every kind above declares `full_sync_exhaustive: true`: the
            // fixture is the entire world this source has, so a full sync
            // emits all of it and the engine's sweep is safe for each kind.
            // Nothing to configure, so the Add-source form for the mock is empty.
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
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
        for it in items(&self.id) {
            // Not the adapter's failure to swallow: a sink that rejected an
            // item wants the sync abandoned, not the remaining items pushed at
            // it and a fresh cursor handed back over the gap.
            sink.item(it).await?;
        }
        if self.tombstone {
            // Last, and still inside the sync: a deletion is an item like any
            // other, carrying the last-known title so the UI can render what
            // vanished.
            sink.item(tombstoned_item(&self.id)).await?;
        }
        Ok(CURSOR.to_owned())
    }

    async fn write(&self, op: WriteOp) -> Result<(), SourceError> {
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
        self.lock().push(op);
        Ok(())
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
/// { "fault": "none" | "unauthorized" | "unreachable", "tombstone": true }
/// ```
///
/// Both keys are optional and both default to the healthy fixture, so a source
/// added through the Add-source form -- whose generated config carries neither
/// -- is exactly the mock it has always been.
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
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct MockConfig {
    fault: ConfiguredFault,
    /// Whether a full sync also reports [`TOMBSTONED_KEY`] as deleted.
    tombstone: bool,
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
