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
//! **Stream E's:** the query parser (prefixes, aliases, `key:value`), the
//! dynamic query builder that applies [`SearchFilters`] -- the one reviewed
//! module allowed to use `AssertSqlSafe` (roadmap §4 gotcha 2) -- the built-in
//! smart lists, and the empty-query board. Until then a filtered query is
//! refused rather than silently answered unfiltered.
//!
//! The corpus is `sync.live_item` and nothing else: notes are M2 and asset
//! ancestor paths are M4 (interfaces §2.4).

pub mod snippet;
pub mod types;

use std::time::Instant;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

pub use types::{
    EntityRow, ParsedQuery, Prefix, ResultGroup, SearchFilters, SearchHit, SearchQuery,
    SearchResponse, Segment,
};

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
/// (roadmap §4 gotcha 2). `count(*) over ()` yields the true totals in the
/// same round trip, because window functions run before `LIMIT`.
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
       count(*) over ()                                               as total,
       count(*) over (partition by i.kind)                            as kind_total
  from sync.live_item i,
       websearch_to_tsquery('english', $1) q
 where i.fts @@ q
 -- entity_id breaks rank ties, so grouping and pagination are stable.
 order by rank desc, i.entity_id
 limit $3
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
    total: i64,
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

    let total = rows.first().map_or(0, |row| saturating_u32(row.total));
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
                let (label, plural, monogram) = kind_display(&row.kind);
                groups.push(ResultGroup {
                    kind: row.kind,
                    label,
                    plural,
                    monogram,
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

/// Display metadata for a kind, derived from the kind string.
///
/// **A seed, and the one place stream E must replace.** §3a is explicit that
/// the launcher renders a source's items from the adapter's
/// `SourceDescriptor::entity_kinds` -- label, plural, monogram -- so that a new
/// adapter needs no UI work. Until `list_adapters` is wired through (stream F's
/// registry, stream E's grouping), this derives something legible rather than
/// leaving the fields blank: `"pr"` becomes `Pr` / `Prs` / `PR`, which is
/// wrong-but-visible, exactly the kind of wrong a reviewer catches.
fn kind_display(kind: &str) -> (String, String, String) {
    let mut chars = kind.chars();
    let label = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    };
    let plural = format!("{label}s");
    let mut monogram: String = kind.chars().take(2).collect::<String>().to_uppercase();
    while monogram.chars().count() < 2 {
        monogram.push('·');
    }
    (label, plural, monogram)
}
