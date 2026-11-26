//! Names found as a picker finds them (the fuzzy-search decisions,
//! 2026-11-19; `docs/decisions/2026-11-19-names-are-found-as-a-picker-finds-them.md`).
//!
//! Björn's two cases: `proxy` did not find the repository `ADProxy`, and
//! `mapping service` did not find `abcMappingService`, because PostgreSQL
//! reads each as one word. A searcher given a [`Picker`] reads what was typed
//! against every name in memory and joins the names it finds to PostgreSQL's
//! answer, without moving a row PostgreSQL returned (D6).
//!
//! This module has a database of its own, shared by its tests, so each test
//! writes the rows of a source it made and asks for that source's rows.

use chrono::{DateTime, Duration, Utc};
use knobas_search::picker::{LOAD_PLANNING, Limits, Picker};
use knobas_search::query::EffectiveFilters;
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

use super::item_search::source;
use super::one_letter::pool;

fn at(minutes: i64) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
        + Duration::minutes(minutes)
}

/// One mirror row of `source`, its id `<source>:<key>`.
#[allow(clippy::too_many_arguments)]
async fn row(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    body: &str,
    author: Option<&str>,
    updated: DateTime<Utc>,
) -> String {
    let id = format!("{source}:{key}");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
        .bind(&id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, author, payload, item_updated_at)
         values ($1, $2, $3, $4, $5, $6, '{}'::jsonb, $7)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(author)
    .bind(updated)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn query(source: &str, raw: &str, titles: bool) -> SearchQuery {
    SearchQuery {
        titles,
        raw: raw.to_owned(),
        limit: 40,
        filters: SearchFilters {
            sources: vec![source.to_owned()],
            ..SearchFilters::default()
        },
    }
}

/// A picker holding every name the module's mirror has now.
async fn picker(pool: &sqlx::PgPool) -> Picker {
    let picker = Picker::default();
    picker.reload(pool).await.unwrap();
    picker
}

async fn ask(pool: &sqlx::PgPool, picker: &Picker, q: SearchQuery) -> SearchResponse {
    Searcher::new(pool.clone())
        .with_picker(picker.clone())
        .search(q)
        .await
        .unwrap()
}

/// Each kind's hit ids, in order, as `kind: ids`.
fn ids(response: &SearchResponse) -> Vec<(String, Vec<String>)> {
    response
        .groups
        .iter()
        .map(|group| {
            (
                group.kind.clone(),
                group
                    .hits
                    .iter()
                    .map(|hit| hit.row.entity_id.clone())
                    .collect(),
            )
        })
        .collect()
}

fn kind<'a>(response: &'a SearchResponse, kind: &str) -> Vec<&'a str> {
    response
        .groups
        .iter()
        .filter(|group| group.kind == kind)
        .flat_map(|group| group.hits.iter().map(|hit| hit.row.entity_id.as_str()))
        .collect()
}

fn holds(response: &SearchResponse, id: &str) -> bool {
    response
        .groups
        .iter()
        .any(|group| group.hits.iter().any(|hit| hit.row.entity_id == id))
}

