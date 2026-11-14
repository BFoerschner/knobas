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
//! Since migration `0084` the copy also holds what the match stage reads of
//! a row beside its vector (its kind, source, date and the words it is named
//! by), and on a mirror whose every copy holds them the stage reads the
//! narrow table alone ([`corpus::LIVE_ITEM_FILLED`]): the columns held to the
//! mirror as the vector is, the stage to the one that joins the mirror
//! ([`LIVE_ITEM`]), and a search to the same answer while some copies are
//! not filled yet.
//!
//! The database is shared by this crate's tests, so each test reads the rows
//! of a source it made, except the one check that is true of every row at
//! every instant.

use knobas_search::corpus::{self, LIVE_ITEM, LIVE_ITEM_FILLED, LIVE_ITEM_UNSTORED};
use knobas_search::query::EffectiveFilters;
use knobas_search::sql::{
    explain_matched, fetch_matched, fetch_matched_by_a_letter, query_as_with, search_sql,
};
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
/// and its weight-A half, or whose six columns of migration `0084` are not
/// the mirror row's: of `source`, or of the whole mirror. A copy whose
/// `kind` is null holds none of the six yet (a row the fill has not
/// reached, which one test here makes on purpose) and is in step.
async fn out_of_step(pool: &sqlx::PgPool, source: Option<&str>) -> i64 {
    sqlx::query_scalar(
        "select count(*) from sync.item i
           left join sync.item_search s on s.entity_id = i.entity_id
          where ($1::text is null or i.source_id = $1)
            and (s.entity_id is null
                 or s.fts is distinct from i.fts
                 or s.fts_a is distinct from ts_filter(i.fts, '{a}')
                 or (s.kind is not null
                     and (s.kind, s.source_id, s.item_updated_at,
                          s.title_words, s.title_tail_words, s.key_words)
                         is distinct from
                         (i.kind, i.source_id, i.item_updated_at,
                          i.title_words, i.title_tail_words, i.key_words)))",
    )
    .bind(source)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// How many of `source`'s copies hold none of the six columns yet.
async fn unfilled(pool: &sqlx::PgPool, source: &str) -> i64 {
    sqlx::query_scalar(
        "select count(*) from sync.item_search s
          where s.entity_id like $1 || ':%' and s.kind is null",
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

/// **Each kind's best rows are the ones the window kept.** The statement
/// cuts a kind to its best rows with an ordered limit a kind
/// (`sql::KindCut::Limit`), where it numbered every match in a window over
/// the whole set; `sql::search_sql_windowed` still builds that statement,
/// and every probe is asked both ways: under both scopes, whole and typed,
/// with one row a kind, three, the launcher's ten and every match, cut to a
/// page shorter than the kinds fill and to one that holds them all, and
/// browsed with no words at all.
#[tokio::test]
async fn a_kind_is_cut_to_the_rows_the_window_kept() {
    use knobas_search::sql::search_sql_windowed;

    let pool = pool().await;
    let source = seed_scale_words(&pool, 600).await;
    let kinds: i64 =
        sqlx::query_scalar("select count(distinct kind) from sync.item where source_id = $1")
            .bind(&source)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(kinds >= 2, "more than one kind to cut: {kinds}");

    let probes: [Option<&str>; 12] = [
        Some("r"),
        Some("p"),
        Some("pay"),
        Some("retry"),
        Some("deploy"),
        Some("fx quotes"),
        Some("\"retry queue\""),
        Some("retry -queue"),
        Some("payout or ledger"),
        Some("PAY-12"),
        Some("the"),
        // Browse mode: no words, ordered by recency.
        None,
    ];
    let mut compared = 0;
    let mut cut = 0;
    for probe in probes {
        for titles in [false, true] {
            for prefix_last in [false, true] {
                for (per_group, limit) in [(1, 50), (3, 5), (10, 50), (100_000, 100_000)] {
                    let filters = EffectiveFilters {
                        sources: vec![source.clone()],
                        titles,
                        ..EffectiveFilters::default()
                    };
                    let now = search_sql(
                        &[&LIVE_ITEM],
                        probe,
                        prefix_last,
                        &filters,
                        per_group,
                        limit,
                    );
                    let windowed = search_sql_windowed(
                        &[&LIVE_ITEM],
                        probe,
                        prefix_last,
                        &filters,
                        per_group,
                        limit,
                    );
                    let now = answer(fetch_matched(&pool, now).await.unwrap());
                    let windowed = answer(fetch_matched(&pool, windowed).await.unwrap());
                    assert_eq!(
                        now, windowed,
                        "{probe:?}, titles {titles}, typing {prefix_last}, {per_group} a kind, {limit} in all"
                    );
                    compared += 1;
                    // A kind that matched more rows than it was let draw.
                    cut += usize::from(
                        now.iter()
                            .any(|(_, total, hits)| (*total as usize) > hits.len()),
                    );
                }
            }
        }
    }
    assert_eq!(compared, probes.len() * 16);
    assert!(
        cut * 2 >= compared,
        "{cut} of {compared} answers cut a kind short"
    );
}

/// The copy is the mirror's vector, and its six columns the mirror row's,
/// after every kind of write: an insert, a retitle, a new body, a payload
/// that leaves the vector alone, a stamp, a date that moves or goes with no
/// word changed, a kind that changes and a delete. A stamp rewrites no copy.
/// And at any instant no row of the whole mirror is out of step with its
/// copy, whatever the other tests of this file are writing.
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
        // The six columns beside the vector (migration `0084`): a date that
        // moves with no word changed, one that goes, a kind that changes.
        (
            "a date that moved",
            "update sync.item set item_updated_at = item_updated_at + interval '3 hours' where source_id = $1 and kind = 'ticket'",
        ),
        (
            "a date that went",
            "update sync.item set item_updated_at = null where source_id = $1 and kind = 'page'",
        ),
        (
            "another kind",
            "update sync.item set kind = 'pull_request' where source_id = $1 and kind = 'pr'",
        ),
    ] {
        sqlx::query(statement)
            .bind(&source)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(out_of_step(&pool, Some(&source)).await, 0, "{what}");
        assert_eq!(unfilled(&pool, &source).await, 0, "{what}");
    }
    // The date reached the copy, and a stamp writes no copy at all: the same
    // row versions before it and after.
    let moved: i64 = sqlx::query_scalar(
        "select count(*) from sync.item_search s join sync.item i on i.entity_id = s.entity_id
          where i.source_id = $1 and i.kind = 'ticket'
            and s.item_updated_at > timestamptz '2026-11-01 14:00+00'",
    )
    .bind(&source)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(moved, 60);
    let versions = |pool: sqlx::PgPool, source: String| async move {
        sqlx::query_scalar::<_, Vec<String>>(
            "select array_agg(s.ctid::text || ' ' || s.xmin::text order by s.entity_id)
               from sync.item_search s where s.entity_id like $1 || ':%'",
        )
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    let before = versions(pool.clone(), source.clone()).await;
    sqlx::query(
        "update sync.item set synced_at = now() + interval '2 seconds' where source_id = $1",
    )
    .bind(&source)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        versions(pool.clone(), source.clone()).await,
        before,
        "a stamp rewrites no search copy"
    );
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

    sqlx::query("delete from sync.item where source_id = $1 and kind = 'pull_request'")
        .bind(&source)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(copies(pool.clone(), source.clone()).await, rows - 20);
    assert_eq!(out_of_step(&pool, None).await, 0, "the whole mirror");
}

/// **The match stage off the narrow table alone answers as the one joined to
/// the mirror does** (migration `0084`), and as the mirror read alone does:
/// every probe under both scopes, whole and typed, the launcher's page and
/// every match, with the dimensions the narrow table answers (sources,
/// kinds, a kind left out, a kind answered as a pipeline, a window of days),
/// over a mirror with entities withdrawn and a source switched off.
///
/// And a search is the same search while copies are unfilled, as rows
/// written before the migration are until the fill reaches them: with every
/// other copy unfilled and with none filled, [`corpus::mirror`] hands out
/// the mirror joined, on which a search finds every row; read off the narrow
/// table then, the unfilled rows would be gone. The fill leaves no copy
/// unfilled and none out of step, and the narrow table is read again.
#[tokio::test]
async fn the_match_stage_reads_the_narrow_table_as_it_read_the_mirror() {
    use knobas_search::{SearchFilters, SearchQuery, Searcher};

    let pool = pool().await;
    let source = seed_scale_words(&pool, 600).await;
    // A second source, switched off: its rows match every probe and no
    // answer holds one.
    let off = seed_scale_words(&pool, 60).await;
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config, enabled)
         values ($1, 'jira', $1, 'http://x', 'Pat', '{}', false)",
    )
    .bind(&off)
    .execute(&pool)
    .await
    .unwrap();
    // One row in seven withdrawn.
    sqlx::query(
        "update knobas.entity e set deleted_at = now()
           from (select entity_id, row_number() over (order by entity_id) as n
                   from sync.item where source_id = $1) i
          where e.id = i.entity_id and i.n % 7 = 0",
    )
    .bind(&source)
    .execute(&pool)
    .await
    .unwrap();
    // Dates across three weeks, so a window of days keeps some; and a word
    // only these two sources' rows hold, for a search of the whole mirror
    // that other tests' rows cannot answer.
    let word = format!("zq{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
    sqlx::query(
        "update sync.item i
            set item_updated_at = now() - make_interval(hours => (x.n % 500)::int),
                body_text = i.body_text || ' ' || $2
           from (select entity_id, row_number() over (order by entity_id) as n
                   from sync.item where source_id = any($1)) x
          where i.entity_id = x.entity_id",
    )
    .bind(vec![source.clone(), off.clone()])
    .bind(&word)
    .execute(&pool)
    .await
    .unwrap();
    // Two tickets that hold one key at the same weight, once each: the first
    // has it for its key, the second, the newer, in its title. Equal ranks,
    // so the one **named** by what was typed comes first, which the copy's
    // `key_words` say.
    let key = format!("ZK{}-7", &uuid::Uuid::new_v4().simple().to_string()[..6]).to_uppercase();
    let keyed = format!("{source}:{key}");
    let titled = format!("{source}:ZZ-{}", &key[2..8]);
    sqlx::query(
        "insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'a'), ($2, 'ticket', 'b')",
    )
    .bind(&keyed)
    .bind(&titled)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1, $3, 'ticket', 'Settles late', '', '{}', now() - interval '2 days'),
                ($2, $3, 'ticket', $4, '', '{}', now())",
    )
    .bind(&keyed)
    .bind(&titled)
    .bind(&source)
    .bind(format!("{key} beta"))
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(out_of_step(&pool, Some(&source)).await, 0);
    assert_eq!(unfilled(&pool, &source).await, 0, "the triggers fill");
    for corpus in [&LIVE_ITEM_FILLED, &LIVE_ITEM, &LIVE_ITEM_UNSTORED] {
        let built = search_sql(
            &[corpus],
            Some(&key),
            false,
            &EffectiveFilters {
                sources: vec![source.clone()],
                ..EffectiveFilters::default()
            },
            10,
            50,
        );
        let hits = answer(fetch_matched(&pool, built).await.unwrap());
        let [(_, 2, tickets)] = &hits[..] else {
            panic!("both tickets hold the key: {hits:?}");
        };
        assert_eq!(tickets[0].1, tickets[1].1, "equal ranks: {tickets:?}");
        assert_eq!(
            tickets[0].0, keyed,
            "the ticket named by the key comes first"
        );
    }

    let probes = [
        "r",
        "p",
        "pa",
        "retry",
        "deploy",
        "fx quotes",
        "\"retry queue\"",
        "retry -queue",
        "payout or ledger",
        "PAY-12",
        "runbook",
        "the",
    ];
    let both = vec![source.clone(), off.clone()];
    let dimensions = |n: usize| -> EffectiveFilters {
        let base = EffectiveFilters {
            sources: both.clone(),
            ..EffectiveFilters::default()
        };
        match n {
            0 => base,
            1 => EffectiveFilters {
                kinds: vec!["ticket".to_owned(), "page".to_owned()],
                ..base
            },
            2 => EffectiveFilters {
                hidden_kinds: vec!["pr".to_owned()],
                pipeline_kinds: vec!["page".to_owned()],
                ..base
            },
            _ => EffectiveFilters {
                updated_within_days: Some(7),
                ..base
            },
        }
    };
    // Every probe, off the narrow table, joined to the mirror, and of the
    // mirror alone. How many answers held a row.
    let ask = async || -> usize {
        let mut kept = 0;
        for probe in probes {
            for n in 0..4 {
                for titles in [false, true] {
                    for prefix_last in [false, true] {
                        let (per_group, limit) = if n % 2 == 0 {
                            (10, 50)
                        } else {
                            (100_000, 100_000)
                        };
                        let filters = EffectiveFilters {
                            titles,
                            ..dimensions(n)
                        };
                        let statement = |corpus: &'static corpus::Corpus| {
                            search_sql(
                                &[corpus],
                                Some(probe),
                                prefix_last,
                                &filters,
                                per_group,
                                limit,
                            )
                        };
                        assert!(
                            statement(&LIVE_ITEM_FILLED)
                                .sql()
                                .contains("from sync.item_search sx cross join q"),
                            "the match stage reads the narrow table alone"
                        );
                        assert!(!statement(&LIVE_ITEM).sql().contains("sx.kind"));
                        let narrow = answer(
                            fetch_matched(&pool, statement(&LIVE_ITEM_FILLED))
                                .await
                                .unwrap(),
                        );
                        let joined =
                            answer(fetch_matched(&pool, statement(&LIVE_ITEM)).await.unwrap());
                        let alone = answer(
                            query_as_with(statement(&LIVE_ITEM_UNSTORED))
                                .fetch_all(&pool)
                                .await
                                .unwrap(),
                        );
                        let what = format!(
                            "{probe:?}, dimensions {n}, titles {titles}, typing {prefix_last}"
                        );
                        assert_eq!(narrow, joined, "{what}: as the stage joined to the mirror");
                        assert_eq!(narrow, alone, "{what}: as the mirror read alone");
                        // One character is read with the narrow table whole.
                        if probe.chars().count() == 1 {
                            assert!(statement(&LIVE_ITEM_FILLED).matches_narrowly());
                            let whole = answer(
                                fetch_matched_by_a_letter(&pool, statement(&LIVE_ITEM_FILLED))
                                    .await
                                    .unwrap(),
                            );
                            assert_eq!(whole, narrow, "{what}: read whole");
                        }
                        kept += usize::from(!narrow.is_empty());
                    }
                }
            }
        }
        kept
    };
    let asked = probes.len() * 16;
    let kept = ask().await;
    assert!(kept * 2 > asked, "{kept} of {asked} answers held a row");

    // A search of the whole mirror for the word only these rows hold: what
    // the launcher's own path answers, whichever relation it reads.
    let searcher = Searcher::new(pool.clone());
    let search = async || -> (u32, Vec<String>) {
        let response = searcher
            .search(SearchQuery {
                raw: format!("{word} retry"),
                titles: false,
                limit: 200,
                filters: SearchFilters::default(),
            })
            .await
            .unwrap();
        let ids = response
            .groups
            .iter()
            .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
            .collect();
        (response.total, ids)
    };
    let narrowly = async || -> usize {
        let built = search_sql(
            &[&LIVE_ITEM_FILLED],
            Some(&word),
            false,
            &EffectiveFilters::default(),
            100_000,
            100_000,
        );
        fetch_matched::<RawHit>(&pool, built).await.unwrap().len()
    };
    let is_filled =
        async || -> bool { corpus::mirror(&pool).await.unwrap().relation_is_narrowed() };
    let found = search().await;
    assert!(found.0 > 50, "the search found the rows: {}", found.0);
    assert!(found.1.iter().all(|id| id.starts_with(&source)));
    let every = narrowly().await;
    assert!(every > 400, "{every} live rows hold the word");

    // Rows written before the migration: none of the six columns.
    let unfill = "update sync.item_search s
            set kind = null, source_id = null, item_updated_at = null,
                title_words = null, title_tail_words = null, key_words = null
           from (select entity_id, row_number() over (order by entity_id) as n
                   from sync.item where source_id = any($1)) i
          where s.entity_id = i.entity_id and i.n % $2 = 0";
    for (every_nth, when) in [(2_i64, "every other copy unfilled"), (1, "no copy filled")] {
        sqlx::query(unfill)
            .bind(&both)
            .bind(every_nth)
            .execute(&pool)
            .await
            .unwrap();
        assert!(unfilled(&pool, &source).await > 300, "{when}");
        assert!(!is_filled().await, "{when}: the mirror is read joined");
        assert_eq!(search().await, found, "{when}: the same search");
        // Not a thing the data could not show: off the narrow table the
        // unfilled rows are gone.
        assert!(narrowly().await < every, "{when}");
        assert_eq!(out_of_step(&pool, Some(&source)).await, 0, "{when}");
    }

    // The fill: every copy filled, in batches, and none out of step.
    let mut after = String::new();
    let mut batches = 0;
    while let Some(next) =
        sqlx::query_scalar::<_, Option<String>>("select sync.fill_item_search($1, 100)")
            .bind(&after)
            .fetch_one(&pool)
            .await
            .unwrap()
    {
        after = next;
        batches += 1;
    }
    assert!(batches > 5, "{batches} batches");
    assert_eq!(unfilled(&pool, &source).await, 0, "filled");
    assert_eq!(unfilled(&pool, &off).await, 0, "filled");
    assert_eq!(out_of_step(&pool, Some(&source)).await, 0, "filled");
    assert!(is_filled().await, "filled: the narrow table is read again");
    assert_eq!(search().await, found, "filled again");
    assert_eq!(narrowly().await, every, "filled again");
    assert_eq!(ask().await, kept, "filled again");
}

