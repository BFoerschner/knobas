//! Demo mode, and M0's one-entry source registry.
//!
//! In M0 the two are the same thing: the only adapter that exists is the
//! compiled-in mock, so "which source can I sync?" and "what does the demo
//! load?" have the same answer. When real adapters arrive this module becomes
//! a registry keyed on `knobas.source_config.kind`, and demo mode stays one
//! caller of it.
//!
//! Both entry points are plain `pool` functions, not commands: the Tauri layer
//! above them adds nothing but argument decoding, and a `#[tauri::command]`
//! cannot be called from a test.

use knobas_source::Source;
use knobas_source_mock::MockSource;
use knobas_sync::{SyncError, SyncReport};
use sqlx::PgPool;

/// Why a demo load or a sync did not happen.
#[derive(Debug, thiserror::Error)]
pub enum DemoError {
    /// No adapter answers to this id. In M0 that is everything but `mock`.
    #[error("no source with id {0:?} -- M0 ships only the mock source")]
    UnknownSource(String),

    /// The adapter exists, but nothing has configured it yet.
    #[error("source {0:?} is not configured -- load the demo data first")]
    NotConfigured(String),

    /// Registering the source failed.
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),

    /// The sync run itself failed.
    #[error("{0}")]
    Sync(#[from] SyncError),
}

/// Register the demo source if it is not registered yet, then sync it in full.
///
/// Idempotent in both halves: the insert leaves an existing configuration
/// alone (keeping the cursor a previous run stored), and a full sync upserts
/// the same rows rather than adding to them. Double-clicking the button is
/// therefore harmless, which is the whole reason this is one function.
///
/// # Errors
///
/// [`DemoError::Db`] if the registration fails, [`DemoError::Sync`] if the run
/// does.
pub async fn demo_load_inner(pool: &PgPool) -> Result<SyncReport, DemoError> {
    let source = MockSource::new();
    register(pool, &source).await?;
    // `None`: demo mode means "give me the whole fixture", regardless of what
    // a previous run recorded. The run is idempotent, so this costs rows
    // rewritten, not rows duplicated.
    Ok(knobas_sync::run_once(pool, &source, None).await?)
}

/// Sync one **configured** source, resuming from where it last stopped.
///
/// Both halves of that are checked, because failing either one silently is
/// worse than refusing: an id no adapter answers to would do nothing at all,
/// and an id with no `source_config` row would run a full sync whose cursor
/// the engine then has nowhere to persist (`run_once` updates a row, and
/// deliberately never invents one) -- so every later call would sync
/// everything again, for ever, with no sign that anything was wrong.
///
/// The cursor is read outside the run's advisory lock, so a concurrent run of
/// the same source can move it between the read and the lock; the worst case
/// is one redundant fetch from a position that has already advanced, and the
/// upserts are idempotent. Reading it inside the transaction that holds the
/// lock is the real fix, and cursor lifecycle belongs to M1 stream F.
///
/// # Errors
///
/// [`DemoError::UnknownSource`] if no adapter answers to `source_id`,
/// [`DemoError::NotConfigured`] if it has no `knobas.source_config` row,
/// [`DemoError::Db`] if the lookup fails, [`DemoError::Sync`] if the run does.
pub async fn sync_now_inner(pool: &PgPool, source_id: &str) -> Result<SyncReport, DemoError> {
    let source =
        adapter_for(source_id).ok_or_else(|| DemoError::UnknownSource(source_id.to_owned()))?;
    let cursor = stored_cursor(pool, source_id)
        .await?
        .ok_or_else(|| DemoError::NotConfigured(source_id.to_owned()))?;
    Ok(knobas_sync::run_once(pool, source.as_ref(), cursor).await?)
}

/// The adapter for `source_id`, if knobas has one compiled in.
///
/// Keyed on the adapter's own descriptor rather than on a literal: the id a
/// source answers to is the source's to declare, and duplicating it here is
/// how the two drift apart.
fn adapter_for(source_id: &str) -> Option<Box<dyn Source>> {
    let mock = MockSource::new();
    (mock.descriptor().id == source_id).then(|| Box::new(mock) as Box<dyn Source>)
}

/// The configured position of `source_id`.
///
/// Two layers of absence, kept apart on purpose: the outer `None` means the
/// source has no `knobas.source_config` row at all, which is a caller error;
/// the inner one means it is configured but has never synced, which is an
/// ordinary full sync.
async fn stored_cursor(
    pool: &PgPool,
    source_id: &str,
) -> Result<Option<Option<String>>, sqlx::Error> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("select cursor from knobas.source_config where id = $1")
            .bind(source_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(cursor,)| cursor))
}

/// Write the source's configuration row, unless it already has one.
///
/// `do nothing` rather than an upsert: the stored row is the *user's* -- it
/// carries the cursor, and from M1 the sync interval and enabled flag -- and
/// re-registering must not reset any of that. Every column comes from the
/// descriptor, so a renamed adapter shows up here without this function
/// knowing anything about it.
async fn register(pool: &PgPool, source: &dyn Source) -> Result<(), sqlx::Error> {
    let descriptor = source.descriptor();
    sqlx::query(
        r#"insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
           values ($1, $2, $3, '', 'none')
           on conflict (id) do nothing"#,
    )
    // The fixture is compiled into the binary and needs no credentials, hence
    // the empty `base_url` and `auth_kind = 'none'`: there is nothing to reach
    // and nothing to unlock. A real adapter fills both from its config.
    .bind(&descriptor.id)
    .bind(&descriptor.adapter_kind)
    .bind(&descriptor.name)
    .execute(pool)
    .await?;
    Ok(())
}
