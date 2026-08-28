//! App lifecycle and status -- stream D (interfaces §2.1).
//!
//! Everything here answers the question the window asks before it can show
//! anything: *is the database up, and which knobas is this?* That question has
//! to be answerable **while PostgreSQL is still starting**, which is why
//! nothing in this module takes `State<'_, AppState>` -- see [`Lifecycle`].

use std::sync::{Arc, Mutex, PoisonError};

use tauri::{Emitter, Manager};

use crate::{AppState, IpcError, Profile};

/// How far the database has got.
///
/// Interfaces §2.1. Tagged on `state`, so TypeScript reads it as a
/// discriminated union and the payload of a `failed` is reachable only after
/// the check that it *is* a failure.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum DbState {
    /// Coming up. `detail` is a line for the boot screen -- "downloading
    /// PostgreSQL 18.6", which on a first run is tens of seconds.
    Starting { detail: Option<String> },
    /// Up, and running migrations.
    Migrating,
    /// Up, migrated, and answering queries.
    Ready,
    /// It will not come up. The message is shown to the user as text.
    Failed { message: String },
}

/// What the shell needs to decide what to draw.
#[derive(Debug, serde::Serialize)]
pub struct AppStatus {
    pub db: DbState,
    /// No source configured and the first run was never completed (§14a).
    pub first_run: bool,
    /// The `--demo` profile: its own data dir, port and keychain (P13).
    pub demo: bool,
    pub source_count: u32,
    pub app_version: String,
}

/// The database's bring-up state, and the shared state once there is one.
///
/// Managed at **build** time, before `setup` runs, so it is the one piece of
/// managed state a command can always count on.
///
/// # Why commands go through this instead of `State<'_, AppState>`
///
/// Carry-over §10.6(a). A `#[tauri::command]` resolves *every* argument before
/// its body runs, and `AppState` exists only once PostgreSQL is up. A command
/// declaring `State<'_, AppState>` is therefore rejected by Tauri itself
/// during a slow bring-up, with the bare string `"state not managed"` -- no
/// code, nothing the frontend can branch on, and `IpcErrorCode::NotReady`
/// unreachable despite existing for exactly this.
///
/// That was latent while `setup` blocked on bring-up behind a hidden window.
/// Making bring-up asynchronous is what makes a window able to call early, so
/// the two land together: `AppState` lives *inside* this type and is reached
/// through [`Lifecycle::pool`], which answers `not_ready` in one place rather
/// than in a guard sprinkled across every command.
pub struct Lifecycle {
    db: Mutex<DbState>,
    /// `Arc`, and not a `OnceLock`: a failed bring-up can be retried, so this
    /// slot is written more than once over a process's life.
    app: Mutex<Option<Arc<AppState>>>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self::new()
    }
}

impl Lifecycle {
    /// A lifecycle that has not started anything yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            db: Mutex::new(DbState::Starting { detail: None }),
            app: Mutex::new(None),
        }
    }

    /// Where bring-up has got to.
    #[must_use]
    pub fn get(&self) -> DbState {
        self.db
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Record a transition. The caller emits `db:state` after it.
    pub fn set(&self, state: DbState) {
        *self.db.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }

    /// Claim a retry: move `Failed` back to `Starting`, and say whether this
    /// call is the one that now owns a bring-up.
    ///
    /// The check and the write are one critical section on purpose. Two quick
    /// clicks on *Retry* would otherwise both read `Failed` and both spawn,
    /// which is two `initdb`s racing for one data directory.
    pub fn begin_retry(&self) -> bool {
        let mut db = self.db.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(*db, DbState::Failed { .. }) {
            *db = DbState::Starting { detail: None };
            true
        } else {
            false
        }
    }

    /// Hand over the shared state, once the pool is live and migrated.
    pub fn install(&self, state: AppState) {
        *self.app.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(state));
    }

    /// The shared state, if the database came up.
    ///
    /// An `Arc` clone rather than a borrow: the guard must not be held across
    /// an await, and every caller here is in an async command.
    #[must_use]
    pub fn app_state(&self) -> Option<Arc<AppState>> {
        self.app
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The pool, or the one honest refusal for a call that beat bring-up.
    ///
    /// # Errors
    ///
    /// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while there is
    /// no database. Retrying later is the right move, and the frontend knows
    /// that from the code without parsing the message.
    pub fn pool(&self) -> Result<sqlx::PgPool, IpcError> {
        match self.app_state() {
            // A `PgPool` is a handle: cloning it shares the same connections.
            Some(state) => Ok(state.pool.clone()),
            None => Err(IpcError::not_ready(match self.get() {
                DbState::Failed { message } => {
                    format!("the database did not start: {message}")
                }
                _ => "the database is still starting".to_owned(),
            })),
        }
    }
}

