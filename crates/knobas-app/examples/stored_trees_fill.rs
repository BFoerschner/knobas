//! The stored-trees pass (PR #2479, 2026-11-14) run on its own against a
//! profile's database and the source's forge, for measuring what it costs:
//! each pass's requests, time, levels and branches, and the store's size on
//! disk after it.
//!
//! ```sh
//! cargo run --release -p knobas-app --example stored_trees_fill -- \
//!   --db postgresql://postgres:<pass>@127.0.0.1:56432/knobas --source live-gitea \
//!   [--base-url http://127.0.0.1:3911] [--secrets <profile>/dev-secrets.json] \
//!   [--passes 1] [--until-done]
//! ```
//!
//! The source is built from its configuration row with a stand-in credential
//! (`testenv/perf/standin-forge.mjs` asks none), or with the profile's own
//! credential for it from `--secrets`, a demo profile's `dev-secrets.json`
//! (the live forges refuse the stand-in), on the background lane as
//! the scheduler runs it, with the per-server quota the adapter keeps. One
//! JSON line a pass. Writes nothing but what the pass writes.
//!
//! `--check <repository entity id>` (the live pass, 2026-11-14) fills
//! nothing: for up to `--check-branches` (3) of the repository's branches
//! whose stored tree stands at the mirror's head, it draws the top, every
//! folder and *Go to file* from the stored levels and compares each, as
//! JSON, with the forge's own read of the same section at the same commit.
//! One JSON line a branch: the sections compared and the ones that differ.

use std::time::Instant;

use knobas_source::instance::SourceInstance;
use knobas_sync::scheduler::AdapterRegistry as _;

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut db, mut source_id, mut base_url, mut secrets) = (None, None, None, None);
    let (mut passes, mut until_done) = (1_u32, false);
    let (mut check, mut check_branches) = (None, 3_i64);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db = args.next(),
            "--source" => source_id = args.next(),
            "--base-url" => base_url = args.next(),
            "--secrets" => secrets = args.next(),
            "--passes" => passes = args.next().and_then(|n| n.parse().ok()).unwrap_or(1),
            "--until-done" => until_done = true,
            "--check" => check = args.next(),
            "--check-branches" => {
                check_branches = args.next().and_then(|n| n.parse().ok()).unwrap_or(3);
            }
            _ => {
                eprintln!(
                    "usage: stored_trees_fill --db <url> --source <id> [--base-url <url>] [--secrets <dev-secrets.json>] [--passes <n>] [--until-done]"
                );
                std::process::exit(2);
            }
        }
    }
    let (Some(db), Some(source_id)) = (db, source_id) else {
        eprintln!("--db and --source are required");
        std::process::exit(2);
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async move {
        let pool = sqlx::PgPool::connect(&db).await.expect("the database");
        let (kind, name, url, config): (String, String, String, serde_json::Value) = sqlx::query_as(
            "select kind, display_name, base_url, config from knobas.source_config where id = $1",
        )
        .bind(&source_id)
        .fetch_one(&pool)
        .await
        .expect("the source's row");
        let secret = match &secrets {
            Some(path) => {
                use knobas_secrets::SecretStore as _;
                knobas_secrets::FileStore::new(path)
                    .get(&knobas_secrets::KeychainAccount::source(&source_id))
                    .expect("the credential file")
                    .expect("a credential for the source")
            }
            None => knobas_secrets::Secret::just(knobas_source::AuthMethod::Pat, "standin"),
        };
        let source = knobas_app::sources::Registry::for_process()
            .build(SourceInstance {
                id: source_id.clone(),
                kind,
                display_name: name,
                base_url: base_url.unwrap_or(url),
                auth: Some(secret.kind),
                secret: Some(secret.value),
                account: secret.account,
                config,
            })
            .expect("the source builds");
        if let Some(repository) = check {
            same_answer(&pool, &source_id, source.as_ref(), &repository, check_branches).await;
            return;
        }
        let mut pass = 0;
        loop {
            pass += 1;
            let started = Instant::now();
            let filled = knobas_http::on_lane(
                knobas_http::Lane::Background,
                knobas_sync::stored_trees::refresh(&pool, &source_id, source.as_ref()),
            )
            .await
            .expect("the pass");
            let ms = started.elapsed().as_millis();
            let (trees, branches, entries, rows): (i64, i64, i64, i64) = sqlx::query_as(
                "select pg_total_relation_size('sync.git_tree'), pg_total_relation_size('sync.branch_tree'),
                        coalesce((select sum(cardinality(names)) from sync.git_tree), 0)::bigint,
                        (select count(*) from sync.git_tree)",
            )
            .fetch_one(&pool)
            .await
            .expect("the sizes");
            println!(
                "{}",
                serde_json::json!({
                    "pass": pass, "ms": ms, "branches": filled.branches, "too_large": filled.too_large,
                    "levels": filled.levels, "requests": filled.requests, "left": filled.left,
                    "removed": filled.removed, "git_tree_bytes": trees, "branch_tree_bytes": branches,
                    "stored_levels": rows, "stored_entries": entries,
                })
            );
            // A line a pass as it ends, also into a file.
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let done = filled.left == 0;
            if (until_done && done) || (!until_done && pass >= passes) {
                break;
            }
        }
    });
}

