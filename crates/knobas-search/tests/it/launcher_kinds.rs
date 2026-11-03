//! The launcher's own kinds (#2422, spec #2419), against a real PostgreSQL:
//! a saved ticket view is kind `view` and a link set kind `set`, each found
//! by what it is called and what it holds, with the path line the row's key
//! cell reads; a build configuration is kind `pipeline` behind `#b`, coloured
//! by its newest run; and a run, a commit and a Jira board's mirror row are
//! in neither the answer nor the board's newest rows, whatever the query.
//!
//! The mirror rows are shaped the way the TeamCity, Gitea and Jira adapters
//! store them, under those adapters' own descriptors. This module has a
//! database of its own (`knobas_db::test_util` keys it by file); every row
//! carries a fresh token, and the tests that read the board's newest rows
//! take [`SERIAL`] because that read spans the whole mirror.

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use knobas_core::payload::StateChip;
use knobas_core::tone::Tone;
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};
use tokio::sync::Mutex;

static SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    for (id, kind) in [
        ("gitea", "gitea"),
        ("teamcity", "teamcity"),
        ("jira", "jira"),
    ] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
             values ($1, $2, $2, 'http://x', 'Pat')
             on conflict (id) do update set kind = excluded.kind, enabled = true",
        )
        .bind(id)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::declaring(
        pool.clone(),
        [
            knobas_source_gitea::descriptor_template(),
            knobas_source_teamcity::descriptor_template(),
            knobas_source_jira::descriptor_template(),
        ],
    )
    .await
    .unwrap()
}

/// A word no other row holds, in letters only, so it lexes as one word.
fn token() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    let letters: String = hex[..12]
        .chars()
        .map(|c| match c.to_digit(16) {
            Some(n) => char::from(b'g' + u8::try_from(n).unwrap()),
            None => c,
        })
        .collect();
    format!("lk{letters}")
}

async fn search(pool: &sqlx::PgPool, raw: &str, titles: bool) -> SearchResponse {
    searcher(pool)
        .await
        .search(SearchQuery {
            titles,
            raw: raw.to_owned(),
            limit: 50,
            filters: SearchFilters::default(),
        })
        .await
        .unwrap()
}

/// Each hit as `(kind, entity id, path)`, in the answer's order.
fn hits(response: &SearchResponse) -> Vec<(String, String, Option<String>)> {
    response
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .map(|hit| {
            (
                hit.row.kind.clone(),
                hit.row.entity_id.clone(),
                hit.row.path.clone(),
            )
        })
        .collect()
}

fn ids(response: &SearchResponse) -> Vec<String> {
    hits(response).into_iter().map(|(_, id, _)| id).collect()
}

