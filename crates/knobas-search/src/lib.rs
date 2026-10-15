//! The launcher's read side.
//!
//! M0 answered `search(q, limit) -> Vec<SearchHit>` from `knobas_db::search`.
//! That shape cannot express the prefixes, chips, grouping or empty-query
//! board of §4, so ruling P2 replaces it with one command carrying a query
//! object, and ruling P9 moves the code here -- moved, not copied:
//! `knobas_db::search` is gone.
//!
//! # The pipeline
//!
//! One [`Searcher`], four stages, and nothing between them that guesses:
//!
//! 1. [`vocab::Vocabulary::load`] -- what this installation is configured with
//!    (sources, the identity behind `@me`, the kind catalog), one query;
//! 2. [`query::parse`] + [`query::merge`] -- the §4 grammar, pure, against that
//!    vocabulary, reconciled with the chips the UI already had;
//! 3. [`sql::search_sql`] -- the statement, assembled per query, every value a
//!    bind (roadmap §4 gotcha 2);
//! 4. [`group::group`] -- flat rows into the launcher's fixed-order groups.
//!
//! The seeded contract statement this replaces carried its per-kind total as a
//! `count(*) over (partition by kind)` -- a second window partition, so a
//! second full sort of every matching row, on the launcher's hot path. That is
//! the M0 carry-over, and installing the builder here is what discharges it.
//!
//! The corpora are [`corpus::ALL`]: the mirror, `knobas.note` since #46, and
//! the estate's assets (#428) and routes (#432) -- so one query answers over
//! what knobas synced *and* what it owns, an asset's ancestor path included
//! (spec §4, interfaces §2.4). A prefix that names no corpus at all -- `t `,
//! `>`, `?` -- answers with *no rows* and still echoes what it understood,
//! never invented ones.
//!
//! The gotcha-2 confinement is enforced, not merely intended: `tests/it/
//! sql_containment.rs` fails the build if any file in this crate outside
//! `sql.rs` so much as names the type, which is also why no other module here
//! spells it out.

pub mod corpus;
pub(crate) mod coverage;
pub mod group;
pub mod home;
pub mod lists;
pub mod query;
pub mod saved;
pub mod snippet;
pub mod sql;
pub(crate) mod suggest;
/// The deterministic corpus the perf gate and the bench share.
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
pub mod types;
pub(crate) mod under;
pub mod vocab;

use std::time::Instant;

use sqlx::PgPool;

pub use group::RawHit;
pub use home::LauncherBoard;
pub use lists::{BuiltinList, SmartListSummary};
pub use query::{EffectiveFilters, FieldFilter, FieldKey, Parsed, merge, parse};
pub use saved::{Refusal, SavedList};
pub use suggest::PARENT_SUGGESTIONS;
pub use types::{
    EntityRow, FilterAnswer, FilterCoverage, FilterDimension, FilterSuggestions, KindSuggestion,
    ParentSuggestion, ParsedQuery, Prefix, ResultGroup, SearchFilters, SearchHit, SearchQuery,
    SearchResponse, Segment, SourceAnswer, SourceSuggestion,
};
pub use vocab::{KindCatalog, SourceVocab, Vocabulary};

/// Longest raw query the launcher will parse.
///
/// A paste, not a query: §4's box is one line. The cap is on *characters*
/// rather than bytes so that a query in a non-Latin script is not cut shorter
/// than the same query in ASCII.
pub(crate) const MAX_RAW_CHARS: usize = 512;

/// Most rows one response may carry, whatever the caller asked for.
const MAX_LIMIT: u32 = 200;

/// Most values one filter dimension may carry.
///
/// `SearchFilters` is deserialized from the frontend, so "how many sources can
/// a chip name" is a question a caller answers, not the UI. Thirty-two is more
/// instances than any installation has and far fewer than an `= any(...)` that
/// costs anything.
pub(crate) const MAX_FILTER_VALUES: usize = 32;

