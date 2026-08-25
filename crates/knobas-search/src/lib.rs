//! The launcher's read side.
//!
//! M0 answered `search(q, limit) -> Vec<SearchHit>` from `knobas_db::search`.
//! That shape cannot express the prefixes, chips, grouping or empty-query
//! board of §4, so ruling P2 replaces it with one command carrying a query
//! object, and ruling P9 moves the code here -- moved, not copied:
//! `knobas_db::search` is gone.
//!
//! # What is seeded and what is stream E's
//!
//! Seeded (this crate's contract PR): the frozen types, the FTS query over
//! `sync.live_item`, sentinel-based snippet segments, grouping by kind.
//! **Stream E's:** the query parser ([`query`], [`vocab`]), the dynamic query
//! builder that applies [`SearchFilters`] ([`sql`] -- the one reviewed module
//! allowed to wrap a runtime-built statement, roadmap §4 gotcha 2), the
//! built-in smart lists, and the empty-query board. Until the builder is wired
//! into [`search`] a filtered query is refused rather than silently answered
//! unfiltered.
//!
//! The gotcha-2 confinement is enforced, not merely intended: `tests/
//! sql_containment.rs` fails the build if any file in this crate outside
//! `sql.rs` so much as names the type, which is also why no other module here
//! spells it out.
//!
//! The corpus is `sync.live_item` and nothing else: notes are M2 and asset
//! ancestor paths are M4 (interfaces §2.4).

pub mod corpus;
pub mod query;
pub mod snippet;
pub mod sql;
pub mod types;
pub mod vocab;

use std::time::Instant;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub use query::{EffectiveFilters, Parsed, merge, parse};
pub use types::{
    EntityRow, ParsedQuery, Prefix, ResultGroup, SearchFilters, SearchHit, SearchQuery,
    SearchResponse, Segment,
};
pub use vocab::{KindCatalog, SourceVocab, Vocabulary};

/// Why a search could not be answered.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    /// The contract seed was handed a query only stream E's builder can answer.
    #[error("unsupported query: {0}")]
    Unsupported(String),
}

/// The tsquery is computed once as a FROM item so the match, the rank and the
/// headline all reuse it; the raw text is **bound**, never interpolated
/// (roadmap §4 gotcha 2). The per-kind total is a window function -- it runs
/// before `LIMIT`, so it counts the matches, not the page. The overall total
/// is [`COUNT_SQL`] instead, because a window needs a row to ride on and an
/// empty page has none.
const SEARCH_SQL: &str = r#"
select i.entity_id,
       i.kind,
       i.source_id,
       i.title,
       i.item_updated_at                                              as updated_at,
       i.synced_at,
       ts_rank_cd(i.fts, q)                                           as rank,
       -- Over the same text the index covers -- title *and* body. `fts`
       -- weights the title into the match, so a query that hits the title
       -- alone is a hit with nothing to quote from the body, and a headline
       -- over the body alone would then be an excerpt with no visible
       -- relation to what was searched for.
       ts_headline('english', i.title || ' — ' || i.body_text, q, $2::text) as headline,
       count(*) over (partition by i.kind)                            as kind_total
  from sync.live_item i,
       websearch_to_tsquery('english', $1) q
 where i.fts @@ q
 -- entity_id breaks rank ties, so grouping and pagination are stable.
 order by rank desc, i.entity_id
 limit $3
"#;

/// How many rows the query matches, independent of `limit`.
///
/// A second statement rather than a window function on the first: the window
/// only produces a value where there is a row, so `limit 0` -- or any query
/// whose page is empty -- would report zero matches instead of the real total.
/// Same bound text, same `websearch_to_tsquery`, same index.
const COUNT_SQL: &str = r#"
select count(*)
  from sync.live_item i,
       websearch_to_tsquery('english', $1) q
 where i.fts @@ q
"#;

#[derive(sqlx::FromRow)]
struct HitRow {
    entity_id: String,
    kind: String,
    source_id: String,
    title: String,
    updated_at: Option<DateTime<Utc>>,
    synced_at: DateTime<Utc>,
    rank: f32,
    headline: String,
    kind_total: i64,
}

/// Answer one launcher query.
///
/// # Errors
///
/// [`SearchError::Unsupported`] for a query carrying filters (stream E's
/// builder applies those), [`SearchError::Db`] if the query fails.
pub async fn search(pool: &PgPool, query: &SearchQuery) -> Result<SearchResponse, SearchError> {
    let started = Instant::now();
    let text = query.raw.trim().to_owned();

    if !query.filters.is_empty() {
        return Err(SearchError::Unsupported(
            "filters are stream E's query builder to apply; the contract seed answers \
             unfiltered queries only"
                .to_owned(),
        ));
    }

    let interpreted = ParsedQuery {
        text: text.clone(),
        // Stream E's parser fills these; a seed that guessed would be a
        // grammar written twice.
        prefix: None,
        filters: query.filters.clone(),
        unknown_tokens: Vec::new(),
    };

    // An empty box is the board, not a query: answering it with SQL would
    // scan the corpus to return nothing.
    if text.is_empty() {
        return Ok(SearchResponse {
            interpreted,
            groups: Vec::new(),
            total: 0,
            took_ms: 0,
        });
    }

    let rows = sqlx::query_as::<_, HitRow>(SEARCH_SQL)
        .bind(&text)
        .bind(snippet::headline_options())
        .bind(i64::from(query.limit))
        .fetch_all(pool)
        .await?;

    // Read independently of the rows: `count(*) over ()` rides on a row, and
    // `limit 0` returns none -- so a caller asking "how many are there?"
    // without wanting the hits would have been told zero. The count is what
    // the empty-query board and the group headers are drawn from, so a
    // silently wrong zero is a wrong number on screen, not an empty list.
    let total = saturating_u32(
        sqlx::query_scalar::<_, i64>(COUNT_SQL)
            .bind(&text)
            .fetch_one(pool)
            .await?,
    );
    // Derived metadata, because nothing has wired the adapter registry through
    // yet (open question **E-Q2**): `KindCatalog::info` falls back to a label,
    // a plural and a monogram worked out from the kind id, which is what a kind
    // no compiled-in adapter declares needs anyway (spec §3a).
    let catalog = KindCatalog::default();
    let mut groups: Vec<ResultGroup> = Vec::new();
    for row in rows {
        let hit = SearchHit {
            row: EntityRow {
                entity_id: row.entity_id,
                kind: row.kind.clone(),
                source_id: row.source_id,
                title: row.title,
                updated_at: row.updated_at,
                synced_at: row.synced_at,
            },
            rank: row.rank,
            snippet: snippet::segments(&row.headline),
        };
        // First appearance wins, so groups come out in rank order.
        match groups.iter_mut().find(|group| group.kind == row.kind) {
            Some(group) => group.hits.push(hit),
            None => {
                let info = catalog.info(&row.kind);
                groups.push(ResultGroup {
                    kind: row.kind,
                    label: info.label,
                    plural: info.plural,
                    monogram: info.monogram,
                    total: saturating_u32(row.kind_total),
                    hits: vec![hit],
                });
            }
        }
    }

    Ok(SearchResponse {
        interpreted,
        groups,
        total,
        took_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
    })
}

fn saturating_u32(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
