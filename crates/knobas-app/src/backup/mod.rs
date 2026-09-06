//! Scheduled backup export (spec §14 / §16.12, issue #38).
//!
//! `commands/backup.rs` is a set of shims over this module; every decision
//! lives here, with tests, because a `#[tauri::command]` cannot be called from
//! one. The same arrangement `sources/` uses, for the same reason.
//!
//! # What is where
//!
//! * [`policy`] decides *when* -- pure, and tested without a clock or a disk.
//! * [`knobas_db::backup`] does the dump and the restore, because that is
//!   where the bundled `pg_dump` and the connection's password are.
//! * This file is the rest: where archives go, what is remembered between
//!   runs, retention, and the task that ticks.
//!
//! That task does one thing this module does not otherwise own: it sweeps the
//! observations `time::passive` has kept past its retention window (#315).
//! Two retention rules, one clock -- see [`tick`] for why the clock is the one
//! that belongs here and the rule is the one that does not.
//!
//! # Where the schedule is stored
//!
//! `knobas.setting`, whose migration (`0002`, comment 6) names "later the
//! export schedule" as one of the things it exists for. So this feature needs
//! no migration, which matters: migrations are single-writer and
//! orchestrator-owned (roadmap §3 rule 1).
//!
//! Storing the *last run* there rather than deriving it from the newest file
//! on disk is deliberate. A user who tidies their backup directory has not
//! asked for a backup to run, and a rule keyed on the directory would run one
//! every time the app started after they did.
//!
//! # Where the scheduler lives
//!
//! In its own task here, next to the sync scheduler rather than inside it.
//! `knobas_sync::scheduler` is per-*source*: everything it does is driven by
//! rows in `knobas.source_config`, its concurrency limit is about being polite
//! to remote systems, and its module docs forbid it knowing about anything but
//! adapters. A backup is none of those things. The design document does not
//! say where this belongs -- §14 says only "scheduled automatic exports as
//! backups" -- so it was raised rather than settled silently, and **ratified
//! here on 2026-08-28** (issue #38).
//!
//! # There is no settings surface yet
//!
//! §14 asks for "a small settings surface (export now / schedule / restore)".
//! knobas has no settings view to hang one off, so it is **issue #69**,
//! blocked on that view existing. What is here is what #69 will call: the four
//! commands in [`crate::commands::backup`] and the typed mirror beside them.

pub mod policy;
pub mod share;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use knobas_db::backup::BackupError;
use knobas_db::embedded::Connector;
use knobas_secrets::SecretStore;
use sqlx::PgPool;
use tauri::Manager;
use tokio_util::sync::CancellationToken;

pub use policy::BackupSchedule;
pub use share::ShareParts;

/// `knobas.setting` key holding the [`BackupSchedule`] as JSON.
const SCHEDULE_KEY: &str = "backup.schedule";

/// `knobas.setting` key holding the last successful [`BackupRecord`].
const LAST_KEY: &str = "backup.last";

/// How often the nightly task asks whether a backup is due.
///
/// A poll rather than a computed sleep, for the reason the sync scheduler
/// polls: a laptop's monotonic clock does not advance while it is suspended,
/// so a task that slept until 03:00 wakes up whenever the machine does and has
/// no idea what time it is. A minute of latency on a nightly job is nothing;
/// a job that never fires is the failure.
const TICK: Duration = Duration::from_secs(60);

/// How long after start-up the first check waits.
///
/// The database has just come up and the first sync is starting; a first-run
/// backup competing with that is a slow window for no reason.
const STARTUP_DELAY: Duration = Duration::from_secs(30);

/// What one successful export produced.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BackupRecord {
    pub taken_at: DateTime<Utc>,
    /// The file name inside the backup directory, not a path: the directory
    /// moves with the profile, and a stored absolute path would outlive it.
    pub file: String,
    pub bytes: i64,
}

/// One archive sitting in the backup directory.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArchiveFile {
    pub file: String,
    pub bytes: i64,
}

/// Everything the settings dialog draws (issue #69).
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackupStatus {
    pub schedule: BackupSchedule,
    /// Absolute path of the directory the archives live in.
    pub directory: String,
    pub last: Option<BackupRecord>,
    /// `None` when the schedule is off.
    pub next_due_at: Option<DateTime<Utc>>,
    /// What is on disk right now, newest first.
    pub archives: Vec<ArchiveFile>,
}

