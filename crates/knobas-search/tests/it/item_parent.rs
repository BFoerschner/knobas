//! `sync.item_parent` against the walk it stores, on a real PostgreSQL
//! (migration `0028`, #627, ADR-0019).
//!
//! `under:` used to walk every live item through the declarations on every
//! keystroke. Since #627 the database runs that walk when something it
//! depends on changes and keeps the answer, and the readers ask an index. This
//! file holds the stored answer to the walk -- `knobas_core`'s
//! `declared_parent_names!` and `declared_names_as_parent!`, bound to the
//! declarations `Vocabulary::declaring` builds in Rust -- after every kind of
//! change the trigger code claims to follow: an insert, a payload update, a
//! delete, a change to the stored declarations, and a source configured after
//! its items were written.
//!
//! Its own database (one per test module, #572), so it may write
//! `sync.parent_declaration` freely; nothing here goes through
//! `Searcher::declaring`, which writes that table whole and would make this
//! file's two adapter kinds the only ones.
//!
//! The seeds are plain `insert`s, as every search test's are: nothing here
//! calls a fill function, because the claim is that nothing has to.

use knobas_core::payload::KindPaths;
use knobas_search::{KindCatalog, Vocabulary};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// What a source of the chain adapter declares: a page's parent is the whole
/// `ancestors` array -- `at`'s array shape -- each answering to its `title`.
fn chain_paths() -> Vec<KindPaths> {
    serde_json::from_value(serde_json::json!([{
        "kind": "page",
        "parent": [{ "kind": "page", "at": ["ancestors"], "names": [["title"]] }]
    }]))
    .unwrap()
}

/// What Gitea declares: `at`'s object shape.
fn gitea_paths() -> Vec<KindPaths> {
    knobas_source_gitea::descriptor_template().payload_paths
}

async fn store_declaration(pool: &PgPool, adapter_kind: &str, paths: &[KindPaths]) {
    sqlx::query(
        "insert into sync.parent_declaration (adapter_kind, payload_paths) values ($1, $2)
         on conflict (adapter_kind) do update set payload_paths = excluded.payload_paths",
    )
    .bind(adapter_kind)
    .bind(sqlx::types::Json(paths))
    .execute(pool)
    .await
    .unwrap();
}

async fn configure(pool: &PgPool, id: &str, adapter_kind: &str) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, $2, $1, 'http://x', 'Pat')",
    )
    .bind(id)
    .bind(adapter_kind)
    .execute(pool)
    .await
    .unwrap();
}

async fn item(pool: &PgPool, source: &str, kind: &str, key: &str, payload: serde_json::Value) {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
}

type Rows = Vec<(String, String, String)>;

