//! The migration baseline: the schema every other crate reads and writes.
//!
//! Every test shares one database (see `test_util`), and that database can
//! outlive a run, so seeds are idempotent (`on conflict do nothing`) or carry
//! ids unique per run. `migrate::run` is re-entrant and nothing truncates.
//!
//! What the *search* built on this schema does with it is
//! `crates/knobas-search/tests/it/search.rs` (ruling P9); what stays here is the
//! shape of the columns and indexes it rests on -- the stored `fts` column,
//! the `sync.live_item` view, the partial link index.

use knobas_db::migrate;

/// The three spellings `link_rule_class_chk` (migration 0007) allows.
///
/// Spelled out rather than read off `knobas_core::suggest::RuleClass`, for the
/// reason [`ORIGINS`] is: this crate is below that one, and the point of the pin
/// is that adding a class needs a migration and not just a variant.
const RULE_CLASSES: [&str; 3] = ["exact_key", "similarity", "source_relation"];

/// The five spellings `link_origin_chk` (migration 0003) allows.
///
/// Spelled out rather than read off `knobas_core::link::Origin`, which is the
/// enum that writes them: this crate is *below* that one and cannot see it,
/// and the point of the pin is that adding an origin needs a migration, not
/// just a variant. The other half -- the enum walked against the migration --
/// is `knobas_core::link`'s own test.
const ORIGINS: [&str; 5] = ["manual", "suggested", "imported", "source", "implied"];

/// The context kinds `0010` allows, spelled here for the reason [`ORIGINS`]
/// is: the other half -- the enum walked against the migration -- is
/// `knobas_core::context`'s own test.
const CONTEXT_KINDS: [&str; 3] = ["epic", "ticket", "adhoc"];

/// `link_pair_active_idx` (0011's unordered successor to `link_active_idx`)
/// is what the link commands built on this schema rest on, in all three of
/// its parts: a second *active* link over the same `(from, to, relation)`
/// fails with SQLSTATE 23505; a different `relation` over the same pair is a
/// distinct link and must be allowed; and the index being partial means a
/// tombstone never blocks re-linking.
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
    // `link_pair_active_idx` would otherwise refuse the second link of the pair.
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

/// The write queue's two vocabularies and its two paired invariants, as the
/// live catalog enforces them (migration `0005`).
///
/// The same pin the run log's `trigger`/`outcome` and the link's `origin` get,
/// and it bites hardest here: `knobas_core::write_queue`'s decoders *refuse* a
/// spelling they do not know, so a stray value is a queued write that can
/// never be read back -- an edit the user is owed and knobas can no longer
/// see. The Rust half is `WriteState::ALL`/`WaitReason::ALL` walked against
/// the migration text in that module; this is the half that only a real
/// database can answer.
#[tokio::test]
async fn the_write_queue_constrains_its_states_and_reasons() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let source = format!("chk-{}", uuid::Uuid::new_v4().simple());
    let (id,): (i64,) = sqlx::query_as(
        "insert into knobas.write_queue (source_id, entity_id, op, payload, target_snapshot)
         values ($1, $1 || ':PAY-1', 'comment', '{}'::jsonb, '{}'::jsonb) returning id",
    )
    .bind(&source)
    .fetch_one(pool)
    .await
    .unwrap();

    for state in ["pending", "held", "refused"] {
        sqlx::query("update knobas.write_queue set state = $2 where id = $1")
            .bind(id)
            .bind(state)
            .execute(pool)
            .await
            .unwrap_or_else(|error| panic!("state {state:?} refused: {error}"));
    }
    for bad in ["expired", "Pending", "cancelled", ""] {
        let refused = sqlx::query("update knobas.write_queue set state = $2 where id = $1")
            .bind(id)
            .bind(bad)
            .execute(pool)
            .await;
        assert!(refused.is_err(), "state {bad:?} should be refused");
    }

    // A terminal state and its timestamp are one fact: neither half is
    // writable alone, in either direction.
    for (state, settled) in [("sent", false), ("pending", true)] {
        let refused = sqlx::query(
            "update knobas.write_queue set state = $2, settled_at = case when $3 then now() end
              where id = $1",
        )
        .bind(id)
        .bind(state)
        .bind(settled)
        .execute(pool)
        .await;
        assert!(
            refused.is_err(),
            "state {state:?} with settled_at present={settled} should be refused"
        );
    }

    // A reason to wait belongs only to a write that is waiting.
    sqlx::query("update knobas.write_queue set state = 'pending', wait_reason = $2 where id = $1")
        .bind(id)
        .bind("unreachable")
        .execute(pool)
        .await
        .unwrap();
    for bad in ["refused", "target_changed", ""] {
        let refused = sqlx::query("update knobas.write_queue set wait_reason = $2 where id = $1")
            .bind(id)
            .bind(bad)
            .execute(pool)
            .await;
        assert!(refused.is_err(), "wait_reason {bad:?} should be refused");
    }
    let refused = sqlx::query("update knobas.write_queue set state = 'held' where id = $1")
        .bind(id)
        .execute(pool)
        .await;
    assert!(
        refused.is_err(),
        "a held write may not also claim to be waiting on a source"
    );
}

/// The namespaces `item_entity_reserved_chk` (migration 0006) refuses.
///
/// Spelled out rather than read off `knobas_core::entity::RESERVED_NAMESPACES`,
/// for the reason [`ORIGINS`] is: this crate is below that one, and the point
/// of the pin is that widening the list takes a migration. The other half --
/// the Rust list walked against the migration text -- is
/// `knobas_core::entity`'s own test.
const RESERVED_NAMESPACES: [&str; 5] = ["note", "ctx", "asset", "route", "monitor"];

/// **The floor under "a note is never swept".**
///
/// The sweep tombstones `knobas.entity` rows *through* `sync.item`
/// (`knobas_sync::SWEEP`: `update knobas.entity e ... from sync.item i where
/// i.entity_id = e.id`). So the question "can a note be swept" reduces to "can
/// a mirror row name a note", and this is where that is answered: it cannot,
/// by a CHECK constraint, for every namespace knobas keeps for itself.
///
/// Enforced rather than merely written, and both directions: a `note:` id is
/// refused and an ordinary source id is not, or the constraint could be
/// refusing everything and every assertion above would still pass.
#[tokio::test]
async fn the_mirror_can_never_name_an_entity_knobas_owns() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();
    let run = uuid::Uuid::new_v4();

    for namespace in RESERVED_NAMESPACES {
        let id = format!("{namespace}:owned-{run}");
        // The entity itself is knobas' to write -- that is what an owned kind
        // *is*. Only the mirror row is refused.
        sqlx::query("insert into knobas.entity (id, kind) values ($1, $2)")
            .bind(&id)
            .bind(namespace)
            .execute(pool)
            .await
            .unwrap();

        let refused = sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, payload)
             values ($1, 'jira', 'ticket', '{}'::jsonb)",
        )
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
            "a mirror row naming {id} must be refused by the check constraint"
        );

        // Case-insensitively, because `is_reserved_namespace` is: an id
        // written `NOTE:` addresses the same namespace to every human.
        let shouting = format!("{}:owned-{run}", namespace.to_uppercase());
        sqlx::query("insert into knobas.entity (id, kind) values ($1, $2)")
            .bind(&shouting)
            .bind(namespace)
            .execute(pool)
            .await
            .unwrap();
        let refused = sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, payload)
             values ($1, 'jira', 'ticket', '{}'::jsonb)",
        )
        .bind(&shouting)
        .execute(pool)
        .await
        .unwrap_err();
        assert_eq!(
            refused
                .as_database_error()
                .and_then(|e| e.code())
                .as_deref(),
            Some("23514"),
            "{shouting} must be refused too"
        );
    }

    // The other direction: an ordinary source's id is still perfectly writable,
    // or the constraint would be refusing the mirror itself.
    let ordinary = format!("jira:PAY-{run}");
    sqlx::query("insert into knobas.entity (id, kind) values ($1, 'ticket')")
        .bind(&ordinary)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, payload)
         values ($1, 'jira', 'ticket', '{}'::jsonb)",
    )
    .bind(&ordinary)
    .execute(pool)
    .await
    .unwrap();
    // A namespace that merely *starts like* a reserved one is not reserved:
    // `notebook:` is somebody's source, not knobas' notes.
    let lookalike = format!("notebook:PAY-{run}");
    sqlx::query("insert into knobas.entity (id, kind) values ($1, 'ticket')")
        .bind(&lookalike)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, payload)
         values ($1, 'notebook', 'ticket', '{}'::jsonb)",
    )
    .bind(&lookalike)
    .execute(pool)
    .await
    .unwrap();

    // Read from the live catalog rather than from the migration file: what this
    // database enforces is what an existing installation got.
    let (definition,): (String,) = sqlx::query_as(
        "select pg_get_constraintdef(oid) from pg_constraint
          where conname = 'item_entity_reserved_chk'
            and conrelid = 'sync.item'::regclass",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    for namespace in RESERVED_NAMESPACES {
        assert!(
            definition.contains(namespace),
            "{namespace:?} missing from {definition}"
        );
    }
}

