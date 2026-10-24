//! The backup archive, at the seam that produces it.
//!
//! Spec §14 / §16.12, ratified: a backup is **everything in the `knobas`
//! schema, notes included**, and the `sync` mirror is left out because it
//! re-syncs. That is a claim about an *archive*, so it is asserted against one
//! -- `pg_restore --list` reads the table of contents the dump actually wrote,
//! which no amount of getting the argument list wrong can fake.
//!
//! Conventions, as everywhere over the shared embedded server: one server per
//! test binary, run-unique seeded ids, set membership rather than counts, and
//! nothing truncates.

use knobas_db::backup;
use knobas_db::test_util::{test_connector, test_pool};

/// A run-unique suffix, so two tests in this binary are never each other's
/// fixture.
fn unique(label: &str) -> String {
    format!(
        "{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos())
    )
}

/// The ratified scope, read off the archive's own table of contents: every
/// table knobas owns is in it, and nothing from the mirror is.
///
/// Deliberately not a hand-written list of table names. The dump is
/// *schema-scoped*, which is the property that keeps it correct when Links v1
/// (#40) or any later migration adds a table -- so the assertion is about the
/// schema each entry belongs to, and a new `knobas` table joins the archive
/// with no change here.
#[tokio::test]
async fn the_archive_carries_the_owned_schema_and_leaves_the_mirror_out() {
    let pool = test_pool().await;
    knobas_db::migrate::run(&pool).await.expect("migrate");
    let connector = test_connector().await;
    let dir = tempfile::tempdir().expect("a scratch directory");
    let archive = dir.path().join("scope.knobas");

    // Rows in both schemas, so the dump runs over a populated server rather
    // than an empty one. They are not what the assertions below rest on --
    // those read the archive's *table of contents*, which lists a table
    // whether or not it has rows. That notes come back with their contents is
    // proved where it is observable, by the round trip in
    // `knobas-app/tests/it/backup.rs`.
    let note_id = format!("note:{}", unique("scope"));
    let entity_id = format!("mock:{}", unique("scope"));
    // A note is an entity (`0006`'s `note_entity_fk`), so its address goes in
    // first. This crate is below `knobas-core`, so the two rows are written by
    // hand rather than through the note store.
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'note', $2)")
        .bind(&note_id)
        .bind("a note that must survive")
        .execute(&pool)
        .await
        .expect("seed a note's entity");
    sqlx::query("insert into knobas.note (id, title, body_md) values ($1, $2, $3)")
        .bind(&note_id)
        .bind("a note that must survive")
        .bind("body")
        .execute(&pool)
        .await
        .expect("seed a note");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'seeded')")
        .bind(&entity_id)
        .execute(&pool)
        .await
        .expect("seed an entity");
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, 'mock', 'ticket', 'seeded', '{}'::jsonb)",
    )
    .bind(&entity_id)
    .execute(&pool)
    .await
    .expect("seed a mirrored item");

    backup::dump(&connector, &archive).await.expect("a dump");

    let toc = backup::archive_contents(&archive)
        .await
        .expect("read the archive's table of contents");

    // The expected set comes from the *catalog*, not from a list written here.
    // That is the whole property under test: the dump is schema-scoped, so a
    // table Links v1 (#40) or any later migration adds has to appear in the
    // archive without anyone editing this file -- and a hand-written list is
    // exactly the thing that would keep passing while a new table went
    // unbacked-up.
    let owned_tables: Vec<String> = sqlx::query_scalar(
        "select c.relname::text
           from pg_class c
           join pg_namespace n on n.oid = c.relnamespace
          where n.nspname = $1 and c.relkind = 'r'
          order by 1",
    )
    .bind(backup::OWNED_SCHEMA)
    .fetch_all(&pool)
    .await
    .expect("read the owned tables from the catalog");

    assert!(
        !owned_tables.is_empty(),
        "the catalog reports no `knobas` tables, so this test would assert nothing"
    );
    for table in &owned_tables {
        assert!(
            toc.iter()
                .any(|e| e.schema == "knobas" && &e.name == table && e.kind == "TABLE DATA"),
            "`knobas.{table}` is in the schema but has no data entry in the archive: {toc:?}"
        );
    }
    assert!(
        !toc.iter().any(|entry| entry.schema == "sync"),
        "the mirror must not be in a backup -- it re-syncs: {toc:?}"
    );
}

