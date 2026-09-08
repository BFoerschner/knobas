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
    service_with(label, Arc::new(knobas_secrets::MemoryStore::new())).await
}

/// The same, over a keychain the caller has stocked.
///
/// A restore asks the store whether this machine holds a credential for each
/// source the archive brought (`backup::restore`), so the store is the fixture
/// for both directions of that question -- an empty one is the fresh machine,
/// and one holding the secret is the person restoring their own backup.
async fn service_with(
    label: &str,
    secrets: Arc<dyn knobas_secrets::SecretStore>,
) -> (Arc<BackupState>, tempfile::TempDir) {
    let connector = knobas_db::test_util::scratch_database(label).await;
    let pool = connector
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    let dir = tempfile::tempdir().expect("a scratch backup directory");
    (
        Arc::new(BackupState::new(
            pool,
            connector,
            dir.path().to_path_buf(),
            secrets,
        )),
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

/// **One tick sweeps the samples retention has aged out** (#443), with the
/// nightly export switched off, exactly as the observation sweep beside it.
///
/// The wire, not the rule: `knobas_sync::samples::prune` has its own tests in
/// its own crate against a horizon of their choosing, and what nothing else
/// can witness is that anything in a running knobas ever calls it. Samples are
/// written by the sync engine at a rate no other table here comes near -- one
/// row per monitor per minute -- so a sweep nothing calls is not a stale row,
/// it is a table that never stops growing.
#[tokio::test]
async fn a_tick_sweeps_the_samples_retention_has_aged_out() {
    let (service, _dir) = service("sample-sweep").await;
    backup::save_schedule(
        &service.pool,
        BackupSchedule {
            enabled: false,
            ..BackupSchedule::default()
        },
    )
    .await
    .expect("a schedule that never comes due");

    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ('kuma:8', 'monitor', 'canary')",
    )
    .execute(&service.pool)
    .await
    .expect("a monitor to have samples of");
    let now = chrono::Utc::now();
    let retention = chrono::Duration::days(knobas_sync::samples::DEFAULT_RETENTION_DAYS);
    for at in [
        now - retention - chrono::Duration::days(1),
        now - chrono::Duration::days(1),
    ] {
        sqlx::query(
            "insert into knobas.monitor_sample (entity_id, taken_at, state, response_time_ms)
             values ('kuma:8', $1, 'up', 35)",
        )
        .bind(at)
        .execute(&service.pool)
        .await
        .expect("a sample in the past");
    }

    backup::tick(&service).await;

    let kept: Vec<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("select taken_at from knobas.monitor_sample order by taken_at")
            .fetch_all(&service.pool)
            .await
            .expect("the samples are readable");
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

// ---------------------------------------------------------------------------
// The share export (#454): an archive restricted to parts, and a restore that
// accepts one.
// ---------------------------------------------------------------------------

/// Everything a share export could carry, seeded into one database.
///
/// Every part's tables and the ones that are in **no** part -- the activity
/// stream and the mirror -- because "and nothing else" is only a claim if the
/// things that must not travel are actually there to be carried.
///
/// Not called `Estate`: `CONTEXT.md` fixes that word for the tree of assets,
/// their routes and the monitors on them, and this is the whole corpus.
struct Seeded {
    ticket: String,
    pr: String,
    asset: String,
    route: String,
    context: String,
    note: String,
    source: String,
    /// The id `create_smart_list` generated for [`SAVED_LABEL`].
    saved_list: String,
}

/// The launcher search the sharer saved, and the name they gave it (#507).
///
/// Saved through the command a reader presses rather than inserted, so the row
/// is one the product can actually make: the id is the generated slug, and the
/// query is text `saved::plan` accepts -- an `insert` could put either beyond
/// what the feature produces and the archive would be carrying a row nothing
/// else in knobas would.
const SAVED_LABEL: &str = "Payout tickets this week";
const SAVED_QUERY: &str = "#payouts updated:7d";

async fn seed_corpus(pool: &sqlx::PgPool, tag: &str) -> Seeded {
    let t = format!("{tag}{}", uuid::Uuid::new_v4().simple());
    let ticket = format!("jira:PAY-{t}");
    let pr = format!("gitea:pr-{t}");
    let asset = format!("asset:{t}");
    let route = format!("route:{t}");
    let context = format!("ctx:{t}");
    let note = format!("note:{t}");
    let source = format!("src-{t}");

    for (id, kind, title) in [
        (&ticket, "ticket", "Retry failed payouts"),
        (&pr, "pr", "Backoff"),
        (&asset, "asset", "db-01"),
        (&route, "route", "postgres"),
        (&note, "note", "Runbook"),
    ] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(id)
            .bind(kind)
            .bind(title)
            .execute(pool)
            .await
            .expect("seed an entity");
    }

    // links
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by)
         values ($1, $2, 'implements', 'manual', 'user')",
    )
    .bind(&ticket)
    .bind(&pr)
    .execute(pool)
    .await
    .expect("seed a link");

    // assets: an asset and the route it exposes
    sqlx::query(
        "insert into knobas.asset (id, type_id, name, environment) values ($1, 'database', 'db-01', 'prod')",
    )
    .bind(&asset)
    .execute(pool)
    .await
    .expect("seed an asset");
    sqlx::query(
        "insert into knobas.route (id, asset_id, name, url)
         values ($1, $2, 'postgres', 'postgres://db-01.internal:5432')",
    )
    .bind(&route)
    .bind(&asset)
    .execute(pool)
    .await
    .expect("seed a route");

    // contexts
    sqlx::query(
        "insert into knobas.context (id, kind, title, anchor_id) values ($1,'ticket','Payouts',$2)",
    )
    .bind(&context)
    .bind(&ticket)
    .execute(pool)
    .await
    .expect("seed a context");

    // notes
    sqlx::query("insert into knobas.note (id, title, body_md) values ($1, 'Runbook', $2)")
        .bind(&note)
        .bind("How to drain the payout queue.")
        .execute(pool)
        .await
        .expect("seed a note");

    // sources
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, auth_state)
         values ($1, 'jira', 'Jira', 'https://jira.example', 'pat', 'ok')",
    )
    .bind(&source)
    .execute(pool)
    .await
    .expect("seed a source configuration");

    // time: a running timer, a block, the worklog it became, an observation
    sqlx::query("insert into knobas.timer (entity_id, label, started_at) values ($1, null, now())")
        .bind(&ticket)
        .execute(pool)
        .await
        .expect("seed a timer");
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, entity_id, kind)
         values (now() - interval '2 hours', now() - interval '1 hour', $1, 'manual')",
    )
    .bind(&ticket)
    .execute(pool)
    .await
    .expect("seed a block");
    sqlx::query(
        "insert into knobas.worklog (entity_id, started_at, seconds, comment, block_ids)
         select $1, now() - interval '2 hours', 3600, 'drained the payout queue', array[b.id]
           from knobas.block b
          where b.entity_id = $1",
    )
    .bind(&ticket)
    .execute(pool)
    .await
    .expect("seed a worklog");
    sqlx::query("insert into knobas.heartbeat (at, entity_id) values (now(), $1)")
        .bind(&ticket)
        .execute(pool)
        .await
        .expect("seed an observation");

    // ...and three tables no part names.
    sqlx::query(
        "insert into knobas.activity (actor, verb, entity_id, detail)
         values ('user', 'linked', $1, '{}'::jsonb)",
    )
    .bind(&ticket)
    .execute(pool)
    .await
    .expect("seed an activity line");
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, 'jira', 'ticket', 'Retry failed payouts', '{}'::jsonb)",
    )
    .bind(&ticket)
    .execute(pool)
    .await
    .expect("seed a mirror row");

    // ...and a launcher search saved as a smart list (#506), which is what the
    // `smart_lists` part of a share export carries (#507).
    let saved_list =
        knobas_app::commands::search::create_smart_list_inner(pool, SAVED_LABEL, SAVED_QUERY)
            .await
            .expect("seed a saved smart list")
            .id;

    Seeded {
        ticket,
        pr,
        asset,
        route,
        context,
        note,
        source,
        saved_list,
    }
}

