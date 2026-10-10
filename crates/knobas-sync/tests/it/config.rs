//! The store the scheduler reads its schedule out of, and writes its verdicts
//! back into. Every test seeds its own uniquely-named source: `test_pool()`
//! hands out one shared database per test file, so truncating is not an
//! option (`knobas_db::test_util` docs).

use chrono::{Duration as ChronoDuration, Utc};
use knobas_source::AuthMethod;
use knobas_sync::config::{self, AuthKind, AuthState, InsertConfig, PatchConfig};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn a_source(id: &str) -> InsertConfig {
    InsertConfig {
        id: id.to_owned(),
        adapter_kind: "mock".to_owned(),
        display_name: "Test source".to_owned(),
        base_url: "https://example.invalid".to_owned(),
        auth_kind: AuthKind::Method(AuthMethod::Pat),
        config: serde_json::json!({ "flavor": "datacenter" }),
        sync_interval_secs: 300,
        enabled: true,
    }
}

fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4().simple())
}

#[tokio::test]
async fn a_new_source_starts_unknown_with_no_backoff_and_is_due_at_once() {
    let pool = pool().await;
    let id = unique("cfg");
    let row = config::insert(&pool, &a_source(&id)).await.unwrap();

    assert_eq!(row.health.state, AuthState::Unknown);
    assert!(row.health.checked_at.is_none());
    assert!(row.backoff_until.is_none());
    assert!(row.cursor.is_none());
    assert_eq!(row.config["flavor"], "datacenter");
    assert_eq!(row.auth_kind, AuthKind::Method(AuthMethod::Pat));
    assert_eq!(row.adapter_kind, "mock");
    assert_eq!(row.sync_interval_secs, 300);

    // Never run before: due now, and the run that follows is the first one.
    let due = config::due(&pool).await.unwrap();
    let mine = due
        .iter()
        .find(|d| d.id == id)
        .expect("a never-run source is due");
    assert!(
        mine.first_run,
        "a source with no finished run is a first_run trigger"
    );
}

#[tokio::test]
async fn patch_changes_what_the_user_owns_and_nothing_else() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    let patched = config::patch(
        &pool,
        &id,
        &PatchConfig {
            display_name: Some("Renamed".to_owned()),
            base_url: None,
            config: Some(serde_json::json!({ "flavor": "cloud" })),
            sync_interval_secs: Some(900),
            enabled: Some(false),
        },
    )
    .await
    .unwrap()
    .expect("the source exists");

    assert_eq!(patched.display_name, "Renamed");
    assert_eq!(
        patched.base_url, "https://example.invalid",
        "an absent field is left alone"
    );
    assert_eq!(patched.config["flavor"], "cloud");
    assert_eq!(patched.sync_interval_secs, 900);
    assert!(!patched.enabled);
    // The id is the entity namespace and is immutable (P10) -- PatchConfig has
    // no field for it, which is the point; assert the row kept it.
    assert_eq!(patched.id, id);
    // A disabled source is never due.
    assert!(config::due(&pool).await.unwrap().iter().all(|d| d.id != id));

    // An empty patch is a no-op, not a wipe: every field coalesces to itself.
    let untouched = config::patch(&pool, &id, &PatchConfig::default())
        .await
        .unwrap()
        .expect("the source exists");
    assert_eq!(untouched.display_name, "Renamed");
    assert_eq!(untouched.config["flavor"], "cloud");
    assert_eq!(untouched.sync_interval_secs, 900);
    assert!(!untouched.enabled);

    assert!(
        config::patch(&pool, "no-such-source", &PatchConfig::default())
            .await
            .unwrap()
            .is_none(),
        "patching an unknown source reports absence rather than inventing a row"
    );
}

