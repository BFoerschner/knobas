//! The source SPI -- the one interface every knobas adapter implements.
//!
//! Jira, Gitea, TeamCity, Confluence, Uptime Kuma, Flowrun and everything added
//! later reach knobas through [`Source`]. The contract freezes at the M0 exit:
//! changing anything in this module afterwards needs an orchestrator decision
//! and a spec update, because it breaks every adapter at once.
//!
//! Two properties are load-bearing and must survive any future edit:
//!
//! * **Transport-agnostic.** Everything crossing the SPI is plain
//!   serde-serializable data (spec §3a), so an adapter can later run out of
//!   process behind a pipe or socket without a contract change. No handles, no
//!   connections, no trait objects in the payload types.
//! * **Self-describing.** The UI renders a source's items from
//!   [`SourceDescriptor::entity_kinds`], its action bar from
//!   [`SourceDescriptor::write_ops`], its Add-source form from
//!   [`SourceDescriptor::config_schema`] and
//!   [`SourceDescriptor::auth_methods`]. Nothing downstream is allowed to carry
//!   a hardcoded per-adapter table, so a new adapter needs no UI work (spec §3a:
//!   "one new adapter and zero changes to knobas core, search, or UI") -- which
//!   is why [`contract::battery`] rejects an adapter that emits an item of an
//!   undeclared kind, or accepts a write op it never declared.
//!
//! Adapters prove they honour the contract by running [`contract::battery`]
//! against themselves in their own test suite.

pub mod contract;
pub mod instance;

/// Everything knobas needs to know about a configured adapter instance.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceDescriptor {
    /// Instance id, e.g. `"jira"`. Doubles as the [`EntityRef`] namespace for
    /// every item this instance emits, so it must be non-blank and free of `:`.
    ///
    /// [`EntityRef`]: knobas_core::entity::EntityRef
    pub id: String,
    /// Which adapter this is an instance of, e.g. `"jira"`, `"mock"`. Distinct
    /// from [`Self::entity_kinds`], which is about the *items* it emits.
    pub adapter_kind: String,
    /// Human-readable name for the source list.
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub adapter_version: String,
    /// How this adapter can authenticate (spec §3, §3a). The Add-source form
    /// offers these; the chosen method's secret goes to the OS keychain and
    /// never into [`Self::config_schema`]'s config blob.
    pub auth_methods: Vec<AuthMethod>,
    /// Which [`WriteOp`]s this adapter supports, as the stable snake_case
    /// identifiers documented on that enum (`Comment` → `"comment"`).
    ///
    /// The UI renders its action bar from this rather than from a hardcoded
    /// per-adapter table, which is why [`Capability::Write`] alone is not
    /// enough: once `WriteOp` grows, "supports writes" no longer says *which*
    /// actions to offer. An adapter must reject any op absent from this list
    /// with [`SourceError::Protocol`].
    ///
    /// This and [`Capability::Write`] are two signals for one fact and must
    /// agree: declaring `Write` with no ops leaves the UI nothing to offer,
    /// and listing ops without `Write` makes the source read as read-only
    /// while advertising actions. [`contract::battery`] enforces both
    /// directions.
    pub write_ops: Vec<String>,
    /// Entity kinds this adapter emits, with display metadata -- the UI renders
    /// a new source's items (launcher groups, chips, monograms) from this
    /// alone, never from hardcoded kind lists (spec §3a extensibility).
    /// [`SyncItem::kind`] must name one of these.
    ///
    /// Each entry also carries [`KindInfo::full_sync_exhaustive`], which is
    /// the sweep's precondition and is declared **per kind** (ADR-0003).
    pub entity_kinds: Vec<KindInfo>,
    /// JSON Schema for this adapter's configuration; the Add-source form is
    /// generated from it (spec §3a). Never holds secrets -- those live in the
    /// OS keychain, keyed by the chosen [`AuthMethod`]. M0: the mock declares
    /// an empty object schema.
    pub config_schema: serde_json::Value,
    /// Where this adapter's records keep the things knobas reads but §4.1 does
    /// not normalize: a status, a priority, an assignee, requested reviewers,
    /// a merged flag, a project key and name -- and which of its status names
    /// mean blocked. One entry per entity kind that has any of them.
    ///
    /// **This is the self-describing property applied to payloads** (M3.1,
    /// issue #277, ADR-0007's recorded destination). Until it existed, every
    /// reader outside an adapter held a little table of per-source spellings:
    /// `coalesce(fields.status.name, status)` in the mini board, three arms
    /// for a project key in the census, two assignee spellings in the inbox --
    /// the same coupling [`Self::entity_kinds`] and [`Self::write_ops`] exist
    /// to prevent, in the one place §4.1 left knobas nothing normalized to
    /// read. A third source's spelling is now one declaration here rather than
    /// one more arm in four statements.
    ///
    /// The rules a declaration is bound by, all enforced by
    /// [`contract::battery`]:
    ///
    /// * every [`KindPaths::kind`] names one of [`Self::entity_kinds`] -- an
    ///   adapter cannot declare paths for a kind it does not emit;
    /// * a declared path lands on a value of its type on this adapter's own
    ///   items, or the source says nothing there. A path into an object the
    ///   source really wrote, naming a key that object does not have, is the
    ///   adapter pointing at a field its records lack;
    /// * a field a kind does not declare is a **miss** for every reader --
    ///   never a guess, never a knobas-side fallback.
    ///
    /// `#[serde(default)]`: a descriptor from a peer built before this grew --
    /// an out-of-process adapter, a stored blob -- decodes as an adapter that
    /// declares nothing, which every reader already handles.
    ///
    /// [`KindPaths::kind`]: knobas_core::payload::KindPaths::kind
    #[serde(default)]
    pub payload_paths: Vec<knobas_core::payload::KindPaths>,
}

