//! Link-set patterns (#868, R-7.2, R-7.7) and the member rows the editor
//! draws (#1008), through the link-set commands and the job's read, over the
//! embedded PostgreSQL with the declarations the compiled-in adapters make
//! (`Registry::for_process`) for a Jira, a TeamCity, a Confluence, an Uptime Kuma
//! and a Gitea source.
//!
//! This file's database is its own (#572); each test makes keys and project
//! names nothing else holds.

use knobas_app::IpcErrorCode;
use knobas_app::link_sets::{self, LinkSetDraft, LinkSetMember, LinkSetPattern, LinkSetRule};
use knobas_app::sources::Registry;
use knobas_core::reach::{self, Origin};
use sqlx::PgPool;
use std::collections::BTreeSet;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    for (id, kind) in [
        ("jira", "jira"),
        ("tc", "teamcity"),
        ("wiki", "confluence"),
        ("kuma", "kuma"),
        ("git", "gitea"),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
             values ($1, $2, $1, 'http://x.example', 'Pat', '{}')
             on conflict (id) do update set kind = excluded.kind, enabled = true",
        )
        .bind(id)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    }
    knobas_app::commands::search::searcher(&pool, &Registry::for_process())
        .await
        .unwrap();
    pool
}

/// A tag nothing else in this database holds, letters only.
fn tag() -> String {
    uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .filter(char::is_ascii_hexdigit)
        .take(6)
        .map(|c| char::from(b'A' + (c.to_digit(16).unwrap() as u8)))
        .collect()
}

/// One live mirror item and its entity row.
async fn mirrored(
    pool: &PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    payload: serde_json::Value,
) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1,$2,$3,$4,'',$5, now())",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A TeamCity configuration in `project`, as the adapter mirrors one.
async fn pipeline(pool: &PgPool, project: &str, id: &str) -> String {
    mirrored(
        pool,
        "tc",
        "build_config",
        id,
        &format!("{project} / {id}"),
        serde_json::json!({
            "id": id, "name": id, "projectId": project, "projectName": project,
            "projectPath": [{ "name": "Root" }, { "name": project }],
        }),
    )
    .await
}

/// A Confluence page in `space` under the ancestor titles `ancestors`.
async fn page(pool: &PgPool, space: &str, id: &str, title: &str, ancestors: &[&str]) -> String {
    let ancestors: Vec<_> = ancestors
        .iter()
        .map(|t| serde_json::json!({ "title": t }))
        .collect();
    mirrored(
        pool,
        "wiki",
        "page",
        id,
        title,
        serde_json::json!({
            "id": id, "title": title, "space": { "key": space, "name": space },
            "ancestors": ancestors,
        }),
    )
    .await
}

/// A Jira ticket of its own project, for a `manual` rule.
async fn ticket(pool: &PgPool) -> String {
    let key = format!("{}-1", tag());
    let project = key.trim_end_matches("-1").to_owned();
    mirrored(
        pool,
        "jira",
        "ticket",
        &key,
        &key,
        serde_json::json!({ "key": key, "fields": { "project": { "key": project } } }),
    )
    .await
}

fn pattern(kind: &str, query: &str) -> LinkSetPattern {
    LinkSetPattern {
        kind: kind.to_owned(),
        query: query.to_owned(),
    }
}

fn draft(
    name: &str,
    members: Vec<LinkSetMember>,
    patterns: Vec<LinkSetPattern>,
    on: &str,
) -> LinkSetDraft {
    LinkSetDraft {
        id: None,
        name: name.to_owned(),
        description: String::new(),
        members,
        patterns,
        rules: vec![LinkSetRule {
            kind: "manual".to_owned(),
            value: on.to_owned(),
        }],
    }
}

/// What `ticket` reaches through link sets.
async fn from_sets(pool: &PgPool, ticket: &str) -> BTreeSet<String> {
    reach::reach(pool, ticket)
        .await
        .unwrap()
        .into_iter()
        .filter(|row| matches!(row.origin, Origin::LinkSet { .. }))
        .map(|row| row.entity_id)
        .collect()
}