/// How many rows one kind may contribute to a page.
///
/// So a flood of tickets cannot push every build off the launcher (§4 groups
/// results by type, which is worth nothing if one type owns the page).
const PER_GROUP: u32 = 10;

/// Why a search could not be answered.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    /// The query is outside the bounds the engine will answer -- a paste
    /// rather than a query, or a filter list nobody can have clicked.
    #[error("invalid query: {0}")]
    Invalid(String),
    /// `list:<id>` named a smart list that does not exist.
    #[error("unknown smart list: {0}")]
    UnknownList(String),
}

/// The launcher's engine.
///
/// Holds a pool and the kind catalog, and nothing per-query: a `Searcher` is
/// built once at startup and answers every keystroke.
#[derive(Debug, Clone)]
pub struct Searcher {
    pool: PgPool,
    kinds: KindCatalog,
    /// Each compiled-in adapter kind's declared payload paths, which every
    /// query's vocabulary hands to its configured sources -- the path line and
    /// `under:`'s coverage read them. Empty from [`Self::new`]: then no source
    /// declares anything to those two. What `under:` itself matches is the
    /// database's since #627 (`sync.item_parent`, kept from the declarations
    /// [`Self::declaring`] stores), so a bare searcher answers it from
    /// whatever was stored last.
    payload_paths: Vec<(String, Vec<knobas_core::payload::KindPaths>)>,
}