// The declaration types themselves live in `knobas-core`, not here: the
// adapters that write them depend on this crate, and the readers that resolve
// them are in `knobas-core`, which may not depend on the SPI. Re-exported so
// an adapter needs one `use`.
pub use knobas_core::payload::{KindPaths, ListPath, PayloadPath};

/// What a successful [`Source::test_connection`] learned about the far end.
///
/// Every field is optional and an adapter fills only what its API actually
/// exposes: the Add-source flow renders what it got and says nothing about
/// what it did not (§3 "Credential health: PAT expiry countdown"). A source
/// whose API has no "who am I" endpoint is not a broken source.
///
/// Plain serde data, like everything else crossing this SPI (§3a).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ConnectionInfo {
    /// Whom knobas is authenticated as, in the source's own spelling
    /// (`"mara.lindqvist"`), so *Test connection* can say **Connected as …**
    /// and the user can tell a wrong-account PAT from a working one.
    pub account: Option<String>,
    /// The remote product version, for the sources view and for the bug report
    /// that follows a dialect mismatch (`flavor: datacenter|cloud`).
    pub server_version: Option<String>,
    /// When the credential that just worked stops working, if the source will
    /// say. Feeds the PAT expiry countdown and `source_config.secret_expires_at`.
    pub secret_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Anything else worth putting on screen in one line.
    pub detail: Option<String>,
    /// Configuration values the adapter *learned* from the far end, keyed by
    /// the `config_schema` property they belong in.
    ///
    /// The Add-source dialog fills an empty field of that name with what is
    /// here, the way it already fills `username` from [`Self::account`] (#82).
    /// It is a map rather than a second named field per fact because the facts
    /// are per-adapter: Jira's is the Epic Link custom field id, whose value
    /// differs on every instance and which no user can be expected to type
    /// (#297), and the next adapter with a per-instance id of its own adds a
    /// key here rather than another field on this frozen struct.
    ///
    /// **A key is a promise about the adapter's own schema**, not about
    /// knobas': a key naming a property the adapter does not declare fills
    /// nothing, and an adapter that discovers nothing sends an empty map.
    /// Nothing here is a secret -- it crosses to the form and into
    /// `source_config.config`, which is Postgres (spec §14).
    #[serde(default)]
    pub discovered: std::collections::BTreeMap<String, String>,
}

/// Display metadata for one entity kind an adapter emits, and whether a full
/// sync of that kind is exhaustive.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KindInfo {
    /// Matches [`SyncItem::kind`], e.g. `"ticket"`.
    pub id: String,
    /// Singular label, e.g. `"Ticket"`.
    pub label: String,
    /// Plural label, e.g. `"Tickets"`.
    pub plural: String,
    /// Two-character monogram for the item chip, e.g. `"JI"`.
    pub monogram: String,
    /// Whether a `cursor: None` sync emits the **complete** current corpus
    /// *of this kind*.
    ///
    /// This is the precondition of the full-sync sweep (interfaces §4.1):
    /// after an exhaustive full sync, every row of this source **and this
    /// kind** whose `synced_at` predates the run is an item the source stopped
    /// returning, and the engine tombstones it -- which is the only way a hard
    /// delete upstream ever reaches knobas.
    ///
    /// `false` says the full sync of this kind is a *window*, not the world:
    /// TeamCity emits the newest N builds per configuration, so a sweep after
    /// it would tombstone the entire build history on every run. Vanished
    /// items of such a kind are never swept, and the mirror keeps what it last
    /// saw.
    ///
    /// **Per kind, not per source** (ADR-0003, ratified 2026-08-27). One
    /// adapter routinely walks some kinds exhaustively and budgets others:
    /// Gitea enumerates every repository and every branch, but bounds commits
    /// and pull requests with `commits_per_repo` / `prs_per_repo`. A single
    /// per-source flag forced one answer for all four -- `true` licensed the
    /// sweep to tombstone every commit past the cap on every full sync, and
    /// `false` left repo-row retirement and branch hard deletes inexpressible.
    ///
    /// A **budgeted kind is non-exhaustive by definition**; declaring
    /// otherwise is the defect class of Jira's `MAX_PAGES` and the 2026-08-25
    /// Gitea budget ruling.
    ///
    /// M2: mock all `true`, Jira `ticket` `true`, Gitea `repo`/`branch`
    /// `true` and `commit`/`pr` `false`, TeamCity both `false`. It is a claim
    /// the adapter makes about its own read path -- the battery cannot check
    /// it without knowing the remote corpus, so the adapter's own integration
    /// tests are what hold it honest.
    pub full_sync_exhaustive: bool,
}