/// An archive taken before `0053` (#1245) still restores. Its `COPY` of
/// `knobas.ticket_view` names `jira_ref`, a column the migrated target calls
/// `saved_view`; the restore loads it and the saved view a ticket view shows
/// comes back under the new name, with no `jira_ref` left behind.
#[tokio::test]
async fn an_archive_from_before_the_saved_view_rename_restores() {
    let from = knobas_db::test_util::scratch_database("before-0053").await;
    let pool = from.pool(2).await.expect("a pool on the source database");
    // The archive an install before `0053` took: the old column, and a row.
    sqlx::raw_sql(
        "alter table knobas.ticket_view rename column saved_view to jira_ref;
         insert into knobas.ticket_view (id, name, view_group, position, jira_ref)
           values ('jira-board-7', 'PAY board', 'Jira boards', 0, 'jira:board-7');",
    )
    .execute(&pool)
    .await
    .unwrap();
    let dir = tempfile::tempdir().expect("a scratch directory");
    let archive = dir.path().join("before-0053.knobas");
    backup::dump(&from, &archive).await.expect("a dump");

    let into = knobas_db::test_util::scratch_database("after-0053").await;
    backup::restore(&into, &archive)
        .await
        .expect("an archive from before 0053 restores");

    let pool = into.pool(2).await.expect("a pool on the target");
    let shows: Option<String> =
        sqlx::query_scalar("select saved_view from knobas.ticket_view where id = 'jira-board-7'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(shows.as_deref(), Some("jira:board-7"));
    assert!(
        !has_the_rename_shim(&pool).await,
        "the restore left the old column behind"
    );
}

/// Whether `knobas.ticket_view` has `jira_ref`, the old name the rename shim
/// adds for the length of a load.
async fn has_the_rename_shim(pool: &sqlx::PgPool) -> bool {
    sqlx::query_scalar(
        "select exists (select 1 from information_schema.columns
          where table_schema = 'knobas' and table_name = 'ticket_view' and column_name = 'jira_ref')",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Every row of a `knobas` table, in a stable order, as one text value: what
/// "the rows it held before" means for the bookkeeping tables below.
async fn rows_of(pool: &sqlx::PgPool, table: &str) -> String {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "select coalesce(jsonb_agg(to_jsonb(t) order by to_jsonb(t)::text), '[]')::text
           from knobas.{table} t"
    )))
    .fetch_one(pool)
    .await
    .unwrap()
}

/// An archive dumped off a scratch database once `seed` has run on it.
/// `tables` restricts the dump the way a share export is restricted; `None`
/// is a whole backup.
async fn an_archive_seeded(
    label: &str,
    seed: &str,
    tables: Option<&[&str]>,
) -> (tempfile::TempDir, std::path::PathBuf) {
    let from = knobas_db::test_util::scratch_database(label).await;
    let source = from.pool(2).await.expect("a pool on the source database");
    sqlx::raw_sql(sqlx::AssertSqlSafe(seed.to_owned()))
        .execute(&source)
        .await
        .unwrap();
    let dir = tempfile::tempdir().expect("a scratch directory");
    let archive = dir.path().join(format!("{label}.knobas"));
    match tables {
        None => backup::dump(&from, &archive).await,
        Some(tables) => backup::dump_tables(&from, &archive, tables).await,
    }
    .expect("a dump");
    (dir, archive)
}

