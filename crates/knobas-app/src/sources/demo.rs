//! Demo mode: register the compiled-in mock, load the Tidewater fixture, and
//! import the checked-in estate beside it.
//!
//! M0 had a second job here -- a one-entry source registry, because the mock
//! was the only adapter there was. That is [`super::registry`]'s now, and the
//! M0 *Sync now* path this module used to own (`prepare_sync`, the blocking
//! `sync_now_inner`, `knobas_sync::run`) is the scheduler's. They were deleted
//! rather than left beside it: §10.8's warning is that a stream which treats
//! the engine as frozen ends the milestone with two sync paths, and two is how
//! a fix lands in one of them.
//!
//! The entry points are plain functions, not commands: the Tauri layer above
//! them adds nothing but argument decoding, and a `#[tauri::command]` cannot
//! be called from a test. [`demo_load_inner`] is the load over a pool alone;
//! [`demo_load_announced`] is the same load followed by the `sync:state` its
//! run owes (#240), for a caller that has a window to tell.
//!
//! ## Why the estate is here and not in the fixture (#440)
//!
//! The work half is fiction: `fixtures/tidewater/work.json` is twenty-one
//! invented tickets, because a demo corpus of somebody's real Jira is a demo
//! nobody can ship. The estate half is the opposite and deliberately so --
//! ADR-0013, *the real container is the witness*: `testenv/hetzner/estate.json`
//! describes the machines this repo is actually developed and tested on, so
//! what the demo's Tree shows is an estate that exists rather than one drawn
//! to fit the model. Spec #427 says it in as many words -- *nothing
//! Tidewater-shaped is invented for assets*.
//!
//! It goes through [`crate::assets::apply_import`] and not through a loader of
//! its own, and that is the decision worth reading. `assets::HAND_EDITED`
//! tells an import's writes from a person's by their `actor`, and a second
//! loader writing `actor = 'user'` would freeze every property it touched
//! against every later import -- the demo profile would be the one profile in
//! which the merge rule is wrong. One writer, one merge rule, and the demo
//! inherits the import's idempotence for free.

use knobas_source::{Source, SourceDescriptor};
use knobas_source_mock::MockSource;
use knobas_sync::scheduler::{SyncEvents, status_for};
use knobas_sync::{SyncError, SyncReport};
use sqlx::PgPool;

/// The estate the demo profile draws, embedded.
///
/// `include_str!` rather than a path read at run time, for
/// `knobas_source_mock`'s reason one crate over: a bundled `knobas --demo` has
/// no repository under it, and a file the binary went looking for would be a
/// demo that works from a checkout and nowhere else. It is the same bytes
/// `crates/knobas-app/tests/assets_ipc.rs` and `knobas-core`'s
/// `tests/estate_file.rs` embed, so the three fail together the day the file
/// stops being an estate file.
const ESTATE_FILE: &str = include_str!("../../../../testenv/hetzner/estate.json");

/// Why a demo load did not happen.
#[derive(Debug, thiserror::Error)]
pub enum DemoError {
    /// Registering the source failed.
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),

    /// Importing the estate failed.
    ///
    /// Its own variant rather than a flattened `Internal`, because the two
    /// halves of a demo load fail for unrelated reasons and a reader who is
    /// told *"the demo failed"* cannot tell a broken fixture from a broken
    /// estate file. The [`crate::IpcError`] underneath keeps the import's own
    /// classification and its own sentence.
    #[error("the estate: {0}")]
    Estate(#[from] crate::IpcError),

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
            DemoError::Db(err) => crate::IpcError::internal(err),
            DemoError::Sync { source_id, error } => {
                crate::IpcError::from_sync_error(&error, Some(&source_id))
            }
            // Already an `IpcError`: the import classified its own failure
            // (`invalid` for a file that is not an estate file, `conflict` for
            // a lost race), and re-coding it here would throw that away.
            DemoError::Estate(error) => error,
        }
    }
}

