//! `sync.item_search` (migration `0077`): a copy of each mirror row's search
//! vector and its weight-A half, in a narrow table the launcher matches and
//! ranks from.
//!
//! Two things are held here. The copy is the mirror's value after every kind
//! of write, and the launcher's statement answers with the hits it gave, in
//! the order it gave them, when it read `sync.item` alone
//! ([`corpus::LIVE_ITEM_UNSTORED`]), over a corpus written from the scale
//! seed's own words (`testenv/scale/common.py`).
//!
//! The database is shared by this crate's tests, so each test reads the rows
//! of a source it made, except the one check that is true of every row at
//! every instant.

use knobas_search::corpus::{self, LIVE_ITEM, LIVE_ITEM_UNSTORED};
use knobas_search::query::EffectiveFilters;
use knobas_search::sql::{explain_matched, fetch_matched, query_as_with, search_sql};
use knobas_search::{KindCatalog, RawHit, group};

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn source(tag: &str) -> String {
    format!("{tag}-{}", &uuid::Uuid::new_v4().simple().to_string()[..10])
}

// -- the scale seed's vocabulary (`testenv/scale/common.py`) -----------------

const COMPONENTS: &[&str] = &[
    "payout scheduler",
    "ledger writer",
    "settlement batch",
    "refund service",
    "card gateway",
    "fraud scoring",
    "invoice renderer",
    "webhook relay",
    "auth proxy",
    "session store",
    "search indexer",
    "pricing engine",
    "checkout flow",
    "rate limiter",
    "billing cron",
    "SEPA connector",
    "FX quotes",
    "KYC checks",
    "deploy pipeline",
    "secrets rotation",
    "Postgres primary",
    "read replica",
    "Redis cache",
    "partner API",
    "dashboard widgets",
];
const VERBS: &[&str] = &[
    "Fix",
    "Add",
    "Remove",
    "Refactor",
    "Investigate",
    "Document",
    "Migrate",
    "Harden",
    "Retry",
    "Validate",
    "Paginate",
    "Cache",
    "Deprecate",
    "Reconcile",
];
const PROBLEMS: &[&str] = &[
    "times out under load",
    "double-counts partial refunds",
    "drops events after a restart",
    "ignores the retry budget",
    "returns 500 on empty input",
    "shows stale totals",
    "is missing an index",
    "rejects valid IBANs",
    "retries forever",
    "skips the last page",
];
const OBJECTS: &[&str] = &[
    "idempotency keys",
    "the retry queue",
    "structured logs",
    "the health endpoint",
    "the batch window",
    "a feature flag",
    "the migration script",
    "the runbook",
    "the backoff policy",
    "pagination tokens",
];
const WORDS: &str = "the a service request queue batch retry ledger payout customer partner window \
    latency error budget deploy rollback cache index query migration schema event \
    consumer producer timeout threshold alert dashboard metric trace span log \
    config flag release branch review test fixture staging production region \
    replica primary failover backlog incident postmortem owner team sprint \
    estimate scope risk dependency contract API endpoint payload response \
    validation edge case regression benchmark throughput capacity quota \
    is was will should must can may needs runs fails passes reads writes \
    sends drops keeps moves checks blocks waits retries handles returns \
    after before during when while because so but and or with without \
    every some most each one two three new old slow fast large small \
    manual automatic nightly hourly weekly critical minor known open closed";

/// A fixed sequence, so every run seeds the same corpus.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(self.0 >> 33).unwrap()
    }
    fn pick<'a>(&mut self, of: &[&'a str]) -> &'a str {
        of[self.next() % of.len()]
    }
    fn sentence(&mut self, words: &[&str]) -> String {
        let n = 6 + self.next() % 13;
        let mut out: Vec<&str> = (0..n).map(|_| self.pick(words)).collect();
        if self.next() % 5 < 2 {
            let at = self.next() % out.len();
            out.insert(at, self.pick(COMPONENTS));
        }
        out.join(" ") + "."
    }
}

/// What the scale seed's `summary` writes: a component and its problem, or
/// a verb and what it is done to.
fn title(rng: &mut Rng) -> String {
    if rng.next() % 10 < 3 {
        format!("{} {}", rng.pick(COMPONENTS), rng.pick(PROBLEMS))
    } else {
        format!(
            "{} {} in the {}",
            rng.pick(VERBS),
            rng.pick(OBJECTS),
            rng.pick(COMPONENTS)
        )
    }
}

