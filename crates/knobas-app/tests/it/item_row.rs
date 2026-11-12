//! `sync.item_row` (migration `0081`): what a list draws of an item of any
//! kind, as the database keeps it, held to the payload walks it replaces for
//! every field, every kind and every adapter, and kept through every kind of
//! write. The reads that take it are held to their answers without it in
//! `entity.rs` (`a_list_answers_the_same_from_its_stored_rows_and_from_the_payloads`).

use knobas_app::sources::Registry;
use serde_json::{Value, json};
use sqlx::PgPool;

/// A database of the test's own: the adapters' declarations stored as
/// bring-up stores them (ADR-0019), then the demo corpus (the reference
/// adapter's every kind) and a Gitea source's pull requests, branches and
/// repository, shaped as Gitea's records are. Returns the Gitea source.
async fn mirror(label: &str) -> (PgPool, String) {
    let pool = knobas_db::test_util::scratch_database(label)
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    knobas_app::sources::demo::demo_load_inner(&pool)
        .await
        .unwrap();
    let gitea = "gitea-rows".to_owned();
    crate::support::source_config(&pool, &gitea, "gitea", "gitea", &json!({})).await;
    let repo = |name: &str| json!({ "full_name": format!("tidewater/{name}"), "name": name, "default_branch": "main" });
    for (kind, key, payload) in [
        (
            "pr",
            "tidewater/ledger#1",
            json!({
                "state": "open", "merged": false,
                "head": { "ref": "mara/retry", "repo": repo("ledger") },
                "base": { "repo": repo("ledger") },
                "requested_reviewers": [{ "login": "jonas" }, { "login": "mara" }, { "login": "jonas" }],
            }),
        ),
        (
            "pr",
            "tidewater/ledger#2",
            json!({
                "state": "closed", "merged": true,
                "head": { "ref": "old" },
                "base": { "repo": repo("ledger") },
                "requested_reviewers": [],
            }),
        ),
        // A state no map names, and nothing else the row reads.
        ("pr", "tidewater/ledger#3", json!({ "state": "draft" })),
        (
            "branch",
            "tidewater/ledger@refs/heads/main",
            json!({ "repository": repo("ledger") }),
        ),
        ("branch", "tidewater/ledger@refs/heads/none", json!({})),
        ("repo", "tidewater/ledger", repo("ledger")),
    ] {
        write(&pool, &gitea, kind, key, payload).await;
    }
    (pool, gitea)
}

/// One mirrored item of `source`, inserted or rewritten.
async fn write(pool: &PgPool, source: &str, kind: &str, key: &str, payload: Value) -> String {
    let entity = format!("{source}:{key}");
    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ($1, $2, $3)
         on conflict (id) do nothing",
    )
    .bind(&entity)
    .bind(kind)
    .bind(key)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, $2, $3, $4, '', $5)
         on conflict (entity_id) do update set payload = excluded.payload, kind = excluded.kind",
    )
    .bind(&entity)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    entity
}