#[tokio::test]
async fn health_is_written_every_time_and_reported_as_changed_only_when_it_moved() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    let (health, changed) =
        config::set_health(&pool, &id, AuthState::Unauthorized, Some("401"), None)
            .await
            .unwrap()
            .expect("the source exists");
    assert!(changed, "unknown -> unauthorized is a change");
    assert_eq!(health.state, AuthState::Unauthorized);
    assert_eq!(health.detail.as_deref(), Some("401"));
    assert_eq!(health.source_id, id);
    let first_check = health.checked_at.expect("checked_at is stamped");

    // Same verdict again: no event, but the freshness stamp still moves, which
    // is what the top strip's "checked N min ago" reads.
    let (health, changed) =
        config::set_health(&pool, &id, AuthState::Unauthorized, Some("401"), None)
            .await
            .unwrap()
            .unwrap();
    assert!(!changed, "an unchanged verdict must not fire source:health");
    // Strictly greater, not `>=`: `>=` is satisfied by a statement that never
    // touched `auth_checked_at` at all, so it would pass with the freshness
    // stamp deleted. Each call is its own transaction, so `now()` really does
    // move between them.
    assert!(
        health.checked_at.unwrap() > first_check,
        "the freshness stamp must move even when the verdict did not: \
         {:?} vs {first_check:?}",
        health.checked_at
    );

    // The *detail* moving is a change too: same state, new message from the
    // server is what the sources view renders.
    let (_, changed) = config::set_health(
        &pool,
        &id,
        AuthState::Unauthorized,
        Some("401: token revoked"),
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert!(changed, "a new detail under the same state is a change");

    let (_, changed) = config::set_health(&pool, &id, AuthState::Ok, None, None)
        .await
        .unwrap()
        .unwrap();
    assert!(changed, "recovering is a change");

    assert!(
        config::set_health(&pool, "no-such-source", AuthState::Ok, None, None)
            .await
            .unwrap()
            .is_none(),
        "an unknown source reports absence rather than inventing a row"
    );
}

/// A connection test that learned nothing about expiry must not erase what an
/// earlier one learned -- the PAT countdown in §3 would blink out on the next
/// ordinary sync.
#[tokio::test]
async fn a_verdict_that_says_nothing_about_expiry_keeps_what_was_known() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    let expires = Utc::now() + ChronoDuration::days(30);
    let (health, _) = config::set_health(&pool, &id, AuthState::Ok, None, Some(expires))
        .await
        .unwrap()
        .unwrap();
    assert!(health.secret_expires_at.is_some());

    let (health, _) = config::set_health(&pool, &id, AuthState::Ok, None, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        health.secret_expires_at.map(|t| t.timestamp_millis()),
        Some(expires.timestamp_millis()),
        "a later verdict with no expiry must not erase the countdown"
    );
}

/// Every `AuthState` this code can produce must satisfy 0002's CHECK
/// constraint. A variant whose stored spelling drifts from the constraint
/// fails at *write* time, in production, on the one path that reports failures.
///
/// Driven by `AuthState::ALL` rather than a hand-written list, so a variant
/// cannot be added without reaching this test.
#[tokio::test]
async fn every_auth_state_is_accepted_by_the_check_constraint() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();
    for state in AuthState::ALL {
        let (health, _) = config::set_health(&pool, &id, *state, None, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            health.state, *state,
            "{state:?} must round-trip through the column"
        );
    }
}

/// P7: the ladder, and the clamp at its top. A source that has been down for a
/// week is retried hourly, not every 60 minutes times 2^n seconds.
#[test]
fn the_backoff_ladder_is_one_two_five_fifteen_sixty_minutes_and_then_stays_there() {
    let mins = |n| config::backoff_after(n).as_secs() / 60;
    assert_eq!(mins(1), 1);
    assert_eq!(mins(2), 2);
    assert_eq!(mins(3), 5);
    assert_eq!(mins(4), 15);
    assert_eq!(mins(5), 60);
    assert_eq!(mins(6), 60);
    assert_eq!(mins(500), 60);
    // Defensive: a caller that computed zero failures still gets the first
    // rung rather than an instant retry loop.
    assert_eq!(mins(0), 1);
    // And a negative one -- `failures_since_last_ok` returns a count, but the
    // ladder must not be indexable out of the bottom of the array by a caller
    // that got its arithmetic wrong.
    assert_eq!(mins(-3), 1);

    // The rungs are seconds, not minutes-as-seconds: the ladder is read
    // straight into a `Duration`, so a factor of 60 here is a source hammered
    // once a second.
    assert_eq!(config::backoff_after(1).as_secs(), 60);
    assert_eq!(
        config::BACKOFF_LADDER_SECS,
        [60, 120, 300, 900, 3600],
        "P7's ladder is 1 -> 2 -> 5 -> 15 -> 60 minutes"
    );
}

#[tokio::test]
async fn a_backed_off_source_is_not_due_until_the_backoff_expires_and_survives_a_restart() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    config::set_backoff(&pool, &id, Utc::now() + ChronoDuration::minutes(5))
        .await
        .unwrap();
    assert!(config::due(&pool).await.unwrap().iter().all(|d| d.id != id));
    // Persisted, not in-memory: re-reading the row is what a restart does.
    let row = config::get(&pool, &id).await.unwrap().unwrap();
    assert!(row.backoff_until.unwrap() > Utc::now());

    config::set_backoff(&pool, &id, Utc::now() - ChronoDuration::seconds(1))
        .await
        .unwrap();
    assert!(config::due(&pool).await.unwrap().iter().any(|d| d.id == id));

    config::clear_backoff(&pool, &id).await.unwrap();
    assert!(
        config::get(&pool, &id)
            .await
            .unwrap()
            .unwrap()
            .backoff_until
            .is_none()
    );
    assert!(config::due(&pool).await.unwrap().iter().any(|d| d.id == id));
}