/// **Björn's two cases, under both scopes.** Without a picker they are the
/// misses he reported; with one, `proxy` finds `ADProxy` after `auth-proxy`,
/// which PostgreSQL found and names, and `mapping service`, in either order,
/// finds `abcMappingService`. The marks say which letters matched.
#[tokio::test]
async fn the_two_real_cases_are_found_under_both_scopes() {
    let pool = pool().await;
    let s = source("cases");
    let adproxy = row(&pool, &s, "repo", "1", "acme/ADProxy", "", None, at(0)).await;
    let mapping = row(
        &pool,
        &s,
        "repo",
        "2",
        "acme/abcMappingService",
        "",
        None,
        at(0),
    )
    .await;
    let auth = row(&pool, &s, "repo", "3", "acme/auth-proxy", "", None, at(-60)).await;
    let plain = row(
        &pool,
        &s,
        "repo",
        "4",
        "acme/mapping-service",
        "",
        None,
        at(-60),
    )
    .await;
    let picker = picker(&pool).await;
    for titles in [false, true] {
        let before = Searcher::new(pool.clone())
            .search(query(&s, "proxy", titles))
            .await
            .unwrap();
        assert_eq!(
            kind(&before, "repo"),
            [auth.as_str()],
            "the miss: {before:?}"
        );
        let before = Searcher::new(pool.clone())
            .search(query(&s, "mapping service", titles))
            .await
            .unwrap();
        assert_eq!(kind(&before, "repo"), [plain.as_str()], "the miss");

        let now = ask(&pool, &picker, query(&s, "proxy", titles)).await;
        assert_eq!(
            kind(&now, "repo"),
            [auth.as_str(), adproxy.as_str()],
            "titles {titles}"
        );
        let marks: Vec<Vec<u32>> = now.groups[0]
            .hits
            .iter()
            .map(|hit| hit.marks.clone())
            .collect();
        assert_eq!(marks, [vec![10, 15], vec![7, 12]], "titles {titles}");
        for typed in ["mapping service", "service mapping"] {
            let now = ask(&pool, &picker, query(&s, typed, titles)).await;
            let repos = kind(&now, "repo");
            assert!(
                repos.contains(&mapping.as_str()),
                "{typed}, titles {titles}: {repos:?}"
            );
            assert!(
                repos.contains(&plain.as_str()),
                "{typed}, titles {titles}: {repos:?}"
            );
        }
    }
    // Letters in order: rows no word of PostgreSQL holds. `mapping-service`
    // holds them too, shaped, at word starts it scores higher on.
    let r = ask(&pool, &picker, query(&s, "mapsvc", false)).await;
    assert_eq!(
        kind(&r, "repo"),
        [plain.as_str(), mapping.as_str()],
        "{r:?}"
    );
    // `adprx` places every letter in `ADProxy` alone; the one typo the pass
    // then allows (one loose row is under five) brings `auth-proxy` after it.
    let r = ask(&pool, &picker, query(&s, "adprx", false)).await;
    assert_eq!(kind(&r, "repo"), [adproxy.as_str(), auth.as_str()], "{r:?}");
}

/// **`system` finds the `DesignSystem` pipelines' runs**, the scale mirror's
/// own miss: 176 titles hold it and PostgreSQL's words name none.
#[tokio::test]
async fn system_finds_design_system() {
    let pool = pool().await;
    let s = source("system");
    let build = row(
        &pool,
        &s,
        "build",
        "1",
        "Scale test / Web / DesignSystem / Build #5",
        "",
        None,
        at(0),
    )
    .await;
    let picker = picker(&pool).await;
    assert!(
        !holds(
            &Searcher::new(pool.clone())
                .search(query(&s, "system", false))
                .await
                .unwrap(),
            &build
        ),
        "the miss"
    );
    for typed in ["system", "syste", "sys", "designsys"] {
        assert!(
            holds(&ask(&pool, &picker, query(&s, typed, false)).await, &build),
            "{typed}"
        );
    }
}

/// **An exact key stays first.** The picker never fuzzes a key, and the
/// merge moves no row PostgreSQL returned.
#[tokio::test]
async fn an_exact_key_is_first() {
    let pool = pool().await;
    let s = source("key");
    let key = row(
        &pool,
        &s,
        "ticket",
        "PAY-231",
        "Payout retry stalls",
        "",
        None,
        at(-600),
    )
    .await;
    let longer = row(
        &pool,
        &s,
        "ticket",
        "PAY-2310",
        "PAY-231 follow-up",
        "",
        None,
        at(0),
    )
    .await;
    let picker = picker(&pool).await;
    let r = ask(&pool, &picker, query(&s, "PAY-231", false)).await;
    let tickets = kind(&r, "ticket");
    assert_eq!(tickets.first(), Some(&key.as_str()), "{tickets:?}");
    assert!(tickets.contains(&longer.as_str()));
}