impl Searcher {
    /// A searcher with **derived** kind metadata.
    ///
    /// The honest default while nothing wires the adapter registry through
    /// (open question **E-Q2**): a group is labelled from its kind id, which
    /// is what a kind no compiled-in adapter declares needs anyway (spec §3a).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            kinds: KindCatalog::default(),
            payload_paths: Vec::new(),
        }
    }

    /// A searcher labelling groups from what the adapters declared.
    #[must_use]
    pub fn with_kinds(pool: PgPool, kinds: KindCatalog) -> Self {
        Self {
            pool,
            kinds,
            payload_paths: Vec::new(),
        }
    }

    /// The same searcher, reading what these adapters declare about their
    /// payloads -- their **templates**, since where a source keeps its parent
    /// is a property of the adapter kind and not of the instance (#277's
    /// reading, which `knobas_app::sources::paths` makes too).
    #[must_use]
    pub fn with_payload_paths(
        mut self,
        templates: impl IntoIterator<Item = knobas_source::SourceDescriptor>,
    ) -> Self {
        self.payload_paths = templates
            .into_iter()
            .map(|template| (template.adapter_kind, template.payload_paths))
            .collect();
        self
    }

    /// A searcher reading what these adapters declare, **and the database's
    /// stored copy of the same declarations**, written from the same list in
    /// the same call so the two cannot differ (#627, ADR-0019).
    ///
    /// `under:` no longer walks the declarations per query: migration `0028`
    /// keeps `sync.item_parent`, the names every item answers to, and
    /// recomputes it from `sync.parent_declaration` whenever an item or a
    /// declaration changes; which adapter a source uses is matched when the
    /// rows are read, so a source command recomputes nothing. This writes that table
    /// whole -- each template's `payload_paths` under its adapter kind, every
    /// other adapter kind removed. It reads the table first and writes only
    /// when it differs, so a call that changes nothing is one read, takes no
    /// lock and recomputes nothing. The path line and coverage still read the in-memory copy
    /// [`Self::with_payload_paths`] keeps.
    ///
    /// The one way the app builds its searcher, and the one way a test that
    /// searches `under:` builds one.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the declarations cannot be written.
    pub async fn declaring(
        pool: PgPool,
        templates: impl IntoIterator<Item = knobas_source::SourceDescriptor>,
    ) -> Result<Self, SearchError> {
        let searcher = Self::new(pool).with_payload_paths(templates);
        let kinds: Vec<String> = searcher
            .payload_paths
            .iter()
            .map(|(kind, _)| kind.clone())
            .collect();
        let paths: Vec<sqlx::types::Json<&Vec<knobas_core::payload::KindPaths>>> = searcher
            .payload_paths
            .iter()
            .map(|(_, paths)| sqlx::types::Json(paths))
            .collect();
        // Read first, and write only on a difference: this runs on every
        // command, and an `on conflict do update` locks the row it matches
        // even when its `where` then declines to change it. Equal is the
        // usual case -- every call after bring-up's first one.
        let stored: Vec<(String, serde_json::Value)> = sqlx::query_as(
            "select adapter_kind, payload_paths from sync.parent_declaration order by adapter_kind",
        )
        .fetch_all(&searcher.pool)
        .await?;
        let mut wanted: Vec<(String, serde_json::Value)> = searcher
            .payload_paths
            .iter()
            .map(|(kind, paths)| {
                (
                    kind.clone(),
                    serde_json::to_value(paths).unwrap_or(serde_json::Value::Null),
                )
            })
            .collect();
        wanted.sort_by(|a, b| a.0.cmp(&b.0));
        if stored == wanted {
            return Ok(searcher);
        }
        let mut tx = searcher.pool.begin().await?;
        // A changed declaration recomputes `sync.item_parent` for every item
        // of its adapter kinds, in this transaction: 7.6 s at 100,000 items
        // on a loaded machine (#745 review). The app's pool cancels a
        // statement after thirty seconds, and a slower machine would have
        // every search retry the recompute and be cancelled with it, so the
        // limit is lifted for this transaction alone (`local` ends with it).
        sqlx::query("set local statement_timeout = 0")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "insert into sync.parent_declaration (adapter_kind, payload_paths)
             select * from unnest($1::text[], $2::jsonb[])
             on conflict (adapter_kind) do update set payload_paths = excluded.payload_paths
              where sync.parent_declaration.payload_paths
                    is distinct from excluded.payload_paths",
        )
        .bind(&kinds)
        .bind(&paths)
        .execute(&mut *tx)
        .await?;
        sqlx::query("delete from sync.parent_declaration where adapter_kind <> all($1)")
            .bind(&kinds)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(searcher)
    }

    /// What this installation is configured with, its sources given their
    /// declarations. One round trip, the one every query already made.
    async fn vocabulary(&self) -> Result<Vocabulary, SearchError> {
        Ok(Vocabulary::load(&self.pool, self.kinds.clone())
            .await?
            .declaring(&self.payload_paths))
    }

    /// The pool this searcher reads through.
    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Answer one launcher query.
    ///
    /// # Errors
    ///
    /// [`SearchError::Invalid`] for a query outside the engine's bounds,
    /// [`SearchError::UnknownList`] for a `list:` nobody ships,
    /// [`SearchError::Db`] if the statement fails.
    pub async fn search(&self, query: SearchQuery) -> Result<SearchResponse, SearchError> {
        let started = Instant::now();
        let query = validate(query)?;

        let vocab = self.vocabulary().await?;
        let mut parsed = query::parse(&query.raw, &vocab);
        // Whether an `under:` value names any parent is the mirror's to say,
        // and it is said before the echo is built so the greyed-out token
        // rides back with everything else the parse reported.
        under::report_unknown(&self.pool, &mut parsed).await?;
        let filters = query::merge(&parsed, &query.filters);
        let interpreted = ParsedQuery {
            filters: filters.echo(),
            ..parsed.query.clone()
        };

        if let Some(list) = parsed.list_id.clone() {
            return self
                .list_response(&list, query.limit, &vocab, interpreted, started)
                .await;
        }

        let text = (!parsed.query.text.is_empty()).then_some(parsed.query.text.as_str());

        // An empty box is the *board's* job (see `launcher_board`), and so is
        // a query that is nothing but grammar: the builder would generate a
        // browse over the whole mirror and count every row in it.
        if text.is_none() && filters.is_empty() {
            return Ok(empty(interpreted, started));
        }
        // A corpus M1 does not have answers with nothing rather than with
        // something else's rows (interfaces §2.4).
        if empty_corpus(parsed.query.prefix) {
            return Ok(empty(interpreted, started));
        }

        let built = sql::search_sql(
            corpus::ALL,
            text,
            parsed.prefix_last_term,
            &filters,
            PER_GROUP,
            query.limit,
        );
        let rows: Vec<RawHit> = sql::query_as_with(built).fetch_all(&self.pool).await?;
        let groups = group::group(rows, &vocab.kinds);

        // A second round trip, and only for a query that filtered by author
        // (issue #141) or by parent (#595). It answers a question the statement above structurally
        // cannot: the match set says which rows came back, never whether a
        // source *could have* contributed one -- and those two are what an
        // empty `author:` result has to be told apart by.
        let coverage = coverage::report(&self.pool, &vocab, &filters).await?;

        Ok(SearchResponse {
            interpreted,
            total: groups.iter().map(|g| g.total).sum(),
            groups,
            coverage,
            took_ms: took_ms(started),
        })
    }

    /// What an empty box answers with: the smart lists and the newest items.
    ///
    /// Spec §4's board. Deliberately not a search: a query with no text and no
    /// filter would scan the whole mirror to return nothing, and the two reads
    /// here are index-backed instead.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if either half cannot be read.
    pub async fn launcher_board(&self) -> Result<LauncherBoard, SearchError> {
        // One vocabulary for both halves: the lists need its identity, and the
        // newest rows its declarations, which their path line reads (#596).
        let vocab = self.vocabulary().await?;
        Ok(LauncherBoard {
            smart_lists: self.smart_lists_of(&vocab).await?,
            recent: home::recent(&self.pool, home::RECENT_LIMIT, &vocab.declarations).await?,
        })
    }

    /// The values the query box suggests after `kind:`, `source:` and
    /// `under:` (#601) -- only values that exist right now; see
    /// [`FilterSuggestions`].
    ///
    /// Four round trips: the vocabulary every query loads, the kinds with
    /// rows, the parents, and the enabled sources with rows. The parents are
    /// the newest [`PARENT_SUGGESTIONS`] whose name starts with `under`,
    /// ignoring case, and simply the newest for an empty one (#761).
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if any read fails.
    pub async fn filter_suggestions(&self, under: &str) -> Result<FilterSuggestions, SearchError> {
        let vocab = self.vocabulary().await?;
        suggest::read(&self.pool, &vocab, under).await
    }

    /// The parents half of [`Searcher::filter_suggestions`] and nothing else
    /// (#768): `kinds` and `sources` come back empty, unread.
    ///
    /// What the box asks on each keystroke of an `under:` value after the
    /// opening's first read, which already holds the kinds and the sources.
    /// One round trip, and no vocabulary.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the read fails.
    pub async fn filter_parent_suggestions(
        &self,
        under: &str,
    ) -> Result<FilterSuggestions, SearchError> {
        Ok(FilterSuggestions {
            kinds: Vec::new(),
            sources: Vec::new(),
            parents: suggest::parents(&self.pool, under).await?,
        })
    }

    /// The plan the board's recency read runs under.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the plan cannot be read.
    #[cfg(any(test, feature = "test-util"))]
    pub async fn explain_recent(&self) -> Result<String, SearchError> {
        home::explain_recent(&self.pool, home::RECENT_LIMIT).await
    }

    /// Every smart list, with its count and its change badge -- the built-ins
    /// and then the ones a reader saved (#506).
    ///
    /// **One list, not two**, which is the shape story 57 asks for: saved and
    /// built-in behave as one kind, and `SmartListSummary::saved` is all that
    /// tells the rail which controls a row carries. The built-ins come first
    /// because they are the ones that are always there.
    ///
    /// Three round trips with nothing saved: the vocabulary (for the identity
    /// behind `@me`, which every search loads anyway), one statement carrying
    /// the built-ins' counts, freshness stamps and seen-stamps together, and
    /// one small select over `knobas.smart_list`. Two more once something is
    /// saved -- see [`saved::summaries`], which is where that cost is stated.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the summary cannot be read.
    pub async fn smart_lists(&self) -> Result<Vec<SmartListSummary>, SearchError> {
        let vocab = self.vocabulary().await?;
        self.smart_lists_of(&vocab).await
    }

    /// [`Self::smart_lists`] over a vocabulary the caller already loaded.
    async fn smart_lists_of(
        &self,
        vocab: &Vocabulary,
    ) -> Result<Vec<SmartListSummary>, SearchError> {
        let mut out = lists::summaries(&self.pool, &vocab.identity).await?;
        out.extend(saved::summaries(&self.pool, vocab).await?);
        Ok(out)
    }

    /// Save the launcher's current query as a smart list (story 56).
    ///
    /// Answers with the row as the rail will draw it, so a caller that has
    /// just saved one has the count and the badge without re-reading the
    /// board.
    ///
    /// # Errors
    ///
    /// [`SearchError::Invalid`] for a blank name, for a query today's grammar
    /// cannot run, and when [`saved::MAX_SAVED_LISTS`] are already saved;
    /// [`SearchError::Db`] if the row cannot be written.
    pub async fn save_list(&self, label: &str, raw: &str) -> Result<SmartListSummary, SearchError> {
        let vocab = self.vocabulary().await?;
        let created = saved::create(&self.pool, &vocab, label, raw).await?;
        // Read back through the same function the rail reads, rather than
        // assembling a summary here: a second way to build this row is a
        // second answer to "what does a saved list look like", and the one the
        // caller gets on save is the one it would get on the next opening.
        saved::summaries(&self.pool, &vocab)
            .await?
            .into_iter()
            .find(|list| list.id == created.id)
            .ok_or_else(|| SearchError::UnknownList(created.id))
    }

    /// Give a saved list a different name (story 58). The id does not move.
    ///
    /// # Errors
    ///
    /// [`SearchError::Invalid`] for a blank name,
    /// [`SearchError::UnknownList`] if nobody saved a list by that id,
    /// [`SearchError::Db`] if the write fails.
    pub async fn rename_list(&self, id: &str, label: &str) -> Result<(), SearchError> {
        saved::rename(&self.pool, id, label).await.map(|_| ())
    }

    /// Forget a saved list (story 58).
    ///
    /// # Errors
    ///
    /// [`SearchError::UnknownList`] if nobody saved a list by that id,
    /// [`SearchError::Db`] if the delete fails.
    pub async fn delete_list(&self, id: &str) -> Result<(), SearchError> {
        saved::delete(&self.pool, id).await
    }

    /// The rows of one built-in smart list, shaped exactly like a search.
    ///
    /// Same grouping, same DTO, `rank = 0` and no snippet -- which is why the
    /// launcher renders a list with the code it renders results with, and why
    /// typing `list:mine` in the box needs no second path.
    ///
    /// Opening a list is also what **clears its badge**: the seen-stamp is
    /// written here, so a list the user has looked at stops claiming to be new.
    ///
    /// # Errors
    ///
    /// [`SearchError::UnknownList`] if nobody ships a list by that id,
    /// [`SearchError::Db`] if the statement fails.
    pub async fn smart_list_items(
        &self,
        id: &str,
        limit: u32,
    ) -> Result<SearchResponse, SearchError> {
        let started = Instant::now();
        let vocab = self.vocabulary().await?;
        let interpreted = ParsedQuery {
            text: String::new(),
            prefix: Some(Prefix::List),
            filters: SearchFilters::default(),
            unknown_tokens: Vec::new(),
        };
        self.list_response(id, limit, &vocab, interpreted, started)
            .await
    }

    /// One list's rows, whoever ships it.
    ///
    /// **`list:<id>` has one meaning, and this is where that is true**: a
    /// built-in is looked up in the compiled-in registry, and anything else is
    /// looked for in `knobas.smart_list` before the id is called unknown. The
    /// two arms answer with the same `SearchResponse` and both clear the
    /// badge, which is what lets the launcher open either one by typing its
    /// id.
    async fn list_response(
        &self,
        id: &str,
        limit: u32,
        vocab: &Vocabulary,
        interpreted: ParsedQuery,
        started: Instant,
    ) -> Result<SearchResponse, SearchError> {
        let limit = limit.clamp(1, MAX_LIMIT);
        if let Some(list) = lists::find(id) {
            let rows = lists::rows(
                &self.pool,
                list,
                &vocab.identity,
                limit,
                &vocab.declarations,
            )
            .await?;
            let groups = group::group(rows, &vocab.kinds);
            lists::mark_seen(&self.pool, list.id).await?;
            return Ok(SearchResponse {
                interpreted,
                total: groups.iter().map(|g| g.total).sum(),
                groups,
                // A built-in list is not a filter the user wrote, so there is
                // no author question of theirs to report on -- `list:mine`
                // narrows by the identity `lists::rows` resolves, and a
                // coverage row for it would explain a query nobody typed.
                coverage: Vec::new(),
                took_ms: took_ms(started),
            });
        }

        let saved = saved::find(&self.pool, id)
            .await?
            .ok_or_else(|| SearchError::UnknownList(id.to_owned()))?;
        // A list the rail is drawing as *needs attention* answers with the
        // reason rather than with an empty page: the row is not openable, so
        // reaching here means somebody typed `list:<id>` for one, and telling
        // them "no rows" would be the launcher hiding a broken saved query.
        let runnable = saved::plan(&saved.query, vocab)
            .map_err(|refusal| SearchError::Invalid(refusal.description()))?;

        let built = sql::search_sql(
            corpus::ALL,
            runnable.text.as_deref(),
            // A saved query is not being typed, so its last word is finished.
            // `sep` saved as a list means `sep`, not "anything starting sep".
            false,
            &runnable.filters,
            PER_GROUP,
            limit,
        );
        let rows: Vec<RawHit> = sql::query_as_with(built).fetch_all(&self.pool).await?;
        let groups = group::group(rows, &vocab.kinds);
        // **Unlike the built-in arm**, and for that arm's own stated reason
        // read the other way: a saved list *is* a filter the user wrote, so an
        // empty `author:` in it has the same two meanings issue #141 exists to
        // tell apart.
        let coverage = coverage::report(&self.pool, vocab, &runnable.filters).await?;
        lists::mark_seen(&self.pool, &saved.id).await?;

        Ok(SearchResponse {
            // The **saved query's** interpretation and not the `list:<id>` the
            // reader typed to get here -- story 56's "keeping its text, chips
            // and prefixes" is what the chip row above the results draws.
            interpreted: runnable.interpreted,
            total: groups.iter().map(|g| g.total).sum(),
            groups,
            coverage,
            took_ms: took_ms(started),
        })
    }
}