/// The app's fill (`knobas_core::item_search::fill`, run behind bring-up):
/// every copy written before `0084` filled, a batch at a time, the table
/// vacuumed, and a second run finds nothing to do. On a database of its own:
/// it fills every row there is, and the test above keeps some unfilled.
#[tokio::test]
async fn the_fill_fills_every_copy_once() {
    let pool = knobas_db::test_util::scratch_database("item_search_fill")
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    let source = seed_scale_words(&pool, 450).await;
    let rows: i64 = sqlx::query_scalar("select count(*) from sync.item_search")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 450 + 150 + 90);
    assert_eq!(
        knobas_core::item_search::fill(&pool).await.unwrap(),
        0,
        "the triggers filled every copy: nothing to do"
    );
    sqlx::query(
        "update sync.item_search
            set kind = null, source_id = null, item_updated_at = null,
                title_words = null, title_tail_words = null, key_words = null",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(unfilled(&pool, &source).await, rows);
    assert!(
        !corpus::mirror(&pool).await.unwrap().relation_is_narrowed(),
        "unfilled: the mirror is read joined"
    );
    let batches = knobas_core::item_search::fill(&pool).await.unwrap();
    assert_eq!(
        i64::from(batches),
        (rows + i64::from(knobas_core::item_search::BATCH) - 1)
            / i64::from(knobas_core::item_search::BATCH),
        "every copy, a batch at a time"
    );
    assert_eq!(unfilled(&pool, &source).await, 0);
    assert_eq!(out_of_step(&pool, None).await, 0);
    assert!(corpus::mirror(&pool).await.unwrap().relation_is_narrowed());
    assert_eq!(knobas_core::item_search::fill(&pool).await.unwrap(), 0);
}