/// A note is an entity, and `0006` is what makes that a fact rather than a
/// convention.
///
/// Both halves matter. The foreign key is what lets a `[[ref]]` be an ordinary
/// link row -- `knobas.link`'s two endpoints reference `knobas.entity(id)`, so
/// a note with no entity row is a note nothing can link to or from. The
/// namespace CHECK is link 2 of the sweep-safety chain: it is what makes
/// "a note's id starts with `note:`" true of every row rather than of every row
/// the store happened to write.
#[tokio::test]
async fn a_note_row_needs_its_entity_and_lives_in_the_note_namespace() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();
    let run = uuid::Uuid::new_v4();

    let orphan = format!("note:orphan-{run}");
    let refused = sqlx::query("insert into knobas.note (id, title) values ($1, 'no entity')")
        .bind(&orphan)
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23503"),
        "a note with no entity row is a note nothing can link to"
    );

    // An id outside the `note:` namespace is refused even with an entity row
    // behind it -- the CHECK is on the note, not on what happens to exist.
    let elsewhere = format!("jira:PAY-{run}");
    sqlx::query("insert into knobas.entity (id, kind) values ($1, 'ticket')")
        .bind(&elsewhere)
        .execute(pool)
        .await
        .unwrap();
    let refused = sqlx::query("insert into knobas.note (id, title) values ($1, 'misfiled')")
        .bind(&elsewhere)
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514"),
        "a note's id is in the namespace knobas keeps for notes"
    );

    // And the pair that is right goes in.
    let good = format!("note:{run}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'note', 'Runbook')")
        .bind(&good)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.note (id, title, body_md) values ($1, 'Runbook', 'body')")
        .bind(&good)
        .execute(pool)
        .await
        .unwrap();
}

/// The two views migration `0007` cuts the link table into are **disjoint and
/// total** over its live rows.
///
/// This is the whole of "the links panel and the tray cannot blur". A `where`
/// clause written out in two readers is a clause one of them can forget or
/// invert; two views whose predicates are each other's negation cannot overlap
/// however either reader is later edited. Asserted against a table holding one
/// row in every state that matters, including the two the views must *both*
/// exclude.
#[tokio::test]
async fn the_confirmed_and_proposed_views_partition_the_live_links() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4();
    let from = format!("test:views-{run}-a");
    let to = format!("test:views-{run}-b");
    for id in [&from, &to] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    // A confirmed link, a live proposal, a dismissed proposal and an unlinked
    // link -- the four states a row can be in.
    sqlx::query(
        "insert into knobas.link
                (from_id, to_id, relation, origin, created_by,
                 confirmed_at, rule, rule_class, reason, deleted_at)
         values ($1, $2, 'confirmed-live',  'manual',    'user',   now(), null, null, null, null),
                ($1, $2, 'proposal-live',   'suggested', 'knobas', null, 'r', 'exact_key', 'why', null),
                ($1, $2, 'proposal-gone',   'suggested', 'knobas', null, 'r', 'exact_key', 'why', now()),
                ($1, $2, 'confirmed-gone',  'manual',    'user',   now(), null, null, null, now())",
    )
    .bind(&from)
    .bind(&to)
    .execute(pool)
    .await
    .unwrap();

    async fn relations(pool: &sqlx::PgPool, view: &str, from: &str) -> Vec<String> {
        let rows: Vec<(String,)> = sqlx::query_as(match view {
            "confirmed" => "select relation from knobas.confirmed_link where from_id = $1",
            _ => "select relation from knobas.proposed_link where from_id = $1",
        })
        .bind(from)
        .fetch_all(pool)
        .await
        .unwrap();
        rows.into_iter().map(|(r,)| r).collect()
    }

    assert_eq!(
        relations(pool, "confirmed", &from).await,
        ["confirmed-live"]
    );
    assert_eq!(relations(pool, "proposed", &from).await, ["proposal-live"]);

    // Disjoint and total, said as a query rather than as two lists: no live row
    // is in both views, and none is in neither.
    let (both, neither): (i64, i64) = sqlx::query_as(
        "select (select count(*) from knobas.confirmed_link c
                   join knobas.proposed_link p on p.id = c.id),
                (select count(*) from knobas.link l
                  where l.deleted_at is null
                    and not exists (select 1 from knobas.confirmed_link c where c.id = l.id)
                    and not exists (select 1 from knobas.proposed_link  p where p.id = l.id))",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        both, 0,
        "a row in both views is a proposal the panel can show"
    );
    assert_eq!(
        neither, 0,
        "a live row in neither view is a link nothing can read"
    );
}

/// `rule_class` is a closed vocabulary, and the database says so.
///
/// The same discipline `link_origin_chk` gets, and it bites for the same
/// reason: `knobas_core::suggest::RuleClass`'s decoder refuses a spelling it
/// does not know, so a stray value is a suggestion that can never be read back.
#[tokio::test]
async fn the_rule_class_vocabulary_is_closed() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4();
    let from = format!("test:class-{run}-a");
    let to = format!("test:class-{run}-b");
    for id in [&from, &to] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    for class in RULE_CLASSES {
        sqlx::query(
            "insert into knobas.link
                    (from_id, to_id, relation, origin, created_by,
                     confirmed_at, rule, rule_class, reason)
             values ($1, $2, $3, 'suggested', 'knobas', null, 'r', $3, 'why')",
        )
        .bind(&from)
        .bind(&to)
        .bind(class)
        .execute(pool)
        .await
        .unwrap_or_else(|error| panic!("{class} must be allowed: {error}"));
    }

    let refused = sqlx::query(
        "insert into knobas.link
                (from_id, to_id, relation, origin, created_by,
                 confirmed_at, rule, rule_class, reason)
         values ($1, $2, 'certain', 'suggested', 'knobas', null, 'r', 'certain', 'why')",
    )
    .bind(&from)
    .bind(&to)
    .execute(pool)
    .await
    .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514"),
        "a class outside the list is a suggestion the reader's decoder refuses"
    );

    // ...and nothing else, so the constraint cannot allow a class no Rust
    // variant produces.
    let (definition,): (String,) = sqlx::query_as(
        "select pg_get_constraintdef(oid) from pg_constraint
          where conname = 'link_rule_class_chk'
            and conrelid = 'knobas.link'::regclass",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        definition.matches('\'').count() / 2,
        RULE_CLASSES.len(),
        "the constraint allows a different number of classes than knobas writes: {definition}"
    );
}

/// `0007` applied by the runner to a database that predates it and is already
/// full of links.
///
/// The case that actually happens on an upgrade, and the one that costs
/// something if it is wrong twice over: `migrate::run` is on the boot path, and
/// `confirmed_at` is what decides whether a link is *in the graph*. A backfill
/// that missed a row would not fail loudly -- it would quietly move that link
/// out of its own links panel and into the suggestion tray, as a proposal with
/// no reason that the CHECK would then have refused to store in the first
/// place.
///
/// Same shape as `zero_three_applies_through_the_runner_to_a_database_that_
/// predates_it`, and its own database for the same reason.
#[tokio::test]
async fn zero_seven_backfills_every_existing_link_as_confirmed() {
    let shared = knobas_db::test_util::test_pool().await;
    let name = format!("knobas_0007_{}", uuid::Uuid::new_v4().simple());
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

    // Wind it back to an installation that predates 0007: everything it did,
    // undone, including the runner's record of having done it.
    migrate::run(&pool).await.unwrap();
    for statement in [
        "drop view knobas.proposed_link",
        "drop view knobas.confirmed_link",
        "drop index knobas.link_proposed_idx",
        "drop index knobas.link_pair_rev_idx",
        "drop index knobas.link_pair_idx",
        "alter table knobas.link drop constraint link_proposal_chk",
        "alter table knobas.link drop constraint link_rule_class_chk",
        "alter table knobas.link drop column reason",
        "alter table knobas.link drop column rule_class",
        "alter table knobas.link drop column rule",
        "alter table knobas.link drop column confirmed_at",
        "delete from public._sqlx_migrations where version = 7",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&pool)
            .await
            .unwrap_or_else(|error| panic!("winding back {statement}: {error}"));
    }

    // The links such an installation holds: every one of them drawn or
    // imported by the user, and dated whenever it was drawn.
    for id in ["test:pre07-a", "test:pre07-b"] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    for origin in ORIGINS {
        sqlx::query(
            "insert into knobas.link (from_id, to_id, relation, origin, created_by, created_at)
             values ('test:pre07-a', 'test:pre07-b', $1, $2, 'user', now() - interval '30 days')",
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
        .expect("0007 must apply to a database that already holds links");

    let (unconfirmed,): (i64,) =
        sqlx::query_as("select count(*) from knobas.link where confirmed_at is null")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        unconfirmed, 0,
        "a link that predates suggestions is a link, not a proposal"
    );

    // Backfilled from `created_at`, not from the minute of the upgrade -- a
    // link was confirmed when it was drawn, and dating them all at migration
    // time is a fact the database would have invented.
    let (drifted,): (i64,) =
        sqlx::query_as("select count(*) from knobas.link where confirmed_at <> created_at")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(drifted, 0, "the backfill must carry each link's own date");

    // And every one of them is in the panel's view rather than the tray's.
    let (visible,): (i64,) =
        sqlx::query_as("select count(*) from knobas.confirmed_link where from_id = 'test:pre07-a'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(visible, ORIGINS.len() as i64);

    // Re-entrant: a second pass applies nothing again.
    migrate::run(&pool).await.unwrap();
    let (applied,): (i64,) = sqlx::query_as(
        "select count(*) from public._sqlx_migrations where version = 7 and success",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(applied, 1, "a second pass must not apply 0007 again");

    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"drop database if exists "{name}" with (force)"#
    )))
    .execute(&shared)
    .await
    .unwrap();
}