/// The corpus: `tickets` tickets with a key, a third as many pull requests
/// under `owner/repo` titles' key cells, and a fifth as many pages, each
/// page's body long enough that its vector is stored out of line (over
/// 2 kB), as a real page's is. The source they were written under.
async fn seed_scale_words(pool: &sqlx::PgPool, tickets: usize) -> String {
    let source = source("scalewords");
    let words: Vec<&str> = WORDS.split_whitespace().collect();
    let mut rng = Rng(0x5ca1e);
    let (mut ids, mut kinds, mut titles, mut bodies, mut payloads) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for n in 0..tickets {
        let project = ["PAY", "OPS", "WEB"][n % 3];
        ids.push(format!("{source}:{project}-{}", n + 1));
        kinds.push("ticket");
        titles.push(title(&mut rng));
        let sentences = 2 + rng.next() % 6;
        bodies.push(
            (0..sentences)
                .map(|_| rng.sentence(&words))
                .collect::<Vec<_>>()
                .join(" "),
        );
        payloads.push(serde_json::json!({}));
    }
    for n in 0..tickets / 3 {
        let repo = rng.pick(COMPONENTS).replace(' ', "-").to_lowercase();
        ids.push(format!("{source}:tidewater/{repo}#{}", n + 1));
        kinds.push("pr");
        titles.push(title(&mut rng));
        bodies.push(rng.sentence(&words));
        payloads.push(serde_json::json!({}));
    }
    for n in 0..tickets / 5 {
        ids.push(format!("{source}:page-{}", n + 1));
        kinds.push("page");
        titles.push(format!("{} runbook", rng.pick(COMPONENTS)));
        let sentences = 150 + rng.next() % 150;
        bodies.push(
            (0..sentences)
                .map(|_| rng.sentence(&words))
                .collect::<Vec<_>>()
                .join(" "),
        );
        payloads.push(serde_json::json!({}));
    }
    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select * from unnest($1::text[], $2::text[], $3::text[])",
    )
    .bind(&ids)
    .bind(&kinds)
    .bind(&titles)
    .execute(pool)
    .await
    .unwrap();
    // Times a few seconds apart and in runs of equal ones, so the order's
    // third key and its tie on the id are both exercised.
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         select t.id, $6, t.kind, t.title, t.body, t.payload,
                timestamptz '2026-11-01 12:00+00' - make_interval(secs => ((t.n / 4) * 7)::int)
           from unnest($1::text[], $2::text[], $3::text[], $4::text[], $5::jsonb[])
                with ordinality as t(id, kind, title, body, payload, n)",
    )
    .bind(&ids)
    .bind(&kinds)
    .bind(&titles)
    .bind(&bodies)
    .bind(&payloads)
    .bind(&source)
    .execute(pool)
    .await
    .unwrap();
    source
}