/// **Every filter is obeyed**: a kind, a source, a window of days and an
/// author keep the picker's rows as they keep PostgreSQL's.
#[tokio::test]
async fn every_filter_is_obeyed() {
    let pool = pool().await;
    let s = source("filters");
    let other = source("filters-other");
    let repo = row(
        &pool,
        &s,
        "repo",
        "1",
        "acme/ADProxy",
        "",
        Some("mara"),
        Utc::now(),
    )
    .await;
    let old = row(
        &pool,
        &s,
        "ticket",
        "2",
        "Harden ADProxy",
        "",
        Some("jonas"),
        Utc::now() - Duration::days(30),
    )
    .await;
    let elsewhere = row(
        &pool,
        &other,
        "repo",
        "3",
        "acme/ADProxy",
        "",
        None,
        Utc::now(),
    )
    .await;
    let picker = picker(&pool).await;

    let all = ask(&pool, &picker, query(&s, "proxy", false)).await;
    assert!(holds(&all, &repo) && holds(&all, &old), "{all:?}");
    assert!(!holds(&all, &elsewhere), "another source: {all:?}");

    let mut by_kind = query(&s, "proxy", false);
    by_kind.filters.kinds = vec!["repo".to_owned()];
    let r = ask(&pool, &picker, by_kind).await;
    assert!(holds(&r, &repo) && !holds(&r, &old), "{r:?}");

    let mut recent = query(&s, "proxy", false);
    recent.filters.updated_within_days = Some(7);
    let r = ask(&pool, &picker, recent).await;
    assert!(holds(&r, &repo) && !holds(&r, &old), "{r:?}");

    let mut hers = query(&s, "proxy", false);
    hers.filters.authors = vec!["jonas".to_owned()];
    let r = ask(&pool, &picker, hers).await;
    assert!(!holds(&r, &repo) && holds(&r, &old), "{r:?}");

    // The grammar's own keys, typed.
    let r = ask(&pool, &picker, query(&s, "#r proxy", false)).await;
    assert!(holds(&r, &repo) && !holds(&r, &old), "{r:?}");
}

/// **A withdrawn row and a switched-off source are dropped**, though the
/// picker still holds their names: the statement that reads the chosen rows
/// reads them live (D8).
#[tokio::test]
async fn a_withdrawn_row_and_a_switched_off_source_are_dropped() {
    let pool = pool().await;
    let s = source("live");
    let off = source("live-off");
    let kept = row(&pool, &s, "repo", "1", "acme/ADProxy", "", None, at(0)).await;
    let withdrawn = row(&pool, &s, "repo", "2", "beta/ADProxy", "", None, at(0)).await;
    let switched = row(&pool, &off, "repo", "3", "gamma/ADProxy", "", None, at(0)).await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&withdrawn)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, enabled)
         values ($1, 'mock', 'off', 'http://off.invalid', 'none', false)",
    )
    .bind(&off)
    .execute(&pool)
    .await
    .unwrap();
    let picker = picker(&pool).await;
    let both = Limits {
        sources: vec![s.clone(), off.clone()],
    };
    let picked = picker.pick("proxy", &both).ids();
    assert!(picked.contains(&withdrawn) && picked.contains(&switched));
    let mut q = query(&s, "proxy", false);
    q.filters.sources.push(off.clone());
    let r = ask(&pool, &picker, q).await;
    assert_eq!(ids(&r), [("repo".to_owned(), vec![kept])]);
}

/// **Story 25 and 26 hold beside the names**: a body that is about the word
/// stands above the picker's name, a single mention below it.
#[tokio::test]
async fn a_body_about_the_word_stands_above_the_name_and_a_mention_below() {
    let pool = pool().await;
    let s = source("bodies");
    let about = row(
        &pool,
        &s,
        "ticket",
        "1",
        "Ops runbook",
        "proxy: the proxy restarts, the proxy drains, the proxy pool.",
        None,
        at(-120),
    )
    .await;
    let mention = row(
        &pool,
        &s,
        "ticket",
        "2",
        "Quarterly review",
        "Notes from the quarter, budgets, hiring and the roadmap; one line on the proxy.",
        None,
        at(-60),
    )
    .await;
    let name = row(
        &pool,
        &s,
        "ticket",
        "3",
        "Fix ADProxy timeouts",
        "",
        None,
        at(0),
    )
    .await;
    let picker = picker(&pool).await;
    let r = ask(&pool, &picker, query(&s, "proxy", false)).await;
    let tickets = kind(&r, "ticket");
    assert_eq!(
        tickets,
        [about.as_str(), name.as_str(), mention.as_str()],
        "{r:?}"
    );
    // Without the picker the name is not there and the rest is as it was.
    let before = Searcher::new(pool.clone())
        .search(query(&s, "proxy", false))
        .await
        .unwrap();
    assert_eq!(kind(&before, "ticket"), [about.as_str(), mention.as_str()]);
}

