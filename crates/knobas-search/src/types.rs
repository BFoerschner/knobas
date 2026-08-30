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
    /// **What a source can answer depends on the source** (issue #106): a
    /// TeamCity item carries an author only when a person pressed Run, which
    /// on a real server is effectively never -- 100 of 100 of the newest
    /// finished builds on JetBrains' public instance named no user. So an
    /// author query narrowed to a corpus of builds legitimately comes back
    /// empty, and that is the data being sparse rather than this filter being
    /// broken. Ruled and left as it is on 2026-08-29; making the emptiness
    /// visible in the response is a change to this frozen schema and is
    /// Björn's call, not one taken here.
    ///
    /// [`Self::mine`] is a *separate* dimension and its resolved identity
    /// never appears here: the echo is what the launcher redraws its chips
    /// from, and putting usernames the user never typed into that row would
    /// have the chips claim a filter nobody wrote.
    ///
    /// The one field here carrying `#[serde(default)]`, because it is the one
    /// field that was **added** to a frozen struct (§10.8): a caller written
    /// against the four-field shape sends no `authors`, and refusing its query
    /// outright is a worse answer than reading the absence as "named nobody".
    /// The mirror declares it required, so the launcher always sends it and
    /// `the_response_shape_matches_its_typescript_mirror` keeps that true --
    /// the default is the wire being permissive, not the frontend being
    /// allowed to forget.
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
    /// Which sources could answer the filters this query narrowed by, for the
    /// dimensions where "could not" is a thing that happens (issue #141).
    ///
    /// **Empty means there was nobody to report on**: no reportable dimension
    /// was filtered on -- the ordinary case, and why an ordinary keystroke pays
    /// nothing for this -- or no source put rows in this query's corpus, which
    /// is a scope that matched nothing rather than a filter that could not be
    /// answered. A dimension that *was* filtered on is here with **every**
    /// contributing source listed, whatever each answered, so a reader can tell
    /// "measured, and they all answered" from "not measured". A list pruned to
    /// the failures could not.
    ///
    /// `#[serde(default)]` for the same reason [`SearchFilters::authors`]
    /// carries it: this field was **added** to a frozen struct (§10.8), and a
    /// peer that sends no `coverage` means "nothing to report" rather than a
    /// response worth refusing. The mirror declares it required, so the backend
    /// always sends it.
    #[serde(default)]
    pub coverage: Vec<FilterCoverage>,
}

/// What each source in a query's scope could do with **one** filter dimension.
///
/// One entry per *reported* dimension, and a dimension is reported only when
/// the query actually filtered on it. Issue #141 scopes the behaviour to
/// [`FilterDimension::Author`]; the list shape is what makes a second dimension
/// an addition rather than a reshape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FilterCoverage {
    pub dimension: FilterDimension,
    /// Every source that put rows in this query's corpus, **ordered by id** --
    /// the vocabulary's own order, which is the order the sources list shows.
    ///
    /// A configured source the query's `source:` or kind scope left with
    /// nothing is **not** here, and that is not an omission: it contributed no
    /// corpus, so this dimension is not why it is absent from the results.
    pub sources: Vec<SourceAnswer>,
}

/// A filter dimension whose coverage is reported.
///
/// Deliberately an enum with one variant rather than a bare string: the wire
/// vocabulary is closed, so a UI that branches on it cannot be handed a word
/// nobody defined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterDimension {
    /// `@jonas`, `author:jonas`, an author chip, or `@me` -- everything that
    /// ends up in the one `author = any(...)` predicate.
    Author,
}

/// One source, and what it could do with the dimension it is listed under.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceAnswer {
    /// The configured instance id, which is also `SearchHit.source_id`.
    pub source_id: String,
    /// The name the sources list shows. Carried rather than looked up: the
    /// launcher has `CredentialHealth` per source and that DTO has no name in
    /// it, so a UI that had to say *Buildserver* would otherwise print an id.
    pub display_name: String,
    pub answer: FilterAnswer,
}