/// What an adapter can do beyond plain syncing (spec §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Capability {
    /// The source can search **server-side**, on its own corpus.
    ///
    /// Reserved: the SPI has no `Source::search` yet, so nothing calls this in
    /// M1 and knobas' own launcher answers from the local index either way
    /// (§3a "Search does not know adapter names -- it knows `sync.item`").
    /// M1's read-only adapters therefore declare **no** capabilities at all;
    /// the alternative reading -- "syncs into the local index" -- would be
    /// true of every adapter ever written and would assert nothing.
    Search,
    /// Must be accompanied by a non-empty
    /// [`SourceDescriptor::write_ops`], which says *which* writes.
    Write,
    Webhooks,
    Import,
}

/// How an adapter authenticates against its remote system (spec §3).
///
/// The descriptor declares which of these it accepts; the secret itself is
/// stored in the OS keychain, never in the source's configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AuthMethod {
    UserPassword,
    Pat,
    ApiToken,
    OAuth,
}

/// One entity pushed across the SPI during a sync.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncItem {
    pub entity: knobas_core::entity::EntityRef,
    /// The *entity* kind, naming one of the descriptor's
    /// [`entity_kinds`](SourceDescriptor::entity_kinds):
    /// ticket|pr|build|page|commit|branch|repo|monitor|…
    pub kind: String,
    pub title: String,
    /// What FTS indexes.
    pub body_text: String,
    pub author: Option<String>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Raw source payload, kept for re-mapping.
    pub payload: serde_json::Value,
    /// Where a human reads this item in the source's own UI, if the adapter
    /// can say. The detail view's *Open in browser* renders from this and
    /// nothing else -- deriving a URL downstream would need the per-adapter
    /// table §3a forbids (interfaces §8 P5). Stored in `sync.item.web_url`.
    pub web_url: Option<String>,
    pub deleted: bool,
}

/// Opaque incremental-sync position, adapter-defined content.
pub type Cursor = String;

/// The failure classes knobas distinguishes; everything else an adapter hits
/// collapses into [`SourceError::Protocol`].
///
/// The distinction is user-visible: [`Unauthorized`](Self::Unauthorized) is
/// what makes the UI offer *re-authenticate* rather than shrug at a protocol
/// error, so an adapter must classify the same failure the same way whether it
/// surfaces from [`Source::test_connection`] or mid-[`sync`](Source::sync).
///
/// **A failure that came from a response also carries the status it came from**
/// (ADR-0004). The four fault classes are the *user's* axis and are unchanged
/// -- 401 and 403 are both `Unauthorized`, which is what puts *Re-enter* on
/// screen and what a Jira DC CAPTCHA lockout needs -- but the adapter's axis is
/// finer than that, and had no home: a 401 is a dead credential and ends the
/// run, a 403 is one object this token may not read, a 404 is one object that
/// is gone, a 409 from Gitea's `/commits` is an empty repository. Before this
/// the status was parsed back out of the `HTTP <status>: <body>` message
/// `knobas-http` had just built, and 401 and 403 could not be told apart at
/// all. [`status`](Self::status) is now the one way to ask.
///
/// Every variant carries at most a string and a status, so the error crosses a
/// process boundary as plain data (spec §3a). The serde form is structural, not
/// the [`Display`](std::fmt::Display) text thiserror generates:
/// `{"Unauthorized": {"status": 401}}`, `{"Unreachable": "<detail>"}`. Both
/// status fields are `#[serde(default)]`, so a peer that sends none -- an
/// out-of-process adapter built against an older SPI -- still decodes, as the
/// adapter that never learned about a status.
#[derive(Debug, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum SourceError {
    /// The credential was refused, or there is none to send.
    ///
    /// `status` is what the source answered -- `401` for a credential it
    /// rejected, `403` for a request it refused -- and `None` when the adapter
    /// raised this without asking anyone, which is the `missing_secret` state
    /// of interfaces §3.
    #[error("unauthorized")]
    Unauthorized {
        #[serde(default)]
        status: Option<u16>,
    },
    #[error("unreachable: {0}")]
    Unreachable(String),
    /// knobas could not make sense of what the source said, or the source said
    /// no with a status that is not about the credential.
    ///
    /// `status` is that status when the failure came from a response, and
    /// `None` for the adapter's own faults -- an undecodable body, a base URL
    /// that is not a URL, a configuration that cannot authenticate at all.
    #[error("protocol: {message}")]
    Protocol {
        #[serde(default)]
        status: Option<u16>,
        message: String,
    },
    /// The [`Sink`] rejected an item and the sync was abandoned. Raised by the
    /// sink, propagated -- never manufactured -- by the adapter.
    #[error("sink: {0}")]
    Sink(String),
}