/// **Ten a kind, a row once**: many names of one kind fill it to ten and no
/// more, and a row both PostgreSQL and the picker found is drawn once.
#[tokio::test]
async fn ten_a_kind_and_a_row_once() {
    let pool = pool().await;
    let s = source("ten");
    for n in 0..14 {
        row(
            &pool,
            &s,
            "repo",
            &n.to_string(),
            &format!("acme/svc{n}ADProxy"),
            "",
            None,
            at(n),
        )
        .await;
    }
    let both = row(&pool, &s, "repo", "plain", "acme/proxy", "", None, at(-5)).await;
    let picker = picker(&pool).await;
    let r = ask(&pool, &picker, query(&s, "proxy", false)).await;
    let repos = kind(&r, "repo");
    assert_eq!(repos.len(), 10, "{repos:?}");
    assert_eq!(repos.iter().filter(|id| **id == both.as_str()).count(), 1);
    assert_eq!(repos[0], both.as_str());
    assert!(r.groups[0].total >= 10);
}

/// **D3: the names' read.** After an insert, a new title, a delete and a
/// withdrawn entity, a reload picks what the mirror holds now. The read is an
/// index-only scan of `item_ticket_brief_idx` and starts no worker.
#[tokio::test]
async fn a_reload_reads_what_the_mirror_holds() {
    let pool = pool().await;
    let s = source("reload");
    let only = |picker: &Picker, typed: &str| {
        picker
            .pick(
                typed,
                &Limits {
                    sources: vec![s.clone()],
                },
            )
            .ids()
    };
    let first = row(&pool, &s, "repo", "1", "acme/ADProxy", "", None, at(0)).await;
    let picker = picker(&pool).await;
    assert_eq!(only(&picker, "proxy"), std::slice::from_ref(&first));
    let second = row(
        &pool,
        &s,
        "repo",
        "2",
        "acme/abcMappingService",
        "",
        None,
        at(1),
    )
    .await;
    assert!(only(&picker, "mapsvc").is_empty(), "not read yet");
    picker.reload(&pool).await.unwrap();
    assert_eq!(only(&picker, "mapsvc"), std::slice::from_ref(&second));
    sqlx::query("update sync.item set title = 'acme/routing' where entity_id = $1")
        .bind(&first)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from sync.item where entity_id = $1")
        .bind(&second)
        .execute(&pool)
        .await
        .unwrap();
    picker.reload(&pool).await.unwrap();
    assert!(only(&picker, "proxy").is_empty());
    assert!(only(&picker, "mapsvc").is_empty());
    assert_eq!(only(&picker, "routing"), std::slice::from_ref(&first));

    // Planned as the load plans it, on a table whose pages are all visible,
    // with the scan and the bitmap off and workers made to look free: the
    // index answers the read alone, and no worker is asked for.
    sqlx::query("vacuum (analyze) sync.item")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(LOAD_PLANNING).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "select set_config('enable_seqscan', 'off', true), \
                set_config('enable_bitmapscan', 'off', true), \
                set_config('parallel_setup_cost', '0', true), \
                set_config('parallel_tuple_cost', '0', true), \
                set_config('min_parallel_index_scan_size', '0', true), \
                set_config('min_parallel_table_scan_size', '0', true)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let plan: Vec<(String,)> = sqlx::query_as(knobas_search::picker::NAMES_PLAN_SQL)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let plan = plan
        .into_iter()
        .map(|(line,)| line)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        plan.contains("Index Only Scan using item_ticket_brief_idx"),
        "{plan}"
    );
    assert!(
        !plan.contains("Gather") && !plan.contains("Workers"),
        "{plan}"
    );
}