/// The inbox stores one thing and only one: the user's own answer (`0009`).
///
/// Two halves, and the second is the one a later reader is likeliest to
/// "simplify" away. The **row survives** whatever happens to the entity it is
/// about -- there is deliberately no foreign key, the same decision
/// `knobas.write_queue` records, and for a sharper reason here: one of the
/// five categories is keyed on a source rather than on an entity, so the key
/// is not an entity id at all. And a row that records **no decision** is
/// refused, so "nothing has been answered about this item" has exactly one
/// spelling -- no row -- rather than two that every reader would have to
/// handle.
#[tokio::test]
async fn the_inbox_stores_a_decision_or_nothing() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();
    let run = uuid::Uuid::new_v4().simple().to_string();

    // A key naming an entity nothing has ever mirrored: the derivation may
    // produce an item for a source that was purged and re-added, and losing
    // the snooze would be the decision destroyed by bookkeeping.
    let key = format!("review_request:nosuchsource-{run}:acme/payouts#144");
    sqlx::query("insert into knobas.inbox_state (item_key, snoozed_until) values ($1, now())")
        .bind(&key)
        .execute(pool)
        .await
        .expect("an answer outlives everything it refers to");

    let empty = format!("mention:nosuchsource-{run}:PAY-1");
    let refused = sqlx::query("insert into knobas.inbox_state (item_key) values ($1)")
        .bind(&empty)
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23514"),
        "a row that is neither snoozed nor done says nothing its absence does not say"
    );

    // Marking done clears the snooze and vice versa, so the constraint can
    // never be satisfied by a row holding two contradictory answers.
    sqlx::query(
        "update knobas.inbox_state set snoozed_until = null, done_at = now() where item_key = $1",
    )
    .bind(&key)
    .execute(pool)
    .await
    .expect("done is as good an answer as snoozed");
}

/// `context.kind` is a closed vocabulary, and the database says so (0010).
///
/// The same pin `origin` gets, for the same reason: the column is plain
/// `text`, the list that writes it lives in Rust
/// (`knobas_core::context::ContextKind`), whose decoder refuses a spelling it
/// does not know -- so a stray value is a context that cannot be read back at
/// all. The other half of this pin -- the enum's variants against the
/// migration's list -- lives in `knobas_core::context`.
#[tokio::test]
async fn the_context_kind_vocabulary_is_closed() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4().simple().to_string();
    // A context's id is its entity's (`0060`), so each gets its entity row.
    sqlx::query(
        "insert into knobas.entity (id, kind)
         select 'ctx:kind-' || $1 || '-' || k, 'ctx' from unnest($2::text[]) as k",
    )
    .bind(&run)
    .bind(CONTEXT_KINDS.iter().copied().chain(["bad"]).collect::<Vec<_>>())
    .execute(pool)
    .await
    .unwrap();
    for kind in CONTEXT_KINDS {
        sqlx::query("insert into knobas.context (id, kind, title) values ($1, $2, $2)")
            .bind(format!("ctx:kind-{run}-{kind}"))
            .bind(kind)
            .execute(pool)
            .await
            .unwrap_or_else(|error| panic!("kind {kind:?} refused: {error}"));
    }

    for bad in ["label", "Epic", ""] {
        let refused =
            sqlx::query("insert into knobas.context (id, kind, title) values ($1, $2, 'x')")
                .bind(format!("ctx:kind-{run}-bad"))
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
            "kind {bad:?} should be refused by the check constraint"
        );
    }

    // Read from the live catalog rather than from the migration file: what
    // this database enforces is what an existing installation got.
    let (definition,): (String,) = sqlx::query_as(
        "select pg_get_constraintdef(oid) from pg_constraint
          where conname = 'context_kind_chk'
            and conrelid = 'knobas.context'::regclass",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    for kind in CONTEXT_KINDS {
        assert!(
            definition.contains(kind),
            "{kind:?} missing from {definition}"
        );
    }
    // ... and nothing else, or the constraint could allow a fourth spelling no
    // Rust variant produces and every assertion above would still pass.
    assert_eq!(
        definition.matches('\'').count() / 2,
        CONTEXT_KINDS.len(),
        "the constraint allows a different number of kinds than knobas writes: {definition}"
    );
}

/// `context_anchor_idx` (0010) is what makes promotion idempotent between the
/// check and the insert: one *unarchived* context per anchor, refused with
/// SQLSTATE 23505 -- while an archived promotion does not block a fresh one,
/// and ad-hoc rows (no anchor) never collide however many exist.
#[tokio::test]
async fn one_unarchived_context_per_anchor() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4().simple().to_string();
    let anchor = format!("test:anchor-{run}");
    sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
        .bind(&anchor)
        .execute(pool)
        .await
        .unwrap();
    // A context's id is its entity's (`0060`).
    sqlx::query(
        "insert into knobas.entity (id, kind)
         select 'ctx:anchor-' || $1 || '-' || n, 'ctx' from generate_series(1, 5) as n",
    )
    .bind(&run)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "insert into knobas.context (id, kind, title, anchor_id) values ($1,'ticket','a',$2)",
    )
    .bind(format!("ctx:anchor-{run}-1"))
    .bind(&anchor)
    .execute(pool)
    .await
    .unwrap();

    let refused = sqlx::query(
        "insert into knobas.context (id, kind, title, anchor_id) values ($1,'ticket','b',$2)",
    )
    .bind(format!("ctx:anchor-{run}-2"))
    .bind(&anchor)
    .execute(pool)
    .await
    .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505"),
        "a second unarchived context on one anchor must be refused"
    );

    // Archive the first: the index covers live contexts only, so the anchor
    // is free again. Since #1501 `promote` answers with the archived context
    // rather than minting a second, and serialises on the anchor itself.
    sqlx::query("update knobas.context set archived_at = now() where id = $1")
        .bind(format!("ctx:anchor-{run}-1"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.context (id, kind, title, anchor_id) values ($1,'ticket','c',$2)",
    )
    .bind(format!("ctx:anchor-{run}-3"))
    .bind(&anchor)
    .execute(pool)
    .await
    .expect("an archived promotion must not block a fresh one");

    // And two ad-hoc rows with no anchor never collide.
    for n in ["4", "5"] {
        sqlx::query("insert into knobas.context (id, kind, title) values ($1,'adhoc','x')")
            .bind(format!("ctx:anchor-{run}-{n}"))
            .execute(pool)
            .await
            .expect("anchorless rows are outside the index");
    }
}

/// `link_pair_active_idx` (0011) is the directed rule replaced by an unordered
/// one: the reverse of an active link is the *same* link, and refused.
///
/// The old `link_active_idx` allowed it, which is #70 -- the rule was directed
/// while `entries_of` reads `from_id = $1 or to_id = $1`, so A->B and B->A both
/// landed and both panels drew two rows for one relationship. All four of the
/// new index's properties are exercised, because the migration replaces the one
/// thing every link write rests on: the reverse is refused, the same pair under
/// another relation is still its own link, a tombstone still does not block
/// re-linking either way round, and the superseded index is gone.
#[tokio::test]
async fn zero_eleven_makes_the_active_link_rule_unordered() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4();
    let from = format!("test:pair-{run}-a");
    let to = format!("test:pair-{run}-b");
    for id in [&from, &to] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    link(pool, &from, &to, "blocks").await.unwrap();

    let reversed = link(pool, &to, &from, "blocks").await.unwrap_err();
    assert_eq!(
        reversed
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505"),
        "the same pair linked the other way round is the same link, not a second one"
    );

    // Third index column, unchanged: the pair under another relation is its own
    // link, from either end (#40 story 15).
    link(pool, &to, &from, "documents").await.unwrap();

    // Still partial: withdrawing frees the pair, and it frees it *both* ways.
    //
    // Keyed by this run's own endpoint, like every other write in this file: the
    // database is shared by every test in the binary, and an unscoped
    // `where relation = 'blocks'` tombstones whatever a test running beside this
    // one just wrote.
    sqlx::query(
        "update knobas.link set deleted_at = now() where relation = 'blocks' and from_id = $1",
    )
    .bind(&from)
    .execute(pool)
    .await
    .unwrap();
    link(pool, &to, &from, "blocks")
        .await
        .expect("a tombstone must not block re-linking in the other direction");

    // The directed rule is gone rather than left in force beside the new one.
    let names: Vec<String> = sqlx::query_scalar(
        "select indexname from pg_indexes
          where schemaname = 'knobas' and indexname = any($1)",
    )
    .bind(vec![
        "link_active_idx".to_owned(),
        "link_pair_active_idx".to_owned(),
    ])
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        names,
        ["link_pair_active_idx"],
        "0011 replaces link_active_idx; leaving it would keep the directed rule in force"
    );
}

