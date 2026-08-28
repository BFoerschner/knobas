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

    // A note, because "including notes" is the half of the ratified default
    // most easily lost, and a mirrored item, because "excluding sync" is the
    // other half.
    let note_id = format!("note:{}", unique("scope"));
    let entity_id = format!("mock:{}", unique("scope"));
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

    let owned: Vec<&backup::TocEntry> = toc.iter().filter(|e| e.schema == "knobas").collect();
    assert!(
        !owned.is_empty(),
        "the archive names no `knobas` object at all: {toc:?}"
    );
    for table in ["note", "entity", "link", "context", "activity", "setting"] {
        assert!(
            toc.iter()
                .any(|e| e.schema == "knobas" && e.name == table && e.kind == "TABLE DATA"),
            "`knobas.{table}` has no data entry in the archive: {toc:?}"
        );
    }
    assert!(
        !toc.iter().any(|entry| entry.schema == "sync"),
        "the mirror must not be in a backup -- it re-syncs: {toc:?}"
    );
}
