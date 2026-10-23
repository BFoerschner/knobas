//! The builds of a commit, read from `sync.item_revision` (#732, migration
//! `0030`), against the walk they replace, on a real PostgreSQL.
//!
//! `DETAIL_BUILDS_AT` used to walk every live item through the declared
//! `revision` paths (#710). Since #732 the database runs that walk when an
//! item or a declaration changes and keeps the answer, and the detail asks an
//! index. This file holds the detail's builds to the walk -- the #710
//! statement, kept here as the oracle -- after every change the trigger code
//! claims to follow: an insert, a payload update, a tombstone, a re-sync in the
//! writer's shape, a source given another adapter kind, a change to the stored
//! declarations, and a delete.
//!
//! `sync.parent_declaration` is one row per adapter kind, and every test's
//! [`pool`] writes it back, so the test that rewrites it takes a database of
//! its own ([`scratch`], #1907). The perf reading is
//! `builds_of_a_commit_perf.rs`, in a database of its own, so a declaration
//! change here never recomputes under its timing.

use knobas_app::commands::entity::get_entity_inner;
use knobas_app::sources::Registry;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    declared(pool).await
}

async fn scratch(label: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database");
    declared(pool).await
}

/// The declarations' stored copy, written as bring-up writes it (ADR-0019).
async fn declared(pool: PgPool) -> PgPool {
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    pool
}

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// A full object id no other test writes.
fn sha() -> String {
    format!("{}{}", unique(), &unique()[..8])
}

async fn configured(pool: &PgPool, adapter: &str) -> String {
    let source = format!("{adapter}-{}", &unique()[..12]);
    crate::support::source_config(pool, &source, adapter, adapter, &serde_json::json!({})).await;
    source
}

/// Write `rows` -- `(kind, key, payload)` of `source` -- in the sync writer's
/// shape: one multi-row `insert ... on conflict (entity_id) do update`, so a
/// batch reaches the statement-level triggers together, and a row already
/// there is an update.
async fn upsert(pool: &PgPool, source: &str, rows: &[(&str, &str, serde_json::Value)]) {
    let ids: Vec<String> = rows
        .iter()
        .map(|(_, k, _)| format!("{source}:{k}"))
        .collect();
    let kinds: Vec<&str> = rows.iter().map(|(kind, _, _)| *kind).collect();
    let titles: Vec<&str> = rows.iter().map(|(_, k, _)| *k).collect();
    let payloads: Vec<&serde_json::Value> = rows.iter().map(|(_, _, p)| p).collect();
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select * from unnest($1::text[], $2::text[], $3::text[])
             on conflict (id) do nothing",
    )
    .bind(&ids)
    .bind(&kinds)
    .bind(&titles)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         select t.id, $5, t.kind, t.title, '', t.payload
           from unnest($1::text[], $2::text[], $3::text[], $4::jsonb[])
                as t(id, kind, title, payload)
             on conflict (entity_id) do update set
                source_id = excluded.source_id,
                kind      = excluded.kind,
                title     = excluded.title,
                payload   = excluded.payload",
    )
    .bind(&ids)
    .bind(&kinds)
    .bind(&titles)
    .bind(&payloads)
    .bind(source)
    .execute(pool)
    .await
    .unwrap();
}

/// A TeamCity build's payload: its state, and one revision per VCS root.
fn ran_on(shas: &[&str], status: &str) -> serde_json::Value {
    let revisions: Vec<_> = shas
        .iter()
        .map(|s| serde_json::json!({ "version": s }))
        .collect();
    serde_json::json!({ "state": "finished", "status": status, "revisions": { "revision": revisions } })
}

/// The #710 walk, verbatim: every live item through the declared `revision`
/// paths, bound as a parameter. The oracle the stored rows are held to.
const WALK: &str = concat!(
    "select i.entity_id from sync.live_item i where ",
    knobas_core::declares_field!("$2", "revision"),
    " and exists (select 1 from ",
    knobas_core::declared_list!("$2", "revision"),
    " r where r.value = $1) order by 1"
);

async fn walked(pool: &PgPool, sha: &str) -> Vec<String> {
    let declarations = knobas_app::sources::paths::declared_paths(pool, &Registry::for_process())
        .await
        .unwrap();
    sqlx::query_scalar(WALK)
        .bind(sha)
        .bind(declarations.as_param())
        .fetch_all(pool)
        .await
        .unwrap()
}

/// The builds the detail of the commit `sha` draws, sorted.
async fn drawn(pool: &PgPool, commit: &str, sha: &str) -> Vec<String> {
    let detail = get_entity_inner(pool, &Registry::for_process(), &format!("{commit}{sha}"))
        .await
        .unwrap();
    let mut ids: Vec<String> = detail.builds.into_iter().map(|r| r.entity_id).collect();
    ids.sort();
    ids
}

/// The builds the first commit's detail draws, after checking every
/// commit's are the walk's.
async fn agree(pool: &PgPool, commit: &str, shas: &[&str], step: &str) -> Vec<String> {
    for sha in shas {
        assert_eq!(
            drawn(pool, commit, sha).await,
            walked(pool, sha).await,
            "after {step}: the stored revisions are not the walk for {sha}"
        );
    }
    drawn(pool, commit, shas[0]).await
}