/// What the database stored for this test's items.
async fn stored(pool: &PgPool, prefix: &str) -> Rows {
    sqlx::query_as(
        "select entity_id, name, kind from sync.item_parent
          where entity_id like $1 || '%' order by 1, 2, 3",
    )
    .bind(prefix)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The walk `under:` made before `0028`: the two payload arms of
/// `under::parent_names!`, per item, through the declarations the searcher
/// binds -- built in Rust by `Vocabulary::declaring` from the configured
/// sources and the adapters' paths, never by the database's own mapping.
async fn walked(pool: &PgPool, prefix: &str, templates: &[(String, Vec<KindPaths>)]) -> Rows {
    const WALK: &str = concat!(
        "select i.entity_id, n.name, n.kind from sync.item i cross join lateral (",
        "select pn.name, pn.kind from ",
        knobas_core::declared_parent_names!("$1"),
        " pn union all select sn.name, i.kind from ",
        knobas_core::declared_names_as_parent!("$1"),
        " sn) n where i.entity_id like $2 || '%' order by 1, 2, 3"
    );
    let vocab = Vocabulary::load(pool, KindCatalog::default())
        .await
        .unwrap()
        .declaring(templates);
    sqlx::query_as(WALK)
        .bind(vocab.declarations.as_param())
        .bind(prefix)
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn agree(
    pool: &PgPool,
    prefix: &str,
    templates: &[(String, Vec<KindPaths>)],
    step: &str,
) -> Rows {
    let stored = stored(pool, prefix).await;
    let walked = walked(pool, prefix, templates).await;
    assert_eq!(
        stored, walked,
        "after {step}: sync.item_parent is not the walk"
    );
    stored
}

fn has(rows: &Rows, entity: &str, name: &str, kind: &str) -> bool {
    rows.iter()
        .any(|(e, n, k)| e == entity && n == name && k == kind)
}

/// Every change the trigger code follows, in turn, each followed by the
/// comparison. One test and not five: the steps build on each other, and the
/// last one -- a source configured after its items -- needs items that were
/// written while it was not.
#[tokio::test]
async fn the_stored_parents_are_the_walk_after_every_change() {
    let pool = pool().await;
    let t = format!("p{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let gitea = format!("gitea-{t}");
    let chain = format!("chain-{t}");
    let (g, c) = (format!("g{t}"), format!("c{t}"));
    let prefix = |source: &str| format!("{source}:");

    store_declaration(&pool, &gitea, &gitea_paths()).await;
    store_declaration(&pool, &chain, &chain_paths()).await;
    let mut templates = vec![
        (gitea.clone(), gitea_paths()),
        (chain.clone(), chain_paths()),
    ];
    configure(&pool, &g, &gitea).await;

    // A commit naming a repo only its children name (never mirrored), a PR
    // naming a mirrored repo through `base.repo`, and that repo.
    item(
        &pool,
        &g,
        "commit",
        "c1",
        serde_json::json!({
        "repository": { "full_name": "tide/ghost", "name": "ghost" } }),
    )
    .await;
    item(
        &pool,
        &g,
        "pr",
        "p1",
        serde_json::json!({
        "base": { "repo": { "full_name": "tide/payout", "name": "payout" } } }),
    )
    .await;
    item(
        &pool,
        &g,
        "repo",
        "r1",
        serde_json::json!({
        "full_name": "tide/payout", "name": "payout" }),
    )
    .await;
    // Pages of a source nobody has configured yet: a chain of two ancestors,
    // and the inner ancestor itself, mirrored.
    item(
        &pool,
        &c,
        "page",
        "leaf",
        serde_json::json!({
        "ancestors": [{ "title": "Handbook" }, { "title": "Payments" }] }),
    )
    .await;
    item(
        &pool,
        &c,
        "page",
        "payments",
        serde_json::json!({ "title": "Payments" }),
    )
    .await;
    // The estate is read off its column, not stored; it is here so the
    // fixture holds every arm `under:` reads.
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'asset', $2)")
        .bind(format!("asset:{t}"))
        .bind(format!("vm-{t}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.asset (id, type_id, name, path_text) values ($1,'vm',$2,'')")
        .bind(format!("asset:{t}"))
        .bind(format!("vm-{t}"))
        .execute(&pool)
        .await
        .unwrap();

    let rows = agree(&pool, &prefix(&g), &templates, "the inserts").await;
    let (c1, p1, r1) = (format!("{g}:c1"), format!("{g}:p1"), format!("{g}:r1"));
    assert!(
        has(&rows, &c1, "tide/ghost", "repo"),
        "an unmirrored parent: {rows:?}"
    );
    assert!(
        has(&rows, &p1, "payout", "repo"),
        "the PR's parent: {rows:?}"
    );
    assert!(
        has(&rows, &r1, "tide/payout", "repo"),
        "a mirrored parent's own name: {rows:?}"
    );
    assert!(
        stored(&pool, &prefix(&c)).await.is_empty(),
        "an unconfigured source declares nothing"
    );

    sqlx::query(
        "update sync.item set payload = jsonb_set(payload, '{repository,full_name}', '\"tide/renamed\"')
          where entity_id = $1",
    )
    .bind(&c1)
    .execute(&pool)
    .await
    .unwrap();
    let rows = agree(&pool, &prefix(&g), &templates, "a payload update").await;
    assert!(has(&rows, &c1, "tide/renamed", "repo") && !has(&rows, &c1, "tide/ghost", "repo"));

    sqlx::query("delete from knobas.entity where id = $1")
        .bind(&p1)
        .execute(&pool)
        .await
        .unwrap();
    let rows = agree(&pool, &prefix(&g), &templates, "a delete").await;
    assert!(
        !rows.iter().any(|(e, _, _)| *e == p1),
        "the deleted PR's rows go with it"
    );

    // The declaration loses its bare-name path: every Gitea item answers to
    // its full name only.
    let mut narrower = gitea_paths();
    for kind in &mut narrower {
        for parent in &mut kind.parent {
            parent.names.truncate(1);
        }
    }
    store_declaration(&pool, &gitea, &narrower).await;
    templates[0].1 = narrower;
    let rows = agree(&pool, &prefix(&g), &templates, "a declaration change").await;
    assert!(
        !has(&rows, &r1, "payout", "repo"),
        "the bare name went with its path: {rows:?}"
    );
    assert!(has(&rows, &r1, "tide/payout", "repo"));

    configure(&pool, &c, &chain).await;
    let rows = agree(&pool, &prefix(&c), &templates, "configuring the source").await;
    let leaf = format!("{c}:leaf");
    assert!(has(&rows, &leaf, "Handbook", "page") && has(&rows, &leaf, "Payments", "page"));
    assert!(
        has(&rows, &format!("{c}:payments"), "Payments", "page"),
        "{rows:?}"
    );
}

/// The trigger function carries the macros' expansion as text, because a
/// migration is SQL and cannot call them. This holds that text to what the
/// macros expand to **now**: change either macro and this fails until a new
/// migration restates `sync.resolve_item_parents` -- so the stored resolution
/// and the path line, which still calls the macros per row, cannot drift apart.
#[tokio::test]
async fn the_stored_function_is_the_macros_expansion() {
    let pool = pool().await;
    let source: String = sqlx::query_scalar(
        "select p.prosrc from pg_proc p join pg_namespace n on n.oid = p.pronamespace
          where n.nspname = 'sync' and p.proname = 'resolve_item_parents'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    for (name, expansion) in [
        (
            "declared_parent_names!",
            knobas_core::declared_parent_names!("decl"),
        ),
        (
            "declared_names_as_parent!",
            knobas_core::declared_names_as_parent!("decl"),
        ),
    ] {
        assert!(
            source.contains(expansion),
            "sync.resolve_item_parents no longer carries {name}'s expansion; \
             restate it in a new migration"
        );
    }
}
