//! One typed character (the one-letter pass, 2026-11-15;
//! `docs/decisions/2026-11-15-one-letter-is-answered-by-names.md`): the launcher answers it
//! with the names that hold a word beginning with it, those that begin with
//! it first, then the newest, read off `sync.item_initial` (migration
//! `0093`) a kind's first rows at a time.
//!
//! Four things are held here. The answer as decided, on rows written for
//! it. The bounded read to the full ranking it stands for
//! ([`letter_sql_unstored`]), over the scale seed's words and after every
//! kind of write. The stored characters to the mirror's rows. And every
//! query the names do not answer to the answer it had before
//! ([`Searcher::search_by_words`]). Two and three characters, which names
//! answer since 2026-11-16, are held in `short_words.rs`.
//!
//! This module has a database of its own (`knobas_db::test_util` keys it by
//! file), shared by its tests, so each test reads the rows of a source it
//! made.

use chrono::{DateTime, Duration, TimeZone, Utc};
use knobas_search::corpus::{LIVE_ITEM, LIVE_ITEM_FILLED};
use knobas_search::query::EffectiveFilters;
use knobas_search::sql::{
    explain_named_at_any_size, fetch_matched, fetch_named, letter_sql, letter_sql_unstored,
    query_as_with, search_sql,
};
use knobas_search::{
    KindCatalog, RawHit, SearchFilters, SearchQuery, SearchResponse, Searcher, group,
};

use super::item_search::{seed_scale_words, source};

pub(super) async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

pub(super) fn at(minutes: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0).unwrap() + Duration::minutes(minutes)
}

