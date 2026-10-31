//! The source SPI -- the one interface every knobas adapter implements.
//!
//! Jira, Gitea, TeamCity, Confluence, Uptime Kuma and everything added later
//! reach knobas through [`Source`]. The contract freezes at the M0 exit:
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

pub mod clone_urls;
pub mod code_pages;
pub mod contract;
pub mod diff;
pub mod docs;
pub mod fingerprint;
pub mod grants;
pub mod instance;
pub mod pipeline;
pub mod query;
pub mod role;
pub mod run;
pub mod url_forms;
pub mod view;
pub mod watermark;

/// The sync-interval floor an adapter declares unless it says otherwise, and
/// the one a descriptor without [`SourceDescriptor::min_sync_interval_secs`]
/// decodes with. A source is a remote system with a rate limit; letting a form
/// store `1` is how knobas gets an account blocked.
pub const DEFAULT_MIN_SYNC_INTERVAL_SECS: u32 = 60;

fn default_min_sync_interval_secs() -> u32 {
    DEFAULT_MIN_SYNC_INTERVAL_SECS
}

/// Everything knobas needs to know about a configured adapter instance: what
/// the Add-source form is generated from.
///
/// A *template* (one per adapter kind, what `list_adapters` answers) has
/// `id == adapter_kind`; a configured instance's descriptor carries the
/// instance id instead.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct SourceDescriptor {
    /// Instance id, e.g. `"jira"`. Doubles as the [`EntityRef`] namespace for
    /// every item this instance emits, so it must be non-blank and free of `:`.
    ///
    /// [`EntityRef`]: knobas_spi_types::entity::EntityRef
    pub id: String,
    /// Which adapter this is an instance of, e.g. `"jira"`, `"mock"`. Distinct
    /// from [`Self::entity_kinds`], which is about the *items* it emits.
    pub adapter_kind: String,
    /// Human-readable name for the source list.
    pub name: String,
    #[ts(as = "Vec<String>")]
    pub capabilities: Vec<Capability>,
    pub adapter_version: String,
    /// How this adapter can authenticate (spec §3, §3a). The Add-source form
    /// offers these; the chosen method's secret goes to the OS keychain and
    /// never into [`Self::config_schema`]'s config blob.
    pub auth_methods: Vec<AuthMethod>,
    /// Whether this adapter can use a **second** credential beside the one
    /// [`auth_methods`](Self::auth_methods) names: an optional username and
    /// password, stored in the same keychain item
    /// ([`instance::Account`], M4.1, issue #452).
    ///
    /// `false` for every adapter but Uptime Kuma, and it is a *declaration*
    /// rather than a fact the forms could work out: what the Add-source and
    /// re-enter forms need to know is whether to draw the optional account
    /// fields at all, and the alternative to declaring it is a
    /// `adapter_kind === "kuma"` in a Svelte component -- the per-adapter
    /// table §3a exists to forbid.
    ///
    /// **Not an [`AuthMethod`]**, for the reason `instance::Account` argues:
    /// the auth method is how a source authenticates its ordinary traffic,
    /// one per source, and offering the account as a rival method would let a
    /// reader pick it *instead of* the key.
    ///
    /// `#[serde(default)]`, so a descriptor built by a peer that predates this
    /// still decodes -- as the adapter with one credential it was.
    #[serde(default)]
    pub accepts_account: bool,
    /// The shortest sync interval, in seconds, a source of this adapter may
    /// be scheduled at: its **floor** (#1187). The source commands refuse an
    /// interval under it, and the settings UI offers nothing under it.
    ///
    /// A declaration for `accepts_account`'s reason: the alternative is a
    /// per-adapter table of rate limits in knobas core or in a Svelte
    /// component. Uptime Kuma declares 45 and TeamCity 20; every other
    /// adapter [`DEFAULT_MIN_SYNC_INTERVAL_SECS`].
    ///
    /// `#[serde(default)]` to that constant, so a descriptor built by a peer
    /// that predates this decodes with the floor every source had before it.
    #[serde(default = "default_min_sync_interval_secs")]
    pub min_sync_interval_secs: u32,
    /// Which [`WriteOp`]s this adapter supports, as the stable snake_case
    /// identifiers documented on that enum (`Comment` → `"comment"`).
    ///
    /// The UI renders its action bar from this rather than from a hardcoded
    /// per-adapter table, which is why [`Capability::Write`] alone is not
    /// enough: once `WriteOp` grows, "supports writes" no longer says *which*
    /// actions to offer. An adapter must reject any op absent from this list
    /// with [`SourceError::Protocol`], and the write queue and the WASM host
    /// refuse one before it is handed over ([`Self::declares`], #1950).
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
    /// generated from it (spec §3a), never hand-built per adapter. Never
    /// holds secrets -- those live in the OS keychain, keyed by the chosen
    /// [`AuthMethod`]. M0: the mock declares an empty object schema.
    #[ts(type = "unknown")]
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
    /// [`KindPaths::kind`]: knobas_spi_types::payload::KindPaths::kind
    #[serde(default)]
    pub payload_paths: Vec<knobas_spi_types::payload::KindPaths>,
    /// The [roles](role) this adapter claims (ADR-0029, issue #1193): which
    /// of the shell's surfaces it fills. Each claim is held to its role's
    /// [`role::Minimum`] by [`role::check`], which [`contract::battery`]
    /// runs as clause 10: the kinds it names are in [`Self::entity_kinds`],
    /// the paths it needs are in [`Self::payload_paths`], and the reads it
    /// needs are in the claim. A role's writes are declared, one by one, in
    /// [`Self::write_ops`] as every write is. A surface reads the claim,
    /// never the adapter kind.
    ///
    /// `#[serde(default)]`, so a descriptor from a peer that predates this
    /// decodes as an adapter that claims nothing.
    #[serde(default)]
    pub roles: Vec<role::RoleClaim>,
    /// The [source actions](WriteOp::SourceAction) this adapter performs, by
    /// its own id (`"add_label"`; ADR-0029, issue #1194): writes outside
    /// every role's vocabulary. Non-empty exactly when [`Self::write_ops`]
    /// lists [`SOURCE_ACTION`], and an adapter refuses a source action whose
    /// id is not here with [`SourceError::Protocol`]; [`contract::battery`]
    /// holds both (clause 11). Each action's dialog is drawn by the view that
    /// offers it, in ADR-0020's action vocabulary, as every action's is: its
    /// views offer them as `source_action` actions, and the shell never reads
    /// this list.
    ///
    /// `#[serde(default)]`, so a descriptor from a peer that predates this
    /// decodes as an adapter with none.
    #[serde(default)]
    pub source_actions: Vec<String>,
    /// Which of an instance's typed addresses a repository is cloned from,
    /// in order of preference: the code-host read `clone_urls` (#1197,
    /// [`clone_urls::CloneFrom`]). *Clone* appends the checked
    /// `owner/repo.git` to the first that ADR-0023's rules admit. Non-empty
    /// exactly when a [`role::CODE_HOST`] claim reads `clone_urls`, and a
    /// config key named here is a string property of
    /// [`Self::config_schema`]; [`role::check`] holds both (clause 10).
    /// *Clone* reads it on the Rust side; the shell never does.
    ///
    /// `#[serde(default)]`, so a descriptor from a peer that predates this
    /// decodes as an adapter that names no address to clone from.
    #[serde(default)]
    pub clone_from: Vec<clone_urls::CloneFrom>,
    /// The other forms of address the source serves an item's page under,
    /// beside the one it reports as [`SyncItem::web_url`] (#1475, spec #491's
    /// *"a descriptor slot with its own entry"*). The paste resolver reads a
    /// pasted address as each form and opens the item whose payload holds
    /// the values it names; it never fetches, and it never knows which
    /// adapter declared what ([`url_forms`] says how a form reads).
    /// [`url_forms::check`] holds each to its rules (contract battery clause
    /// 12).
    ///
    /// `#[serde(default)]`, so a descriptor from a peer that predates this
    /// -- an external adapter's JSON among them -- decodes as an adapter
    /// whose items open by their stored address alone.
    #[serde(default)]
    pub url_forms: Vec<url_forms::UrlForm>,
    /// Where a code host serves the page of a repository, a file, a commit,
    /// a range and a pull request, after the instance's typed base URL
    /// (#2274, [`code_pages`]): what *Open in › Browser* and the `{url}`
    /// placeholder open. [`code_pages::check`] holds it to its rules
    /// (contract battery clause 14). Read on the Rust side; the shell never
    /// does.
    ///
    /// `#[serde(default)]`, so a descriptor from a peer that predates this
    /// decodes as a source with no code pages: *Browser* is then not offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub code_pages: Option<code_pages::CodePages>,
}

impl SourceDescriptor {
    /// Whether this source reads one item by id: it declares
    /// [`Capability::Refresh`] (ADR-0031), for some kind. Answered off the
    /// descriptor, so a caller deciding between a refresh and the run alone
    /// after a landed write calls nothing; one deciding between *Refresh* and
    /// *Sync now* for an item asks [`Self::refreshes_kind`] (#2146).
    #[must_use]
    pub fn refreshes(&self) -> bool {
        self.capabilities.contains(&Capability::Refresh)
    }

    /// Whether this source reads an item of `kind` by id (#2146): it
    /// declares [`Capability::Refresh`] and its
    /// [`entity_kinds`](Self::entity_kinds) entry for `kind` says
    /// [`KindInfo::refreshes`]. `false` for a kind it does not declare.
    /// What an item's detail reads to draw *Refresh* or *Sync now*.
    #[must_use]
    pub fn refreshes_kind(&self, kind: &str) -> bool {
        self.refreshes()
            && self
                .entity_kinds
                .iter()
                .any(|info| info.id == kind && info.refreshes)
    }

    /// Does this source declare `op`? `Ok` when [`Self::write_ops`] lists its
    /// [identifier](WriteOp::identifier) and, for a
    /// [source action](WriteOp::SourceAction), [`Self::source_actions`] lists
    /// its id. Otherwise the [`SourceError::Protocol`] that refuses it, naming
    /// the source and the op.
    ///
    /// **Clause 5 held by the caller, not only by the adapter** (#1950). The
    /// write queue's flush asks this of the built source's own descriptor
    /// before it hands any op over, and the WASM host asks it before a call
    /// crosses into a guest, so an op the instance does not declare is
    /// refused there with nothing sent, whatever the adapter would have done
    /// with it. Every compiled-in adapter still refuses one itself, and
    /// [`contract::battery`] still holds it to that: the check here is the
    /// rule in one place, theirs is the defence behind it. The flush words
    /// the refusal by [`Source::undeclared_reason`] where the adapter has a
    /// more specific one.
    ///
    /// # Errors
    ///
    /// [`SourceError::Protocol`] when the op, or the source action's id, is
    /// not declared.
    pub fn declares(&self, op: &WriteOp) -> Result<(), SourceError> {
        let id = op.identifier();
        if !self.write_ops.iter().any(|declared| declared == id) {
            return Err(SourceError::protocol(format!(
                "source {:?} does not declare the write {id:?}, so it was not sent",
                self.id
            )));
        }
        if let WriteOp::SourceAction { id, .. } = op
            && !self.source_actions.iter().any(|declared| declared == id)
        {
            return Err(SourceError::protocol(format!(
                "source {:?} does not declare the source action {id:?}, so it was not sent",
                self.id
            )));
        }
        Ok(())
    }
}

// The declaration types themselves live in `knobas-spi-types`, not here: the
// adapters that write them depend on this crate, and the readers that resolve
// them are in `knobas-core`, which may not depend on the SPI. Neither
// `knobas-spi-types` nor this crate needs a database, so both build for
// `wasm32-wasip2` (#1234). Re-exported so an adapter needs one `use`.
pub use knobas_spi_types::delivery::Delivery;
pub use knobas_spi_types::entity::EntityRef;
pub use knobas_spi_types::payload::{
    Frame, KindPaths, ListPath, ParentPath, PayloadPath, ProjectPath, StateChip, StatePath,
    StateValue, Trigger, TriggerPath, TriggerValue,
};
pub use knobas_spi_types::tone::Tone;

/// What a successful [`Source::test_connection`] learned about the far end.
///
/// Every field is optional and an adapter fills only what its API actually
/// exposes: the Add-source flow renders what it got and says nothing about
/// what it did not (§3 "Credential health: credential expiry countdown").
/// A source whose API has no "who am I" endpoint is not a broken source.
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
    /// say. Feeds the credential expiry countdown and
    /// `source_config.secret_expires_at`. No built-in adapter fills it (#1513):
    /// none of their sources says when *the credential in use* expires.
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
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
    /// Whether [`Source::refresh`] reads an item **of this kind** by id
    /// (#2146, ADR-0031). Read only under [`Capability::Refresh`], through
    /// [`SourceDescriptor::refreshes_kind`]: the capability says the adapter
    /// reads one item at all, this says which of its kinds. `false` for a
    /// kind its refresh answers [`RefreshAnswer::NotOffered`] for, such as
    /// TeamCity's build configuration or Gitea's repository, branch and
    /// commit, so their detail draws *Sync now* from the start rather than
    /// *Refresh* until the first press. [`contract::battery`] clause 13
    /// holds it to the answers, kind by kind.
    ///
    /// **Per kind, not per adapter**, as [`Self::full_sync_exhaustive`] is.
    ///
    /// `#[serde(default = ...)]` to `true`, so a descriptor that predates
    /// this (an external adapter's JSON) reads as one whose every kind is
    /// refreshed when it declares the capability, which is what it was read
    /// as before. Left out of the TypeScript mirror: the shell reads
    /// `EntityDetail.refreshes`, never an adapter's kinds.
    #[serde(default = "every_kind_refreshes")]
    #[ts(skip)]
    pub refreshes: bool,
}

/// [`KindInfo::refreshes`]' default: a kind is refreshed unless its
/// descriptor says otherwise.
fn every_kind_refreshes() -> bool {
    true
}

