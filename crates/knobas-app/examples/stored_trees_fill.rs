//! The stored-trees pass (PR #2479, 2026-11-14) run on its own against a
//! profile's database and the source's forge, for measuring what it costs:
//! each pass's requests, time, levels and branches, and the store's size on
//! disk after it.
//!
//! ```sh
//! cargo run --release -p knobas-app --example stored_trees_fill -- \
//!   --db postgresql://postgres:<pass>@127.0.0.1:56432/knobas --source live-gitea \
//!   [--base-url http://127.0.0.1:3911] [--passes 1] [--until-done]
//! ```
//!
//! The source is built from its configuration row with a stand-in credential
//! (`testenv/perf/standin-forge.mjs` asks none), on the background lane as
//! the scheduler runs it, with the per-server quota the adapter keeps. One
//! JSON line a pass. Writes nothing but what the pass writes.

use std::time::Instant;

use knobas_source::instance::SourceInstance;
use knobas_sync::scheduler::AdapterRegistry as _;

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut db, mut source_id, mut base_url) = (None, None, None);
    let (mut passes, mut until_done) = (1_u32, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db = args.next(),
            "--source" => source_id = args.next(),
            "--base-url" => base_url = args.next(),
            "--passes" => passes = args.next().and_then(|n| n.parse().ok()).unwrap_or(1),
            "--until-done" => until_done = true,
            _ => {
                eprintln!(
                    "usage: stored_trees_fill --db <url> --source <id> [--base-url <url>] [--passes <n>] [--until-done]"
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
        let source = knobas_app::sources::Registry::for_process()
            .build(SourceInstance {
                id: source_id.clone(),
                kind,
                display_name: name,
                base_url: base_url.unwrap_or(url),
                auth: Some(knobas_source::AuthMethod::Pat),
                secret: Some("standin".to_owned()),
                account: None,
                config,
            })
            .expect("the source builds");
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
