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

use knobas_source::{Source, SourceDescriptor};
use knobas_source_mock::MockSource;
use knobas_sync::{ProgressSink, SyncError, SyncReport, SyncTrigger};
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

    /// The sync run itself failed, for this source.
    ///
    /// The id is carried rather than derived: [`SyncError`] does not know
    /// which source it belongs to, and `unauthorized` is precisely the case
    /// the sources view routes by `source_id` (see
    /// [`crate::IpcError::from_sync_error`]).
    #[error("{error}")]
    Sync {
        source_id: String,
        #[source]
        error: SyncError,
    },
}

/// What a demo failure looks like on the bridge.
///
/// It lives here rather than in `error.rs` because this is the only module
/// that knows what each variant *means*: which of them a user can act on, and
/// which source the failure belongs to. `Sync` keeps the adapter's own
/// classification by deferring to [`crate::IpcError`]'s `SyncError`
/// conversion, so a 401 mid-demo-load is still a 401 by the time the sources
/// view sees it.
impl From<DemoError> for crate::IpcError {
    fn from(error: DemoError) -> Self {
        match error {
            // Nothing the user can do: no such adapter is compiled in.
            DemoError::UnknownSource(id) => crate::IpcError::not_found(format!(
                "no source with id {id:?} -- M0 ships only the mock source"
            ))
            .with_source(id),
            // Add the source (or load the demo data) and it will work.
            DemoError::NotConfigured(id) => crate::IpcError::not_ready(format!(
                "source {id:?} is not configured -- load the demo data first"
            ))
            .with_source(id),
            DemoError::Db(err) => crate::IpcError::internal(err),
            DemoError::Sync { source_id, error } => {
                crate::IpcError::from_sync_error(&error, Some(&source_id))
            }
        }
    }
}

/// Register the demo source if it is not registered yet, then sync it in full.
///
/// Idempotent in both halves: the registration refreshes the descriptor's own
/// columns and leaves the rest of an existing configuration alone (the cursor
/// a previous run stored included), and a full sync upserts the same rows
/// rather than adding to them. Double-clicking the button is therefore
/// harmless, which is the whole reason this is one function.
///
/// # Errors
///
/// [`DemoError::Db`] if the registration fails, [`DemoError::Sync`] if the run
/// does.
pub async fn demo_load_inner(pool: &PgPool) -> Result<SyncReport, DemoError> {
    let source = MockSource::new();
    register(pool, &source.descriptor()).await?;
    // `None`: demo mode means "give me the whole fixture", regardless of what
    // a previous run recorded. The run is idempotent, so this costs rows
    // rewritten, not rows duplicated.
    knobas_sync::run_once(pool, &source, None)
        .await
        .map_err(|error| DemoError::Sync {
            source_id: source.descriptor().id,
            error,
        })
}

/// Everything a sync needs before it can run: which adapter, from where, and
/// the log row that already records it.
///
/// Returned as a unit because `sync_now` has to hand the run id back to the
/// caller *before* the run executes (ruling P3), so the three cannot be
/// resolved lazily inside the run.
pub struct PreparedSync {
    /// The adapter that answers to this id.
    pub source: Box<dyn Source>,
    /// Where it should resume, or `None` for a full sync.
    pub cursor: Option<String>,
    /// The open `knobas.sync_run` row. Closing it is
    /// [`knobas_sync::run`]'s job.
    pub run_id: i64,
}

/// Resolve a sync of one **configured** source and open its log row.
///
/// This module's whole remaining share of a sync: which adapter answers to
/// `source_id`, and where it left off. The composition around the run --
/// phases, classification, closing the log row -- is [`knobas_sync::run`],
/// where stream F's scheduler can extend it without importing this module.
///
/// Both halves of "configured" are checked, because failing either one
/// silently is worse than refusing: an id no adapter answers to would do
/// nothing at all, and an id with no `source_config` row would run a full sync
/// whose cursor the engine then has nowhere to persist (`run_once` updates a
/// row, and deliberately never invents one) -- so every later call would sync
/// everything again, for ever, with no sign that anything was wrong. Neither
/// refusal writes a log line: neither is a run, and a diagnostics view showing
/// a phantom run for a typo would be worse than showing nothing.
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
/// [`DemoError::Db`] if the lookup or the log write fails.
pub async fn prepare_sync(
    pool: &PgPool,
    source_id: &str,
    trigger: SyncTrigger,
) -> Result<PreparedSync, DemoError> {
    let source =
        adapter_for(source_id).ok_or_else(|| DemoError::UnknownSource(source_id.to_owned()))?;
    let cursor = stored_cursor(pool, source_id)
        .await?
        .ok_or_else(|| DemoError::NotConfigured(source_id.to_owned()))?;

    // Last, and only once the two refusals above are past: an open row for a
    // run that was never going to happen is a phantom in the diagnostics view.
    let run_id = knobas_sync::run_log::start(pool, source_id, trigger).await?;
    Ok(PreparedSync {
        source,
        cursor,
        run_id,
    })
}