/// What an adapter can do beyond plain syncing (spec §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Capability {
    /// The source can search **server-side**, on its own corpus.
    ///
    /// Reserved: the SPI has no `Source::search` yet, so nothing calls this in
    /// M1 and knobas' own launcher answers from the mirror either way (§3a
    /// "Search does not know adapter names -- it knows `sync.item`"). M1's
    /// read-only adapters therefore declare **no** capabilities at all; the
    /// alternative reading -- "syncs into the mirror" -- would be true of
    /// every adapter ever written and would assert nothing.
    Search,
    /// Must be accompanied by a non-empty
    /// [`SourceDescriptor::write_ops`], which says *which* writes.
    Write,
    Webhooks,
    Import,
    /// The source reads one item by id ([`Source::refresh`], ADR-0031):
    /// what a landed write is followed by before its run, and what an item's
    /// detail offers as *Refresh*. Declared exactly when `refresh` answers
    /// for some kind; [`contract::battery`] holds the two to each other, as
    /// it holds `Write` to `write_ops`. Read through
    /// [`SourceDescriptor::refreshes`], so whether a source can refresh is
    /// known without calling it.
    Refresh,
}

/// How an adapter authenticates against its remote system (spec §3).
///
/// The descriptor declares which of these it accepts; the secret itself is
/// stored in the OS keychain, never in the source's configuration. Spelled in
/// PascalCase on the wire, unchanged since M0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub enum AuthMethod {
    UserPassword,
    Pat,
    ApiToken,
    OAuth,
}

