//! The frozen search IPC types (interfaces §2.4, ruling P2).
//!
//! One command carrying a query object, not a family of prefix commands: the
//! prefix/alias/`key:value` grammar of §4 is one grammar, and the same parser
//! has to serve M4's saved searches. The response echoes its interpretation so
//! the UI can render the chips it inferred.

use chrono::{DateTime, Utc};

/// What the launcher asks for: the raw box text, plus whatever the UI already
/// knows (chips the user clicked).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct SearchQuery {
    /// Exactly what is in the box, prefixes and all. **The backend parses it**
    /// -- see the module docs.
    pub raw: String,
    pub limit: u32,
    pub filters: SearchFilters,
}

/// The filters a query is narrowed by, whether typed inline or clicked.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct SearchFilters {
    pub sources: Vec<String>,
    pub kinds: Vec<String>,
    pub updated_within_days: Option<u32>,
    pub mine: bool,
    /// The people the query **named** -- `@jonas`, `author:jonas`, `owner:`,
    /// `by:`, or an author chip (ruling **E-Q1**).
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
    /// The generated TypeScript declares it required, so the launcher always
    /// sends it -- the default is the wire being permissive, not the frontend
    /// being allowed to forget.
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
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
    /// response worth refusing. The generated TypeScript declares it required,
    /// and the backend always sends it.
    #[serde(default)]
    pub coverage: Vec<FilterCoverage>,
}

/// What each source in a query's scope could do with **one** filter dimension.
///
/// One entry per *reported* dimension, and a dimension is reported only when
/// the query actually filtered on it. Issue #141 scopes the behaviour to
/// [`FilterDimension::Author`]; the list shape is what makes a second dimension
/// an addition rather than a reshape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct FilterCoverage {
    pub dimension: FilterDimension,
    /// Every source that put rows in this query's corpus, **ordered by id** --
    /// the vocabulary's own order, which is the order the sources list shows.
    ///
    /// A configured source the query's `source:` or kind scope left with
    /// nothing is **not** here, and that is not an omission: it contributed no
    /// corpus, so this dimension is not why it is absent from the results.
    /// Neither is a **disabled** source — the report's scope is the
    /// vocabulary's, which reads enabled sources only. Its still-mirrored rows
    /// can match a plain search, but the grammar cannot name a source the user
    /// turned off, and this report does not verdict it either.
    pub sources: Vec<SourceAnswer>,
}

/// A filter dimension whose coverage is reported.
///
/// Deliberately an enum rather than a bare string: the wire vocabulary is
/// closed, so a UI that branches on it cannot be handed a word nobody defined.
/// Two variants since #595; a further dimension appends one (#141's entry in
/// §10.8).
///
/// - `author` (since #141): `@jonas`, `author:jonas`, an author chip, or
///   `@me` -- everything that ends up in the one `author = any(...)`
///   predicate.
/// - `under` (ADR-0018, #595): `under:<name>`. Answered by a source some live
///   item of which names a parent (measured on the corpus since #642); a
///   source whose corpus names none -- an Uptime Kuma, whose metrics have no
///   grouping, or a classic Jira with no Epic Link field configured -- has
///   nothing the filter can match, and an empty `under:` there is not an
///   empty repo. The second dimension #141's list shape was built for: an
///   entry and a variant, nothing existing moved.
///
/// The variants are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/search.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
#[serde(rename_all = "snake_case")]
pub enum FilterDimension {
    Author,
    Under,
}

/// One source, and what it could do with the dimension it is listed under.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
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
///
/// - `answered`: the source's corpus carries values this dimension matches
///   on, so an empty result for it means nobody matched -- for `author:`,
///   *nobody by that name*.
/// - `no_values`: the source put rows in this query's corpus and **not one of
///   them** carries such a value -- *this source cannot be asked*. Measured on
///   the corpus, not declared about the source: this is why issue #141 was
///   ruled onto the response rather than onto the descriptor.
///
/// Documented here and not on each variant for [`FilterDimension`]'s reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
#[serde(rename_all = "snake_case")]
pub enum FilterAnswer {
    Answered,
    NoValues,
}

/// Results of one entity kind, with the display metadata the launcher renders
/// from (§3a: never a hardcoded kind list).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
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
///
/// Its fields cross inlined into every [`SearchHit`] (`#[serde(flatten)]`),
/// and the launcher board's `recent` rows cross as the room's
/// `knobas_core::entity_list::EntityRow`, which has the same wire shape.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct EntityRow {
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    pub title: String,
    /// When the source says the item changed, an RFC 3339 timestamp; `None`
    /// if the source never dated the item.
    pub updated_at: Option<DateTime<Utc>>,
    /// When knobas last saw it, an RFC 3339 timestamp -- the per-row
    /// provenance §4 requires ("synced 4 min ago").
    pub synced_at: DateTime<Utc>,
    /// Where this row sits **inside its source**, as one line -- a Confluence
    /// page's ancestor path, `Engineering \u{203a} Payments` (#284, ratified
    /// as a contract §10.8 exception under that ticket's criterion 5).
    ///
    /// Since #596 it is the **parent chain** the row's kind declares
    /// (ADR-0018) -- `owner/repo` for a Gitea branch, pull request or commit
    /// -- drawn by one SQL expression, `knobas_core::path_line_read!`, so the
    /// launcher row and the detail panel cannot disagree about what a path
    /// is. A page's chain is
    /// its `ancestors`, which Confluence declares (#599, and #646 retired the
    /// undeclared ancestor read). `None` for every row whose kind declares no
    /// parent or whose declared parent misses: the ADR-0007 **miss**, so a
    /// launcher row shows no path rather than a wrong one, and nothing
    /// downstream has to ask what kind it is holding.
    ///
    /// `#[serde(default)]`, so a stored or piped row written before this
    /// existed still decodes -- as a row that sits nowhere, which is the same
    /// thing a miss says.
    #[serde(default)]
    pub path: Option<String>,
    /// The item's state chip (#666) -- a pull request's `open` / `merged` /
    /// `closed`, a build's `running` or outcome -- read by
    /// `knobas_core::declared_state!` beside the path line. `None` for a kind
    /// that declares no state, a value the declaration does not name, and
    /// every row of a corpus knobas owns: the ADR-0007 **miss**.
    /// `#[serde(default)]` for `path`'s reason.
    #[serde(default)]
    pub state: Option<knobas_core::payload::StateChip>,
}