/// The `id`, `label` and `query` of every saved list a database holds, in the
/// rail's own order.
///
/// Read as three strings rather than through `saved::all`, because what the
/// assertions below are about is the **text** an archive moved: a comparison
/// that went through a type would be comparing what this side made of the
/// bytes, which is the one thing a "nothing rewrites a stored query" claim
/// cannot afford.
async fn saved_lists(pool: &sqlx::PgPool) -> Vec<(String, String, String)> {
    sqlx::query_as("select id, label, query from knobas.smart_list order by created_at, id")
        .fetch_all(pool)
        .await
        .expect("read the saved smart lists")
}

/// How many rows a table holds -- the reading every assertion below is made of.
async fn rows(pool: &sqlx::PgPool, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "select count(*) from knobas.{table}"
    )))
    .fetch_one(pool)
    .await
    .unwrap_or_else(|error| panic!("count knobas.{table}: {error}"))
}

/// Every `knobas` table the archive carries, read off the file.
///
/// **Rows or none**, which is not what this helper's first sentence said: it
/// read *"every table the archive holds rows for"* until #507, and that was
/// wrong from the day it was written. `pg_dump` writes a `TABLE DATA` entry
/// for every table its argument list names and never counts rows first, so an
/// archive of a knobas with nothing saved still names `smart_list` while that
/// part is on -- asserted, not assumed, by
/// [`an_empty_saved_list_table_is_still_in_the_archive_while_the_part_is_on`].
///
/// Which is why "the archive carries no list table" is a claim about the
/// *argument list* and can only be delivered by switching the part off: the
/// share dialog is what does that while no saved list exists (#507), and
/// `BackupSection.test.svelte.ts` is where that half is pinned.
async fn data_tables(archive: &std::path::Path) -> Vec<String> {
    let mut tables: Vec<String> = knobas_db::backup::archive_contents(archive)
        .await
        .expect("read the archive's table of contents")
        .into_iter()
        .filter(|entry| entry.kind == "TABLE DATA" && entry.schema == "knobas")
        .map(|entry| entry.name)
        .collect();
    tables.sort();
    tables
}

/// Move `file` from one profile's backup directory into another's, which is
/// what handing somebody an archive is.
fn hand_over(from: &tempfile::TempDir, to: &tempfile::TempDir, file: &str) {
    std::fs::copy(from.path().join(file), to.path().join(file)).expect("hand the archive over");
}