async fn view(pool: &sqlx::PgPool, id: &str, name: &str, group: &str, query: &str) {
    sqlx::query(
        "insert into knobas.ticket_view (id, name, view_group, position, query)
         values ($1, $2, $3, 0, $4)",
    )
    .bind(id)
    .bind(name)
    .bind(group)
    .bind(query)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_view_is_found_by_its_name_its_group_and_its_conditions() {
    let pool = pool().await;
    let t = token();
    let id = format!("{t}-board");
    view(
        &pool,
        &id,
        &format!("Payouts {t}name"),
        &format!("Team {t}group"),
        &format!("#t status:open {t}cond"),
    )
    .await;

    for word in ["name", "group", "cond"] {
        let response = search(&pool, &format!("{t}{word}"), false).await;
        assert_eq!(
            hits(&response),
            vec![(
                "view".to_owned(),
                id.clone(),
                Some(format!("Team {t}group · #t status:open {t}cond"))
            )],
            "searching its {word}"
        );
    }
    let group = &search(&pool, &format!("{t}name"), false).await.groups[0];
    assert_eq!(
        (
            group.label.as_str(),
            group.plural.as_str(),
            group.monogram.as_str()
        ),
        ("Ticket view", "Ticket views", "VW")
    );
}

#[tokio::test]
async fn a_view_with_no_conditions_has_its_group_for_a_path_line() {
    let pool = pool().await;
    let t = token();
    view(&pool, &format!("{t}-all"), &format!("All {t}"), "Mine", "").await;
    assert_eq!(
        hits(&search(&pool, &t, false).await),
        vec![(
            "view".to_owned(),
            format!("{t}-all"),
            Some("Mine".to_owned())
        )]
    );
}

#[tokio::test]
async fn under_titles_a_view_is_found_by_its_name_alone() {
    let pool = pool().await;
    let t = token();
    view(
        &pool,
        &format!("{t}-v"),
        &format!("{t}name"),
        &format!("{t}group"),
        "",
    )
    .await;
    assert_eq!(
        ids(&search(&pool, &format!("{t}name"), true).await),
        vec![format!("{t}-v")]
    );
    assert!(ids(&search(&pool, &format!("{t}group"), true).await).is_empty());
}

async fn link_set(pool: &sqlx::PgPool, id: &str, name: &str, description: &str) {
    sqlx::query("insert into knobas.link_set (id, name, description) values ($1, $2, $3)")
        .bind(id)
        .bind(name)
        .bind(description)
        .execute(pool)
        .await
        .unwrap();
}

async fn member(
    pool: &sqlx::PgPool,
    set: &str,
    kind: &str,
    entity: Option<&str>,
    query: Option<&str>,
) {
    sqlx::query(
        "insert into knobas.link_set_member (set_id, kind, entity_id, query, pattern, position)
         values ($1, $2, $3, $4, case when $4::text is null then null else '{}'::jsonb end, 0)",
    )
    .bind(set)
    .bind(kind)
    .bind(entity)
    .bind(query)
    .execute(pool)
    .await
    .unwrap();
}

async fn rule(pool: &sqlx::PgPool, set: &str, kind: &str, value: &str) {
    sqlx::query(
        "insert into knobas.link_set_rule (set_id, kind, value, position) values ($1, $2, $3, 0)",
    )
    .bind(set)
    .bind(kind)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

/// A mirrored row: an entity and its live item.
async fn item(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    payload: serde_json::Value,
    at: DateTime<Utc>,
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
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at, synced_at)
         values ($1,$2,$3,$4,'',$5,$6,$6)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(payload)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[tokio::test]
async fn a_set_is_found_by_its_name_and_its_description_with_its_counts() {
    let pool = pool().await;
    let t = token();
    let id = format!("{t}-stack");
    link_set(
        &pool,
        &id,
        &format!("Payout {t}name"),
        &format!("What {t}desc needs"),
    )
    .await;
    member(
        &pool,
        &id,
        "repo",
        Some("gitea:tidewater/payout-service"),
        None,
    )
    .await;
    member(&pool, &id, "page", Some("confluence:123"), None).await;
    member(&pool, &id, "pipeline", None, Some("project:Payouts")).await;
    // Two live tickets by hand, and one rule naming no ticket the mirror holds.
    let now = Utc::now();
    let one = item(
        &pool,
        "jira",
        "ticket",
        &format!("{t}-1"),
        "one",
        serde_json::json!({}),
        now,
    )
    .await;
    let two = item(
        &pool,
        "jira",
        "ticket",
        &format!("{t}-2"),
        "two",
        serde_json::json!({}),
        now,
    )
    .await;
    rule(&pool, &id, "manual", &one).await;
    rule(&pool, &id, "manual", &two).await;
    rule(&pool, &id, "manual", "jira:GONE-1").await;

    for word in ["name", "desc"] {
        let response = search(&pool, &format!("{t}{word}"), false).await;
        assert_eq!(
            hits(&response)
                .into_iter()
                .filter(|(kind, ..)| kind == "set")
                .collect::<Vec<_>>(),
            vec![(
                "set".to_owned(),
                id.clone(),
                Some("2 items · 1 pattern · 2 tickets".to_owned())
            )],
            "searching its {word}"
        );
    }
    let response = search(&pool, &format!("{t}name"), false).await;
    let group = response.groups.iter().find(|g| g.kind == "set").unwrap();
    assert_eq!(
        (
            group.label.as_str(),
            group.plural.as_str(),
            group.monogram.as_str()
        ),
        ("Link set", "Link sets", "LS")
    );
}

#[tokio::test]
async fn an_empty_set_counts_nothing_in_the_singular_and_plural_it_reads() {
    let pool = pool().await;
    let t = token();
    let id = format!("{t}-empty");
    link_set(&pool, &id, &format!("Empty {t}"), "").await;
    member(&pool, &id, "repo", Some("gitea:tidewater/x"), None).await;
    assert_eq!(
        hits(&search(&pool, &t, false).await),
        vec![(
            "set".to_owned(),
            id,
            Some("1 item · 0 patterns · 0 tickets".to_owned())
        )]
    );
}

/// A TeamCity configuration `<t>_Deploy` under *Root › Payouts*, with an
/// older passing run and a newer failing one, the way the adapter stores
/// them. Answers with the configuration's and the two runs' ids.
async fn pipeline(pool: &sqlx::PgPool, t: &str, at: DateTime<Utc>) -> (String, [String; 2]) {
    let config = format!("{t}_Deploy");
    let id = item(
        pool,
        "teamcity",
        "build_config",
        &config,
        &format!("Deploy {t}"),
        serde_json::json!({
            "id": config, "name": format!("Deploy {t}"),
            "projectId": "Payouts", "projectName": "Payouts",
            "projectPath": [{ "id": "Root", "name": "Root" }, { "id": "Payouts", "name": "Payouts" }],
        }),
        at,
    )
    .await;
    let run = |n: i64, status: &str| {
        serde_json::json!({
            "buildTypeId": config, "number": n.to_string(), "state": "finished", "status": status,
            "buildType": { "id": config, "name": format!("Deploy {t}"), "projectId": "Payouts" },
        })
    };
    let older = item(
        pool,
        "teamcity",
        "build",
        &format!("{t}1"),
        &format!("Deploy {t} #1"),
        run(1, "SUCCESS"),
        at - chrono::Duration::hours(2),
    )
    .await;
    let newer = item(
        pool,
        "teamcity",
        "build",
        &format!("{t}2"),
        &format!("Deploy {t} #2"),
        run(2, "FAILURE"),
        at - chrono::Duration::hours(1),
    )
    .await;
    (id, [older, newer])
}

#[tokio::test]
async fn hash_b_answers_a_configuration_as_a_pipeline_coloured_by_its_newest_run() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let (id, _) = pipeline(&pool, &t, Utc::now()).await;

    let response = search(&pool, &format!("#b {t}"), false).await;
    assert_eq!(
        hits(&response),
        vec![("pipeline".to_owned(), id, Some("Root › Payouts".to_owned()))]
    );
    let group = &response.groups[0];
    assert_eq!(
        (
            group.label.as_str(),
            group.plural.as_str(),
            group.monogram.as_str()
        ),
        ("Pipeline", "Pipelines", "PL")
    );
    assert_eq!(
        group.hits[0].row.state,
        Some(StateChip {
            text: "FAILURE".to_owned(),
            tone: Some(Tone::Err)
        })
    );
}

#[tokio::test]
async fn a_pipeline_with_no_run_carries_no_state() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let config = format!("{t}_Quiet");
    item(
        &pool,
        "teamcity",
        "build_config",
        &config,
        &format!("Quiet {t}"),
        serde_json::json!({ "id": config, "name": format!("Quiet {t}") }),
        Utc::now(),
    )
    .await;
    let response = search(&pool, &format!("#b {t}"), false).await;
    assert_eq!(response.groups[0].hits[0].row.state, None);
}