/// 0011 applied by the runner to a database that predates it **and already
/// holds the rows it forbids** -- which is the case that actually happens,
/// because the defect shipped.
///
/// `create unique index` fails on a table that violates it, `migrate::run` is on
/// the boot path, and a migration that fails to apply is an app that no longer
/// opens. So the migration resolves the collisions first, and this is the test
/// that it does -- on a database wound back the way
/// [`zero_three_applies_through_the_runner_to_a_database_that_predates_it`]
/// winds one back, and its own, for the same reason: applying a migration takes
/// an `ACCESS EXCLUSIVE` lock on `knobas.link`.
///
/// **Which row survives is the assertion with teeth.** A confirmed link
/// outranks a proposal for the same pair, whatever their ages: that is #41's
/// stranded-proposal symptom, and resolving it the other way would tombstone a
/// link the user drew by hand in favour of a guess nobody accepted.
#[tokio::test]
async fn zero_eleven_resolves_the_reversed_pairs_a_shipped_defect_left_behind() {
    let shared = knobas_db::test_util::test_pool().await;
    let name = format!("knobas_0011_{}", uuid::Uuid::new_v4().simple());
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

    // Wind it back to an installation that predates 0011: the directed index
    // restored, the unordered one gone, and the runner's record of 0011 with it.
    migrate::run(&pool).await.unwrap();
    for statement in [
        "drop index knobas.link_pair_active_idx",
        "create unique index link_active_idx on knobas.link (from_id, to_id, relation) \
         where deleted_at is null",
        "delete from public._sqlx_migrations where version = 11",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    for id in ["test:old-a", "test:old-b", "test:old-c", "test:old-d"] {
        sqlx::query("insert into knobas.entity (id, kind) values ($1,'ticket')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }

    // What such an installation holds. Each row carries its `note` so the
    // survivor can be named rather than counted.
    //
    // The proposal is written *first* and is therefore the older row: the
    // survivor must still be the confirmed link, which is what proves the sort
    // is by standing before age rather than by age alone.
    for (from, to, relation, note, confirmed) in [
        ("test:old-a", "test:old-b", "blocks", "proposal", false),
        ("test:old-b", "test:old-a", "blocks", "hand-drawn", true),
        // A pair with no collision at all: it must come through untouched.
        ("test:old-c", "test:old-d", "blocks", "lonely", true),
        // And one where both sides are confirmed, so age decides.
        ("test:old-a", "test:old-c", "documents", "elder", true),
        ("test:old-c", "test:old-a", "documents", "younger", true),
    ] {
        sqlx::query(
            "insert into knobas.link
                 (from_id, to_id, relation, origin, created_by, note,
                  confirmed_at, rule, rule_class, reason)
             values ($1, $2, $3, 'manual', 'user', $4,
                     case when $5 then now() else null end,
                     case when $5 then null else 'key' end,
                     case when $5 then null else 'exact_key' end,
                     case when $5 then null else 'a reason' end)",
        )
        .bind(from)
        .bind(to)
        .bind(relation)
        .bind(note)
        .bind(confirmed)
        .execute(&pool)
        .await
        .unwrap();
    }

    // The boot path, on that database.
    migrate::run(&pool)
        .await
        .expect("0011 must apply to a database that already holds reversed pairs");

    let survivors: Vec<String> =
        sqlx::query_scalar("select note from knobas.link where deleted_at is null order by note")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        survivors,
        ["elder", "hand-drawn", "lonely"],
        "the confirmed link must outrank the proposal, the older confirmed row must \
         outrank the younger, and an uncontested pair must be left alone"
    );

    // Tombstoned, not deleted: the withdrawal memory is what stops the
    // detector proposing the loser straight back.
    let (total,): (i64,) = sqlx::query_as("select count(*) from knobas.link")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 5, "the losers are tombstoned, never deleted");

    // Applied, not merely recorded.
    let refused = link(&pool, "test:old-d", "test:old-c", "blocks")
        .await
        .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23505"),
        "the unordered index must be in force on the migrated database"
    );

    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"drop database "{name}" with (force)"#
    )))
    .execute(&shared)
    .await
    .unwrap();
}

/// `sync.live_item` also hides what the **user** switched off (migration
/// `0012`, issue #202) — and, just as load-bearing, what was never configured
/// at all.
///
/// Ruled by Björn 2026-08-31, asked explicitly about blast radius: a source the
/// user has turned off is invisible to *every* reader, not merely absent from
/// search. Putting that in the view rather than in each reader is the same
/// decision `0002` made for the tombstone filter, for the same reason — a
/// reader cannot forget a join it does not write.
///
/// **The three-way split is the test.** `sync.item.source_id` has no foreign
/// key to `source_config`, deliberately: `run_once` syncs unconfigured sources
/// (tests, ad-hoc imports). So the view uses `left join … coalesce(s.enabled,
/// true)`, and an inner join — the obvious way to write this — would silently
/// hide the third row below. That is not hypothetical: `knobas-search`'s own
/// suite seeds items for `teamcity` and `confluence` while registering only
/// `jira` and `gitea`.
#[tokio::test]
async fn live_item_hides_a_disabled_source_but_not_an_unconfigured_one() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4().simple().to_string();
    let on = format!("src-on-{run}");
    let off = format!("src-off-{run}");
    // Never inserted into `knobas.source_config` at all.
    let unconfigured = format!("src-none-{run}");

    for (source, enabled) in [(&on, true), (&off, false)] {
        sqlx::query(
            "insert into knobas.source_config
               (id, kind, display_name, base_url, auth_kind, config, enabled)
             values ($1, 'test-kind', 'Test', 'http://x', 'Pat', '{}'::jsonb, $2)",
        )
        .bind(source)
        .bind(enabled)
        .execute(pool)
        .await
        .unwrap();
    }

    let mut ids = Vec::new();
    for source in [&on, &off, &unconfigured] {
        let id = format!("{source}:item");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket','x')")
            .bind(&id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, 'ticket', 'x', 'x', '{}'::jsonb)",
        )
        .bind(&id)
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
        ids.push(id);
    }

    let visible: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.live_item where entity_id = any($1) order by entity_id",
    )
    .bind(ids.clone())
    .fetch_all(pool)
    .await
    .unwrap();

    let mut expected = vec![format!("{on}:item"), format!("{unconfigured}:item")];
    expected.sort();
    assert_eq!(
        visible, expected,
        "the view must hide the disabled source's item, keep the enabled one, \
         and — the trap an inner join falls into — keep the item whose source \
         was never configured"
    );

    // The row is hidden, not gone: disabling is not deleting, and re-enabling
    // must bring it straight back with no re-sync.
    sqlx::query("update knobas.source_config set enabled = true where id = $1")
        .bind(&off)
        .execute(pool)
        .await
        .unwrap();
    let back: i64 = sqlx::query_scalar("select count(*) from sync.live_item where entity_id = $1")
        .bind(format!("{off}:item"))
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(
        back, 1,
        "re-enabling a source must restore its items as they were"
    );
}

/// **`0019`'s expression, written twice, read back against itself.**
///
/// PostgreSQL forbids a generated column from referencing another generated
/// column, so `knobas.asset.fts` cannot read `props_text` and recomputes it.
/// `0019` names that duplication and names this test as the mitigation: the
/// two copies are adjacent in one file, and a change to one alone fails here
/// rather than silently narrowing what the launcher matches.
///
/// Read from the live catalog rather than from the migration file, for the
/// reason the tests above give: what the launcher matches is what PostgreSQL
/// stored, and a file the runner never applied proves nothing.
#[tokio::test]
async fn the_assets_property_text_is_the_same_expression_the_index_matches() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    async fn generation(pool: &sqlx::PgPool, column: &str) -> (String, String) {
        sqlx::query_as(
            "select attgenerated::text, pg_get_expr(adbin, adrelid)
               from pg_attribute
               join pg_attrdef on adrelid = attrelid and adnum = attnum
              where attrelid = 'knobas.asset'::regclass and attname = $1",
        )
        .bind(column)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    let (props_stored, props_expr) = generation(pool, "props_text").await;
    let (fts_stored, fts_expr) = generation(pool, "fts").await;

    // STORED, for roadmap §4 gotcha 1: PG 18 makes an unqualified generated
    // column virtual, and a virtual column takes a `create index` without
    // complaint and then recomputes every row on every keystroke.
    assert_eq!(props_stored, "s", "props_text must be STORED");
    assert_eq!(fts_stored, "s", "fts must be STORED");

    assert!(
        fts_expr.contains(&props_expr),
        "the fts must weight the *same* text props_text holds, or an asset \
         reads as findable by a property it cannot be found by.\n\
         props_text: {props_expr}\n\
         fts:        {fts_expr}"
    );
    // And at weight **C on that expression** -- below the name (A) and the
    // ancestor path (B), which is the ranking `0017` designed and `0019` fills
    // the last rung of. The weight is read off the `setweight` wrapping the
    // property text, not off the first `'C'` anywhere in the statement: a
    // property bag weighted A and some other term weighted C would satisfy the
    // looser reading and invert the launcher's whole ranking.
    let weighted = fts_expr
        .split("setweight(")
        .find(|term| term.contains(&props_expr))
        .unwrap_or_else(|| panic!("no setweight carries the property text: {fts_expr}"));
    assert!(
        weighted.contains("'C'"),
        "the property text must be weighted C, below the name and the path: {weighted}"
    );
}