impl SourceError {
    /// The HTTP status behind this failure, when it came from one.
    ///
    /// `None` for every fault an adapter raised on its own account, and that
    /// is load-bearing in both directions: a sink failure or a DNS failure
    /// that read as a 404 would be swallowed as a skipped repository, and a
    /// 401 that read as nothing would be believed as a refusal of one object
    /// rather than the death of the credential.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Unauthorized { status } | Self::Protocol { status, .. } => *status,
            Self::Unreachable(_) | Self::Sink(_) => None,
        }
    }

    /// The credential was refused and nobody was asked: a missing secret.
    #[must_use]
    pub fn unauthorized() -> Self {
        Self::Unauthorized { status: None }
    }

    /// A fault of the adapter's own, carrying no status.
    ///
    /// Everything that *did* come from a response is built by
    /// `knobas_http::status_error`, which is the only place a status is
    /// attached -- so an adapter cannot accidentally claim one.
    #[must_use]
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol {
            status: None,
            message: message.into(),
        }
    }
}

/// A write knobas asks an adapter to perform on the remote system.
///
/// Each variant has a stable snake_case identifier that adapters list in
/// [`SourceDescriptor::write_ops`] and the UI renders its action bar from:
///
/// | variant | identifier | ratified for |
/// | --- | --- | --- |
/// | [`Comment`](Self::Comment) | `"comment"` | M1 (Jira, Gitea) |
/// | [`Transition`](Self::Transition) | `"transition"` | M2 (Jira) |
/// | [`CreateTicket`](Self::CreateTicket) | `"create_ticket"` | M2 (Jira) |
/// | [`CreateBranch`](Self::CreateBranch) | `"create_branch"` | M2 (Gitea) |
/// | [`CreatePullRequest`](Self::CreatePullRequest) | `"create_pull_request"` | M2 (Gitea) |
/// | [`Approve`](Self::Approve) | `"approve"` | M2 (Gitea) |
/// | [`TriggerBuild`](Self::TriggerBuild) | `"trigger_build"` | M2 (TeamCity) |
/// | [`RerunBuild`](Self::RerunBuild) | `"rerun_build"` | M2 (TeamCity) |
/// | [`LogWork`](Self::LogWork) | `"log_work"` | M3 (Jira) |
///
/// The enum grows per milestone and **each growth is a §10.8 ratified
/// exception** (ADR-0006); an adapter must reject every op it does not declare
/// with [`SourceError::Protocol`].
///
/// **One variant per operation, never per adapter** (ADR-0006). Two sources
/// that do the same conceptual thing share a variant, and the adapter-specific
/// spelling lives behind the adapter's own `write`. That is why `Comment` says
/// nothing about issues or pull requests, and why `CreateTicket` is spelled in
/// knobas' vocabulary (`CONTEXT.md`: a ticket) rather than in Jira's.
///
/// ## Every variant carries `entity`, and it is the same field everywhere
///
/// `entity` is an [`EntityRef`] in string form, and it is three things at once:
/// the **target** the adapter resolves to an API path, the **ordering key** the
/// write queue keeps per-entity order within, and the thing hold detection
/// snapshots. `knobas_sync::write_queue::target_entity` reads it out of every
/// variant with no wildcard arm, so a variant without one does not compile.
///
/// For an op that *creates* something, the entity is the **container** the new
/// thing goes into -- the Jira project, the Gitea repository -- addressed in
/// the source's own namespace (`jira:PAY`, `gitea:tidewater/payout-service`).
/// A container knobas does not mirror is still a legal target: the queue has no
/// foreign key on it, and hold detection reads "not in the mirror" as a fact
/// rather than an error.
///
/// [`EntityRef`]: knobas_core::entity::EntityRef
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum WriteOp {
    /// Identifier `"comment"`. Reply on a ticket or a pull request.
    Comment { entity: String, body: String },
    /// Identifier `"transition"`. Move a ticket to another status.
    ///
    /// `status` is the **status the user picked**, in the source's own
    /// spelling (`"In Progress"`), not a transition id: which transition
    /// reaches a status is workflow-dependent and per-instance, so resolving
    /// the name against what the source says is available *right now* is the
    /// adapter's job and is done on every write. An adapter that cannot reach
    /// `status` from where the ticket stands refuses with
    /// [`SourceError::Protocol`] naming what it could have reached.
    Transition { entity: String, status: String },
    /// Identifier `"create_ticket"`. `entity` is the **project** the ticket is
    /// created in (`jira:PAY`), which is a container knobas does not mirror.
    CreateTicket {
        entity: String,
        title: String,
        body: String,
        /// The source's own name for the kind of ticket (`"Task"`, `"Bug"`).
        /// Required: Jira refuses a create without one and there is no
        /// defensible default -- a project's issue types are configured.
        ticket_type: String,
    },
    /// Identifier `"create_branch"`. `entity` is the **repository**.
    CreateBranch {
        entity: String,
        name: String,
        /// What the branch starts from -- a branch name, a tag or a commit.
        from_ref: String,
    },
    /// Identifier `"create_pull_request"`. `entity` is the **repository**.
    CreatePullRequest {
        entity: String,
        title: String,
        body: String,
        /// The branch carrying the change.
        head: String,
        /// The branch it is proposed into.
        base: String,
    },
    /// Identifier `"approve"`. Sign off on a pull request.
    ///
    /// `body` may be empty -- an approval with no words is an approval.
    Approve { entity: String, body: String },
    /// Identifier `"trigger_build"`. `entity` is the **build configuration**.
    TriggerBuild { entity: String },
    /// Identifier `"rerun_build"`. `entity` is the **build** to run again; the
    /// adapter asks the source which configuration it belonged to.
    RerunBuild { entity: String },
    /// Identifier `"log_work"`. Log time against a ticket (M3.1, issue #280).
    ///
    /// `entity` is the **ticket** the time goes on. One op carries one
    /// worklog: `CONTEXT.md`'s **worklog** is what one or more of knobas'
    /// blocks *become* when logged, so the concatenation of a day's blocks
    /// into a single span happens on knobas' side and what crosses here is
    /// already the record the source will hold.
    ///
    /// `started` is the instant the logged span began, as
    /// [`DateTime<Utc>`](chrono::DateTime) -- an instant rather than a
    /// source-formatted string, because "what a worklog's `started` looks
    /// like on the wire" is the adapter's business and Jira's spelling of it
    /// (`yyyy-MM-dd'T'HH:mm:ss.SSSZ`, offset mandatory) is not a shape the
    /// SPI should be teaching every other source.
    ///
    /// `seconds` is how long was worked, which is **not** `ended - started`:
    /// the blocks a worklog covers may have gaps between them, and it is the
    /// worked time that is logged rather than the span it sits in.
    ///
    /// `comment` may be empty -- a worklog with no words is a worklog -- and
    /// an adapter sends it as the source's own comment field rather than
    /// inventing text for an empty one.
    ///
    /// **Nothing here says what to do with the remaining estimate.** Jira's
    /// `adjustEstimate` defaults to `auto` and that default is what knobas
    /// takes: an op that carried the choice would be asking every caller a
    /// question no surface in knobas puts to the user.
    LogWork {
        entity: String,
        started: chrono::DateTime<chrono::Utc>,
        seconds: i64,
        comment: String,
    },
}