#[tokio::test]
async fn combined_letters_answer_tickets_and_pipelines() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let (config, _) = pipeline(&pool, &t, Utc::now()).await;
    let ticket = item(
        &pool,
        "jira",
        "ticket",
        &format!("{t}-9"),
        &format!("Fix {t}"),
        serde_json::json!({}),
        Utc::now(),
    )
    .await;
    for raw in [format!("#t #b {t}"), format!("#tb {t}")] {
        let response = search(&pool, &raw, false).await;
        let mut got: Vec<(String, String)> = hits(&response)
            .into_iter()
            .map(|(kind, id, _)| (kind, id))
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                ("pipeline".to_owned(), config.clone()),
                ("ticket".to_owned(), ticket.clone())
            ],
            "{raw}"
        );
    }
}

/// A run, a commit and a Jira board's mirror row, each holding the word,
/// beside the configuration and a ticket that hold it too.
async fn everything(pool: &sqlx::PgPool, t: &str, at: DateTime<Utc>) -> (Vec<String>, Vec<String>) {
    let (config, runs) = pipeline(pool, t, at).await;
    let ticket = item(
        pool,
        "jira",
        "ticket",
        &format!("{t}-1"),
        &format!("Fix {t}"),
        serde_json::json!({}),
        at,
    )
    .await;
    let commit = item(
        pool,
        "gitea",
        "commit",
        &format!("tidewater/payout-service@{t}"),
        &format!("Bump {t}"),
        serde_json::json!({}),
        at,
    )
    .await;
    let board = item(
        pool,
        "jira",
        "jira_view",
        &format!("board-{t}"),
        &format!("Board {t}"),
        serde_json::json!({}),
        at,
    )
    .await;
    let [older, newer] = runs;
    (vec![config, ticket], vec![older, newer, commit, board])
}

#[tokio::test]
async fn runs_commits_and_jira_boards_are_in_no_answer_whatever_the_query() {
    let pool = pool().await;
    searcher(&pool).await;
    let t = token();
    let (kept, left_out) = everything(&pool, &t, Utc::now()).await;

    let mut found = ids(&search(&pool, &t, false).await);
    found.sort();
    let mut want = kept.clone();
    want.sort();
    assert_eq!(found, want);
    for raw in [
        format!("kind:build {t}"),
        format!("kind:commit {t}"),
        format!("kind:jira_view {t}"),
        format!("Deploy {t} #2"),
    ] {
        let found = ids(&search(&pool, &raw, false).await);
        assert!(
            found.iter().all(|id| !left_out.contains(id)),
            "{raw} answered {found:?}"
        );
    }
}

