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

/// Sync one configured source, resuming from where it last stopped.
///
/// # Errors
///
/// [`DemoError::UnknownSource`] if no adapter answers to `source_id`,
/// [`DemoError::Db`] if the stored cursor cannot be read, [`DemoError::Sync`]
/// if the run fails.
pub async fn sync_now_inner(pool: &PgPool, source_id: &str) -> Result<SyncReport, DemoError> {
    let source =
        adapter_for(source_id).ok_or_else(|| DemoError::UnknownSource(source_id.to_owned()))?;
    let cursor = stored_cursor(pool, source_id).await?;
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

/// Where the last run of `source_id` stopped, or `None` for a full sync.
///
/// `None` covers both "configured but never synced" and "not configured at
/// all": either way there is no position to resume from.
async fn stored_cursor(pool: &PgPool, source_id: &str) -> Result<Option<String>, sqlx::Error> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("select cursor from knobas.source_config where id = $1")
            .bind(source_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(cursor,)| cursor))
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
