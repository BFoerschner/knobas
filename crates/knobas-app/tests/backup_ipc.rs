//! The backup settings surface, at the seam the commands are shims over, and
//! at the seam Tauri dispatches through.
//!
//! Two halves, the same split `tests/search_ipc.rs` and `tests/ipc.rs` make:
//!
//! * the *behaviour* -- schedule, export, retention, restore -- driven through
//!   `knobas_app::backup`, which is what each `#[tauri::command]` calls;
//! * the *wiring* -- every command registered under the name the TypeScript
//!   mirror invokes, and its argument shape decoding -- driven through the
//!   `tauri::test` mock runtime.
//!
//! Each behavioural test gets a database of its own
//! (`knobas_db::test_util::scratch_database`) on the binary's shared server,
//! because the settings this feature reads and writes are one row per key: two
//! tests sharing `backup.schedule` would be each other's fixture. The server
//! is still shared, so this costs a `create database` and not a postmaster.

use std::sync::Arc;

use knobas_app::backup::{self, BackupSchedule, BackupState};
use tauri::ipc::CallbackFn;
use tauri::test::MockRuntime;

#[cfg(windows)]
const LOCAL_ORIGIN: &str = "http://tauri.localhost";
#[cfg(not(windows))]
const LOCAL_ORIGIN: &str = "tauri://localhost";

/// A backup service over a database and a directory of its own.
///
/// The `TempDir` is returned with it: dropping it deletes the archives, and a
/// test that let it drop early would be writing into a path that no longer
/// exists.
async fn service(label: &str) -> (Arc<BackupState>, tempfile::TempDir) {
    let connector = knobas_db::test_util::scratch_database(label).await;
    let pool = connector
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    let dir = tempfile::tempdir().expect("a scratch backup directory");
    (
        Arc::new(BackupState::new(pool, connector, dir.path().to_path_buf())),
        dir,
    )
}

/// The ratified default is what a knobas that has never been configured runs
/// on: nightly, and enabled without anyone turning it on.
#[tokio::test]
async fn a_database_with_no_stored_schedule_is_nightly_by_default() {
    let (service, _dir) = service("defaults").await;

    let status = backup::status(&service).await.expect("status");
    assert!(
        status.schedule.enabled,
        "the ratified default is a scheduled nightly export"
    );
    assert_eq!(status.schedule, BackupSchedule::default());
    assert_eq!(status.last, None, "nothing has been exported yet");
    assert!(status.archives.is_empty());
    assert!(
        status.next_due_at.is_some(),
        "an enabled schedule has a next run"
    );
    assert_eq!(status.directory, service.directory.display().to_string());
}

/// *Schedule*: what the dialog stores is what the next read answers with.
#[tokio::test]
async fn a_stored_schedule_survives_and_is_clamped() {
    let (service, _dir) = service("schedule").await;

    backup::save_schedule(
        &service.pool,
        BackupSchedule {
            enabled: false,
            hour: 22,
            minute: 30,
            keep: 3,
        },
    )
    .await
    .expect("store a schedule");

    let status = backup::status(&service).await.expect("status");
    assert_eq!(
        status.schedule,
        BackupSchedule {
            enabled: false,
            hour: 22,
            minute: 30,
            keep: 3
        }
    );
    assert_eq!(
        status.next_due_at, None,
        "a schedule that is off has no next run to show"
    );

    // A value out of range is brought back in rather than refused: a scheduler
    // that stops because a stored row is odd is the failure this feature is
    // for.
    backup::save_schedule(
        &service.pool,
        BackupSchedule {
            enabled: true,
            hour: 99,
            minute: 0,
            keep: 0,
        },
    )
    .await
    .expect("store an impossible schedule");
    let status = backup::status(&service).await.expect("status");
    assert_eq!(status.schedule.hour, 23);
    assert_eq!(status.schedule.keep, 1);
}

/// *Export now*: an archive appears on disk, and the status says so.
#[tokio::test]
async fn export_now_writes_an_archive_and_records_it() {
    let (service, dir) = service("exportnow").await;

    let record = backup::export_now(&service).await.expect("export");
    assert!(record.bytes > 0, "the archive is empty");
    assert!(
        dir.path().join(&record.file).is_file(),
        "{} is not on disk",
        record.file
    );
    assert!(
        record.file.starts_with("knobas-") && record.file.ends_with(".knobas"),
        "spec §14 names the archive `*.knobas`: {}",
        record.file
    );

    let status = backup::status(&service).await.expect("status");
    assert_eq!(status.last.as_ref(), Some(&record));
    assert_eq!(
        status
            .archives
            .iter()
            .map(|archive| archive.file.as_str())
            .collect::<Vec<_>>(),
        vec![record.file.as_str()]
    );
}