/// How many rows `sync.item_revision` holds for one item: a re-sync must not
/// add a second copy of a commit.
async fn stored_count(pool: &PgPool, entity: &str) -> i64 {
    sqlx::query_scalar("select count(*) from sync.item_revision where entity_id = $1")
        .bind(entity)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Every change the trigger code follows, in turn, each followed by the
/// comparison with the walk. One test: the steps build on each other.
#[tokio::test]
async fn a_commits_builds_are_the_walk_after_every_change() {
    let pool = scratch("builds_of_a_commit_walk").await;
    let gitea = configured(&pool, "gitea").await;
    let teamcity = configured(&pool, "teamcity").await;
    let (a, b) = (sha(), sha());
    let shas = [a.as_str(), b.as_str()];
    let repo = "tidewater/payout-service@";
    upsert(
        &pool,
        &gitea,
        &[
            (
                "commit",
                &format!("{repo}{a}"),
                serde_json::json!({ "sha": a }),
            ),
            (
                "commit",
                &format!("{repo}{b}"),
                serde_json::json!({ "sha": b }),
            ),
        ],
    )
    .await;
    let commit = format!("{gitea}:{repo}");
    let id = |key: &str| format!("{teamcity}:{key}");

    // One batch: a build on `a`, one on both (two VCS roots), one on `b`, and
    // one of two roots at the same commit, stored once.
    upsert(
        &pool,
        &teamcity,
        &[
            ("build", "1", ran_on(&[&a], "SUCCESS")),
            ("build", "2", ran_on(&[&b, &a], "FAILURE")),
            ("build", "3", ran_on(&[&b], "SUCCESS")),
            ("build", "4", ran_on(&[&a, &a], "SUCCESS")),
        ],
    )
    .await;
    assert_eq!(
        agree(&pool, &commit, &shas, "the inserts").await,
        [id("1"), id("2"), id("4")]
    );
    assert_eq!(stored_count(&pool, &id("4")).await, 1, "one row per commit");

    // A payload update moves build 1 from `a` to `b`.
    upsert(
        &pool,
        &teamcity,
        &[("build", "1", ran_on(&[&b], "SUCCESS"))],
    )
    .await;
    assert_eq!(
        agree(&pool, &commit, &shas, "a payload update").await,
        [id("2"), id("4")]
    );

    // A tombstone: the mirror row stays, the build is not drawn.
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(id("4"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(agree(&pool, &commit, &shas, "a tombstone").await, [id("2")]);

    // A re-sync in the writer's shape, every payload unchanged: nothing is
    // added twice and nothing is lost.
    upsert(
        &pool,
        &teamcity,
        &[
            ("build", "1", ran_on(&[&b], "SUCCESS")),
            ("build", "2", ran_on(&[&b, &a], "FAILURE")),
            ("build", "3", ran_on(&[&b], "SUCCESS")),
            ("build", "4", ran_on(&[&a, &a], "SUCCESS")),
        ],
    )
    .await;
    assert_eq!(agree(&pool, &commit, &shas, "a re-sync").await, [id("2")]);
    assert_eq!(stored_count(&pool, &id("2")).await, 2);

    // The tombstone lifted, as the next sync that sees the build again does.
    sqlx::query("update knobas.entity set deleted_at = null where id = $1")
        .bind(id("4"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        agree(&pool, &commit, &shas, "the tombstone lifted").await,
        [id("2"), id("4")]
    );

    // The source pointed at an adapter that declares no revision: its rows
    // are matched at read time, so nothing is drawn and nothing recomputed.
    for (kind, step, want) in [
        ("jira", "a source's adapter kind change", vec![]),
        (
            "teamcity",
            "the adapter kind changed back",
            vec![id("2"), id("4")],
        ),
    ] {
        sqlx::query("update knobas.source_config set kind = $2 where id = $1")
            .bind(&teamcity)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(agree(&pool, &commit, &shas, step).await, want);
    }

    // The stored declaration loses `revision`: every build's rows go. Put
    // back, they return. (The walk reads the compiled-in declarations, so
    // this step is held to its own expectation, not to the walk.)
    sqlx::query(
        "update sync.parent_declaration
            set payload_paths = (select jsonb_agg(p - 'revision') from jsonb_array_elements(payload_paths) p)
          where adapter_kind = 'teamcity'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        stored_count(&pool, &id("2")).await,
        0,
        "a declaration change"
    );
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    assert_eq!(
        agree(&pool, &commit, &shas, "the declaration restored").await,
        [id("2"), id("4")]
    );

    // A delete cascades.
    sqlx::query("delete from knobas.entity where id = $1")
        .bind(id("2"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(agree(&pool, &commit, &shas, "a delete").await, [id("4")]);
    assert_eq!(stored_count(&pool, &id("2")).await, 0);
}

/// The resolver carries `declared_list!`'s expansion as text, because a
/// migration is SQL and cannot call it. This holds that text to what the
/// macro expands to **now**: change the macro and this fails until a new
/// migration restates `sync.resolve_item_revisions`.
#[tokio::test]
async fn the_stored_resolver_is_the_macros_expansion() {
    let pool = pool().await;
    let source: String = sqlx::query_scalar(
        "select p.prosrc from pg_proc p join pg_namespace n on n.oid = p.pronamespace
          where n.nspname = 'sync' and p.proname = 'resolve_item_revisions'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let expansion = knobas_core::declared_list!("dd.decl", "revision");
    assert!(
        source.contains(expansion),
        "sync.resolve_item_revisions no longer carries declared_list!'s expansion; \
         restate it in a new migration:\n{expansion}"
    );
}
