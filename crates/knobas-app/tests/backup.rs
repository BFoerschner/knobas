//! The backup, round-tripped: dumped from one database, restored into
//! another, and read back through the seams the shell reads through.
//!
//! A test that dumps and never restores proves that `pg_dump` exits zero. What
//! spec §14 actually promises is that *entity references use the stable ids of
//! §5a, so a dump restores on another machine or after a re-sync* -- so the
//! round trip here is deliberately arranged as the fresh-machine sequence:
//!
//! 1. a machine with links, a note, a context and a mirror **takes a backup**;
//! 2. a *different* database, migrated and empty, **restores** it;
//! 3. that database then **syncs from scratch** -- new mirror rows, new
//!    payloads, new `synced_at`, the same stable entity ids;
//! 4. the restored link, note and history are read back through
//!    `get_entity_inner` and `search_inner`, which is what the slide-over and
//!    the launcher call.
//!
//! Step 3 is the part that makes step 4 mean something. The mirror row every
//! assertion resolves through **did not exist when the archive was written**;
//! it was written by the re-sync afterwards. A link that still lands on it is
//! a link that survived a machine move, which is the property, and one that a
//! test dumping and restoring the mirror along with the rest could not tell
//! apart from copying the whole database.
//!
//! Conventions, as everywhere: one embedded server per test binary, run-unique
//! ids, set membership rather than counts, nothing truncates. The restore
//! target is a database of its own (`test_util::scratch_database`) because a
//! restore refuses a populated `knobas` -- which the shared database always is.

use knobas_app::commands::entity::get_entity_inner;
use knobas_app::commands::search::search_inner;
use knobas_db::backup;
use knobas_search::{SearchFilters, SearchQuery};
use sqlx::PgPool;

/// A token nothing else in this binary can match.
fn token(tag: &str) -> String {
    format!("zb{tag}{}", uuid::Uuid::new_v4().simple())
}

/// The shared database, migrated.
async fn source_pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// The ids one seeded corpus is made of.
struct Seeded {
    /// The run-unique token every seeded title carries, so an assertion can be
    /// scoped to this test's own rows.
    token: String,
    ticket: String,
    pr: String,
    note_title: String,
    context: String,
}

/// A ticket, a PR, a note, a context and a hand-made link between the two
/// entities -- plus the mirror rows a *first* sync would have written.
///
/// Everything except the mirror rows is knobas-owned, and therefore what the
/// archive is expected to carry.
async fn seed(pool: &PgPool, tag: &str) -> Seeded {
    let t = token(tag);
    let ticket = format!("jira:PAY-{t}");
    let pr = format!("gitea:pr-{t}");
    let context = format!("ctx:{t}");
    let note_title = format!("Runbook {t}");

    for (id, kind, title) in [
        (&ticket, "ticket", "Retry failed payouts"),
        (&pr, "pr", "Backoff"),
    ] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(id)
            .bind(kind)
            .bind(title)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, $4, 'before the move', '{\"round\": 1}'::jsonb)",
        )
        .bind(id)
        .bind(if kind == "ticket" { "jira" } else { "gitea" })
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    }

    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, note, created_by)
         values ($1, $2, 'implements', 'manual', 'made by hand before the backup', 'user')",
    )
    .bind(&ticket)
    .bind(&pr)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query("insert into knobas.note (id, title, body_md) values ($1, $2, $3)")
        .bind(format!("note:{t}"))
        .bind(&note_title)
        .bind("How to drain the payout queue.")
        .execute(pool)
        .await
        .unwrap();

    sqlx::query(
        "insert into knobas.context (id, kind, title, anchor_id) values ($1,'ticket',$2,$3)",
    )
    .bind(&context)
    .bind(format!("Context {t}"))
    .bind(&ticket)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "insert into knobas.activity (actor, verb, entity_id, detail)
         values ('user', 'linked', $1, '{}'::jsonb)",
    )
    .bind(&ticket)
    .execute(pool)
    .await
    .unwrap();

    Seeded {
        token: t,
        ticket,
        pr,
        note_title,
        context,
    }
}