/// Exit criterion 4, at the seam that produces it: the nightly job takes an
/// archive when one is due and does nothing when one is not.
///
/// The second call is the load-bearing half. Without it, "runs nightly" and
/// "runs every minute" are the same test -- and a tick loop that exported on
/// every pass would fill the disk while looking like it worked.
#[tokio::test]
async fn the_nightly_job_exports_once_and_then_leaves_it_alone() {
    let (service, _dir) = service("nightly").await;

    let first = backup::export_if_due(&service)
        .await
        .expect("the first run")
        .expect("a database that has never been backed up is due");

    let second = backup::export_if_due(&service)
        .await
        .expect("the second run");
    assert_eq!(
        second, None,
        "a backup taken moments ago is not due again tonight"
    );

    let status = backup::status(&service).await.expect("status");
    assert_eq!(status.last.as_ref(), Some(&first));
    assert_eq!(status.archives.len(), 1, "{:?}", status.archives);
}

/// ...and nothing at all happens while the schedule is off.
#[tokio::test]
async fn a_disabled_schedule_exports_nothing() {
    let (service, _dir) = service("disabled").await;
    backup::save_schedule(
        &service.pool,
        BackupSchedule {
            enabled: false,
            ..BackupSchedule::default()
        },
    )
    .await
    .expect("store a schedule");

    assert_eq!(
        backup::export_if_due(&service).await.expect("no run"),
        None,
        "a disabled schedule must not export"
    );
    assert!(backup::status(&service).await.unwrap().archives.is_empty());

    // *Export now* still works: turning the schedule off is not turning the
    // feature off.
    backup::export_now(&service)
        .await
        .expect("an explicit export");
    assert_eq!(backup::status(&service).await.unwrap().archives.len(), 1);
}

/// Retention: `keep` archives survive an export, older ones are deleted, and
/// a file that is not ours is never touched.
#[tokio::test]
async fn retention_deletes_the_oldest_and_spares_what_is_not_an_archive() {
    let (service, dir) = service("retention").await;
    backup::save_schedule(
        &service.pool,
        BackupSchedule {
            keep: 2,
            ..BackupSchedule::default()
        },
    )
    .await
    .expect("store a schedule");

    // Three archives older than anything this test writes, plus a file that
    // is emphatically not one of ours.
    for day in ["20260101", "20260102", "20260103"] {
        std::fs::write(dir.path().join(format!("knobas-{day}-030000.knobas")), b"x").unwrap();
    }
    std::fs::write(dir.path().join("do-not-delete.txt"), b"mine").unwrap();

    let record = backup::export_now(&service).await.expect("export");

    let mut left: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        vec![
            "do-not-delete.txt".to_owned(),
            "knobas-20260103-030000.knobas".to_owned(),
            record.file.clone(),
        ],
        "keep=2 means the new archive and the newest old one"
    );
}

/// A *directory* wearing an archive's name is not an archive.
///
/// `archives` reads the backup directory with `read_dir`, which yields
/// directories too, and retention then hands what it finds to `remove_file`.
/// One directory called `knobas-....knobas` -- a half-unpacked something, a
/// user's own tidying -- would therefore fail every export from then on, long
/// after whoever made it had forgotten it. It is not ours; it is not touched;
/// and the export it does not belong to still succeeds.
#[tokio::test]
async fn a_directory_named_like_an_archive_is_neither_listed_nor_pruned() {
    let (service, dir) = service("archivedir").await;
    backup::save_schedule(
        &service.pool,
        BackupSchedule {
            keep: 1,
            ..BackupSchedule::default()
        },
    )
    .await
    .expect("store a schedule");

    let impostor = dir.path().join("knobas-20200101-030000.knobas");
    std::fs::create_dir(&impostor).unwrap();

    let record = backup::export_now(&service)
        .await
        .expect("a directory in the way must not fail the export");

    assert!(impostor.is_dir(), "the directory was not left alone");
    assert_eq!(
        backup::status(&service)
            .await
            .expect("status")
            .archives
            .iter()
            .map(|archive| archive.file.as_str())
            .collect::<Vec<_>>(),
        vec![record.file.as_str()],
        "a directory must not be listed as an archive"
    );
}