/// **The whole promise, end to end**: a share export taken with the ratified
/// defaults, restored into an empty knobas, is the link map and the estate and
/// none of the hours.
///
/// The restore is what makes it mean something. A table of contents says a
/// table's rows are in the archive; only a database that has them says they
/// arrived, and only a database that *has been restored into* can say a note
/// did not.
#[tokio::test]
async fn a_share_export_restores_the_link_map_and_leaves_the_hours_behind() {
    let (sharer, sharer_dir) = service("sharesource").await;
    let estate = seed_corpus(&sharer.pool, "share").await;

    let record = backup::share_export(&sharer, backup::ShareParts::default())
        .await
        .expect("a share export with the defaults");
    assert!(
        record.file.starts_with("knobas-share-") && record.file.ends_with(".knobas"),
        "a share export is named apart from a backup: {}",
        record.file
    );
    assert!(record.bytes > 0, "the archive is empty");

    // It is not a backup: nothing about the nightly schedule moved.
    let status = backup::status(&sharer).await.expect("status");
    assert_eq!(
        status.last, None,
        "a share export must not count as the last backup"
    );

    // The colleague's machine: migrated, empty, its own first backup taken.
    let (colleague, colleague_dir) = service("sharetarget").await;
    backup::export_if_due(&colleague)
        .await
        .expect("the colleague's first nightly run")
        .expect("a machine that has never backed up is due");
    hand_over(&sharer_dir, &colleague_dir, &record.file);

    backup::restore(&colleague, &record.file)
        .await
        .expect("a share export restores like any other archive");

    let there = &colleague.pool;
    for (table, expected) in [
        ("link", 1),
        ("asset", 1),
        ("route", 1),
        ("context", 1),
        ("source_config", 1),
        ("smart_list", 1),
    ] {
        assert_eq!(
            rows(there, table).await,
            expected,
            "knobas.{table} did not come across"
        );
    }
    for absent in ["note", "block", "worklog", "timer", "heartbeat", "activity"] {
        assert_eq!(
            rows(there, absent).await,
            0,
            "knobas.{absent} is in a share export taken with the defaults"
        );
    }

    // The addresses came with the things that have them, and the ends of the
    // link resolve on the far machine.
    let addressed: Vec<String> = sqlx::query_scalar("select id from knobas.entity order by id")
        .fetch_all(there)
        .await
        .expect("the entity rows");
    for id in [&estate.ticket, &estate.pr, &estate.asset, &estate.route] {
        assert!(
            addressed.contains(id),
            "{id} has no entity row after the restore"
        );
    }
    let anchored: Option<String> =
        sqlx::query_scalar("select anchor_id from knobas.context where id = $1")
            .bind(&estate.context)
            .fetch_one(there)
            .await
            .expect("the restored context");
    assert_eq!(
        anchored.as_ref(),
        Some(&estate.ticket),
        "the context came across still anchored to the ticket it was about"
    );
    let ends: (String, String) = sqlx::query_as("select from_id, to_id from knobas.link")
        .fetch_one(there)
        .await
        .expect("the restored link");
    assert_eq!(ends, (estate.ticket.clone(), estate.pr.clone()));
    assert_eq!(
        rows(there, "note").await,
        0,
        "the note's body stayed on the machine that wrote it"
    );
    // ...and its *address* did not, which is the recorded consequence of the
    // address book travelling whole: `pg_dump` restricts an archive by table
    // and never by row, so an entity row rides across for every entity there
    // is, note titles included. What a notes-off export withholds is the
    // note's contents, not the fact that a note by that title exists. See
    // `backup::share`'s module docs.
    assert!(
        addressed.contains(&estate.note),
        "the entity table travels whole; this assertion is the record of that"
    );

    // **The saved list arrived, and it arrived as the reader wrote it** (#507).
    // A count says a row crossed; only the text says the *query* did, and the
    // query is the whole of what a saved list is -- the label is a name for it
    // and the id is how it is addressed.
    assert_eq!(
        saved_lists(there).await,
        vec![(
            estate.saved_list.clone(),
            SAVED_LABEL.to_owned(),
            SAVED_QUERY.to_owned()
        )],
        "the saved smart list did not cross with its id, its name and its query"
    );
}

/// **Each toggle alone brings exactly its tables and nothing else**, read off
/// each archive's own table of contents.
///
/// Six dumps over one populated database, so every table exists and has rows
/// in all six: an archive missing a table is missing it because the argument
/// list said so, and not because there was nothing to dump.
#[tokio::test]
async fn each_part_alone_brings_exactly_its_own_tables() {
    let (sharer, dir) = service("sharetoggles").await;
    seed_corpus(&sharer.pool, "toggle").await;

    let none = backup::ShareParts::none();
    for (label, parts, expected) in [
        (
            "links",
            backup::ShareParts {
                links: true,
                ..none
            },
            vec!["entity", "link"],
        ),
        (
            "assets",
            backup::ShareParts {
                assets: true,
                ..none
            },
            vec!["asset", "entity", "route"],
        ),
        (
            "contexts",
            backup::ShareParts {
                contexts: true,
                ..none
            },
            vec!["context", "entity"],
        ),
        (
            "notes",
            backup::ShareParts {
                notes: true,
                ..none
            },
            vec!["entity", "note"],
        ),
        (
            "time",
            backup::ShareParts { time: true, ..none },
            vec!["block", "heartbeat", "timer", "worklog"],
        ),
        (
            "sources",
            backup::ShareParts {
                sources: true,
                ..none
            },
            vec!["source_config"],
        ),
        // The one part with no address book: a saved smart list is not an
        // entity, so `entity` is deliberately absent here.
        (
            "smart lists",
            backup::ShareParts {
                smart_lists: true,
                ..none
            },
            vec!["smart_list"],
        ),
    ] {
        let record = backup::share_export(&sharer, parts)
            .await
            .unwrap_or_else(|error| panic!("a share export of {label} alone: {error}"));
        let held = data_tables(&dir.path().join(&record.file)).await;
        assert_eq!(held, expected, "the {label} part");
        assert!(
            !held.contains(&"setting".to_owned()),
            "the {label} part carries knobas.setting, which is every feature's bookkeeping"
        );
    }
}

/// **An empty table is still a table in the archive** -- the measurement the
/// share dialog's behaviour rests on (#507).
///
/// `pg_dump` writes a `TABLE DATA` entry for every table its argument list
/// names and never counts rows first, so a knobas nobody has saved a search on
/// still produces an archive naming `smart_list` while the part is *on*. That
/// is why hiding the toggle hides nothing by itself, and why the dialog sends
/// the part **off** while no saved list exists rather than merely drawing no
/// checkbox (`BackupSection.svelte`'s `openShare`, pinned in
/// `BackupSection.test.svelte.ts`).
///
/// Asserted rather than argued in a doc comment, for two reasons. It is the
/// load-bearing premise of a decision taken in the *webview*, and a premise
/// stated only in prose is one nobody finds out has stopped being true. And it
/// is a fact about a **tool**: a `pg_dump` that began omitting empty tables
/// would make that switch unnecessary, and the honest way to learn that is a
/// red test rather than a reading of the diff.
///
/// The second half is this criterion's other clause at the seam -- with the
/// part off, an archive of a knobas with nothing saved names no list table --
/// and the pair is what makes each reading mean something: the same empty
/// database, twice, and only the argument list differs.
#[tokio::test]
async fn an_empty_saved_list_table_is_still_in_the_archive_while_the_part_is_on() {
    let (sharer, dir) = service("shareemptylists").await;
    assert_eq!(
        rows(&sharer.pool, "smart_list").await,
        0,
        "this is the knobas nobody has saved a search on"
    );

    let on = backup::share_export(&sharer, backup::ShareParts::default())
        .await
        .expect("a share export with the ratified defaults");
    assert!(
        data_tables(&dir.path().join(&on.file))
            .await
            .contains(&"smart_list".to_owned()),
        "an empty table is not in the archive, so `pg_dump` counts rows after all -- and the share \
         dialog switching the part off is doing nothing"
    );

    let off = backup::share_export(
        &sharer,
        backup::ShareParts {
            smart_lists: false,
            ..backup::ShareParts::default()
        },
    )
    .await
    .expect("a share export with the saved lists switched off");
    assert!(
        !data_tables(&dir.path().join(&off.file))
            .await
            .contains(&"smart_list".to_owned()),
        "the part is off and the archive still names the list table"
    );
}

