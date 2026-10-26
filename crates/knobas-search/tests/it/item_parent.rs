//! `sync.item_parent` against the walk it stores, on a real PostgreSQL
//! (migrations `0028` and `0029`, #627, #642, ADR-0019).
//!
//! `under:` used to walk every live item through the declarations on every
//! keystroke. Since #627 the database runs that walk when something it
//! depends on changes and keeps the answer, and the readers ask an index. This
//! file holds the stored answer to the walk -- `knobas_core`'s
//! `declared_parent_names!` and `declared_names_as_parent!`, bound to the
//! declarations `Vocabulary::declaring` builds in Rust -- after every kind of
//! change the trigger code claims to follow: an insert, a payload update, an
//! item moved to another source and back, a delete, a change to the stored
//! declarations, a source configured after its items were written, a source
//! given another adapter kind, and a source removed -- and an insert and an
//! update of several rows in one statement, as the sync writer makes them
//! (#655).
//!
//! Its own database (one per test module, #572), so it may write
//! `sync.parent_declaration` freely; nothing here goes through
//! `Searcher::declaring`, which writes that table whole and would make this
//! file's two adapter kinds the only ones.
//!
//! The seeds are plain `insert`s (and, for the batch, the writer's
//! `insert ... on conflict do update`), as every search test's are: nothing
//! here calls a fill function, because the claim is that nothing has to.

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

/// `(entity_id, name, kind, arm)`: the arm is `'child'` for a parent the item
/// names and `'parent'` for a name it answers to (migration `0029`, #642).
type Rows = Vec<(String, String, String, String)>;