/// Every live or withdrawn item against its stored row, read as the lists
/// read it (`row_keys!`, `row_join!`) under the declarations the app binds:
/// how many items there are, how many of a declared kind have no row, and
/// how many rows hold a field that is not the payload walk's answer.
const HELD_TO_THE_WALK: &str = concat!(
    "with ",
    knobas_core::row_keys!("$1"),
    " select count(*) as items,
             count(*) filter (where rw_k.k is not null and rw.entity_id is null) as unstored,
             count(*) filter (where rw.entity_id is not null and (
                   rw.path is distinct from ",
    knobas_core::path_line_read!("$1"),
    "           or rw.state is distinct from ",
    knobas_core::declared_state!("$1"),
    "           or rw.tone is distinct from ",
    knobas_core::declared_state!("$1"),
    "->>'tone'
                or rw.head is distinct from ",
    knobas_core::declared_string!("$1", "head"),
    "           or rw.head_repo is distinct from ",
    knobas_core::declared_string!("$1", "head_repo"),
    "           or rw.default_branch is distinct from ",
    knobas_core::declared_string!("$1", "default_branch"),
    "           or rw.reviewers is distinct from array(select l.value from ",
    knobas_core::declared_list!("$1", "reviewers"),
    " l))) as differing,
             count(rw.path) as paths, count(rw.state) as states, count(rw.head) as heads,
             count(rw.head_repo) as head_repos, count(rw.default_branch) as defaults,
             count(*) filter (where cardinality(rw.reviewers) > 0) as asking
        from sync.item i",
    knobas_core::row_join!("rw", "i"),
);

#[derive(Debug, sqlx::FromRow)]
struct Held {
    items: i64,
    unstored: i64,
    differing: i64,
    paths: i64,
    states: i64,
    heads: i64,
    head_repos: i64,
    defaults: i64,
    asking: i64,
}

async fn held(pool: &PgPool) -> Held {
    let declarations = knobas_app::sources::paths::declared_paths(pool, &Registry::for_process())
        .await
        .unwrap();
    sqlx::query_as(HELD_TO_THE_WALK)
        .bind(declarations.as_param())
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Where a row is stored, so a test sees whether a write wrote it again.
async fn place(pool: &PgPool, entity: &str) -> Option<String> {
    sqlx::query_scalar("select ctid::text from sync.item_row where entity_id = $1")
        .bind(entity)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// `sync.item_row_rows` carries each macro's expansion: a stored field is
/// read by the expression the lists walk the payload with.
#[tokio::test]
async fn the_stored_rows_are_the_macros_expansions() {
    let (pool, _) = mirror("item_row_expansions").await;
    let source: String = sqlx::query_scalar(
        "select p.prosrc from pg_proc p join pg_namespace n on n.oid = p.pronamespace
          where n.nspname = 'sync' and p.proname = 'item_row_rows'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for expansion in [
        knobas_core::path_line_read!("dd.decl"),
        knobas_core::declared_state!("dd.decl"),
        knobas_core::declared_string!("dd.decl", "head"),
        knobas_core::declared_string!("dd.decl", "head_repo"),
        knobas_core::declared_string!("dd.decl", "default_branch"),
        knobas_core::declared_list!("dd.decl", "reviewers"),
    ] {
        assert!(
            source.contains(expansion),
            "sync.item_row_rows no longer carries a macro's expansion; \
             restate it in a new migration:\n{expansion}"
        );
    }
}

/// Every item of every kind has its row under the key the lists read by,
/// and every stored field is the payload walk's answer: the reference
/// adapter's whole corpus and Gitea's pull requests, branches and
/// repository, with every field held by some row, so none is compared as
/// null against null alone.
#[tokio::test]
async fn a_stored_row_is_the_walks_answer_for_every_field_kind_and_adapter() {
    let (pool, _) = mirror("item_row_walk").await;
    let held = held(&pool).await;
    assert!(held.items > 30, "the corpus is there: {held:?}");
    assert_eq!(
        held.unstored, 0,
        "every declared item has its row: {held:?}"
    );
    assert_eq!(held.differing, 0, "every field is the walk's: {held:?}");
    for (field, rows) in [
        ("path", held.paths),
        ("state", held.states),
        ("head", held.heads),
        ("head_repo", held.head_repos),
        ("default_branch", held.defaults),
        ("reviewers", held.asking),
    ] {
        assert!(rows > 0, "some row holds a {field}: {held:?}");
    }
}

/// The rows follow the mirror: a rewritten item's row is the new payload's,
/// a stamp writes none, a deleted item's row goes, a changed declaration
/// drops the rows read under it and no other, and the fill writes them back.
#[tokio::test]
async fn the_rows_follow_every_write_and_the_fill_restores_them() {
    let (pool, gitea) = mirror("item_row_writes").await;
    let pull = format!("{gitea}:tidewater/ledger#1");
    let branch = format!("{gitea}:tidewater/ledger@refs/heads/main");
    let stored = async |entity: &str| -> (Option<String>, Option<String>, Vec<String>) {
        sqlx::query_as(
            "select tone, head, array(select x from unnest(reviewers) x order by x)
               from sync.item_row where entity_id = $1",
        )
        .bind(entity)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    assert_eq!(
        stored(&pull).await,
        (
            Some("info".to_owned()),
            Some("mara/retry".to_owned()),
            vec!["jonas".to_owned(), "jonas".to_owned(), "mara".to_owned()]
        )
    );

    // A stamp: the row stays where it is.
    let before = place(&pool, &pull).await;
    sqlx::query("update sync.item set synced_at = now() where entity_id = $1")
        .bind(&pull)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(place(&pool, &pull).await, before, "a stamp writes no row");

    // A rewrite: merged, from another head, asking nobody.
    write(
        &pool,
        &gitea,
        "pr",
        "tidewater/ledger#1",
        json!({ "state": "closed", "merged": true, "head": { "ref": "mara/done" } }),
    )
    .await;
    assert_eq!(
        stored(&pull).await,
        (Some("ok".to_owned()), Some("mara/done".to_owned()), vec![])
    );
    assert_eq!(held(&pool).await.differing, 0);

    // A delete takes the row with it.
    sqlx::query("delete from sync.item where entity_id = $1")
        .bind(format!("{gitea}:tidewater/ledger#3"))
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar(
        "select count(*) from sync.item_row where entity_id like $1 || ':tidewater/ledger#3'",
    )
    .bind(&gitea)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, 0);

    // Gitea's pull requests are declared another way: their rows go, the
    // branch's and the reference adapter's stay where they are.
    let branch_place = place(&pool, &branch).await;
    let mock_rows: i64 = sqlx::query_scalar(
        "select count(*) from sync.item_row r join sync.item i using (entity_id)
          where i.source_id = 'mock'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(mock_rows > 20, "the reference adapter's rows: {mock_rows}");
    sqlx::query(
        "update sync.parent_declaration
            set payload_paths = (
              select jsonb_agg(case when kp.p->>'kind' = 'pr'
                                    then jsonb_set(kp.p, '{head}', '[[\"head\", \"label\"]]')
                                    else kp.p end order by kp.ord)
                from jsonb_array_elements(payload_paths) with ordinality as kp(p, ord))
          where adapter_kind = 'gitea'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(place(&pool, &pull).await, None, "a row under the old key");
    assert_eq!(
        place(&pool, &branch).await,
        branch_place,
        "a kind untouched"
    );
    let mock_after: i64 = sqlx::query_scalar(
        "select count(*) from sync.item_row r join sync.item i using (entity_id)
          where i.source_id = 'mock'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mock_after, mock_rows, "another adapter's rows");

    // The declarations as the app holds them again, and the fill: every row
    // back, each the walk's answer, and a second fill writes nothing.
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    assert_eq!(
        place(&pool, &pull).await,
        None,
        "nothing fills but the fill"
    );
    assert!(held(&pool).await.unstored > 0);
    let batches = knobas_core::item_row::fill(&pool).await.unwrap();
    assert!(batches > 0);
    let after = held(&pool).await;
    assert_eq!((after.unstored, after.differing), (0, 0), "{after:?}");
    let filled = place(&pool, &pull).await;
    assert!(filled.is_some());
    knobas_core::item_row::fill(&pool).await.unwrap();
    assert_eq!(
        place(&pool, &pull).await,
        filled,
        "a filled row is left alone"
    );
}
