//! **What knobas last read, kept between sessions** (ADR-0042, the second
//! smoothness pass on PR #2479): `sync.held_read`, where a page's source read
//! waits for the next time the page opens, so the page shows it at once and
//! reads the source behind it. The app's live reads use it
//! (`knobas_app::sources::held`), and so does the sync's held-sections pass
//! ([`crate::held_sections`], 2026-11-15), which holds each open pull
//! request's *Files* before anyone opens it.
//!
//! What is held is the adapter's answer, as JSON, with when it was read and
//! what the source was built from short of its credential (its [`recipe`]).
//! A row read through another recipe is never answered, and is replaced by
//! the next read. A source's rows go with its configuration (the foreign key)
//! and with a new credential ([`forget`]).
//!
//! **Speculative rows go first** (migration `0092`): a row the sync held that
//! no page has drawn yet is `speculative`, and leaves before every row a
//! person read when the table is over [`BOUND`]. A page's read of it
//! ([`get`]) makes it a person's.
//!
//! Every function here is best effort: a store that will not answer is a
//! page that waits for its source, as before, never a failed read.

use chrono::{DateTime, Utc};
use knobas_source::AuthMethod;
use sqlx::PgPool;

/// How many bytes of answers the table holds; past it the speculative rows go
/// first, then the oldest reads. A few hundred pull requests' views, far more
/// ordinary ones.
pub const BOUND: i64 = 64 * 1024 * 1024;

/// A read larger than this is answered and not held: a whole diff, a long log.
pub const ROW_CAP: usize = 2 * 1024 * 1024;

/// The ask of a pull request's *Files*, which the sync holds ahead of a page
/// ([`crate::held_sections`]).
pub const FILES_ASK: &str = "view:files";

/// The cap of a [`FILES_ASK`] row, whoever writes it (the held-files pass,
/// 2026-11-15, Fable's ruling): the diff view's own ceiling of 50,000 lines
/// fits, so a pull request of thousands of files is held. Its first open is
/// the one that costs seconds on a forge under load; held, its 4.6 MB answer
/// for 3,000 files is 417 KB on disk and 13 ms to read back. A page's read
/// behind of the same row writes under the same cap, so it can replace the
/// sync's copy.
pub const FILES_ROW_CAP: usize = 8 * 1024 * 1024;

/// The largest answer held under `ask`: [`FILES_ROW_CAP`] for a pull
/// request's *Files*, [`ROW_CAP`] for everything else.
#[must_use]
pub fn row_cap(ask: &str) -> usize {
    if ask == FILES_ASK {
        FILES_ROW_CAP
    } else {
        ROW_CAP
    }
}

/// The ask a view is held under: `view:` and its section, `view:` alone for
/// the whole view.
#[must_use]
pub fn view_ask(section: Option<&str>) -> String {
    format!("view:{}", section.unwrap_or_default())
}

/// What a source was built from short of its credential, which a held read is
/// keyed by: its adapter, address, method and configuration. The app's live
/// reads and the sync's pass both key their rows by it, so a row either holds
/// is one the other answers.
#[must_use]
pub fn recipe(
    adapter_kind: &str,
    base_url: &str,
    method: Option<AuthMethod>,
    config: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "adapter": adapter_kind,
        "base_url": base_url,
        "method": method,
        "config": config,
    })
}

