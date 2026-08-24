//! The frozen search IPC types (interfaces §2.4, ruling P2).
//!
//! One command carrying a query object, not a family of prefix commands: the
//! prefix/alias/`key:value` grammar of §4 is one grammar, and the same parser
//! has to serve M4's saved searches. The response echoes its interpretation so
//! the UI can render the chips it inferred.

use chrono::{DateTime, Utc};

/// What the launcher asks for: the raw box text, plus whatever the UI already
/// knows (chips the user clicked).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchQuery {
    /// Exactly what is in the box, prefixes and all. **The backend parses it**
    /// -- see the module docs.
    pub raw: String,
    pub limit: u32,
    pub filters: SearchFilters,
}

/// The filters a query is narrowed by, whether typed inline or clicked.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SearchFilters {
    pub sources: Vec<String>,
    pub kinds: Vec<String>,
    pub updated_within_days: Option<u32>,
    pub mine: bool,
}

impl SearchFilters {
    /// Whether anything is actually being filtered on.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.kinds.is_empty()
            && self.updated_within_days.is_none()
            && !self.mine
    }
}

/// What the backend made of the raw text -- echoed back so the UI renders the
/// chips it inferred rather than guessing at the same grammar twice.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedQuery {
    /// The search terms with the grammar stripped out.
    pub text: String,
    pub prefix: Option<Prefix>,
    pub filters: SearchFilters,
    /// `key:value` pairs the parser did not recognise, so the UI can say so
    /// instead of silently ignoring them.
    pub unknown_tokens: Vec<String>,
}

/// The launcher's prefixes (§4): `>` `#` `@` `/` `t ` `note:` `list:` `asset:` `?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Prefix {
    Action,
    Ticket,
    Person,
    Source,
    Time,
    Note,
    List,
    Asset,
    Help,
}

/// One answered query.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResponse {
    pub interpreted: ParsedQuery,
    pub groups: Vec<ResultGroup>,
    /// Matches across every kind, before `limit` was applied.
    pub total: u32,
    pub took_ms: u32,
}

/// Results of one entity kind, with the display metadata the launcher renders
/// from (§3a: never a hardcoded kind list).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResultGroup {
    pub kind: String,
    pub label: String,
    pub plural: String,
    pub monogram: String,
    /// Matches of this kind, before `limit` was applied.
    pub total: u32,
    pub hits: Vec<SearchHit>,
}

/// The identity of one result -- everything a row needs before its excerpt.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityRow {
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    pub title: String,
    /// When the source says the item changed; `None` if it never said.
    pub updated_at: Option<DateTime<Utc>>,
    /// When knobas last saw it -- the per-row provenance §4 requires
    /// ("synced 4 min ago").
    pub synced_at: DateTime<Utc>,
}

/// One result: a row, its rank, and the excerpt with the match marked.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    #[serde(flatten)]
    pub row: EntityRow,
    pub rank: f32,
    pub snippet: Vec<Segment>,
}

/// A run of excerpt text, and whether it is part of the match.
///
/// **`text` is raw source text** -- whatever a person typed into a ticket,
/// `<script>` included. Render it as text; the highlighting is the `hit` flag,
/// never markup inside the string (roadmap §4 gotcha 7).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Segment {
    pub text: String,
    pub hit: bool,
}