/// One result: a row, its rank, and the excerpt with the match marked.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct SearchHit {
    #[serde(flatten)]
    pub row: EntityRow,
    pub rank: f32,
    pub snippet: Vec<Segment>,
}

/// A run of excerpt text, and whether it is part of the match.
///
/// **XSS-unsafe: `text` is raw source text** -- whatever a person typed into
/// a ticket, `<script>` included. Render it as text (`{segment.text}`), never
/// with `{@html}`; the highlighting is the `hit` flag, and no markup crosses
/// the bridge (roadmap §4 gotcha 7).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct Segment {
    pub text: String,
    pub hit: bool,
}

/// The values the query box suggests after `kind:`, `source:` and `under:`
/// (#601, spec #591 stories 27-30).
///
/// **Only values that exist right now**, so a picked suggestion never leads
/// to an empty result or an unknown token: the kinds the corpus has live rows
/// of, the enabled sources that have a live row, and the parents the
/// mirror and the estate declare. The kinds and sources are read once per
/// launcher opening and narrowed by the box; the parents are asked again per
/// keystroke of an `under:` value, by prefix (#761) and alone (#768).
/// Free-text keys (`author:`, `updated:`) get no suggestions and have no list
/// here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct FilterSuggestions {
    /// Every kind with at least one row in the launcher's corpora, by id.
    pub kinds: Vec<KindSuggestion>,
    /// Every enabled source with at least one live row, ordered by id -- the
    /// sources `source:` resolves that lead somewhere (#614).
    pub sources: Vec<SourceSuggestion>,
    /// The newest [`crate::PARENT_SUGGESTIONS`] (twenty) names a parent answers to that
    /// start with the prefix asked for, ignoring case, each with the parent's
    /// kind, newest first (#761). A parent with two names (a Gitea repo's
    /// `owner/name` and `name`) is listed once per name that has the prefix.
    pub parents: Vec<ParentSuggestion>,
    /// The values the ticket keys can take (#811): for each of `project:`,
    /// `status:`, `issuetype:`, `priority:`, `assignee:` and `label:` the
    /// most used [`crate::FIELD_SUGGESTIONS`] (fifty) values the live mirror holds,
    /// in the source's spelling; `assignee:`'s `@me` first; `sprint:`'s
    /// `current` while a ticket is in an active sprint; `assigned:`'s
    /// `today` and `week`. Read with the kinds and the sources, once per
    /// opening, and empty from a parents-only read.
    #[serde(default)]
    pub fields: Vec<FieldSuggestion>,
}

/// One value a ticket key can take (#811).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct FieldSuggestion {
    /// The key as the grammar spells it, without the colon: `status`.
    pub key: String,
    /// What the box completes the key with: `In Review`, `PAY`, `@me`.
    pub value: String,
    /// What is drawn beside it: a project's name, `you` for `@me`, or empty.
    pub detail: String,
}

/// One kind `kind:` can name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct KindSuggestion {
    /// The kind id, which is what the box completes `kind:` with.
    pub kind: String,
    /// The catalog's label for it, which is what a result group says.
    pub label: String,
}

/// One source `source:` can name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct SourceSuggestion {
    /// The instance id, which is what the box completes `source:` with: an
    /// exact id resolves to that instance and nothing else.
    pub id: String,
    /// The name the sources list shows.
    pub display_name: String,
}

/// One name `under:` can scope by (ADR-0018).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "search.ts")]
pub struct ParentSuggestion {
    /// The name the parent answers to, as its source spells it.
    pub name: String,
    /// The parent's kind, so `payout-service · repo` and a same-named page can
    /// be told apart. For a parent outside the mirror it is the kind its
    /// children declare, which is why `ParentPath` carries one (#594).
    pub kind: String,
}

/// The Docs search's answer (#860): what [`crate::Searcher::page_search`]
/// found, before the app adds each page's snippet, space and editor.
#[derive(Debug, Clone)]
pub struct PageSearch {
    /// Every live page the query admits, before the limit.
    pub total: u32,
    /// The best of them, at most the limit, best first.
    pub hits: Vec<SearchHit>,
    /// The words and phrases a reader marks ([`crate::query::needles`]).
    pub needles: Vec<String>,
    /// What the grammar could not read, as the launcher reports it.
    pub unknown_tokens: Vec<String>,
    pub took_ms: u32,
}