/// P7 again, from the other side: `unauthorized` and `missing_secret` need a
/// human, so the scheduler must not pick them up at all -- no request, no
/// backoff churn (interfaces §3, "Missing").
#[tokio::test]
async fn a_source_needing_a_human_is_never_due() {
    let pool = pool().await;
    for state in [AuthState::Unauthorized, AuthState::MissingSecret] {
        let id = unique("cfg");
        config::insert(&pool, &a_source(&id)).await.unwrap();
        config::set_health(&pool, &id, state, None, None)
            .await
            .unwrap();
        assert!(
            config::due(&pool).await.unwrap().iter().all(|d| d.id != id),
            "{state:?} must not be scheduled"
        );
    }
    // …while the three that do not need a human still are.
    for state in [AuthState::Ok, AuthState::Unreachable, AuthState::Unknown] {
        let id = unique("cfg");
        config::insert(&pool, &a_source(&id)).await.unwrap();
        config::set_health(&pool, &id, state, None, None)
            .await
            .unwrap();
        assert!(
            config::due(&pool).await.unwrap().iter().any(|d| d.id == id),
            "{state:?} is retryable and must stay scheduled"
        );
    }
}

/// The interval means "seconds after the previous run *finished*" (P7), so a
/// source that has just finished a run is not due again until it elapses --
/// and `next_run_at` is derived from the log rather than stored.
#[tokio::test]
async fn a_source_that_just_finished_a_run_waits_out_its_interval() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    // A run that finished a moment ago: 300s interval, so not due.
    sqlx::query(
        "insert into knobas.sync_run (source_id, trigger, finished_at, outcome)
         values ($1, 'schedule', now(), 'ok')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(config::due(&pool).await.unwrap().iter().all(|d| d.id != id));

    // One that finished longer ago than the interval: due again, and no longer
    // a first run.
    sqlx::query(
        "insert into knobas.sync_run (source_id, trigger, finished_at, outcome)
         values ($1, 'schedule', now() - interval '1 hour', 'ok')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        config::due(&pool).await.unwrap().iter().all(|d| d.id != id),
        "the *newest* finished run decides, not the oldest"
    );

    sqlx::query(
        "update knobas.sync_run set finished_at = now() - interval '1 hour' where source_id = $1",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    let due = config::due(&pool).await.unwrap();
    let mine = due.iter().find(|d| d.id == id).expect("due again");
    assert!(
        !mine.first_run,
        "a source with a finished run is not a first run"
    );
}

/// A run still in flight has no `finished_at`, so it must not make a source
/// look never-run: the scheduler would start a second run of something already
/// running and call it a first sync.
#[tokio::test]
async fn a_run_still_in_flight_does_not_reset_the_schedule() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();
    sqlx::query(
        "insert into knobas.sync_run (source_id, trigger, finished_at, outcome)
         values ($1, 'schedule', now() - interval '1 hour', 'ok')",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    // …and an open run on top of it.
    sqlx::query("insert into knobas.sync_run (source_id, trigger) values ($1, 'manual')")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();

    let due = config::due(&pool).await.unwrap();
    let mine = due.iter().find(|d| d.id == id).expect("still due");
    assert!(
        !mine.first_run,
        "an open run must not make a synced source look never-run"
    );
}

#[test]
fn an_interval_below_the_floor_is_refused_rather_than_hammering_a_source() {
    assert_eq!(config::check_interval(300).unwrap(), 300);
    assert_eq!(
        config::check_interval(config::MIN_SYNC_INTERVAL_SECS).unwrap(),
        60
    );
    assert!(config::check_interval(config::MIN_SYNC_INTERVAL_SECS - 1).is_err());
    assert!(config::check_interval(1).is_err());
    assert!(config::check_interval(0).is_err());
    assert_eq!(config::MIN_SYNC_INTERVAL_SECS, 60);
}

#[tokio::test]
async fn deleting_a_source_can_purge_its_mirror_while_leaving_its_entities_addressable() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    let entity = format!("{id}:PAY-1");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'kept')")
        .bind(&entity)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, $2, 'ticket', 'kept', '{}'::jsonb)",
    )
    .bind(&entity)
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();

    assert!(config::delete(&pool, &id, true).await.unwrap());
    assert!(config::get(&pool, &id).await.unwrap().is_none());

    let (items,): (i64,) = sqlx::query_as("select count(*) from sync.item where source_id = $1")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(items, 0, "purge_items removes the mirror");

    // The entity survives, tombstoned: links, notes and activity point at it.
    let (deleted_at,): (Option<chrono::DateTime<Utc>>,) =
        sqlx::query_as("select deleted_at from knobas.entity where id = $1")
            .bind(&entity)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        deleted_at.is_some(),
        "a purged source's entities are tombstoned, not dropped"
    );

    assert!(
        !config::delete(&pool, &id, false).await.unwrap(),
        "deleting twice reports absence"
    );
}