/// Prepare a manual sync and run it to completion, returning its run id.
///
/// The **blocking** composition, which is what a test wants: the run is over
/// by the time this returns. The commands do not use it -- `sync_now` returns
/// the id and lets the run continue on its own task (ruling P3) -- so this is
/// the shape that makes "did the run do what it should" assertable without
/// polling.
///
/// # Errors
///
/// Whatever [`prepare_sync`] refuses with, or [`DemoError::Sync`] if the run
/// itself fails.
pub async fn sync_now_inner(
    pool: &PgPool,
    source_id: &str,
    progress: Option<&dyn ProgressSink>,
) -> Result<i64, DemoError> {
    let prepared = prepare_sync(pool, source_id, SyncTrigger::Manual).await?;
    let run_id = prepared.run_id;
    knobas_sync::run(
        pool,
        prepared.source.as_ref(),
        prepared.cursor,
        run_id,
        progress,
    )
    .await
    .map_err(|error| DemoError::Sync {
        source_id: source_id.to_owned(),
        error,
    })?;
    Ok(run_id)
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

/// Write the source's configuration row, refreshing the columns the descriptor
/// owns if it already has one.
///
/// The row has two kinds of column in it, and they belong to different people:
///
/// * The **descriptor's** -- `kind` and `display_name`. The adapter is the
///   authority on these, so re-registering overwrites them: renaming an
///   adapter, or shipping a version that reports a different kind, has to show
///   up here. `do nothing` froze them at whatever the first registration saw,
///   for the lifetime of the row.
/// * The **user's** -- the cursor, and from M1 the sync interval and the
///   enabled flag. Those are deliberately absent from the update: re-running
///   the demo load must not throw away a sync position or a setting.
///
/// Both halves come from the descriptor rather than from a literal, so this
/// function knows nothing about which adapter it is registering.
async fn register(pool: &PgPool, descriptor: &SourceDescriptor) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
           values ($1, $2, $3, '', 'none')
           on conflict (id) do update set
             kind         = excluded.kind,
             display_name = excluded.display_name"#,
    )
    // The fixture is compiled into the binary and needs no credentials, hence
    // the empty `base_url` and `auth_kind = 'none'`: there is nothing to reach
    // and nothing to unlock. A real adapter fills both from its config.
    //
    // They are left out of the update for that reason too -- a real source's
    // are the user's to set, and an M1 adapter that re-registers must not
    // reset the URL somebody typed.
    .bind(&descriptor.id)
    .bind(&descriptor.adapter_kind)
    .bind(&descriptor.name)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A descriptor of the mock's shape, under an id no other test uses.
    fn descriptor(id: &str) -> SourceDescriptor {
        SourceDescriptor {
            id: id.to_owned(),
            ..MockSource::new().descriptor()
        }
    }

    async fn row(pool: &PgPool, id: &str) -> (String, String, Option<String>) {
        sqlx::query_as("select kind, display_name, cursor from knobas.source_config where id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// Re-registering refreshes what the descriptor owns and leaves what the
    /// user owns alone. Both halves in one test, because the interesting thing
    /// is that one statement does both.
    #[tokio::test]
    async fn re_registering_refreshes_the_descriptor_columns_and_keeps_the_cursor() {
        let pool = knobas_db::test_util::test_pool().await;
        knobas_db::migrate::run(&pool).await.unwrap();
        // Unique per run: this table is shared with every other test in the
        // binary, and the row is asserted on by absolute value.
        let id = format!("registry-{}", std::process::id());
        sqlx::query("delete from knobas.source_config where id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();

        let mut d = descriptor(&id);
        d.adapter_kind = "before".to_owned();
        d.name = "Before".to_owned();
        register(&pool, &d).await.unwrap();
        assert_eq!(
            row(&pool, &id).await,
            ("before".to_owned(), "Before".to_owned(), None)
        );

        // A position the user's syncs have earned since.
        sqlx::query("update knobas.source_config set cursor = 'earned' where id = $1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();

        d.adapter_kind = "after".to_owned();
        d.name = "After".to_owned();
        register(&pool, &d).await.unwrap();

        assert_eq!(
            row(&pool, &id).await,
            (
                "after".to_owned(),
                "After".to_owned(),
                Some("earned".to_owned())
            ),
            "the descriptor's columns must follow the adapter, the cursor must not"
        );
    }
}