/// **One tick sweeps the observations retention has aged out** (#315), and it
/// does so with the nightly export switched off.
///
/// The wire, not the rule: `time::passive::prune` has its own tests against a
/// horizon of their choosing, and what nothing else can witness is that
/// anything in a running knobas ever calls it. The schedule is off on purpose
/// -- a sweep that only happened on the nights a backup was written would
/// leave the table of a person who turned backups off growing for ever, and a
/// test that let the export run could not tell the two arrangements apart.
#[tokio::test]
async fn a_tick_sweeps_the_observations_retention_has_aged_out() {
    let (service, _dir) = service("sweep").await;
    backup::save_schedule(
        &service.pool,
        BackupSchedule {
            enabled: false,
            ..BackupSchedule::default()
        },
    )
    .await
    .expect("a schedule that never comes due");

    let now = chrono::Utc::now();
    let retention = chrono::Duration::days(knobas_app::time::passive::RETENTION_DAYS);
    for at in [
        now - retention - chrono::Duration::days(1),
        now - chrono::Duration::days(1),
    ] {
        knobas_app::time::passive::record_at(
            &service.pool,
            at,
            Some(&knobas_app::time::TimerTarget::Entity {
                entity_id: "jira:PAY-231".to_owned(),
            }),
        )
        .await
        .expect("an observation in the past");
    }

    backup::tick(&service).await;

    let kept: Vec<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("select at from knobas.heartbeat order by at")
            .fetch_all(&service.pool)
            .await
            .expect("the observations are readable");
    assert_eq!(
        kept.len(),
        1,
        "the tick left the table exactly as it found it: nothing calls the sweep"
    );
    assert!(
        kept[0] > now - retention,
        "the tick swept the wrong side of the horizon: {:?}",
        kept[0]
    );
    assert_eq!(
        backup::status(&service).await.expect("status").last,
        None,
        "the sweep must not have depended on an export being due"
    );
}

/// *Restore*, minimal: a name that is not an archive in this profile's
/// directory is refused before anything touches the database.
///
/// The command takes a **file name**, not a path, and this is what makes that
/// meaningful -- a webview naming `../../etc/passwd`, or any archive knobas
/// did not write, gets `not_found` rather than a `pg_restore` invocation.
#[tokio::test]
async fn a_restore_only_accepts_an_archive_in_the_backup_directory() {
    let (service, dir) = service("restorepick").await;
    std::fs::write(dir.path().join("someone-elses.dump"), b"x").unwrap();

    for name in [
        "someone-elses.dump",
        "knobas-20260101-030000.knobas", // an archive name, but no such file
        "../outside.knobas",
        // Prefixed *and* suffixed like ours, and still not a file name: this
        // is the one a `starts_with`/`ends_with` pair waves through, and
        // `directory.join` then resolves outside the directory entirely.
        "knobas-x/../../outside.knobas",
    ] {
        let error = backup::restore(&service, name)
            .await
            .err()
            .unwrap_or_else(|| panic!("{name} must be refused"));
        assert!(
            matches!(error, backup::ExportError::NoSuchArchive(_)),
            "{name}: {error:?}"
        );
        assert_eq!(
            knobas_app::IpcError::from(error).code,
            knobas_app::IpcErrorCode::NotFound
        );
    }
}

/// ...and restoring over a database that still holds knobas *content* is a
/// `conflict`, not an overwrite. Merge-restore is M4.
///
/// Content, and deliberately not `knobas.setting`: that table is knobas's own
/// bookkeeping and knobas fills it in by itself, so a rule that counted it
/// would refuse every machine rather than every populated one -- which is what
/// `a_fresh_machine_restores_after_taking_its_own_first_backup` is about.
#[tokio::test]
async fn a_restore_over_a_populated_database_is_a_conflict() {
    let (service, _dir) = service("restoreconflict").await;
    let record = backup::export_now(&service).await.expect("export");
    seed_entity(&service.pool, "conflict").await;

    let error = backup::restore(&service, &record.file)
        .await
        .expect_err("a populated knobas must not be overwritten");
    assert_eq!(
        knobas_app::IpcError::from(error).code,
        knobas_app::IpcErrorCode::Conflict,
        "the frontend branches on this code to offer the M4 merge later"
    );
}

/// The fresh-machine path spec §14 calls **primary**, at the moment a user
/// actually reaches it.
///
/// Nobody restores into a knobas that has never run: to have an archive to
/// pick you must have installed knobas, started it, and gone looking -- and by
/// then the nightly task has fired once (`STARTUP_DELAY` is 30 seconds) and
/// written `backup.last` into `knobas.setting`. A restore that treats *any*
/// row in the `knobas` schema as "populated" is therefore refused on every
/// machine it exists to serve, which is not a conservative guard but a dead
/// command.
///
/// So: an old machine's corpus, archived; a new machine that has already taken
/// its own first backup; and the old archive restored onto it.
#[tokio::test]
async fn a_fresh_machine_restores_after_taking_its_own_first_backup() {
    // The old machine: a corpus, and an archive of it.
    let (old, old_dir) = service("oldmachine").await;
    let entity = seed_entity(&old.pool, "oldmachine").await;
    let archive = backup::export_now(&old)
        .await
        .expect("the old machine's backup");

    // The new machine: empty, but its nightly job has already run once, so
    // `knobas.setting` holds `backup.last`.
    let (fresh, fresh_dir) = service("freshmachine").await;
    backup::export_if_due(&fresh)
        .await
        .expect("the first nightly run")
        .expect("a machine that has never backed up is due");
    let settings: i64 = sqlx::query_scalar("select count(*) from knobas.setting")
        .fetch_one(&fresh.pool)
        .await
        .unwrap();
    assert!(
        settings > 0,
        "this test is about a database knobas has written its own bookkeeping into"
    );

    // The user drops the old machine's archive into the new profile.
    std::fs::copy(
        old_dir.path().join(&archive.file),
        fresh_dir.path().join(&archive.file),
    )
    .unwrap();

    backup::restore(&fresh, &archive.file)
        .await
        .expect("the fresh-machine restore §14 calls the primary path");

    let restored: Option<String> = sqlx::query_scalar("select id from knobas.entity where id = $1")
        .bind(&entity)
        .fetch_optional(&fresh.pool)
        .await
        .unwrap();
    assert_eq!(
        restored.as_deref(),
        Some(entity.as_str()),
        "the old machine's corpus did not come across"
    );
}

