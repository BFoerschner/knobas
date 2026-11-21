//! The held-files pass (PR #2479, 2026-11-15) run on its own against a
//! profile's database and the source's forge, for measuring what it costs:
//! each pass's requests, time and sections held, and the held store's size on
//! disk after it.
//!
//! ```sh
//! cargo run --release -p knobas-app --example held_files_fill -- \
//!   --db postgresql://postgres:<pass>@127.0.0.1:56432/knobas --source live-gitea \
//!   [--base-url http://127.0.0.1:3911] [--secrets <profile>/dev-secrets.json] \
//!   [--passes 1] [--until-done]
//! ```
//!
//! Built as `stored_trees_fill` builds its source: from the configuration
//! row, with a stand-in credential or the profile's own from `--secrets`, on
//! the background lane as the scheduler runs it, with the per-server quota
//! the adapter keeps. The held rows are keyed by the configuration row's
//! address, which is what the app reads them through, so point the row at the
//! forge (`update_source`) rather than only `--base-url`. One JSON line a
//! pass. Writes nothing but what the pass writes.

use std::time::Instant;

use knobas_source::instance::SourceInstance;
use knobas_sync::scheduler::AdapterRegistry as _;

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut db, mut source_id, mut base_url, mut secrets) = (None, None, None, None);
    let (mut passes, mut until_done) = (1_u32, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db" => db = args.next(),
            "--source" => source_id = args.next(),
            "--base-url" => base_url = args.next(),
            "--secrets" => secrets = args.next(),
            "--passes" => passes = args.next().and_then(|n| n.parse().ok()).unwrap_or(1),
            "--until-done" => until_done = true,
            _ => {
                eprintln!(
                    "usage: held_files_fill --db <url> --source <id> [--base-url <url>] [--secrets <dev-secrets.json>] [--passes <n>] [--until-done]"
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
        let mut pass = 0;
        loop {
            pass += 1;
            let started = Instant::now();
            let held = knobas_http::on_lane(
                knobas_http::Lane::Background,
                knobas_sync::held_sections::refresh(&pool, &source_id, source.as_ref()),
            )
            .await
            .expect("the pass");
            let ms = started.elapsed().as_millis();
            let (table, notes, rows, speculative, answers): (i64, i64, i64, i64, i64) =
                sqlx::query_as(
                    "select pg_total_relation_size('sync.held_read'),
                            pg_total_relation_size('sync.held_section'),
                            (select count(*) from sync.held_read where source_id = $1),
                            (select count(*) from sync.held_read where source_id = $1 and speculative),
                            coalesce((select sum(bytes) from sync.held_read where source_id = $1), 0)::bigint",
                )
                .bind(&source_id)
                .fetch_one(&pool)
                .await
                .expect("the sizes");
            println!(
                "{}",
                serde_json::json!({
                    "pass": pass, "ms": ms, "read": held.read, "too_large": held.too_large,
                    "refused": held.refused, "noted": held.noted, "requests": held.requests,
                    "left": held.left, "past_share": held.past_share, "dropped": held.dropped,
                    "bytes": held.bytes, "held_read_bytes": table, "held_section_bytes": notes,
                    "rows": rows, "speculative": speculative, "answer_bytes": answers,
                })
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let done = held.left == 0;
            if (until_done && done) || (!until_done && pass >= passes) {
                break;
            }
        }
    });
}