/// **D8: the statement's plan.** Each chosen row through `item_pkey`, no
/// sort of the mirror and no worker, at any size.
#[tokio::test]
async fn the_chosen_rows_are_read_by_their_key() {
    use knobas_search::sql::{explain_kept, picked_sql};
    let pool = pool().await;
    let s = source("plan");
    let mut ids = Vec::new();
    for n in 0..30 {
        ids.push(
            row(
                &pool,
                &s,
                "repo",
                &n.to_string(),
                &format!("acme/r{n}"),
                "",
                None,
                at(n),
            )
            .await,
        );
    }
    let ranks: Vec<f32> = (0..30).map(|n| 1.0 - n as f32 / 100.0).collect();
    let filters = EffectiveFilters {
        sources: vec![s.clone()],
        ..EffectiveFilters::default()
    };
    for at_any_size in [false, true] {
        let plan = explain_kept(
            &pool,
            picked_sql(&ids, &ranks, &filters, 10, 80),
            at_any_size,
        )
        .await
        .unwrap();
        assert!(plan.contains("item_pkey"), "{plan}");
        assert!(!plan.contains("Seq Scan on item "), "{plan}");
        assert!(
            !plan.contains("Gather") && !plan.contains("Workers"),
            "{plan}"
        );
    }
}

/// **D13 point 1: the names read alone**, over a real mirror: the database
/// `KNOBAS_PICKER_DB_URL` names (a copy of the saved scale home's, never the
/// original). Loads the names as the app does, then reads every key from two
/// characters to twenty of the spike's twelve queries, seven times each, and
/// prints the load and the median, the 90th and the worst. Holds the median
/// under 5 ms and the worst under 15 on the Mac. `--ignored`, by hand:
///
/// ```sh
/// KNOBAS_PICKER_DB_URL=postgresql://... cargo test --release -p knobas-search \
///   --test it fuzzy::the_names_read_alone -- --ignored --nocapture
/// ```
#[tokio::test]
#[ignore = "reads a copy of the saved scale database"]
async fn the_names_read_alone() {
    let url =
        std::env::var("KNOBAS_PICKER_DB_URL").expect("KNOBAS_PICKER_DB_URL names a scale copy");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    let picker = Picker::default();
    let mut loads = Vec::new();
    for _ in 0..5 {
        let loaded = picker.reload(&pool).await.unwrap().expect("a load");
        loads.push(loaded.millis);
    }
    loads.sort_unstable();
    let names = picker.reload(&pool).await.unwrap().expect("a load").names;
    println!(
        "load: {names} names, median {} ms of five (min {}, max {})",
        loads[2], loads[0], loads[4]
    );

    let queries = [
        "payout scheduler retry",
        "abcmappingservice",
        "mapping service",
        "idempotency keys audit",
        "scale-platform/observ",
        "sclpay-4988",
        "fix csv import trace",
        "adproxy",
        "mapsvc",
        "obsstack",
        "shceduler",
        "paysched",
    ];
    let limits = Limits::default();
    let mut all: Vec<(f64, String)> = Vec::new();
    for typed in queries {
        let chars: Vec<char> = typed.chars().collect();
        for n in 2..=chars.len().min(20) {
            let part: String = chars[..n].iter().collect();
            if part.ends_with(' ') {
                continue;
            }
            let mut times: Vec<f64> = (0..7)
                .map(|_| {
                    let started = std::time::Instant::now();
                    std::hint::black_box(picker.pick(&part, &limits));
                    started.elapsed().as_secs_f64() * 1e3
                })
                .collect();
            times.sort_by(f64::total_cmp);
            all.push((times[3], part));
        }
    }
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let median = all[all.len() / 2].0;
    let ninetieth = all[all.len() * 9 / 10].0;
    let (worst, slowest) = all.last().cloned().unwrap();
    println!(
        "{} keys: median {median:.2} ms, 90th {ninetieth:.2}, worst {worst:.2} ({slowest:?})",
        all.len()
    );
    for typed in [
        "system",
        "proxy",
        "obsstack",
        "paysched",
        "shceduler",
        "mapsvc",
        "sys",
        "sclpay-4988",
    ] {
        let picked = picker.pick(typed, &limits);
        let solid = picked.rows.iter().filter(|row| row.rank >= 0.0).count();
        println!(
            "{typed:>12}: {solid} solid, {} loose, first {:?}",
            picked.rows.len() - solid,
            picked.rows.first().map(|row| &row.entity_id)
        );
    }
    assert!(
        median < 5.0 && worst < 15.0,
        "median {median:.2}, worst {worst:.2}"
    );
}