/// Anything an export or a restore can fail with.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Backup(#[from] BackupError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("there is no archive called {0} in the backup directory")]
    NoSuchArchive(String),
    #[error("a share export needs at least one part; every one of them is switched off")]
    NoParts,
    #[error("the keychain could not be read: {0}")]
    Secret(#[from] knobas_secrets::SecretError),
}

impl From<ExportError> for crate::IpcError {
    fn from(error: ExportError) -> Self {
        match &error {
            // The one refusal a user can act on: pick an empty database, or
            // wait for M4's merge-restore.
            ExportError::Backup(BackupError::TargetNotEmpty { .. }) => Self::conflict(error),
            ExportError::NoSuchArchive(_) => Self::not_found(error),
            // The one refusal the *caller* made: every toggle off. `invalid`
            // rather than `internal`, because nothing went wrong -- the
            // request cannot be honoured as asked.
            ExportError::NoParts => Self::invalid(error),
            _ => Self::internal(error),
        }
    }
}

/// Everything the backup commands and the nightly task share.
///
/// Reached through [`state`] rather than declared as a command argument, for
/// the reason `sources::SourcesState` is (carry-over §10.6(a)): it is managed
/// only once the database is up, and a command that declared it would be
/// rejected by Tauri during bring-up with a bare string the frontend cannot
/// branch on.
pub struct BackupState {
    /// For the settings reads and writes.
    pub pool: PgPool,
    /// How `pg_dump` reaches the server. Not the application pool: the tools
    /// are separate processes and speak libpq, not sqlx.
    pub connector: Connector,
    /// Where archives are written -- inside the profile, so the demo profile
    /// cannot write into the real one's backups (P13).
    pub directory: PathBuf,
    /// The same store `sources` uses, so that a restore can ask whether this
    /// machine actually holds a credential for a source the archive brought.
    /// See [`restore`].
    pub secrets: Arc<dyn SecretStore>,
    cancel: CancellationToken,
}

impl BackupState {
    /// The state, without starting the nightly task.
    ///
    /// Public because `tests/backup_ipc.rs` builds one over a scratch database
    /// and a temporary directory; there is nothing in it a production caller
    /// could misuse, unlike `AppState::over_pool`, so it is not behind a
    /// feature.
    #[must_use]
    pub fn new(
        pool: PgPool,
        connector: Connector,
        directory: PathBuf,
        secrets: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            pool,
            connector,
            directory,
            secrets,
            cancel: CancellationToken::new(),
        }
    }
}

/// The backup state, or the one honest refusal for a call that beat bring-up.
///
/// # Errors
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while the database
/// has not come up.
pub fn state<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<tauri::State<'_, Arc<BackupState>>, crate::IpcError> {
    app.try_state::<Arc<BackupState>>()
        .ok_or_else(|| crate::IpcError::not_ready("the database is still starting".to_owned()))
}

/// One `knobas.setting` row, decoded -- or `None` when it is absent *or no
/// longer decodes*.
///
/// Those two are deliberately the same answer. A value that has stopped
/// parsing -- an older knobas, a hand-edited row -- is a thing this feature
/// cannot read, and the response to "I cannot read your schedule" is to keep
/// backing up nightly, not to stop.
async fn read_setting<T: serde::de::DeserializeOwned>(
    pool: &PgPool,
    key: &str,
) -> Result<Option<T>, sqlx::Error> {
    let stored: Option<serde_json::Value> =
        sqlx::query_scalar("select value from knobas.setting where key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(stored.and_then(|value| serde_json::from_value(value).ok()))
}

/// Store one `knobas.setting` row.
async fn write_setting<T: serde::Serialize>(
    pool: &PgPool,
    key: &str,
    value: &T,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into knobas.setting (key, value) values ($1, $2)
         on conflict (key) do update set value = excluded.value, updated_at = now()",
    )
    .bind(key)
    .bind(serde_json::to_value(value).unwrap_or(serde_json::Value::Null))
    .execute(pool)
    .await?;
    Ok(())
}

/// The stored schedule, or the default when nothing has been stored.
///
/// # Errors
/// [`sqlx::Error`] if the read fails.
pub async fn load_schedule(pool: &PgPool) -> Result<BackupSchedule, sqlx::Error> {
    Ok(read_setting::<BackupSchedule>(pool, SCHEDULE_KEY)
        .await?
        .unwrap_or_default()
        .clamped())
}

