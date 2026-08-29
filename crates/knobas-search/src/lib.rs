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
//! The corpora are [`corpus::ALL`]: the mirror, and `knobas.note` since #46 --
//! so one query answers over what knobas synced *and* what it owns. Asset
//! ancestor paths are M4 (interfaces §2.4). A prefix whose corpus does not
//! exist yet answers with *no rows* and still echoes what it understood --
//! never invented ones.
//!
//! The gotcha-2 confinement is enforced, not merely intended: `tests/
//! sql_containment.rs` fails the build if any file in this crate outside
//! `sql.rs` so much as names the type, which is also why no other module here
//! spells it out.

pub mod corpus;
pub mod group;
pub mod home;
pub mod lists;
pub mod query;
pub mod snippet;
pub mod sql;
/// The deterministic corpus the perf gate and the bench share.
#[cfg(any(test, feature = "test-util"))]
pub mod testing;
pub mod types;
pub mod vocab;

use std::time::Instant;

use sqlx::PgPool;

pub use group::RawHit;
pub use home::LauncherBoard;
pub use lists::{BuiltinList, SmartListSummary};
pub use query::{EffectiveFilters, Parsed, merge, parse};
pub use types::{
    EntityRow, ParsedQuery, Prefix, ResultGroup, SearchFilters, SearchHit, SearchQuery,
    SearchResponse, Segment,
};
pub use vocab::{KindCatalog, SourceVocab, Vocabulary};

/// Longest raw query the launcher will parse.
///
/// A paste, not a query: §4's box is one line. The cap is on *characters*
/// rather than bytes so that a query in a non-Latin script is not cut shorter
/// than the same query in ASCII.
const MAX_RAW_CHARS: usize = 512;

/// Most rows one response may carry, whatever the caller asked for.
const MAX_LIMIT: u32 = 200;

/// Most values one filter dimension may carry.
///
/// `SearchFilters` is deserialized from the frontend, so "how many sources can
/// a chip name" is a question a caller answers, not the UI. Thirty-two is more
/// instances than any installation has and far fewer than an `= any(...)` that
/// costs anything.
const MAX_FILTER_VALUES: usize = 32;

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
        }
    }

    /// A searcher labelling groups from what the adapters declared.
    #[must_use]
    pub fn with_kinds(pool: PgPool, kinds: KindCatalog) -> Self {
        Self { pool, kinds }
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

        let vocab = Vocabulary::load(&self.pool, self.kinds.clone()).await?;
        let parsed = query::parse(&query.raw, &vocab);
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

        Ok(SearchResponse {
            interpreted,
            total: groups.iter().map(|g| g.total).sum(),
            groups,
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
        Ok(LauncherBoard {
            smart_lists: self.smart_lists().await?,
            recent: home::recent(&self.pool, home::RECENT_LIMIT).await?,
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

    /// Every built-in smart list, with its count and its change badge.
    ///
    /// Two round trips: the vocabulary (for the identity behind `@me`, which
    /// every search loads anyway) and one statement carrying the counts, the
    /// freshness stamps and the seen-stamps together.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the summary cannot be read.
    pub async fn smart_lists(&self) -> Result<Vec<SmartListSummary>, SearchError> {
        let vocab = Vocabulary::load(&self.pool, self.kinds.clone()).await?;
        lists::summaries(&self.pool, &vocab.identity).await
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
        let vocab = Vocabulary::load(&self.pool, self.kinds.clone()).await?;
        let interpreted = ParsedQuery {
            text: String::new(),
            prefix: Some(Prefix::List),
            filters: SearchFilters::default(),
            unknown_tokens: Vec::new(),
        };
        self.list_response(id, limit, &vocab, interpreted, started)
            .await
    }

    async fn list_response(
        &self,
        id: &str,
        limit: u32,
        vocab: &Vocabulary,
        interpreted: ParsedQuery,
        started: Instant,
    ) -> Result<SearchResponse, SearchError> {
        let list = lists::find(id).ok_or_else(|| SearchError::UnknownList(id.to_owned()))?;
        let rows =
            lists::rows(&self.pool, list, &vocab.identity, limit.clamp(1, MAX_LIMIT)).await?;
        let groups = group::group(rows, &vocab.kinds);
        lists::mark_seen(&self.pool, list.id).await?;
        Ok(SearchResponse {
            interpreted,
            total: groups.iter().map(|g| g.total).sum(),
            groups,
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
fn validate(mut query: SearchQuery) -> Result<SearchQuery, SearchError> {
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

/// Whether this prefix names a corpus knobas does not have yet.
///
/// Assets are M4 (interfaces §2.4: *"the parser must simply return no `asset:`
/// results rather than pretending"*), and `t `, `>` and `?` are not corpus
/// searches at all -- worklogs, the command palette and help. Every one of them
/// is still **parsed and echoed**, so the launcher greys the prefix out with a
/// reason instead of showing tickets for `asset:`.
///
/// `note:` was in this list until #46 and is not any more: notes have a corpus
/// ([`corpus::NOTE`]) and a write path behind it. Removing it here is the whole
/// of what turns the prefix on -- the parser already claimed it and already set
/// `kinds = ["note"]`.
fn empty_corpus(prefix: Option<Prefix>) -> bool {
    matches!(
        prefix,
        Some(Prefix::Asset | Prefix::Time | Prefix::Action | Prefix::Help)
    )
}

/// A response that understood the query and found nothing.
fn empty(interpreted: ParsedQuery, started: Instant) -> SearchResponse {
    SearchResponse {
        interpreted,
        groups: Vec::new(),
        total: 0,
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

    /// The prefixes whose corpus M1 does not have, and -- just as important --
    /// the ones whose corpus it does.
    #[test]
    fn only_the_absent_corpora_short_circuit() {
        for absent in [Prefix::Asset, Prefix::Time, Prefix::Action, Prefix::Help] {
            assert!(empty_corpus(Some(absent)), "{absent:?}");
        }
        for present in [
            Prefix::Ticket,
            Prefix::Person,
            Prefix::Source,
            Prefix::List,
            // #46: notes are knobas' own corpus, not a milestone away.
            Prefix::Note,
        ] {
            assert!(!empty_corpus(Some(present)), "{present:?}");
        }
        assert!(!empty_corpus(None));
    }
}