/// **A loose row stays loose at the cap.** `paysched` places all eight
/// letters in *Payout scheduler* at a score past sixteen a letter; the row is
/// still drawn loose, marked, and after the kind's solid name.
#[tokio::test]
async fn a_loose_row_at_the_cap_stays_loose_and_marked() {
    let pool = pool().await;
    let s = source("cap");
    let loose = row(
        &pool,
        &s,
        "ticket",
        "1",
        "Payout scheduler has no alerting",
        "",
        None,
        at(0),
    )
    .await;
    let solid = row(
        &pool,
        &s,
        "ticket",
        "2",
        "Rename paysched flag",
        "",
        None,
        at(-600),
    )
    .await;
    let picker = picker(&pool).await;
    let r = ask(&pool, &picker, query(&s, "paysched", false)).await;
    assert_eq!(
        kind(&r, "ticket"),
        [solid.as_str(), loose.as_str()],
        "{r:?}"
    );
    let hit = &r.groups[0].hits[1];
    assert!(hit.rank < 0.0, "{hit:?}");
    assert!(!hit.marks.is_empty(), "{hit:?}");
}

/// **The Docs search reads as the launcher does** (D12 step 3): a page
/// whose name holds the typed words in parts is found, a ticket of the same
/// name is not (pages only), and a word nobody wrote finds the page that says
/// the nearest one. Without a picker it answers as before.
#[tokio::test]
async fn the_docs_search_finds_pages_by_their_names_parts() {
    let pool = pool().await;
    let s = source("docs");
    let tag = format!("zd{}", &s[s.len() - 8..]);
    let page = row(
        &pool,
        &s,
        "page",
        "1",
        &format!("{tag} ADProxy runbook"),
        "",
        None,
        at(0),
    )
    .await;
    let ticket = row(
        &pool,
        &s,
        "ticket",
        "2",
        &format!("{tag} ADProxy upgrade"),
        "",
        None,
        at(0),
    )
    .await;
    let body = row(
        &pool,
        &s,
        "page",
        "3",
        &format!("{tag} Notes"),
        "the pagination tokens expire",
        None,
        at(0),
    )
    .await;
    let picker = picker(&pool).await;
    let typed = format!("proxy {tag}");
    let before = Searcher::new(pool.clone())
        .page_search(&typed, None, 20)
        .await
        .unwrap()
        .expect("words");
    assert!(
        !before.hits.iter().any(|hit| hit.row.entity_id == page),
        "the miss"
    );
    let searcher = Searcher::new(pool.clone()).with_picker(picker.clone());
    let found = searcher
        .page_search(&typed, None, 20)
        .await
        .unwrap()
        .expect("words");
    let ids: Vec<&str> = found
        .hits
        .iter()
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert!(ids.contains(&page.as_str()), "{ids:?}");
    assert!(!ids.contains(&ticket.as_str()), "{ids:?}");
    let corrected = searcher
        .page_search(&format!("{tag} pagintion"), None, 20)
        .await
        .unwrap()
        .expect("words");
    assert!(
        corrected.hits.iter().any(|hit| hit.row.entity_id == body),
        "{corrected:?}"
    );
    assert!(
        corrected
            .needles
            .iter()
            .any(|needle| needle == "pagination"),
        "{:?}",
        corrected.needles
    );
}