/// One entity pushed across the SPI during a sync.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncItem {
    pub entity: knobas_spi_types::entity::EntityRef,
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
/// Every variant carries at most a string and a status (`Moved` two strings),
/// so the error crosses a process boundary as plain data (spec §3a). The serde
/// form is structural, not the [`Display`](std::fmt::Display) text thiserror
/// generates:
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
    /// The source did not answer, and **nothing it did depends on that**: a
    /// read that can be asked again, a write the transport sends again
    /// itself, or a once-sent write whose connection never opened.
    #[error("unreachable: {0}")]
    Unreachable(String),
    /// A write that may be applied only once reached the source, and no answer
    /// it can read says whether the source acted (issue #1125): it timed out
    /// after the request went out, a gateway answered `502`/`504` for it, the
    /// server read it and closed the connection without answering (#1821), or
    /// (Kuma) an acknowledgement arrived in a shape knobas cannot read.
    ///
    /// Raised by `knobas_http` for a request sent with
    /// `Request::no_retry`, which is how an adapter says a write must not
    /// arrive twice, and for any request but a `GET` or `HEAD` the server
    /// hung up on that way, since it is never sent again (#1821). Everywhere but the write queue it reads as
    /// [`Unreachable`](Self::Unreachable). An adapter whose write takes more
    /// than one request turns it back into `Unreachable` when the request
    /// that failed came before the write's own (Kuma's login,
    /// `not_yet_emitted`), and raises it itself for a failure after the
    /// write's own was accepted and before its answer was read (Kuma's ack
    /// poll and deadline, `once_emitted`, #1627), and for an ack that arrived
    /// but is not a socket.io ack knobas can read or carries no boolean `ok`
    /// (Kuma's `verdict`, #1721). The queue does not send such a write again
    /// by itself: it holds it, so a person checks the source and sends it
    /// again or discards it (`knobas_sync::write_queue`). The exception is a
    /// write whose delivery says a second arrival is harmless (Kuma's pause
    /// and resume): that one waits as `Unreachable` and goes again on the
    /// backoff ladder.
    ///
    /// `status` is the gateway's, or the one Kuma's lost ack poll got
    /// (`once_emitted` keeps it), and `None` for a timeout or an ack knobas
    /// cannot read.
    #[error("unconfirmed: {message}")]
    Unconfirmed {
        #[serde(default)]
        status: Option<u16>,
        message: String,
    },
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
    /// The write was made against a version of its target the source no
    /// longer holds, so the source did not apply it (issue #822).
    ///
    /// `base` is the version the source holds **now**, in its own spelling
    /// (Gitea's blob `sha` of a file), and `current` the target's text at
    /// that version: what a reader compares their own against before they
    /// choose. The write queue holds the write with both rather than
    /// refusing it, and *Apply anyway* re-sends it against `base`
    /// (`knobas_core::write_queue::apply_anyway`). Never retried on its own,
    /// and not a fault of the credential or the network.
    ///
    /// Two strings and no status, so it crosses a process boundary as plain
    /// data like the others: the adapter decides it from the version it read,
    /// not from a status a server spelt.
    #[error("moved: the target changed since the write was made against it")]
    Moved { base: String, current: String },
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
            Self::Unauthorized { status }
            | Self::Protocol { status, .. }
            | Self::Unconfirmed { status, .. } => *status,
            Self::Unreachable(_) | Self::Sink(_) | Self::Moved { .. } => None,
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
/// | [`MergePullRequest`](Self::MergePullRequest) | `"merge_pull_request"` | source views (Gitea, #667) |
/// | [`SetParameter`](Self::SetParameter) | `"set_parameter"` | source views (TeamCity, #668) |
/// | [`RunBuild`](Self::RunBuild) | `"run_build"` | source views (TeamCity, #669) |
/// | [`SubmitReview`](Self::SubmitReview) | `"submit_review"` | git workflows, spec #701 (Gitea, #705) |
/// | [`DeleteBranch`](Self::DeleteBranch) | `"delete_branch"` | source views (Gitea, #707) |
/// | [`EditPullRequest`](Self::EditPullRequest) | `"edit_pull_request"` | git workflows, spec #701 (Gitea, #706) |
/// | [`UpdatePullRequestBranch`](Self::UpdatePullRequestBranch) | `"update_pull_request_branch"` | git workflows, spec #701 (Gitea, #708) |
/// | [`CommitFile`](Self::CommitFile) | `"commit_file"` | ticket-first, spec #808 (Gitea, #822) |
/// | [`SourceAction`](Self::SourceAction) | `"source_action"` | roles, spec #1192 (Gitea, #1194) |
/// | [`ReplyToThread`](Self::ReplyToThread) | `"reply_to_thread"` | code area, spec #2234 (Gitea, #2277) |
/// | [`ResolveThread`](Self::ResolveThread) | `"resolve_thread"` | code area, spec #2234 (Gitea, #2277) |
/// | [`RequestReviewers`](Self::RequestReviewers) | `"request_reviewers"` | code area, spec #2234 (Gitea, #2303) |
///
/// The enum grows per milestone and **each growth is a §10.8 ratified
/// exception** (ADR-0006); an adapter must reject every op it does not declare
/// with [`SourceError::Protocol`].
///
/// On the wire it is externally tagged, so the variant name is the object's
/// one key, and a frontend written today will meet a payload it does not
/// recognise. It handles that by narrowing on the key rather than assuming
/// `Comment`: an unrecognised op can still be applied and discarded, and only
/// *editing* needs to know where the words are. What each variant means is
/// in the table above and on the variant; what the shell needs of its payload
/// is on the payload's fields, because the generated TypeScript
/// (`app/src/lib/ipc/generated/sources.ts`) carries a field's doc but not a
/// variant's (#1325).
///
/// **One variant per operation, never per adapter** (ADR-0006). Two sources
/// that do the same conceptual thing share a variant, and the adapter-specific
/// spelling lives behind the adapter's own `write`. That is why `Comment` says
/// nothing about issues or pull requests, and why `CreateTicket` is spelled in
/// knobas' vocabulary (`CONTEXT.md`: a ticket) rather than in Jira's.
///
/// ## Every variant names its target, almost always as `entity`
///
/// The target is an [`EntityRef`] in string form, and it is three things at
/// once: what the adapter resolves to an API path, the **ordering key** the
/// write queue keeps per-entity order within, and the thing hold detection
/// snapshots. [`WriteOp::delivery`] says which field carries it and
/// [`WriteOp::target`] reads that field; `CreatePage`'s `parent` and
/// `CommitFile`'s `repo` are the two that are not `entity`.
///
/// For an op that *creates* something, the entity is the **container** the new
/// thing goes into -- the Jira project, the Gitea repository -- addressed in
/// the source's own namespace (`jira:PAY`, `gitea:tidewater/payout-service`).
/// A container knobas does not mirror is still a legal target: the queue has no
/// foreign key on it, and hold detection reads "not in the mirror" as a fact
/// rather than an error.
///
/// [`EntityRef`]: knobas_spi_types::entity::EntityRef
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts", rename = "WriteOpPayload")]
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
        /// RFC 3339, UTC -- when the logged span began.
        started: chrono::DateTime<chrono::Utc>,
        /// How long was **worked**, which is not the span's length.
        seconds: i64,
        comment: String,
    },
    /// Identifier `"create_page"`. Make a new wiki page (M3.2, issue #286).
    ///
    /// `parent` rather than `entity`, and it is the **only** variant that
    /// spells its target under another name. The reason is that this op has
    /// two containers and they are not interchangeable: a page is created
    /// *inside a space* and *under a parent page*, and Confluence's create
    /// wants both. Calling the parent `entity` and leaving `space` beside it
    /// would read as though the space were incidental; naming the field for
    /// what it is says which of the two the queue orders and holds against.
    /// Its delivery names `parent` as the target ([`WriteOp::target`]).
    ///
    /// `parent` is an [`EntityRef`] in string form naming a **mirrored page**
    /// (`confluence:98400`) -- unlike a create's container elsewhere in this
    /// enum, which is typically something knobas does not mirror. That is not
    /// a stricter rule imposed here, it is what Confluence's own model gives:
    /// every page has a parent, and the one knobas creates under is a page a
    /// reader picked out of the mirror.
    ///
    /// `space` is the space **key** in the source's own spelling (`"ENG"`),
    /// not an `EntityRef`: ADR-0010 makes a space Confluence's *project*, and
    /// knobas does not mirror projects as entities in any source.
    ///
    /// `body` is **storage format**, the dialect the page is stored in and the
    /// one `body.storage` gives back. Not knobas-flavoured markup and not
    /// HTML: what goes out is what a later read must return unchanged, and a
    /// translation layer here would make the round trip lossy.
    ///
    /// [`EntityRef`]: knobas_spi_types::entity::EntityRef
    CreatePage {
        /// The **parent page**, as an entity id -- this op's queue target.
        parent: String,
        /// The space key in Confluence's own spelling (`ENG`), not an id.
        space: String,
        title: String,
        /// Storage format, not text: what a later read must return unchanged.
        body: String,
    },
    /// Identifier `"update_page"`. Replace a wiki page's body (M3.2, issue
    /// #286).
    ///
    /// `entity` is the **page**, and `body` is its **whole** storage format --
    /// never the fragment the reader edited. Confluence's content `PUT`
    /// replaces the record, so a request carrying one section would delete the
    /// rest of the page; re-assembling the whole body around the edited
    /// section is the caller's job and `DocWrites::edit_section`
    /// (`knobas-source-confluence`, `blocks.rs`) is where it happens.
    ///
    /// `base_version` is the version number the edit was made **against** --
    /// `version.number` as the mirror held it when the reader started typing.
    /// It is not the version to write: the adapter sends `base_version + 1`,
    /// which is how Confluence is asked to abort when somebody else got there
    /// first. That abort is the **backstop** and not the mechanism: the write
    /// queue holds this op when the mirror moved on after it was queued
    /// (`knobas_core::write_queue::project`), and Confluence's own conflict
    /// answer catches the window the mirror could not see.
    UpdatePage {
        entity: String,
        /// The version the edit was made **against**, not the one to write:
        /// the adapter sends `base_version + 1` and Confluence aborts if
        /// somebody else got there first (#286).
        base_version: i64,
        /// The page's **whole** storage format. Confluence's content `PUT`
        /// replaces the record, so a body carrying only the edited section
        /// would delete the rest of the page -- `DocWrites::edit_section`
        /// is what composes this.
        body: String,
    },
    /// Identifier `"pause_monitor"`. Stop a monitor checking (M4.1, issue
    /// #452). `entity` is the **monitor** (`kuma:8`).
    ///
    /// No second field, and the absence is the decision: Uptime Kuma's
    /// `pauseMonitor` takes a monitor id and nothing else -- no duration, no
    /// reason, no note. A `until` or `reason` here would be a field knobas
    /// asks a reader for and then throws away, and a *silence for an hour*
    /// that knobas kept locally would be a scheduler this milestone does not
    /// have (spec #427, *Out of scope*).
    PauseMonitor {
        /// The **monitor** (`kuma:8`). No second field: Uptime Kuma's
        /// `pauseMonitor` takes a monitor id and nothing else, so a duration
        /// or a reason here would be something knobas asked a reader for and
        /// threw away.
        entity: String,
    },
    /// Identifier `"resume_monitor"`. Start a paused monitor checking again
    /// (M4.1, issue #452). `entity` is the **monitor**.
    ///
    /// The pair of [`PauseMonitor`](WriteOp::PauseMonitor) and not one op with
    /// a flag: an action bar renders one button per identifier, a queue row
    /// records which act is owed in its `op` column, and *pause* and *resume*
    /// are two acts a reader chooses between rather than one act with an
    /// argument. It is also what lets a descriptor offer one without the
    /// other, which is what a source with a read-only account would declare.
    ResumeMonitor {
        /// The paused **monitor**, to start checking again.
        entity: String,
    },
    /// Identifier `"create_monitor"`. Ask a monitoring source to start
    /// watching a URL (M4.1, issue #453).
    ///
    /// `entity` is the container the new thing goes into, like every other
    /// create in this enum, and for this op that container is **the source
    /// itself**: a monitoring system has nothing above a monitor that knobas
    /// mirrors -- Uptime Kuma's only one is a monitor group, and spec #427
    /// puts groups out of scope. [`monitor_target`] spells it, and no call
    /// site invents it. Like a create's container elsewhere here (`jira:PAY`)
    /// it is one knobas does not mirror, so it projects `{"live": false}` at
    /// queue time and again at flush time, holds nothing, and sends.
    ///
    /// **Deliberately not called a *roster***: `CONTEXT.md` gives that word to
    /// the Monitors tab's list of rows (`assets::monitor_roster`), and this is
    /// an address rather than a list.
    ///
    /// **What it checks is `url`, spelled as its [`kind`](MonitorKind) reads
    /// it** (#864). An HTTP check takes an http(s) URL, a TCP check `host:port`,
    /// a ping a host, and a Docker check a container's name. Until #864 a
    /// monitor of a URL was the only check there was (#453 said the day knobas
    /// offers a ping or a TCP check is the day this op grows the field that
    /// says which), so the field keeps its name: a queued row written then
    /// still reads, and `kind` defaults to [`MonitorKind::Http`], which is what
    /// that row meant.
    ///
    /// **`name` is what attaches it.** The new monitor reaches knobas the
    /// ordinary way, through the next poll, and what draws the `monitored-by`
    /// link is the name: `knobas_sync::attach` resolves the names an asset
    /// carries against the monitors the mirror holds, which is the rule the
    /// estate import already uses (issue #439). So the name is not decoration
    /// -- it is the address the attachment is made through, which is why the
    /// caller records it on the asset *before* queueing the write.
    CreateMonitor {
        /// What the create targets: the source that will make it, as
        /// `assets::MonitorTarget.entity` gives it. Never composed by a
        /// caller (#453).
        entity: String,
        /// What the monitor is called, and therefore what attaches it: the
        /// next poll mirrors it, and `knobas_sync::attach` draws the
        /// `monitored-by` link to every asset carrying this name.
        name: String,
        /// What it checks, spelled as `kind` reads it: an http(s) URL,
        /// `host:port`, a host, or a container's name (#864).
        url: String,
        /// The kind of check; absent is `http`, what every create meant
        /// before #864.
        #[serde(default)]
        #[ts(as = "Option<MonitorKind>", optional)]
        kind: MonitorKind,
    },
    /// Identifier `"merge_pull_request"`. Merge a pull request into its base
    /// (spec #660 story 10, issue #667). `entity` is the **pull request**.
    ///
    /// `style` is how, in knobas' words rather than one server's: Gitea calls
    /// the field `Do` and spells the same three values, and a GitLab adapter
    /// would map them onto its own merge-method switches. A closed set, so a
    /// payload naming a style no adapter can perform fails to decode at
    /// `submit_write` rather than reaching a source.
    ///
    /// Whether the pull request can be merged *right now* is the source's
    /// answer and not the op's: the view offers the action only where the
    /// source said mergeable, and a merge that became impossible between the
    /// offer and the flush comes back refused with the source's own sentence
    /// (ADR-0004). Hold detection takes the whole mirrored record, as for
    /// [`Approve`](WriteOp::Approve): a merge is a judgement on one state of
    /// the pull request, and a push after it was queued is a reason to look
    /// again.
    ///
    /// `delete_branch` asks the source to delete the head branch once the
    /// merge has landed (spec #701 workflow 9, issue #708), in the same
    /// request: Gitea's `delete_branch_after_merge`. It defaults to `false`,
    /// so a merge queued before the field existed still reads, as the merge
    /// it was then: one that keeps the branch. A branch the source will not
    /// delete (protected, in a fork the account cannot write) is the source's
    /// to skip; the merge itself still lands.
    ///
    /// `message` is the commit message the reader wrote for the merge (#843,
    /// R-8.19: *Squash into one commit* with an editable message): its first
    /// line the title, the rest the body. `None` leaves the source's own
    /// default, which is what every merge queued before the field existed
    /// meant; absent on the wire when `None`, so such a merge reads and
    /// writes the shape it had.
    MergePullRequest {
        /// The **pull request** (#667).
        entity: String,
        /// How the merge lands; a word outside the three does not decode.
        style: MergeStyle,
        /// Delete the head branch once merged (#708); absent reads as
        /// `false`.
        #[serde(default)]
        delete_branch: bool,
        /// The commit message the reader wrote (#843): first line the title;
        /// absent or `null` keeps the source's default.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Identifier `"set_parameter"`. Set one parameter of a build
    /// configuration to `value` (spec #660 story 19, issue #668). `entity` is
    /// the **build configuration**; `name` is the parameter's full name
    /// (`env.SEPA_RETRIES`).
    ///
    /// Setting a parameter the configuration inherits gives it an own value
    /// that overrides the inherited one, which is what the source itself does;
    /// the view says so before the reader saves. A password parameter is
    /// writable, and its value travels here and nowhere else: no view sends
    /// it back.
    ///
    /// `base` is the value the edit was made **against** -- the value the
    /// view drew when the reader started typing. Before it writes, the
    /// adapter reads the parameter again and refuses when the source holds
    /// anything else, naming what it holds now: the configuration's mirrored
    /// record does not carry its parameters, so the write queue's hold
    /// cannot see a colleague changing this one, and this read is what keeps
    /// that from being a silent last-write-wins (`update_page`'s
    /// `base_version` answers the same window). `None` skips the read, for a
    /// value the reader never saw: a parameter that did not exist, or a
    /// secret, whose value is never drawn. A secret's write is therefore
    /// knowingly last-write-wins.
    ///
    /// Hold detection takes the whole mirrored record, as for
    /// [`MergePullRequest`](WriteOp::MergePullRequest): see
    /// `knobas_core::write_queue::project` for what that record can and
    /// cannot witness for a parameter.
    SetParameter {
        /// The **build configuration** (#668).
        entity: String,
        /// The parameter's full name. On an inherited parameter the source
        /// creates an own value that overrides it.
        name: String,
        value: String,
        /// The value the edit was made against; the source refuses the
        /// write when the parameter no longer holds it. `null` for a secret,
        /// whose value is never drawn and whose write is last-write-wins, and
        /// for a parameter that did not exist.
        base: Option<String>,
    },
    /// Identifier `"run_build"`. Put one build of a build configuration on
    /// the queue, on a branch the reader picked and with parameters the
    /// reader changed (spec #660 story 21, issue #669). `entity` is the
    /// **build configuration**.
    ///
    /// The custom run beside [`TriggerBuild`](WriteOp::TriggerBuild), which
    /// stays the one-click run of the default branch with the
    /// configuration's own values. `branch: None` runs the default branch,
    /// as a trigger does; a branch is the source's own name for it
    /// (`feature/PAY-231-sepa-retry`), and an empty one is refused rather
    /// than read as either.
    ///
    /// `parameters` carries **only the overrides**, in the order the reader
    /// sees them: a value the reader left as drawn is not sent, so a run
    /// changes exactly what the reader changed and inherits everything else
    /// from the configuration as it stands at the source when the build
    /// starts. An override is for this one build; the configuration keeps
    /// its own values (that is [`SetParameter`](WriteOp::SetParameter)). A
    /// secret's value is never drawn, so it is here only when the reader
    /// typed one.
    ///
    /// `clean_checkout` and `first_in_queue` are the source's run options
    /// (spec #808, issue #852): check the sources out anew rather than
    /// reuse the agent's working directory, and put the run at the top of
    /// the source's queue. Both default to `false` and are absent on an
    /// older payload, which reads as neither; an adapter with no such option
    /// ignores a `false` and refuses a `true` it cannot honour.
    ///
    /// Hold detection takes liveness alone, as for a trigger: a run adds a
    /// build beside whatever the configuration holds and overwrites nothing.
    RunBuild {
        /// The **build configuration** (#669).
        entity: String,
        /// The source's name for the branch; `null` for the default branch.
        branch: Option<String>,
        /// Only the values the reader changed, for this build alone.
        parameters: Vec<BuildParameter>,
        /// Check the sources out anew (#852); absent reads as `false`.
        #[serde(default)]
        #[ts(as = "Option<bool>", optional)]
        clean_checkout: bool,
        /// Put the run at the top of the source's queue (#852); absent reads
        /// as `false`.
        #[serde(default)]
        #[ts(as = "Option<bool>", optional)]
        first_in_queue: bool,
    },
    /// Identifier `"submit_review"`. Review a pull request: approve it,
    /// request changes, or only comment, with a body and line comments (spec
    /// #701 workflow 6, issue #705). `entity` is the **pull request**.
    ///
    /// [`Approve`](WriteOp::Approve) stays as the inbox's one-click sign-off;
    /// this is the review a reader writes from the pull request's view.
    /// `body` may be empty where the source allows it (an approval with no
    /// words); a source that wants words for a verdict refuses with its own
    /// sentence. `comments` are the line comments that go with the review,
    /// empty for a review of the whole change (#709 fills them); absent on
    /// the wire reads as empty.
    ///
    /// Whether the reader may give the verdict -- Gitea refuses an approval or
    /// a request for changes from the pull request's own author -- is the
    /// source's answer, drawn on the view's action as its `disabled` reason
    /// and, where it changed between the offer and the flush, a refusal with
    /// the source's sentence (ADR-0004).
    ///
    /// Hold detection takes the whole mirrored record, as for `Approve`: a
    /// review is a judgement on one state of the pull request.
    SubmitReview {
        /// The **pull request** (#705).
        entity: String,
        verdict: ReviewVerdict,
        body: String,
        /// The line comments that go with the review; empty for a review of
        /// the whole change.
        #[serde(default)]
        comments: Vec<ReviewComment>,
    },
    /// Identifier `"delete_branch"`. Delete a branch at the source (spec #701
    /// workflow 10, issue #707). `entity` is the **branch**.
    ///
    /// The adapter refuses the repository's default branch and a protected
    /// branch with [`SourceError::Protocol`] naming why, before a request
    /// leaves: the view offers neither, and a payload that names one anyway
    /// is refused rather than tried. Whether the branch is merged is the
    /// view's sentence to say, not the op's to check: deleting an unmerged
    /// branch is a thing a reader may mean.
    ///
    /// Hold detection takes the whole mirrored record, as for
    /// [`MergePullRequest`](WriteOp::MergePullRequest): a delete is a
    /// judgement on one state of the branch, and a commit pushed to it after
    /// the reader pressed *Delete branch…* would otherwise go with it unread.
    /// Like a merge it leaves nothing new at the source, so a withdrawal in
    /// flight leaves no unclaimed write: what is left is the branch gone,
    /// which the next sync reads back.
    DeleteBranch {
        /// The **branch** (#707); the source refuses the default branch and a
        /// protected one.
        entity: String,
    },
    /// Identifier `"edit_pull_request"`. Edit a pull request's title and
    /// description, close it or reopen it (spec #701 workflow 8, issue #706).
    /// `entity` is the **pull request**.
    ///
    /// **Only the fields that are `Some` are written**, so an edit of the
    /// title never sends a stale copy of a description somebody else changed
    /// since the view was read (the deputy's ruling on #706). Each is `null`
    /// on the wire when left alone, and absent reads the same. An edit with
    /// nothing in it is refused before a request leaves.
    ///
    /// A merged pull request cannot be closed or reopened: that is the
    /// source's refusal, with its sentence (Gitea's 412, *cannot change state
    /// of this pull request, it was already merged*), never retried
    /// (ADR-0004). Merging is [`MergePullRequest`](WriteOp::MergePullRequest)'s,
    /// which is why [`PullRequestState`] has no `merged`.
    ///
    /// Hold detection is per field (the deputy's ruling on #706): a title
    /// edit holds on a retitle, a description edit on the description, a
    /// close or reopen on the state the source declares for the kind -- see
    /// `knobas_core::write_queue::project`.
    EditPullRequest {
        /// The **pull request** (#706). A field below left `null` is not
        /// written, so only what the reader changed reaches the source.
        entity: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        state: Option<PullRequestState>,
    },
    /// Identifier `"update_pull_request_branch"`. Bring a pull request's head
    /// branch up to date with its base (spec #701 workflow 9, issue #708):
    /// the base merged into the head, or the head rebased onto the base, as
    /// `style` says. `entity` is the **pull request**.
    ///
    /// Gitea's `POST .../pulls/{n}/update?style=`. A conflict, or a style the
    /// repository does not allow, comes back refused with the source's own
    /// sentence (ADR-0004). Hold detection takes the whole mirrored record,
    /// as for [`MergePullRequest`](WriteOp::MergePullRequest): the update
    /// rewrites (rebase) or extends (merge) the head as it stands at the
    /// flush, and a push after the press is a reason to look again. Like a
    /// merge it makes nothing new knobas would have to claim, so a withdrawal
    /// in flight leaves no unclaimed write: what is left is the head moved,
    /// which the next sync reads back.
    UpdatePullRequestBranch {
        /// The **pull request** (#708).
        entity: String,
        style: UpdateStyle,
    },
    /// Identifier `"commit_file"`. Commit a new version of one file through
    /// the source, never through a checkout (spec #808 D4, R-8.6, issue
    /// #822). `repo` is the **repository**, an [`EntityRef`] in string form
    /// (`gitea:tidewater/payout-service`).
    ///
    /// `repo` rather than `entity`, the second variant after
    /// [`CreatePage`](WriteOp::CreatePage) to spell its target under another
    /// name: the spec names the fields, and the op has a branch and a path
    /// beside its repository, so naming the container for what it is says
    /// which of the three the queue orders and holds against.
    /// Its delivery names `repo` as the target ([`WriteOp::target`]).
    ///
    /// `branch` is the branch the file was read on. With `new_branch` set,
    /// the source makes that branch from `branch` first and commits there, so
    /// `branch` is left as it was (the *Commit to a new branch and open a pull
    /// request* path; the pull request is a
    /// [`CreatePullRequest`](WriteOp::CreatePullRequest) of its own). `path`
    /// is the file's path in the repository; `content` its **whole** new
    /// text, never a patch; `message` the commit message.
    ///
    /// `base_sha` is the version of the file the edit was made **against**,
    /// in the source's own spelling: Gitea's blob `sha` of the file as it was
    /// read. The source compares it with what `branch` holds at the flush,
    /// and a file that changed since comes back as [`SourceError::Moved`]
    /// with the version it holds now, which the write queue turns into a
    /// held write carrying both (theirs in the held snapshot, the reader's in
    /// this payload). That comparison is the whole hold: the mirror does not
    /// hold files, so the queue's own projection takes only the repository's
    /// liveness, as for a create.
    ///
    /// *Apply anyway* re-sends against their version by itself
    /// (`knobas_core::write_queue::apply_anyway`). *Edit* is an amend, and an
    /// amend's payload is the caller's whole: one answering a moved commit
    /// carries `base_sha` set to the held side's `base`, or the source
    /// compares it against the stale blob again and it is held again.
    ///
    /// Text only: `content` is a `String` and the source is sent its UTF-8
    /// bytes. A file that does not exist on `branch` is refused rather than
    /// created: an edit is of a file somebody read.
    ///
    /// [`EntityRef`]: knobas_spi_types::entity::EntityRef
    CommitFile {
        /// The **repository** (#822), this op's queue target.
        repo: String,
        branch: String,
        /// Makes this branch from `branch` first; `null` commits onto
        /// `branch`.
        #[serde(default)]
        new_branch: Option<String>,
        path: String,
        /// The whole new text, never a patch.
        content: String,
        message: String,
        /// The blob the edit was made against. A file that changed since
        /// comes back held, theirs in the held snapshot (`text`, `base`) and
        /// this payload as the reader's. *Apply anyway* re-sends against
        /// `base` by itself; an *Edit* (amend) of a moved commit sends
        /// `base_sha` set to the held side's `base`, or it is held again.
        base_sha: String,
    },
    /// Identifier `"source_action"`. A write outside every
    /// [role](role)'s vocabulary (ADR-0029, `CONTEXT.md`: *source action*,
    /// issue #1194): Gitea's *Add label…* on a pull request, a *Convert to
    /// draft*. `entity` is what the action acts on, as for every op.
    ///
    /// **The one variant knobas does not understand, by design.** `id` is the
    /// adapter's own name for the action, one it declares in
    /// [`SourceDescriptor::source_actions`]; `fields` are the answers of the
    /// dialog the adapter drew for it (a [`view::ViewAction`] whose
    /// `write_op` is this identifier and whose fixed `values` name the `id`),
    /// text by field name, plus its other fixed values. The shell copies them
    /// here and never reads them; the queue holds, refuses and retries this op
    /// like any write and hands it back to the adapter, which performs it or
    /// refuses an `id` it did not declare with [`SourceError::Protocol`]
    /// (battery clause 11).
    ///
    /// The ops of a role stay typed variants, because the shell acts on them
    /// itself (*Start work* creates a branch). One that the shell starts to
    /// act on is promoted to a variant of its own, in a §10.8 entry.
    ///
    /// Hold detection takes the whole mirrored record: knobas cannot know
    /// what the action overwrites, so any change to the target holds it. A
    /// resend is not harmless for the same reason, so a write sent once and
    /// unanswered is held for a person.
    SourceAction {
        entity: String,
        /// The adapter's own name for the action (#1194, ADR-0029).
        id: String,
        /// The answers of the dialog the adapter declared, by field name. The
        /// shell copies `id` and these from the action and never reads them.
        #[serde(default)]
        fields: std::collections::BTreeMap<String, String>,
    },
    /// Identifier `"reply_to_thread"`. Answer one review thread on a pull
    /// request's diff (spec #2234 step 6d, issue #2277). `entity` is the
    /// **pull request**.
    ///
    /// `thread` is the source's own id for the thread, as the diff's
    /// [`LineThread::id`](crate::view::LineThread::id) hands it out, and
    /// `comment` is the reply placed where the thread is: a source that
    /// threads by id (GitHub's `in_reply_to`) reads `thread`, one that threads
    /// by place (Gitea groups a conversation by path and line) reads
    /// `comment`'s place. The shell copies both from the thread it answers.
    ///
    /// Hold detection is a comment's: the declared state only. A second
    /// arrival is a second reply.
    ReplyToThread {
        /// The **pull request** (#2277).
        entity: String,
        /// The thread's id, as the diff handed it out.
        thread: String,
        /// The reply, at the thread's path, side and line.
        comment: ReviewComment,
    },
    /// Identifier `"resolve_thread"`. Mark one review thread resolved, or
    /// open again (`resolved: false`), on a pull request (spec #2234 step 6d,
    /// issue #2277). `entity` is the **pull request**, `thread` the id the
    /// diff handed out.
    ///
    /// A source that cannot resolve threads does not declare the op, or does
    /// not offer it on the diff ([`Commenting::resolve`](crate::view::Commenting::resolve)):
    /// Forgejo 17 has no such endpoint (the 2026-10-30 Gitea fact note). The
    /// same state twice is one, so a resend is harmless.
    ResolveThread {
        /// The **pull request** (#2277).
        entity: String,
        /// The thread's id, as the diff handed it out.
        thread: String,
        /// `true` resolves, `false` opens it again.
        resolved: bool,
    },
    /// Identifier `"request_reviewers"`. Ask one or more people to review a
    /// pull request (spec #2234 step 6d, issue #2303). `entity` is the **pull
    /// request**; `reviewers` are logins as the source names them, picked
    /// from the ones the view's action offers.
    ///
    /// Asking somebody already asked asks them again and adds nobody, so a
    /// resend is harmless. Hold detection is the declared state only: a push
    /// to the pull request is no reason to hold a request for a review of it.
    RequestReviewers {
        /// The **pull request** (#2303).
        entity: String,
        /// Who is asked, by login.
        reviewers: Vec<String>,
    },
}