/// **The saved-lists part off leaves the table out of the archive; on brings
/// it** -- two archives from the *same* populated database (#507).
///
/// The pair is the whole test, and the first half is the load-bearing one. An
/// archive that does not name `smart_list` is evidence of nothing on its own:
/// a knobas nobody has saved a list on produces one whatever the toggle says,
/// and this file's own history has a scan that looked for a needle absent by
/// construction and was green about nothing. So the reading with the part on
/// is the control -- the table is in an archive of *this* database -- and the
/// reading with it off is the claim.
#[tokio::test]
async fn switching_the_saved_lists_part_off_leaves_the_list_table_behind() {
    let (sharer, dir) = service("sharelists").await;
    seed_corpus(&sharer.pool, "lists").await;
    assert_eq!(
        rows(&sharer.pool, "smart_list").await,
        1,
        "the fixture has a saved list there to be left behind"
    );

    let on = backup::share_export(&sharer, backup::ShareParts::default())
        .await
        .expect("a share export with the ratified defaults");
    assert!(
        data_tables(&dir.path().join(&on.file))
            .await
            .contains(&"smart_list".to_owned()),
        "saved smart lists are on by default"
    );

    let off = backup::share_export(
        &sharer,
        backup::ShareParts {
            smart_lists: false,
            ..backup::ShareParts::default()
        },
    )
    .await
    .expect("a share export with the saved lists switched off");
    let held = data_tables(&dir.path().join(&off.file)).await;
    assert!(
        !held.contains(&"smart_list".to_owned()),
        "the saved lists were switched off and the table is in the archive: {held:?}"
    );
    // ...and the rest of the defaults are still in it, so that "off" is one
    // part switched off rather than an export that fell over.
    for still in [
        "entity",
        "link",
        "asset",
        "route",
        "context",
        "source_config",
    ] {
        assert!(
            held.contains(&still.to_owned()),
            "switching the saved lists off took knobas.{still} with it: {held:?}"
        );
    }
}

/// **A stored query crosses an archive exactly as it was written** (#507,
/// `CONTEXT.md`'s *Smart list*).
///
/// The rule the glossary states about migrations -- nothing ever rewrites
/// `knobas.smart_list.query` to a newer grammar, because an upgrade is a parse
/// in disguise and would make *needs attention* a state no row can reach -- is
/// the same rule for an export and an import, which are the other two places
/// stored text passes through something that could read it. A `pg_dump` table
/// list is the one shape that cannot rewrite anything, and this is the
/// assertion that says so at the seam rather than on the strength of the tool.
///
/// The row is **inserted and not saved**, because `saved::create` refuses a
/// query it cannot run: there is no command that can make this row, which is
/// exactly why it is the row worth putting through an archive. The rail's
/// verdict is read on both machines, so the fixture is not merely a string
/// that survived a copy -- it is a query today's grammar refuses, before and
/// after.
#[tokio::test]
async fn a_saved_query_crosses_an_archive_exactly_as_it_was_written() {
    /// A query today's grammar refuses: a saved list may not name another one.
    const REFUSED_QUERY: &str = "list:mine";
    const REFUSED_ID: &str = "was-a-list";

    let (sharer, sharer_dir) = service("sharelistverbatim").await;
    sqlx::query(
        "insert into knobas.smart_list (id, label, query) values ($1, 'Was a list once', $2)",
    )
    .bind(REFUSED_ID)
    .bind(REFUSED_QUERY)
    .execute(&sharer.pool)
    .await
    .expect("a saved row today's grammar refuses");

    assert!(
        needs_attention(&sharer.pool, REFUSED_ID).await,
        "this test proves nothing unless the stored query is one the grammar refuses here"
    );

    let record = backup::share_export(
        &sharer,
        backup::ShareParts {
            smart_lists: true,
            ..backup::ShareParts::none()
        },
    )
    .await
    .expect("a share export of the saved lists alone");

    let (colleague, colleague_dir) = service("sharelistverbatimtarget").await;
    hand_over(&sharer_dir, &colleague_dir, &record.file);
    backup::restore(&colleague, &record.file)
        .await
        .expect("an archive of saved lists alone restores like any other");

    assert_eq!(
        saved_lists(&colleague.pool).await,
        vec![(
            REFUSED_ID.to_owned(),
            "Was a list once".to_owned(),
            REFUSED_QUERY.to_owned()
        )],
        "the archive rewrote a stored query on its way across"
    );
    assert!(
        needs_attention(&colleague.pool, REFUSED_ID).await,
        "the restored list reads as runnable, so something upgraded it"
    );
}