/// An archive from a "newer build": its `COPY` of `knobas.entity` names a
/// column this schema lacks, so a load of it refuses.
async fn an_archive_from_a_newer_build(
    label: &str,
    tables: Option<&[&str]>,
) -> (tempfile::TempDir, std::path::PathBuf) {
    an_archive_seeded(
        label,
        "alter table knobas.entity add column from_a_newer_build text;
         insert into knobas.setting (key, value) values ('archive-only', '1'::jsonb);",
        tables,
    )
    .await
}

/// The tables a restore clears for the archive's own rows and puts back when
/// it fails: `knobas_db::backup`'s bookkeeping tables, children first.
const BOOKKEEPING: [&str; 4] = ["setting", "dep_layer_kind", "dep_layer", "dep_view"];

/// Every `knobas` table outside [`BOOKKEEPING`] that holds a row: what "the
/// restore loaded nothing" is asserted against.
async fn tables_holding_rows(pool: &sqlx::PgPool) -> Vec<String> {
    let tables: Vec<String> = sqlx::query_scalar(
        "select c.relname::text from pg_class c
           join pg_namespace n on n.oid = c.relnamespace
          where n.nspname = 'knobas' and c.relkind = 'r' and c.relname <> all($1)
          order by 1",
    )
    .bind(&BOOKKEEPING[..])
    .fetch_all(pool)
    .await
    .unwrap();
    let mut holding = Vec::new();
    for table in tables {
        // A name read off the catalog of a test database, nothing a caller
        // wrote.
        let any: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "select exists (select 1 from knobas.{table})"
        )))
        .fetch_one(pool)
        .await
        .unwrap();
        if any {
            holding.push(table);
        }
    }
    holding
}

/// Restore `archive`, which must not restore, into a fresh target holding
/// rows of its own in the bookkeeping tables; `refused` asserts on the error.
/// Then assert the restore left the target as it found it (#1635, spec #1619
/// ruling B3; #1930): no row of the archive's in any table, the bookkeeping
/// rows the restore cleared to make room for the archive's own -- settings,
/// and the Dependencies layers and views only migrations seed -- back, and
/// the column the rename shim added for the load gone.
async fn a_restore_left_as_found(
    archive: &std::path::Path,
    label: &str,
    refused: impl FnOnce(Result<(), backup::BackupError>),
) {
    let into = knobas_db::test_util::scratch_database(label).await;
    let target = into.pool(2).await.expect("a pool on the target");
    // Rows of this machine's own, so "held before" is more than the seed.
    sqlx::raw_sql(
        "insert into knobas.setting (key, value) values ('this-machine', '\"kept\"'::jsonb);
         insert into knobas.dep_layer (id, name, position) values ('mine', 'Mine', 99);
         insert into knobas.dep_layer_kind (kind, layer_id, position) values ('mine-kind', 'mine', 0);",
    )
    .execute(&target)
    .await
    .unwrap();
    let mut before = Vec::new();
    for table in BOOKKEEPING {
        let rows = rows_of(&target, table).await;
        assert_ne!(
            rows, "[]",
            "knobas.{table} is empty, so this test would assert nothing"
        );
        before.push(rows);
    }

    refused(backup::restore(&into, archive).await);

    assert_eq!(
        tables_holding_rows(&target).await,
        Vec::<String>::new(),
        "the restore that did not happen left the archive's rows behind"
    );
    for (table, held) in BOOKKEEPING.iter().zip(&before) {
        assert_eq!(
            &rows_of(&target, table).await,
            held,
            "knobas.{table} lost or changed rows in a restore that did not happen"
        );
    }
    assert!(
        !has_the_rename_shim(&target).await,
        "the restore that did not happen left the rename shim's column behind"
    );
}

/// A restore of `archive` whose load fails, leaving the target as it was.
async fn a_failed_restore_of(archive: &std::path::Path, label: &str) {
    a_restore_left_as_found(archive, label, |refused| {
        assert!(
            matches!(refused, Err(backup::BackupError::Tool { .. })),
            "a COPY naming a column this schema lacks fails the load: {refused:?}"
        );
    })
    .await;
}