/// The identifier of [`WriteOp::SourceAction`], which a descriptor lists in
/// [`SourceDescriptor::write_ops`] when it declares any
/// [`source_actions`](SourceDescriptor::source_actions), and a view's action
/// names as its `write_op`.
pub const SOURCE_ACTION: &str = "source_action";

/// The [`view::ViewAction::values`] key a source action's `id` is fixed
/// under, which is also its field's name in [`WriteOp::SourceAction`]: so
/// one op is many actions, told apart by a fixed value (the #705 ruling), and
/// no dialog field of a source action may be named this.
pub const SOURCE_ACTION_ID: &str = "id";

/// The state an [`EditPullRequest`](WriteOp::EditPullRequest) puts a pull
/// request in. Serialized as the lowercase word (`"closed"`), which is also
/// Gitea's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum PullRequestState {
    Open,
    Closed,
}

impl PullRequestState {
    /// The word on the wire, which is also what a view's action fixes as
    /// its `state` value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            PullRequestState::Open => "open",
            PullRequestState::Closed => "closed",
        }
    }
}

/// What a [`SubmitReview`](WriteOp::SubmitReview) says of the pull request.
/// Serialized as the snake_case word (`"request_changes"`); each adapter
/// spells it its own way (Gitea's `event`).
///
/// - `approve`: the change may land.
/// - `request_changes`: the change must change before it lands.
/// - `comment`: words, and no verdict.
///
/// The verdicts are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum ReviewVerdict {
    Approve,
    RequestChanges,
    Comment,
}

/// One line comment of a [`SubmitReview`](WriteOp::SubmitReview): the file's
/// path in the change, the line number on `side`, and what it says.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct ReviewComment {
    pub path: String,
    pub line: u64,
    pub side: ReviewSide,
    pub body: String,
}

/// Which side of a diff a [`ReviewComment`]'s `line` counts on: the file as it
/// was (`old`, a removed or unchanged line's old number) or as it becomes
/// (`new`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum ReviewSide {
    Old,
    New,
}

/// One parameter of a [`RunBuild`](WriteOp::RunBuild) (#669): its full name
/// (`env.SEPA_RETRIES`) and the value this run takes for it. A struct rather
/// than a pair so the wire reads `{"name": .., "value": ..}`, the shape a
/// source's own parameter lists have.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct BuildParameter {
    pub name: String,
    pub value: String,
}

/// What kind of check a [`CreateMonitor`](WriteOp::CreateMonitor) asks for
/// (#864, spec #808 story 154), in knobas' words rather than one server's:
/// Uptime Kuma calls a TCP check `port`. Serialized as the lowercase word.
///
/// The four a reader is offered for an asset (R-11.4): HTTP for a route or a
/// service, TCP for a database, Docker for a container, Ping for a VM. A
/// closed set, so a payload naming a check no adapter makes fails to decode at
/// `submit_write` rather than reaching a source.
///
/// - `http`: an http(s) URL fetched and expected to answer 2xx.
/// - `tcp`: a TCP connection to `host:port`.
/// - `ping`: an ICMP ping to a host.
/// - `docker`: a container, by name, asked of the monitoring source's Docker
///   host.
///
/// The kinds are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum MonitorKind {
    #[default]
    Http,
    Tcp,
    Ping,
    Docker,
}

impl MonitorKind {
    /// The word on the wire.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MonitorKind::Http => "http",
            MonitorKind::Tcp => "tcp",
            MonitorKind::Ping => "ping",
            MonitorKind::Docker => "docker",
        }
    }
}

/// How a [`MergePullRequest`](WriteOp::MergePullRequest) lands its commits on
/// the base branch. Serialized as the lowercase word (`"squash"`), which is
/// also what a source view's merge action offers as its options.
///
/// - `merge`: a merge commit joining head into base.
/// - `rebase`: the head's commits replayed onto base, no merge commit.
/// - `squash`: the head's commits folded into one commit on base.
///
/// The styles are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum MergeStyle {
    Merge,
    Rebase,
    Squash,
}

impl MergeStyle {
    /// Every style, in the order a view offers them: `merge` first, the
    /// default a source's own *Merge* button takes.
    pub const ALL: [MergeStyle; 3] = [MergeStyle::Merge, MergeStyle::Rebase, MergeStyle::Squash];

    /// The word on the wire, which is also the option a view offers.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            MergeStyle::Merge => "merge",
            MergeStyle::Rebase => "rebase",
            MergeStyle::Squash => "squash",
        }
    }
}

/// How an [`UpdatePullRequestBranch`](WriteOp::UpdatePullRequestBranch)
/// brings the head up to date with its base. Serialized as the lowercase
/// word, which is also what the view's *Update branch…* offers as its options
/// and what Gitea's `?style=` takes.
///
/// - `merge`: the base merged into the head: a merge commit on the head.
/// - `rebase`: the head's commits replayed onto the base: the head rewritten.
///
/// The styles are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum UpdateStyle {
    Merge,
    Rebase,
}

impl UpdateStyle {
    /// Every style, in the order a view offers them: `merge` first, the one
    /// that rewrites nothing.
    pub const ALL: [UpdateStyle; 2] = [UpdateStyle::Merge, UpdateStyle::Rebase];

    /// The word on the wire, which is also the option a view offers.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            UpdateStyle::Merge => "merge",
            UpdateStyle::Rebase => "rebase",
        }
    }
}

/// The key half of what a [`CreateMonitor`](WriteOp::CreateMonitor) targets.
///
/// Private, because nothing outside this module has a use for half an address:
/// [`monitor_target`] is what callers want, and a key on its own would be an
/// invitation to compose the other half by hand.
///
/// A monitor id in every adapter that has one is the source's own, and Uptime
/// Kuma's are decimal, so `"monitors"` cannot collide with a mirrored monitor
/// -- pinned by `the_create_target_is_not_a_monitor_id`.
const MONITOR_TARGET_KEY: &str = "monitors";