/// The held answer to `ask` about `entity_id` from source `source_id`, read
/// through `recipe`, and when it was read; `None` when none is held, when it
/// was read through another recipe, or when the store does not answer. A
/// speculative row read here is a person's from now on.
pub async fn get(
    pool: &PgPool,
    source_id: &str,
    entity_id: &str,
    ask: &str,
    recipe: &serde_json::Value,
) -> Option<(DateTime<Utc>, serde_json::Value)> {
    let row: Option<(DateTime<Utc>, serde_json::Value, bool)> = sqlx::query_as(
        "select read_at, answer, speculative from sync.held_read
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
    let (read_at, answer, speculative) = row?;
    if speculative
        && let Err(error) = sqlx::query(
            "update sync.held_read set speculative = false
              where source_id = $1 and entity_id = $2 and ask = $3 and speculative",
        )
        .bind(source_id)
        .bind(entity_id)
        .bind(ask)
        .execute(pool)
        .await
    {
        tracing::debug!(%error, "held_read: not marked read");
    }
    Some((read_at, answer))
}

/// Hold `answer` to `ask` about `entity_id`, read at `read_at` through
/// `recipe`, over whatever was held, as a person's read; a row over its ask's
/// cap ([`row_cap`]) is not written, and the speculative rows, then the
/// oldest, go while the table is over [`BOUND`].
pub async fn put(
    pool: &PgPool,
    source_id: &str,
    entity_id: &str,
    ask: &str,
    recipe: &serde_json::Value,
    read_at: DateTime<Utc>,
    answer: &serde_json::Value,
) {
    let _ = put_as(
        pool, source_id, entity_id, ask, recipe, read_at, answer, false,
    )
    .await;
}

/// What [`put_as`] did with an answer of so many bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Put {
    /// Held.
    Held(usize),
    /// Over its ask's cap ([`row_cap`]), not written.
    TooLarge(usize),
    /// Not written: it does not serialize, or the store refused it (a text
    /// with a NUL, which `jsonb` cannot hold; a source removed meanwhile).
    Refused,
}

/// [`put`], `speculative` where the sync holds it for a page nobody opened
/// yet: such a row stays a person's where it was one.
#[allow(clippy::too_many_arguments)]
pub async fn put_as(
    pool: &PgPool,
    source_id: &str,
    entity_id: &str,
    ask: &str,
    recipe: &serde_json::Value,
    read_at: DateTime<Utc>,
    answer: &serde_json::Value,
    speculative: bool,
) -> Put {
    let Ok(bytes) = serde_json::to_vec(answer).map(|b| b.len()) else {
        return Put::Refused;
    };
    if bytes > row_cap(ask) {
        return Put::TooLarge(bytes);
    }
    let written = sqlx::query(
        "insert into sync.held_read
                (source_id, entity_id, ask, recipe, read_at, bytes, answer, speculative)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         on conflict (source_id, entity_id, ask) do update
           set recipe = excluded.recipe, read_at = excluded.read_at,
               bytes = excluded.bytes, answer = excluded.answer,
               speculative = sync.held_read.speculative and excluded.speculative
           where sync.held_read.read_at <= excluded.read_at",
    )
    .bind(source_id)
    .bind(entity_id)
    .bind(ask)
    .bind(recipe)
    .bind(read_at)
    .bind(i32::try_from(bytes).unwrap_or(i32::MAX))
    .bind(answer)
    .bind(speculative)
    .execute(pool)
    .await;
    if let Err(error) = written {
        // A source removed while its read was out has no row to name.
        tracing::debug!(%error, "held_read: not held");
        return Put::Refused;
    }
    bound(pool).await;
    Put::Held(bytes)
}

/// Over [`BOUND`], the speculative rows go first, oldest first, then the
/// oldest a person read: one statement.
async fn bound(pool: &PgPool) {
    let _ = sqlx::query(
        "delete from sync.held_read where (source_id, entity_id, ask) in (
           select source_id, entity_id, ask from (
             select source_id, entity_id, ask,
                    sum(bytes) over (order by speculative, read_at desc) as running
               from sync.held_read) r
            where r.running > $1)",
    )
    .bind(BOUND)
    .execute(pool)
    .await;
}

/// Drop every read held from `source_id`, and what the sync's pass knows it
/// held: its credential changed, and a new one may read less.
pub async fn forget(pool: &PgPool, source_id: &str) {
    for sql in [
        "delete from sync.held_read where source_id = $1",
        "delete from sync.held_section where source_id = $1",
    ] {
        if let Err(error) = sqlx::query(sql).bind(source_id).execute(pool).await {
            tracing::debug!(%error, "held_read: not forgotten");
        }
    }
}