/// **The cap is `saved::create`'s alone, and an archive is held to nothing**
/// (#507).
///
/// `create` counts the table and refuses the row past `MAX_SAVED_LISTS`, and
/// nothing else does: a restore is a schema dump. So a restored database holds
/// however many lists the archive it came from held, and **every row arrives**.
/// The alternative -- a restore that dropped rows to fit today's constant --
/// loses somebody's data silently, on the machine least able to notice, and
/// would have to choose *which* to lose. The cap reasserts itself the next
/// time anybody saves a list, which is the second half of this test.
///
/// The two cannot disagree on a database this tree has written, because the
/// constant has never moved; the fixture reaches the state with an `insert`
/// past `create`, which is the only way to reach it. It stops being a
/// contrived state the day somebody lowers the number, and there is an open
/// ticket to do that (#533) -- which is why this is asserted now rather than
/// after.
#[tokio::test]
async fn an_archive_holding_more_saved_lists_than_the_cap_restores_all_of_them() {
    let over = knobas_search::saved::MAX_SAVED_LISTS + 1;

    let (sharer, sharer_dir) = service("sharelistcap").await;
    sqlx::query(
        "insert into knobas.smart_list (id, label, query)
         select 'kept-' || n, 'Kept ' || n, '#payouts'
           from generate_series(1, $1) as n",
    )
    .bind(over)
    .execute(&sharer.pool)
    .await
    .expect("more saved lists than the cap, written past the command that counts");
    assert_eq!(rows(&sharer.pool, "smart_list").await, over);

    let record = backup::share_export(
        &sharer,
        backup::ShareParts {
            smart_lists: true,
            ..backup::ShareParts::none()
        },
    )
    .await
    .expect("a share export of the saved lists alone");

    let (colleague, colleague_dir) = service("sharelistcaptarget").await;
    hand_over(&sharer_dir, &colleague_dir, &record.file);
    backup::restore(&colleague, &record.file)
        .await
        .expect("restore");

    let landed = rows(&colleague.pool, "smart_list").await;
    assert_eq!(
        landed,
        over,
        "the restore dropped rows to fit this build's cap of {}",
        knobas_search::saved::MAX_SAVED_LISTS
    );
    // Stated as its own assertion, because the equality above is satisfied by
    // a fixture that never went past the cap at all: a database of exactly
    // `MAX_SAVED_LISTS` rows restores whole and refuses the next `create` too,
    // and this test would be green about a bound it had not crossed.
    assert!(
        landed > knobas_search::saved::MAX_SAVED_LISTS,
        "the fixture holds {landed} lists and the cap is {}; this test is about an archive that \
         goes past it",
        knobas_search::saved::MAX_SAVED_LISTS
    );

    // ...and the cap is still the cap: the next list somebody saves is
    // refused, which is what says the restore went past a bound rather than
    // that there was never one.
    let refused = knobas_app::commands::search::create_smart_list_inner(
        &colleague.pool,
        "One more",
        "#payouts",
    )
    .await
    .expect_err("a saved list past the cap must be refused");
    // The code and the message, not a `Debug` rendering of the whole error: a
    // formatted struct is a *representation* of the refusal, and the cap's
    // digits could match something else printed beside them.
    assert_eq!(refused.code, knobas_app::IpcErrorCode::Invalid);
    assert!(
        refused
            .message
            .contains(&knobas_search::saved::MAX_SAVED_LISTS.to_string()),
        "the refusal does not tell the reader the cap: {}",
        refused.message
    );
}

/// Whether the rail reads *needs attention* for one saved list -- the product's
/// own verdict on a stored query, asked through the read the launcher makes.
async fn needs_attention(pool: &sqlx::PgPool, id: &str) -> bool {
    knobas_app::commands::search::smart_lists_inner(pool)
        .await
        .expect("the rail")
        .into_iter()
        .find(|summary| summary.id == id)
        .unwrap_or_else(|| panic!("no smart list called {id} on the rail"))
        .needs_attention
}

/// The other direction of the defaults: the parts that are **off** by default
/// arrive when they are switched on, and their rows are readable afterwards.
///
/// Without this, "notes and time are excluded" and "notes and time are not
/// implemented" are the same passing test.
#[tokio::test]
async fn switching_notes_and_time_on_brings_the_notes_and_the_hours() {
    let (sharer, sharer_dir) = service("sharepersonal").await;
    let estate = seed_corpus(&sharer.pool, "personal").await;

    let record = backup::share_export(
        &sharer,
        backup::ShareParts {
            notes: true,
            time: true,
            ..backup::ShareParts::default()
        },
    )
    .await
    .expect("a share export with notes and time on");

    let (colleague, colleague_dir) = service("sharepersonaltarget").await;
    hand_over(&sharer_dir, &colleague_dir, &record.file);
    backup::restore(&colleague, &record.file)
        .await
        .expect("restore");

    let there = &colleague.pool;
    for (table, expected) in [
        ("note", 1),
        ("timer", 1),
        ("block", 1),
        ("worklog", 1),
        ("heartbeat", 1),
    ] {
        assert_eq!(
            rows(there, table).await,
            expected,
            "knobas.{table} was switched on and did not arrive"
        );
    }
    let body: String = sqlx::query_scalar("select body_md from knobas.note where id = $1")
        .bind(&estate.note)
        .fetch_one(there)
        .await
        .expect("the restored note");
    assert_eq!(body, "How to drain the payout queue.");
    assert_eq!(
        rows(there, "activity").await,
        0,
        "the activity stream is in no part, whatever is switched on"
    );

    // **The identity sequences came with the tables that own them.** A block
    // and a worklog have `bigint generated always as identity` primary keys,
    // and the restore loads their ids verbatim; a sequence still sitting at 1
    // on the far machine would make the reader's very next tracked block a
    // duplicate-key failure -- days after the restore, and unattributable to
    // it. `pg_dump --table` carries a table's owned sequences, and this is the
    // assertion that says so at the seam rather than on the strength of the
    // documentation.
    sqlx::query(
        "insert into knobas.block (started_at, ended_at, label, kind)
         values (now(), now() + interval '1 minute', 'the first block after the restore', 'manual')",
    )
    .execute(there)
    .await
    .expect("a block written after a restore must not collide with a restored id");
    assert_eq!(rows(there, "block").await, 2);
}