/// Store the schedule, clamped into range on the way in.
///
/// # Errors
/// [`sqlx::Error`] if the write fails.
pub async fn save_schedule(
    pool: &PgPool,
    schedule: BackupSchedule,
) -> Result<BackupSchedule, sqlx::Error> {
    let schedule = schedule.clamped();
    write_setting(pool, SCHEDULE_KEY, &schedule).await?;
    Ok(schedule)
}

/// The last successful export, if there has been one.
///
/// # Errors
/// [`sqlx::Error`] if the read fails.
pub async fn last_record(pool: &PgPool) -> Result<Option<BackupRecord>, sqlx::Error> {
    read_setting(pool, LAST_KEY).await
}

/// Take a backup now, whatever the schedule says.
///
/// Writes the archive, records it, and deletes whatever retention has aged
/// out. Retention runs **after** the write, so a failed export never costs an
/// old archive.
///
/// # Errors
/// [`ExportError`] if the dump fails or the record cannot be stored.
pub async fn export_now(state: &BackupState) -> Result<BackupRecord, ExportError> {
    let taken_at = Utc::now();
    let file = policy::archive_name(&taken_at.with_timezone(&Local));
    let path = state.directory.join(&file);

    let bytes = knobas_db::backup::dump(&state.connector, &path).await?;
    let record = BackupRecord {
        taken_at,
        file,
        bytes: i64::try_from(bytes).unwrap_or(i64::MAX),
    };

    write_setting(&state.pool, LAST_KEY, &record).await?;

    let schedule = load_schedule(&state.pool).await?;
    prune(state, schedule.keep)?;
    tracing::info!(file = %record.file, bytes = record.bytes, "backup exported");
    Ok(record)
}

/// Take a backup if the schedule says one is due; otherwise do nothing.
///
/// Returns what was written, or `None` when nothing was due. This is the whole
/// of the nightly job -- the task around it only decides how often to ask.
///
/// # Errors
/// [`ExportError`] if the schedule cannot be read, or the export fails.
pub async fn export_if_due(state: &BackupState) -> Result<Option<BackupRecord>, ExportError> {
    let schedule = load_schedule(&state.pool).await?;
    let last = last_record(&state.pool)
        .await?
        .map(|record| record.taken_at);
    if !policy::is_due(schedule, last, Utc::now(), &Local) {
        return Ok(None);
    }
    export_now(state).await.map(Some)
}

/// Take a **share export**: the same archive, restricted to `parts`.
///
/// Spec #427 (M4.2). Not a backup, and deliberately not treated as one:
///
/// * it is named `knobas-share-<stamp>.knobas`, so retention cannot age out an
///   archive a person made on purpose to hand to somebody
///   ([`policy::expired`] skips it);
/// * it does not record itself as `backup.last`, so the nightly schedule still
///   measures from the last *backup* and a week of share exports does not look
///   like a week of backups;
/// * it prunes nothing.
///
/// It is still one of this profile's archives -- it is listed by
/// [`status`] and [`restore`] accepts it -- because the archive a colleague is
/// handed is the archive they drop into their own backup directory and
/// restore.
///
/// # Errors
/// [`ExportError::NoParts`] when every toggle is off, and [`ExportError`] if
/// the dump fails.
pub async fn share_export(
    state: &BackupState,
    parts: ShareParts,
) -> Result<BackupRecord, ExportError> {
    if !parts.any() {
        return Err(ExportError::NoParts);
    }
    let taken_at = Utc::now();
    let file = policy::share_name(&taken_at.with_timezone(&Local));
    let path = state.directory.join(&file);

    let bytes = knobas_db::backup::dump_tables(&state.connector, &path, &parts.tables()).await?;
    tracing::info!(file = %file, bytes, ?parts, "share export written");
    Ok(BackupRecord {
        taken_at,
        file,
        bytes: i64::try_from(bytes).unwrap_or(i64::MAX),
    })
}