/// `0001`'s expression for `sync.item.fts`, verbatim: what every row was
/// lexed by before `0027`, and what a title without a `/` must still give.
const FTS_0001: &str = "setweight(to_tsvector('english', coalesce(title, '')), 'A') || \
                        setweight(to_tsvector('english', coalesce(body_text, '')), 'B')";

/// One mirrored row with this title and body, under a fresh entity.
async fn mirror_row(pool: &sqlx::PgPool, id: &str, title: &str, body: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'repo',$2)")
        .bind(id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at, synced_at)
         values ($1,'gitea','repo',$2,$3,'{}'::jsonb,
                 '2026-01-02T03:04:05Z','2026-01-02T03:04:05Z')",
    )
    .bind(id)
    .bind(title)
    .bind(body)
    .execute(pool)
    .await
    .unwrap();
}

/// Only a slashed title changes under `0027` (#612): every other title gives
/// the `tsvector` `0001` gave, byte for byte, and so does every body. The
/// slashed control is there so the comparison is seen to be able to differ.
#[tokio::test]
async fn zero_twenty_seven_changes_only_a_title_that_holds_a_slash() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4().simple().to_string();
    let unchanged = [
        ("Fix typo in payout", ""),
        (
            "payout-service",
            "tidewater/payout-service\n\nPayout processing",
        ),
        (
            "SEPA retry with exponential backoff",
            "https://git.example/tidewater/payout-service/pulls/7 a/b c/d",
        ),
        ("", ""),
    ];
    for (n, (title, body)) in unchanged.iter().enumerate() {
        let id = format!("gitea:fts27-{run}-{n}");
        mirror_row(pool, &id, title, body).await;
        let (stored, before): (String, String) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "select fts::text, ({FTS_0001})::text from sync.item where entity_id = $1"
        )))
        .bind(&id)
        .fetch_one(pool)
        .await
        .unwrap();
        assert_eq!(stored, before, "title {title:?}, body {body:?}");
    }

    // A known-good literal, measured on 18.6 under `0001`.
    let (typo,): (String,) = sqlx::query_as("select fts::text from sync.item where entity_id = $1")
        .bind(format!("gitea:fts27-{run}-0"))
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(typo, "'fix':1A 'payout':4A 'typo':2A");

    // The control: a slashed title gains its parts at A and keeps the whole.
    let id = format!("gitea:fts27-{run}-slash");
    mirror_row(pool, &id, "tidewater/payout-service", "").await;
    let (stored,): (String,) =
        sqlx::query_as("select fts::text from sync.item where entity_id = $1")
            .bind(&id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        stored,
        "'payout':4A 'payout-servic':3A 'servic':5A 'tidewat':2A 'tidewater/payout-service':1A"
    );
}

/// An existing install (#612's criterion 4): a row mirrored under `0001`'s
/// expression is found by its bare name once `0027` runs, without a sync
/// writing it again -- the generated column is recomputed by the migration's
/// rewrite. Its own database, because the wind-back rewrites `sync.item`.
#[tokio::test]
async fn zero_twenty_seven_reaches_a_row_mirrored_before_it() {
    let pool = knobas_db::test_util::scratch_database("fts27-existing")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");

    // Wind back to an install that predates 0027: `0001`'s expression, and
    // the runner's record of 0027 gone.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "alter table sync.item alter column fts set expression as ({FTS_0001})"
    )))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("delete from public._sqlx_migrations where version = 27")
        .execute(&pool)
        .await
        .unwrap();

    mirror_row(
        &pool,
        "gitea:tidewater/payout-service",
        "tidewater/payout-service",
        "tidewater/payout-service\n\nPayout processing",
    )
    .await;
    let found = async |pool: &sqlx::PgPool| -> (bool, String) {
        sqlx::query_as(
            "select fts @@ websearch_to_tsquery('english', 'payout-service'),
                    synced_at::text
               from sync.live_item where entity_id = 'gitea:tidewater/payout-service'",
        )
        .fetch_one(pool)
        .await
        .unwrap()
    };
    let (before, synced_before) = found(&pool).await;
    assert!(
        !before,
        "under 0001 the bare name matches nothing of the repo"
    );

    migrate::run(&pool).await.unwrap();

    let (after, synced_after) = found(&pool).await;
    assert!(
        after,
        "after 0027 the row already mirrored is found by its bare name"
    );
    assert_eq!(synced_after, synced_before, "and nothing wrote it again");
}

