//! `reach:` (#817, R-2.9) against a real PostgreSQL: the job-reach read as a
//! search filter. The claim is that the two are one walk, so the test asks
//! both on one graph and compares the ids.
//!
//! One database for the file (#572); every key is unique to the test, so no
//! other test's rows can answer.

use knobas_core::entity::EntityRef;
use knobas_core::link::{self, Origin};
use knobas_search::{SearchFilters, SearchQuery, Searcher};

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..10].to_uppercase()
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// One live mirror item.
async fn item(pool: &sqlx::PgPool, source: &str, kind: &str, key: &str) -> String {
    let id = EntityRef::new(source, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(key)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,$4,$5)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(key)
    .bind(serde_json::json!({ "key": key }))
    .execute(pool)
    .await
    .unwrap();
    id
}

async fn asset(pool: &sqlx::PgPool, type_id: &str, name: &str, parent: Option<&str>) -> String {
    let id = EntityRef::new("asset", &uuid::Uuid::new_v4().to_string()).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'asset',$2)")
        .bind(&id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("insert into knobas.asset (id, parent_id, type_id, name) values ($1,$2,$3,$4)")
        .bind(&id)
        .bind(parent)
        .bind(type_id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
    id
}

async fn draw(pool: &sqlx::PgPool, from: &str, to: &str, relation: &str) {
    link::create(
        pool,
        &EntityRef::parse(from).unwrap(),
        &EntityRef::parse(to).unwrap(),
        relation,
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();
}

async fn search(pool: &sqlx::PgPool, raw: &str) -> Vec<String> {
    let response = Searcher::declaring(pool.clone(), [knobas_source_jira::descriptor_template()])
        .await
        .unwrap()
        .search(SearchQuery {
            raw: raw.to_owned(),
            limit: 200,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap();
    assert!(
        response.interpreted.unknown_tokens.is_empty(),
        "{raw}: {:?}",
        response.interpreted.unknown_tokens
    );
    let mut ids: Vec<String> = response
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .collect();
    ids.sort();
    ids
}

/// `reach:<key>` and `reach:<id>` return exactly the ids the read returns, on
/// a graph with every edge the walk treats differently: a hop, a second hop it
/// stops before, a container's VM, host and the VM's monitor it climbs to
/// (R-4.17, ADR-0025 as amended on #942), the container beside it and what a
/// linked VM holds it never brings, and a sub-task's parent's links.
#[tokio::test]
async fn reach_returns_the_same_ids_as_the_read() {
    let pool = pool().await;
    let tag = unique();
    // The tickets' source of its own, configured with the Jira adapter whose
    // stored declaration the walk reads a parent's issue type through
    // (#2364); stored before the read, as bring-up stores it.
    let jira = format!("jira-{}", tag.to_lowercase());
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'jira', $1, 'http://127.0.0.1:1', 'pat')",
    )
    .bind(&jira)
    .execute(&pool)
    .await
    .unwrap();
    Searcher::declaring(pool.clone(), [knobas_source_jira::descriptor_template()])
        .await
        .unwrap();
    let parent = item(&pool, &jira, "ticket", &format!("P{tag}-1")).await;
    sqlx::query("update sync.item set payload = payload || '{\"fields\":{\"issuetype\":{\"name\":\"Story\"}}}' where entity_id = $1")
        .bind(&parent)
        .execute(&pool)
        .await
        .unwrap();
    let key = format!("P{tag}-2");
    let sub = EntityRef::new(&jira, &key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'ticket',$2)")
        .bind(&sub)
        .bind(&key)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$4,'ticket',$2,$2,$3)",
    )
    .bind(&sub)
    .bind(&key)
    .bind(serde_json::json!({ "key": key, "fields": { "parent": { "key": format!("P{tag}-1") } } }))
    .bind(&jira)
    .execute(&pool)
    .await
    .unwrap();

    let pr = item(&pool, "gitea", "pr", &format!("svc-{tag}#1")).await;
    let build = item(&pool, "teamcity", "build", &format!("Build-{tag}#1")).await;
    let beyond = item(&pool, "teamcity", "build", &format!("Deploy-{tag}#1")).await;
    let page = item(&pool, "confluence", "page", &format!("Design-{tag}")).await;
    let repo = item(&pool, "gitea", "repo", &format!("tidewater/svc-{tag}")).await;
    let host = asset(&pool, "hypervisor", &format!("hv-{tag}"), None).await;
    let vm = asset(&pool, "vm", &format!("vm-{tag}"), Some(&host)).await;
    let container = asset(&pool, "container", &format!("svc-{tag}"), Some(&vm)).await;
    let sibling = asset(&pool, "container", &format!("db-{tag}"), Some(&vm)).await;
    let watch = item(&pool, "kuma", "monitor", &format!("vm-{tag}-ping")).await;
    let other_vm = asset(&pool, "vm", &format!("vm2-{tag}"), None).await;
    let on_other_vm = asset(&pool, "container", &format!("app-{tag}"), Some(&other_vm)).await;

    draw(&pool, &sub, &pr, "implements").await;
    draw(&pool, &pr, &build, "built-by").await;
    draw(&pool, &build, &beyond, "triggers").await;
    draw(&pool, &page, &sub, "documents").await;
    draw(&pool, &sub, &container, "deploys").await;
    draw(&pool, &vm, &watch, "monitored-by").await;
    draw(&pool, &sub, &other_vm, "deploys").await;
    draw(&pool, &parent, &repo, "works-in").await;

    let mut read: Vec<String> = knobas_core::reach::reach(&pool, &sub)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.entity_id)
        .collect();
    read.sort();
    let mut want = vec![pr, build, page, repo, container, vm, host, watch, other_vm];
    want.sort();
    assert_eq!(read, want, "the read itself, before comparing");
    for out in [&beyond, &sibling, &on_other_vm] {
        assert!(!read.contains(out), "{out} is past the walk's edge");
    }

    assert_eq!(search(&pool, &format!("reach:{key}")).await, read);
    assert_eq!(
        search(&pool, &format!("reach:{}", sub.to_lowercase())).await,
        read
    );

    // A saved list holding it: the rail's count is the read's, from the one
    // walk its statement makes for every corpus (#1653), and so are its rows.
    let searcher = Searcher::declaring(pool.clone(), [knobas_source_jira::descriptor_template()])
        .await
        .unwrap();
    let saved = searcher
        .save_list(&format!("Reach of {key}"), &format!("reach:{key}"))
        .await
        .unwrap();
    assert_eq!(saved.count, i64::try_from(read.len()).unwrap(), "{saved:?}");
    let opened = searcher.smart_list_items(&saved.id, 200).await.unwrap();
    let mut listed: Vec<String> = opened
        .groups
        .iter()
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .collect();
    listed.sort();
    assert_eq!(listed, read);
}

