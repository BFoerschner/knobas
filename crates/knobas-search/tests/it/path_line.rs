//! Every launcher row whose kind declares a **parent** carries the parent
//! chain as its path (#596, ADR-0018), against a real PostgreSQL.
//!
//! #284 drew a Confluence page's ancestors on the row's path line; this is
//! that line generalised to every kind the adapter declares a parent for. The
//! first such rows are Gitea's branches, pull requests and commits, whose
//! parent is their repo, shown as `owner/repo` -- the first name the
//! declaration gives -- so two "fix typo" commits from different repos are
//! told apart by their row. `ancestor_path.rs` stays as it was and still
//! passes: the Confluence line has not moved.
//!
//! The items are shaped the way the real Gitea adapter stores them, and the
//! declaration is that adapter's own descriptor, not a copy of it. One
//! database for the file, a unique token per test, nothing truncated.

use chrono::{TimeZone, Utc};
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

fn token() -> String {
    format!("pl{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    for (id, kind, name) in [("gitea", "gitea", "Gitea"), ("kuma", "kuma", "Uptime Kuma")] {
        sqlx::query(
            "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
             values ($1, $2, $3, 'http://x', 'Pat')
             on conflict (id) do update set kind = excluded.kind, enabled = true",
        )
        .bind(id)
        .bind(kind)
        .bind(name)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool
}

/// The searcher the app builds, with Gitea's declared payload paths. Uptime
/// Kuma's template is not passed, as in `under.rs`: a source whose adapter
/// declares nothing declares no parent, which is what Kuma's real descriptor
/// says too.
async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::declaring(pool.clone(), [knobas_source_gitea::descriptor_template()])
        .await
        .unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn item(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    body: &str,
    payload: serde_json::Value,
    updated_at: chrono::DateTime<Utc>,
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
                                item_updated_at)
         values ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(payload)
    .bind(updated_at)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A repo and a branch, a pull request and a commit in it, each carrying `t`
/// in its title so one query finds all four. Answers with the four ids, repo
/// first.
async fn repo(
    pool: &sqlx::PgPool,
    owner: &str,
    name: &str,
    t: &str,
    updated_at: chrono::DateTime<Utc>,
) -> Vec<String> {
    let full = format!("{owner}/{name}");
    let repository = serde_json::json!({ "full_name": full, "name": name });
    let sha = uuid::Uuid::new_v4().simple().to_string();
    vec![
        item(
            pool,
            "gitea",
            "repo",
            &full,
            &format!("{full} {t}"),
            "",
            serde_json::json!({ "full_name": full, "name": name, "owner": { "login": owner } }),
            updated_at,
        )
        .await,
        item(
            pool,
            "gitea",
            "branch",
            &format!("{full}@refs/heads/fix-{t}"),
            &format!("fix-{t}"),
            "",
            serde_json::json!({ "name": format!("fix-{t}"), "repository": repository }),
            updated_at,
        )
        .await,
        item(
            pool,
            "gitea",
            "pr",
            &format!("{full}#7"),
            &format!("fix typo {t}"),
            "",
            serde_json::json!({ "number": 7, "base": { "ref": "main", "repo": repository } }),
            updated_at,
        )
        .await,
        item(
            pool,
            "gitea",
            "commit",
            &format!("{full}@{sha}"),
            &format!("fix typo {t}"),
            "",
            serde_json::json!({ "sha": sha, "repository": repository }),
            updated_at,
        )
        .await,
    ]
}

/// The path each hit carries, by entity id.
fn paths(response: &SearchResponse) -> Vec<(String, Option<String>)> {
    let mut out: Vec<(String, Option<String>)> = response
        .groups
        .iter()
        .flat_map(|g| {
            g.hits
                .iter()
                .map(|h| (h.row.entity_id.clone(), h.row.path.clone()))
        })
        .collect();
    out.sort();
    out
}

/// A branch, a pull request and a commit each carry `owner/repo` -- the repo's
/// full name, the first name Gitea declares for it -- and the repo itself,
/// whose kind declares no parent, carries none. Two "fix typo" commits from
/// two repos are told apart by their row.
#[tokio::test]
async fn a_gitea_branch_pr_and_commit_carry_their_repo_as_their_path() {
    let pool = pool().await;
    let t = token();
    let now = Utc::now();
    let ours = repo(&pool, "tidewater", &format!("payout-{t}"), &t, now).await;
    let theirs = repo(&pool, "elsewhere", &format!("ledger-{t}"), &t, now).await;

    let response = searcher(&pool).await.search(q(&t)).await.unwrap();

    let ours_path = Some(format!("tidewater/payout-{t}"));
    let theirs_path = Some(format!("elsewhere/ledger-{t}"));
    let mut expected = vec![
        (ours[0].clone(), None),
        (ours[1].clone(), ours_path.clone()),
        (ours[2].clone(), ours_path.clone()),
        (ours[3].clone(), ours_path),
        (theirs[0].clone(), None),
        (theirs[1].clone(), theirs_path.clone()),
        (theirs[2].clone(), theirs_path.clone()),
        (theirs[3].clone(), theirs_path),
    ];
    expected.sort();
    assert_eq!(paths(&response), expected);

    // An `under:` query binds the declarations for its predicate, and the
    // path line reads that same placeholder rather than a second bind: its
    // rows draw the line too.
    let scoped = searcher(&pool).await
        .search(q(&format!("under:tidewater/payout-{t} {t}")))
        .await
        .unwrap();
    let mut ours_only: Vec<(String, Option<String>)> = expected
        .into_iter()
        .filter(|(id, _)| ours.contains(id))
        .collect();
    ours_only.sort();
    assert_eq!(paths(&scoped), ours_only);
}

/// A kind no adapter declares a parent for carries no path: a Kuma monitor,
/// whose adapter declares none, and a Gitea commit read by a searcher nobody
/// told about the adapters -- the declaration is what draws the line, never
/// the payload's shape on its own.
#[tokio::test]
async fn a_kind_that_declares_no_parent_carries_no_path() {
    let pool = pool().await;
    let t = token();
    let monitor = item(
        &pool,
        "kuma",
        "monitor",
        &format!("m-{t}"),
        &format!("payout health {t}"),
        "",
        serde_json::json!({ "id": 8, "name": format!("payout health {t}"), "type": "http" }),
        Utc::now(),
    )
    .await;
    let ids = repo(
        &pool,
        "tidewater",
        &format!("undeclared-{t}"),
        &t,
        Utc::now(),
    )
    .await;

    let response = searcher(&pool).await
        .search(q(&format!("health {t}")))
        .await
        .unwrap();
    assert_eq!(paths(&response), vec![(monitor, None)]);

    let blind = Searcher::new(pool.clone())
        .search(q(&format!("kind:commit {t}")))
        .await
        .unwrap();
    assert_eq!(paths(&blind), vec![(ids[3].clone(), None)]);
}

/// The board's newest rows and a built-in list's rows are launcher rows too,
/// and draw the same line a search draws: one answer to "where is this",
/// wherever the row comes from.
#[tokio::test]
async fn the_boards_and_a_lists_rows_carry_the_same_path() {
    let pool = pool().await;
    let t = token();
    // Far enough ahead that no other test's rows outrank these on the board.
    let ahead = Utc.with_ymd_and_hms(2999, 1, 1, 0, 0, 0).unwrap();
    let ids = repo(&pool, "tidewater", &format!("board-{t}"), &t, ahead).await;

    let board = searcher(&pool).await.launcher_board().await.unwrap();
    let commit = board
        .recent
        .iter()
        .find(|row| row.entity_id == ids[3])
        .expect("the newest commit is on the board");
    assert_eq!(commit.path, Some(format!("tidewater/board-{t}")));

    // Just synced: everything the mirror saw in the last hour, newest first.
    let listed = searcher(&pool).await.search(q("list:just-synced")).await.unwrap();
    let in_list: Vec<(String, Option<String>)> = paths(&listed)
        .into_iter()
        .filter(|(id, _)| ids.contains(id))
        .collect();
    let line = Some(format!("tidewater/board-{t}"));
    assert_eq!(
        in_list,
        vec![
            (ids[0].clone(), None),
            (ids[2].clone(), line.clone()),
            (ids[3].clone(), line.clone()),
            (ids[1].clone(), line),
        ]
    );
}