/// What a source's next full sync writes into a database that has never seen
/// it: mirror rows only, keyed by the same stable ids, with fresh payloads.
///
/// It never touches `knobas.entity`, `knobas.link` or `knobas.note` -- a sync
/// writes the mirror, and the point of the test is that everything else came
/// out of the archive.
async fn resync(pool: &PgPool, seeded: &Seeded) {
    let ticket_title = format!("Retry failed {} payouts", seeded.token);
    for (id, source, kind, title) in [
        (&seeded.ticket, "jira", "ticket", ticket_title.as_str()),
        (&seeded.pr, "gitea", "pr", "Backoff"),
    ] {
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, $4, 'after the move', '{\"round\": 2}'::jsonb)",
        )
        .bind(id)
        .bind(source)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    }
}

/// The whole promise, end to end.
#[tokio::test]
async fn a_restored_backup_still_links_the_entities_a_later_sync_rebuilt() {
    let from = source_pool().await;
    let seeded = seed(&from, "trip").await;

    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("round-trip.knobas");
    let bytes = backup::dump(&knobas_db::test_util::test_connector().await, &archive)
        .await
        .expect("take a backup");
    assert!(bytes > 0, "the archive is empty");

    // A different database: migrated by `scratch_database`, and holding
    // nothing at all -- the fresh machine.
    let target = knobas_db::test_util::scratch_database("restore").await;
    backup::restore(&target, &archive)
        .await
        .expect("restore into an empty database");

    let into = target.pool(2).await.unwrap();
    // ...and only *then* does the source get synced, so every mirror row the
    // assertions below resolve through was written after the archive was.
    resync(&into, &seeded).await;

    // Seam 1: the detail slide-over. The link was made before the dump; the
    // mirror row it resolves through was made after the restore.
    let detail = get_entity_inner(&into, &seeded.ticket)
        .await
        .expect("the restored ticket opens");
    assert_eq!(
        detail.payload["round"], 2,
        "the mirror is the re-synced one"
    );
    assert_eq!(detail.body_text, "after the move");
    let targets: Vec<&str> = detail
        .links
        .iter()
        .map(|link| link.to_id.as_str())
        .collect();
    assert!(
        targets.contains(&seeded.pr.as_str()),
        "the hand-made link did not survive the move: {:?}",
        detail.links
    );
    assert_eq!(
        detail.links[0].relation, "implements",
        "the relation is part of the link record (§5a)"
    );
    assert!(
        detail.activity.iter().any(|line| line.verb == "linked"),
        "the activity stream is knobas-owned and belongs in a backup: {:?}",
        detail.activity
    );

    // Seam 2: the launcher. Its corpus is `sync.live_item`, which is the
    // re-synced mirror **joined to `knobas.entity`** -- and `knobas.entity`
    // came out of the archive. A hit therefore needs both halves: an entity
    // restored from the backup and an item written by a sync that ran after
    // it.
    let hits = search_inner(
        &into,
        SearchQuery {
            raw: seeded.token.clone(),
            limit: 20,
            filters: SearchFilters::default(),
        },
    )
    .await
    .expect("search the restored database");
    let found: Vec<&str> = hits
        .groups
        .iter()
        .flat_map(|group| group.hits.iter())
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert!(
        found.contains(&seeded.ticket.as_str()),
        "the re-synced ticket is not reachable from the launcher: {found:?}"
    );

    // Seam 3: what no command exposes yet. Notes (M1's search corpus is
    // `sync.live_item` and nothing else -- `knobas_search::corpus::NOTE` is
    // test-only until notes land), contexts (no context command until M2's
    // context work), and a link's `note` column (Links v1, #40, is wiring it
    // through as this lands). All three are knobas-owned and all three are in
    // the ratified backup scope, so they are read back where they live rather
    // than left uncovered until the seam that shows them exists.
    let restored_note: Option<String> =
        sqlx::query_scalar("select title from knobas.note where title = $1")
            .bind(&seeded.note_title)
            .fetch_optional(&into)
            .await
            .unwrap();
    assert_eq!(
        restored_note.as_deref(),
        Some(seeded.note_title.as_str()),
        "a backup includes notes (§16.12) and this one did not come back"
    );
    let anchored: Option<String> =
        sqlx::query_scalar("select anchor_id from knobas.context where id = $1")
            .bind(&seeded.context)
            .fetch_optional(&into)
            .await
            .unwrap();
    assert_eq!(
        anchored.as_deref(),
        Some(seeded.ticket.as_str()),
        "the context and its anchor are knobas-owned"
    );
    let link_note: Option<String> =
        sqlx::query_scalar("select note from knobas.link where from_id = $1")
            .bind(&seeded.ticket)
            .fetch_one(&into)
            .await
            .unwrap();
    assert_eq!(link_note.as_deref(), Some("made by hand before the backup"));
}