/// One mirror row of `source`, its id `<source>:<key>`.
pub(super) async fn item(
    pool: &sqlx::PgPool,
    source: &str,
    kind: &str,
    key: &str,
    title: &str,
    body: &str,
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
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         values ($1, $2, $3, $4, $5, '{}'::jsonb, $6)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(updated)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// The launcher's answer to `raw` over `source`'s rows.
pub(super) async fn ask(
    pool: &sqlx::PgPool,
    source: &str,
    raw: &str,
    titles: bool,
) -> SearchResponse {
    Searcher::new(pool.clone())
        .search(query(source, raw, titles))
        .await
        .unwrap()
}

pub(super) fn query(source: &str, raw: &str, titles: bool) -> SearchQuery {
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

/// Each group as `(kind, total, its hits' ids and ranks in order)`.
pub(super) type Answer = Vec<(String, u32, Vec<(String, f32)>)>;

pub(super) fn read(response: &SearchResponse) -> Answer {
    response
        .groups
        .iter()
        .map(|g| {
            let hits = g
                .hits
                .iter()
                .map(|h| (h.row.entity_id.clone(), h.rank))
                .collect();
            (g.kind.clone(), g.total, hits)
        })
        .collect()
}

pub(super) fn answer(rows: Vec<RawHit>) -> Answer {
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

/// [`Answer`] without its totals: a bounded read counts the rows it holds.
pub(super) fn hits(answer: &Answer) -> Vec<(String, Vec<(String, f32)>)> {
    answer
        .iter()
        .map(|(kind, _, hits)| (kind.clone(), hits.clone()))
        .collect()
}

/// **The answer as decided.** A row answers a character when a word of its
/// title or key begins with it, never by its body; the rows a name of which
/// begins with it come first (a title past its stop words, the text after a
/// title's last `/`, a ticket's key), then the newest; under *Titles* and
/// under *Everything* alike, typed in either case. A stop-word letter
/// answers as any other, where ranked as a word it answered nothing.
#[tokio::test]
async fn one_character_is_answered_by_names_leading_first_then_newest() {
    let pool = pool().await;
    let s = source("names");
    let scheduler = item(
        &pool,
        &s,
        "ticket",
        "OPS-1",
        "Payout scheduler times out",
        "",
        at(5),
    )
    .await;
    // The newest of all, and no name of it begins with `p`.
    let rounding = item(
        &pool,
        &s,
        "ticket",
        "OPS-2",
        "Fix the payout rounding",
        "",
        at(50),
    )
    .await;
    // `The` is no lexeme: the title's first word is `partner`.
    let partner = item(
        &pool,
        &s,
        "ticket",
        "OPS-3",
        "The partner API returns 500",
        "",
        at(10),
    )
    .await;
    // Named by its key alone.
    let keyed = item(&pool, &s, "ticket", "PAY-4", "Ledger drift", "", at(1)).await;
    // Only its body holds the letter, and it is the newest row there is.
    let body = item(
        &pool,
        &s,
        "ticket",
        "OPS-5",
        "Ledger drift returns",
        "payout partner payment",
        at(60),
    )
    .await;
    item(&pool, &s, "ticket", "OPS-6", "Queue backlog", "", at(70)).await;
    let audit = item(
        &pool,
        &s,
        "ticket",
        "OPS-7",
        "Audit log rotation",
        "",
        at(4),
    )
    .await;
    let repo = item(
        &pool,
        &s,
        "repo",
        "tidewater/payout-service",
        "tidewater/payout-service",
        "",
        at(2),
    )
    .await;
    let page = item(
        &pool,
        &s,
        "page",
        "page-1",
        "Runbook for payments",
        "",
        at(3),
    )
    .await;

    let decided: Answer = vec![
        (
            "ticket".into(),
            4,
            vec![
                (partner.clone(), 1.0),
                (scheduler.clone(), 1.0),
                (keyed.clone(), 1.0),
                (rounding.clone(), 0.0),
            ],
        ),
        ("repo".into(), 1, vec![(repo.clone(), 1.0)]),
        ("page".into(), 1, vec![(page.clone(), 0.0)]),
    ];
    for raw in ["p", "P"] {
        for titles in [false, true] {
            assert_eq!(
                read(&ask(&pool, &s, raw, titles).await),
                decided,
                "{raw:?}, titles {titles}"
            );
        }
    }

    // A page shorter than the kinds fill is dealt a row a kind at a time:
    // every kind shows its first row before any shows its second, where the
    // four that lead and are newest would be three tickets and the
    // repository, and no page.
    let dealt = Searcher::new(pool.clone())
        .search(SearchQuery {
            limit: 4,
            ..query(&s, "p", false)
        })
        .await
        .unwrap();
    assert_eq!(
        read(&dealt),
        vec![
            (
                "ticket".into(),
                4,
                vec![(partner.clone(), 1.0), (scheduler.clone(), 1.0)]
            ),
            ("repo".into(), 1, vec![(repo.clone(), 1.0)]),
            ("page".into(), 1, vec![(page.clone(), 0.0)]),
        ],
    );

    // `a` is a stop word of the `english` configuration: as a word it
    // matched nothing, and as a character it names the ticket that begins
    // with it and the older one whose title holds `API`.
    let by_words = Searcher::new(pool.clone())
        .search_by_words(query(&s, "a", false))
        .await
        .unwrap();
    assert_eq!(read(&by_words), Answer::new());
    for titles in [false, true] {
        assert_eq!(
            read(&ask(&pool, &s, "a", titles).await),
            vec![(
                "ticket".into(),
                2,
                vec![(audit.clone(), 1.0), (partner.clone(), 0.0)]
            )],
            "titles {titles}"
        );
    }

    // Ranked as a word, `p` answered with the row only whose body holds it
    // too: the answer this pass replaced, so the decided one is not the old
    // one by chance.
    let by_words = Searcher::new(pool.clone())
        .search_by_words(query(&s, "p", false))
        .await
        .unwrap();
    assert_ne!(read(&by_words), decided);
    assert!(
        by_words.groups[0]
            .hits
            .iter()
            .any(|h| h.row.entity_id == body),
        "{by_words:#?}"
    );
}

/// **A character no name holds.** Under *Everything* the bodies are asked,
/// as they were, so *nothing matches* is never said of a letter a body
/// holds; under *Titles* the names were the question and none is the
/// answer.
#[tokio::test]
async fn a_character_no_name_holds_is_asked_of_the_bodies_under_everything() {
    let pool = pool().await;
    let s = source("bodies");
    let ticket = item(
        &pool,
        &s,
        "ticket",
        "OPS-1",
        "Queue backlog",
        "a zebra crossing",
        at(0),
    )
    .await;

    let everything = ask(&pool, &s, "z", false).await;
    let found: Vec<&str> = everything
        .groups
        .iter()
        .flat_map(|g| &g.hits)
        .map(|h| h.row.entity_id.as_str())
        .collect();
    assert_eq!(found, [ticket.as_str()]);
    let by_words = Searcher::new(pool.clone())
        .search_by_words(query(&s, "z", false))
        .await
        .unwrap();
    assert_eq!(read(&everything), read(&by_words));

    assert_eq!(read(&ask(&pool, &s, "z", true).await), Answer::new());
    // A character the parser reads as no word names nothing, as before:
    // asked of every source, since no row of any could answer.
    for raw in ["-", "\"", "/", "+"] {
        for titles in [false, true] {
            let nothing = Searcher::new(pool.clone())
                .search(SearchQuery {
                    filters: SearchFilters::default(),
                    ..query(&s, raw, titles)
                })
                .await
                .unwrap();
            assert_eq!(read(&nothing), Answer::new(), "{raw:?}, titles {titles}");
        }
    }
}

/// How the scale corpus of `source` is asked for a character.
pub(super) fn filters(source: &str) -> EffectiveFilters {
    EffectiveFilters {
        sources: vec![source.to_owned()],
        ..EffectiveFilters::default()
    }
}

/// `letter` asked of the stored characters and of the mirror's own rows,
/// under `filters`, `per_group` rows a kind and `limit` in all.
pub(super) async fn both(
    pool: &sqlx::PgPool,
    letter: &str,
    filters: &EffectiveFilters,
    per_group: u32,
    limit: u32,
) -> (Answer, Answer) {
    let stored = letter_sql(&[&LIVE_ITEM_FILLED], letter, filters, per_group, limit)
        .expect("nothing asked that the stored characters do not hold");
    let unstored = letter_sql_unstored(&[&LIVE_ITEM], letter, filters, per_group, limit);
    (
        answer(fetch_named(pool, stored).await.unwrap()),
        answer(query_as_with(unstored).fetch_all(pool).await.unwrap()),
    )
}

/// The characters the scale corpus is asked for: every letter and digit, a
/// capital, and one no row of it holds.
const ASKED: &str = "abcdefghijklmnopqrstuvwxyz0123456789Pé";

/// **The bounded read is the full ranking's first rows.** Each kind's first
/// rows off the index are the rows a ranking of every name match keeps
/// ([`letter_sql_unstored`]: each match's lead worked out from its row, the
/// statement's own stages cutting them): for every character, the launcher's
/// page, a page shorter than the kinds fill, and every match there is, where
/// the totals are the same too. Narrowed by a kind, by a kind left out, by a
/// window of days and by a kind answered as a pipeline.
///
/// And the names a character matches are the ones *Titles* matched it by:
/// for every character that is no stop word, the rows are those
/// [`search_sql`] finds under *Titles* with the character typed.
#[tokio::test]
async fn the_bounded_read_is_the_full_rankings_first_rows() {
    let pool = pool().await;
    let s = seed_scale_words(&pool, 900).await;
    // Older rows, which a window of days leaves out, leading with `p`.
    for n in 0..12 {
        let key = format!("OLD-{n}");
        item(
            &pool,
            &s,
            "ticket",
            &key,
            "Payout archive",
            "",
            at(-60 * 24 * 60 - n),
        )
        .await;
    }
    let plain = filters(&s);
    let narrowed = [
        plain.clone(),
        EffectiveFilters {
            kinds: vec!["ticket".into()],
            ..plain.clone()
        },
        EffectiveFilters {
            hidden_kinds: vec!["pr".into()],
            ..plain.clone()
        },
        EffectiveFilters {
            updated_within_days: Some(30),
            ..plain.clone()
        },
        EffectiveFilters {
            pipeline_kinds: vec!["pr".into(), "page".into()],
            ..plain.clone()
        },
    ];

    let (mut compared, mut matched, mut cut) = (0, Vec::new(), 0);
    for letter in ASKED.chars().map(String::from) {
        for (n, filters) in narrowed.iter().enumerate() {
            for (per_group, limit) in [(10, 40), (3, 5), (100_000, 100_000)] {
                let (stored, unstored) = both(&pool, &letter, filters, per_group, limit).await;
                let what = format!("{letter:?}, filters {n}, {per_group} a kind, {limit} in all");
                if per_group == 100_000 {
                    assert_eq!(stored, unstored, "{what}");
                } else {
                    assert_eq!(hits(&stored), hits(&unstored), "{what}");
                    // A bounded read counts the rows it holds of a kind, and
                    // where the kinds are not renamed that is `per_group` at
                    // most.
                    if filters.pipeline_kinds.is_empty() {
                        assert!(
                            stored.iter().all(|(_, total, _)| *total <= per_group),
                            "{what}"
                        );
                    }
                    cut += usize::from(stored != unstored);
                }
                compared += 1;
                if n == 0 && per_group == 10 && !stored.is_empty() {
                    matched.push(letter.clone());
                }
            }
        }

        // The names *Titles* matched the character by, where it is a word.
        let titles = EffectiveFilters {
            titles: true,
            ..plain.clone()
        };
        let typed = search_sql(
            &[&LIVE_ITEM],
            Some(&letter),
            true,
            &titles,
            100_000,
            100_000,
        );
        let typed = answer(fetch_matched(&pool, typed).await.unwrap());
        let (stored, _) = both(&pool, &letter, &plain, 100_000, 100_000).await;
        let ids = |answer: &Answer| {
            let mut ids: Vec<String> = answer
                .iter()
                .flat_map(|(_, _, hits)| hits.iter().map(|(id, _)| id.clone()))
                .collect();
            ids.sort();
            ids
        };
        if !typed.is_empty() {
            assert_eq!(
                ids(&stored),
                ids(&typed),
                "{letter:?}: the names Titles matched"
            );
        }
    }
    assert_eq!(compared, ASKED.chars().count() * narrowed.len() * 3);
    // Not vacuous: most letters name a row of the seed's words, the stop
    // words among them, and a page of ten is a cut of what there is.
    for letter in ["p", "r", "s", "t", "a", "1", "P"] {
        assert!(
            matched.iter().any(|m| m == letter),
            "{letter:?} of {matched:?}"
        );
    }
    assert!(!matched.iter().any(|m| m == "é"), "{matched:?}");
    assert!(matched.len() >= 20, "{matched:?}");
    assert!(
        cut >= 20,
        "a bounded read's totals are the rows it holds: {cut}"
    );

    // A row of each lead and of none is in the page for `p`: the order has
    // both tiers to get wrong.
    let (stored, _) = both(&pool, "p", &plain, 100_000, 100_000).await;
    let ranks: Vec<f32> = stored
        .iter()
        .flat_map(|(_, _, hits)| hits.iter().map(|(_, rank)| *rank))
        .collect();
    assert!(ranks.contains(&1.0) && ranks.contains(&0.0), "{ranks:?}");
}

/// How many of `source`'s stored characters are not the mirror row's: a
/// row the function gives that is not stored, or a stored row it does not
/// give, by id, characters (one, two or three), lead, kind, source and date.
pub(super) async fn out_of_step(pool: &sqlx::PgPool, source: &str) -> i64 {
    sqlx::query_scalar(
        "with now as (
           select i.entity_id, n.initial, n.leads, i.kind, i.source_id, i.item_updated_at
             from sync.item i
            cross join lateral sync.name_prefixes(ts_filter(i.fts, '{a}'), i.title, i.kind,
                                                  i.entity_id, i.key_cell) n
            where i.entity_id like $1 || ':%'),
         stored as (
           select x.entity_id, x.initial, x.leads, x.kind, x.source_id, x.item_updated_at
             from sync.item_initial x
            where x.entity_id like $1 || ':%')
         select (select count(*) from (select * from now except select * from stored) a)
              + (select count(*) from (select * from stored except select * from now) b)",
    )
    .bind(source)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// `id`'s stored first characters as `(character, leads)`, in order: its
/// rows of one character (`short_words.rs` reads the longer ones).
async fn initials(pool: &sqlx::PgPool, id: &str) -> Vec<(String, bool)> {
    sqlx::query_as(
        "select initial, leads from sync.item_initial
          where entity_id = $1 and length(initial) = 1 order by initial",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Where `source`'s stored rows sit, which a write of one moves.
async fn places(pool: &sqlx::PgPool, source: &str) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "select entity_id, initial, ctid::text from sync.item_initial
          where entity_id like $1 || ':%' order by 1, 2",
    )
    .bind(source)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// **The stored characters are the mirror's, after every kind of write.**
/// Written with the row, written again when its title, kind, source or date
/// changes, left alone by a stamp and by an edit of the body, and gone with
/// the row. And what a row's are, spelled out: a title past its stop words
/// leads, a ticket's key leads, the text after a title's last `/` leads.
#[tokio::test]
async fn the_stored_characters_follow_every_write() {
    let pool = pool().await;
    let s = source("writes");
    let service = item(
        &pool,
        &s,
        "ticket",
        "OPS-9",
        "The payout service",
        "ledger",
        at(0),
    )
    .await;
    let repo = item(
        &pool,
        &s,
        "repo",
        "tidewater/payout-service",
        "tidewater/payout-service",
        "",
        at(1),
    )
    .await;
    item(&pool, &s, "page", "page-1", "Runbook", "payout", at(2)).await;
    assert_eq!(out_of_step(&pool, &s).await, 0);
    // `payout`, `servic`, and the key's `op` and `-9`.
    assert_eq!(
        initials(&pool, &service).await,
        [
            ("-".to_owned(), false),
            ("o".to_owned(), true),
            ("p".to_owned(), true),
            ("s".to_owned(), false),
        ],
    );
    let of_repo = initials(&pool, &repo).await;
    assert!(of_repo.contains(&("t".to_owned(), true)), "{of_repo:?}");
    assert!(of_repo.contains(&("p".to_owned(), true)), "{of_repo:?}");
    assert!(of_repo.contains(&("s".to_owned(), false)), "{of_repo:?}");

    // A stamp and an edit of the body write nothing here.
    let before = places(&pool, &s).await;
    sqlx::query(
        "update sync.item set synced_at = now() + interval '1 minute' where source_id = $1",
    )
    .bind(&s)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("update sync.item set body_text = 'quota' where entity_id = $1")
        .bind(&service)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(places(&pool, &s).await, before);
    assert_eq!(out_of_step(&pool, &s).await, 0);

    // A new title: the characters it no longer holds go.
    sqlx::query("update sync.item set title = 'Quota alerts' where entity_id = $1")
        .bind(&service)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(out_of_step(&pool, &s).await, 0);
    assert_eq!(
        initials(&pool, &service).await,
        [
            ("-".to_owned(), false),
            ("a".to_owned(), false),
            ("o".to_owned(), true),
            ("q".to_owned(), true),
        ],
    );

    // A date that moves, a kind that changes (a ticket's key no longer
    // leads), and an upsert of several rows at once.
    sqlx::query("update sync.item set item_updated_at = $2 where source_id = $1")
        .bind(&s)
        .bind(at(90))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(out_of_step(&pool, &s).await, 0);
    sqlx::query("update sync.item set kind = 'page' where entity_id = $1")
        .bind(&service)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(out_of_step(&pool, &s).await, 0);
    assert_eq!(
        initials(&pool, &service).await,
        [("a".to_owned(), false), ("q".to_owned(), true)],
    );
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
         select i.entity_id, i.source_id, i.kind, 'Zone ' || i.title, i.body_text, i.payload, $2
           from sync.item i where i.source_id = $1
             on conflict (entity_id) do update
            set title = excluded.title, item_updated_at = excluded.item_updated_at",
    )
    .bind(&s)
    .bind(at(120))
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(out_of_step(&pool, &s).await, 0);
    assert!(
        initials(&pool, &repo)
            .await
            .contains(&("z".to_owned(), true))
    );

    // A row that goes takes its characters.
    sqlx::query("delete from sync.item where entity_id = $1")
        .bind(&repo)
        .execute(&pool)
        .await
        .unwrap();
    assert!(initials(&pool, &repo).await.is_empty());
    assert_eq!(out_of_step(&pool, &s).await, 0);
    let stored: i64 = sqlx::query_scalar(
        "select count(*) from sync.item_initial where entity_id like $1 || ':%'",
    )
    .bind(&s)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(stored > 0);
}

/// **A row that is not live is not answered**, by the two tests the mirror's
/// view makes: an entity withdrawn, a source switched off.
#[tokio::test]
async fn a_withdrawn_row_and_a_source_switched_off_are_not_answered() {
    let pool = pool().await;
    let s = source("live");
    let kept = item(&pool, &s, "ticket", "OPS-1", "Payout scheduler", "", at(0)).await;
    let gone = item(&pool, &s, "ticket", "OPS-2", "Payout ledger", "", at(1)).await;
    let ids = |response: &SearchResponse| -> Vec<String> {
        response
            .groups
            .iter()
            .flat_map(|g| &g.hits)
            .map(|h| h.row.entity_id.clone())
            .collect()
    };
    assert_eq!(
        ids(&ask(&pool, &s, "p", false).await),
        [gone.clone(), kept.clone()]
    );

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&gone)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(ids(&ask(&pool, &s, "p", false).await), [kept.as_str()]);
    let plain = filters(&s);
    let (stored, unstored) = both(&pool, "p", &plain, 10, 40).await;
    assert_eq!(stored, unstored);

    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, enabled)
         values ($1, 'jira', $1, 'http://x', 'Pat', false)",
    )
    .bind(&s)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(ids(&ask(&pool, &s, "p", true).await), Vec::<String>::new());
    let (stored, unstored) = both(&pool, "p", &plain, 10, 40).await;
    assert_eq!((stored, unstored), (Answer::new(), Answer::new()));
}