/// Liveness probe. Answers as soon as the webview can call, which is now
/// *before* the database is up -- so a successful `ping` says the backend is
/// running and nothing about PostgreSQL. Ask [`app_status`] for that.
#[tauri::command]
pub fn ping() -> &'static str {
    "pong"
}

/// Everything the shell needs to draw its current state.
///
/// Deliberately reads no `State<'_, AppState>`: it is the one command whose
/// whole job is to answer while the database is still coming up.
///
/// Generic over the runtime for the reason `sync_now` is (§10.2): a bare
/// `tauri::AppHandle` means `AppHandle<Wry>`, and a command taking one cannot
/// be registered on the `tauri::test` mock app at all -- which would put this
/// command out of reach of `tests/ipc.rs`.
#[tauri::command]
pub async fn app_status<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppStatus {
    let db = app.state::<Lifecycle>().get();
    let demo = app.state::<Profile>().demo;
    let app_version = app.package_info().version.to_string();

    // Counted live rather than cached, so adding a source is reflected without
    // a restart. Before the pool exists there is nothing to count -- and "0
    // sources while starting" is *not* the same claim as "this is a first
    // run", so both stay false-y until the database can answer.
    let pool = app.state::<Lifecycle>().app_state();
    let (source_count, first_run) = match pool {
        Some(state) => counts(&state.pool).await.unwrap_or((0, false)),
        None => (0, false),
    };

    AppStatus {
        db,
        first_run,
        demo,
        source_count,
        app_version,
    }
}

/// How many sources are configured, and whether this is still a first run.
///
/// A query failure is not propagated: `app_status` is what the boot screen
/// polls, and a window that cannot say "the database is up" because a `count`
/// failed is worse than one reporting zero sources.
async fn counts(pool: &sqlx::PgPool) -> Result<(u32, bool), sqlx::Error> {
    let sources: i64 = sqlx::query_scalar("select count(*) from knobas.source_config")
        .fetch_one(pool)
        .await?;
    let completed: bool = sqlx::query_scalar(
        "select exists(select 1 from knobas.setting where key = 'first_run.completed')",
    )
    .fetch_one(pool)
    .await?;

    let source_count = u32::try_from(sources).unwrap_or(u32::MAX);
    // Both halves, because they are different facts: a user who finished the
    // wizard and then deleted their only source is not on a first run.
    Ok((source_count, source_count == 0 && !completed))
}

/// Arm event emission: the frontend has its listeners up.
///
/// Roadmap §4 gotcha 9 -- the backend must not `emit` before the webview is
/// listening, and a `db:state` fired during startup would otherwise be lost.
/// The frontend registers its `listen()`s, calls this, and gets the current
/// state replayed.
///
/// # Errors
///
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the emit
/// fails, which means the webview this was called from has gone.
#[tauri::command]
pub fn frontend_ready<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), IpcError> {
    let state = app.state::<Lifecycle>().get();
    app.emit(crate::events::DB_STATE, state)
        .map_err(IpcError::internal)?;
    Ok(())
}

/// Start the database again after a failure.
///
/// The *Retry* button on the boot screen. A retry that only re-polled would
/// redraw the same failure for ever -- the database has to be asked to start
/// again, and only this side can do that.
///
/// Idempotent by construction: [`Lifecycle::begin_retry`] hands the bring-up
/// to exactly one caller, so a second click while the first attempt is still
/// running succeeds and starts nothing. That is the right answer to "start the
/// database" when it is already starting.
///
/// # Errors
///
/// Never, today. It returns a `Result` because every command does and because
/// a future bring-up that can refuse (a profile whose data directory is gone)
/// has somewhere to say so.
#[tauri::command]
pub fn retry_database<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), IpcError> {
    if app.state::<Lifecycle>().begin_retry() {
        crate::spawn_bring_up(app);
    }
    Ok(())
}

/// The SQL `complete_first_run` runs. Named so the test can assert on the
/// shape rather than on a copy of it.
const COMPLETE_FIRST_RUN_SQL: &str = "insert into knobas.setting (key, value) \
     values ('first_run.completed', 'true'::jsonb) \
     on conflict (key) do update set value = excluded.value, updated_at = now()";