/// The mirror is **not** in the archive, so the restored database has exactly
/// the mirror its own sync gave it and nothing from the machine the archive
/// came from.
///
/// The other side of the ratified default, asserted where it is observable: a
/// dump that quietly included `sync` would still pass every assertion above.
#[tokio::test]
async fn a_restore_brings_back_no_mirror_rows_of_its_own() {
    let from = source_pool().await;
    let seeded = seed(&from, "nomirror").await;

    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("no-mirror.knobas");
    backup::dump(&knobas_db::test_util::test_connector().await, &archive)
        .await
        .expect("take a backup");

    let target = knobas_db::test_util::scratch_database("nomirror").await;
    backup::restore(&target, &archive).await.expect("restore");
    let into = target.pool(2).await.unwrap();

    // The entity is there -- it is knobas-owned...
    let entity: Option<String> = sqlx::query_scalar("select id from knobas.entity where id = $1")
        .bind(&seeded.ticket)
        .fetch_optional(&into)
        .await
        .unwrap();
    assert_eq!(entity.as_deref(), Some(seeded.ticket.as_str()));

    // ...and its mirror row is not, because a mirror re-syncs.
    let mirrored: Option<String> =
        sqlx::query_scalar("select entity_id from sync.item where entity_id = $1")
            .bind(&seeded.ticket)
            .fetch_optional(&into)
            .await
            .unwrap();
    assert_eq!(
        mirrored, None,
        "the archive carried a mirror row it was supposed to leave behind"
    );

    // Before the re-sync the detail read therefore has nothing to open, which
    // is the honest state of a restored machine that has not synced yet.
    let error = get_entity_inner(&into, &seeded.ticket)
        .await
        .expect_err("nothing is mirrored yet");
    assert_eq!(error.code, knobas_app::IpcErrorCode::NotFound);
}

/// Restoring over a database that already holds knobas data is refused.
///
/// Spec §14 gives merge-with-a-preview to M4. Until then the only wrong answer
/// is the silent one: overwriting somebody's links with an older archive's.
#[tokio::test]
async fn a_restore_refuses_a_database_that_already_holds_knobas_data() {
    let from = source_pool().await;
    seed(&from, "occupied").await;

    let dir = tempfile::tempdir().unwrap();
    let archive = dir.path().join("occupied.knobas");
    let connector = knobas_db::test_util::test_connector().await;
    backup::dump(&connector, &archive).await.expect("a backup");

    // The shared database is populated by construction -- every test in this
    // binary is in it.
    let error = backup::restore(&connector, &archive)
        .await
        .expect_err("a populated knobas must not be overwritten");
    assert!(
        matches!(error, backup::BackupError::TargetNotEmpty { .. }),
        "{error:?}"
    );
    assert!(
        error.to_string().contains("Merge-restore is not built yet"),
        "the refusal has to say what to do instead: {error}"
    );
}
