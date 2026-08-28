//! The migration baseline: the schema every other crate reads and writes.
//!
//! Every test shares one database (see `test_util`), and that database can
//! outlive a run, so seeds are idempotent (`on conflict do nothing`) or carry
//! ids unique per run. `migrate::run` is re-entrant and nothing truncates.
//!
//! What the *search* built on this schema does with it is
//! `crates/knobas-search/tests/search.rs` (ruling P9); what stays here is the
//! shape of the columns and indexes it rests on -- the stored `fts` column,
//! the `sync.live_item` view, the partial link index.

use knobas_db::migrate;

/// The five spellings `link_origin_chk` (migration 0003) allows.
///
/// Spelled out rather than read off `knobas_core::link::Origin`, which is the
/// enum that writes them: this crate is *below* that one and cannot see it,
/// and the point of the pin is that adding an origin needs a migration, not
/// just a variant. The other half -- the enum walked against the migration --
/// is `knobas_core::link`'s own test.
const ORIGINS: [&str; 5] = ["manual", "suggested", "imported", "source", "implied"];

/// `link_active_idx` is what the link commands built on this schema rest on,
/// in all three of its parts: a second *active* link over the same
/// `(from, to, relation)` fails with SQLSTATE 23505; a different `relation`
/// over the same pair is a distinct link and must be allowed; and the index
/// being partial means a tombstone never blocks re-linking.
#[tokio::test]
async fn active_links_are_unique_per_relation_and_tombstones_do_not_block() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    // The database outlives a single run, so every run gets its own pair.
    let run = uuid::Uuid::new_v4();
    let from = format!("test:link-{run}-a");
    let to = format!("test:link-{run}-b");
    for id in [&from, &to] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    link(pool, &from, &to, "related").await.unwrap();

    let duplicate = link(pool, &from, &to, "related").await.unwrap_err();
    assert_eq!(
        duplicate
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505"),
        "a second active link over the same pair and relation must be a unique violation"
    );

    // Third index column: the same pair under another relation is its own link.
    link(pool, &from, &to, "blocks").await.unwrap();

    sqlx::query(
        "update knobas.link set deleted_at = now()
         where from_id = $1 and relation = 'related' and deleted_at is null",
    )
    .bind(&from)
    .execute(pool)
    .await
    .unwrap();

    // The index is partial, so the tombstone does not block a fresh link.
    link(pool, &from, &to, "related").await.unwrap();
}

/// Insert one active link, surfacing the database error rather than panicking.
async fn link(
    pool: &sqlx::PgPool,
    from: &str,
    to: &str,
    relation: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by)
         values ($1,$2,$3,'manual','user')",
    )
    .bind(from)
    .bind(to)
    .bind(relation)
    .execute(pool)
    .await
    .map(|_| ())
}

#[tokio::test]
async fn fts_column_is_stored_not_virtual() {
    let pool = &knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(pool).await.unwrap();
    // Not named `gen`: that is a reserved keyword in edition 2024.
    let (generated,): (String,) = sqlx::query_as(
        "select attgenerated::text from pg_attribute
         where attrelid = 'sync.item'::regclass and attname = 'fts'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        generated, "s",
        "fts column must be STORED (PG 18 defaults to virtual!)"
    );
}

/// `sync.live_item` is the tombstone filter made structural: every reader of
/// the mirror goes through it, so a smart-list author cannot forget the join
/// and ship a launcher that offers rows the source deleted.
#[tokio::test]
async fn live_item_hides_what_a_source_deleted() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4().simple().to_string();
    let live = format!("test:live-{run}");
    let gone = format!("test:gone-{run}");
    for (id, deleted) in [(&live, false), (&gone, true)] {
        sqlx::query(
            "insert into knobas.entity (id, kind, title, deleted_at)
             values ($1, 'ticket', 'Retry failed SEPA payouts',
                     case when $2 then now() end)",
        )
        .bind(id)
        .bind(deleted)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, web_url)
             values ($1, 'test', 'ticket', 'Retry failed SEPA payouts', 'sepa body',
                     '{}'::jsonb, 'https://tidewater.example/browse/PAY-231')",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    let visible: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.live_item where entity_id = any($1) order by entity_id",
    )
    .bind(vec![gone.clone(), live.clone()])
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        visible,
        vec![live.clone()],
        "the tombstoned row must not be in the view"
    );

    // The view carries the mirror's columns *and* the entity's own timestamp,
    // which is the one thing a reader of sync.item alone cannot get.
    let (web_url, entity_updated): (Option<String>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "select web_url, entity_updated_at from sync.live_item where entity_id = $1",
        )
        .bind(&live)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(
        web_url.as_deref(),
        Some("https://tidewater.example/browse/PAY-231")
    );
    let (from_entity,): (chrono::DateTime<chrono::Utc>,) =
        sqlx::query_as("select updated_at from knobas.entity where id = $1")
            .bind(&live)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(entity_updated, from_entity);
}