/// A query that asks what the narrow table does not hold is matched through
/// the mirror's own rows, as before `0084`: an author, a parent, a reach, a
/// name glob, a ticket key. And a browse, which matches nothing.
#[tokio::test]
async fn a_query_the_narrow_table_cannot_answer_reads_the_mirror() {
    use knobas_search::query::{FieldFilter, FieldKey};
    let base = EffectiveFilters::default();
    let wide = [
        EffectiveFilters {
            names: vec!["pay*".to_owned()],
            ..base.clone()
        },
        EffectiveFilters {
            under: vec!["pay-1".to_owned()],
            ..base.clone()
        },
        EffectiveFilters {
            reach: vec!["pay-1".to_owned()],
            ..base.clone()
        },
        EffectiveFilters {
            mine: true,
            ..base.clone()
        },
        EffectiveFilters {
            fields: vec![FieldFilter {
                key: FieldKey::Status,
                negated: false,
                values: vec!["done".to_owned()],
                within_days: None,
                me: Vec::new(),
            }],
            ..base.clone()
        },
    ];
    let joined = "from sync.live_item i join sync.item_search sx";
    let narrow = "from sync.item_search sx cross join q";
    for filters in &wide {
        let built = search_sql(&[&LIVE_ITEM_FILLED], Some("retry"), true, filters, 10, 50);
        assert!(built.sql().contains(joined), "{filters:?}");
        assert!(!built.sql().contains(narrow), "{filters:?}");
        assert!(!built.matches_narrowly(), "{filters:?}");
    }
    let browse = search_sql(
        &[&LIVE_ITEM_FILLED],
        None,
        false,
        &EffectiveFilters {
            sources: vec!["jira".to_owned()],
            ..base.clone()
        },
        10,
        50,
    );
    assert!(browse.sql().contains(joined) && !browse.sql().contains(narrow));
    assert!(!browse.matches_narrowly());
    let typed = search_sql(&[&LIVE_ITEM_FILLED], Some("retry"), true, &base, 10, 50);
    assert!(typed.sql().contains(narrow) && typed.matches_narrowly());
    // And the mirror as it is read while a copy is unfilled never is.
    let unfilled = search_sql(&[&LIVE_ITEM], Some("retry"), true, &base, 10, 50);
    assert!(unfilled.sql().contains(joined) && !unfilled.sql().contains(narrow));
    assert!(!unfilled.matches_narrowly());
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