impl WriteOp {
    /// The stable snake_case identifier adapters declare for this op in
    /// [`SourceDescriptor::write_ops`].
    ///
    /// Defined once, beside the enum, because everything that maps a variant
    /// to a name has to agree: the contract battery (which rejects a
    /// descriptor naming an op the SPI does not define), and every adapter
    /// deciding whether it supports the op it was handed. A per-crate copy of
    /// this table is how an adapter comes to reject `"comment"` while
    /// declaring it.
    ///
    /// **No wildcard arm, deliberately.** `WriteOp` is documented to grow per
    /// milestone, and a stale table does not fail quietly -- it falsely
    /// rejects the first adapter to declare the new identifier, with a message
    /// pointing at that adapter's descriptor instead of at this file. So the
    /// reminder is the compiler: adding a variant stops this module compiling
    /// until the variant is given an identifier here, and the battery's
    /// `known_write_ops` stops compiling until it is given a probe value.
    #[must_use]
    pub fn identifier(&self) -> &'static str {
        match self {
            WriteOp::Comment { .. } => "comment",
            WriteOp::Transition { .. } => "transition",
            WriteOp::CreateTicket { .. } => "create_ticket",
            WriteOp::CreateBranch { .. } => "create_branch",
            WriteOp::CreatePullRequest { .. } => "create_pull_request",
            WriteOp::Approve { .. } => "approve",
            WriteOp::TriggerBuild { .. } => "trigger_build",
            WriteOp::RerunBuild { .. } => "rerun_build",
            WriteOp::LogWork { .. } => "log_work",
        }
    }
}

/// What the source said about the write it just performed.
///
/// [`Source::write`] answered `()` until M3.1 (issue #280), and the reason it
/// no longer does is one write op rather than a general appetite for return
/// values: a **worklog** is a record the source assigns an id to, knobas keeps
/// a local copy of it, and the copy has to be able to name the remote row --
/// `start_work`'s look-before-write trick of finding the thing again by
/// reading the mirror cannot work here, because a worklog is not a mirrored
/// entity and two worklogs of the same length on the same day are
/// indistinguishable from outside.
///
/// A struct rather than `Option<String>` so the next thing a source has to say
/// about a write is a field rather than a second signature change, and so the
/// call site reads as what it is.
///
/// Delivery is still at-least-once (ADR-0012): a receipt is what the source
/// answered *this* time, and a re-sent write answers with a second id for a
/// second row. It is not an idempotency key and nothing treats it as one.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WriteReceipt {
    /// The id the source gave what this write created, in the source's own
    /// spelling (Jira's worklog ids are decimal strings). `None` when the op
    /// creates nothing addressable, or when the source did not say.
    pub remote_id: Option<String>,
}