/// A plain view is inlined by the planner, so the mirror's indexes still serve
/// queries written against `sync.live_item`. If that ever stopped being true,
/// the launcher would silently degrade to a sequential scan over the whole
/// corpus and only a benchmark would notice.
#[tokio::test]
async fn the_view_still_reaches_the_fts_index() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let mut tx = pool.begin().await.unwrap();
    // A test corpus is small enough that a sequential scan wins on cost; this
    // asks the planner what it would do with one that is not.
    sqlx::query("set local enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .unwrap();
    let plan: Vec<String> = sqlx::query_scalar(
        "explain select entity_id
           from sync.live_item, websearch_to_tsquery('english', 'sepa') q
          where fts @@ q",
    )
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    let plan = plan.join("\n");
    assert!(
        plan.contains("item_fts_idx"),
        "the view must still reach the GIN index:\n{plan}"
    );
    tx.rollback().await.unwrap();
}

/// The launcher's non-FTS listings order by recency within a kind or a source,
/// and the activity strip reads the newest lines across every entity -- neither
/// of which 0001's indexes can serve.
#[tokio::test]
async fn zero_two_adds_the_listing_indexes_and_drops_the_one_it_supersedes() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let mut present: Vec<String> = sqlx::query_scalar(
        "select indexname from pg_indexes
          where schemaname in ('knobas', 'sync') and indexname = any($1)",
    )
    .bind(vec![
        "activity_recent_idx".to_owned(),
        "item_kind_updated_idx".to_owned(),
        "item_source_updated_idx".to_owned(),
        "sync_run_source_idx".to_owned(),
        "sync_run_running_idx".to_owned(),
        // Superseded: equality on source_id is the compound index's prefix.
        "item_source_idx".to_owned(),
    ])
    .fetch_all(pool)
    .await
    .unwrap();
    present.sort();
    assert_eq!(
        present,
        [
            "activity_recent_idx",
            "item_kind_updated_idx",
            "item_source_updated_idx",
            "sync_run_running_idx",
            "sync_run_source_idx",
        ],
        "item_source_idx is superseded by item_source_updated_idx and must be gone"
    );
}

/// The Add-source form's values and the credential health the top strip reads
/// are columns on the source, one value each -- and `auth_state` is a closed
/// list, because every reader of it branches on the exact spelling.
#[tokio::test]
async fn source_config_gains_config_and_credential_health() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let id = format!("cfg-{}", uuid::Uuid::new_v4().simple());
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'jira', 'Jira', 'https://jira.example', 'pat')",
    )
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();

    let (config, auth_state, checked_at, backoff): (
        serde_json::Value,
        String,
        Option<chrono::DateTime<chrono::Utc>>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "select config, auth_state, auth_checked_at, backoff_until
           from knobas.source_config where id = $1",
    )
    .bind(&id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(config, serde_json::json!({}));
    assert_eq!(
        auth_state, "unknown",
        "a source nobody has tested is not 'ok'"
    );
    assert!(checked_at.is_none() && backoff.is_none());

    let refused =
        sqlx::query("update knobas.source_config set auth_state = 'expired' where id = $1")
            .bind(&id)
            .execute(pool)
            .await
            .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514"),
        "auth_state is constrained to the five states knobas knows"
    );

    // The CHECK and `knobas_sync::AuthState` are one list written in two
    // places; a state added to one and not the other is a source whose health
    // cannot be stored.
    let (definition,): (String,) = sqlx::query_as(
        "select pg_get_constraintdef(oid) from pg_constraint
          where conname = 'source_config_auth_state_chk'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    for state in [
        "ok",
        "unauthorized",
        "unreachable",
        "missing_secret",
        "unknown",
    ] {
        assert!(
            definition.contains(state),
            "{state:?} missing from {definition}"
        );
    }
}

/// The diagnostics view's per-run log: deliberately not the activity stream,
/// which carries no durations and writes nothing at all for a run that changed
/// nothing. It also outlives its source -- deleting a source must not rewrite
/// its history, so there is no foreign key.
#[tokio::test]
async fn sync_run_logs_a_run_and_outlives_its_source() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let source = format!("run-{}", uuid::Uuid::new_v4().simple());
    let (id,): (i64,) = sqlx::query_as(
        "insert into knobas.sync_run (source_id, trigger) values ($1, 'manual') returning id",
    )
    .bind(&source)
    .fetch_one(pool)
    .await
    .unwrap();

    // While it runs: no finish, no outcome, counters at zero.
    let (finished, outcome, upserted, swept): (
        Option<chrono::DateTime<chrono::Utc>>,
        Option<String>,
        i64,
        i64,
    ) = sqlx::query_as(
        "select finished_at, outcome, upserted, swept from knobas.sync_run where id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert!(finished.is_none() && outcome.is_none());
    assert_eq!((upserted, swept), (0, 0));

    sqlx::query(
        "update knobas.sync_run
            set finished_at = now(), outcome = 'ok', upserted = 21, cursor_after = 'tidewater-v1'
          where id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();

    // No FK: the log survives a source that was never configured at all.
    let (rows,): (i64,) =
        sqlx::query_as("select count(*) from knobas.sync_run where source_id = $1")
            .bind(&source)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);
}