/// **A share export carries no secret, and a source restored from one says so.**
///
/// Three readings, because the claim has three parts. The archive's table list
/// names one table for the sources part. The archive's own *contents* -- the
/// SQL `pg_restore` would replay, which is the only place a value can be
/// looked for -- hold neither the credential nor the shape knobas stores one
/// in. And the source lands on the far machine as `missing_secret`, which is
/// the honest verdict there: the keychain is the colleague's and has nothing
/// under that id.
///
/// **Both keychain namespaces** since #509. An importer's token is not a
/// source's credential -- it has no `knobas.source_config` row, so no part list
/// and no `missing_secret` verdict apply to it -- and that is exactly why it
/// gets its own reading here rather than being taken as covered: nothing in the
/// export knows the word `importer`, and a scan that only looked for the
/// source's PAT would be green about a token it never looked for. The token is
/// put in the sharer's keychain under `importer:hcloud` before the export, so
/// the assertion is about a store that really held one.
#[tokio::test]
async fn a_shared_source_carries_no_secret_and_lands_as_missing_secret() {
    const PAT: &str = "knobas-test-pat-2f6c9a4e1b";
    /// The importer's token (#509): a different value from the PAT, so a scan
    /// finding neither cannot be a scan that found one and stopped.
    const IMPORTER_TOKEN: &str = "knobas-test-hcloud-7b1d4e0c93";

    let sharer_secrets: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    let (sharer, sharer_dir) = service_with("sharesecret", Arc::clone(&sharer_secrets)).await;
    let estate = seed_corpus(&sharer.pool, "secret").await;
    knobas_secrets::spawn::put(
        &sharer_secrets,
        &knobas_secrets::KeychainAccount::source(&estate.source),
        knobas_secrets::Secret::just(knobas_source::AuthMethod::Pat, PAT),
    )
    .await
    .expect("the sharer's own credential");
    knobas_secrets::spawn::put(
        &sharer_secrets,
        &knobas_secrets::KeychainAccount::importer("hcloud"),
        knobas_secrets::Secret::just(knobas_source::AuthMethod::ApiToken, IMPORTER_TOKEN),
    )
    .await
    .expect("the sharer's hcloud importer token");

    let record = backup::share_export(&sharer, backup::ShareParts::default())
        .await
        .expect("a share export");
    let archive = sharer_dir.path().join(&record.file);

    // 1. The table list.
    assert!(
        data_tables(&archive)
            .await
            .contains(&"source_config".to_owned()),
        "the sources part is on by default"
    );

    // 2. The contents. `pg_restore` with no destination prints the script it
    //    would replay: the DDL and every row. A `TABLE DATA` entry says a
    //    table's rows are in the file and says nothing about what is in them,
    //    so this is the reading that can see a secret.
    let sql = knobas_db::backup::archive_sql(&archive)
        .await
        .expect("render the archive");
    assert!(
        sql.contains(&estate.source),
        "this scan proves nothing unless the source is in the script it read"
    );
    assert!(
        !sql.contains(PAT),
        "the credential itself is in the archive"
    );
    for envelope in ["\"kind\":\"pat\"", "\"secret\"", "\"v\":1"] {
        assert!(
            !sql.contains(envelope),
            "the keychain envelope's {envelope} is in the archive"
        );
    }
    // The importer's half (#509). Its token, the namespace its account is
    // under, and the envelope kind it is stored as -- an importer has no row in
    // any table the export carries, so any of the three appearing would mean
    // something wrote a credential somewhere no part list mentions.
    assert!(
        !sql.contains(IMPORTER_TOKEN),
        "the importer's token itself is in the archive"
    );
    for trace in ["importer:", "\"kind\":\"api_token\""] {
        assert!(
            !sql.contains(trace),
            "the importer keychain namespace's {trace} is in the archive"
        );
    }

    // 3. The colleague's machine, whose keychain holds nothing.
    let (colleague, colleague_dir) = service("sharesecrettarget").await;
    hand_over(&sharer_dir, &colleague_dir, &record.file);
    backup::restore(&colleague, &record.file)
        .await
        .expect("restore");

    let (state, detail): (String, Option<String>) =
        sqlx::query_as("select auth_state, auth_detail from knobas.source_config where id = $1")
            .bind(&estate.source)
            .fetch_one(&colleague.pool)
            .await
            .expect("the restored source configuration");
    assert_eq!(
        state, "missing_secret",
        "the sharer's machine said `ok`; this one has no credential at all"
    );
    assert!(
        detail.is_some_and(|line| line.contains("archive")),
        "the sources view has to be able to say why"
    );
}

/// ...and a machine that **does** hold the credential keeps the health it
/// restored.
///
/// The other direction, and the one that stops the rule above from being
/// "always say missing_secret": a person restoring their own backup onto the
/// machine that took it has every credential still, and marking those sources
/// missing would drop them out of `trigger_all` until somebody re-typed a
/// password that was never lost.
#[tokio::test]
async fn a_restore_onto_a_machine_that_still_holds_the_credential_leaves_the_health_alone() {
    let (sharer, sharer_dir) = service("keepsecretsource").await;
    let estate = seed_corpus(&sharer.pool, "keep").await;
    let record = backup::share_export(&sharer, backup::ShareParts::default())
        .await
        .expect("a share export");

    let secrets: Arc<dyn knobas_secrets::SecretStore> =
        Arc::new(knobas_secrets::MemoryStore::new());
    knobas_secrets::spawn::put(
        &secrets,
        &knobas_secrets::KeychainAccount::source(&estate.source),
        knobas_secrets::Secret::just(knobas_source::AuthMethod::Pat, "still-here"),
    )
    .await
    .expect("the credential this machine already holds");
    let (same_machine, target_dir) = service_with("keepsecrettarget", secrets).await;
    hand_over(&sharer_dir, &target_dir, &record.file);

    backup::restore(&same_machine, &record.file)
        .await
        .expect("restore");

    let state: String =
        sqlx::query_scalar("select auth_state from knobas.source_config where id = $1")
            .bind(&estate.source)
            .fetch_one(&same_machine.pool)
            .await
            .expect("the restored source configuration");
    assert_eq!(
        state, "ok",
        "the keychain holds this source's secret, so nothing about it is missing"
    );
}