/// Bound one query, or refuse it.
///
/// The limit is **clamped**, the rest is **refused**. That split is on
/// purpose: a limit outside the range is a caller asking for a bigger page
/// than the launcher draws, and answering with a smaller one is what it meant;
/// a 600-character box or a chip naming forty sources is not a query anybody
/// typed, and quietly truncating it would answer a question nobody asked.
pub(crate) fn validate(mut query: SearchQuery) -> Result<SearchQuery, SearchError> {
    let chars = query.raw.chars().count();
    if chars > MAX_RAW_CHARS {
        return Err(SearchError::Invalid(format!(
            "query is {chars} characters; the launcher accepts {MAX_RAW_CHARS}"
        )));
    }
    for (dimension, values) in [
        ("sources", &query.filters.sources),
        ("kinds", &query.filters.kinds),
        ("authors", &query.filters.authors),
    ] {
        if values.len() > MAX_FILTER_VALUES {
            return Err(SearchError::Invalid(format!(
                "{dimension} filter names {} values; at most {MAX_FILTER_VALUES} are accepted",
                values.len()
            )));
        }
    }
    query.limit = query.limit.clamp(1, MAX_LIMIT);
    Ok(query)
}

/// Whether this prefix names a corpus the launcher does not answer from yet.
///
/// `t `, `>` and `?` are not corpus searches at all -- worklogs, the command
/// palette and help. Every one of them is still **parsed and echoed**, so the
/// launcher greys the prefix out with a reason instead of showing tickets for
/// `t `.
///
/// **`asset:` left this list with #436** and is the second prefix to do so.
/// The rows had existed since #428 ([`corpus::ASSET`], and [`corpus::ROUTE`]
/// since #432), so a *plain* query already answered with them; what was
/// missing was the launcher's own estate hit -- the monogram, the path from
/// the root, and the action chain -- and a prefix that promised the estate and
/// answered with nothing was the honest state until that landed. It has, so
/// removing the variant here is the whole of what turns the prefix on: the
/// parser already claimed it and already set `kinds = ["asset"]`.
/// Interfaces §2.4's *"the parser must simply return no `asset:` results
/// rather than pretending"* is discharged rather than broken -- there is
/// nothing left to pretend about.
///
/// `note:` was in this list until #46 and left it the same way, for the same
/// reason.
pub(crate) fn empty_corpus(prefix: Option<Prefix>) -> bool {
    matches!(prefix, Some(Prefix::Time | Prefix::Action | Prefix::Help))
}