/// App-level state that has no other home: first-run completion, the last
/// opened context, later the export schedule. One table beats a
/// column-per-flag migration per milestone.
#[tokio::test]
async fn setting_stores_json_by_key() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let key = format!("first_run_completed_{}", uuid::Uuid::new_v4().simple());
    for value in [serde_json::json!(false), serde_json::json!(true)] {
        sqlx::query(
            "insert into knobas.setting (key, value) values ($1, $2)
             on conflict (key) do update set value = excluded.value, updated_at = now()",
        )
        .bind(&key)
        .bind(&value)
        .execute(pool)
        .await
        .unwrap();
    }
    let (stored,): (serde_json::Value,) =
        sqlx::query_as("select value from knobas.setting where key = $1")
            .bind(&key)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(stored, serde_json::json!(true));
}

/// `trigger` and `outcome` are closed vocabularies, and the database says so.
///
/// The same pin `auth_state` gets: the enums live in `knobas_sync::run_log`,
/// the columns are plain `text`, and nothing but a constraint keeps the two
/// lists the same. It matters more here than for a display column, because
/// stream F's backoff branches on `outcome` — `unauthorized` is never retried,
/// `unreachable` is — so an unrecognised value is a source that hammers or
/// stalls rather than a label that looks wrong.
///
/// `outcome` must still accept NULL: that is what "running" is.
#[tokio::test]
async fn the_run_log_constrains_its_two_vocabularies() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let source = format!("chk-{}", uuid::Uuid::new_v4().simple());

    // A run in flight: no outcome yet, and that is not a violation.
    let (id,): (i64,) = sqlx::query_as(
        "insert into knobas.sync_run (source_id, trigger) values ($1, 'schedule') returning id",
    )
    .bind(&source)
    .fetch_one(pool)
    .await
    .unwrap();

    for bad in ["cron", "Manual", ""] {
        let refused = sqlx::query("update knobas.sync_run set trigger = $2 where id = $1")
            .bind(id)
            .bind(bad)
            .execute(pool)
            .await;
        assert!(refused.is_err(), "trigger {bad:?} should be refused");
    }
    for bad in ["failed", "OK", ""] {
        let refused = sqlx::query("update knobas.sync_run set outcome = $2 where id = $1")
            .bind(id)
            .bind(bad)
            .execute(pool)
            .await;
        assert!(refused.is_err(), "outcome {bad:?} should be refused");
    }

    // Every spelling the enums produce is accepted.
    for trigger in ["schedule", "manual", "first_run"] {
        sqlx::query("update knobas.sync_run set trigger = $2 where id = $1")
            .bind(id)
            .bind(trigger)
            .execute(pool)
            .await
            .unwrap_or_else(|error| panic!("trigger {trigger:?} refused: {error}"));
    }
    for outcome in ["ok", "unauthorized", "unreachable", "error"] {
        sqlx::query("update knobas.sync_run set outcome = $2 where id = $1")
            .bind(id)
            .bind(outcome)
            .execute(pool)
            .await
            .unwrap_or_else(|error| panic!("outcome {outcome:?} refused: {error}"));
    }
}