fn ids(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// R-7.2: "a pattern ... picks up new items as they appear". The set's
/// pattern brings the project's matching pipelines onto the job and not the
/// rest, and a pipeline that syncs afterwards is on the job's next read with
/// no edit to the set.
#[tokio::test]
async fn a_pattern_brings_what_it_matches_and_a_new_match_without_a_set_edit() {
    let pool = pool().await;
    let p = format!("Payouts{}", tag());
    let build = pipeline(&pool, &p, &format!("{p}_Payout_Build")).await;
    let deploy = pipeline(&pool, &p, &format!("{p}_Payout_Deploy")).await;
    let _dashboard = pipeline(&pool, &p, &format!("{p}_Dashboard")).await;
    let _elsewhere = pipeline(&pool, &format!("Other{}", tag()), &format!("{p}_Payout_X")).await;
    let job = ticket(&pool).await;

    link_sets::save(
        &pool,
        &Registry::for_process(),
        draft(
            "Payout stack",
            vec![],
            vec![pattern("pipeline", &format!("project:{p} name:*_payout_*"))],
            &job,
        ),
    )
    .await
    .unwrap();
    assert_eq!(from_sets(&pool, &job).await, ids(&[&build, &deploy]));

    let later = pipeline(&pool, &p, &format!("{p}_Payout_Smoke")).await;
    assert_eq!(
        from_sets(&pool, &job).await,
        ids(&[&build, &deploy, &later])
    );
}

/// The job's *Also on this job* group (#867) lists what a pattern matches as
/// the set's members, and *Hide here* on one leaves it off that job's reach.
#[tokio::test]
async fn the_jobs_set_group_lists_a_patterns_matches_and_hides_one() {
    let pool = pool().await;
    let p = format!("Hide{}", tag());
    let a = pipeline(&pool, &p, &format!("{p}_A")).await;
    let b = pipeline(&pool, &p, &format!("{p}_B")).await;
    let job = ticket(&pool).await;
    link_sets::save(
        &pool,
        &Registry::for_process(),
        draft(
            "Hide",
            vec![],
            vec![pattern("pipeline", &format!("project:{p}"))],
            &job,
        ),
    )
    .await
    .unwrap();

    link_sets::hide(&pool, &job, &a, true).await.unwrap();
    let shown = link_sets::on_job(&pool, &Registry::for_process(), &job)
        .await
        .unwrap();
    let members: Vec<(String, bool)> = shown.sets[0]
        .members
        .iter()
        .map(|m| (m.entity_id.clone(), m.hidden))
        .collect();
    assert_eq!(members, [(a.clone(), true), (b.clone(), false)]);
    assert_eq!(from_sets(&pool, &job).await, ids(&[&b]));
}

/// R-7.7's page pattern: a space, and a parent page or the whole space.
#[tokio::test]
async fn a_page_pattern_is_a_space_and_a_parent_page() {
    let pool = pool().await;
    let space = format!("OPS{}", tag());
    let runbooks = page(&pool, &space, &format!("{space}1"), "Runbooks", &[]).await;
    let restart = page(
        &pool,
        &space,
        &format!("{space}2"),
        "Restart",
        &["Runbooks"],
    )
    .await;
    let deep = page(
        &pool,
        &space,
        &format!("{space}3"),
        "Deep",
        &["Runbooks", "Restart"],
    )
    .await;
    let other = page(&pool, &space, &format!("{space}4"), "Onboarding", &[]).await;
    let _else = page(
        &pool,
        &format!("X{space}"),
        &format!("{space}5"),
        "Elsewhere",
        &["Runbooks"],
    )
    .await;
    let job = ticket(&pool).await;
    let whole = ticket(&pool).await;

    link_sets::save(
        &pool,
        &Registry::for_process(),
        draft(
            "Runbooks",
            vec![],
            vec![pattern("page", &format!("project:{space} under:Runbooks"))],
            &job,
        ),
    )
    .await
    .unwrap();
    link_sets::save(
        &pool,
        &Registry::for_process(),
        draft(
            "Space",
            vec![],
            vec![pattern("page", &format!("project:{space}"))],
            &whole,
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        from_sets(&pool, &job).await,
        ids(&[&runbooks, &restart, &deep])
    );
    assert_eq!(
        from_sets(&pool, &whole).await,
        ids(&[&runbooks, &restart, &deep, &other])
    );
}

/// The editor's read, the dialog's preview and the job's read are one
/// answer, and the launcher's grammar reads the same text the same way.
#[tokio::test]
async fn the_editor_the_preview_the_job_and_the_launcher_agree() {
    let pool = pool().await;
    let p = format!("Ledger{}", tag());
    let a = pipeline(&pool, &p, &format!("{p}_Api")).await;
    let b = pipeline(&pool, &p, &format!("{p}_Apx")).await;
    let _c = pipeline(&pool, &p, &format!("{p}_Web")).await;
    let job = ticket(&pool).await;
    let query = format!("project:{p} name:{p}_Ap?");

    let saved = link_sets::save(
        &pool,
        &Registry::for_process(),
        draft("Ledger", vec![], vec![pattern("pipeline", &query)], &job),
    )
    .await
    .unwrap();
    let row = &saved.set.patterns[0];
    assert_eq!(row.query, query);
    assert_eq!(row.count, 2);
    let editor: BTreeSet<String> = row.matches.iter().map(|m| m.entity_id.clone()).collect();
    let preview = link_sets::preview_pattern(
        &pool,
        &Registry::for_process(),
        &pattern("pipeline", &query),
    )
    .await
    .unwrap();
    assert_eq!(preview.refusal, None);
    let previewed: BTreeSet<String> = preview
        .matches
        .iter()
        .map(|m| m.entity_id.clone())
        .collect();
    let launcher: BTreeSet<String> =
        knobas_app::commands::search::searcher(&pool, &Registry::for_process())
            .await
            .unwrap()
            .search(knobas_search::SearchQuery {
                raw: query.clone(),
                limit: 200,
                filters: knobas_search::SearchFilters::default(),
            })
            .await
            .unwrap()
            .groups
            .iter()
            .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
            .collect();

    assert_eq!(editor, ids(&[&a, &b]));
    assert_eq!(previewed, editor);
    assert_eq!(from_sets(&pool, &job).await, editor);
    assert_eq!(launcher, editor);
    assert!(
        preview
            .projects
            .iter()
            .any(|x| x.key == p && x.source_id == "tc"),
        "the dialog offers the project the pipelines sit in"
    );
}

/// A TeamCity configuration in a subproject: `chain` is the projects from
/// below `_Root` down to its own, `(id, name)`, as the adapter writes
/// `projectPath` (#849).
async fn nested_pipeline(pool: &PgPool, chain: &[(String, String)], id: &str) -> String {
    let (own_id, own_name) = chain.last().unwrap();
    let path: Vec<_> = chain
        .iter()
        .map(|(id, name)| serde_json::json!({ "id": id, "name": name }))
        .collect();
    mirrored(
        pool,
        "tc",
        "build_config",
        id,
        &format!("{own_name} / {id}"),
        serde_json::json!({
            "id": id, "name": id, "projectId": own_id, "projectName": own_name,
            "projectPath": path,
        }),
    )
    .await
}

/// #1019: a pipeline pattern naming a parent project takes the configurations
/// of its subprojects ("Payout_* under TeamCity project Payments" in
/// prototype-6), by the project's id or its name, on the job, in the editor's
/// count, in the dialog's preview and in the launcher; and the dialog offers
/// each project of the chain with its path.
#[tokio::test]
async fn a_pipeline_pattern_naming_a_parent_project_takes_its_subprojects() {
    let pool = pool().await;
    let t = tag();
    let top = (format!("Pay{t}"), format!("Payments{t}"));
    let payouts = (format!("Pay{t}_Payouts"), "Payouts".to_owned());
    let psp = (format!("Pay{t}_Psp"), "PSP client".to_owned());
    let chain = |sub: &(String, String)| {
        [
            (top.0.clone(), top.1.clone()),
            (sub.0.clone(), sub.1.clone()),
        ]
    };
    let build = nested_pipeline(&pool, &chain(&payouts), &format!("{t}_Payout_Build")).await;
    let client = nested_pipeline(&pool, &chain(&psp), &format!("{t}_Payout_Client")).await;
    let _dashboard = nested_pipeline(&pool, &chain(&payouts), &format!("{t}_Dashboard")).await;
    let _elsewhere = pipeline(&pool, &format!("Other{t}"), &format!("{t}_Payout_X")).await;

    for parent in [&top.0, &top.1] {
        let job = ticket(&pool).await;
        let query = format!("project:\"{parent}\" name:*_payout_*");
        let saved = link_sets::save(
            &pool,
            &Registry::for_process(),
            draft("Payments", vec![], vec![pattern("pipeline", &query)], &job),
        )
        .await
        .unwrap();
        let editor: BTreeSet<String> = saved.set.patterns[0]
            .matches
            .iter()
            .map(|m| m.entity_id.clone())
            .collect();
        assert_eq!(editor, ids(&[&build, &client]), "{query}");
        assert_eq!(saved.set.patterns[0].count, 2, "{query}");
        assert_eq!(from_sets(&pool, &job).await, editor, "{query}");
        let preview = link_sets::preview_pattern(
            &pool,
            &Registry::for_process(),
            &pattern("pipeline", &query),
        )
        .await
        .unwrap();
        assert_eq!(preview.count, 2, "{query}");
        let launcher: BTreeSet<String> =
            knobas_app::commands::search::searcher(&pool, &Registry::for_process())
                .await
                .unwrap()
                .search(knobas_search::SearchQuery {
                    raw: query.clone(),
                    limit: 200,
                    filters: knobas_search::SearchFilters::default(),
                })
                .await
                .unwrap()
                .groups
                .iter()
                .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
                .collect();
        assert_eq!(launcher, editor, "{query}");
    }

    // A subproject still takes only its own.
    let job = ticket(&pool).await;
    let saved = link_sets::save(
        &pool,
        &Registry::for_process(),
        draft(
            "Payouts",
            vec![],
            vec![pattern(
                "pipeline",
                &format!("project:{} name:*_payout_*", payouts.0),
            )],
            &job,
        ),
    )
    .await
    .unwrap();
    assert_eq!(saved.set.patterns[0].count, 1);
    assert_eq!(from_sets(&pool, &job).await, ids(&[&build]));

    // The dialog offers the parent, which no configuration sits in directly,
    // and each subproject, each with its chain of names.
    let preview =
        link_sets::preview_pattern(&pool, &Registry::for_process(), &pattern("pipeline", ""))
            .await
            .unwrap();
    let offered = |key: &str| {
        preview
            .projects
            .iter()
            .find(|x| x.source_id == "tc" && x.key == key)
            .map(|x| x.path.clone())
    };
    assert_eq!(offered(&top.0), Some(vec![top.1.clone()]));
    assert_eq!(
        offered(&payouts.0),
        Some(vec![top.1.clone(), "Payouts".to_owned()])
    );
    assert_eq!(
        offered(&psp.0),
        Some(vec![top.1.clone(), "PSP client".to_owned()])
    );
}

/// A pattern is stored only as a filter the grammar reads, for a kind that
/// has patterns (R-7.7: builds and pages); the preview says why instead of
/// failing.
#[tokio::test]
async fn a_pattern_knobas_cannot_read_is_refused() {
    let pool = pool().await;
    let job = ticket(&pool).await;
    for (kind, query) in [
        ("pipeline", "payout"),
        ("pipeline", ""),
        ("pipeline", "name:x status:done"),
        ("repo", "name:x"),
    ] {
        let refused = link_sets::save(
            &pool,
            &Registry::for_process(),
            draft("Bad", vec![], vec![pattern(kind, query)], &job),
        )
        .await
        .unwrap_err();
        assert_eq!(refused.code, IpcErrorCode::Invalid, "{kind} {query:?}");
    }
    let twice = link_sets::save(
        &pool,
        &Registry::for_process(),
        draft(
            "Twice",
            vec![],
            vec![pattern("page", "name:a"), pattern("page", "name:a")],
            &job,
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(twice.code, IpcErrorCode::Invalid);

    let preview = link_sets::preview_pattern(
        &pool,
        &Registry::for_process(),
        &pattern("pipeline", "payout"),
    )
    .await
    .unwrap();
    assert!(preview.refusal.is_some());
    assert!(preview.matches.is_empty());
}

/// A TeamCity build of `config`, finished with `status`, `minutes_ago`.
async fn run(pool: &PgPool, config: &str, number: u32, status: &str, minutes_ago: i32) {
    let key = format!("{config}-{number}");
    let id = mirrored(
        pool,
        "tc",
        "build",
        &key,
        &key,
        serde_json::json!({
            "id": key, "buildTypeId": config, "number": number.to_string(),
            "state": "finished", "status": status,
            "buildType": { "id": config, "name": config },
        }),
    )
    .await;
    sqlx::query(
        "update sync.item set item_updated_at = now() - make_interval(mins => $2) where entity_id = $1",
    )
    .bind(&id)
    .bind(minutes_ago)
    .execute(pool)
    .await
    .unwrap();
}

/// #1008: each member row carries the line and the dot prototype-6's
/// `renderSet` draws, read through the declarations (a pipeline's project
/// path and its newest run's state, a page's space and path) or off the
/// asset (its type and environment, its health); a member knobas has no
/// entity for has neither, and no title.
#[tokio::test]
async fn a_member_row_carries_its_line_and_its_dot() {
    let pool = pool().await;
    let p = format!("Payouts{}", tag());
    let config = format!("{p}_Build");
    let build = pipeline(&pool, &p, &config).await;
    run(&pool, &config, 1, "SUCCESS", 30).await;
    run(&pool, &config, 2, "FAILURE", 5).await;
    let space = format!("OPS{}", tag());
    let doc = page(
        &pool,
        &space,
        &format!("{space}9"),
        "Restart",
        &["Runbooks", "Payouts"],
    )
    .await;
    let asset = format!("asset:{}", uuid::Uuid::new_v4());
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'asset','pve-02')")
        .bind(&asset)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.asset (id, type_id, name, environment, status)
         values ($1, 'vm', 'pve-02', 'prod', 'down')",
    )
    .bind(&asset)
    .execute(&pool)
    .await
    .unwrap();
    let job = ticket(&pool).await;
    let member = |kind: &str, id: &str| LinkSetMember {
        kind: kind.to_owned(),
        entity_id: id.to_owned(),
    };
    let saved = link_sets::save(
        &pool,
        &Registry::for_process(),
        draft(
            "Rows",
            vec![
                member("pipeline", &build),
                member("page", &doc),
                member("asset", &asset),
            ],
            vec![],
            &job,
        ),
    )
    .await
    .unwrap();
    let rows = &saved.set.members;
    let view = |i: usize| {
        (
            rows[i].line.clone(),
            rows[i]
                .state
                .as_ref()
                .map(|chip| (chip.text.clone(), chip.tone)),
        )
    };
    use knobas_core::tone::Tone;
    assert_eq!(
        view(0),
        (
            Some(format!("Root \u{203a} {p}")),
            Some(("FAILURE".to_owned(), Tone::Err))
        ),
        "the newest run's state, not the older SUCCESS"
    );
    assert_eq!(
        view(1),
        (
            Some(format!("{space} \u{203a} Runbooks \u{203a} Payouts")),
            None
        )
    );
    assert_eq!(
        view(2),
        (
            Some("vm, prod".to_owned()),
            Some(("down".to_owned(), Tone::Err))
        )
    );

    // A member whose item knobas no longer has keeps its place and reads as
    // not synced: no title, no line, no dot.
    let gone = format!("tc:{p}_Gone");
    sqlx::query(
        "insert into knobas.link_set_member (set_id, kind, entity_id, position) values ($1, 'pipeline', $2, 9)",
    )
    .bind(&saved.set.id)
    .bind(&gone)
    .execute(&pool)
    .await
    .unwrap();
    let again = link_sets::get(&pool, &Registry::for_process(), &saved.set.id)
        .await
        .unwrap();
    let last = again.set.members.last().unwrap();
    assert_eq!(
        (
            last.entity_id.as_str(),
            &last.title,
            &last.line,
            &last.state
        ),
        (gone.as_str(), &None, &None, &None)
    );
}

/// #1020: a monitor row draws its type and its state's dot, and a repository
/// row its teams, else its owner, all from what Uptime Kuma and Gitea declare
/// (ADR-0007). The payloads are the adapters' own shapes (`map`).
#[tokio::test]
async fn a_monitor_row_draws_its_type_and_dot_and_a_repository_its_team_or_owner() {
    let pool = pool().await;
    let t = tag();
    async fn monitor(pool: &PgPool, key: String, kind: &str, state: &str) -> String {
        let payload = serde_json::json!({ "id": 1, "name": key, "type": kind, "state": state });
        mirrored(pool, "kuma", "monitor", &key, &key, payload).await
    }
    async fn repo(pool: &PgPool, name: String, teams: serde_json::Value) -> String {
        let payload = serde_json::json!({
            "name": name, "full_name": format!("tidewater/{name}"),
            "owner": { "login": "tidewater" }, "teams": teams,
        });
        mirrored(
            pool,
            "git",
            "repo",
            &format!("tidewater/{name}"),
            &name,
            payload,
        )
        .await
    }
    let up = monitor(&pool, format!("api{t}"), "http", "up").await;
    let down = monitor(&pool, format!("db{t}"), "ping", "down").await;
    let pending = monitor(&pool, format!("dns{t}"), "dns", "pending").await;
    let resting = monitor(&pool, format!("mq{t}"), "port", "maintenance").await;
    let teamed = repo(
        &pool,
        format!("payout-service{t}"),
        serde_json::json!([{ "name": "Payments" }, { "name": "Platform" }]),
    )
    .await;
    let bare = repo(&pool, format!("ledger{t}"), serde_json::json!([])).await;
    let job = ticket(&pool).await;
    let member = |kind: &str, id: &str| LinkSetMember {
        kind: kind.to_owned(),
        entity_id: id.to_owned(),
    };
    let saved = link_sets::save(
        &pool,
        &Registry::for_process(),
        draft(
            "Watched",
            vec![
                member("monitor", &up),
                member("monitor", &down),
                member("monitor", &pending),
                member("monitor", &resting),
                member("repo", &teamed),
                member("repo", &bare),
            ],
            vec![],
            &job,
        ),
    )
    .await
    .unwrap();
    use knobas_core::tone::Tone;
    let view: Vec<_> = saved
        .set
        .members
        .iter()
        .map(|row| {
            (
                row.line.clone(),
                row.state
                    .as_ref()
                    .map(|chip| (chip.text.clone(), chip.tone)),
            )
        })
        .collect();
    let line = |s: &str| Some(s.to_owned());
    let dot = |s: &str, tone| Some((s.to_owned(), tone));
    assert_eq!(
        view,
        vec![
            (line("http"), dot("up", Tone::Ok)),
            (line("ping"), dot("down", Tone::Err)),
            (line("dns"), dot("pending", Tone::Warn)),
            (line("port"), dot("maintenance", Tone::Muted)),
            (line("Payments, Platform"), None),
            (line("tidewater"), None),
        ]
    );
}

/// #1843: a pattern's `project:` and `under:` hold a value the server and
/// Rust's `to_lowercase` fold differently (`İ`). The pattern is stored as
/// typed and the read folds both sides with `lower()`, so the page and the
/// pipeline are found, and `lower()` leaves a value Rust folded already as it is.
#[tokio::test]
async fn a_pattern_with_a_dotted_capital_i_matches_under_the_servers_fold() {
    let pool = pool().await;
    let t = tag();
    let space = format!("OPS{t}");
    let parent = format!("İstanbul Rollout {t}");
    let child = page(
        &pool,
        &space,
        &format!("{space}1"),
        "Plan",
        &[parent.as_str()],
    )
    .await;
    let project = format!("İstanbul{t}");
    let build = pipeline(&pool, &project, &format!("{t}_Build")).await;
    let job = ticket(&pool).await;

    let saved = link_sets::save(
        &pool,
        &Registry::for_process(),
        draft(
            "Istanbul",
            vec![],
            vec![
                pattern("page", &format!(r#"project:{space} under:"{parent}""#)),
                pattern("pipeline", &format!(r#"project:"{project}""#)),
            ],
            &job,
        ),
    )
    .await
    .unwrap();
    assert_eq!(from_sets(&pool, &job).await, ids(&[&child, &build]));
    assert_eq!(saved.set.patterns[0].count, 1);
    assert_eq!(saved.set.patterns[1].count, 1);
}