/// The app's pool cancels a statement that runs past thirty seconds (#745).
///
/// Every connection of [`knobas_db::embedded::Connector::app_pool`] carries
/// the limit from its first packet. `migrate::run` over that very pool clears
/// the limit for itself on a pool of its own, and the app pool's connections
/// still carry theirs afterwards: the cleared limit did not leak back in.
#[tokio::test]
async fn the_app_pool_carries_a_statement_timeout_that_a_migration_leaves_in_place() {
    let connector = knobas_db::test_util::scratch_database("app_pool").await;
    let pool = connector.app_pool().await.unwrap();
    let limit = || async {
        sqlx::query_scalar::<_, String>("show statement_timeout")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    assert_eq!(limit().await, "30s");
    assert_eq!(
        knobas_db::embedded::UI_STATEMENT_TIMEOUT,
        std::time::Duration::from_secs(30)
    );

    migrate::run(&pool).await.unwrap();
    assert_eq!(
        limit().await,
        "30s",
        "the pool's connections keep their limit"
    );
}

/// **A scratch database is a copy of one migrated template** (#925). A test
/// that needs a database of its own used to pay a `create database` plus every
/// migration -- about 0.15 s, most of it waiting on the disk -- and hundreds of
/// tests in `knobas-app`'s binary did. Now the migrations run once, into this
/// process's template, and each scratch database is `create database ...
/// template` of it -- or, since #1897, one a finished test held, reset to the
/// template's state (`test_util`'s `recycle`).
///
/// Two scratch databases each hold every migration, a row written to one is
/// not in the other, and the template they were copied from is on the server
/// and is itself fully migrated.
#[tokio::test]
async fn scratch_databases_are_migrated_copies_of_one_template_and_stay_apart() {
    let applied = |pool: sqlx::PgPool| async move {
        sqlx::query_scalar::<_, i64>("select count(*) from _sqlx_migrations where success")
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    let one = knobas_db::test_util::scratch_database("template-one")
        .await
        .pool(2)
        .await
        .unwrap();
    let two = knobas_db::test_util::scratch_database("template-two")
        .await
        .pool(2)
        .await
        .unwrap();

    // What `migrate::run` applies: the same macro over the same directory.
    let migrations = i64::try_from(sqlx::migrate!("./migrations").migrations.len()).unwrap();
    assert_eq!(
        applied(one.clone()).await,
        migrations,
        "the first holds every migration"
    );
    assert_eq!(
        applied(two.clone()).await,
        migrations,
        "the second holds every migration"
    );

    sqlx::query("create table knobas.only_in_one (x int)")
        .execute(&one)
        .await
        .unwrap();
    let in_two: Option<String> =
        sqlx::query_scalar("select to_regclass('knobas.only_in_one')::text")
            .fetch_one(&two)
            .await
            .unwrap();
    assert_eq!(in_two, None, "a table made in one copy is not in the other");

    let templates: Vec<String> = sqlx::query_scalar(
        "select datname::text from pg_database where datname like 'knobas_template_%'",
    )
    .fetch_all(&one)
    .await
    .unwrap();
    assert!(
        !templates.is_empty(),
        "the run keeps a migrated template on the server: {templates:?}"
    );
}

/// The columns that hold an entity id and reach `knobas.entity` by no foreign
/// key, spelled out: the catalog cannot tell an id from any other text.
const UNKEYED_ENTITY_IDS: [&str; 18] = [
    "knobas.activity.entity_id",
    "knobas.block.context_id",
    "knobas.block.entity_id",
    "knobas.heartbeat.entity_id",
    "knobas.link_set_hidden.member_id",
    "knobas.link_set_hidden.ticket_id",
    "knobas.link_set_member.entity_id",
    "knobas.run_preset.pipeline_id",
    "knobas.start_work_step.ticket_id",
    "knobas.ticket_view.saved_view",
    "knobas.timer.context_id",
    "knobas.timer.entity_id",
    "knobas.worklog.entity_id",
    "knobas.write_queue.entity_id",
    "knobas.write_queue.link_to",
    "sync.saved_view_member.entity_id",
    "sync.similar_text_counted.entity_id",
    "sync.similar_text_top.other_id",
];

/// Migration `0050` (#1172): every column that holds an entity id compares
/// bytewise (`collate "C"`), so a probe by id walks its btree with `memcmp`
/// rather than the database's `strcoll` -- about 0.75 us against 4.7 us on
/// the responsiveness probe's mirror. "Every" is `knobas.entity.id`, each
/// column a chain of single-column foreign keys leads from to it, and
/// [`UNKEYED_ENTITY_IDS`]. A text column a user reads sorted, a title, keeps
/// the database's collation. `0060` (#1708) collated the six `0050` missed
/// and keyed `knobas.context.id` to its entity, so the chain reaches it.
#[tokio::test]
async fn every_entity_id_compares_bytewise_and_a_title_by_the_database() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let keyed: Vec<(String, String)> = sqlx::query_as(
        "with recursive ids(rel, att) as (
           select 'knobas.entity'::regclass::oid, a.attnum
             from pg_attribute a
            where a.attrelid = 'knobas.entity'::regclass and a.attname = 'id'
           union
           select c.conrelid, c.conkey[1]
             from pg_constraint c
             join ids on c.confrelid = ids.rel and c.confkey[1] = ids.att
            where c.contype = 'f' and cardinality(c.conkey) = 1
         )
         select n.nspname || '.' || r.relname || '.' || a.attname, co.collname::text
           from ids
           join pg_class r on r.oid = ids.rel
           join pg_namespace n on n.oid = r.relnamespace
           join pg_attribute a on a.attrelid = ids.rel and a.attnum = ids.att
           join pg_collation co on co.oid = a.attcollation
          order by 1",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    // The chain reaches past `entity`: `sync.item`, and the tables keyed by it.
    for reached in [
        "knobas.context.id",
        "knobas.link.to_id",
        "sync.item_parent.entity_id",
    ] {
        assert!(
            keyed.iter().any(|(column, _)| column == reached),
            "the foreign keys reach {reached}: {keyed:?}"
        );
    }

    let unkeyed: Vec<(String, String)> = sqlx::query_as(
        "select n.nspname || '.' || r.relname || '.' || a.attname, co.collname::text
           from pg_attribute a
           join pg_class r on r.oid = a.attrelid
           join pg_namespace n on n.oid = r.relnamespace
           join pg_collation co on co.oid = a.attcollation
          where n.nspname || '.' || r.relname || '.' || a.attname = any($1)
          order by 1",
    )
    .bind(UNKEYED_ENTITY_IDS)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        unkeyed.len(),
        UNKEYED_ENTITY_IDS.len(),
        "every named column exists: {unkeyed:?}"
    );

    let not_bytewise: Vec<_> = keyed
        .iter()
        .chain(&unkeyed)
        .filter(|(_, collation)| collation != "C")
        .collect();
    assert!(
        not_bytewise.is_empty(),
        "these hold entity ids and compare by another collation: {not_bytewise:?}"
    );

    let titles: Vec<String> = sqlx::query_scalar(
        "select co.collname::text
           from pg_attribute a join pg_collation co on co.oid = a.attcollation
          where (a.attrelid, a.attname) in (('sync.item'::regclass, 'title'),
                                            ('knobas.context'::regclass, 'title'))",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        titles,
        ["default", "default"],
        "a title sorts as the user reads"
    );
}

/// `0060` (#1708): a context is an entity, and the database says so. A
/// context row with no entity row is refused, so nothing can make one the
/// switcher lists and *Add to context* cannot link to; the entity row going
/// takes its context with it, as a note's does (`0006`).
#[tokio::test]
async fn zero_sixty_refuses_a_context_without_its_entity() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let run = uuid::Uuid::new_v4().simple().to_string();
    let refused = sqlx::query("insert into knobas.context (id, kind, title) values ($1,'adhoc','x')")
        .bind(format!("ctx:no-entity-{run}"))
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(
        refused
            .as_database_error()
            .and_then(|e| e.code())
            .as_deref(),
        Some("23503"),
        "a context without its entity row must be refused: {refused}"
    );

    let whole = format!("ctx:whole-{run}");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "insert into knobas.entity (id, kind, title) values ('{whole}', 'ctx', 'x');
         insert into knobas.context (id, kind, title) values ('{whole}', 'adhoc', 'x');
         delete from knobas.entity where id = '{whole}';"
    )))
    .execute(pool)
    .await
    .expect("a context with its entity row is written, and goes with it");
    let left: i64 = sqlx::query_scalar("select count(*) from knobas.context where id = $1")
        .bind(&whole)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(left, 0, "the entity row going takes its context");
}

/// `0060` on an install that predates it: a context row with no entity row
/// (none of knobas' writers ever made one; a hand edit or a partial restore
/// could) is removed, a whole context stays, and a block that ran in the
/// removed room keeps the id it recorded (`0016`: a dangling `ctx:` id reads
/// correctly). Wound back to the schema `0059` left.
#[tokio::test]
async fn zero_sixty_removes_a_context_without_its_entity_and_keeps_the_rest() {
    let pool = knobas_db::test_util::scratch_database("context-orphans-60")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");

    sqlx::raw_sql(
        "alter table knobas.context drop constraint context_entity_fk;
         alter table knobas.context alter column id type text collate \"default\";
         alter table knobas.timer alter column context_id type text collate \"default\";
         alter table knobas.block alter column context_id type text collate \"default\";
         alter table knobas.write_queue alter column link_to type text collate \"default\";
         alter table knobas.run_preset alter column pipeline_id type text collate \"default\";
         alter table knobas.ticket_view alter column saved_view type text collate \"default\";
         delete from public._sqlx_migrations where version = 60;
         insert into knobas.entity (id, kind, title) values ('ctx:whole', 'ctx', 'whole');
         insert into knobas.context (id, kind, title) values ('ctx:whole', 'adhoc', 'whole'),
                                                             ('ctx:orphan', 'adhoc', 'orphan');
         insert into knobas.block (started_at, ended_at, kind, label, context_id)
         values ('2026-10-01T08:00:00Z', '2026-10-01T09:00:00Z', 'manual', 'standup', 'ctx:orphan');",
    )
    .execute(&pool)
    .await
    .unwrap();

    migrate::run(&pool).await.unwrap();

    let contexts: Vec<String> = sqlx::query_scalar("select id from knobas.context order by id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(contexts, ["ctx:whole"], "the orphan goes, the whole one stays");
    let rooms: Vec<Option<String>> = sqlx::query_scalar("select context_id from knobas.block")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rooms, [Some("ctx:orphan".to_owned())], "the afternoon keeps its room");
}

/// Migration `0053` (#1245): a ticket system's saved views keep no Jira name.
/// The two tables `0044` made are `sync.saved_view` and
/// `sync.saved_view_member`, `knobas.ticket_view.jira_ref` is `saved_view`,
/// and no constraint or index on them still says `jira`.
#[tokio::test]
async fn zero_fifty_three_names_the_saved_views_for_the_role() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let tables: Vec<Option<String>> = sqlx::query_scalar(
        "select to_regclass(name)::text from unnest(array[
           'sync.saved_view', 'sync.saved_view_member', 'sync.jira_view', 'sync.jira_view_member'
         ]) as name",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        tables,
        [
            Some("sync.saved_view".to_owned()),
            Some("sync.saved_view_member".to_owned()),
            None,
            None
        ]
    );

    let columns: Vec<String> = sqlx::query_scalar(
        "select column_name::text from information_schema.columns
          where table_schema = 'knobas' and table_name = 'ticket_view'
            and column_name in ('jira_ref', 'saved_view')",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(columns, ["saved_view"]);

    let named_for_jira: Vec<String> = sqlx::query_scalar(
        "select c.conname::text from pg_constraint c
          where c.conrelid in ('sync.saved_view'::regclass, 'sync.saved_view_member'::regclass)
            and c.conname like '%jira%'
         union all
         select i.relname::text from pg_index x join pg_class i on i.oid = x.indexrelid
          where x.indrelid in ('sync.saved_view'::regclass, 'sync.saved_view_member'::regclass)
            and i.relname like '%jira%'",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(
        named_for_jira.is_empty(),
        "still named for Jira: {named_for_jira:?}"
    );
}