#[tokio::test]
async fn runs_commits_and_jira_boards_are_not_among_the_boards_newest_rows() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let searcher = searcher(&pool).await;
    let t = token();
    // Newer than anything another test of this file writes.
    let (kept, left_out) = everything(&pool, &t, Utc::now() + chrono::Duration::days(1)).await;

    let board = searcher.launcher_board().await.unwrap();
    let recent: Vec<(&str, &str)> = board
        .recent
        .iter()
        .map(|row| (row.kind.as_str(), row.entity_id.as_str()))
        .collect();
    for id in &left_out {
        assert!(
            recent.iter().all(|(_, r)| r != id),
            "{id} is among {recent:?}"
        );
    }
    assert!(
        recent.contains(&("pipeline", kept[0].as_str())),
        "{recent:?}"
    );
    assert!(recent.contains(&("ticket", kept[1].as_str())), "{recent:?}");
    let config = board
        .recent
        .iter()
        .find(|row| row.entity_id == kept[0])
        .unwrap();
    assert_eq!(config.path.as_deref(), Some("Root › Payouts"));
    assert_eq!(config.state.as_ref().and_then(|s| s.tone), Some(Tone::Err));
}

/// `kind:` offers what the answer can hold (#2422): a view, a set and a
/// pipeline under the shell's word, and never a kind the answer leaves out,
/// so no suggested value leads nowhere.
#[tokio::test]
async fn kind_suggestions_offer_the_launchers_kinds_and_none_it_leaves_out() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let searcher = searcher(&pool).await;
    let t = token();
    view(&pool, &format!("{t}-v"), &format!("V {t}"), "G", "").await;
    link_set(&pool, &format!("{t}-s"), &format!("S {t}"), "").await;
    everything(&pool, &t, Utc::now()).await;

    let suggestions = searcher.filter_suggestions("").await.unwrap();
    let kinds: Vec<(&str, &str)> = suggestions
        .kinds
        .iter()
        .map(|k| (k.kind.as_str(), k.label.as_str()))
        .collect();
    for wanted in [
        ("pipeline", "Pipeline"),
        ("set", "Link set"),
        ("ticket", "Ticket"),
        ("view", "Ticket view"),
    ] {
        assert!(kinds.contains(&wanted), "{wanted:?} among {kinds:?}");
    }
    for left_out in ["build", "build_config", "commit", "jira_view"] {
        assert!(
            kinds.iter().all(|(k, _)| *k != left_out),
            "{left_out} among {kinds:?}"
        );
    }
}

/// Coverage leaves out what the answer leaves out (#2422): a source whose
/// only authored rows are commits contributed nothing to an `author:`
/// query's answer, so it is not listed as answering it.
#[tokio::test]
async fn a_source_with_only_commits_is_not_in_an_author_querys_coverage() {
    let pool = pool().await;
    let t = token();
    let source = format!("gitea-{t}");
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ($1, 'gitea', $1, 'http://x', 'Pat')",
    )
    .bind(&source)
    .execute(&pool)
    .await
    .unwrap();
    let id = format!("{source}:tidewater/payout-service@{t}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'commit',$2)")
        .bind(&id)
        .bind(format!("Bump {t}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                author, item_updated_at, synced_at)
         values ($1,$2,'commit',$3,'','{}',$4,now(),now())",
    )
    .bind(&id)
    .bind(&source)
    .bind(format!("Bump {t}"))
    .bind(format!("alice{t}"))
    .execute(&pool)
    .await
    .unwrap();

    let response = search(&pool, &format!("author:alice{t} {t}"), false).await;
    assert!(ids(&response).is_empty(), "{:?}", ids(&response));
    let listed: Vec<&str> = response
        .coverage
        .iter()
        .flat_map(|c| c.sources.iter().map(|s| s.source_id.as_str()))
        .collect();
    assert!(!listed.contains(&source.as_str()), "{listed:?}");
}

/// A saved list counts what applying its text answers (#2422 with #2423):
/// `#b` counts the configuration as the pipeline it is, and the word alone
/// counts neither of its runs.
#[tokio::test]
async fn a_saved_lists_count_is_the_answer_its_text_applies() {
    let pool = pool().await;
    let searcher = searcher(&pool).await;
    let t = token();
    pipeline(&pool, &t, Utc::now()).await;

    let pipelines = searcher
        .save_list(&format!("Pipelines {t}"), &format!("#b {t}"))
        .await
        .unwrap();
    assert_eq!(pipelines.count, 1, "{pipelines:?}");
    let word = searcher.save_list(&format!("Word {t}"), &t).await.unwrap();
    assert_eq!(word.count, 1, "{word:?}");
}
