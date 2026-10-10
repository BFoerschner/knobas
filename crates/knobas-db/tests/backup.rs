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