/// Without `purge_items` the mirror stays exactly as it was: *Remove source,
/// keep items* is a real choice in the sources view (interfaces §3, Delete).
#[tokio::test]
async fn deleting_a_source_without_purging_leaves_its_items_live() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    let entity = format!("{id}:PAY-2");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'kept')")
        .bind(&entity)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, payload)
         values ($1, $2, 'ticket', 'kept', '{}'::jsonb)",
    )
    .bind(&entity)
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();

    assert!(config::delete(&pool, &id, false).await.unwrap());

    let (live,): (i64,) =
        sqlx::query_as("select count(*) from sync.live_item where source_id = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(live, 1, "keeping the items means they are still live");
}

/// The sync-run history is deliberately kept: there is no FK, and deleting a
/// source must not rewrite what happened (interfaces §1).
#[tokio::test]
async fn deleting_a_source_keeps_its_run_history() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();
    sqlx::query("insert into knobas.sync_run (source_id, trigger) values ($1, 'manual')")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(config::delete(&pool, &id, true).await.unwrap());

    let (runs,): (i64,) =
        sqlx::query_as("select count(*) from knobas.sync_run where source_id = $1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(runs, 1, "deleting a source must not rewrite its history");
}

/// `list` is the sources view's read, and it must not lose the parsed enums or
/// the health block on the way through.
#[tokio::test]
async fn list_and_get_agree_on_what_a_row_is() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();
    config::set_health(&pool, &id, AuthState::Unreachable, Some("dns"), None)
        .await
        .unwrap();

    let listed = config::list(&pool).await.unwrap();
    let mine = listed.iter().find(|r| r.id == id).expect("listed");
    let got = config::get(&pool, &id).await.unwrap().unwrap();

    assert_eq!(mine.health.state, AuthState::Unreachable);
    assert_eq!(got.health.state, AuthState::Unreachable);
    assert_eq!(mine.health.detail.as_deref(), Some("dns"));
    assert_eq!(mine.auth_kind, got.auth_kind);
    assert_eq!(mine.display_name, got.display_name);
    assert!(
        config::get(&pool, "no-such-source")
            .await
            .unwrap()
            .is_none()
    );
}

/// Every `AuthKind` must survive the round trip through `auth_kind`, and the
/// mock's credential-free shape must stay expressible.
#[tokio::test]
async fn every_auth_kind_round_trips_through_the_column() {
    let pool = pool().await;
    for kind in [
        AuthKind::None,
        AuthKind::Method(AuthMethod::UserPassword),
        AuthKind::Method(AuthMethod::Pat),
        AuthKind::Method(AuthMethod::ApiToken),
        AuthKind::Method(AuthMethod::OAuth),
    ] {
        let id = unique("cfg");
        let row = config::insert(
            &pool,
            &InsertConfig {
                auth_kind: kind,
                ..a_source(&id)
            },
        )
        .await
        .unwrap();
        assert_eq!(row.auth_kind, kind, "{kind:?} must round-trip");
        assert_eq!(row.auth_kind.method(), kind.method());
    }
}

/// The cheap poll the top strip's monograms use.
#[tokio::test]
async fn health_all_reports_every_configured_source() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();
    config::set_health(&pool, &id, AuthState::Ok, Some("fine"), None)
        .await
        .unwrap();

    let all = config::health_all(&pool).await.unwrap();
    let mine = all.iter().find(|h| h.source_id == id).expect("reported");
    assert_eq!(mine.state, AuthState::Ok);
    assert_eq!(mine.detail.as_deref(), Some("fine"));
    assert!(mine.checked_at.is_some());
}

/// The id is the primary key and the entity namespace: a second source under
/// the same id would write into the first one's world. Surfaced as a
/// unique-violation for the IPC layer to map to `Conflict`.
#[tokio::test]
async fn a_duplicate_id_is_refused_by_the_database() {
    let pool = pool().await;
    let id = unique("cfg");
    config::insert(&pool, &a_source(&id)).await.unwrap();

    let again = config::insert(&pool, &a_source(&id)).await;
    let error = again.expect_err("the id is taken");
    assert!(
        error
            .as_database_error()
            .is_some_and(|db| db.is_unique_violation()),
        "expected a unique violation, got {error:?}"
    );
}
