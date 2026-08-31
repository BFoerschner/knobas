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

    // Archive the first: the anchor is free again, which is what lets a
    // re-promotion mint a fresh room instead of resurrecting the old one.
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