/// **A restored source reads its system from the top** (#455, criterion 2).
///
/// `knobas.source_config.cursor` is where a source stood *against a mirror*,
/// and **no archive carries the mirror** -- neither a backup nor a share
/// export, because it re-syncs. So the position that rides across in the
/// sources part describes a corpus that is not there, and a first sync
/// resuming from it fetches only what changed upstream since somebody else's
/// last run: on a clean machine that is nothing, and every link the archive
/// brought stays an id nothing can open.
///
/// Cleared on the way in, then, which makes the recipient's first run a full
/// sync -- the same run a source added by hand does, because a restored
/// source is in the same position as a new one.
///
/// The negative is the load-bearing half and it is what makes this a rule
/// about the *cursor*: everything else the archive said about the source is
/// still there afterwards. A restore that cleared the row would pass the
/// first assertion.
///
/// # Why the archive here is a **backup**
///
/// Because that is the half of the rule nothing else witnesses. The share
/// export's side is asserted twice in `tests/share_exit.rs` -- once offline
/// and once against the real Jira -- so an implementation that cleared the
/// position only for a file named `knobas-share-…` would pass every other
/// test in this repository, and the widening to a backup's restore is the
/// deliberate part of #455 that Björn's gate is asked to rule on. It is the
/// same rule for the same reason: a backup carries no mirror either, and the
/// machine restoring one is standing in front of an empty `sync.item` with a
/// position that describes a corpus it does not have.
#[tokio::test]
async fn a_restored_source_starts_from_the_top_and_keeps_everything_else() {
    let (sharer, sharer_dir) = service("backupcursorsource").await;
    let estate = seed_corpus(&sharer.pool, "cursor").await;
    // A position -- and two other columns moved off their defaults, so that
    // "the source itself arrived intact" below is an assertion about what the
    // archive carried rather than about what the schema writes for a row
    // nobody filled in.
    sqlx::query(
        "update knobas.source_config
            set cursor = $2, sync_interval_secs = 900, enabled = false
          where id = $1",
    )
    .bind(&estate.source)
    .bind(r#"{"v":1,"since":"2026-09-01T00:00:00Z"}"#)
    .execute(&sharer.pool)
    .await
    .expect("the sharer has synced, so it stands somewhere");

    let record = backup::export_now(&sharer).await.expect("a backup");
    let (colleague, colleague_dir) = service("backupcursortarget").await;
    hand_over(&sharer_dir, &colleague_dir, &record.file);
    backup::restore(&colleague, &record.file)
        .await
        .expect("restore");

    let (cursor, base_url, interval, enabled): (Option<String>, String, i32, bool) =
        sqlx::query_as(
            "select cursor, base_url, sync_interval_secs, enabled
           from knobas.source_config where id = $1",
        )
        .bind(&estate.source)
        .fetch_one(&colleague.pool)
        .await
        .expect("the restored source configuration");

    assert_eq!(
        cursor, None,
        "the sharer's position is against a mirror this machine does not have; \
         resuming from it fetches nothing and every link stays dangling"
    );
    assert_eq!(
        (base_url.as_str(), interval, enabled),
        ("https://jira.example", 900, false),
        "only the position is dropped -- the source itself arrived intact"
    );
}

/// **A restore never resets the settings of the knobas doing the restoring.**
///
/// A share export carries no `knobas.setting`, and the restore used to clear
/// that table unconditionally before loading the archive's own -- which for a
/// partial archive is a delete with nothing to put back. The colleague's
/// nightly schedule, first-run flag and retention stamp are theirs.
#[tokio::test]
async fn restoring_a_partial_archive_leaves_the_targets_own_settings_alone() {
    let (sharer, sharer_dir) = service("settingsource").await;
    seed_corpus(&sharer.pool, "settings").await;
    let record = backup::share_export(&sharer, backup::ShareParts::default())
        .await
        .expect("a share export");

    let (colleague, colleague_dir) = service("settingtarget").await;
    backup::save_schedule(
        &colleague.pool,
        BackupSchedule {
            enabled: true,
            hour: 22,
            minute: 15,
            keep: 3,
        },
    )
    .await
    .expect("the colleague's own schedule");
    let before = rows(&colleague.pool, "setting").await;
    assert!(
        before > 0,
        "this test is about settings that are there to lose"
    );

    hand_over(&sharer_dir, &colleague_dir, &record.file);
    backup::restore(&colleague, &record.file)
        .await
        .expect("restore");

    assert_eq!(
        backup::status(&colleague).await.expect("status").schedule,
        BackupSchedule {
            enabled: true,
            hour: 22,
            minute: 15,
            keep: 3
        },
        "the restore reset the schedule of the machine it restored onto"
    );
    assert_eq!(rows(&colleague.pool, "setting").await, before);
}

/// A partial archive is refused over a populated database exactly as a whole
/// one is: a share export is not a merge either.
#[tokio::test]
async fn a_share_export_restored_over_a_populated_database_is_a_conflict() {
    let (sharer, sharer_dir) = service("shareconflictsource").await;
    seed_corpus(&sharer.pool, "conflict").await;
    let record = backup::share_export(&sharer, backup::ShareParts::default())
        .await
        .expect("a share export");

    let (colleague, colleague_dir) = service("shareconflicttarget").await;
    seed_entity(&colleague.pool, "shareconflict").await;
    hand_over(&sharer_dir, &colleague_dir, &record.file);

    let error = backup::restore(&colleague, &record.file)
        .await
        .expect_err("a populated knobas must not be overwritten by a share export either");
    assert_eq!(
        knobas_app::IpcError::from(error).code,
        knobas_app::IpcErrorCode::Conflict
    );
}

/// A share export with every part switched off is refused, and nothing is
/// written.
///
/// `pg_dump` given neither a schema nor a table dumps the **whole database**,
/// mirror included -- so the empty selection is the one that must never reach
/// it, and this is the assertion that keeps the guard in front of it.
#[tokio::test]
async fn a_share_export_with_no_parts_is_refused_and_writes_nothing() {
    let (sharer, dir) = service("shareempty").await;

    let error = backup::share_export(&sharer, backup::ShareParts::none())
        .await
        .expect_err("nothing selected is nothing to export");
    assert!(
        matches!(error, backup::ExportError::NoParts),
        "{error:?} -- an empty selection must be refused by name"
    );
    assert_eq!(
        knobas_app::IpcError::from(error).code,
        knobas_app::IpcErrorCode::Invalid,
        "nothing went wrong; the request cannot be honoured as asked"
    );
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "the refusal left a file behind"
    );
}

/// Retention never ages out a share export, at the seam that does the deleting.
///
/// `policy::expired` has the rule; this is the wire. `keep: 1` and an export
/// on either side of it, so a rule that read the share export as a backup
/// would delete it here.
#[tokio::test]
async fn a_nightly_export_never_prunes_a_share_export() {
    let (service, dir) = service("sharekeep").await;
    backup::save_schedule(
        &service.pool,
        BackupSchedule {
            keep: 1,
            ..BackupSchedule::default()
        },
    )
    .await
    .expect("store a schedule");
    seed_corpus(&service.pool, "keepshare").await;

    let shared = backup::share_export(&service, backup::ShareParts::default())
        .await
        .expect("a share export");
    std::fs::write(dir.path().join("knobas-20260101-030000.knobas"), b"x").unwrap();

    let backup_record = backup::export_now(&service)
        .await
        .expect("a nightly export");

    let mut left: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    let mut expected = vec![backup_record.file.clone(), shared.file.clone()];
    expected.sort();
    assert_eq!(
        left, expected,
        "keep=1 aged out the old backup and must have left the share export alone"
    );

    // ...and it is still listed, because it is still restorable.
    assert!(
        backup::status(&service)
            .await
            .expect("status")
            .archives
            .iter()
            .any(|archive| archive.file == shared.file),
        "a share export a user has to be able to restore has to be listed"
    );
}