/// Register the demo source if it is not registered yet, sync it in full, and
/// import the estate.
///
/// Idempotent in all three halves: the registration refreshes the descriptor's
/// own columns and leaves the rest of an existing configuration alone (the
/// cursor a previous run stored included), a full sync upserts the same rows
/// rather than adding to them, and a second import of an unchanged file is
/// all-known and writes one summary line. Double-clicking the button is
/// therefore harmless, and so is starting the demo profile again tomorrow,
/// which is the whole reason this is one function.
///
/// **The work first, then the estate**, and the order is not arbitrary: the
/// run is what the answer is about, and a caller that got a `SyncReport` back
/// from a load whose sync had not happened would be reading a number about
/// nothing. The two halves share no rows -- the mirror is `sync.*`, the estate
/// is `knobas.asset` and `knobas.route` -- so neither can spoil the other, and
/// an estate that will not import leaves a demo profile with its work in it
/// and says why.
///
/// # Errors
///
/// [`DemoError::Db`] if the registration fails, [`DemoError::Sync`] if the run
/// does, [`DemoError::Estate`] if the estate will not import.
pub async fn demo_load_inner(pool: &PgPool) -> Result<SyncReport, DemoError> {
    let source = MockSource::new();
    register(pool, &source.descriptor()).await?;
    // `None`: demo mode means "give me the whole fixture", regardless of what
    // a previous run recorded. The run is idempotent, so this costs rows
    // rewritten, not rows duplicated.
    let report = knobas_sync::run_once(pool, &source, None)
        .await
        .map_err(|error| DemoError::Sync {
            source_id: source.descriptor().id,
            error,
        })?;

    let estate = crate::assets::apply_import(pool, ESTATE_FILE).await?;
    // Logged rather than returned: `SyncReport` is a frozen wire shape and
    // this is a second load's counts, not the run's. `Load demo data` reports
    // the mirror it can count; what the estate did is in the app log and in
    // every imported asset's own history.
    tracing::info!(
        assets = estate.value.assets_created,
        routes = estate.value.routes_created,
        properties_set = estate.value.properties_set,
        properties_kept = estate.value.properties_kept,
        monitors_kept = estate.value.monitors_kept,
        "the demo estate is loaded"
    );

    Ok(report)
}

/// [`demo_load_inner`], followed by the `sync:state` its run owes (#240).
///
/// The contract's event table says `sync:state` fires on every run transition
/// and P3 was granted as "all runs emit coarse `sync:state`". The scheduler
/// keeps that promise for every scheduled and manual run; the demo load syncs
/// through the bare [`knobas_sync::run_once`], which emits nothing, so it was
/// the one run whose ending the window could not hear -- and the projects
/// store re-lists the census on a terminal `sync:state` and on nothing else.
/// Ruled at triage (2026-09-02) over a second frontend patch and over routing
/// the load through the scheduler: this is compliance with the existing rule,
/// not a new event, and the scheduler route would change what the command
/// returns.
///
/// Two functions rather than a parameter on one: the demo test binary drives
/// [`demo_load_inner`] against a scratch database with nothing listening, and
/// the command is the caller that has a window to tell.
///
/// The status is read back through [`status_for`], not hand-built. The
/// scheduler's own emits no longer read it whole -- since #304 they compose the
/// run's half from the run they are about -- but there is no run here to
/// compose from: the load logs no `sync_run` row at all (out of scope by
/// ruling), so the source-level read *is* the whole answer, and the `coalesce`
/// that decides #304's `run_id` has nothing to choose between. The registration
/// wrote the `source_config` row the query is keyed on, so the read still
/// answers for the mock: the status is terminal (`running: false`, no
/// `run_id`), which is all the store asks of it. A failed read is logged and
/// not raised, as in the scheduler -- the corpus already landed, and an event
/// the window missed is not a reason to report the load as failed.
///
/// # Errors
///
/// [`demo_load_inner`]'s.
pub async fn demo_load_announced(
    pool: &PgPool,
    events: &dyn SyncEvents,
) -> Result<SyncReport, DemoError> {
    let result = demo_load_inner(pool).await;
    // A `Db` error is the registration failing, so there was no run to
    // announce. Every other variant means the run happened: a `Sync` error is
    // a run that ended, and its ending is a transition too, and an `Estate`
    // error is raised *after* the run returned, so its status is as worth
    // announcing as a clean load's.
    if !matches!(result, Err(DemoError::Db(_))) {
        let source_id = MockSource::new().descriptor().id;
        match status_for(pool, &source_id).await {
            Ok(Some(status)) => events.sync_state(status),
            Ok(None) => tracing::warn!(source_id, "the demo source has no status row to announce"),
            Err(error) => {
                tracing::warn!(source_id, %error, "reading sync status for the event failed");
            }
        }
    }
    result
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