/// What a response says beside its timing: each group's kind, total, and
/// each hit's id, rank and excerpts.
pub(super) fn said(response: &SearchResponse) -> String {
    let groups: Vec<_> = response
        .groups
        .iter()
        .map(|g| {
            let hits: Vec<_> = g
                .hits
                .iter()
                .map(|h| (&h.row.entity_id, h.rank, &h.row.title, &h.excerpts))
                .collect();
            (&g.kind, g.total, hits)
        })
        .collect();
    format!("{:?} {groups:?}", response.total)
}

/// **What the names do not answer is answered as it was.** Every probe is
/// asked of the launcher and of the ranking of words alone
/// ([`Searcher::search_by_words`], every query's answer before the
/// one-letter pass), under both scopes: words of four characters and more,
/// whole words, two words, a phrase, a key, a word left out, a finished word
/// of one, two and three characters (`p` and a space), which is a word and
/// not the start of one, and two or three characters that are not all
/// letters and digits. One, two and three characters typed are the answers
/// that differ.
#[tokio::test]
async fn what_the_names_do_not_answer_is_answered_as_it_was() {
    let pool = pool().await;
    let s = seed_scale_words(&pool, 300).await;
    let searcher = Searcher::new(pool.clone());
    let probes = [
        "payo",
        "retr",
        "sche",
        "retry",
        "payout",
        "fx quotes",
        "retry budget",
        "\"retry queue\"",
        "retry -queue",
        "PAY-12",
        "p ",
        "r ",
        "pa ",
        "re ",
        "pay ",
        "p r",
        "pa re",
        "#t payo",
        "#t pay ",
        "p-",
        "a-b",
        "c#",
        "-pa",
        "\"pa",
        "p.",
        "pa:",
    ];
    let mut matched = 0;
    for raw in probes {
        for titles in [false, true] {
            let now = searcher.search(query(&s, raw, titles)).await.unwrap();
            let before = searcher
                .search_by_words(query(&s, raw, titles))
                .await
                .unwrap();
            assert_eq!(said(&now), said(&before), "{raw:?}, titles {titles}");
            matched += usize::from(now.total > 0);
        }
    }
    assert!(matched >= 24, "the probes that matched: {matched}");

    // One, two and three characters typed are the answers the two passes
    // changed, under both scopes, so the comparison above can tell the two
    // apart.
    for raw in ["p", "pa", "pay", "#t pa"] {
        for titles in [false, true] {
            let now = searcher.search(query(&s, raw, titles)).await.unwrap();
            let before = searcher
                .search_by_words(query(&s, raw, titles))
                .await
                .unwrap();
            assert!(
                now.total > 0 && before.total > now.total,
                "{raw:?}, titles {titles}"
            );
            assert_ne!(said(&now), said(&before), "{raw:?}, titles {titles}");
        }
    }
}