/// A response that understood the query and found nothing.
///
/// No coverage, and it is not an omission: both callers are queries that never
/// reached a corpus at all -- an empty box, or a prefix that is not a corpus
/// search (`t `, `>`, `?`). Reporting "and Buildserver has no authors" for one
/// of those would explain the wrong absence. `asset: @jonas` is no longer one
/// of them (#436): it reaches the corpora like any other query and is reported
/// on like any other -- which is what "coverage reports assets like any other
/// kind" means.
fn empty(interpreted: ParsedQuery, started: Instant) -> SearchResponse {
    SearchResponse {
        interpreted,
        groups: Vec::new(),
        total: 0,
        coverage: Vec::new(),
        took_ms: took_ms(started),
    }
}

fn took_ms(started: Instant) -> u32 {
    u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX)
}

pub(crate) fn saturating_u32(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(raw: &str) -> SearchQuery {
        SearchQuery {
            raw: raw.to_owned(),
            limit: 30,
            filters: SearchFilters::default(),
        }
    }

    /// A page bigger than the launcher draws is answered with the page it
    /// draws; `limit: 0` is a request for nothing at all, which no caller
    /// means and which the builder would answer with an empty group list and a
    /// full corpus count behind it.
    #[test]
    fn the_limit_is_clamped_rather_than_refused() {
        assert_eq!(
            validate(SearchQuery {
                limit: 0,
                ..query("sepa")
            })
            .unwrap()
            .limit,
            1
        );
        assert_eq!(
            validate(SearchQuery {
                limit: 10_000,
                ..query("sepa")
            })
            .unwrap()
            .limit,
            MAX_LIMIT
        );
        assert_eq!(
            validate(SearchQuery {
                limit: 30,
                ..query("sepa")
            })
            .unwrap()
            .limit,
            30
        );
    }

    #[test]
    fn a_pasted_document_is_refused_rather_than_truncated() {
        let long = query(&"x".repeat(MAX_RAW_CHARS + 1));
        assert!(matches!(validate(long), Err(SearchError::Invalid(_))));
        // Exactly at the cap is a query, not a paste.
        assert!(validate(query(&"x".repeat(MAX_RAW_CHARS))).is_ok());
        // Counted in characters, so a non-Latin query is not cut short.
        assert!(validate(query(&"ß".repeat(MAX_RAW_CHARS))).is_ok());
    }

    #[test]
    fn an_unbounded_filter_list_is_refused() {
        let many: Vec<String> = (0..=MAX_FILTER_VALUES).map(|n| n.to_string()).collect();
        for filters in [
            SearchFilters {
                sources: many.clone(),
                ..SearchFilters::default()
            },
            SearchFilters {
                kinds: many.clone(),
                ..SearchFilters::default()
            },
            // Authors are a chip dimension too (ruling E-Q1), and a chip list
            // is deserialized from the frontend rather than typed: the raw-text
            // cap that bounds `@a @b @c ...` does not reach it.
            SearchFilters {
                authors: many.clone(),
                ..SearchFilters::default()
            },
        ] {
            let refused = validate(SearchQuery {
                filters,
                ..query("sepa")
            });
            assert!(matches!(refused, Err(SearchError::Invalid(_))));
        }
        let at_cap: Vec<String> = (0..MAX_FILTER_VALUES).map(|n| n.to_string()).collect();
        assert!(
            validate(SearchQuery {
                filters: SearchFilters {
                    sources: at_cap,
                    ..SearchFilters::default()
                },
                ..query("sepa")
            })
            .is_ok()
        );
    }

    /// The prefixes the launcher does not answer from, and -- just as
    /// important -- the ones it does.
    ///
    /// **Every prefix left in the first list is one that names no corpus at
    /// all**, and that is now a property rather than a coincidence: worklogs,
    /// the palette and the help card are not searches. So the list is checked
    /// against [`corpus::ALL`] in both directions -- a prefix that
    /// short-circuits must have no corpus behind it, which is what would have
    /// failed on `asset:` from #428 until #436. `corpus_of` below is
    /// **exhaustive**, with no `_` arm, so the next prefix cannot arrive
    /// without a verdict on which of the two lists it belongs in.
    #[test]
    fn only_the_absent_corpora_short_circuit() {
        // The prefix that pins a corpus, and the corpus it pins. Every variant
        // is named rather than swept up by a wildcard: a wildcard would answer
        // "no corpus" for a prefix nobody had thought about, which is the one
        // answer that makes the assertion below pass for the wrong reason.
        fn corpus_of(prefix: Prefix) -> Option<&'static str> {
            match prefix {
                Prefix::Asset => Some("'asset'"),
                Prefix::Note => Some("'note'"),
                Prefix::Action
                | Prefix::Ticket
                | Prefix::Person
                | Prefix::Source
                | Prefix::Time
                | Prefix::List
                | Prefix::Help => None,
            }
        }
        for prefix in [
            Prefix::Ticket,
            Prefix::Person,
            Prefix::Source,
            Prefix::List,
            // #46 for notes, #436 for the estate: both left the list the same
            // way, once the launcher could draw what the corpus already held.
            Prefix::Note,
            Prefix::Asset,
        ] {
            assert!(!empty_corpus(Some(prefix)), "{prefix:?}");
            if let Some(kind) = corpus_of(prefix) {
                assert!(
                    corpus::ALL.iter().any(|corpus| corpus.kind == kind),
                    "{prefix:?} answers from {kind}, which is not in corpus::ALL"
                );
            }
        }
        for absent in [Prefix::Time, Prefix::Action, Prefix::Help] {
            assert!(empty_corpus(Some(absent)), "{absent:?}");
            assert!(
                corpus_of(absent).is_none(),
                "{absent:?} short-circuits and names a corpus: one of the two is wrong"
            );
        }
        assert!(!empty_corpus(None));
    }
}
