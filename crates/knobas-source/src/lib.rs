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
    pub entity_kinds: Vec<KindInfo>,
    /// Whether a `cursor: None` sync emits this source's **complete** current
    /// corpus.
    ///
    /// This is the precondition of the full-sync sweep (interfaces §4.1): after
    /// an exhaustive full sync, every row of this source whose `synced_at`
    /// predates the run is an item the source stopped returning, and the engine
    /// tombstones it -- which is the only way a hard delete upstream ever
    /// reaches knobas.
    ///
    /// `false` says the full sync is a *window*, not the world: TeamCity emits
    /// the newest N builds per configuration, so a sweep after it would
    /// tombstone the entire build history on every run. For such a source
    /// vanished items are never swept, and the mirror keeps what it last saw.
    ///
    /// M1: mock `true`, Jira `true`, Gitea `true`, TeamCity `false`. It is a
    /// claim the adapter makes about its own read path -- the battery cannot
    /// check it without knowing the remote corpus, so the adapter's own
    /// integration tests are what hold it honest.
    pub full_sync_exhaustive: bool,
    /// JSON Schema for this adapter's configuration; the Add-source form is
    /// generated from it (spec §3a). Never holds secrets -- those live in the
    /// OS keychain, keyed by the chosen [`AuthMethod`]. M0: the mock declares
    /// an empty object schema.
    pub config_schema: serde_json::Value,
}

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
}

/// Display metadata for one entity kind an adapter emits.
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
/// Every variant carries at most a string, so the error crosses a process
/// boundary as plain data (spec §3a). The serde form is structural, not the
/// [`Display`](std::fmt::Display) text thiserror generates: `Unauthorized`
/// round-trips as the bare variant, the others as `{"Unreachable": "<detail>"}`.
#[derive(Debug, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub enum SourceError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("unreachable: {0}")]
    Unreachable(String),
    #[error("protocol: {0}")]
    Protocol(String),
    /// The [`Sink`] rejected an item and the sync was abandoned. Raised by the
    /// sink, propagated -- never manufactured -- by the adapter.
    #[error("sink: {0}")]
    Sink(String),
}

/// A write knobas asks an adapter to perform on the remote system.
///
/// Each variant has a stable snake_case identifier that adapters list in
/// [`SourceDescriptor::write_ops`] and the UI renders its action bar from:
///
/// | variant | identifier |
/// | --- | --- |
/// | [`Comment`](Self::Comment) | `"comment"` |
///
/// The enum grows per milestone; an adapter must reject every op it does not
/// declare with [`SourceError::Protocol`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum WriteOp {
    /// Identifier `"comment"`. `entity` is an [`EntityRef`] in string form.
    ///
    /// [`EntityRef`]: knobas_core::entity::EntityRef
    Comment { entity: String, body: String },
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
    /// [`SourceError::Protocol`].
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
    async fn write(&self, op: WriteOp) -> Result<(), SourceError>;
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
            }],
            full_sync_exhaustive: true,
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
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
        assert_eq!(v["full_sync_exhaustive"], true);
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
        // The sweep's precondition travels with the descriptor: a flag that
        // did not survive the hop would default to "sweep it" downstream.
        assert_eq!(back.full_sync_exhaustive, d.full_sync_exhaustive);
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
        let back: SourceError = serde_json::from_value(serde_json::json!("Unauthorized")).unwrap();
        assert_eq!(back.to_string(), "unauthorized");
        let json = serde_json::to_value(SourceError::Sink("pool closed".into())).unwrap();
        assert_eq!(json, serde_json::json!({ "Sink": "pool closed" }));
        let back: SourceError = serde_json::from_value(json).unwrap();
        assert_eq!(back.to_string(), "sink: pool closed");
    }

    /// Write ops travel adapter-ward, so they round-trip in both directions.
    #[test]
    fn write_op_round_trips() {
        let op = WriteOp::Comment {
            entity: "jira:PAY-231".into(),
            body: "on it".into(),
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: WriteOp = serde_json::from_str(&json).unwrap();
        let WriteOp::Comment { entity, body } = back;
        assert_eq!((entity.as_str(), body.as_str()), ("jira:PAY-231", "on it"));
    }
}
