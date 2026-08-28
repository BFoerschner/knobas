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
    /// The people the query **named** -- `@jonas`, `author:jonas`, or an
    /// author chip (ruling **E-Q1**).
    ///
    /// A username as the source spells it, matched exactly, because that is
    /// what `sync.item.author` holds and what `mine` already matches against.
    ///
    /// [`Self::mine`] is a *separate* dimension and its resolved identity
    /// never appears here: the echo is what the launcher redraws its chips
    /// from, and putting usernames the user never typed into that row would
    /// have the chips claim a filter nobody wrote.
    #[serde(default)]
    pub authors: Vec<String>,
}

impl SearchFilters {
    /// Whether anything is actually being filtered on.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
            && self.kinds.is_empty()
            && self.updated_within_days.is_none()
            && !self.mine
            && self.authors.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key one hit puts on the wire, in the spelling
    /// `app/src/lib/ipc/search.ts` declares -- and **no `row`**.
    ///
    /// This mirror is the one that needs pinning most, because it is the only
    /// one that is not a literal transcription: `SearchHit.row` carries
    /// `#[serde(flatten)]`, so `EntityRow`'s six fields appear inline and the
    /// TypeScript declares them inline too. Nothing but this test connects the
    /// two. Drop the attribute and the wire grows a nested `row` object while
    /// both files still compile, both still look right, and every hit in the
    /// launcher renders blank.
    ///
    /// Stream E owns this crate from the contract PR on, which is exactly why
    /// the shape is pinned before it is handed over.
    #[test]
    fn the_hit_shape_matches_its_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/search.ts");
        let hit = SearchHit {
            row: EntityRow {
                entity_id: "mock:PAY-231".to_owned(),
                kind: "ticket".to_owned(),
                source_id: "mock".to_owned(),
                title: "Retry failed SEPA payouts".to_owned(),
                updated_at: None,
                synced_at: chrono::Utc::now(),
            },
            rank: 0.5,
            snippet: vec![Segment {
                text: "SEPA".to_owned(),
                hit: true,
            }],
        };

        let wire = serde_json::to_value(&hit).expect("a hit serializes");
        let object = wire.as_object().expect("a hit is a JSON object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "entity_id",
                "kind",
                "rank",
                "snippet",
                "source_id",
                "synced_at",
                "title",
                "updated_at",
            ],
            "the flatten put something unexpected on the wire"
        );

        for key in &keys {
            assert!(
                mirror.contains(&format!("{key}:")),
                "SearchHit.{key} is missing from app/src/lib/ipc/search.ts"
            );
        }
        assert!(
            !mirror.contains("row:"),
            "the TS mirror declares a nested `row`, but `#[serde(flatten)]` inlines it"
        );
    }

    /// The rest of the response, one level down: a renamed field here is a
    /// group that renders with no monogram or a total that reads `undefined`.
    #[test]
    fn the_response_shape_matches_its_typescript_mirror() {
        let mirror = include_str!("../../../app/src/lib/ipc/search.ts");
        let response = SearchResponse {
            interpreted: ParsedQuery {
                text: "sepa".to_owned(),
                prefix: Some(Prefix::Ticket),
                filters: SearchFilters::default(),
                unknown_tokens: vec!["nope".to_owned()],
            },
            groups: vec![ResultGroup {
                kind: "ticket".to_owned(),
                label: "Ticket".to_owned(),
                plural: "Tickets".to_owned(),
                monogram: "TI".to_owned(),
                total: 1,
                hits: Vec::new(),
            }],
            total: 1,
            took_ms: 3,
        };

        let wire = serde_json::to_value(&response).expect("a response serializes");
        for (path, value) in [
            ("SearchResponse", &wire),
            ("ParsedQuery", &wire["interpreted"]),
            ("SearchFilters", &wire["interpreted"]["filters"]),
            ("ResultGroup", &wire["groups"][0]),
        ] {
            for key in value
                .as_object()
                .unwrap_or_else(|| panic!("{path} is a JSON object"))
                .keys()
            {
                assert!(
                    mirror.contains(&format!("{key}:")),
                    "{path}.{key} is missing from app/src/lib/ipc/search.ts"
                );
            }
        }

        // The prefix vocabulary is a TS union, so each spelling must be there.
        for prefix in [
            Prefix::Action,
            Prefix::Ticket,
            Prefix::Person,
            Prefix::Source,
            Prefix::Time,
            Prefix::Note,
            Prefix::List,
            Prefix::Asset,
            Prefix::Help,
        ] {
            let wire = serde_json::to_string(&prefix).expect("a prefix serializes");
            assert!(
                mirror.contains(&wire),
                "{wire} is missing from app/src/lib/ipc/search.ts"
            );
        }
    }
}
