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
//!   [`SourceDescriptor::kinds`] and builds its configuration form from
//!   [`SourceDescriptor::config_schema`]. Nothing downstream is allowed to
//!   carry a hardcoded list of kinds, so a new adapter needs no UI work --
//!   which is why [`contract::battery`] rejects any adapter that emits an item
//!   of a kind it did not declare.
//!
//! Adapters prove they honour the contract by running [`contract::battery`]
//! against themselves in their own test suite.

pub mod contract;

/// Everything knobas needs to know about a configured adapter instance.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SourceDescriptor {
    /// Instance id, e.g. `"jira"`. Doubles as the [`EntityRef`] namespace for
    /// every item this instance emits.
    ///
    /// [`EntityRef`]: knobas_core::entity::EntityRef
    pub id: String,
    /// Adapter kind, e.g. `"jira"`, `"mock"`.
    pub kind: String,
    /// Human-readable name for the source list.
    pub name: String,
    pub capabilities: Vec<Capability>,
    pub adapter_version: String,
    /// Entity kinds this adapter emits, with display metadata -- the UI renders
    /// a new source's items (launcher groups, chips, monograms) from this
    /// alone, never from hardcoded kind lists (spec §3a extensibility).
    pub kinds: Vec<KindInfo>,
    /// JSON Schema for this adapter's configuration; the Add-source form is
    /// generated from it (spec §3a). M0: the mock declares an empty object
    /// schema.
    pub config_schema: serde_json::Value,
}

/// Display metadata for one entity kind an adapter emits.
#[derive(Debug, Clone, serde::Serialize)]
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

/// What an adapter can do beyond plain syncing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Capability {
    Search,
    Write,
    Import,
}

/// One entity pushed across the SPI during a sync.
#[derive(Debug, Clone)]
pub struct SyncItem {
    pub entity: knobas_core::entity::EntityRef,
    /// One of the kind ids the descriptor declares:
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

/// The three failure classes knobas distinguishes; everything else an adapter
/// hits collapses into [`SourceError::Protocol`].
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("unreachable: {0}")]
    Unreachable(String),
    #[error("protocol: {0}")]
    Protocol(String),
}

/// A write knobas asks an adapter to perform on the remote system.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum WriteOp {
    /// `entity` is an [`EntityRef`] in string form.
    ///
    /// [`EntityRef`]: knobas_core::entity::EntityRef
    Comment { entity: String, body: String },
    // grows per milestone; every adapter rejects ops it lacks with Protocol
}

/// The adapter interface. One implementation per configured source instance.
#[async_trait::async_trait]
pub trait Source: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;
    async fn test_connection(&self) -> Result<(), SourceError>;
    /// Push every item changed since `cursor` (None = full sync); return the new cursor.
    async fn sync(
        &self,
        cursor: Option<Cursor>,
        sink: &mut (dyn Sink + Send),
    ) -> Result<Cursor, SourceError>;
    async fn write(&self, op: WriteOp) -> Result<(), SourceError>;
}

/// Where a syncing adapter hands its items. Implemented by the sync engine;
/// [`contract::VecSink`] is the in-memory one the battery uses.
#[async_trait::async_trait]
pub trait Sink {
    async fn item(&mut self, item: SyncItem);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The descriptor is what the UI reads, so it has to survive the IPC hop
    /// as plain data -- including the kind metadata the launcher renders from.
    #[test]
    fn descriptor_serializes_to_plain_json() {
        let d = SourceDescriptor {
            id: "jira".into(),
            kind: "jira".into(),
            name: "Jira".into(),
            capabilities: vec![Capability::Search, Capability::Write],
            adapter_version: "0.1.0".into(),
            kinds: vec![KindInfo {
                id: "ticket".into(),
                label: "Ticket".into(),
                plural: "Tickets".into(),
                monogram: "JI".into(),
            }],
            config_schema: serde_json::json!({ "type": "object", "properties": {} }),
        };
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["capabilities"], serde_json::json!(["Search", "Write"]));
        assert_eq!(v["kinds"][0]["monogram"], "JI");
        assert_eq!(v["config_schema"]["type"], "object");
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