/// A restore of `archive`, whose rows break `key` on `knobas.{table}`,
/// refused by name with one row counted, leaving the target as it was.
async fn a_refused_restore_of(archive: &std::path::Path, label: &str, table: &str, key: &str) {
    a_restore_left_as_found(archive, label, |refused| match refused {
        Err(backup::BackupError::BrokenKey {
            table: refused,
            key: broken,
            rows,
        }) => {
            assert_eq!((refused.as_str(), broken.as_str()), (table, key));
            assert_eq!(rows, 1, "one row of the archive breaks {key}");
        }
        other => panic!("a restore breaking {key} on knobas.{table} must refuse: {other:?}"),
    })
    .await;
}

/// A whole backup that fails to load: every bookkeeping table was cleared for
/// the archive's own rows, and every one is put back.
#[tokio::test]
async fn a_failed_restore_leaves_the_bookkeeping_rows_in_place() {
    let (_dir, archive) = an_archive_from_a_newer_build("newer-build", None).await;
    a_failed_restore_of(&archive, "failed-restore").await;
}

/// A share export that fails to load. It carries `dep_layer` and no other
/// bookkeeping table, so the restore clears that one alone -- which cascades
/// into `dep_layer_kind` -- and leaves `setting` and `dep_view` standing,
/// where putting their rows back without clearing them first would collide.
#[tokio::test]
async fn a_failed_restore_of_a_partial_archive_leaves_the_bookkeeping_rows_in_place() {
    let (_dir, archive) =
        an_archive_from_a_newer_build("newer-share", Some(&["entity", "dep_layer"])).await;
    a_failed_restore_of(&archive, "failed-partial-restore").await;
}

/// A dump into a backup directory that has never been used creates it: the
/// promise `backup::dump` makes, and what a first nightly export relies on.
#[tokio::test]
async fn a_dump_into_a_directory_never_used_creates_it() {
    let pool = test_pool().await;
    knobas_db::migrate::run(&pool).await.expect("migrate");
    let connector = test_connector().await;
    let dir = tempfile::tempdir().expect("a scratch directory");
    let archive = dir.path().join("never").join("used").join("first.knobas");

    let bytes = backup::dump_tables(&connector, &archive, &["setting"])
        .await
        .expect("a dump into a missing directory");

    assert!(archive.is_file(), "no archive at {}", archive.display());
    assert!(bytes > 0, "the dump reported an empty archive");
}

/// A tool that refuses says why: its stderr is in the error, the one clue a
/// failed backup or restore gives the user.
#[tokio::test]
async fn a_tool_that_refuses_reports_its_stderr() {
    // The shared server is what installs the bundled `pg_restore`.
    let _ = test_connector().await;
    let dir = tempfile::tempdir().expect("a scratch directory");
    let archive = dir.path().join("not-an-archive.knobas");
    std::fs::write(&archive, "this is not a pg_dump archive").expect("write the file");

    match backup::archive_contents(&archive).await {
        Err(backup::BackupError::Tool { stderr, .. }) => assert!(
            !stderr.is_empty(),
            "pg_restore refused the file and the error carries none of its stderr"
        ),
        other => panic!("pg_restore should refuse a file that is no archive: {other:?}"),
    }
}

/// A whole backup holding a note with no entity row (#1930): a note is the
/// one content knobas cannot fetch again, so the restore refuses rather than
/// drop it, and names the table and the key.
#[tokio::test]
async fn a_backup_holding_a_note_without_its_entity_is_refused() {
    let (_dir, archive) = an_archive_seeded(
        "orphan-note",
        "alter table knobas.note drop constraint note_entity_fk;
         insert into knobas.note (id, title, body_md) values ('note:orphan', 'orphan', 'body');",
        None,
    )
    .await;
    a_refused_restore_of(&archive, "orphan-note-into", "note", "note_entity_fk").await;
}