/// Everything the settings dialog draws.
///
/// # Errors
/// [`ExportError`] if the settings cannot be read.
pub async fn status(state: &BackupState) -> Result<BackupStatus, ExportError> {
    let schedule = load_schedule(&state.pool).await?;
    let last = last_record(&state.pool).await?;
    Ok(BackupStatus {
        next_due_at: policy::next_due(
            schedule,
            last.as_ref().map(|record| record.taken_at),
            Utc::now(),
            &Local,
        ),
        schedule,
        directory: state.directory.display().to_string(),
        last,
        archives: archives(state),
    })
}

/// Restore one of the archives in the backup directory.
///
/// Named by file rather than by path: the archive has to be one knobas wrote,
/// in the directory knobas owns, and a command taking an arbitrary path would
/// let a webview name any file on the machine.
///
/// # Errors
/// [`ExportError::NoSuchArchive`] for a name that is not in the directory, and
/// whatever [`knobas_db::backup::restore`] refuses with -- notably
/// [`BackupError::TargetNotEmpty`] when the database still holds data.
pub async fn restore(state: &BackupState, file: &str) -> Result<(), ExportError> {
    let path = state.directory.join(file);
    if !policy::is_archive_name(file) || !path.is_file() {
        return Err(ExportError::NoSuchArchive(file.to_owned()));
    }
    knobas_db::backup::restore(&state.connector, &path).await?;
    settle_credential_health(state).await?;
    Ok(())
}

/// Say `missing_secret` about every source this machine has no credential for.
///
/// **No archive carries a secret** -- spec §14 puts every credential in the OS
/// keychain and nothing secret ever reaches Postgres -- but `auth_state` is a
/// column on `knobas.source_config`, so a restore brings across what the
/// *other* machine last learned about its own credentials. Left alone, a share
/// export's recipient gets a sources view saying `ok` about systems they
/// cannot reach, and a scheduler that keeps asking.
///
/// So the verdict is recomputed from the one fact this machine can establish
/// without a network: whether its keychain holds a secret under the source's
/// id. It is asked per source rather than assumed, because the assumption is
/// wrong in the other direction too -- a person restoring their own backup
/// onto the machine that took it still has every credential, and
/// `missing_secret` would take those sources out of `trigger_all` until they
/// went and re-entered a password that was never lost.
///
/// A source whose secret *is* here keeps whatever health it arrived with:
/// stale, and settled by the next run, which is what `auth_checked_at` is for.
async fn settle_credential_health(state: &BackupState) -> Result<(), ExportError> {
    let sources: Vec<String> = sqlx::query_scalar("select id from knobas.source_config order by id")
        .fetch_all(&state.pool)
        .await?;
    for id in sources {
        if knobas_secrets::spawn::get(&state.secrets, &id)
            .await?
            .is_some()
        {
            continue;
        }
        // The same statement `knobas_sync::config::set_health` writes, without
        // the event: nothing is listening during a restore, and the sources
        // view re-reads on its next mount.
        sqlx::query(
            "update knobas.source_config
                set auth_state = 'missing_secret',
                    auth_checked_at = now(),
                    auth_detail = $2,
                    secret_expires_at = null
              where id = $1",
        )
        .bind(&id)
        .bind(RESTORED_WITHOUT_A_SECRET)
        .execute(&state.pool)
        .await?;
        tracing::info!(source_id = %id, "restored source has no credential on this machine");
    }
    Ok(())
}

/// What the sources view says about a source an archive brought with no
/// credential to go with it.
const RESTORED_WITHOUT_A_SECRET: &str =
    "restored from an archive; archives never carry credentials, so this source needs its own";


/// The archives on disk, newest first.
///
/// A missing or unreadable directory is an empty list, not an error: "there
/// are no backups yet" is exactly what the settings dialog should say before
/// the first one.
///
/// Regular files only. `read_dir` yields directories as happily as files, and
/// everything downstream of here treats what it returns as something
/// `remove_file` can take -- so a directory called `knobas-....knobas` would
/// be listed to the user as an archive and then fail every export that tried
/// to age it out.
fn archives(state: &BackupState) -> Vec<ArchiveFile> {
    let Ok(entries) = std::fs::read_dir(&state.directory) else {
        return Vec::new();
    };
    let mut found: Vec<ArchiveFile> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            if !policy::is_archive_name(&name) {
                return None;
            }
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some(ArchiveFile {
                bytes: i64::try_from(meta.len()).unwrap_or(i64::MAX),
                file: name,
            })
        })
        .collect();
    found.sort_by(|a, b| b.file.cmp(&a.file));
    found
}