/// Whether a source's corpus can answer a filter at all.
///
/// **These two are the whole of issue #141**: collapsing them is the defect
/// this type exists to remove. An `author:` query that comes back empty is
/// *honest* for an [`Answered`](FilterAnswer::Answered) source and
/// *unanswerable* for a [`NoValues`](FilterAnswer::NoValues) one, and nothing
/// on the wire said which until now.
///
/// There is deliberately **no third variant** for a source that contributed no
/// rows to the query at all. Such a source is left out of
/// [`FilterCoverage::sources`] instead: whatever it is missing from the
/// results, the filter is not the reason, and a verdict on it would explain the
/// wrong absence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterAnswer {
    /// The source's corpus carries values this dimension matches on, so an
    /// empty result for it means nobody matched.
    Answered,
    /// The source put rows in this query's corpus and **not one of them**
    /// carries such a value. Measured on the corpus, not declared about the
    /// source: this is why issue #141 was ruled onto the response rather than
    /// onto the descriptor.
    NoValues,
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

    /// The lines between `export interface <name> {` and its closing brace.
    ///
    /// The mirror is one file holding eight interfaces and two functions, so a
    /// search that is not scoped to a declaration is not a search for that
    /// declaration -- see [`declares`].
    fn interface_body<'a>(mirror: &'a str, name: &str) -> &'a str {
        let header = format!("export interface {name} {{");
        let start = mirror
            .find(&header)
            .unwrap_or_else(|| panic!("`{header}` is not in app/src/lib/ipc/search.ts"))
            + header.len();
        let rest = &mirror[start..];
        let end = rest
            .find("\n}")
            .unwrap_or_else(|| panic!("`interface {name}` is never closed"));
        &rest[..end]
    }

    /// Whether `body` **declares** `key` -- a line whose first token is `key:`.
    ///
    /// Not `contains`, and not over the whole file. Both halves are
    /// load-bearing, and these tests had neither until a mutation proved it:
    /// deleting `authors: string[];` from `interface SearchFilters` left every
    /// assertion below green, because `mirror.contains("authors:")` was
    /// satisfied by the `authors: []` inside `noFilters()`'s body sixty lines
    /// further down. A doc comment inside the block does the same for the
    /// field it documents.
    ///
    /// This is verbatim the failure `crates/knobas-app/src/commands/search.rs`
    /// records finding and fixing in its own mirror test, with the same two
    /// helpers. It was fixed there and left standing here; the negative
    /// controls below are what stop a slice quietly ceasing to slice.
    fn declares(body: &str, key: &str) -> bool {
        body.lines()
            .any(|line| line.trim_start().starts_with(&format!("{key}:")))
    }

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

        let body = interface_body(mirror, "SearchHit");
        // The negative control: `hits` is a `ResultGroup` field declared eight
        // lines below this interface's closing brace, so a slice that reaches
        // it is not a slice.
        assert!(
            !declares(body, "hits"),
            "the SearchHit slice reaches ResultGroup.hits, so it is searching \
             more than the declaration:\n{body}"
        );

        for key in &keys {
            assert!(
                declares(body, key),
                "`interface SearchHit` in app/src/lib/ipc/search.ts does not \
                 declare `{key}`, which the Rust type puts on the wire:\n{body}"
            );
        }
        assert!(
            !declares(body, "row"),
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
            // Populated rather than left empty, or the three shapes below are
            // never on the wire for this test to slice against -- an empty
            // `coverage` would let the whole #141 addition be deleted from the
            // mirror with this assertion still green.
            coverage: vec![FilterCoverage {
                dimension: FilterDimension::Author,
                sources: vec![SourceAnswer {
                    source_id: "teamcity".to_owned(),
                    display_name: "Buildserver".to_owned(),
                    answer: FilterAnswer::NoValues,
                }],
            }],
        };

        let wire = serde_json::to_value(&response).expect("a response serializes");

        // The negative control, on the slice the mutation escaped through:
        // `text` is a `ParsedQuery` field twenty lines below `SearchFilters`'
        // closing brace, and `authors` also appears in `noFilters()`'s body
        // further down again. A `SearchFilters` slice that reaches either is
        // not a slice, and a rearrangement that did not fix it would look
        // identical from here.
        let filters_body = interface_body(mirror, "SearchFilters");
        assert!(
            !declares(filters_body, "text"),
            "the SearchFilters slice reaches ParsedQuery.text, so it is \
             searching more than the declaration:\n{filters_body}"
        );

        for (path, value) in [
            ("SearchResponse", &wire),
            ("ParsedQuery", &wire["interpreted"]),
            ("SearchFilters", &wire["interpreted"]["filters"]),
            ("ResultGroup", &wire["groups"][0]),
            ("FilterCoverage", &wire["coverage"][0]),
            ("SourceAnswer", &wire["coverage"][0]["sources"][0]),
        ] {
            let body = interface_body(mirror, path);
            for key in value
                .as_object()
                .unwrap_or_else(|| panic!("{path} is a JSON object"))
                .keys()
            {
                assert!(
                    declares(body, key),
                    "`interface {path}` in app/src/lib/ipc/search.ts does not \
                     declare `{key}`, which the Rust type puts on the wire:\n{body}"
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

        // The two #141 vocabularies are TS unions for the same reason, and the
        // one that matters is `FilterAnswer`: the UI branches on it, and a
        // spelling only one side knows is a branch that never runs.
        for answer in [FilterAnswer::Answered, FilterAnswer::NoValues] {
            let wire = serde_json::to_string(&answer).expect("an answer serializes");
            assert!(
                mirror.contains(&wire),
                "{wire} is missing from app/src/lib/ipc/search.ts"
            );
        }
        let dimension =
            serde_json::to_string(&FilterDimension::Author).expect("a dimension serializes");
        assert!(
            mirror.contains(&dimension),
            "{dimension} is missing from app/src/lib/ipc/search.ts"
        );
    }
}