/// **The archive list is newest first, across both kinds of archive.**
///
/// `knobas-share-` sorts above `knobas-2026...` byte for byte -- `s` is not a
/// digit -- so a list ordered on the whole file name shows a share export from
/// last year above a backup taken this morning, under a heading that says
/// newest first and a *Restore* button on the row a person reads first. Four
/// files written by hand, because the assertion is about names and a real
/// export cannot produce two dates a second apart.
#[tokio::test]
async fn the_archive_list_is_ordered_by_date_and_not_by_the_share_prefix() {
    let (service, dir) = service("shareorder").await;
    for name in [
        "knobas-20260101-030000.knobas",
        "knobas-share-20251231-090000.knobas",
        "knobas-20260103-030000.knobas",
        "knobas-share-20260102-090000.knobas",
    ] {
        std::fs::write(dir.path().join(name), b"x").expect("an archive on disk");
    }

    let listed: Vec<String> = backup::status(&service)
        .await
        .expect("status")
        .archives
        .into_iter()
        .map(|archive| archive.file)
        .collect();
    assert_eq!(
        listed,
        vec![
            "knobas-20260103-030000.knobas".to_owned(),
            "knobas-share-20260102-090000.knobas".to_owned(),
            "knobas-20260101-030000.knobas".to_owned(),
            "knobas-share-20251231-090000.knobas".to_owned(),
        ],
        "the settings view lists archives newest first, share exports among them"
    );
}

/// **The list says which archives are share exports** (#455, criterion 1).
///
/// The file name carries it -- `knobas-share-` -- and a name is not a
/// listing: the settings view draws share exports apart from the backups, and
/// a frontend that told them apart by parsing the name would be a second
/// place to keep `policy::share_name` right. So the answer travels with the
/// row.
///
/// Both directions in one read, because a flag that is always false and a
/// flag that is always true both pass a one-sided assertion.
#[tokio::test]
async fn the_archive_list_says_which_archives_are_share_exports() {
    let (service, _dir) = service("sharelisted").await;
    seed_corpus(&service.pool, "listed").await;

    let backup_record = backup::export_now(&service).await.expect("a backup");
    let shared = backup::share_export(&service, backup::ShareParts::default())
        .await
        .expect("a share export");

    let listed: std::collections::BTreeMap<String, bool> = backup::status(&service)
        .await
        .expect("status")
        .archives
        .into_iter()
        .map(|archive| (archive.file, archive.share))
        .collect();

    assert_eq!(
        listed.get(&shared.file),
        Some(&true),
        "the share export is listed as one"
    );
    assert_eq!(
        listed.get(&backup_record.file),
        Some(&false),
        "the nightly backup is not a share export"
    );
}

/// A store that refuses one id and knows nothing about any other.
///
/// The keychain failures a person actually meets are not absence: a locked
/// keychain, `errSecAuthFailed` and a *Deny* on the prompt all arrive as
/// `SecretError::Backend` (`knobas_secrets`' own docs say so). Absence is
/// `Ok(None)`, which is a different answer and already covered.
struct RefusesOne {
    id: String,
}

impl knobas_secrets::SecretStore for RefusesOne {
    fn get(
        &self,
        account: &knobas_secrets::KeychainAccount,
    ) -> Result<Option<knobas_secrets::Secret>, knobas_secrets::SecretError> {
        if *account == knobas_secrets::KeychainAccount::source(&self.id) {
            return Err(knobas_secrets::SecretError::Backend(
                "the keychain is locked".to_owned(),
            ));
        }
        Ok(None)
    }

    fn put(
        &self,
        _account: &knobas_secrets::KeychainAccount,
        _secret: &knobas_secrets::Secret,
    ) -> Result<(), knobas_secrets::SecretError> {
        unreachable!("a restore never writes a credential")
    }

    fn delete(
        &self,
        _account: &knobas_secrets::KeychainAccount,
    ) -> Result<(), knobas_secrets::SecretError> {
        unreachable!("a restore never deletes a credential")
    }
}

/// **A keychain that refuses one source does not silence the verdict on the
/// rest.**
///
/// The question the restore asks is per source, and a single shared `?` over
/// the loop made it per *machine*: the first refusal ended the pass, and every
/// source after it kept the `ok` the sharer's machine wrote -- the sources
/// view that lies, arrived at from the other direction. The refused source is
/// left alone on purpose: a refusal is this machine learning nothing, which is
/// not the same as learning there is no credential, and the next sync run
/// settles it.
#[tokio::test]
async fn a_keychain_that_refuses_one_source_still_settles_the_others() {
    let (sharer, sharer_dir) = service("refusesource").await;
    let estate = seed_corpus(&sharer.pool, "refuse").await;
    // A second source, sorting *after* the first: the loop reads them in id
    // order, so this is the one an aborted pass would never reach.
    let second = format!("{}-zz", estate.source);
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, auth_state)
         values ($1, 'gitea', 'Gitea', 'https://gitea.example', 'pat', 'ok')",
    )
    .bind(&second)
    .execute(&sharer.pool)
    .await
    .expect("a second source configuration");

    let record = backup::share_export(&sharer, backup::ShareParts::default())
        .await
        .expect("a share export");

    let secrets: Arc<dyn knobas_secrets::SecretStore> = Arc::new(RefusesOne {
        id: estate.source.clone(),
    });
    let (colleague, colleague_dir) = service_with("refusetarget", secrets).await;
    hand_over(&sharer_dir, &colleague_dir, &record.file);
    backup::restore(&colleague, &record.file)
        .await
        .expect("a keychain that refuses must not fail the restore");

    let states: Vec<(String, String)> =
        sqlx::query_as("select id, auth_state from knobas.source_config order by id")
            .fetch_all(&colleague.pool)
            .await
            .expect("the restored source configurations");
    assert_eq!(
        states,
        vec![
            (estate.source.clone(), "ok".to_owned()),
            (second.clone(), "missing_secret".to_owned()),
        ],
        "the refused source keeps what it arrived with; the one after it is still asked"
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
            knobas_app::commands::backup::share_export,
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
        (
            "share_export",
            serde_json::json!({ "parts": {
                "links": true, "assets": true, "contexts": true,
                "notes": false, "time": false, "sources": true
            } }),
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