/// `origin` is a closed vocabulary, and the database says so.
///
/// The same pin `auth_state` and the run log's two columns get, for the same
/// reason: the column is plain `text`, the list that writes it lives in Rust
/// (`knobas_core::link::Origin`), and nothing but a constraint keeps the two
/// the same. It matters here because origin is not decoration -- the panel
/// tells hand-made links from machine-made ones by it, and `Origin`'s decoder
/// refuses a spelling it does not know, so a stray value is a link that cannot
/// be read back at all.
///
/// The other half of this pin -- the enum's variants against the migration's
/// list -- lives in `knobas_core::link`.
#[tokio::test]
async fn the_link_origin_vocabulary_is_closed() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4().simple().to_string();
    let from = format!("test:origin-{run}-a");
    let to = format!("test:origin-{run}-b");
    for id in [&from, &to] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    // Every spelling `Origin` can produce is storable. One relation each:
    // `link_active_idx` would otherwise refuse the second link of the pair.
    for origin in ORIGINS {
        sqlx::query(
            "insert into knobas.link (from_id, to_id, relation, origin, created_by)
             values ($1, $2, $3, $4, 'user')",
        )
        .bind(&from)
        .bind(&to)
        .bind(origin)
        .bind(origin)
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("origin {origin:?} refused: {error}"));
    }

    for bad in ["confirmed", "Manual", ""] {
        let refused = sqlx::query(
            "insert into knobas.link (from_id, to_id, relation, origin, created_by)
             values ($1, $2, 'related', $3, 'user')",
        )
        .bind(&from)
        .bind(&to)
        .bind(bad)
        .execute(pool)
        .await
        .unwrap_err();
        assert_eq!(
            refused
                .as_database_error()
                .and_then(|e| e.code())
                .as_deref(),
            Some("23514"),
            "origin {bad:?} should be refused by the check constraint"
        );
    }

    // Read from the live catalog rather than from the migration file: what
    // this database enforces is what an existing installation got, and the
    // constraint is only real once the runner has applied it.
    let (definition,): (String,) = sqlx::query_as(
        "select pg_get_constraintdef(oid) from pg_constraint
          where conname = 'link_origin_chk'
            and conrelid = 'knobas.link'::regclass",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    for origin in ORIGINS {
        assert!(
            definition.contains(origin),
            "{origin:?} missing from {definition}"
        );
    }
    // ... and nothing else. Without this the constraint could allow a sixth
    // spelling no Rust variant produces and every assertion above would still
    // pass.
    assert_eq!(
        definition.matches('\'').count() / 2,
        ORIGINS.len(),
        "the constraint allows a different number of origins than knobas writes: {definition}"
    );
}

/// 0003 applied by the runner to a database that predates it and is already
/// full of links -- which is the case that actually happens, and the one that
/// costs something if it is wrong: `migrate::run` is on the boot path, a CHECK
/// added by `ALTER TABLE` validates the rows already there, and a migration
/// that fails to apply is an app that no longer opens.
///
/// It needs a database *without* 0003, which the shared one cannot be, so this
/// test makes its own on the same server and drops it again. Isolation is not
/// incidental here: applying a migration takes an `ACCESS EXCLUSIVE` lock on
/// `knobas.link`, and doing that in the shared database would stall every test
/// running beside this one.
#[tokio::test]
async fn zero_three_applies_through_the_runner_to_a_database_that_predates_it() {
    let shared = knobas_db::test_util::test_pool().await;
    // A database name cannot be a bind parameter, so it is interpolated --
    // and it is this test's own hex uuid, which is what makes that audit
    // trivial.
    let name = format!("knobas_0003_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(sqlx::AssertSqlSafe(format!(r#"create database "{name}""#)))
        .execute(&shared)
        .await
        .unwrap();

    let options = (*shared.connect_options()).clone().database(&name);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .unwrap();

    // Wind it back to the state of an installation that predates 0003:
    // everything 0003 did, undone, including the runner's record of having
    // done it.
    migrate::run(&pool).await.unwrap();
    sqlx::query("alter table knobas.link drop constraint link_origin_chk")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from public._sqlx_migrations where version = 3")
        .execute(&pool)
        .await
        .unwrap();

    // The links such an installation holds: one of every origin knobas writes.
    for id in ["test:pre-a", "test:pre-b"] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    for origin in ORIGINS {
        sqlx::query(
            "insert into knobas.link (from_id, to_id, relation, origin, created_by)
             values ('test:pre-a', 'test:pre-b', $1, $2, 'user')",
        )
        .bind(origin)
        .bind(origin)
        .execute(&pool)
        .await
        .unwrap();
    }

    // The boot path, on that database.
    migrate::run(&pool)
        .await
        .expect("0003 must apply to a database that already holds links");

    let (applied,): (i64,) = sqlx::query_as(
        "select count(*) from public._sqlx_migrations where version = 3 and success",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(applied, 1, "0003 is recorded as applied");

    // Applied, not merely recorded.
    let refused = sqlx::query(
        "insert into knobas.link (from_id, to_id, relation, origin, created_by)
         values ('test:pre-a', 'test:pre-b', 'related', 'confirmed', 'user')",
    )
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514"),
        "the constraint the runner applied must be enforcing"
    );

    // Re-entrant: a second pass over the same database applies nothing again.
    migrate::run(&pool).await.unwrap();
    let (applied,): (i64,) = sqlx::query_as(
        "select count(*) from public._sqlx_migrations where version = 3 and success",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(applied, 1, "a second pass must not apply 0003 again");

    // The scratch database goes away with the server at the end of the run;
    // dropping it here keeps a long-lived one from collecting them.
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"drop database if exists "{name}" with (force)"#
    )))
    .execute(&shared)
    .await
    .unwrap();
}
