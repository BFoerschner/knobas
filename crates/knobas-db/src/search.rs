//! Full-text search over synced items.

/// One search result: the item that matched, with a highlighted excerpt.
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct SearchHit {
    pub entity_id: String,
    pub kind: String,
    pub source_id: String,
    pub title: String,
    /// May contain `<b>` marks AND raw source text -- escape before rendering.
    pub snippet: String,
    pub rank: f32,
    pub synced_at: chrono::DateTime<chrono::Utc>,
}

/// The best `limit` matches for `query`, ranked by cover density.
///
/// `query` is user text in the `websearch_to_tsquery` dialect (quoted phrases,
/// `or`, leading `-`); it is bound as a parameter, never interpolated, and the
/// tsquery is computed once as a `FROM` item so both the match and the
/// highlight reuse it.
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
                  ts_headline('english', i.body_text, q,
                              'MaxWords=18, MinWords=8') as snippet,
                  ts_rank_cd(i.fts, q) as rank,
                  i.synced_at
           from sync.item i, websearch_to_tsquery('english', $1) q
           where i.fts @@ q
           order by rank desc
           limit $2"#,
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(hits)
}