/// The entity id a [`CreateMonitor`](WriteOp::CreateMonitor) against
/// `source_id` targets: that source's own address for the monitors it holds.
///
/// **Spelled once, here, and not in the app.** The alternative is every
/// producer of the op composing the string itself, which is a per-adapter
/// table in the app (`adapter_kind == "kuma"`) of exactly the kind
/// `config_schema` and `accepts_account` exist to forbid: the app asks which
/// sources offer `create_monitor` and asks the SPI what to address, and it
/// never learns the word *Kuma*.
#[must_use]
pub fn monitor_target(source_id: &str) -> String {
    format!("{source_id}:{MONITOR_TARGET_KEY}")
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
            WriteOp::CreatePage { .. } => "create_page",
            WriteOp::UpdatePage { .. } => "update_page",
            WriteOp::PauseMonitor { .. } => "pause_monitor",
            WriteOp::ResumeMonitor { .. } => "resume_monitor",
            WriteOp::CreateMonitor { .. } => "create_monitor",
            WriteOp::MergePullRequest { .. } => "merge_pull_request",
            WriteOp::SetParameter { .. } => "set_parameter",
            WriteOp::RunBuild { .. } => "run_build",
            WriteOp::SubmitReview { .. } => "submit_review",
            WriteOp::DeleteBranch { .. } => "delete_branch",
            WriteOp::EditPullRequest { .. } => "edit_pull_request",
            WriteOp::UpdatePullRequestBranch { .. } => "update_pull_request_branch",
            WriteOp::CommitFile { .. } => "commit_file",
            WriteOp::SourceAction { .. } => SOURCE_ACTION,
            WriteOp::ReplyToThread { .. } => "reply_to_thread",
            WriteOp::ResolveThread { .. } => "resolve_thread",
            WriteOp::RequestReviewers { .. } => "request_reviewers",
        }
    }

    /// How the queue delivers this op: what a hold compares, whether a resend
    /// is harmless, whether a withdrawal can leave an unclaimed artefact,
    /// which field names the target, and which link an arrival draws
    /// (ADR-0006, amended 2026-10-19).
    ///
    /// **No wildcard arm**, for [`WriteOp::identifier`]'s reason: a new
    /// variant does not compile until it states its delivery here. The
    /// reasons per op are argued in `knobas_core::write_queue::project` for
    /// the projection, and in `knobas-sync`'s `attempt` and `unclaimed` for
    /// the resend and the withdrawal, which read this value. The app's
    /// `link_on_arrival` reads the link from here.
    #[must_use]
    pub fn delivery(&self) -> Delivery {
        use knobas_spi_types::delivery::{ArrivalLink, Projection, TargetField};
        // A source action's: maximally conservative on every count (#1194).
        // Every other arm names each answer it loosens.
        let conservative = Delivery {
            projection: Projection::WHOLE,
            resend_harmless: false,
            can_leave_unclaimed: true,
            target: TargetField::Entity,
            link: None,
        };
        match self {
            // What it does is the adapter's alone, so knobas can say nothing
            // in its favour.
            WriteOp::SourceAction { .. } => conservative,
            // They add to a discussion and hold only on its declared state
            // (#752); a second arrival is a second comment or review.
            WriteOp::Comment { .. }
            | WriteOp::Approve { .. }
            | WriteOp::SubmitReview { .. }
            | WriteOp::ReplyToThread { .. } => Delivery {
                projection: Projection::DeclaredState,
                can_leave_unclaimed: false,
                ..conservative
            },
            // They set a state on the discussion, and the same state twice is
            // one (#2277, #2303): Gitea answers a repeated resolve 204 and a
            // repeated request adds nobody.
            WriteOp::ResolveThread { .. } | WriteOp::RequestReviewers { .. } => Delivery {
                projection: Projection::DeclaredState,
                resend_harmless: true,
                can_leave_unclaimed: false,
                ..conservative
            },
            // A judgement on the target's current state; a second arrival is
            // a second move.
            WriteOp::Transition { .. } => Delivery {
                can_leave_unclaimed: false,
                ..conservative
            },
            // They overwrite, and the source refuses the repeat or it sets
            // what is already set (#1122).
            WriteOp::UpdatePage { .. }
            | WriteOp::MergePullRequest { .. }
            | WriteOp::SetParameter { .. }
            | WriteOp::DeleteBranch { .. }
            | WriteOp::UpdatePullRequestBranch { .. } => Delivery {
                resend_harmless: true,
                can_leave_unclaimed: false,
                ..conservative
            },
            // Overwrites only the fields it carries (#706).
            WriteOp::EditPullRequest { .. } => Delivery {
                projection: Projection::Whole { per_field: true },
                resend_harmless: true,
                can_leave_unclaimed: false,
                ..conservative
            },
            // A ticket's title is no address to resend against (#336). The
            // arrival is linked to what it was filed from, a failed run
            // (#1451), found by the key its receipt names once a sync brings
            // it in (#1754).
            WriteOp::CreateTicket { .. } => Delivery {
                projection: Projection::Liveness,
                link: Some(ArrivalLink::Related),
                ..conservative
            },
            // The head branch is the address, the source refuses the repeat,
            // and the arrival is linked to its job (#984): a pull request by
            // the key its receipt names (#1949), a branch by its name.
            WriteOp::CreateBranch { .. } | WriteOp::CreatePullRequest { .. } => Delivery {
                projection: Projection::Liveness,
                resend_harmless: true,
                can_leave_unclaimed: false,
                link: Some(ArrivalLink::Related),
                ..conservative
            },
            // They add beside the container; a second arrival is a second
            // build or worklog.
            WriteOp::TriggerBuild { .. }
            | WriteOp::RerunBuild { .. }
            | WriteOp::RunBuild { .. }
            | WriteOp::LogWork { .. } => Delivery {
                projection: Projection::Liveness,
                can_leave_unclaimed: false,
                ..conservative
            },
            // Created under its parent and drawn against its job (#1121).
            WriteOp::CreatePage { .. } => Delivery {
                projection: Projection::Liveness,
                target: TargetField::Parent,
                link: Some(ArrivalLink::Documents),
                ..conservative
            },
            // The monitor's own state, and the same state twice is one.
            WriteOp::PauseMonitor { .. } | WriteOp::ResumeMonitor { .. } => Delivery {
                projection: Projection::Liveness,
                resend_harmless: true,
                can_leave_unclaimed: false,
                ..conservative
            },
            // A monitor's name is a label, not an address (#453).
            WriteOp::CreateMonitor { .. } => Delivery {
                projection: Projection::Liveness,
                ..conservative
            },
            // Into a repository; the source compares the file's own version
            // (#822).
            WriteOp::CommitFile { .. } => Delivery {
                projection: Projection::Liveness,
                resend_harmless: true,
                can_leave_unclaimed: false,
                target: TargetField::Repository,
                ..conservative
            },
        }
    }

    /// The entity this write targets, read from the field its
    /// [`delivery`](WriteOp::delivery) names: what the queue orders against,
    /// what hold detection snapshots, and what an amendment may not repoint.
    ///
    /// The `entity` arm has **no wildcard**, so a new variant does not compile
    /// until it says whether it carries one. A delivery that names a field
    /// its variant lacks answers `""`, which no `EntityRef` parses, so the
    /// queue refuses the write rather than ordering it against nothing; the
    /// battery's `every_write_op_answers_a_delivery_whose_target_it_carries`
    /// is what keeps that from happening.
    #[must_use]
    pub fn target(&self) -> &str {
        use knobas_spi_types::delivery::TargetField;
        let field = match self.delivery().target {
            TargetField::Parent => match self {
                WriteOp::CreatePage { parent, .. } => Some(parent),
                _ => None,
            },
            TargetField::Repository => match self {
                WriteOp::CommitFile { repo, .. } => Some(repo),
                _ => None,
            },
            TargetField::Entity => match self {
                WriteOp::Comment { entity, .. }
                | WriteOp::Transition { entity, .. }
                | WriteOp::CreateTicket { entity, .. }
                | WriteOp::CreateBranch { entity, .. }
                | WriteOp::CreatePullRequest { entity, .. }
                | WriteOp::Approve { entity, .. }
                | WriteOp::TriggerBuild { entity }
                | WriteOp::RerunBuild { entity }
                | WriteOp::LogWork { entity, .. }
                | WriteOp::UpdatePage { entity, .. }
                | WriteOp::PauseMonitor { entity }
                | WriteOp::ResumeMonitor { entity }
                | WriteOp::CreateMonitor { entity, .. }
                | WriteOp::MergePullRequest { entity, .. }
                | WriteOp::SetParameter { entity, .. }
                | WriteOp::RunBuild { entity, .. }
                | WriteOp::SubmitReview { entity, .. }
                | WriteOp::DeleteBranch { entity }
                | WriteOp::EditPullRequest { entity, .. }
                | WriteOp::UpdatePullRequestBranch { entity, .. }
                | WriteOp::SourceAction { entity, .. }
                | WriteOp::ReplyToThread { entity, .. }
                | WriteOp::ResolveThread { entity, .. }
                | WriteOp::RequestReviewers { entity, .. } => Some(entity),
                WriteOp::CreatePage { .. } | WriteOp::CommitFile { .. } => None,
            },
        };
        field.map_or("", String::as_str)
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
    /// spelling (Jira's worklog ids are decimal strings). For a create of
    /// something the mirror holds -- a page, a ticket, a pull request, a
    /// commit, a queued build -- it is that entity's local part, so
    /// `<source_id>:<remote_id>` is its entity id (#1121, #1754, #1949).
    ///
    /// **Or it names something the write created inside an entity**, which
    /// the mirror holds in that entity's payload rather than as an entity of
    /// its own: a Confluence comment's id, which the page's comment carries
    /// as `DocComment::id` (#2074). `<source_id>:<remote_id>` is then **not**
    /// an entity id, so a reader of `knobas.write_queue.remote_id` that builds
    /// one reads it only off an op, or an entity kind, whose id is an entity's.
    ///
    /// A create whose answer names what it made answers it here rather than
    /// dropping it. `None` when the op creates nothing addressable, or when
    /// the source did not say.
    pub remote_id: Option<String>,
}

impl WriteReceipt {
    /// The source had nothing to say -- the answer for every op that creates
    /// nothing addressable.
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

/// What a source answers when asked for one item by id
/// ([`Source::refresh`], ADR-0031).
///
/// `Item` is unboxed, though it dwarfs the other two: the answer is built
/// once per read and moved straight into the sink, and every adapter writes
/// `RefreshAnswer::Item(item)` without a `Box::new` around what its sync
/// already builds.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum RefreshAnswer {
    /// The item as the source holds it now, in the shape a sync emits it:
    /// the same entity, kind, title and payload a run would write for it.
    Item(SyncItem),
    /// The source no longer has the item. The engine tombstones it, as a
    /// run's probe would.
    Gone,
    /// This adapter does not read this item by id (its kind, or the whole
    /// adapter). The default, and the answer for every kind of an adapter
    /// that does not declare [`Capability::Refresh`].
    NotOffered,
}

