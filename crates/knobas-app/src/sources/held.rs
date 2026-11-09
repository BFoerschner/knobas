//! **What knobas last read, kept between sessions** (ADR-0042, the second
//! smoothness pass on PR #2479): `sync.held_read`, where a page's source read
//! waits for the next time the page opens, so the page shows it at once and
//! reads the source behind it ([`super::live::LiveSources::shown`]).
//!
//! What is held is the adapter's answer, as JSON, with when it was read and
//! what the source was built from short of its credential (its `recipe`). A
//! row read through another recipe is never answered, and is replaced by the
//! next read. A source's rows go with its configuration (the foreign key)
//! and with a new credential ([`forget`]).
//!
//! Every function here is best effort: a store that will not answer is a
//! page that waits for its source, as before, never a failed read.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// How many bytes of answers the table holds; past it the oldest reads go
/// first. A few hundred pull requests' views, far more ordinary ones.
pub const BOUND: i64 = 64 * 1024 * 1024;

/// A read larger than this is answered and not held: a whole diff, a long log.
pub const ROW_CAP: usize = 2 * 1024 * 1024;

/// The ask a view is held under: `view:` and its section, `view:` alone for
/// the whole view.
#[must_use]
pub fn view_ask(section: Option<&str>) -> String {
    format!("view:{}", section.unwrap_or_default())
}

/// The held answer to `ask` about `entity_id` from source `source_id`, read
/// through `recipe`, and when it was read; `None` when none is held, when it
/// was read through another recipe, or when the store does not answer.
pub async fn get(
    pool: &PgPool,
    source_id: &str,
    entity_id: &str,
    ask: &str,
    recipe: &serde_json::Value,
) -> Option<(DateTime<Utc>, serde_json::Value)> {
    let row: Option<(DateTime<Utc>, serde_json::Value)> = sqlx::query_as(
        "select read_at, answer from sync.held_read
          where source_id = $1 and entity_id = $2 and ask = $3 and recipe = $4",
    )
    .bind(source_id)
    .bind(entity_id)
    .bind(ask)
    .bind(recipe)
    .fetch_optional(pool)
    .await
    .map_err(|error| tracing::debug!(%error, "held_read: no answer"))
    .ok()?;
    row
}

/// Hold `answer` to `ask` about `entity_id`, read at `read_at` through
/// `recipe`, over whatever was held; a row over [`ROW_CAP`] is not written,
/// and the oldest rows go while the table is over [`BOUND`].
pub async fn put(
    pool: &PgPool,
    source_id: &str,
    entity_id: &str,
    ask: &str,
    recipe: &serde_json::Value,
    read_at: DateTime<Utc>,
    answer: &serde_json::Value,
) {
    let Ok(bytes) = serde_json::to_vec(answer) else {
        return;
    };
    if bytes.len() > ROW_CAP {
        return;
    }
    let written = sqlx::query(
        "insert into sync.held_read (source_id, entity_id, ask, recipe, read_at, bytes, answer)
         values ($1, $2, $3, $4, $5, $6, $7)
         on conflict (source_id, entity_id, ask) do update
           set recipe = excluded.recipe, read_at = excluded.read_at,
               bytes = excluded.bytes, answer = excluded.answer
           where sync.held_read.read_at <= excluded.read_at",
    )
    .bind(source_id)
    .bind(entity_id)
    .bind(ask)
    .bind(recipe)
    .bind(read_at)
    .bind(i32::try_from(bytes.len()).unwrap_or(i32::MAX))
    .bind(answer)
    .execute(pool)
    .await;
    if let Err(error) = written {
        // A source removed while its read was out has no row to name.
        tracing::debug!(%error, "held_read: not held");
        return;
    }
    // Over the bound, the oldest go: one statement, which reads the index.
    let _ = sqlx::query(
        "delete from sync.held_read where (source_id, entity_id, ask) in (
           select source_id, entity_id, ask from (
             select source_id, entity_id, ask,
                    sum(bytes) over (order by read_at desc) as running
               from sync.held_read) r
            where r.running > $1)",
    )
    .bind(BOUND)
    .execute(pool)
    .await;
}

/// Drop every read held from `source_id`: its credential changed, and a new
/// one may read less.
pub async fn forget(pool: &PgPool, source_id: &str) {
    if let Err(error) = sqlx::query("delete from sync.held_read where source_id = $1")
        .bind(source_id)
        .execute(pool)
        .await
    {
        tracing::debug!(%error, "held_read: not forgotten");
    }
}