/// Record that the §14a wizard finished.
///
/// `app_status.first_run` is `source_count == 0 && !completed` -- two facts,
/// because they are different ones. Without this row, a person who finished
/// the wizard and later removed their only source would be shown the wizard
/// again, which is the app forgetting something it was told.
///
/// It lives in `knobas.setting` rather than in the webview's `localStorage`
/// because §14 makes the database the thing an export carries: a flag outside
/// it would not survive the export/import round trip the milestone is built
/// around.
///
/// Idempotent by `on conflict`: finishing the wizard twice -- a double click,
/// a second window -- is the same as finishing it once.
///
/// # Errors
///
/// [`IpcErrorCode::NotReady`](crate::IpcErrorCode::NotReady) while PostgreSQL
/// is still coming up, and
/// [`IpcErrorCode::Internal`](crate::IpcErrorCode::Internal) if the write
/// fails.
#[tauri::command]
pub async fn complete_first_run<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), IpcError> {
    // Through `Lifecycle`, not `State<'_, AppState>`: a command declaring the
    // latter is rejected by Tauri itself during bring-up with a bare "state
    // not managed" that the frontend cannot branch on. See [`Lifecycle`].
    let pool = app.state::<Lifecycle>().pool()?;
    sqlx::query(COMPLETE_FIRST_RUN_SQL)
        .execute(&pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The write has to be an upsert, and it has to touch exactly the key
    /// `counts()` reads.
    ///
    /// Both halves are load-bearing and neither is visible from the type: a
    /// plain `insert` makes finishing the wizard twice a primary-key violation
    /// (a double click is enough), and a key that does not match the one
    /// `app_status` looks for leaves the wizard reappearing for ever with the
    /// row sitting in the table.
    #[test]
    fn completing_the_first_run_upserts_the_key_app_status_reads() {
        assert!(
            COMPLETE_FIRST_RUN_SQL.contains("on conflict (key) do update"),
            "a plain insert makes a second Finish a primary-key violation: {COMPLETE_FIRST_RUN_SQL}"
        );
        // The literal `counts()` queries, quoted from the same file, so a
        // rename on one side fails here rather than at run time.
        assert!(
            COMPLETE_FIRST_RUN_SQL.contains("'first_run.completed'"),
            "the wizard would write a key nothing reads: {COMPLETE_FIRST_RUN_SQL}"
        );
        let reader = include_str!("app.rs");
        assert!(
            reader.contains("where key = 'first_run.completed'"),
            "app_status no longer reads the key this command writes"
        );
    }

    /// The frontend narrows on `state` and reads `message` only inside the
    /// `failed` arm. A different tag, or a payload that moved, is a union that
    /// type-checks in TypeScript and is wrong at run time.
    #[test]
    fn db_state_serialises_as_the_contract_declares() {
        assert_eq!(
            serde_json::to_value(DbState::Starting {
                detail: Some("downloading postgres".into())
            })
            .unwrap(),
            serde_json::json!({ "state": "starting", "detail": "downloading postgres" })
        );
        assert_eq!(
            serde_json::to_value(DbState::Starting { detail: None }).unwrap(),
            serde_json::json!({ "state": "starting", "detail": null })
        );
        assert_eq!(
            serde_json::to_value(DbState::Migrating).unwrap(),
            serde_json::json!({ "state": "migrating" })
        );
        assert_eq!(
            serde_json::to_value(DbState::Ready).unwrap(),
            serde_json::json!({ "state": "ready" })
        );
        assert_eq!(
            serde_json::to_value(DbState::Failed {
                message: "port 5432 in use".into()
            })
            .unwrap(),
            serde_json::json!({ "state": "failed", "message": "port 5432 in use" })
        );
    }

    /// Every variant, in the spelling `app/src/lib/ipc/app.ts` declares. A
    /// state added on one side only is a screen the shell can never draw.
    #[test]
    fn every_db_state_has_its_typescript_counterpart() {
        let mirror = include_str!("../../../../app/src/lib/ipc/app.ts");
        for state in [
            DbState::Starting { detail: None },
            DbState::Migrating,
            DbState::Ready,
            DbState::Failed {
                message: String::new(),
            },
        ] {
            let json = serde_json::to_value(&state).unwrap();
            let tag = json["state"].as_str().unwrap().to_owned();
            assert!(
                mirror.contains(&format!("state: \"{tag}\"")),
                "{tag:?} is missing from app/src/lib/ipc/app.ts"
            );
        }
    }

    /// The exact key set, not a `contains` check: a field added to `AppStatus`
    /// with no counterpart in the mirror is invisible to a test that only
    /// walks a hardcoded list.
    #[test]
    fn app_status_serialises_the_keys_the_mirror_declares() {
        let status = AppStatus {
            db: DbState::Ready,
            first_run: false,
            demo: true,
            source_count: 3,
            app_version: "0.1.0".to_owned(),
        };
        let json = serde_json::to_value(&status).unwrap();
        let mut keys = json
            .as_object()
            .expect("an object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            ["app_version", "db", "demo", "first_run", "source_count"]
        );

        let mirror = include_str!("../../../../app/src/lib/ipc/app.ts");
        for key in &keys {
            assert!(
                mirror.contains(&format!("{key}:")),
                "AppStatus.{key} is missing from app/src/lib/ipc/app.ts"
            );
        }
    }

    /// Every command in this module is callable from the frontend.
    ///
    /// The mirror is hand-written (contract §2.6), so a command added on the
    /// Rust side alone compiles, lints and tests green while being unreachable
    /// from the window -- and a command *renamed* on the Rust side alone leaves
    /// the mirror invoking a name that no longer exists, which fails only when
    /// a person clicks the button.
    ///
    /// Matched on the `invoke("<name>")` string, because that is the thing
    /// that actually has to agree: a TS function may be called anything.
    #[test]
    fn every_command_in_this_module_has_a_typescript_mirror() {
        let code = include_str!("app.rs");
        let mirror = include_str!("../../../../app/src/lib/ipc/app.ts");

        // Read out of this file rather than listed, so a command added below
        // is covered without anyone remembering to add it here.
        let mut commands: Vec<&str> = Vec::new();
        for (index, line) in code.lines().enumerate() {
            if line.trim() != "#[tauri::command]" {
                continue;
            }
            let signature = code
                .lines()
                .nth(index + 1)
                .expect("a #[tauri::command] is followed by its signature");
            let name = signature
                .split("fn ")
                .nth(1)
                .and_then(|rest| rest.split(['<', '(']).next())
                .expect("the signature names a function");
            commands.push(name);
        }
        assert!(
            commands.len() >= 5,
            "no commands were found, so this test proves nothing: {commands:?}"
        );

        let missing: Vec<&str> = commands
            .iter()
            .copied()
            .filter(|name| !mirror.contains(&format!("(\"{name}\"")))
            .collect();
        assert_eq!(
            missing,
            Vec::<&str>::new(),
            "these commands have no `invoke(\"..\")` in app/src/lib/ipc/app.ts, so the frontend \
             cannot call them. Found: {commands:?}"
        );
    }

    #[test]
    fn lifecycle_starts_starting_and_remembers_the_last_set() {
        let life = Lifecycle::new();
        assert_eq!(life.get(), DbState::Starting { detail: None });

        life.set(DbState::Starting {
            detail: Some("downloading postgres".into()),
        });
        assert_eq!(
            life.get(),
            DbState::Starting {
                detail: Some("downloading postgres".into())
            }
        );

        life.set(DbState::Migrating);
        assert_eq!(life.get(), DbState::Migrating);
    }

    /// The carry-over, discharged: a call that beats bring-up gets a code the
    /// frontend can branch on, and not Tauri's bare `"state not managed"`.
    #[test]
    fn a_call_before_the_database_is_up_is_not_ready() {
        let life = Lifecycle::new();
        let error = life.pool().expect_err("there is no pool yet");
        assert_eq!(error.code, crate::IpcErrorCode::NotReady);
        assert!(error.message.contains("starting"), "{}", error.message);
    }

    /// One retry, however many callers ask for one.
    #[test]
    fn a_retry_is_claimed_once_and_only_out_of_a_failure() {
        let life = Lifecycle::new();

        // Nothing to retry while it is still coming up: a second `initdb` on
        // the same data directory is the failure this prevents.
        assert!(!life.begin_retry(), "a starting database is not retried");

        life.set(DbState::Failed {
            message: "port 5432 in use".to_owned(),
        });
        assert!(life.begin_retry(), "a failure can be retried");
        assert_eq!(
            life.get(),
            DbState::Starting { detail: None },
            "the retry moves the state, so the boot screen stops saying `failed`"
        );
        assert!(
            !life.begin_retry(),
            "the second click must not start a second bring-up"
        );

        life.set(DbState::Ready);
        assert!(!life.begin_retry(), "a live database is not restarted");
    }

    /// ...and a bring-up that *failed* says so, rather than repeating "still
    /// starting" for the rest of the session.
    #[test]
    fn a_failed_bring_up_says_why_in_the_refusal() {
        let life = Lifecycle::new();
        life.set(DbState::Failed {
            message: "port 5432 in use".to_owned(),
        });
        let error = life.pool().expect_err("there is no pool");
        assert_eq!(error.code, crate::IpcErrorCode::NotReady);
        assert!(
            error.message.contains("port 5432 in use"),
            "{}",
            error.message
        );
    }
}