impl WriteReceipt {
    /// The source had nothing to say -- the answer for every op but
    /// `log_work`.
    #[must_use]
    pub fn none() -> Self {
        Self { remote_id: None }
    }

    /// The source named what it made.
    #[must_use]
    pub fn id(remote_id: impl Into<String>) -> Self {
        Self {
            remote_id: Some(remote_id.into()),
        }
    }
}

/// The adapter interface. One implementation per configured source instance.
#[async_trait::async_trait]
pub trait Source: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;
    /// Reach the remote system with the configured credential and report what
    /// answered.
    ///
    /// Called when a source is added, when its secret is re-entered, and by
    /// the credential-health poll. The same fault classification as
    /// [`sync`](Source::sync) applies: 401/403 → [`SourceError::Unauthorized`],
    /// connect/DNS/TLS/timeout → [`SourceError::Unreachable`], anything else →
    /// [`SourceError::Protocol`] -- each carrying the status it came from
    /// ([`SourceError::status`]), which is what tells 401 from 403.
    async fn test_connection(&self) -> Result<ConnectionInfo, SourceError>;
    /// Push every item changed since `cursor` (None = full sync); return the new cursor.
    ///
    /// Sink failures are **not** the adapter's to swallow: propagate every
    /// [`Sink::item`] error with `?` and abandon the sync. A sink that has lost
    /// its database has no use for the remaining 4,988 items, and returning a
    /// fresh cursor after a partial write would silently skip everything the
    /// sink dropped.
    ///
    /// A sync that emitted **nothing** must return the cursor it was given,
    /// unchanged. The cursor is a position, not a timestamp of the attempt:
    /// the engine reads "same cursor, no items" as "nothing happened" and
    /// writes no activity line for it, so an adapter that stamps a fresh
    /// cursor onto an idle poll turns a five-minute schedule into 288 "synced
    /// nothing" log lines per source per day. [`contract::battery`] enforces
    /// it.
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError>;
    /// Perform a write on the remote system. Ops absent from
    /// [`SourceDescriptor::write_ops`] must be refused with
    /// [`SourceError::Protocol`] rather than attempted.
    ///
    /// The answer is a [`WriteReceipt`], which for almost every op is
    /// [`WriteReceipt::none`]: knobas addresses what it wrote by *reading it
    /// back*, and a receipt exists only for the writes where reading it back
    /// cannot identify the thing that was made. There is one today -- a
    /// worklog, whose id is what the local copy carries and what a later
    /// edit or delete would need (issue #280) -- and an adapter that has
    /// nothing to say answers `none` rather than inventing an id.
    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError>;
}

/// Where a syncing adapter hands its items. Implemented by the sync engine;
/// [`contract::VecSink`] is the in-memory one the battery uses.
#[async_trait::async_trait]
pub trait Sink {
    /// Accept one item, or fail the sync.
    ///
    /// Returning [`SourceError::Sink`] is how a sink applies back-pressure or
    /// aborts: the adapter must propagate it with `?` and stop syncing.
    async fn item(&mut self, item: SyncItem) -> Result<(), SourceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_descriptor() -> SourceDescriptor {
        SourceDescriptor {
            id: "jira".into(),
            adapter_kind: "jira".into(),
            name: "Jira".into(),
            capabilities: vec![Capability::Search, Capability::Write, Capability::Webhooks],
            adapter_version: "0.1.0".into(),
            auth_methods: vec![AuthMethod::Pat, AuthMethod::OAuth],
            write_ops: vec!["comment".into()],
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "JI".into(),
                full_sync_exhaustive: true,
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
            payload_paths: vec![KindPaths {
                kind: "ticket".into(),
                status_name: vec![PayloadPath::of(["fields", "status", "name"])],
                ..KindPaths::default()
            }],
        }
    }

    /// The descriptor is what the UI reads, so it has to survive the IPC hop
    /// as plain data -- including the kind metadata the launcher renders from,
    /// the auth methods the Add-source form offers, and the write-op
    /// identifiers the action bar renders instead of a hardcoded table.
    #[test]
    fn descriptor_serializes_to_plain_json() {
        let v = serde_json::to_value(a_descriptor()).unwrap();
        assert_eq!(
            v["capabilities"],
            serde_json::json!(["Search", "Write", "Webhooks"])
        );
        assert_eq!(v["auth_methods"], serde_json::json!(["Pat", "OAuth"]));
        assert_eq!(v["entity_kinds"][0]["monogram"], "JI");
        assert_eq!(v["write_ops"], serde_json::json!(["comment"]));
        assert_eq!(v["adapter_kind"], "jira");
        // The sweep's precondition rides on the *kind*, not on the source
        // (ADR-0003): one descriptor can carry an exhaustive kind beside a
        // budgeted one, and a flag at the top could not say that.
        assert_eq!(v["entity_kinds"][0]["full_sync_exhaustive"], true);
        assert!(
            v.get("full_sync_exhaustive").is_none(),
            "the per-source flag is gone; a descriptor that still carried one \
             would leave two answers to the same question"
        );
        assert_eq!(v["config_schema"]["type"], "object");
    }