/// **Each kind's rows are read in the index's order, and the read stops.**
/// Planned with every other way to read a table switched off, so a small
/// mirror is planned as a large one is: the stored characters are reached
/// through `item_initial_newest_idx` and never sorted, which holds only
/// while the statement's order within a kind is the index's.
#[tokio::test]
async fn a_kinds_rows_are_read_in_the_index_order() {
    let pool = pool().await;
    let s = seed_scale_words(&pool, 300).await;
    let built = letter_sql(&[&LIVE_ITEM_FILLED], "p", &filters(&s), 10, 40).unwrap();
    assert!(
        built.sql().starts_with("with recursive\n"),
        "{}",
        built.sql()
    );
    let plan = explain_named_at_any_size(&pool, built).await.unwrap();
    // The two stages that read the stored characters: the kinds, and each
    // kind's rows.
    for stage in ["CTE letter_kinds", "CTE m"] {
        let read = stage_of(&plan, stage);
        assert!(read.contains("item_initial_newest_idx"), "{stage}:\n{plan}");
        assert!(
            !read.contains("Sort"),
            "{stage} sorts its rows, where it should read them in order:\n{plan}"
        );
    }
    assert!(!plan.contains("Seq Scan on item_initial"), "{plan}");
    assert!(!plan.contains("Bitmap"), "{plan}");
}

/// The lines of `plan` under the node that begins `stage`: those indented
/// past it, up to the next that is not.
pub(super) fn stage_of(plan: &str, stage: &str) -> String {
    let indent = |line: &str| line.len() - line.trim_start().len();
    let mut lines = plan.lines().skip_while(|line| line.trim_start() != stage);
    let head = lines
        .next()
        .unwrap_or_else(|| panic!("no {stage} in:\n{plan}"));
    lines
        .take_while(|line| indent(line) > indent(head))
        .collect::<Vec<_>>()
        .join("\n")
}
