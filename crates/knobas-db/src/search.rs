//! Full-text search over synced items.

/// One search result: the item that matched, with a highlighted excerpt.
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct SearchHit {
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    pub title: String,
    /// Plain text, and **raw source text** -- a ticket body can contain
    /// anything a person typed, `<script>` included. Render it as text; it is
    /// never safe to interpolate as markup.
    ///
    /// The match is not marked up: `ts_headline`'s selectors are emptied, so
    /// what comes back is the excerpt and nothing else. Highlighting means
    /// returning the match *offsets* alongside the text, which is M1's --
    /// smuggling `<b>` through a string that must be escaped anyway only ever
    /// produced literal tags on screen.
    pub snippet: String,
    pub rank: f32,
    pub synced_at: chrono::DateTime<chrono::Utc>,
}

/// The best `limit` matches for `query` among the **live** items, ranked by
/// cover density.
///
/// `query` is user text in the `websearch_to_tsquery` dialect (quoted phrases,
/// `or`, leading `-`); it is bound as a parameter, never interpolated, and the
/// tsquery is computed once as a `FROM` item so both the match and the
/// highlight reuse it.
///
/// An item deleted upstream keeps its `sync.item` row -- that is what still
/// holds the last-known title of something a link or a note points at -- so
/// "deleted" lives on `knobas.entity.deleted_at` alone, and it is this join
/// that keeps the launcher from offering what no longer exists. The join is
/// inner rather than filtering afterwards because `sync.item.entity_id`
/// references `knobas.entity(id)`: every mirrored item has exactly one entity
/// row, so the join drops the tombstoned and nothing else.
///
/// # Errors
///
/// Returns [`crate::DbError::Sqlx`] if the query fails -- most likely because
/// the schema has not been migrated yet.
pub async fn search(
    pool: &sqlx::PgPool,
    query: &str,
    limit: i64,
) -> Result<Vec<SearchHit>, crate::DbError> {
    let hits = sqlx::query_as::<_, SearchHit>(
        r#"select i.entity_id, i.kind, i.source_id, i.title,
                  -- Empty selectors: the excerpt comes back as plain text.
                  -- Anything else would be markup inside a string every caller
                  -- has to escape, which renders as literal tags.
                  ts_headline('english', i.body_text, q,
                              'MaxWords=18, MinWords=8, StartSel="", StopSel=""')
                    as snippet,
                  ts_rank_cd(i.fts, q) as rank,
                  i.synced_at
           from sync.item i
                join knobas.entity e on e.id = i.entity_id,
                websearch_to_tsquery('english', $1) q
           where i.fts @@ q
             and e.deleted_at is null
           order by rank desc
           limit $2"#,
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(hits)
}