    /// Spec §3a: every type crossing the SPI is plain serde data, so an adapter
    /// can later run out of process. That is only true if the types survive a
    /// round trip in *both* directions -- an out-of-process adapter sends its
    /// descriptor and its items, and reports its errors, across the boundary.
    #[test]
    fn spi_types_round_trip_in_both_directions() {
        let d = a_descriptor();
        let back: SourceDescriptor =
            serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back.id, d.id);
        assert_eq!(back.adapter_kind, d.adapter_kind);
        assert_eq!(back.capabilities, d.capabilities);
        assert_eq!(back.auth_methods, d.auth_methods);
        assert_eq!(back.entity_kinds[0].plural, "Tickets");
        assert_eq!(back.write_ops, d.write_ops);
        // The sweep's precondition travels with the kind: a flag that did not
        // survive the hop would default to "sweep it" downstream.
        assert_eq!(
            back.entity_kinds[0].full_sync_exhaustive,
            d.entity_kinds[0].full_sync_exhaustive
        );
        assert_eq!(back.config_schema, d.config_schema);

        let item = SyncItem {
            entity: knobas_core::entity::EntityRef::new("jira", "PAY-231"),
            kind: "ticket".into(),
            title: "SEPA payout fails".into(),
            body_text: "the batch job times out".into(),
            author: Some("bjoern".into()),
            updated_at: Some(
                chrono::DateTime::parse_from_rfc3339("2026-08-24T09:15:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
            payload: serde_json::json!({ "fields": { "status": "In Progress" } }),
            web_url: Some("https://jira.example/browse/PAY-231".into()),
            deleted: false,
        };
        let json = serde_json::to_value(&item).unwrap();
        // The entity keeps EntityRef's string form, not a nested object.
        assert_eq!(json["entity"], "jira:PAY-231");
        let back: SyncItem = serde_json::from_value(json).unwrap();
        assert_eq!(back.entity, item.entity);
        assert_eq!(back.kind, item.kind);
        assert_eq!(back.title, item.title);
        assert_eq!(back.body_text, item.body_text);
        assert_eq!(back.author, item.author);
        assert_eq!(back.updated_at, item.updated_at);
        assert_eq!(back.payload, item.payload);
        // *Open in browser* renders from this alone, so it has to survive the
        // hop like everything else the detail view reads.
        assert_eq!(back.web_url, item.web_url);
        assert!(!back.deleted);

        // Errors travel structurally, not as their Display text.
        let json = serde_json::to_value(SourceError::Unreachable("refused".into())).unwrap();
        assert_eq!(json, serde_json::json!({ "Unreachable": "refused" }));
        let back: SourceError = serde_json::from_value(json).unwrap();
        assert!(matches!(back, SourceError::Unreachable(d) if d == "refused"));
        let json = serde_json::to_value(SourceError::Sink("pool closed".into())).unwrap();
        assert_eq!(json, serde_json::json!({ "Sink": "pool closed" }));
        let back: SourceError = serde_json::from_value(json).unwrap();
        assert_eq!(back.to_string(), "sink: pool closed");

        // ADR-0004: the status rides along, so an out-of-process adapter's
        // "this repository is not ours" does not arrive as "this token is
        // dead". It is the *only* thing that separates them -- both are
        // `Unauthorized` -- so a hop that dropped it would silently turn every
        // 403 into a fatal run.
        let json = serde_json::to_value(SourceError::Unauthorized { status: Some(403) }).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "Unauthorized": { "status": 403 } })
        );
        let back: SourceError = serde_json::from_value(json).unwrap();
        assert_eq!(back.status(), Some(403));
        assert_eq!(back.to_string(), "unauthorized");
        let json = serde_json::to_value(SourceError::Protocol {
            status: Some(409),
            message: "HTTP 409 Conflict: Git Repository is empty.".to_owned(),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "Protocol": {
                    "status": 409,
                    "message": "HTTP 409 Conflict: Git Repository is empty."
                }
            })
        );
        let back: SourceError = serde_json::from_value(json).unwrap();
        assert_eq!(back.status(), Some(409));

        // Both status fields are `#[serde(default)]`, so a peer that sends
        // none -- an adapter built against an SPI that had no status -- still
        // decodes, as the fault it is with no status attached.
        let back: SourceError =
            serde_json::from_value(serde_json::json!({ "Unauthorized": {} })).unwrap();
        assert_eq!(back.status(), None);
        let back: SourceError =
            serde_json::from_value(serde_json::json!({ "Protocol": { "message": "boom" } }))
                .unwrap();
        assert_eq!(back.status(), None);
        assert_eq!(back.to_string(), "protocol: boom");
    }

    /// One probe per variant. The match below has **no wildcard arm**, so a
    /// new variant stops this module compiling until it is listed here -- the
    /// same device as [`WriteOp::identifier`], for the same reason.
    fn every_write_op() -> Vec<WriteOp> {
        let probes = vec![
            WriteOp::Comment {
                entity: "jira:PAY-231".into(),
                body: "on it".into(),
            },
            WriteOp::Transition {
                entity: "jira:PAY-231".into(),
                status: "In Progress".into(),
            },
            WriteOp::CreateTicket {
                entity: "jira:PAY".into(),
                title: "SEPA payout fails".into(),
                body: "the batch job times out".into(),
                ticket_type: "Bug".into(),
            },
            WriteOp::CreateBranch {
                entity: "gitea:tidewater/payout-service".into(),
                name: "feature/PAY-231-sepa-retry".into(),
                from_ref: "main".into(),
            },
            WriteOp::CreatePullRequest {
                entity: "gitea:tidewater/payout-service".into(),
                title: "Retry SEPA payouts".into(),
                body: "closes PAY-231".into(),
                head: "feature/PAY-231-sepa-retry".into(),
                base: "main".into(),
            },
            WriteOp::Approve {
                entity: "gitea:tidewater/payout-service#142".into(),
                body: "looks right".into(),
            },
            WriteOp::TriggerBuild {
                entity: "teamcity:buildType:Payout_Build".into(),
            },
            WriteOp::RerunBuild {
                entity: "teamcity:build:1187".into(),
            },
            WriteOp::LogWork {
                entity: "jira:PAY-231".into(),
                started: chrono::DateTime::from_timestamp(1_788_000_000, 0)
                    .expect("a fixed instant"),
                seconds: 2_700,
                comment: "SEPA retry".into(),
            },
        ];
        for op in &probes {
            match op {
                WriteOp::Comment { .. }
                | WriteOp::Transition { .. }
                | WriteOp::CreateTicket { .. }
                | WriteOp::CreateBranch { .. }
                | WriteOp::CreatePullRequest { .. }
                | WriteOp::Approve { .. }
                | WriteOp::TriggerBuild { .. }
                | WriteOp::RerunBuild { .. }
                | WriteOp::LogWork { .. } => {}
            }
        }
        probes
    }

    /// Write ops travel adapter-ward, so they round-trip in both directions --
    /// every variant, with every field, because a field dropped on the hop is
    /// a write that arrives at the adapter missing what it needed.
    #[test]
    fn write_op_round_trips() {
        for op in every_write_op() {
            let json = serde_json::to_string(&op).unwrap();
            let back: WriteOp = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_value(&back).unwrap(),
                serde_json::to_value(&op).unwrap(),
                "{op:?} did not survive the hop"
            );
        }

        let op = WriteOp::Comment {
            entity: "jira:PAY-231".into(),
            body: "on it".into(),
        };
        assert_eq!(
            serde_json::to_value(&op).unwrap(),
            serde_json::json!({ "Comment": { "entity": "jira:PAY-231", "body": "on it" } }),
            "the wire shape is externally tagged plain data (§3a), not Display text"
        );
    }

    /// The identifier is what a descriptor lists, what the queue stores in its
    /// `op` column and what hold detection dispatches on. Two variants sharing
    /// one would make a descriptor's declaration ambiguous and a queued row
    /// undecidable; a variant whose identifier is not snake_case would be a
    /// spelling no descriptor could guess.
    #[test]
    fn every_op_has_its_own_snake_case_identifier() {
        let mut seen = std::collections::HashSet::new();
        for op in every_write_op() {
            let id = op.identifier();
            assert!(
                id.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    && !id.is_empty(),
                "{id:?} is not a snake_case identifier"
            );
            assert!(seen.insert(id), "two variants both call themselves {id:?}");
        }
        assert_eq!(seen.len(), 9, "a variant lost its probe in every_write_op");
    }

    /// ADR-0004: a failure that came from a response carries the **status** it
    /// came from, so an adapter reads it instead of parsing it back out of the
    /// message `knobas-http` built. 401 and 403 stay the one fault class the
    /// user is asked to act on and are nonetheless told apart.
    #[test]
    fn a_fault_from_a_response_carries_the_status_it_came_from() {
        assert_eq!(
            SourceError::Unauthorized { status: Some(401) }.status(),
            Some(401)
        );
        assert_eq!(
            SourceError::Unauthorized { status: Some(403) }.status(),
            Some(403)
        );
        assert_eq!(
            SourceError::Protocol {
                status: Some(404),
                message: "HTTP 404 Not Found: gone".to_owned(),
            }
            .status(),
            Some(404)
        );
        // A fault the adapter raised without asking anyone carries none, and
        // nothing may invent one for it: a sink failure read as a 404 would be
        // swallowed as a skipped repository.
        assert_eq!(SourceError::unauthorized().status(), None);
        assert_eq!(
            SourceError::protocol("no authentication method is configured").status(),
            None
        );
        assert_eq!(
            SourceError::Unreachable("refused".to_owned()).status(),
            None
        );
        assert_eq!(SourceError::Sink("pool closed".to_owned()).status(), None);
    }
}
