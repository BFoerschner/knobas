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
    /// JSON Schema for this adapter's configuration; the Add-source form is
    /// generated from it (spec §3a). Never holds secrets -- those live in the
    /// OS keychain, keyed by the chosen [`AuthMethod`]. M0: the mock declares
    /// an empty object schema.
    pub config_schema: serde_json::Value,
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

/// The adapter interface. One implementation per configured source instance.
#[async_trait::async_trait]
pub trait Source: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;
    async fn test_connection(&self) -> Result<(), SourceError>;
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
