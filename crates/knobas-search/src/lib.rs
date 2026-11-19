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
pub mod excerpt;
pub mod favourites;
pub mod frames;
pub mod group;
pub mod home;
pub mod keys;
pub mod picker;
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

pub use favourites::FavouriteTarget;
pub use group::RawHit;
pub use home::LauncherBoard;
pub use keys::{ActionKey, Keys, ListKey};
pub use query::{
    EffectiveFilters, FieldFilter, FieldKey, KIND_LETTERS, Parsed, Pattern, merge, parse,
    pattern_of,
};
pub use saved::{Refusal, SavedList, SmartListSummary};
pub use suggest::{FIELD_SUGGESTIONS, PARENT_SUGGESTIONS};
pub use types::{
    EntityRow, Excerpt, FieldSuggestion, FilterAnswer, FilterCoverage, FilterDimension,
    FilterSuggestions, KindSuggestion, PageSearch, ParentSuggestion, ParsedQuery, Prefix,
    ResultGroup, SearchFilters, SearchHit, SearchQuery, SearchResponse, Segment, SourceAnswer,
    SourceSuggestion,
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

/// What a stored query reads as today ([`Searcher::readings`], #812).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// The grammar read all of it, and this many rows match.
    Count(u64),
    /// The part the grammar could not read: the tokens it did not know, or
    /// the bound the text is past.
    Unreadable(String),
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
    /// Whether notes are searched. False while *Show notes and quick capture*
    /// is off (#940, R-13a.6): the notes corpus leaves every statement, so
    /// `note:` finds nothing, and `kind:` stops offering `note`. The notes
    /// themselves stay (D10).
    notes_shown: bool,
    /// How many rows may hold a query for it to be ranked whole, and how many
    /// of a kind's newest rows a broader one is looked for in:
    /// [`sql::RANKED_WHOLE`] and [`sql::WALK`], but for a test, whose mirror
    /// holds tens of rows ([`Self::bounded_at`]).
    bounds: (u32, u32),
    /// Whether its statements are planned as on Windows, where no read a
    /// person waits on asks for a parallel worker ([`sql::matched_planning`],
    /// [`sql::letter_planning`]; the workers pass, 2026-11-16): `cfg!(windows)`,
    /// but for a test or a probe that plans the Windows statements on another
    /// platform ([`Self::planned_as_windows`]).
    windows: bool,
    /// Every name of the mirror, read by a picker (the fuzzy-search
    /// decisions, 2026-11-19): its names join each answer
    /// ([`Self::with_picker`]). `None`, and a picker that holds no name yet,
    /// answer exactly as before it.
    picker: Option<picker::Picker>,
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
            notes_shown: true,
            bounds: (sql::RANKED_WHOLE, sql::WALK),
            windows: cfg!(windows),
            picker: None,
        }
    }

    /// The same searcher, the names `picker` holds joining each launcher
    /// answer (the fuzzy-search decisions, 2026-11-19, D2 and D6): what
    /// was typed is read against every name in memory, and the names it
    /// finds join PostgreSQL's rows without moving one of them. A searcher
    /// built without this call is the launcher as it was before.
    #[must_use]
    pub fn with_picker(mut self, picker: picker::Picker) -> Self {
        self.picker = Some(picker);
        self
    }

    /// The same searcher, its statements planned as on Windows where
    /// `windows` says so, whatever this build runs on. A new searcher plans
    /// as its platform does.
    #[must_use]
    pub fn planned_as_windows(mut self, windows: bool) -> Self {
        self.windows = windows;
        self
    }

    /// The same searcher, searching notes or leaving them out (#940). A new
    /// searcher searches them.
    #[must_use]
    pub fn with_notes_shown(mut self, shown: bool) -> Self {
        self.notes_shown = shown;
        self
    }

    /// The same searcher, ranking a query whole while no more than
    /// `ranked_whole` rows hold it and looking for a broader one's text in
    /// `walk` of a kind's newest rows: the two bounds of
    /// [`Self::rows_by_words`], at a size a test's mirror reaches.
    #[cfg(any(test, feature = "test-util"))]
    #[must_use]
    pub fn bounded_at(mut self, ranked_whole: u32, walk: u32) -> Self {
        self.bounds = (ranked_whole, walk);
        self
    }

    /// The corpora a statement is built over: [`corpus::ALL`], less notes
    /// while they are hidden.
    fn corpora(&self) -> &'static [&'static corpus::Corpus] {
        if self.notes_shown {
            corpus::ALL
        } else {
            corpus::WITHOUT_NOTES
        }
    }

    /// A searcher labelling groups from what the adapters declared.
    #[must_use]
    pub fn with_kinds(pool: PgPool, kinds: KindCatalog) -> Self {
        Self {
            pool,
            kinds,
            payload_paths: Vec::new(),
            notes_shown: true,
            bounds: (sql::RANKED_WHOLE, sql::WALK),
            windows: cfg!(windows),
            picker: None,
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
    /// declarations, the reader's tone overrides merged in (spec #2463).
    ///
    /// The overrides are read here, per vocabulary, rather than kept beside
    /// [`Self::with_payload_paths`]' copy: that copy is the adapters' own and
    /// is what [`Self::declaring`] writes into `sync.parent_declaration`, so an
    /// override never reaches the stored table and never recomputes the
    /// parents. Two round trips: the source listing, and a read of a table of
    /// a few rows.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if a read fails.
    pub async fn vocabulary(&self) -> Result<Vocabulary, SearchError> {
        let overrides = knobas_core::state_colours::overrides(&self.pool).await?;
        let templates: Vec<(String, Vec<knobas_core::payload::KindPaths>)> = self
            .payload_paths
            .iter()
            .map(|(adapter, paths)| {
                (
                    adapter.clone(),
                    knobas_core::state_colours::merged(paths.clone(), &overrides),
                )
            })
            .collect();
        Ok(Vocabulary::load(&self.pool, self.kinds.clone())
            .await?
            .declaring(&templates))
    }

    /// Read `raw` as a link-set pattern (#868, R-7.2) with this
    /// installation's vocabulary (`source:` and `in:` resolved), or say why
    /// it is not one: [`query::pattern_of`].
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the vocabulary cannot be read. A text that is
    /// not a pattern is the inner `Err`, never an error.
    pub async fn pattern(&self, raw: &str) -> Result<Result<Pattern, String>, SearchError> {
        if raw.chars().count() > MAX_RAW_CHARS {
            return Ok(Err(format!(
                "a pattern is at most {MAX_RAW_CHARS} characters"
            )));
        }
        let vocab = self.vocabulary().await?;
        Ok(query::pattern_of(&query::parse(raw, &vocab)))
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
    /// [`SearchError::Db`] if the statement fails.
    pub async fn search(&self, query: SearchQuery) -> Result<SearchResponse, SearchError> {
        self.search_as(query, true).await
    }

    /// [`Self::search`] with a word's first characters ranked as longer words
    /// are ([`sql::search_sql`]), as every query was answered before the
    /// one-letter pass (2026-11-15): the reference `tests/it/one_letter.rs`
    /// and `tests/it/short_words.rs` hold the queries the names do not
    /// answer to, and show one to three characters against.
    ///
    /// # Errors
    ///
    /// As [`Self::search`].
    #[cfg(any(test, feature = "test-util"))]
    pub async fn search_by_words(&self, query: SearchQuery) -> Result<SearchResponse, SearchError> {
        self.search_as(query, false).await
    }

    /// [`Self::search`], a word's first characters answered by names where
    /// `by_name` says so.
    async fn search_as(
        &self,
        query: SearchQuery,
        by_name: bool,
    ) -> Result<SearchResponse, SearchError> {
        let started = Instant::now();
        let query = validate(query)?;

        // The vocabulary, and beside it what the database holds of the
        // mirror's stored reads (migrations `0084` and `0093` to `0095`,
        // [`corpus::stored`]): one round trip, asked while the vocabulary's
        // two reads are out, so it adds no wait of its own.
        let (vocab, stored) = tokio::try_join!(self.vocabulary(), async {
            Ok(corpus::stored(&self.pool).await?)
        })?;
        let mut parsed = query::parse(&query.raw, &vocab);
        // Pipelines under the shell's word, and no runs or commits (#2422).
        let frames = frames::LauncherFrames::of(&vocab.declarations);
        // Whether an `under:` value names any parent is the mirror's to say,
        // and it is said before the echo is built so the greyed-out token
        // rides back with everything else the parse reported. The same read
        // says which known names have nothing the launcher admits beneath
        // them, the left-out kinds being what it leaves out (#2495).
        let known_under = under::report_unknown(&self.pool, &mut parsed, &frames.left_out).await?;
        let filters = EffectiveFilters {
            titles: query.titles,
            excerpts: !query.titles,
            pipeline_kinds: frames.pipelines,
            hidden_kinds: frames.left_out,
            ..query::merge(&parsed, &query.filters)
        };
        let mut interpreted = ParsedQuery {
            filters: filters.echo(),
            ..parsed.query.clone()
        };

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

        // The picker's names, read beside the statements (D6): only for a
        // searcher that holds names, text of two letters and more, and a
        // query whose `under:` admits something.
        let picking = self
            .picker
            .as_ref()
            .filter(|picker| picker.holds_names() && !known_under.admits_nothing)
            .and(text)
            .and_then(|typed| {
                picker::Typed::read(typed)
                    .filter(|read| read.letters() >= 2)
                    .map(|read| (typed.to_owned(), read))
            });
        // A leading `!` on a word is the picker's sign for *not*, and
        // `websearch_to_tsquery` would read it as the word itself: the
        // statements are handed `-` in its place (D4 rule 9). The echo stays
        // as typed.
        let negated = picking
            .as_ref()
            .and_then(|(typed, _)| query::bangs_as_minus(typed));
        let text = negated.as_deref().or(text);

        // An `under:` scoped to names with nothing admitted beneath any of
        // them answers with nothing, and the statement is not asked: to say
        // nothing, it reads the whole mirror twice, for the live items and
        // for the run logs (#2495).
        let statements = async {
            if known_under.admits_nothing {
                return Ok::<Vec<RawHit>, SearchError>(Vec::new());
            }
            // A word's first characters are read off the stored ones only
            // once every item has its own (`Stored::names`): until bring-up's
            // fill has passed, they are ranked as every query was before.
            let named = self
                .rows_by_name(
                    text.filter(|_| by_name && stored.names),
                    parsed.prefix_last_term,
                    &filters,
                    query.limit,
                )
                .await?;
            Ok(match named {
                Named::Rows(rows) => rows,
                words => {
                    // The mirror's match stage off its search copies alone,
                    // once every copy holds what the stage reads (migration
                    // `0084`).
                    let corpora = corpus::with_mirror(
                        corpus::searched(self.corpora(), filters.titles),
                        stored.mirror,
                    );
                    let rows = self
                        .rows_by_words(
                            &corpora,
                            text,
                            parsed.prefix_last_term,
                            &filters,
                            query.limit,
                            by_name && stored.whole(),
                        )
                        .await?;
                    match words {
                        // Few names: the text's rows in the room they leave.
                        Named::Few(names) => group::filled(names, rows, PER_GROUP, query.limit),
                        // No name holds the characters, or they are no
                        // word's first: the words' answer whole.
                        _ => rows,
                    }
                }
            })
        };
        let names = async {
            let Some((typed, _)) = &picking else {
                return Ok::<Option<Vec<RawHit>>, SearchError>(None);
            };
            self.picked_rows(typed, &filters, query.limit)
                .await
                .map(Some)
        };
        let (rows, picked) = tokio::join!(statements, names);
        let mut rows = rows?;
        if let (Some(picked), Some((_, typed))) = (picked?, &picking) {
            rows = group::with_names(
                rows,
                picked,
                &|title| typed.names(title),
                PER_GROUP,
                query.limit,
            );
            group::mark(&mut rows, typed);
        }
        if rows.is_empty() {
            interpreted.left_out_tokens = under::left_out_tokens(&parsed, &known_under, &filters);
        }
        let mut groups = if filters.excerpts {
            group::with_excerpts(rows, &vocab.kinds, &query::needles(&parsed.query.text))
        } else {
            group::group(rows, &vocab.kinds)
        };
        flag_lists_needing_attention(&mut groups, &vocab);

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

    /// The rows the picker chooses for `typed` (D5), as the statement that
    /// reads them by key gives them (D8, [`sql::picked_sql`]): live, within
    /// every filter, each kind's best ten and twice the page at most. The
    /// read itself runs on a blocking thread, one core, and the statement
    /// on a pooled connection of its own, beside the words' statements.
    async fn picked_rows(
        &self,
        typed: &str,
        filters: &EffectiveFilters,
        limit: u32,
    ) -> Result<Vec<RawHit>, SearchError> {
        let Some(picker) = self.picker.clone() else {
            return Ok(Vec::new());
        };
        let limits = picker::Limits {
            sources: filters.sources.clone(),
        };
        let typed = typed.to_owned();
        let Ok(picked) = tokio::task::spawn_blocking(move || picker.pick(&typed, &limits)).await
        else {
            // A read that panicked answers as no names: the statements'
            // rows stand alone.
            return Ok(Vec::new());
        };
        if picked.is_empty() {
            return Ok(Vec::new());
        }
        let built = sql::picked_sql(
            &picked.ids(),
            &picked.ranks(),
            filters,
            PER_GROUP,
            limit.saturating_mul(2),
        );
        Ok(sql::fetch_kept(&self.pool, built).await?)
    }

    /// The rows **a word's first characters** are answered with: one typed
    /// character (the one-letter pass, 2026-11-15;
    /// `docs/decisions/2026-11-15-one-letter-is-answered-by-names.md`), and
    /// two or three (the two-letters pass, 2026-11-16;
    /// `docs/decisions/2026-11-16-two-letters-by-names-and-keys-wait-for-their-answer.md`).
    /// The names that hold a word beginning with them, those that begin
    /// with them first, then the newest ([`sql::letter_sql`]), under
    /// *Titles* and under *Everything* alike.
    ///
    /// Where [`sql::search_sql`] is to answer instead, or beside, it says
    /// which way:
    ///
    /// * [`Named::NotAsked`] for a longer word, a finished one (`p` and a
    ///   space), two or three characters that are not all letters and
    ///   digits, or a query that asks what the stored characters do not
    ///   hold.
    /// * [`Named::NoName`] where, under *Everything*, no name holds the
    ///   characters, so the bodies are asked as before and *nothing matches*
    ///   stays true of them too.
    /// * [`Named::Few`] where, under *Everything*, the names of two or three
    ///   characters are few, filling no kind and not the page
    ///   ([`group::few`]): the text is asked too, and its rows take the room
    ///   the names leave ([`group::filled`]). Names that fill a kind are the
    ///   answer, and so are one character's, as they were decided.
    ///
    /// Under *Titles* the names were the whole question, and none is the
    /// answer.
    async fn rows_by_name(
        &self,
        words: Option<&str>,
        prefix_last: bool,
        filters: &EffectiveFilters,
        limit: u32,
    ) -> Result<Named, sqlx::Error> {
        let Some(typed) = words.filter(|text| prefix_last && named_by_its_start(text)) else {
            return Ok(Named::NotAsked);
        };
        // Names, so no run log: a log is never one (`corpus::searched`).
        let corpora = corpus::searched(self.corpora(), true);
        let Some(built) = sql::letter_sql(&corpora, typed, filters, PER_GROUP, limit) else {
            return Ok(Named::NotAsked);
        };
        let rows: Vec<RawHit> = sql::fetch_named(&self.pool, built).await?;
        Ok(if filters.titles {
            Named::Rows(rows)
        } else if rows.is_empty() {
            Named::NoName
        } else if typed.chars().count() > 1 && group::few(&rows, PER_GROUP, limit) {
            Named::Few(rows)
        } else {
            Named::Rows(rows)
        })
    }

    /// The rows **words** are answered with (the long-words pass, 2026-11-16;
    /// `docs/decisions/2026-11-16-a-word-many-rows-hold.md`): four typed
    /// characters and more, a finished word, and what a word's first
    /// characters leave to the text ([`Self::rows_by_name`]).
    ///
    /// The matches are counted first, no further than one past
    /// [`sql::RANKED_WHOLE`] ([`sql::probe_sql`]):
    ///
    /// * **Few rows hold the query**: every match is ranked, as
    ///   [`sql::search_sql`] always ranked, the same answer
    ///   ([`sql::kept_search_sql`]).
    /// * **Many do**: the rows the query names, ranked where they are few
    ///   and the newest where they are many too, and under *Everything*,
    ///   where they leave a kind room, that kind's newest rows whose text
    ///   holds it ([`sql::broad_sql`]). Ranking 18,553 matches to keep ten a
    ///   kind was 54 ms of `payout` on Björn's scale; this is 10.
    ///
    /// Both run under one plan a connection ([`sql::fetch_kept`]).
    ///
    /// `bounded` says the stored reads those statements take are whole
    /// (`corpus::Stored::whole`) and a word's first characters are answered
    /// by names: where it is false, and for a browse or a query that asks
    /// what the narrow relations do not hold (an author, a parent, a reach,
    /// a name glob, a ticket key), [`sql::search_sql`] answers, planned for
    /// the query, as before.
    async fn rows_by_words(
        &self,
        corpora: &[&'static corpus::Corpus],
        text: Option<&str>,
        prefix_last: bool,
        filters: &EffectiveFilters,
        limit: u32,
        bounded: bool,
    ) -> Result<Vec<RawHit>, sqlx::Error> {
        let whole = || sql::search_sql(corpora, text, prefix_last, filters, PER_GROUP, limit);
        let Some(typed) = text.filter(|_| bounded) else {
            return self.rows_of(whole(), text).await;
        };
        let (ranked_whole, walk) = self.bounds;
        let Some(probe) = sql::probe_sql(typed, prefix_last, filters, ranked_whole) else {
            return self.rows_of(whole(), text).await;
        };
        // The count and the answer in one transaction, on one connection.
        let mut kept = sql::Kept::begin(&self.pool).await?;
        let (matched, named) = kept
            .fetch::<(i64, i64)>(probe)
            .await?
            .first()
            .copied()
            .unwrap_or_default();
        let cap = i64::from(ranked_whole);
        let built = if matched <= cap {
            sql::kept_search_sql(corpora, typed, prefix_last, filters, PER_GROUP, limit)
        } else {
            let names = if named <= cap {
                sql::Names::Ranked
            } else {
                sql::Names::Newest
            };
            sql::broad_sql(
                corpora,
                typed,
                prefix_last,
                filters,
                PER_GROUP,
                limit,
                names,
                walk,
            )
        };
        let Some(built) = built else {
            kept.end().await?;
            return self.rows_of(whole(), text).await;
        };
        let rows = kept.fetch(built).await?;
        kept.end().await?;
        Ok(rows)
    }

    /// The rows of a statement [`sql::search_sql`] built: through the indexes
    /// when it has words to match ([`sql::fetch_matched`]), the narrow table
    /// read whole for a single character
    /// ([`sql::fetch_matched_by_a_letter`]), as the planner chooses for a
    /// browse. A single character reaches it from the Docs search, and from
    /// the launcher only where no name holds it or it is a finished word
    /// ([`Self::rows_by_name`]).
    async fn rows_of(
        &self,
        built: sql::SearchSql,
        words: Option<&str>,
    ) -> Result<Vec<RawHit>, sqlx::Error> {
        match words {
            // One character matches most of any mirror: read whole, where
            // the whole is the narrow table and not the mirror's own rows.
            Some(text) if text.chars().count() == 1 && built.matches_narrowly() => {
                sql::fetch_matched_by_a_letter_as(&self.pool, built, self.windows).await
            }
            Some(_) => sql::fetch_matched_as(&self.pool, built, self.windows).await,
            None => sql::query_as_with(built).fetch_all(&self.pool).await,
        }
    }

    /// Every entity id of `kind` the query admits, uncapped (#810): the
    /// ticket-first Tickets view's server-side filter (R-2.7), which hands the
    /// ids to the board it narrows. `None` for a query with neither text nor a
    /// filter, which narrows nothing; `Some` of none for one that admits no
    /// `kind` (a `type:` naming another kind among them).
    ///
    /// The keys are the launcher's: [`query::parse`] and [`query::merge`]
    /// with no chips, matched by the predicates [`sql::search_sql`] uses. The
    /// words are the view's: each must appear in the row's key, title,
    /// assignee or epic, a `-word` or `-"phrase"` must not, and `or`
    /// separates alternatives ([`query::word_alternatives`],
    /// [`sql::ids_sql`], #1889). Over the live mirror. A prefix (`#`, `>`)
    /// and `list:` mean nothing here and are not acted on.
    ///
    /// # Errors
    ///
    /// [`SearchError::Invalid`] for a query past the launcher's length,
    /// [`SearchError::Db`] if a statement fails.
    pub async fn matching_ids(
        &self,
        raw: &str,
        kind: &str,
    ) -> Result<Option<Vec<String>>, SearchError> {
        let vocab = self.vocabulary().await?;
        self.matching_ids_in(&vocab, raw, kind).await
    }

    /// [`matching_ids`](Self::matching_ids) against a vocabulary the caller
    /// already loaded from [`vocabulary`](Self::vocabulary), for a command that
    /// needs the identity in it too and so loads it once (#1659).
    ///
    /// # Errors
    ///
    /// As [`matching_ids`](Self::matching_ids).
    pub async fn matching_ids_in(
        &self,
        vocab: &Vocabulary,
        raw: &str,
        kind: &str,
    ) -> Result<Option<Vec<String>>, SearchError> {
        match Self::admits_in(vocab, raw, kind)? {
            Admits::Every => Ok(None),
            Admits::Nothing => Ok(Some(Vec::new())),
            Admits::Some(narrowing) => Ok(Some(narrowing.ids(&self.pool).await?)),
        }
    }

    /// What `raw` admits of `kind`, as [`matching_ids`](Self::matching_ids)
    /// reads it, without asking the mirror for the ids: for a reader that
    /// narrows a scan of its own by the query ([`Narrowing`]), the Tickets
    /// view's board, which then reads its tickets once and not twice (the
    /// broad-keys pass, 2026-11-13).
    ///
    /// # Errors
    ///
    /// [`SearchError::Invalid`] for a query past the launcher's length.
    pub fn admits_in(vocab: &Vocabulary, raw: &str, kind: &str) -> Result<Admits, SearchError> {
        let query = validate(SearchQuery {
            titles: false,
            raw: raw.to_owned(),
            limit: 1,
            filters: SearchFilters::default(),
        })?;
        let parsed = query::parse(&query.raw, vocab);
        let mut filters = query::merge(&parsed, &query.filters);
        let alternatives = query::word_alternatives(&parsed.query.text);
        if alternatives.is_empty() && filters.is_empty() {
            return Ok(Admits::Every);
        }
        if !filters.kinds.is_empty() && !filters.kinds.iter().any(|k| k == kind) {
            return Ok(Admits::Nothing);
        }
        filters.kinds = vec![kind.to_owned()];
        Ok(Admits::Some(Box::new(Narrowing {
            alternatives,
            filters,
        })))
    }

    /// The Docs search (#860, R-10.3, R-10.4): the live pages a query
    /// admits, best first, at most `limit` of them, with how many there are
    /// in all and the words and phrases a reader marks.
    ///
    /// The launcher's grammar and statement ([`query::parse`],
    /// [`sql::search_sql`]) over the mirror, kept to pages: every word must
    /// appear, English stemming, `"…"` a phrase, `-word` leaves out, `by:`
    /// and `updated:` filter, and `space:` is `project:` under another name.
    /// `space` is the picker's `(source id, space key)`; a `space:` the reader
    /// typed wins over it, as typed text wins over a chip in the launcher. A
    /// `type:` naming another kind admits nothing.
    ///
    /// `None` for a query with neither words nor a filter, which the Docs
    /// pane answers with its tree.
    ///
    /// # Errors
    ///
    /// [`SearchError::Invalid`] for a query past the launcher's length,
    /// [`SearchError::Db`] if the statement fails.
    pub async fn page_search(
        &self,
        raw: &str,
        space: Option<(&str, &str)>,
        limit: u32,
    ) -> Result<Option<PageSearch>, SearchError> {
        const PAGE: &str = "page";
        let started = Instant::now();
        let query = validate(SearchQuery {
            titles: false,
            raw: raw.to_owned(),
            limit,
            filters: SearchFilters::default(),
        })?;
        let vocab = self.vocabulary().await?;
        let mut parsed = query::parse(&query.raw, &vocab);
        under::report_unknown(&self.pool, &mut parsed, &[]).await?;
        let mut filters = query::merge(&parsed, &query.filters);
        let text = (!parsed.query.text.trim().is_empty()).then_some(parsed.query.text.as_str());
        if text.is_none() && filters.is_empty() {
            return Ok(None);
        }
        let answer = |hits: Vec<SearchHit>, total: u32| PageSearch {
            total,
            hits,
            needles: query::needles(&parsed.query.text),
            unknown_tokens: parsed.query.unknown_tokens.clone(),
            took_ms: took_ms(started),
        };
        if !filters.kinds.is_empty() && !filters.kinds.iter().any(|k| k == PAGE) {
            return Ok(Some(answer(Vec::new(), 0)));
        }
        filters.kinds = vec![PAGE.to_owned()];
        let typed_space = filters
            .fields
            .iter()
            .any(|f| f.key == query::FieldKey::Project && !f.negated);
        if let Some((source, key)) = space
            && !typed_space
        {
            if filters.sources.is_empty() {
                filters.sources = vec![source.to_owned()];
            }
            filters.fields.push(query::FieldFilter {
                key: query::FieldKey::Project,
                negated: false,
                values: vec![key.to_owned()],
                within_days: None,
                me: Vec::new(),
            });
        }
        let stored = corpus::stored(&self.pool).await?;
        let whole = || {
            sql::search_sql(
                &[stored.mirror],
                text,
                parsed.prefix_last_term,
                &filters,
                query.limit,
                query.limit,
            )
        };
        // **One typed character is answered by the pages' names** once every
        // item has its stored characters (the workers pass, 2026-11-16;
        // Fable's ruling): the pages a name of which holds a word beginning
        // with it, those that begin with it first, then the newest, all of
        // them counted ([`sql::page_letter_sql`]). A character no page's
        // name holds is asked of the bodies as before, so *no pages* is
        // never said of one a body holds; and so is every character until
        // the names are whole.
        let named = match text {
            Some(typed)
                if stored.names && parsed.prefix_last_term && typed.chars().count() == 1 =>
            {
                match sql::page_letter_sql(
                    &[stored.mirror],
                    typed,
                    &filters,
                    query.limit,
                    query.limit,
                ) {
                    Some(built) => sql::fetch_named::<RawHit>(&self.pool, built).await?,
                    None => Vec::new(),
                }
            }
            _ => Vec::new(),
        };
        let rows = if named.is_empty() {
            self.rows_of(whole(), text).await?
        } else {
            named
        };
        let group = group::group(rows, &vocab.kinds)
            .into_iter()
            .find(|g| g.kind == PAGE);
        Ok(Some(match group {
            Some(group) => answer(group.hits, group.total),
            None => answer(Vec::new(), 0),
        }))
    }

    /// What each stored text reads as today (#812): how many rows of `kind`
    /// [`matching_ids`](Self::matching_ids) would admit, or the part the
    /// grammar could not read. A ticket view's count in the views pane.
    ///
    /// One vocabulary for all of them, then one statement counting every
    /// readable text ([`sql::ids_counts_sql`], #1654). A
    /// text with neither words nor a filter counts every live row of `kind`,
    /// which is what its board draws. Nothing is rewritten: the text is read,
    /// never stored back.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if a statement fails. A text the grammar refuses is
    /// a [`Reading::Unreadable`], never an error.
    pub async fn readings(&self, raws: &[&str], kind: &str) -> Result<Vec<Reading>, SearchError> {
        let vocab = self.vocabulary().await?;
        let mut out: Vec<Option<Reading>> = Vec::with_capacity(raws.len());
        // The readable texts that still need a count: (slot in `out`, words, filters).
        let mut pending: Vec<(usize, Vec<query::WordAlternative>, query::EffectiveFilters)> =
            Vec::new();
        // **A text is counted once** however many views hold it (the workers
        // pass, 2026-11-16): the same text reads the same, and each count is
        // a read of every live row of `kind` the text admits. Björn's nine
        // views held one text between them, and the statement counted his
        // 23,176 tickets nine times. Each later view of a text is pointed at
        // the slot of its first.
        let mut first_of: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        let mut same_as: Vec<(usize, usize)> = Vec::new();
        for raw in raws {
            if let Some(first) = first_of.get(raw) {
                same_as.push((out.len(), *first));
                out.push(None);
                continue;
            }
            first_of.insert(*raw, out.len());
            if let Err(error) = validate(SearchQuery {
                titles: false,
                raw: (*raw).to_owned(),
                limit: 1,
                filters: SearchFilters::default(),
            }) {
                out.push(Some(Reading::Unreadable(match error {
                    SearchError::Invalid(message) => message,
                    other => other.to_string(),
                })));
                continue;
            }
            let parsed = query::parse(raw, &vocab);
            if !parsed.query.unknown_tokens.is_empty() {
                out.push(Some(Reading::Unreadable(
                    parsed.query.unknown_tokens.join(" "),
                )));
                continue;
            }
            let mut filters = query::merge(&parsed, &SearchFilters::default());
            if !filters.kinds.is_empty() && !filters.kinds.iter().any(|k| k == kind) {
                out.push(Some(Reading::Count(0)));
                continue;
            }
            filters.kinds = vec![kind.to_owned()];
            let words = query::word_alternatives(&parsed.query.text);
            pending.push((out.len(), words, filters));
            out.push(None);
        }
        // One statement for every readable text, not a scan awaited per view.
        let queries: Vec<(&[query::WordAlternative], &query::EffectiveFilters)> = pending
            .iter()
            .map(|(_, words, filters)| (words.as_slice(), filters))
            .collect();
        if let Some(built) = sql::ids_counts_sql(&queries) {
            let rows: Vec<(i64, i64)> = sql::query_as_with(built).fetch_all(&self.pool).await?;
            for (position, count) in rows {
                if let Some((slot, ..)) = usize::try_from(position)
                    .ok()
                    .and_then(|position| pending.get(position))
                {
                    out[*slot] = Some(Reading::Count(u64::try_from(count).unwrap_or(0)));
                }
            }
        }
        for (slot, first) in same_as {
            out[slot] = out[first].clone();
        }
        Ok(out
            .into_iter()
            .map(|reading| reading.unwrap_or(Reading::Count(0)))
            .collect())
    }

    /// What an empty box answers with: the favourites, then the newest items.
    ///
    /// Spec §4's board. Deliberately not a search: a query with no text and no
    /// filter would scan the whole mirror to return nothing, and the read here
    /// is index-backed instead. No lists since #2423: a saved list is found
    /// behind `#l`, and the built-in ones are gone.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the read fails.
    pub async fn launcher_board(&self) -> Result<LauncherBoard, SearchError> {
        // The newest rows' path line reads the declarations (#596).
        let vocab = self.vocabulary().await?;
        Ok(LauncherBoard {
            favourites: favourites::list(&self.pool, &vocab).await?,
            recent: home::recent(&self.pool, home::RECENT_LIMIT, &vocab.declarations).await?,
        })
    }

    /// Every favourite, in the order they were favourited (#2428): what the
    /// board draws above the recent items. See [`favourites::list`].
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if a read fails.
    pub async fn favourites(&self) -> Result<Vec<EntityRow>, SearchError> {
        let vocab = self.vocabulary().await?;
        favourites::list(&self.pool, &vocab).await
    }

    /// Favourite an entity or a saved list, last in the block (#2428).
    ///
    /// # Errors
    ///
    /// [`SearchError::UnknownList`] for a list nobody saved,
    /// [`SearchError::Db`] if the write fails.
    pub async fn add_favourite(&self, target: &FavouriteTarget) -> Result<(), SearchError> {
        favourites::add(&self.pool, target).await
    }

    /// Unfavourite an entity or a saved list (#2428); not a favourite is no
    /// error.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the delete fails.
    pub async fn remove_favourite(&self, target: &FavouriteTarget) -> Result<(), SearchError> {
        favourites::remove(&self.pool, target).await
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
        suggest::read(&self.pool, &vocab, under, self.notes_shown).await
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
            fields: Vec::new(),
        })
    }

    /// The parents [`Searcher::filter_parent_suggestions`] offers, read by
    /// the whole statement however many parent rows there are: what the
    /// newest rows' shortcut has to answer the same as.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the read fails.
    #[cfg(any(test, feature = "test-util"))]
    pub async fn whole_parent_suggestions(
        &self,
        under: &str,
    ) -> Result<Vec<ParentSuggestion>, SearchError> {
        suggest::whole_parents(&self.pool, under).await
    }

    /// The ticket keys' values [`Searcher::filter_suggestions`] offers, as
    /// `(key, value, detail)` rows, read from what the database stores and
    /// then by the walk of every sampled payload the statement made before:
    /// the two a test holds equal.
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if a read fails.
    #[cfg(any(test, feature = "test-util"))]
    pub async fn field_suggestion_rows_both_ways(
        &self,
    ) -> Result<(Vec<suggest::FieldRow>, Vec<suggest::FieldRow>), SearchError> {
        let vocab = self.vocabulary().await?;
        suggest::field_rows_both_ways(&self.pool, &vocab).await
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

    /// Every list a reader saved (#506), in the order they were saved, each
    /// with its count, or the reason it needs attention.
    ///
    /// Two round trips: the vocabulary, then [`saved::summaries`]. Not on the
    /// `⌘K` path since #2423: the board draws no lists, and `#l` finds them
    /// through [`Self::search`].
    ///
    /// # Errors
    ///
    /// [`SearchError::Db`] if the summary cannot be read.
    pub async fn smart_lists(&self) -> Result<Vec<SmartListSummary>, SearchError> {
        let vocab = self.vocabulary().await?;
        saved::summaries(&self.pool, &vocab, self.corpora()).await
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
        saved::summaries(&self.pool, &vocab, self.corpora())
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
}

/// What the names say of a word's first characters
/// ([`Searcher::rows_by_name`]).
enum Named {
    /// The answer: the rows the names give, which under *Titles* may be none.
    Rows(Vec<RawHit>),
    /// The names of two or three characters, and few of them: under
    /// *Everything* the text is still to ask, for the room they leave.
    Few(Vec<RawHit>),
    /// No name holds the characters, and under *Everything* the bodies are
    /// still to ask.
    NoName,
    /// Not a word's first characters, or asked with what the stored
    /// characters do not hold: the words' ranking answers, as before.
    NotAsked,
}

/// Whether `text` is a word's first characters as the stored ones hold them
/// (`sync.item_initial`): one character, or two or three that are all
/// letters and digits. A longer word is ranked, and so are two or three
/// characters the parser would read as more than one word (`c#`, `a-b`,
/// `-pa`): their lexemes are not what was typed.
fn named_by_its_start(text: &str) -> bool {
    match text.chars().count() {
        1 => true,
        2 | 3 => text.chars().all(char::is_alphanumeric),
        _ => false,
    }
}

/// What a query admits of one kind ([`Searcher::admits_in`]).
#[derive(Debug)]
pub enum Admits {
    /// A query with neither words nor a filter: it narrows nothing.
    Every,
    /// One that admits none of the kind, a `type:` naming another among
    /// them.
    Nothing,
    /// The rows of the kind the [`Narrowing`] keeps.
    Some(Box<Narrowing>),
}

/// A query as the rows of one kind it keeps: the words and keys of
/// [`Searcher::matching_ids`], parsed and not yet asked of the mirror.
///
/// Asked for the ids ([`Self::ids`]), or put in a ticket board's own scan:
/// it is a [`knobas_core::mini_board::CardsNarrowing`], which reads the board's
/// card rows with the query's predicates beside the board's
/// ([`sql::board_sql`]).
#[derive(Debug)]
pub struct Narrowing {
    alternatives: Vec<query::WordAlternative>,
    filters: query::EffectiveFilters,
}

impl Narrowing {
    /// Every live id the narrowing keeps: [`sql::ids_sql`]'s.
    ///
    /// # Errors
    ///
    /// [`sqlx::Error`] if the statement fails.
    pub async fn ids(&self, pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
        let built = sql::ids_sql(&self.alternatives, &self.filters);
        let ids: Vec<(String,)> = sql::query_as_with(built).fetch_all(pool).await?;
        Ok(ids.into_iter().map(|(id,)| id).collect())
    }

    /// Whether a board bound to `declarations` takes the narrowing in its
    /// own scan ([`sql::board_sql`]): they are the declarations the query
    /// was parsed with. Under any others the narrowing reads its ids first.
    #[must_use]
    pub fn in_scan_under(&self, declarations: &knobas_core::payload::Declarations) -> bool {
        self.declared_as(&declarations.as_param().0)
    }

    fn declared_as(&self, declared: &serde_json::Value) -> bool {
        self.filters.declarations.as_param().0 == *declared
    }
}

impl knobas_core::mini_board::CardsNarrowing for Narrowing {
    /// The board's card rows for the tickets the query keeps.
    ///
    /// In the board's own scan where the board binds the declarations the
    /// query was parsed with, which is every board the app reads: both come
    /// from the registry's descriptors and the configured sources. A board
    /// bound to other declarations reads the ids first and its statement
    /// over them, the two statements a filter always was: the query's words
    /// and keys read a stored card and stored fields under *its*
    /// declarations, and in the board's scan they would read the board's.
    async fn rows(
        &self,
        pool: &PgPool,
        statement: knobas_core::mini_board::CardsStatement,
        binds: knobas_core::mini_board::CardsBinds<'_>,
    ) -> Result<Vec<knobas_core::mini_board::CardRow>, sqlx::Error> {
        if self.declared_as(&binds.declared.0) {
            let built = sql::board_sql(statement, binds, &self.alternatives, &self.filters);
            return sql::query_as_with(built).fetch_all(pool).await;
        }
        let mut ids = self.ids(pool).await?;
        if let Some(members) = binds.members {
            let members: std::collections::HashSet<&str> =
                members.iter().map(String::as_str).collect();
            ids.retain(|id| members.contains(id.as_str()));
        }
        let query = sqlx::query_as(statement.whole)
            .persistent(false)
            .bind(&ids)
            .bind(binds.sources)
            .bind(binds.project)
            .bind(binds.declared)
            .bind(binds.per_column)
            .bind(binds.lanes)
            .bind(binds.lanes_first);
        // `$8` only where the statement reads a page (`CardsStatement::paged`).
        if statement.paged {
            query.bind(binds.page).fetch_all(pool).await
        } else {
            query.fetch_all(pool).await
        }
    }
}

/// A saved list whose text today's grammar refuses says so (#2423): its
/// state is the refusal, in the warning tone, so the row is coloured by it and
/// the launcher refuses to apply it with the reason. Read here and not in the
/// corpus's SQL because the grammar is this crate's Rust ([`saved::plan`]),
/// and a list hit's path line is the text it plans.
fn flag_lists_needing_attention(groups: &mut [ResultGroup], vocab: &Vocabulary) {
    for group in groups.iter_mut().filter(|group| group.kind == "list") {
        for hit in &mut group.hits {
            let Some(text) = hit.row.path.as_deref() else {
                continue;
            };
            if let Err(refusal) = saved::plan(text, vocab) {
                hit.row.state = Some(knobas_core::payload::StateChip {
                    text: refusal.description(),
                    tone: Some(knobas_core::tone::Tone::Warn),
                });
            }
        }
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
            titles: false,
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
                titles: false,
                limit: 0,
                ..query("sepa")
            })
            .unwrap()
            .limit,
            1
        );
        assert_eq!(
            validate(SearchQuery {
                titles: false,
                limit: 10_000,
                ..query("sepa")
            })
            .unwrap()
            .limit,
            MAX_LIMIT
        );
        assert_eq!(
            validate(SearchQuery {
                titles: false,
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
                titles: false,
                filters,
                ..query("sepa")
            });
            assert!(matches!(refused, Err(SearchError::Invalid(_))));
        }
        let at_cap: Vec<String> = (0..MAX_FILTER_VALUES).map(|n| n.to_string()).collect();
        assert!(
            validate(SearchQuery {
                titles: false,
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
                Prefix::Action | Prefix::Person | Prefix::Source | Prefix::Time | Prefix::Help => {
                    None
                }
            }
        }
        for prefix in [
            Prefix::Person,
            Prefix::Source,
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