/// Delete the archives retention has aged out.
fn prune(state: &BackupState, keep: u32) -> Result<(), ExportError> {
    let names: Vec<String> = archives(state)
        .into_iter()
        .map(|archive| archive.file)
        .collect();
    for name in policy::expired(&names, keep) {
        let path = state.directory.join(&name);
        std::fs::remove_file(&path).map_err(|source| ExportError::Io { path, source })?;
        tracing::info!(file = %name, "backup pruned");
    }
    Ok(())
}

/// Manage the backup state and start the nightly task.
///
/// Called from `spawn_bring_up` once the database is migrated, beside
/// `sources::start`.
pub fn start<R: tauri::Runtime>(app: &tauri::AppHandle<R>, db: &knobas_db::EmbeddedDb) {
    let profile = app.state::<crate::Profile>().inner().clone();
    // The store `sources::start` built, which ran before this in `spawn_bring_up`
    // and returns `Err` rather than leaving its state unmanaged -- so the
    // fallback below is unreachable. It is an empty store rather than a panic
    // because of which way the mistake would go: an empty keychain makes
    // `restore` call every source `missing_secret`, which is the conservative
    // answer and one a user can act on, where reusing a stale `ok` is a
    // sources view that lies.
    let secrets = app.try_state::<crate::sources::SourcesState>().map_or_else(
        || {
            tracing::error!("the backup service started before the sync engine");
            Arc::new(knobas_secrets::MemoryStore::new()) as Arc<dyn SecretStore>
        },
        |sources| Arc::clone(&sources.secrets),
    );
    let state = Arc::new(BackupState::new(
        db.pool().clone(),
        db.connector(),
        profile.backup_dir(),
        secrets,
    ));

    if !app.manage(Arc::clone(&state)) {
        tracing::error!("the backup service was started twice in one process");
        return;
    }
    tauri::async_runtime::spawn(tick_loop(state));
}

/// Stop the nightly task. Idempotent -- both exit events reach it.
pub fn shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(state) = app.try_state::<Arc<BackupState>>() {
        state.cancel.cancel();
    }
}

/// One pass of the background task: back up if one is due, then sweep.
///
/// A function rather than the body of the loop so that
/// `tests/backup_ipc.rs` can run one pass and read what it did; the loop
/// around it decides only how often.
///
/// **The sweep is not conditional on the export.** Observations age out on a
/// clock of their own (`time::passive::RETENTION_DAYS`), and a person who
/// turns nightly backups off has not asked knobas to keep a record of what
/// they had open for ever -- if anything, the opposite.
///
/// # Why the observation sweep is here at all
///
/// Because this is the app's one wall-clock loop that is not per-source, and
/// because retention is already this module's business -- the archives on disk
/// have a `keep` and this is where it is spent. The rule and the constant stay
/// in `time::passive`, which owns what an observation is; what this module
/// contributes is the clock. `time::passive::prune` records the rest of the
/// reasoning, including why the day read is the wrong home for it.
///
/// Neither half can stop the other: each is logged and the tick returns, for
/// the reason the loop never breaks. A backup that cannot be written is not a
/// reason to stop trying every night, and there is no window to report either
/// failure in from here (the settings dialog reads `backup_status`, which
/// shows the last export that *did* work).
pub async fn tick(state: &BackupState) {
    match export_if_due(state).await {
        Ok(Some(record)) => tracing::info!(file = %record.file, "nightly backup taken"),
        Ok(None) => {}
        Err(error) => tracing::error!(%error, "the nightly backup failed"),
    }
    match crate::time::passive::prune(&state.pool, Utc::now()).await {
        Ok(0) => {}
        Ok(taken) => tracing::info!(taken, "observations older than the horizon pruned"),
        Err(error) => tracing::error!(%error, "the observation sweep failed"),
    }
}

/// Ask whether a backup is due, every [`TICK`], until cancelled.
async fn tick_loop(state: Arc<BackupState>) {
    tokio::select! {
        () = state.cancel.cancelled() => return,
        () = tokio::time::sleep(STARTUP_DELAY) => {}
    }
    loop {
        tick(&state).await;
        tokio::select! {
            () = state.cancel.cancelled() => return,
            () = tokio::time::sleep(TICK) => {}
        }
    }
}