impl WriteOp {
    /// Whether a [`WriteReceipt::remote_id`] for this op is the **local part
    /// of an entity id**: the item the write made, which `<source_id>:<remote_id>`
    /// addresses and a refresh reads after the write lands (ADR-0031). A
    /// page, a ticket, a pull request, a commit, a queued build, a monitor.
    ///
    /// `false` where the id names something inside an entity (a Confluence
    /// comment, #2074) or outside the mirror (a worklog, #280), and for every
    /// op that makes nothing: there the write's [`target`](WriteOp::target)
    /// is the item to read back. No wildcard, so a new op says which it is.
    #[must_use]
    pub fn receipt_names_an_entity(&self) -> bool {
        match self {
            WriteOp::CreateTicket { .. }
            | WriteOp::CreatePage { .. }
            | WriteOp::CreatePullRequest { .. }
            | WriteOp::CommitFile { .. }
            | WriteOp::TriggerBuild { .. }
            | WriteOp::RerunBuild { .. }
            | WriteOp::RunBuild { .. }
            | WriteOp::CreateMonitor { .. } => true,
            WriteOp::Comment { .. }
            | WriteOp::LogWork { .. }
            | WriteOp::Transition { .. }
            | WriteOp::CreateBranch { .. }
            | WriteOp::Approve { .. }
            | WriteOp::SubmitReview { .. }
            | WriteOp::UpdatePage { .. }
            | WriteOp::PauseMonitor { .. }
            | WriteOp::ResumeMonitor { .. }
            | WriteOp::MergePullRequest { .. }
            | WriteOp::SetParameter { .. }
            | WriteOp::DeleteBranch { .. }
            | WriteOp::EditPullRequest { .. }
            | WriteOp::UpdatePullRequestBranch { .. }
            | WriteOp::SourceAction { .. }
            | WriteOp::ReplyToThread { .. }
            | WriteOp::ResolveThread { .. }
            | WriteOp::RequestReviewers { .. } => false,
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
    /// [`SourceError::Protocol`] rather than attempted. The write queue's
    /// flush, the one caller, refuses one first by
    /// [`SourceDescriptor::declares`] against this source's own descriptor,
    /// and the WASM host does the same before a call crosses into a guest
    /// (#1950). An adapter's refusal is the defence behind them, still held
    /// by [`contract::battery`].
    ///
    /// The answer is a [`WriteReceipt`], which for almost every op is
    /// [`WriteReceipt::none`]: knobas addresses what it wrote by *reading it
    /// back*, and a receipt exists only for the writes where reading it back
    /// cannot identify the thing that was made: a worklog, whose id is what
    /// the local copy carries (issue #280); a create the next sync mirrors
    /// (a page, a ticket, a pull request, a commit, a queued build: #1121,
    /// #1754, #1949); a Confluence comment, which its page's payload carries
    /// under that id (#2074). An adapter that has nothing to say answers
    /// `none` rather than inventing an id.
    ///
    /// Reading it back is a [`refresh`](Source::refresh) where the adapter
    /// offers one (ADR-0031): once the write lands, the app reads the item
    /// the receipt names ([`WriteOp::receipt_names_an_entity`]) or else the
    /// write's [`target`](WriteOp::target), and then runs the source for what
    /// the write changed elsewhere. An adapter that offers no refresh gets
    /// the run alone.
    async fn write(&self, op: WriteOp) -> Result<WriteReceipt, SourceError>;
    /// Read one item by id: a **refresh** (`CONTEXT.md`, ADR-0031). `entity`
    /// is an [`EntityRef`] in string form, in this source's namespace.
    ///
    /// The answer is the [`SyncItem`] a sync would emit for that id
    /// ([`RefreshAnswer::Item`]): the same kind, title and declared paths, so
    /// the engine writes it through the run's own item write.
    /// [`RefreshAnswer::Gone`] is an id the source no longer has, which the
    /// engine tombstones. [`RefreshAnswer::NotOffered`] is the default and
    /// says this adapter does not read this item's kind by id; the app then
    /// runs the source instead.
    ///
    /// An adapter that answers for any kind declares [`Capability::Refresh`],
    /// and one that does not declare it answers `NotOffered` for every id.
    /// [`contract::battery`] clause 13 holds both, and holds each kind it
    /// answers for to what the sync emitted.
    ///
    /// A read and nothing else: no cursor, no sweep. The engine calls it
    /// under the source's run lock, so it never overlaps a run of the same
    /// source. A failed read is an error with the same fault classification
    /// as [`sync`](Source::sync), never `Gone`.
    ///
    /// [`EntityRef`]: knobas_spi_types::entity::EntityRef
    async fn refresh(&self, entity: &str) -> Result<RefreshAnswer, SourceError> {
        let _ = entity;
        Ok(RefreshAnswer::NotOffered)
    }
    /// Why this source does not declare `op`, in words a person can act on,
    /// when the adapter knows more than that it does not. An Uptime Kuma with
    /// no account names the account to add (#452).
    ///
    /// The flush asks this only after [`SourceDescriptor::declares`] refused
    /// `op`, and the answer becomes the refused row's detail in place of the
    /// generic one (#1950). It is a wording, not a check: it sends nothing,
    /// and its answer never decides whether `op` is refused. The default,
    /// `None`, keeps the generic wording, as does every WASM guest.
    fn undeclared_reason(&self, op: &WriteOp) -> Option<String> {
        let _ = op;
        None
    }
    /// The writes among `asks` this source's **credential** cannot make, each
    /// with its reason (issue #2040, [`grants`]): what a surface that draws a
    /// write from [`SourceDescriptor::write_ops`] asks after drawing it, to
    /// draw the refused ones disabled. An ask it does not answer is no
    /// verdict, and stays as drawn.
    ///
    /// Never found out by a write that could change anything: an adapter
    /// answers from a read of what its credential may do, or, where its
    /// system has no such read, from a write the system refuses before it
    /// changes anything (Gitea's empty-bodied creates, #2030, #2054). It
    /// answers only about asks it was given, and only for ops it declares.
    ///
    /// The default answers nothing: the right answer for an adapter whose
    /// credential cannot be narrower than the adapter (a mock, Uptime Kuma,
    /// whose account holds every right), and what every WASM guest answers.
    /// An adapter whose credential can be narrower overrides it, finding out
    /// as the paragraph above allows; an adapter that does not override it
    /// answers nothing. A failed read or probe is an error, never an answer
    /// that refuses nothing (#2306): the app keeps an answer that moves and
    /// asks it again behind, so a failure answered as taken would stand. The
    /// app draws the writes of a failed ask disabled, saying it could not
    /// ask, and keeps nothing of it.
    async fn refused_writes(
        &self,
        asks: &[grants::WriteAsk],
    ) -> Result<Vec<grants::RefusedWrite>, SourceError> {
        let _ = asks;
        Ok(Vec::new())
    }
    /// What the answer [`refused_writes`](Source::refused_writes) gives
    /// for `ask` stands at, said before it is asked, as
    /// [`view_standing`](Source::view_standing) says it of a view (ADR-0032,
    /// #2241): [`Standing::Live`](view::Standing::Live), the default, is
    /// asked every time; [`Standing::Moving`](view::Standing::Moving) is
    /// answered from memory where the app holds it and asked again behind.
    /// Pure: it reads nothing from the source.
    fn refusal_standing(&self, ask: &grants::WriteAsk) -> view::Standing {
        let _ = ask;
        view::Standing::Live
    }
    /// The statuses this entity's workflow offers **from where it stands right
    /// now**, in the source's own spelling (`CONTEXT.md`: *reachable
    /// transition*).
    ///
    /// `entity` is an [`EntityRef`] in string form, the same address every
    /// [`WriteOp`] carries, and the answer is about *now*: it is read when a
    /// detail opens and nothing stores it. A source that answers `["In
    /// Progress", "Done"]` is not promising those two will still be reachable
    /// when the write is flushed -- [`WriteOp::Transition`] still names the
    /// status it wants and the adapter still resolves and refuses that name
    /// against the source's answer at write time. This read is what the
    /// **select** offers; it is not a substitute for that resolution and
    /// nothing may treat it as one.
    ///
    /// **No default implementation, and that is the point** (ADR-0006's
    /// forcing function applied to a read). A default returning "nothing" would
    /// be an adapter silently declining to answer, which reads downstream
    /// exactly like a workflow with no moves left; a default returning an error
    /// would let a new adapter with a real workflow ship a select that offers
    /// nothing until somebody noticed. So every adapter answers, and one
    /// without a workflow at all refuses **by name** -- the message says which
    /// adapter has no workflow -- the way [`write`](Source::write) refuses an
    /// op the descriptor does not declare. [`contract::battery`] holds an
    /// adapter to it: a source that does not declare the `"transition"` write
    /// op must refuse this read.
    ///
    /// A refusal is [`SourceError::Protocol`], as it is for an undeclared
    /// write. A source that *has* a workflow reports whatever its request maps
    /// to -- an expired credential is [`SourceError::Unauthorized`] here as
    /// everywhere else, and the shell falls back to the corpus-observed offer
    /// rather than showing an empty select.
    ///
    /// [`EntityRef`]: knobas_spi_types::entity::EntityRef
    async fn reachable_transitions(&self, entity: &str) -> Result<Vec<String>, SourceError>;
    /// The **source view** this adapter draws for one of its own entities
    /// (ADR-0020, `CONTEXT.md` *Source view*): sections of typed blocks from
    /// the fixed vocabulary in [`view`], read live when a detail opens and
    /// never stored in the database. What
    /// [`view_standing`](Source::view_standing) says is fixed the app keeps in
    /// memory (ADR-0032).
    ///
    /// `entity` is an [`EntityRef`] in string form, the address
    /// [`reachable_transitions`](Source::reachable_transitions) takes.
    ///
    /// * `section: None` asks for the whole view. A **deferred** section comes
    ///   with its id, title and count and no blocks.
    /// * `section: Some(id)` asks for that one section's blocks: the answer is
    ///   a view holding exactly that section, not deferred. An id this kind's
    ///   view does not have is refused with [`SourceError::Protocol`].
    ///
    /// `Ok(None)` is **this adapter draws no view for this kind**, and the
    /// detail draws the generic payload projection with no comment. A failed
    /// read is an error, and the detail draws the same projection under a
    /// line naming it (ADR-0020 rule 2) -- so an adapter must never answer
    /// `None` for a read that failed, or the reader is told nothing.
    ///
    /// **No default implementation** (ADR-0006's forcing function, as for
    /// `reachable_transitions`): an adapter with nothing to draw says so by
    /// answering `Ok(None)` for every kind. [`contract::battery`] clause 9
    /// asks every adapter for the view of every entity it emitted.
    ///
    /// [`EntityRef`]: knobas_spi_types::entity::EntityRef
    async fn view(
        &self,
        entity: &str,
        section: Option<&str>,
    ) -> Result<Option<view::SourceView>, SourceError>;
    /// What the answer [`view`](Source::view) gives for `entity` and
    /// `section` stands at (ADR-0032, #2239), said before it is read: whether
    /// the app may keep it in memory and answer the next ask for the same
    /// pair from there.
    ///
    /// [`Standing::Live`](view::Standing::Live) is the default and is never
    /// kept. An adapter answers [`Standing::Fixed`](view::Standing::Fixed)
    /// only where the pair names something that cannot change -- a file at a
    /// full commit sha -- and the answer carries nothing that can: no
    /// toolbar a credential narrows, no status that moves. Pure: it reads
    /// nothing from the source.
    fn view_standing(&self, entity: &str, section: Option<&str>) -> view::Standing {
        let _ = (entity, section);
        view::Standing::Live
    }
    /// The bytes of the file at `path` in `entity`'s tree (#2256, spec #2234
    /// step 5), read whole up to `cap` bytes: a README's image, a PDF.
    ///
    /// `commit` is a [`Block::File`](view::Block::File)'s `commit`, handed
    /// back: the read is at that commit, where the file the reader has open
    /// was read. `None` reads at the entity's own reference (its branch, its
    /// repository's default branch).
    ///
    /// A file longer than `cap` is [`FileBytes::TooLarge`](view::FileBytes::TooLarge)
    /// with its size; the adapter stops reading past the cap and never
    /// answers a cut file. A path that is no file is
    /// [`SourceError::Protocol`].
    ///
    /// `Ok(None)` is **this adapter reads no file bytes**, and is the
    /// default.
    async fn file_bytes(
        &self,
        entity: &str,
        path: &str,
        commit: Option<&str>,
        cap: u64,
    ) -> Result<Option<view::FileBytes>, SourceError> {
        let _ = (entity, path, commit, cap);
        Ok(None)
    }
    /// What [`file_bytes`](Source::file_bytes)' answer for the same
    /// arguments stands at, as [`view_standing`](Source::view_standing) says
    /// it of a view (ADR-0032, #2256): [`Standing::Fixed`](view::Standing::Fixed)
    /// only where `commit` names a commit no push can change. The default is
    /// [`Standing::Live`](view::Standing::Live). Pure: it reads nothing.
    fn bytes_standing(&self, entity: &str, path: &str, commit: Option<&str>) -> view::Standing {
        let _ = (entity, path, commit);
        view::Standing::Live
    }
    /// A pipeline's settings, read live (ADR-0022, issue #851): the
    /// repositories it builds, its triggers and its parameters, scrubbed of
    /// every credential. `entity` is a pipeline's [`EntityRef`] in string
    /// form.
    ///
    /// `Ok(None)` is **this adapter keeps no pipelines**, and is the default:
    /// a source with no Builds frame has nothing to answer, and every adapter
    /// but a CI one is that. An adapter whose descriptor declares a
    /// [`Frame::Pipeline`](knobas_spi_types::payload::Frame::Pipeline) kind
    /// overrides it, and must never answer `None` for a read that failed.
    ///
    /// [`EntityRef`]: knobas_spi_types::entity::EntityRef
    async fn pipeline_settings(
        &self,
        entity: &str,
    ) -> Result<Option<pipeline::PipelineSettings>, SourceError> {
        let _ = entity;
        Ok(None)
    }
    /// Runs of the pipeline `entity` older than the run `before`, newest
    /// first, at most `count` (capped at [`pipeline::OLDER_RUNS_MAX`]) (ADR-0022,
    /// issue #851): the history beyond the window the sync keeps, asked for
    /// when the reader scrolls past the end of it.
    ///
    /// Each run is the [`SyncItem`] the sync would have emitted for it, so the
    /// app reads it through the run kind's declared paths; nothing is stored.
    /// `before` is a run's [`EntityRef`] in string form, and is not itself in
    /// the answer. An empty answer is the start of the history.
    ///
    /// `Ok(None)` as for [`pipeline_settings`](Source::pipeline_settings).
    ///
    /// [`EntityRef`]: knobas_spi_types::entity::EntityRef
    async fn older_runs(
        &self,
        entity: &str,
        before: &str,
        count: u32,
    ) -> Result<Option<Vec<SyncItem>>, SourceError> {
        let _ = (entity, before, count);
        Ok(None)
    }
    /// The chain the run `run` is in (ADR-0022, issue #857, R-9.13): one
    /// entry per step of its pipeline's [`chain`](pipeline::PipelineSettings::chain),
    /// in order, each with its run on the run's branch (see
    /// [`pipeline::RunChainStep::run`]). Empty for a run of a pipeline in no
    /// chain. `run` is a run's [`EntityRef`] in string form.
    ///
    /// `Ok(None)` as for [`pipeline_settings`](Source::pipeline_settings).
    ///
    /// [`EntityRef`]: knobas_spi_types::entity::EntityRef
    async fn run_chain(
        &self,
        run: &str,
    ) -> Result<Option<Vec<pipeline::RunChainStep>>, SourceError> {
        let _ = run;
        Ok(None)
    }
    /// A range of run `entity`'s whole log, and its steps (ADR-0022, #854):
    /// `count` lines (capped at [`run::LOG_LINES_MAX`]) from line `from`
    /// (from 1), or the last `count` lines when `from` is `None`. The log is
    /// streamed and never held whole.
    ///
    /// `Ok(None)` is **this adapter keeps no runs**, and is the default, as
    /// for [`pipeline_settings`](Source::pipeline_settings).
    async fn run_log(
        &self,
        entity: &str,
        from: Option<u64>,
        count: u32,
    ) -> Result<Option<run::RunLog>, SourceError> {
        let _ = (entity, from, count);
        Ok(None)
    }
    /// Run `entity`'s tests, failed ones first (ADR-0022, #855).
    /// `Ok(None)` as for [`run_log`](Source::run_log).
    async fn run_tests(&self, entity: &str) -> Result<Option<run::RunTests>, SourceError> {
        let _ = entity;
        Ok(None)
    }
    /// How the test `test` (a [`run::RunTest::id`]) came out in the last
    /// [`run::TEST_HISTORY_RUNS`] runs of run `entity`'s pipeline, newest
    /// first (#855). `Ok(None)` as for [`run_log`](Source::run_log).
    async fn test_history(
        &self,
        entity: &str,
        test: &str,
    ) -> Result<Option<Vec<run::TestOutcome>>, SourceError> {
        let _ = (entity, test);
        Ok(None)
    }
    /// The entries of one folder of run `entity`'s artifacts, `""` for the
    /// root (ADR-0022, #856). `Ok(None)` as for [`run_log`](Source::run_log).
    async fn run_artifacts(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<Vec<run::ArtifactEntry>>, SourceError> {
        let _ = (entity, path);
        Ok(None)
    }
    /// The head of one artifact file of run `entity`, for *Preview* (#856).
    /// `Ok(None)` as for [`run_log`](Source::run_log).
    async fn artifact_preview(
        &self,
        entity: &str,
        path: &str,
    ) -> Result<Option<run::ArtifactPreview>, SourceError> {
        let _ = (entity, path);
        Ok(None)
    }
    /// A saved view's query read into knobas' search grammar (#1208,
    /// ADR-0029): the conditions knobas can test and the parts left out, each
    /// with its reason, for *Make an editable copy*. `view` is the saved
    /// view's payload as this adapter emitted it (a kind declaring
    /// [`Frame::SavedView`](knobas_spi_types::payload::Frame::SavedView)), so the
    /// source's query language stays inside the adapter.
    ///
    /// `Ok(None)` is **this adapter translates no query**, and is the
    /// default: such a source offers no editable copy, and its saved views
    /// still show. An adapter that answers it claims the ticket-system read
    /// [`role::TRANSLATE_QUERY`]. Pure: it reads nothing from the source.
    async fn translate_query(
        &self,
        view: &serde_json::Value,
    ) -> Result<Option<query::Translation>, SourceError> {
        let _ = view;
        Ok(None)
    }
    /// Stream `target` of run `entity` into `sink` with the stored credential
    /// (D8, #856): read from the source, nothing written to it. `Ok(false)`
    /// is **this adapter keeps no runs**, and is the default.
    async fn download(
        &self,
        entity: &str,
        target: &run::DownloadTarget,
        sink: &mut dyn run::DownloadSink,
    ) -> Result<bool, SourceError> {
        let _ = (entity, target, sink);
        Ok(false)
    }
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

    /// Say how far the run's outer walk has got: `done` units of `of`, where
    /// `of` is `None` when the adapter cannot know the total, and `unit` is a
    /// plural noun the UI prints as is ("repositories"). Issue #1379.
    ///
    /// An adapter calls it before each unit and once after the last, so a run
    /// that pushes no item still moves a progress bar. It is a reading, not an
    /// item: it cannot fail the sync, and an adapter that never calls it
    /// changes nothing -- the engine reports its items as before.
    ///
    /// The default does nothing, so [`contract::VecSink`] and every sink that
    /// predates it need no change. A sink that wraps another forwards it.
    async fn walked(&mut self, done: u64, of: Option<u64>, unit: &str) {
        let _ = (done, of, unit);
    }

    /// Say that this run emitted every item of `kind` the source's current
    /// scope holds, as a run handed no cursor would. Issue #2186.
    ///
    /// The engine reconciles a kind only after a run that saw all of it, and
    /// it knows that for itself only when it handed the adapter no cursor. An
    /// adapter that was handed one and walked a kind whole anyway (its scope
    /// changed, so the position no longer applied, #2181) says so here, and
    /// the engine then sweeps that kind as after a cursor-less run: what the
    /// run did not emit is tombstoned. Everything else still gates the sweep
    /// (ADR-0003): the kind's `full_sync_exhaustive`, the run having emitted
    /// something of it, and the run not being a backfill.
    ///
    /// It is a declaration, not a read: the sink still answers nothing, so an
    /// adapter cannot ask what the mirror holds. Call it only for a walk that
    /// ended, and only when the run returns `Ok`; a failed run sweeps nothing.
    /// Saying it of a walk that stopped early tombstones what it never reached.
    ///
    /// The default does nothing, so a sink that ignores it leaves the run
    /// incremental. A sink that wraps another forwards it.
    async fn walked_whole(&mut self, kind: &str) {
        let _ = kind;
    }
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
            accepts_account: false,
            min_sync_interval_secs: 60,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            code_pages: None,
            write_ops: vec!["comment".into()],
            entity_kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "JI".into(),
                full_sync_exhaustive: true,
                refreshes: true,
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

    /// The sync-interval floor (#1187) crosses the hop as declared, and a
    /// descriptor from a peer that predates the field decodes with the 60 s
    /// floor every source had before adapters could declare one.
    #[test]
    fn the_interval_floor_round_trips_and_defaults_to_sixty_seconds() {
        let declared = SourceDescriptor {
            min_sync_interval_secs: 20,
            roles: Vec::new(),
            source_actions: Vec::new(),
            clone_from: Vec::new(),
            url_forms: Vec::new(),
            code_pages: None,
            ..a_descriptor()
        };
        let mut wire = serde_json::to_value(&declared).unwrap();
        assert_eq!(wire["min_sync_interval_secs"], 20);
        let back: SourceDescriptor = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(back.min_sync_interval_secs, 20);

        wire.as_object_mut()
            .unwrap()
            .remove("min_sync_interval_secs");
        let older: SourceDescriptor = serde_json::from_value(wire).unwrap();
        assert_eq!(older.min_sync_interval_secs, 60);
    }

    /// A kind's refresh flag (#2146) crosses as declared, a descriptor from
    /// a peer that predates it reads every kind refreshed, and a kind is
    /// refreshed only under `Capability::Refresh`.
    #[test]
    fn a_kinds_refresh_flag_round_trips_defaults_on_and_needs_the_capability() {
        let mut declared = SourceDescriptor {
            capabilities: vec![Capability::Refresh],
            ..a_descriptor()
        };
        let mut config = declared.entity_kinds[0].clone();
        config.id = "config".into();
        config.refreshes = false;
        declared.entity_kinds.push(config);
        let first = declared.entity_kinds[0].id.clone();
        assert!(declared.refreshes_kind(&first));
        assert!(!declared.refreshes_kind("config"));
        assert!(!declared.refreshes_kind("not-declared"));

        let mut wire = serde_json::to_value(&declared).unwrap();
        let back: SourceDescriptor = serde_json::from_value(wire.clone()).unwrap();
        assert!(back.refreshes_kind(&first) && !back.refreshes_kind("config"));

        for kind in wire["entity_kinds"].as_array_mut().unwrap() {
            kind.as_object_mut().unwrap().remove("refreshes");
        }
        let older: SourceDescriptor = serde_json::from_value(wire).unwrap();
        assert!(older.refreshes_kind(&first) && older.refreshes_kind("config"));

        let without = SourceDescriptor {
            capabilities: Vec::new(),
            ..older
        };
        assert!(!without.refreshes_kind(&first));
    }

    /// An external adapter's descriptor crosses the WIT boundary as this
    /// JSON (#1475): one that declares forms keeps them, and one built before
    /// the slot existed, which sends no `url_forms` key, still decodes, as an
    /// adapter whose items open by their stored address alone.
    #[test]
    fn url_forms_round_trip_and_a_descriptor_without_them_still_decodes() {
        let declared = SourceDescriptor {
            url_forms: vec![url_forms::UrlForm::of(
                "ticket",
                "/browse/{}",
                [PayloadPath::of(["key"])],
            )],
            ..a_descriptor()
        };
        let mut wire = serde_json::to_value(&declared).unwrap();
        let back: SourceDescriptor = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(back.url_forms, declared.url_forms);

        wire.as_object_mut().unwrap().remove("url_forms");
        let older: SourceDescriptor = serde_json::from_value(wire).unwrap();
        assert!(older.url_forms.is_empty());
    }

    /// A kind's done-like set crosses the SPI beside its blocked-like one
    /// (#1501): a descriptor that declares it keeps it, and one built before
    /// the field existed, which sends no `done_statuses` key, decodes as a
    /// source that calls nothing done, so it archives nothing.
    #[test]
    fn done_statuses_round_trip_and_a_descriptor_without_them_declares_none() {
        let declared = SourceDescriptor {
            payload_paths: vec![KindPaths {
                kind: "ticket".to_owned(),
                blocked_statuses: vec!["Blocked".to_owned()],
                done_statuses: vec!["Done".to_owned(), "Won't Do".to_owned()],
                ..KindPaths::default()
            }],
            ..a_descriptor()
        };
        let mut wire = serde_json::to_value(&declared).unwrap();
        assert_eq!(
            wire["payload_paths"][0]["done_statuses"],
            serde_json::json!(["Done", "Won't Do"])
        );
        let back: SourceDescriptor = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(back.payload_paths, declared.payload_paths);

        wire["payload_paths"][0]
            .as_object_mut()
            .unwrap()
            .remove("done_statuses");
        let older: SourceDescriptor = serde_json::from_value(wire).unwrap();
        assert!(older.payload_paths[0].done_statuses.is_empty());
        assert_eq!(older.payload_paths[0].blocked_statuses, ["Blocked"]);
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
            entity: knobas_spi_types::entity::EntityRef::new("jira", "PAY-231"),
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
            WriteOp::CreatePage {
                parent: "confluence:98400".into(),
                space: "ENG".into(),
                title: "Standup 2026-09-03".into(),
                body: "<p>nothing blocked</p>".into(),
            },
            WriteOp::UpdatePage {
                entity: "confluence:98307".into(),
                base_version: 3,
                body: "<h2>Backoff policy</h2><p>base 30 s.</p>".into(),
            },
            WriteOp::PauseMonitor {
                entity: "kuma:8".into(),
            },
            WriteOp::ResumeMonitor {
                entity: "kuma:8".into(),
            },
            WriteOp::CreateMonitor {
                entity: monitor_target("kuma"),
                name: "gitea".into(),
                url: "http://gitea:3000/api/healthz".into(),
                kind: crate::MonitorKind::Http,
            },
            WriteOp::MergePullRequest {
                entity: "gitea:tidewater/payout-service#142".into(),
                style: MergeStyle::Squash,
                delete_branch: true,
                message: Some("Idempotent retry window (#142)\n\nRetries on 503.".into()),
            },
            WriteOp::SetParameter {
                entity: "teamcity:buildType:Payout_Build".into(),
                name: "env.SEPA_RETRIES".into(),
                value: "7".into(),
                base: Some("5".into()),
            },
            WriteOp::RunBuild {
                entity: "teamcity:buildType:Payout_Build".into(),
                branch: Some("feature/PAY-231-sepa-retry".into()),
                parameters: vec![BuildParameter {
                    name: "env.SEPA_RETRIES".into(),
                    value: "9".into(),
                }],
                clean_checkout: true,
                first_in_queue: false,
            },
            WriteOp::SubmitReview {
                entity: "gitea:tidewater/payout-service#142".into(),
                verdict: ReviewVerdict::RequestChanges,
                body: "the jitter is unbounded".into(),
                comments: vec![ReviewComment {
                    path: "src/retry.rs".into(),
                    line: 12,
                    side: ReviewSide::New,
                    body: "bound this".into(),
                }],
            },
            WriteOp::DeleteBranch {
                entity: "gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry"
                    .into(),
            },
            WriteOp::EditPullRequest {
                entity: "gitea:tidewater/payout-service#142".into(),
                title: Some("Bound the retry jitter".into()),
                body: Some("as discussed".into()),
                state: Some(PullRequestState::Closed),
            },
            WriteOp::UpdatePullRequestBranch {
                entity: "gitea:tidewater/payout-service#142".into(),
                style: UpdateStyle::Rebase,
            },
            WriteOp::CommitFile {
                repo: "gitea:tidewater/payout-service".into(),
                branch: "main".into(),
                new_branch: Some("feature/PAY-231-retry-doc".into()),
                path: "docs/retry.md".into(),
                content: "# Retry\n\nThree times, then the dead-letter queue.\n".into(),
                message: "PAY-231: say how often a payout is retried".into(),
                base_sha: "986f74b729a686f111e61e07635d01e9bf265198".into(),
            },
            WriteOp::SourceAction {
                entity: "gitea:tidewater/payout-service#142".into(),
                id: "add_label".into(),
                fields: [("label".to_owned(), "7".to_owned())].into(),
            },
            WriteOp::ReplyToThread {
                entity: "gitea:tidewater/payout-service#142".into(),
                thread: "4088".into(),
                comment: ReviewComment {
                    path: "src/retry.rs".into(),
                    line: 27,
                    side: ReviewSide::New,
                    body: "Done in 7aa01f9.".into(),
                },
            },
            WriteOp::ResolveThread {
                entity: "gitea:tidewater/payout-service#142".into(),
                thread: "4088".into(),
                resolved: true,
            },
            WriteOp::RequestReviewers {
                entity: "gitea:tidewater/payout-service#142".into(),
                reviewers: vec!["olu".into(), "kai".into()],
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
                | WriteOp::LogWork { .. }
                | WriteOp::CreatePage { .. }
                | WriteOp::UpdatePage { .. }
                | WriteOp::PauseMonitor { .. }
                | WriteOp::ResumeMonitor { .. }
                | WriteOp::CreateMonitor { .. }
                | WriteOp::MergePullRequest { .. }
                | WriteOp::SetParameter { .. }
                | WriteOp::RunBuild { .. }
                | WriteOp::SubmitReview { .. }
                | WriteOp::DeleteBranch { .. }
                | WriteOp::EditPullRequest { .. }
                | WriteOp::UpdatePullRequestBranch { .. }
                | WriteOp::CommitFile { .. }
                | WriteOp::SourceAction { .. }
                | WriteOp::ReplyToThread { .. }
                | WriteOp::ResolveThread { .. }
                | WriteOp::RequestReviewers { .. } => {}
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

    /// A source action (#1194, ADR-0029) crosses as its id and its fields,
    /// the dialog's answers by name, and nothing knobas reads: the wire shape
    /// is pinned here because the shell builds it and the adapter reads it,
    /// and neither may guess the other's spelling. Absent `fields` reads as
    /// none, an action with no dialog.
    #[test]
    fn a_source_action_is_its_id_and_its_fields_by_name() {
        let op = WriteOp::SourceAction {
            entity: "gitea:o/r#1".into(),
            id: "add_label".into(),
            fields: [("label".to_owned(), "7".to_owned())].into(),
        };
        assert_eq!(op.identifier(), "source_action");
        let wire = serde_json::to_value(&op).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({
                "SourceAction": { "entity": "gitea:o/r#1", "id": "add_label", "fields": { "label": "7" } }
            })
        );
        let back: WriteOp = serde_json::from_value(wire).unwrap();
        assert_eq!(
            serde_json::to_value(&back).unwrap()["SourceAction"]["fields"]["label"],
            "7"
        );

        let bare: WriteOp = serde_json::from_value(serde_json::json!({
            "SourceAction": { "entity": "gitea:o/r#1", "id": "convert_to_draft" }
        }))
        .unwrap();
        let WriteOp::SourceAction { fields, .. } = bare else {
            panic!("{bare:?}")
        };
        assert!(fields.is_empty());
    }

    /// A descriptor's source actions (#1194) cross the hop as declared, and
    /// one from a peer that predates them decodes as declaring none.
    #[test]
    fn declared_source_actions_round_trip_and_default_to_none() {
        let declared = SourceDescriptor {
            source_actions: vec!["add_label".into()],
            ..a_descriptor()
        };
        let mut wire = serde_json::to_value(&declared).unwrap();
        assert_eq!(wire["source_actions"], serde_json::json!(["add_label"]));
        let back: SourceDescriptor = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(back.source_actions, ["add_label"]);
        wire.as_object_mut().unwrap().remove("source_actions");
        let older: SourceDescriptor = serde_json::from_value(wire).unwrap();
        assert!(older.source_actions.is_empty());
    }

    /// A merge's style crosses as the lowercase word a view's merge action
    /// offers as its option, and a word outside the three does not decode --
    /// so `submit_write` refuses it rather than a source (issue #667).
    #[test]
    fn a_merge_style_is_its_lowercase_word_and_nothing_else_decodes() {
        for (style, word) in [
            (MergeStyle::Merge, "merge"),
            (MergeStyle::Rebase, "rebase"),
            (MergeStyle::Squash, "squash"),
        ] {
            assert_eq!(style.as_str(), word);
            let op = WriteOp::MergePullRequest {
                entity: "gitea:o/r#1".into(),
                style,
                delete_branch: false,
                message: None,
            };
            assert_eq!(
                serde_json::to_value(&op).unwrap(),
                serde_json::json!({
                    "MergePullRequest": { "entity": "gitea:o/r#1", "style": word, "delete_branch": false }
                })
            );
        }
        let odd = serde_json::from_value::<WriteOp>(serde_json::json!({
            "MergePullRequest": { "entity": "gitea:o/r#1", "style": "fast-forward-only" }
        }));
        assert!(odd.is_err(), "{odd:?}");
    }

    /// A merge queued before *Delete branch after merge* existed (#708) has no
    /// `delete_branch` in its JSON; it still reads, as a merge that keeps the
    /// branch -- what it meant when it was queued.
    #[test]
    fn a_merge_queued_before_the_delete_flag_reads_as_one_that_keeps_the_branch() {
        let old = serde_json::json!({
            "MergePullRequest": { "entity": "gitea:o/r#1", "style": "squash" }
        });
        let read = serde_json::from_value::<WriteOp>(old).unwrap();
        assert!(
            matches!(
                &read,
                WriteOp::MergePullRequest { entity, style: MergeStyle::Squash, delete_branch: false, message: None }
                    if entity == "gitea:o/r#1"
            ),
            "{read:?}"
        );
        let new = WriteOp::MergePullRequest {
            entity: "gitea:o/r#1".into(),
            style: MergeStyle::Merge,
            delete_branch: true,
            message: None,
        };
        assert_eq!(
            serde_json::to_value(&new).unwrap(),
            serde_json::json!({
                "MergePullRequest": { "entity": "gitea:o/r#1", "style": "merge", "delete_branch": true }
            })
        );
    }

    /// A create queued before monitors had kinds (#864) has no `kind` in its
    /// JSON; it still reads as the HTTP check it was. A kind crosses as its
    /// lowercase word, and a word knobas has no check for does not decode.
    #[test]
    fn a_create_queued_before_kinds_reads_as_an_http_check() {
        let old = serde_json::json!({
            "CreateMonitor": { "entity": "kuma:monitors", "name": "gitea", "url": "http://gitea:3000/" }
        });
        let read = serde_json::from_value::<WriteOp>(old).unwrap();
        assert!(
            matches!(
                &read,
                WriteOp::CreateMonitor {
                    kind: MonitorKind::Http,
                    ..
                }
            ),
            "{read:?}"
        );
        let new = WriteOp::CreateMonitor {
            entity: "kuma:monitors".into(),
            name: "pg".into(),
            url: "db.internal:5432".into(),
            kind: MonitorKind::Tcp,
        };
        assert_eq!(
            serde_json::to_value(&new).unwrap(),
            serde_json::json!({
                "CreateMonitor": {
                    "entity": "kuma:monitors", "name": "pg", "url": "db.internal:5432", "kind": "tcp"
                }
            })
        );
        for (word, kind) in [
            ("http", MonitorKind::Http),
            ("tcp", MonitorKind::Tcp),
            ("ping", MonitorKind::Ping),
            ("docker", MonitorKind::Docker),
        ] {
            assert_eq!(serde_json::to_value(kind).unwrap(), serde_json::json!(word));
            assert_eq!(kind.as_str(), word);
        }
        assert!(serde_json::from_value::<MonitorKind>(serde_json::json!("cert")).is_err());
    }

    /// An update's style crosses as `merge` or `rebase` and nothing else
    /// decodes (#708): Gitea's `?style=` takes exactly those two.
    #[test]
    fn an_update_style_is_merge_or_rebase_and_nothing_else_decodes() {
        for (style, word) in [
            (UpdateStyle::Merge, "merge"),
            (UpdateStyle::Rebase, "rebase"),
        ] {
            assert_eq!(style.as_str(), word);
            let op = WriteOp::UpdatePullRequestBranch {
                entity: "gitea:o/r#1".into(),
                style,
            };
            assert_eq!(op.identifier(), "update_pull_request_branch");
            assert_eq!(
                serde_json::to_value(&op).unwrap(),
                serde_json::json!({ "UpdatePullRequestBranch": { "entity": "gitea:o/r#1", "style": word } })
            );
        }
        let odd = serde_json::from_value::<WriteOp>(serde_json::json!({
            "UpdatePullRequestBranch": { "entity": "gitea:o/r#1", "style": "squash" }
        }));
        assert!(odd.is_err(), "{odd:?}");
    }

    /// A file commit's wire shape (issue #822), pinned: the frontend builds it
    /// by hand (#845), so a renamed field is a commit that no longer decodes
    /// at `submit_write`. `new_branch` is `null` for a commit straight onto
    /// `branch`, and absent reads the same.
    #[test]
    fn a_commit_file_crosses_in_its_own_shape() {
        let op = WriteOp::CommitFile {
            repo: "gitea:o/r".into(),
            branch: "main".into(),
            new_branch: None,
            path: "docs/a.md".into(),
            content: "mine\n".into(),
            message: "edit a".into(),
            base_sha: "abc123".into(),
        };
        assert_eq!(op.identifier(), "commit_file");
        assert_eq!(
            serde_json::to_value(&op).unwrap(),
            serde_json::json!({ "CommitFile": {
                "repo": "gitea:o/r", "branch": "main", "new_branch": null,
                "path": "docs/a.md", "content": "mine\n", "message": "edit a",
                "base_sha": "abc123"
            } })
        );
        let read = serde_json::from_value::<WriteOp>(serde_json::json!({ "CommitFile": {
            "repo": "gitea:o/r", "branch": "main", "path": "docs/a.md",
            "content": "mine\n", "message": "edit a", "base_sha": "abc123"
        } }))
        .unwrap();
        assert!(
            matches!(
                &read,
                WriteOp::CommitFile {
                    new_branch: None,
                    ..
                }
            ),
            "{read:?}"
        );
    }

    /// A target that moved under a write crosses as data (#822): the version
    /// the source holds now and its text, both, because the queue holds the
    /// write with them and the reader is shown the text.
    #[test]
    fn a_moved_target_crosses_with_what_the_source_holds_now() {
        let moved = SourceError::Moved {
            base: "da0c4e".into(),
            current: "theirs\n".into(),
        };
        let json = serde_json::to_value(&moved).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "Moved": { "base": "da0c4e", "current": "theirs\n" } })
        );
        let back: SourceError = serde_json::from_value(json).unwrap();
        assert!(
            matches!(&back, SourceError::Moved { base, current } if base == "da0c4e" && current == "theirs\n"),
            "{back:?}"
        );
        assert_eq!(back.status(), None);
        assert_eq!(
            back.to_string(),
            "moved: the target changed since the write was made against it"
        );
    }

    /// A once-sent write that may have landed crosses as data (#1125), with
    /// the gateway's status where there was one, and decodes from a peer that
    /// sends none.
    #[test]
    fn an_unconfirmed_write_crosses_with_its_status() {
        let busy = SourceError::Unconfirmed {
            status: Some(504),
            message: "gateway timeout".into(),
        };
        let json = serde_json::to_value(&busy).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "Unconfirmed": { "status": 504, "message": "gateway timeout" } })
        );
        let back: SourceError = serde_json::from_value(json).unwrap();
        assert_eq!(back.status(), Some(504));
        assert_eq!(back.to_string(), "unconfirmed: gateway timeout");

        let older: SourceError = serde_json::from_value(
            serde_json::json!({ "Unconfirmed": { "message": "timed out" } }),
        )
        .unwrap();
        assert!(
            matches!(&older, SourceError::Unconfirmed { status: None, message } if message == "timed out"),
            "{older:?}"
        );
    }

    /// A custom run's wire shape (issue #669), pinned: the frontend builds it
    /// by hand (`app/src/lib/detail/view/actions.ts`), so a renamed field
    /// here is a run that no longer decodes at `submit_write`. No branch is
    /// `null`, and the overrides are `{name, value}` objects in order.
    #[test]
    fn a_run_crosses_as_its_branch_and_its_overrides_in_order() {
        let op = WriteOp::RunBuild {
            entity: "teamcity:buildType:Payout_Build".into(),
            branch: None,
            parameters: vec![
                BuildParameter {
                    name: "env.SEPA_RETRIES".into(),
                    value: "9".into(),
                },
                BuildParameter {
                    name: "tidewater.region".into(),
                    value: "".into(),
                },
            ],
            clean_checkout: true,
            first_in_queue: false,
        };
        assert_eq!(
            serde_json::to_value(&op).unwrap(),
            serde_json::json!({ "RunBuild": {
                "entity": "teamcity:buildType:Payout_Build",
                "branch": null,
                "parameters": [
                    { "name": "env.SEPA_RETRIES", "value": "9" },
                    { "name": "tidewater.region", "value": "" }
                ],
                "clean_checkout": true,
                "first_in_queue": false
            } })
        );
        let back: WriteOp = serde_json::from_value(serde_json::json!({ "RunBuild": {
            "entity": "teamcity:buildType:Payout_Build",
            "branch": "main",
            "parameters": []
        } }))
        .unwrap();
        assert!(
            matches!(&back, WriteOp::RunBuild {
                branch: Some(b), parameters, clean_checkout: false, first_in_queue: false, ..
            } if b == "main" && parameters.is_empty()),
            "a payload from before the run options reads as neither (#852): {back:?}"
        );
    }

    /// A review's wire shape (issue #705), pinned: the frontend builds it by
    /// hand (`app/src/lib/detail/view/actions.ts`). The verdict and a line
    /// comment's side are snake_case words, and a payload sent without
    /// `comments` -- every review before #709 -- decodes as one with none.
    #[test]
    fn a_review_crosses_as_its_verdict_body_and_line_comments() {
        for (verdict, word) in [
            (ReviewVerdict::Approve, "approve"),
            (ReviewVerdict::RequestChanges, "request_changes"),
            (ReviewVerdict::Comment, "comment"),
        ] {
            let op = WriteOp::SubmitReview {
                entity: "gitea:o/r#1".into(),
                verdict,
                body: "b".into(),
                comments: vec![ReviewComment {
                    path: "a.rs".into(),
                    line: 3,
                    side: ReviewSide::Old,
                    body: "c".into(),
                }],
            };
            assert_eq!(
                serde_json::to_value(&op).unwrap(),
                serde_json::json!({ "SubmitReview": {
                    "entity": "gitea:o/r#1",
                    "verdict": word,
                    "body": "b",
                    "comments": [{ "path": "a.rs", "line": 3, "side": "old", "body": "c" }]
                } })
            );
        }
        let back: WriteOp = serde_json::from_value(serde_json::json!({ "SubmitReview": {
            "entity": "gitea:o/r#1", "verdict": "approve", "body": ""
        } }))
        .unwrap();
        assert!(
            matches!(&back, WriteOp::SubmitReview { verdict: ReviewVerdict::Approve, comments, .. }
                if comments.is_empty()),
            "{back:?}"
        );
        let odd = serde_json::from_value::<WriteOp>(serde_json::json!({ "SubmitReview": {
            "entity": "gitea:o/r#1", "verdict": "APPROVED", "body": ""
        } }));
        assert!(
            odd.is_err(),
            "Gitea's spelling is the adapter's, not the SPI's: {odd:?}"
        );
    }

    /// An edit's wire shape (issue #706), pinned: the frontend builds it by
    /// hand (`app/src/lib/detail/view/actions.ts`). A field left alone is
    /// `null` -- and absent reads the same -- so the adapter sends only what
    /// was given; the state is the lowercase word.
    #[test]
    fn an_edit_crosses_as_only_the_fields_it_changes() {
        let op = WriteOp::EditPullRequest {
            entity: "gitea:o/r#1".into(),
            title: Some("t".into()),
            body: None,
            state: Some(PullRequestState::Closed),
        };
        assert_eq!(
            serde_json::to_value(&op).unwrap(),
            serde_json::json!({ "EditPullRequest": {
                "entity": "gitea:o/r#1", "title": "t", "body": null, "state": "closed"
            } })
        );
        let back: WriteOp = serde_json::from_value(serde_json::json!({ "EditPullRequest": {
            "entity": "gitea:o/r#1", "state": "open"
        } }))
        .unwrap();
        assert!(
            matches!(
                &back,
                WriteOp::EditPullRequest {
                    title: None,
                    body: None,
                    state: Some(PullRequestState::Open),
                    ..
                }
            ),
            "{back:?}"
        );
        let merged = serde_json::from_value::<WriteOp>(serde_json::json!({ "EditPullRequest": {
            "entity": "gitea:o/r#1", "state": "merged"
        } }));
        assert!(
            merged.is_err(),
            "merging is merge_pull_request's, never an edit's: {merged:?}"
        );
        // The word a view's action fixes is the word the wire carries.
        for state in [PullRequestState::Open, PullRequestState::Closed] {
            assert_eq!(serde_json::to_value(state).unwrap(), state.as_str());
        }
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
        assert_eq!(seen.len(), 26, "a variant lost its probe in every_write_op");
    }

    /// A create's target is an entity id, and it is one no monitor can also
    /// carry.
    ///
    /// Both halves matter. The first is what makes a `create_monitor`
    /// submittable at all: `submit_write` parses the target and routes the
    /// write by its namespace, so a target that did not parse would be an op
    /// nothing could ever deliver. The second is what makes it safe to be a
    /// *literal* key: an adapter's monitor ids are the source's own, Uptime
    /// Kuma's are decimal, and a collision would be a create ordered behind
    /// one monitor's writes and held against that monitor leaving the mirror.
    #[test]
    fn the_create_target_is_not_a_monitor_id() {
        let target = monitor_target("kuma");
        assert_eq!(target, "kuma:monitors");
        let parsed =
            knobas_spi_types::entity::EntityRef::parse(&target).expect("the target parses");
        assert_eq!(parsed.namespace, "kuma");
        assert_eq!(parsed.key, MONITOR_TARGET_KEY);
        assert!(
            MONITOR_TARGET_KEY.parse::<i64>().is_err(),
            "a decimal key could be a monitor id"
        );
        // The namespace is the source id verbatim, which is what routes the
        // write: a second Kuma addresses its own and never the first's.
        assert_eq!(monitor_target("kuma-eu"), "kuma-eu:monitors");
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