/// How many mirror rows have no copy, or a copy that is not their vector
/// and its weight-A half: of `source`, or of the whole mirror.
async fn out_of_step(pool: &sqlx::PgPool, source: Option<&str>) -> i64 {
    sqlx::query_scalar(
        "select count(*) from sync.item i
           left join sync.item_search s on s.entity_id = i.entity_id
          where ($1::text is null or i.source_id = $1)
            and (s.entity_id is null
                 or s.fts is distinct from i.fts
                 or s.fts_a is distinct from ts_filter(i.fts, '{a}'))",
    )
    .bind(source)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// One answer of the statement: per kind, its total and its hits' ids and
/// ranks, in the order they came back.
type Answer = Vec<(String, u32, Vec<(String, f32)>)>;

fn answer(rows: Vec<RawHit>) -> Answer {
    group::group(rows, &KindCatalog::default())
        .into_iter()
        .map(|g| {
            let hits = g
                .hits
                .into_iter()
                .map(|h| (h.row.entity_id, h.rank))
                .collect();
            (g.kind, g.total, hits)
        })
        .collect()
}

/// **The same hits in the same order.** Every probe below is asked twice:
/// of the mirror as the launcher reads it now, through the narrow table and
/// with sequential scans off, and of the mirror read as before `0077`. Under
/// both scopes, whole and while the last word is still being typed, for the
/// launcher's page (ten a kind) and for every match there is.
///
/// The probes are the scale seed's own words: one letter and the prefixes a
/// reader types on the way to a word, words half the corpus holds, a word
/// only bodies hold, two words, a phrase, a word left out, alternatives, a
/// ticket's key and a stop word.
#[tokio::test]
async fn the_launcher_answers_as_it_did_reading_the_mirror_alone() {
    let pool = pool().await;
    let source = seed_scale_words(&pool, 900).await;
    assert_eq!(out_of_step(&pool, Some(&source)).await, 0);
    let wide: i64 = sqlx::query_scalar(
        "select count(*) from sync.item where source_id = $1 and pg_column_size(fts) > 2000",
    )
    .bind(&source)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(wide >= 100, "the pages' vectors are wide ones: {wide}");

    let probes = [
        "r",
        "p",
        "pa",
        "pay",
        "retr",
        "retry",
        "deploy",
        "payout",
        "postmortem",
        "fx quotes",
        "retry budget",
        "\"retry queue\"",
        "retry -queue",
        "payout or ledger",
        "PAY-12",
        "runbook",
        "the",
    ];
    let mut compared = 0;
    // The probes that matched a row while typed, under *Everything*.
    let mut matched = Vec::new();
    for probe in probes {
        for titles in [false, true] {
            for prefix_last in [false, true] {
                for (per_group, limit) in [(10, 50), (100_000, 100_000)] {
                    let filters = EffectiveFilters {
                        sources: vec![source.clone()],
                        titles,
                        ..EffectiveFilters::default()
                    };
                    let now = search_sql(
                        &[&LIVE_ITEM],
                        Some(probe),
                        prefix_last,
                        &filters,
                        per_group,
                        limit,
                    );
                    let before = search_sql(
                        &[&LIVE_ITEM_UNSTORED],
                        Some(probe),
                        prefix_last,
                        &filters,
                        per_group,
                        limit,
                    );
                    let now = answer(fetch_matched(&pool, now).await.unwrap());
                    let before = answer(query_as_with(before).fetch_all(&pool).await.unwrap());
                    assert_eq!(
                        now, before,
                        "{probe:?}, titles {titles}, typing {prefix_last}, {per_group} a kind"
                    );
                    compared += 1;
                    if !titles && prefix_last && !now.is_empty() && !matched.contains(&probe) {
                        matched.push(probe);
                    }
                }
            }
        }
    }
    // Not vacuous: every probe but the stop word matches rows while typed.
    assert_eq!(compared, probes.len() * 8);
    assert_eq!(
        matched,
        probes[..probes.len() - 1],
        "the probes that matched"
    );
}

/// The copy is the mirror's vector after every kind of write: an insert, a
/// retitle, a new body, a payload that leaves the vector alone, a stamp and
/// a delete. And at any instant no row of the whole mirror is out of step
/// with its copy, whatever the other tests of this file are writing.
#[tokio::test]
async fn the_copy_follows_the_mirror_under_every_write() {
    let pool = pool().await;
    let source = seed_scale_words(&pool, 60).await;
    assert_eq!(out_of_step(&pool, Some(&source)).await, 0, "inserted");
    let copies = |pool: sqlx::PgPool, source: String| async move {
        sqlx::query_scalar::<_, i64>(
            "select count(*) from sync.item_search s where s.entity_id like $1 || ':%'",
        )
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    let rows = copies(pool.clone(), source.clone()).await;
    assert_eq!(rows, 60 + 20 + 12);

    for (what, statement) in [
        (
            "retitled",
            "update sync.item set title = title || ' zzrenamed' where source_id = $1 and kind = 'ticket'",
        ),
        (
            "a new body",
            "update sync.item set body_text = 'zzbody ' || body_text where source_id = $1 and kind = 'page'",
        ),
        (
            "a payload that changes no word",
            "update sync.item set payload = '{\"n\": 1}' where source_id = $1",
        ),
        (
            "stamped",
            "update sync.item set synced_at = now() + interval '1 second' where source_id = $1",
        ),
    ] {
        sqlx::query(statement)
            .bind(&source)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(out_of_step(&pool, Some(&source)).await, 0, "{what}");
    }
    // The retitle reached the weight-A half, and only the tickets'.
    let renamed: i64 = sqlx::query_scalar(
        "select count(*) from sync.item_search s
          where s.entity_id like $1 || ':%' and s.fts_a @@ to_tsquery('english', 'zzrenamed')",
    )
    .bind(&source)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(renamed, 60);

    sqlx::query("delete from sync.item where source_id = $1 and kind = 'pr'")
        .bind(&source)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(copies(pool.clone(), source.clone()).await, rows - 20);
    assert_eq!(out_of_step(&pool, None).await, 0, "the whole mirror");
}

/// A *Titles* search reads the rows the word names from the title index and
/// no others, and neither scope scans the mirror or the narrow table whole.
/// Asked of every source, as the launcher asks: narrowed to one small
/// source, the planner rightly starts from that source's rows instead.
#[tokio::test]
async fn a_search_with_words_reads_through_the_narrow_tables_indexes() {
    let pool = pool().await;
    seed_scale_words(&pool, 300).await;
    // The planner's statistics, as the migration leaves them for the narrow
    // table and a sync's analyze pass leaves them for the rest.
    sqlx::query("analyze sync.item_search, sync.item, knobas.entity")
        .execute(&pool)
        .await
        .unwrap();
    for (titles, index) in [
        (true, "item_search_fts_a_idx"),
        (false, "item_search_fts_idx"),
    ] {
        let built = search_sql(
            corpus::ALL,
            Some("retry"),
            true,
            &EffectiveFilters {
                titles,
                ..EffectiveFilters::default()
            },
            10,
            50,
        );
        let plan = explain_matched(&pool, built).await.unwrap();
        assert!(plan.contains(index), "titles {titles}:\n{plan}");
        assert!(
            !plan.contains("Seq Scan on item ") && !plan.contains("Seq Scan on item_search"),
            "titles {titles}:\n{plan}"
        );
        // *Titles* never reads the body matches to keep the titles.
        assert_eq!(
            plan.contains("item_search_fts_idx"),
            !titles,
            "titles {titles}:\n{plan}"
        );
    }
}