/// An existing install (#1245): the views, their members and a ticket view's
/// reference written under `0044`'s names are there under `0053`'s, so the
/// rename asks no sync to run again. Its own database, because the wind-back
/// renames the tables back.
#[tokio::test]
async fn zero_fifty_three_keeps_the_saved_views_an_install_already_had() {
    let pool = knobas_db::test_util::scratch_database("saved-views-53")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");

    // Wind back to an install that predates 0053: the old names, and the
    // runner's record of 0053 gone.
    sqlx::raw_sql(
        "do $$
         declare r record;
         begin
           for r in
             select c.conrelid::regclass::text as rel, c.conname::text as name
               from pg_constraint c
              where c.conrelid in ('sync.saved_view'::regclass, 'sync.saved_view_member'::regclass)
                and c.conname like 'saved\\_view%'
           loop
             execute format('alter table %s rename constraint %I to %I',
                            r.rel, r.name, 'jira' || substr(r.name, length('saved') + 1));
           end loop;
         end
         $$;
         alter index sync.saved_view_source_idx rename to jira_view_source_idx;
         alter table sync.saved_view rename to jira_view;
         alter table sync.saved_view_member rename to jira_view_member;
         alter table knobas.ticket_view rename column saved_view to jira_ref;
         delete from public._sqlx_migrations where version = 53;",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(
        "insert into knobas.entity (id, kind, title) values ('jira:board-53', 'jira_view', 'PAY board');
         insert into sync.item (entity_id, source_id, kind, payload)
           values ('jira:board-53', 'jira', 'jira_view', '{}');
         insert into sync.jira_view (entity_id, source_id, ref_kind, ref_id, name)
           values ('jira:board-53', 'jira', 'board', '53', 'PAY board');
         insert into sync.jira_view_member (view_id, entity_id, rank)
           values ('jira:board-53', 'jira:PAY-2', 1), ('jira:board-53', 'jira:PAY-1', 2);
         insert into knobas.ticket_view (id, name, view_group, position, jira_ref)
           values ('jira-board-53', 'PAY board', 'Jira boards', 0, 'jira:board-53');",
    )
    .execute(&pool)
    .await
    .unwrap();

    migrate::run(&pool).await.unwrap();

    let name: String =
        sqlx::query_scalar("select name from sync.saved_view where entity_id = 'jira:board-53'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(name, "PAY board");
    let members: Vec<(String, i32)> = sqlx::query_as(
        "select entity_id, rank from sync.saved_view_member
          where view_id = 'jira:board-53' order by rank",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        members,
        [("jira:PAY-2".to_owned(), 1), ("jira:PAY-1".to_owned(), 2)]
    );
    let shows: Option<String> =
        sqlx::query_scalar("select saved_view from knobas.ticket_view where id = 'jira-board-53'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(shows.as_deref(), Some("jira:board-53"));
}

/// Migration `0054` (#1285): a saved view's query is stored under the role's
/// word. `sync.saved_view.jql` is `source_query`, the name the ticket-view
/// facts carry it under (#1273), and no constraint on the table says `jql`.
#[tokio::test]
async fn zero_fifty_four_names_the_saved_views_query_for_the_role() {
    let pool = &knobas_db::test_util::test_pool().await;
    migrate::run(pool).await.unwrap();

    let columns: Vec<String> = sqlx::query_scalar(
        "select column_name::text from information_schema.columns
          where table_schema = 'sync' and table_name = 'saved_view'
            and column_name in ('jql', 'source_query')",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(columns, ["source_query"]);

    let named_for_jql: Vec<String> = sqlx::query_scalar(
        "select conname::text from pg_constraint
          where conrelid = 'sync.saved_view'::regclass and conname like '%jql%'",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(
        named_for_jql.is_empty(),
        "still named for JQL: {named_for_jql:?}"
    );
}

/// An existing install (#1285): a saved view's query written under `0044`'s
/// column name reads back under `0054`'s, so the rename asks no sync to run
/// again. Its own database, because the wind-back renames the column back.
#[tokio::test]
async fn zero_fifty_four_keeps_the_query_an_install_already_had() {
    let pool = knobas_db::test_util::scratch_database("saved-view-query-54")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");

    // Wind back to an install that predates 0054: the old column and its
    // not-null constraint's old name, and the runner's record of 0054 gone.
    sqlx::raw_sql(
        "do $$
         declare r record;
         begin
           for r in
             select conname::text as name from pg_constraint
              where conrelid = 'sync.saved_view'::regclass and conname like '%source\\_query%'
           loop
             execute format('alter table sync.saved_view rename constraint %I to %I',
                            r.name, replace(r.name, 'source_query', 'jql'));
           end loop;
         end
         $$;
         alter table sync.saved_view rename column source_query to jql;
         delete from public._sqlx_migrations where version = 54;",
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(
        "insert into knobas.entity (id, kind, title) values ('jira:board-54', 'jira_view', 'PAY board');
         insert into sync.item (entity_id, source_id, kind, payload)
           values ('jira:board-54', 'jira', 'jira_view', '{}');
         insert into sync.saved_view (entity_id, source_id, ref_kind, ref_id, name, jql)
           values ('jira:board-54', 'jira', 'board', '54', 'PAY board', 'project = PAY ORDER BY Rank ASC');",
    )
    .execute(&pool)
    .await
    .unwrap();

    migrate::run(&pool).await.unwrap();

    let query: String = sqlx::query_scalar(
        "select source_query from sync.saved_view where entity_id = 'jira:board-54'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(query, "project = PAY ORDER BY Rank ASC");
}

/// `0055` (#1571): a source already failing its sign-in when the migration
/// runs started that episode at its last check, and one that is not failing
/// has no episode. Wound back to an install that predates `0055`.
#[tokio::test]
async fn zero_fifty_five_dates_an_episode_already_under_way_at_its_last_check() {
    let pool = knobas_db::test_util::scratch_database("auth-failing-since-55")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");

    sqlx::raw_sql(
        "drop trigger source_config_auth_failing_since on knobas.source_config;
         drop function knobas.keep_auth_failing_since();
         alter table knobas.source_config drop column auth_failing_since;
         delete from public._sqlx_migrations where version = 55;
         insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind, auth_state, auth_checked_at)
         values ('rejected', 'jira', 'rejected', 'http://127.0.0.1:1', 'pat',
                 'unauthorized', '2026-10-01T08:00:00Z'),
                ('fine', 'jira', 'fine', 'http://127.0.0.1:1', 'pat',
                 'ok', '2026-10-01T08:00:00Z');",
    )
    .execute(&pool)
    .await
    .unwrap();

    migrate::run(&pool).await.unwrap();

    let since: Vec<(String, Option<chrono::DateTime<chrono::Utc>>)> =
        sqlx::query_as("select id, auth_failing_since from knobas.source_config order by id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        since,
        vec![
            ("fine".to_owned(), None),
            (
                "rejected".to_owned(),
                Some("2026-10-01T08:00:00Z".parse().unwrap())
            ),
        ]
    );
}

/// `0055`'s trigger is what makes a failed sign-in once per episode: it starts
/// an episode on the first failing verdict, keeps it through the next failing
/// ones (a rejection turning into a missing secret included), and ends it on
/// any other verdict.
#[tokio::test]
async fn zero_fifty_five_keeps_the_start_of_an_episode_of_failed_sign_ins() {
    let pool = knobas_db::test_util::scratch_database("auth-failing-since-trigger")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    sqlx::raw_sql(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('s', 'jira', 's', 'http://127.0.0.1:1', 'pat')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let set = |state: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<chrono::DateTime<chrono::Utc>>>(
                "update knobas.source_config set auth_state = $1 where id = 's'
                 returning auth_failing_since",
            )
            .bind(state)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };

    assert_eq!(set("unknown").await, None, "a new source has no episode");
    let began = set("unauthorized")
        .await
        .expect("the first failure starts one");
    assert_eq!(
        set("unauthorized").await,
        Some(began),
        "a retry is the same episode"
    );
    assert_eq!(
        set("missing_secret").await,
        Some(began),
        "so is a missing secret"
    );
    assert_eq!(set("unreachable").await, None, "any other verdict ends it");
    let again = set("missing_secret")
        .await
        .expect("and the next failure starts another");
    assert!(again > began);
    assert_eq!(set("ok").await, None);
}

/// `0056` (#1650): the mirror stores the lexemes the launcher's `named`
/// compares the typed words with, and each column is, row for row, what the
/// statement computed per match before it: `NAMED`'s two halves over the
/// title and `KEY_NAMED` over a ticket's key, spelled below as they stood in
/// `knobas_search::sql` before #1650. `sync.live_item` shows them, and a title
/// that changes takes its lexemes with it.
#[tokio::test]
async fn zero_fifty_six_stores_what_the_launcher_lexed_per_match() {
    let pool = knobas_db::test_util::scratch_database("stored-name-words")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    let rows = [
        ("jira:PAY-231", "ticket", "Retry failed SEPA payouts"),
        ("jira:PAY-2310", "ticket", "Payout"),
        (
            "gitea:repo:tidewater/payout-service",
            "repo",
            "tidewater/payout-service",
        ),
        ("gitea:branch:7", "branch", "feature/payout"),
        ("confluence:page:1", "page", "The Payouts of the day"),
        ("confluence:page:2", "page", ""),
    ];
    for (id, kind, title) in rows {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(id)
            .bind(kind)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, payload)
             values ($1, split_part($1, ':', 1), $2, $3, '{}'::jsonb)",
        )
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(&pool)
        .await
        .unwrap();
    }

    // Every row, through the view the launcher reads: whether each stored
    // column differs from the expression it stands for.
    let differing = "select i.entity_id from sync.live_item i
        where i.title_words is distinct from tsvector_to_array(to_tsvector('english', i.title))
           or i.title_tail_words is distinct from
              case when strpos(i.title, '/') > 0
                   then tsvector_to_array(to_tsvector('english', split_part(i.title, '/', -1))) end
           or i.key_words is distinct from
              case when i.kind = 'ticket'
                   then tsvector_to_array(to_tsvector('english',
                          substr(i.entity_id, strpos(i.entity_id, ':') + 1))) end";
    let shown: i64 = sqlx::query_scalar("select count(*) from sync.live_item")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        shown, 6,
        "every row is live, so the comparison reads them all"
    );
    let wrong: Vec<String> = sqlx::query_scalar(differing)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(wrong.is_empty(), "stored lexemes differ for {wrong:?}");

    // Literals, so the comparison above cannot agree with a column that
    // copies a wrong expression into both places.
    let words = |id: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_as::<_, (Vec<String>, Option<Vec<String>>, Option<Vec<String>>)>(
                "select title_words, title_tail_words, key_words from sync.item
                  where entity_id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let strs = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        words("jira:PAY-2310").await,
        (strs(&["payout"]), None, Some(strs(&["-2310", "pay"])))
    );
    assert_eq!(
        words("gitea:branch:7").await,
        (strs(&["feature/payout"]), Some(strs(&["payout"])), None)
    );
    assert_eq!(
        words("confluence:page:1").await,
        (strs(&["day", "payout"]), None, None)
    );
    assert_eq!(words("confluence:page:2").await, (Vec::new(), None, None));

    sqlx::query("update sync.item set title = 'Payout jitter' where entity_id = 'jira:PAY-2310'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        words("jira:PAY-2310").await,
        (
            strs(&["jitter", "payout"]),
            None,
            Some(strs(&["-2310", "pay"]))
        )
    );
}

/// `0057` (#1864): the mention rule's trigram index keeps a pending list of
/// at most 256 kB, so a load leaves the planner a few pages to charge for it
/// and not up to 4 MB. With the default limit, 600 bodies of twenty random
/// words reach some 480 unmerged pages before the 4 MB merge, each costed as
/// a random read, and the inbox's mention rule read every body instead of the
/// index once that cost passed a scan of the mirror's prose. Read after every
/// 20 rows, so a list that grew past the cap and was merged before the load
/// ended still counts. Rerun over a list left under the old limit, the
/// migration also merges it, so an upgraded mirror does not wait for its next
/// write.
#[tokio::test]
async fn zero_fifty_seven_caps_the_body_trigram_pending_list() {
    /// The pages 256 kB of pending list fill, and the one a merge starts on.
    const CAP: i32 = 256 / 8 + 1;
    let pool = knobas_db::test_util::scratch_database("body-trigram-pending")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    sqlx::query("create extension if not exists pgstattuple")
        .execute(&pool)
        .await
        .unwrap();
    // An autovacuum worker's analyze merges the list too, and holds the merge
    // lock while it does, so an insert past the cap meanwhile skips its own
    // merge: off, so the only merges are the inserts' and the migration's.
    sqlx::query("alter table sync.item set (autovacuum_enabled = false)")
        .execute(&pool)
        .await
        .unwrap();
    let pending = |pool: sqlx::PgPool| async move {
        sqlx::query_scalar::<_, i32>(
            "select pending_pages from pgstatginindex('sync.item_body_trgm_idx')",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    // 600 tickets from `from` on, 20 a statement; the most pages pending
    // after any of them.
    let load = |from: i32| {
        let pool = pool.clone();
        async move {
            let mut most = 0;
            for batch in (from..from + 600).step_by(20) {
                sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                    "insert into knobas.entity (id, kind, title)
                     select 'jira:T-' || g, 'ticket', 't'
                       from generate_series({batch}, {batch} + 19) g;
                     insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
                     select 'jira:T-' || g, 'jira', 'ticket', 't',
                            (select string_agg(md5((g * 100 + w)::text), ' ')
                               from generate_series(1, 20) w),
                            '{{}}'::jsonb
                       from generate_series({batch}, {batch} + 19) g"
                )))
                .execute(&pool)
                .await
                .unwrap();
                most = most.max(pending(pool.clone()).await);
            }
            most
        }
    };

    let capped = load(1).await;
    assert!(
        capped <= CAP,
        "{capped} pages pending, over the 256 kB limit"
    );

    // The list an index under the default limit is left with, merged by the
    // migration rerun.
    sqlx::query("alter index sync.item_body_trgm_idx reset (gin_pending_list_limit)")
        .execute(&pool)
        .await
        .unwrap();
    let at_default = load(1_001).await;
    assert!(
        at_default > 4 * CAP,
        "the default limit left {at_default} pages pending, too few to tell the cap from it"
    );
    sqlx::raw_sql(include_str!(
        "../migrations/0057_a_mention_index_keeps_a_short_pending_list.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        pending(pool.clone()).await,
        0,
        "the migration merges what was pending"
    );
    let again = load(2_001).await;
    assert!(
        again <= CAP,
        "{again} pages pending after the migration, over the 256 kB limit"
    );
}

/// `0059` (#1869): the launcher's mirror search index keeps a pending list of
/// at most 256 kB, as `0057` keeps the mention index's. With the default
/// limit, 1,000 bodies of forty random words reach about 330 unmerged pages,
/// short of the 4 MB merge; on the responsiveness probe's never-analyzed mirror
/// a list past about 220 pages priced `item_fts_idx` above a walk of every
/// entity, and a two-word search went from 2.5 ms to 25 ms. Read after every
/// 20 rows, so a list that grew past the cap and was merged before the load
/// ended still counts. Rerun over a list left under the old limit, the
/// migration also merges it.
#[tokio::test]
async fn zero_fifty_nine_caps_the_mirror_search_pending_list() {
    /// The pages 256 kB of pending list fill, and the one a merge starts on.
    const CAP: i32 = 256 / 8 + 1;
    let pool = knobas_db::test_util::scratch_database("mirror-fts-pending")
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    sqlx::query("create extension if not exists pgstattuple")
        .execute(&pool)
        .await
        .unwrap();
    // `0057`'s reason: an autovacuum analyze merges the list under the merge
    // lock, so an insert past the cap meanwhile skips its own merge.
    sqlx::query("alter table sync.item set (autovacuum_enabled = false)")
        .execute(&pool)
        .await
        .unwrap();
    let pending = |pool: sqlx::PgPool| async move {
        sqlx::query_scalar::<_, i32>(
            "select pending_pages from pgstatginindex('sync.item_fts_idx')",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    // 1,000 tickets from `from` on, 20 a statement; the most pages pending
    // after any of them.
    let load = |from: i32| {
        let pool = pool.clone();
        async move {
            let mut most = 0;
            for batch in (from..from + 1_000).step_by(20) {
                sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
                    "insert into knobas.entity (id, kind, title)
                     select 'jira:F-' || g, 'ticket', 't'
                       from generate_series({batch}, {batch} + 19) g;
                     insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
                     select 'jira:F-' || g, 'jira', 'ticket', 't',
                            (select string_agg(md5((g * 100 + w)::text), ' ')
                               from generate_series(1, 40) w),
                            '{{}}'::jsonb
                       from generate_series({batch}, {batch} + 19) g"
                )))
                .execute(&pool)
                .await
                .unwrap();
                most = most.max(pending(pool.clone()).await);
            }
            most
        }
    };

    let capped = load(1).await;
    assert!(
        capped <= CAP,
        "{capped} pages pending, over the 256 kB limit"
    );

    // The list an index under the default limit is left with, merged by the
    // migration rerun.
    sqlx::query("alter index sync.item_fts_idx reset (gin_pending_list_limit)")
        .execute(&pool)
        .await
        .unwrap();
    let at_default = load(2_001).await;
    assert!(
        at_default > 4 * CAP,
        "the default limit left {at_default} pages pending, too few to tell the cap from it"
    );
    sqlx::raw_sql(include_str!(
        "../migrations/0059_the_mirror_search_keeps_a_short_pending_list.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        pending(pool.clone()).await,
        0,
        "the migration merges what was pending"
    );
    let again = load(4_001).await;
    assert!(
        again <= CAP,
        "{again} pages pending after the migration, over the 256 kB limit"
    );
}