/// A key no ticket answers to reaches nothing: an empty answer, not every row.
#[tokio::test]
async fn reach_of_an_unknown_key_is_empty() {
    let pool = pool().await;
    assert!(
        search(&pool, &format!("reach:NOPE-{}", unique()))
            .await
            .is_empty()
    );
}

/// What [`the_start_set_is_found_through_an_index`] plans over: its own
/// database, `analyzed` or never, holding 4,000 entities of which 800 are
/// Jira tickets (`jira:PAY-<n>`), so a scan of `knobas.entity` is a real
/// cost and not one page the planner may as well read, and three of them
/// mirrored.
async fn many_entities(analyzed: bool) -> sqlx::PgPool {
    let pool = knobas_db::test_util::scratch_database(if analyzed {
        "reach_start_analyzed"
    } else {
        "reach_start_never"
    })
    .await
    .pool(2)
    .await
    .expect("a pool on the scratch database");
    if !analyzed {
        knobas_db::plan_shape::never_analyzed(&pool).await;
    }
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select case when g % 5 = 0 then 'jira:PAY-' || g else 'gitea:o/r#' || g end,
                case when g % 5 = 0 then 'ticket' else 'pr' end,
                'entity ' || g
           from generate_series(1, 4000) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    // A mirror row for a few of them, so the branches read the start set.
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         select id, split_part(id, ':', 1), kind, title, title, '{}'::jsonb
           from knobas.entity where title in ('entity 15', 'entity 20', 'entity 21')",
    )
    .execute(&pool)
    .await
    .unwrap();
    if analyzed {
        sqlx::query("analyze knobas.entity")
            .execute(&pool)
            .await
            .unwrap();
    }
    pool
}

/// `reach:`'s start set is found through an index on the lowercased id and
/// key (#1850), not a scan of every entity, with and without statistics:
/// the values arrive lowercased, so the primary key, which compares the id
/// as stored, cannot answer `pay-15` for `jira:PAY-15`. Planned over a
/// typed id and a bare key, the two halves the start set compares.
#[tokio::test]
async fn the_start_set_is_found_through_an_index() {
    use knobas_search::query::EffectiveFilters;
    use knobas_search::{corpus, sql};

    for analyzed in [false, true] {
        let pool = many_entities(analyzed).await;
        let built = sql::search_sql(
            corpus::ALL,
            None,
            false,
            &EffectiveFilters {
                reach: vec!["jira:PAY-15".to_owned(), "PAY-20".to_owned()],
                ..EffectiveFilters::default()
            },
            10,
            40,
        );
        let plan = sql::explain(&pool, built).await.unwrap();
        let start: Vec<&str> = plan
            .lines()
            .skip_while(|line| line.trim() != "CTE start")
            .skip(1)
            .take_while(|line| !line.trim_start().starts_with("CTE "))
            .collect();
        assert!(!start.is_empty(), "no start set in the plan:\n{plan}");
        let start = start.join("\n");
        // Probed, not walked: no scan, no row read only to be dropped, and a
        // probe of each half. `kind = 'ticket'` filters what the probes
        // found, and both rows found are tickets.
        assert!(
            !start.contains("Seq Scan")
                && !start.contains("Rows Removed by Filter")
                && start.contains("Index Cond: (lower(id) = ")
                && start.contains("Index Cond: (lower(substr(id, "),
            "analyzed: {analyzed}; the start set does not probe both ticket indexes:\n{start}"
        );
        // The rows it found are the two the values name, case aside: the
        // start set's own node, not one of the probes under it.
        let top = start.lines().next().unwrap_or_default();
        assert!(
            top.contains(" rows=2.00 "),
            "analyzed: {analyzed}:\n{start}"
        );
    }
}