/// A backup taken before `0060` (#1708) that holds a context with no entity
/// row (#1930): `0060`'s orphan rule holds on restore as it holds on an
/// upgrade. The orphan is not restored, the whole context and its member
/// come back, a block that ran in the orphan keeps the room it recorded, and
/// the restored database holds `context_entity_fk`. The source is wound back
/// to the schema `0059` left, as `tests/schema.rs` winds back its upgrade.
#[tokio::test]
async fn a_backup_from_before_zero_sixty_restores_without_its_orphan_context() {
    let (_dir, archive) = an_archive_seeded(
        "orphan-context",
        "alter table knobas.context drop constraint context_entity_fk;
         alter table knobas.context alter column id type text collate \"default\";
         alter table knobas.timer alter column context_id type text collate \"default\";
         alter table knobas.block alter column context_id type text collate \"default\";
         alter table knobas.write_queue alter column link_to type text collate \"default\";
         alter table knobas.run_preset alter column pipeline_id type text collate \"default\";
         alter table knobas.ticket_view alter column saved_view type text collate \"default\";
         delete from public._sqlx_migrations where version = 60;
         insert into knobas.entity (id, kind, title) values ('ctx:whole', 'ctx', 'whole'),
                                                            ('mock:member', 'ticket', 'member');
         insert into knobas.context (id, kind, title) values ('ctx:whole', 'adhoc', 'whole'),
                                                             ('ctx:orphan', 'adhoc', 'orphan');
         insert into knobas.link (from_id, to_id, origin, created_by)
         values ('ctx:whole', 'mock:member', 'manual', 'me');
         insert into knobas.block (started_at, ended_at, kind, label, context_id)
         values ('2026-10-01T08:00:00Z', '2026-10-01T09:00:00Z', 'manual', 'standup', 'ctx:orphan');",
        None,
    )
    .await;

    let into = knobas_db::test_util::scratch_database("orphan-context-into").await;
    backup::restore(&into, &archive)
        .await
        .expect("a pre-0060 backup with an orphan context restores");

    let pool = into.pool(2).await.expect("a pool on the target");
    let contexts: Vec<String> = sqlx::query_scalar("select id from knobas.context order by id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        contexts,
        ["ctx:whole"],
        "the orphan goes, the whole one stays"
    );
    let members: Vec<String> =
        sqlx::query_scalar("select to_id from knobas.link where from_id = 'ctx:whole'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        members,
        ["mock:member"],
        "the whole context keeps its member"
    );
    let rooms: Vec<Option<String>> = sqlx::query_scalar("select context_id from knobas.block")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        rooms,
        [Some("ctx:orphan".to_owned())],
        "the afternoon keeps its room"
    );
    // The key itself, re-checked against every row the way adding it checks
    // them: what a restore that left the orphan in would fail with 23503.
    sqlx::raw_sql(
        "alter table knobas.context drop constraint context_entity_fk;
         alter table knobas.context add constraint context_entity_fk
           foreign key (id) references knobas.entity (id) on delete cascade;",
    )
    .execute(&pool)
    .await
    .expect("the restored contexts hold context_entity_fk");
}

/// A share export (`dump_tables`, the path every share part takes) holding a
/// link to an entity it does not carry (#1930): refused by name, and nothing
/// of it restored. No part knobas offers does this, since each part that
/// links takes `entity` with it; the archive is the hand-edited shape.
#[tokio::test]
async fn a_share_export_holding_a_link_to_a_missing_entity_is_refused() {
    let (_dir, archive) = an_archive_seeded(
        "orphan-link",
        "alter table knobas.link drop constraint link_to_id_fkey;
         insert into knobas.entity (id, kind, title) values ('mock:here', 'ticket', 'here');
         insert into knobas.link (from_id, to_id, origin, created_by)
         values ('mock:here', 'mock:gone', 'manual', 'me');",
        Some(&["entity", "link"]),
    )
    .await;
    a_refused_restore_of(&archive, "orphan-link-into", "link", "link_to_id_fkey").await;
}