/// One entity, run-unique, so a database can be *populated* in the sense the
/// occupancy guard means. Returns its id.
async fn seed_entity(pool: &sqlx::PgPool, tag: &str) -> String {
    let id = format!(
        "mock:{tag}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    );
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'seeded')")
        .bind(&id)
        .execute(pool)
        .await
        .expect("seed an entity");
    id
}

// ---------------------------------------------------------------------------
// The wiring: registered, named, and decoding.
// ---------------------------------------------------------------------------

/// Invoke `cmd` on a mock app that manages nothing.
///
/// Every backup command takes an `AppHandle` and asks it for the state, so a
/// call with nothing managed reaches the *body* and answers `not_ready`. That
/// is the marker for "registered and dispatched", as distinct from "no such
/// command" -- and it is only reachable because none of them declares the
/// state as an argument (carry-over §10.6(a)).
fn invoke(cmd: &str, body: serde_json::Value) -> Result<String, String> {
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            knobas_app::commands::backup::backup_status,
            knobas_app::commands::backup::backup_now,
            knobas_app::commands::backup::set_backup_schedule,
            knobas_app::commands::backup::restore_backup,
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .expect("mock app");
    let webview: tauri::WebviewWindow<MockRuntime> =
        tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
            .build()
            .expect("mock webview");

    let outcome = tauri::test::get_ipc_response(
        &webview,
        tauri::webview::InvokeRequest {
            cmd: cmd.to_owned(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: LOCAL_ORIGIN.parse().expect("url"),
            body: body.into(),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        },
    )
    .map_err(|error| format!("{error:?}"));

    if let Err(rejection) = &outcome {
        assert!(
            !rejection.contains("not allowed"),
            "the permission check refused {cmd} before its arguments were decoded, \
             so this proves nothing about decoding: {rejection}"
        );
    }
    outcome.map(|_| String::new())
}

/// Every backup command is registered under the name the mirror invokes, and
/// its argument list decodes.
///
/// The mistake this catches is the one an append-only handler list invites:
/// adding a command and forgetting the list, which is a frontend failing at
/// run time with "command not found" against a Rust side that compiles.
#[test]
fn every_backup_command_is_registered_and_its_arguments_decode() {
    let schedule = serde_json::json!({
        "enabled": true, "hour": 3, "minute": 0, "keep": 7
    });
    for (cmd, args) in [
        ("backup_status", serde_json::json!({})),
        ("backup_now", serde_json::json!({})),
        (
            "set_backup_schedule",
            serde_json::json!({ "schedule": schedule }),
        ),
        (
            "restore_backup",
            serde_json::json!({ "file": "knobas-20260828-030000.knobas" }),
        ),
    ] {
        let rejection = invoke(cmd, args).expect_err("nothing is managed, so nothing is ready");
        assert!(
            rejection.contains("not_ready"),
            "{cmd} did not reach its own body -- it is missing from the handler \
             list, or an argument failed to decode: {rejection}"
        );
    }
}

/// An argument the mirror spells differently is an argument that never
/// arrives.
///
/// Tauri renames command arguments to camelCase; both of these are single
/// words, so the check that matters is that the *key* is the one the command
/// declares. A wrong key fails argument resolution -- which is a different
/// rejection from `not_ready`, and that difference is the assertion.
#[test]
fn a_misspelled_argument_is_refused_before_the_body_runs() {
    let wrong = invoke(
        "restore_backup",
        serde_json::json!({ "archive": "knobas-20260828-030000.knobas" }),
    )
    .expect_err("`archive` is not the argument's name");
    assert!(
        wrong.contains("file"),
        "the refusal must name the missing argument: {wrong}"
    );
    assert!(
        !wrong.contains("not_ready"),
        "an argument that did not decode must not look like a command that ran: {wrong}"
    );
}