/// What the readers see of the database's stored rows for this test's items:
/// `sync.item_parent` matched to each item's configured adapter on
/// `adapter_kind`, the join `under::parent_names!` and `corpus::LIVE_ITEM`
/// make (the deputy's second ruling on #627).
async fn stored(pool: &PgPool, prefix: &str) -> Rows {
    sqlx::query_as(
        "select ip.entity_id, ip.name, ip.kind, ip.arm from sync.item_parent ip
           join sync.item x on x.entity_id = ip.entity_id
           join knobas.source_config s on s.id = x.source_id and s.kind = ip.adapter_kind
          where ip.entity_id like $1 || '%' order by 1, 2, 3, 4",
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
        "select i.entity_id, n.name, n.kind, n.arm from sync.item i cross join lateral (",
        "select pn.name, pn.kind, 'child' as arm from ",
        knobas_core::declared_parent_names!("$1"),
        " pn union all select sn.name, i.kind, 'parent' from ",
        knobas_core::declared_names_as_parent!("$1"),
        " sn) n where i.entity_id like $2 || '%' order by 1, 2, 3, 4"
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
    // `source_id` is the item's, copied (migration `0029`, #642): the
    // coverage probe reads it in place of joining `sync.item`.
    let stale: Vec<(String, String, String)> = sqlx::query_as(
        "select ip.entity_id, ip.source_id, x.source_id from sync.item_parent ip
           join sync.item x on x.entity_id = ip.entity_id
          where ip.entity_id like $1 || '%' and ip.source_id is distinct from x.source_id",
    )
    .bind(prefix)
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(
        stale.is_empty(),
        "after {step}: rows whose source_id is not their item's: {stale:?}"
    );
    stored
}

fn has(rows: &Rows, entity: &str, name: &str, kind: &str, arm: &str) -> bool {
    rows.iter()
        .any(|(e, n, k, a)| e == entity && n == name && k == kind && a == arm)
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
        has(&rows, &c1, "tide/ghost", "repo", "child"),
        "an unmirrored parent: {rows:?}"
    );
    assert!(
        has(&rows, &p1, "payout", "repo", "child"),
        "the PR's parent: {rows:?}"
    );
    assert!(
        has(&rows, &r1, "tide/payout", "repo", "parent"),
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
    assert!(
        has(&rows, &c1, "tide/renamed", "repo", "child")
            && !has(&rows, &c1, "tide/ghost", "repo", "child")
    );

    // The commit moves to another Gitea source and back: its rows follow
    // the item's `source_id` each time.
    let g2 = format!("g2{t}");
    configure(&pool, &g2, &gitea).await;
    for (to, step) in [
        (&g2, "moving an item to another source"),
        (&g, "moving it back"),
    ] {
        sqlx::query("update sync.item set source_id = $2 where entity_id = $1")
            .bind(&c1)
            .bind(to)
            .execute(&pool)
            .await
            .unwrap();
        let rows = agree(&pool, &prefix(&g), &templates, step).await;
        assert!(has(&rows, &c1, "tide/renamed", "repo", "child"), "{rows:?}");
    }

    sqlx::query("delete from knobas.entity where id = $1")
        .bind(&p1)
        .execute(&pool)
        .await
        .unwrap();
    let rows = agree(&pool, &prefix(&g), &templates, "a delete").await;
    assert!(
        !rows.iter().any(|(e, _, _, _)| *e == p1),
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
        !has(&rows, &r1, "payout", "repo", "parent"),
        "the bare name went with its path: {rows:?}"
    );
    assert!(has(&rows, &r1, "tide/payout", "repo", "parent"));

    configure(&pool, &c, &chain).await;
    let rows = agree(&pool, &prefix(&c), &templates, "configuring the source").await;
    let leaf = format!("{c}:leaf");
    assert!(
        has(&rows, &leaf, "Handbook", "page", "child")
            && has(&rows, &leaf, "Payments", "page", "child")
    );
    assert!(
        has(
            &rows,
            &format!("{c}:payments"),
            "Payments",
            "page",
            "parent"
        ),
        "{rows:?}"
    );

    // The Gitea source pointed at the chain adapter: its items now declare
    // what a chain page would, which for a commit, a PR or a repo is nothing.
    sqlx::query("update knobas.source_config set kind = $2 where id = $1")
        .bind(&g)
        .bind(&chain)
        .execute(&pool)
        .await
        .unwrap();
    let rows = agree(
        &pool,
        &prefix(&g),
        &templates,
        "a source's adapter kind change",
    )
    .await;
    assert!(
        rows.is_empty(),
        "no chain declaration reaches these kinds: {rows:?}"
    );

    sqlx::query("delete from knobas.source_config where id = $1")
        .bind(&c)
        .execute(&pool)
        .await
        .unwrap();
    let rows = agree(&pool, &prefix(&c), &templates, "removing the source").await;
    assert!(
        rows.is_empty(),
        "a source nobody configures declares nothing: {rows:?}"
    );
}

/// Write `rows` -- `(kind, key, payload)` of `source` -- in the sync writer's
/// shape (`ITEM_UPSERT`, `knobas-sync`): **one** multi-row
/// `insert ... select from unnest(...) on conflict (entity_id) do update`.
/// The item triggers are `for each statement` over transition tables, so the
/// rows this statement inserts reach `item_parent_on_insert`'s `new_rows`
/// together, and the rows it updates reach `item_parent_on_update`'s
/// `old_rows` and `new_rows` together -- which `item()`, one row per
/// statement, never does. The entities go in first, also in one statement,
/// left alone when already there.
async fn upsert_batch(pool: &PgPool, source: &str, rows: &[(&str, &str, serde_json::Value)]) {
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
    let written = sqlx::query(
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
    assert_eq!(
        written.rows_affected(),
        rows.len() as u64,
        "one statement, every row"
    );
}

/// **Every row of a batch is resolved, not only the first** (#655). The
/// sync writer inserts and updates a whole batch in one statement; the
/// triggers see it as one transition table. Four items of three kinds and
/// three different parents are inserted in one statement, then all four
/// payloads change in one upsert of the writer's shape -- so the insert
/// trigger's `new_rows` and the update trigger's `new_rows` each hold four
/// rows, and a trigger that resolves only one of them leaves three that
/// disagree with the walk.
#[tokio::test]
async fn every_row_of_one_statement_is_resolved() {
    let pool = pool().await;
    let t = format!("b{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let gitea = format!("gitea-{t}");
    let g = format!("g{t}");
    let prefix = format!("{g}:");
    store_declaration(&pool, &gitea, &gitea_paths()).await;
    let templates = vec![(gitea.clone(), gitea_paths())];
    configure(&pool, &g, &gitea).await;

    let repo = |full: &str, name: &str| serde_json::json!({ "full_name": full, "name": name });
    upsert_batch(
        &pool,
        &g,
        &[
            (
                "commit",
                "c1",
                serde_json::json!({ "repository": repo("tide/ghost", "ghost") }),
            ),
            (
                "commit",
                "c2",
                serde_json::json!({ "repository": repo("tide/ledger", "ledger") }),
            ),
            (
                "pr",
                "p1",
                serde_json::json!({ "base": { "repo": repo("tide/payout", "payout") } }),
            ),
            ("repo", "r1", repo("tide/payout", "payout")),
        ],
    )
    .await;
    let rows = agree(&pool, &prefix, &templates, "one multi-row insert").await;
    let id = |key: &str| format!("{g}:{key}");
    for (entity, name, arm) in [
        ("c1", "tide/ghost", "child"),
        ("c2", "tide/ledger", "child"),
        ("p1", "tide/payout", "child"),
        ("r1", "tide/payout", "parent"),
    ] {
        assert!(
            has(&rows, &id(entity), name, "repo", arm),
            "{entity} was inserted in the batch but not resolved: {rows:?}"
        );
    }

    // Every payload changes, in the writer's own upsert: all four rows
    // exist, so the insert trigger's `new_rows` is empty and the update
    // trigger's holds all four.
    upsert_batch(
        &pool,
        &g,
        &[
            (
                "commit",
                "c1",
                serde_json::json!({ "repository": repo("tide/spectre", "spectre") }),
            ),
            (
                "commit",
                "c2",
                serde_json::json!({ "repository": repo("tide/journal", "journal") }),
            ),
            (
                "pr",
                "p1",
                serde_json::json!({ "base": { "repo": repo("tide/refund", "refund") } }),
            ),
            ("repo", "r1", repo("tide/refund", "refund")),
        ],
    )
    .await;
    let rows = agree(
        &pool,
        &prefix,
        &templates,
        "one multi-row upsert of existing rows",
    )
    .await;
    for (entity, old, new, arm) in [
        ("c1", "tide/ghost", "tide/spectre", "child"),
        ("c2", "tide/ledger", "tide/journal", "child"),
        ("p1", "tide/payout", "tide/refund", "child"),
        ("r1", "tide/payout", "tide/refund", "parent"),
    ] {
        assert!(
            has(&rows, &id(entity), new, "repo", arm),
            "{entity} was updated in the batch but lacks its new parent: {rows:?}"
        );
        assert!(
            !has(&rows, &id(entity), old, "repo", arm),
            "{entity} was updated in the batch but kept its old parent: {rows:?}"
        );
    }
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
            knobas_core::declared_parent_names!("dd.decl"),
        ),
        (
            "declared_names_as_parent!",
            knobas_core::declared_names_as_parent!("dd.decl"),
        ),
    ] {
        assert!(
            source.contains(expansion),
            "sync.resolve_item_parents no longer carries {name}'s expansion; \
             restate it in a new migration"
        );
    }
}

/// The backend pid a transaction runs on, so a lock in `pg_locks` can be told
/// apart from another test's.
async fn pid_of(tx: &mut sqlx::Transaction<'static, sqlx::Postgres>) -> i32 {
    sqlx::query_scalar("select pg_backend_pid()")
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

/// Whether backend `pid` is waiting -- not holding -- the `(627, 0)` key.
/// `pg_advisory_xact_lock(int4, int4)` shows as `classid` = the first key,
/// `objid` = the second, `objsubid` = 2.
async fn waiting_on_the_key(pool: &PgPool, pid: i32) -> bool {
    sqlx::query_scalar(
        "select exists (select 1 from pg_locks
                         where locktype = 'advisory' and classid = 627 and objid = 0
                           and objsubid = 2 and pid = $1 and not granted)",
    )
    .bind(pid)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Poll until `pid` waits on the key, up to five seconds. A deadline, not a
/// sleep that orders anything: the order comes from the lock itself, and this
/// only says when the waiting side has got there.
async fn until_waiting(pool: &PgPool, pid: i32) -> bool {
    for _ in 0..500 {
        if waiting_on_the_key(pool, pid).await {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    false
}

/// One race fixture: a Gitea-declared source with a repo of its own, and the
/// pieces a racing item and a racing declaration change need.
struct Race {
    pool: PgPool,
    gitea: String,
    source: String,
}

async fn race() -> Race {
    let pool = pool().await;
    let t = format!("r{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let gitea = format!("gitea-{t}");
    let source = format!("g{t}");
    store_declaration(&pool, &gitea, &gitea_paths()).await;
    configure(&pool, &source, &gitea).await;
    Race {
        pool,
        gitea,
        source,
    }
}

/// Gitea's declarations with every parent answering to its full name only.
fn full_names_only() -> Vec<KindPaths> {
    let mut paths = gitea_paths();
    for kind in &mut paths {
        for parent in &mut kind.parent {
            parent.names.truncate(1);
        }
    }
    paths
}

/// Insert one commit naming `own/race` -- full name and bare name -- inside
/// the open transaction.
async fn insert_commit(tx: &mut sqlx::Transaction<'static, sqlx::Postgres>, source: &str) {
    let id = format!("{source}:race");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'commit', 'race')")
        .bind(&id)
        .execute(&mut **tx)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, $2, 'commit', 'race', '', $3)",
    )
    .bind(&id)
    .bind(source)
    .bind(serde_json::json!({ "repository": { "full_name": "own/race", "name": "race" } }))
    .execute(&mut **tx)
    .await
    .unwrap();
}

/// Narrow the stored declaration inside the open transaction.
async fn narrow(tx: &mut sqlx::Transaction<'static, sqlx::Postgres>, gitea: &str) {
    sqlx::query("update sync.parent_declaration set payload_paths = $2 where adapter_kind = $1")
        .bind(gitea)
        .bind(sqlx::types::Json(full_names_only()))
        .execute(&mut **tx)
        .await
        .unwrap();
}

/// **An item written while a declaration change is uncommitted resolves
/// against the change** (#627, the deputy's second ruling): the declaration
/// change goes first and holds its transaction open; the item insert then
/// waits on the `(627, 0)` key, seen in `pg_locks`; the change commits, and
/// the insert resolves against what it committed.
///
/// Without the item trigger's shared lock the insert does not wait, resolves
/// under a snapshot that cannot see the change, and keeps the bare name the
/// change removed -- the stored row disagrees with the walk.
#[tokio::test]
async fn an_item_written_during_a_declaration_change_resolves_against_it() {
    let r = race().await;
    let mut a = r.pool.begin().await.unwrap();
    narrow(&mut a, &r.gitea).await;

    let mut b = r.pool.begin().await.unwrap();
    let b_pid = pid_of(&mut b).await;
    let source = r.source.clone();
    let inserting = tokio::spawn(async move {
        insert_commit(&mut b, &source).await;
        b
    });
    let waited = until_waiting(&r.pool, b_pid).await;
    a.commit().await.unwrap();
    inserting.await.unwrap().commit().await.unwrap();

    let templates = vec![(r.gitea.clone(), full_names_only())];
    let rows = agree(
        &r.pool,
        &format!("{}:", r.source),
        &templates,
        "the declaration change, then the item",
    )
    .await;
    assert!(
        !rows.iter().any(|(_, name, _, _)| name == "race"),
        "{rows:?}"
    );
    assert!(
        waited,
        "the item insert never waited on the declaration change's key"
    );
}

/// **A declaration change made while an item write is uncommitted reaches
/// that item** -- the other order. The insert goes first and holds its
/// transaction open; the declaration change then waits on the key; the
/// insert commits, and the change's recompute sees the item.
///
/// Without the declaration trigger's exclusive lock the change does not wait,
/// recomputes under a snapshot that cannot see the item, and the item keeps
/// the resolution it made against the old declarations.
#[tokio::test]
async fn a_declaration_change_during_an_item_write_reaches_the_item() {
    let r = race().await;
    let mut b = r.pool.begin().await.unwrap();
    insert_commit(&mut b, &r.source).await;

    let mut a = r.pool.begin().await.unwrap();
    let a_pid = pid_of(&mut a).await;
    let gitea = r.gitea.clone();
    let changing = tokio::spawn(async move {
        narrow(&mut a, &gitea).await;
        a
    });
    let waited = until_waiting(&r.pool, a_pid).await;
    b.commit().await.unwrap();
    changing.await.unwrap().commit().await.unwrap();

    let templates = vec![(r.gitea.clone(), full_names_only())];
    let rows = agree(
        &r.pool,
        &format!("{}:", r.source),
        &templates,
        "the item, then the declaration change",
    )
    .await;
    assert!(
        !rows.iter().any(|(_, name, _, _)| name == "race"),
        "{rows:?}"
    );
    assert!(
        waited,
        "the declaration change never waited on the item write's key"
    );
}

/// **An undated item's parents take its new sync time** (#2189). A source
/// that reports no `updated` dates its rows by `synced_at`, so a re-sync that
/// only stamps the row moves the time its parents are ordered by, and the
/// update trigger has to find that row although no column it resolves from
/// changed -- and leave a dated row's alone when only its stamp moved.
#[tokio::test]
async fn an_undated_items_parents_take_its_new_sync_time() {
    let pool = pool().await;
    let t = format!("u{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    let gitea = format!("gitea-{t}");
    let g = format!("g{t}");
    store_declaration(&pool, &gitea, &gitea_paths()).await;
    configure(&pool, &g, &gitea).await;
    for key in ["undated", "dated"] {
        item(
            &pool,
            &g,
            "pr",
            key,
            serde_json::json!({ "base": { "repo": { "full_name": "tide/payout", "name": "payout" } } }),
        )
        .await;
    }
    sqlx::query("update sync.item set item_updated_at = timestamptz '2026-01-01' where entity_id = $1")
        .bind(format!("{g}:dated"))
        .execute(&pool)
        .await
        .unwrap();

    // The writer's stamp: one statement, both rows, only `synced_at`.
    sqlx::query(
        "update sync.item set synced_at = timestamptz '2030-01-01' where entity_id like $1 || '%'",
    )
    .bind(format!("{g}:"))
    .execute(&pool)
    .await
    .unwrap();

    let at: Vec<(String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "select distinct entity_id, at from sync.item_parent where entity_id like $1 || '%' order by 1",
    )
    .bind(format!("{g}:"))
    .fetch_all(&pool)
    .await
    .unwrap();
    let day = |s: &str| s.parse::<chrono::DateTime<chrono::Utc>>().unwrap();
    assert_eq!(
        at,
        vec![
            (format!("{g}:dated"), day("2026-01-01T00:00:00Z")),
            (format!("{g}:undated"), day("2030-01-01T00:00:00Z")),
        ]
    );
}