/// One stored level as the database holds it: sha, names, modes, sizes, ids.
type LevelRow = (Vec<u8>, Vec<String>, Vec<i32>, Vec<i64>, Vec<Vec<u8>>);

/// A stored branch: its entity, commit, root, payload and repository page.
type BranchRow = (String, String, Vec<u8>, serde_json::Value, Option<String>);

/// The stored levels the root reaches, by sha.
async fn levels_of(
    pool: &sqlx::PgPool,
    source_id: &str,
    root: &[u8],
) -> std::collections::HashMap<String, Vec<knobas_source::tree::TreeItem>> {
    let rows: Vec<LevelRow> = sqlx::query_as(
        "with recursive reached(sha) as (
           select $2::bytea
           union
           select u.sha from reached r
             join sync.git_tree t on t.source_id = $1 and t.sha = r.sha
             cross join lateral unnest(t.shas, t.modes) as u(sha, mode)
            where u.mode = 16384
         )
         select t.sha, t.names, t.modes, t.sizes, t.shas
           from reached r join sync.git_tree t on t.source_id = $1 and t.sha = r.sha",
    )
    .bind(source_id)
    .bind(root)
    .fetch_all(pool)
    .await
    .expect("the levels");
    rows.into_iter()
        .map(|(sha, names, modes, sizes, shas)| {
            let items = names
                .into_iter()
                .zip(modes)
                .zip(sizes)
                .zip(shas)
                .map(
                    |(((name, mode), size), sha)| knobas_source::tree::TreeItem {
                        name,
                        mode: u32::try_from(mode).unwrap_or(0),
                        size: u64::try_from(size).unwrap_or(0),
                        sha: hex(&sha),
                    },
                )
                .collect();
            (hex(&sha), items)
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Each section of `repository`'s stored branches drawn from the store
/// against the forge's own read of it.
async fn same_answer(
    pool: &sqlx::PgPool,
    source_id: &str,
    source: &dyn knobas_source::Source,
    repository: &str,
    branches: i64,
) {
    let rows: Vec<BranchRow> = sqlx::query_as(
        "select bt.entity_id, bt.commit_sha, bt.root, i.payload, r.web_url
           from sync.branch_tree bt
           join sync.item i on i.entity_id = bt.entity_id
           left join sync.item r on r.entity_id = bt.repository_id
          where bt.source_id = $1 and bt.repository_id = $2 and bt.root is not null
          order by bt.entity_id limit $3",
    )
    .bind(source_id)
    .bind(repository)
    .bind(branches)
    .fetch_all(pool)
    .await
    .expect("the stored branches");
    if rows.is_empty() {
        println!(
            "{}",
            serde_json::json!({ "repository": repository, "error": "no stored branch" })
        );
    }
    for (branch, commit, root, payload, web) in rows {
        let tree = knobas_source::tree::StoredTree {
            commit: commit.clone(),
            root: hex(&root),
            levels: levels_of(pool, source_id, &root).await,
            repository_url: web,
        };
        let walked = tree.walk().expect("every level below the root is stored");
        let mut sections = vec!["files".to_owned(), format!("paths@{commit}")];
        sections.extend(
            walked
                .iter()
                .filter(|(_, item)| item.is_tree())
                .map(|(path, _)| format!("files@{commit}:{path}")),
        );
        let (mut same, mut differ) = (0, Vec::new());
        for section in &sections {
            let drawn = source.tree_view(&branch, Some(section), &payload, &tree);
            let read = source.view(&branch, Some(section)).await;
            let (drawn, read) = match (drawn, read) {
                (Some(drawn), Ok(Some(read))) => (
                    serde_json::to_value(&drawn).expect("json"),
                    serde_json::to_value(&read).expect("json"),
                ),
                (drawn, read) => {
                    differ.push(serde_json::json!({
                        "section": section, "drawn": drawn.is_some(), "read": format!("{read:?}").chars().take(200).collect::<String>(),
                    }));
                    continue;
                }
            };
            if drawn == read {
                same += 1;
            } else {
                let (d, r) = (drawn.to_string(), read.to_string());
                let at = d.bytes().zip(r.bytes()).take_while(|(a, b)| a == b).count();
                let from = at.saturating_sub(80);
                differ.push(serde_json::json!({
                    "section": section,
                    "drawn": d.get(from..(at + 120).min(d.len())).unwrap_or(""),
                    "read": r.get(from..(at + 120).min(r.len())).unwrap_or(""),
                }));
            }
        }
        println!(
            "{}",
            serde_json::json!({
                "branch": branch, "commit": commit, "root": hex(&root), "entries": walked.len(),
                "sections": sections.len(), "same": same, "differ": differ,
            })
        );
    }
}
