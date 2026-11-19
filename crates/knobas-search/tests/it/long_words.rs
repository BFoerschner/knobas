//! Four typed characters and more, and a finished word (the long-words pass,
//! 2026-11-16; `docs/decisions/2026-11-16-a-word-many-rows-hold.md`).
//!
//! The launcher counts a query's matches, no further than one past a bound
//! (2,000 in the app, `sql::RANKED_WHOLE`):
//!
//! * **few rows hold it**: every match is ranked, the answer it always had,
//!   under one plan a connection (`sql::kept_search_sql`);
//! * **many do**: the rows it names, and under *Everything*, where they
//!   leave a kind room, that kind's newest rows whose text holds it
//!   (`sql::broad_sql`), read off the names' index and the empty start's
//!   (`sync.item_initial`, migration `0095`).
//!
//! Held here: the bounded answer as decided, on rows written for it; that
//! answer to the unbounded reads it stands for, over the scale seed's words
//! at every bound; that the walk's budget changes no answer; the page dealt a
//! row a kind at a time at every length; the plan; and the fallback while the
//! stored rows are not whole. `one_letter.rs` and `short_words.rs` hold a
//! word's first characters, and that what few rows hold is answered as it
//! was.
//!
//! This module has a database of its own (`knobas_db::test_util` keys it by
//! file), shared by its tests, so each test reads the rows of a source it
//! made. The tests that unmark the stored rows take a scratch database.

use std::collections::BTreeMap;

use knobas_search::corpus::LIVE_ITEM_FILLED;
use knobas_search::query::EffectiveFilters;
use knobas_search::sql::{Names, broad_sql, explain_kept, fetch_kept, probe_sql};
use knobas_search::{SearchQuery, SearchResponse, Searcher};

use super::item_search::{seed_scale_words, source};
use super::one_letter::{Answer, answer, at, filters, item, pool, query, read, said, stage_of};

/// `raw` asked of `source`'s rows by a searcher that ranks whole while no
/// more than `ranked_whole` rows hold the query, and looks for a broader
/// one's text in `walk` of a kind's newest rows.
async fn bounded(
    pool: &sqlx::PgPool,
    source: &str,
    raw: &str,
    titles: bool,
    (ranked_whole, walk): (u32, u32),
    limit: u32,
) -> SearchResponse {
    Searcher::new(pool.clone())
        .bounded_at(ranked_whole, walk)
        .search(SearchQuery {
            limit,
            ..query(source, raw, titles)
        })
        .await
        .unwrap()
}

/// `raw` ranked whole, as every query was before this pass, the page as
/// large as every kind's ten rows.
async fn ranked(pool: &sqlx::PgPool, source: &str, raw: &str, titles: bool) -> SearchResponse {
    Searcher::new(pool.clone())
        .search_by_words(SearchQuery {
            limit: 200,
            ..query(source, raw, titles)
        })
        .await
        .unwrap()
}

/// Each kind's hits as `(id, rank)`, in order.
fn kinds(response: &SearchResponse) -> BTreeMap<String, Vec<(String, f32)>> {
    read(response)
        .into_iter()
        .map(|(kind, _, hits)| (kind, hits))
        .collect()
}

/// **The answer as decided.** A word more rows hold than the bound is
/// answered by the rows it names, ranked as *Titles* ranks them, and after
/// them, where they leave a kind room, the newest rows whose text holds it,
/// not ranked: a body that says the word twenty times stands after a name
/// that says it once, and between two bodies the newer is first. A kind's
/// total is the rows the answer holds of it. Under *Titles* the names were
/// the question. The same word under a bound it is within is ranked whole,
/// as before: the loud body first.
#[tokio::test]
async fn a_word_many_rows_hold_is_answered_by_its_names_then_the_newest_text() {
    let pool = pool().await;
    let s = source("long");
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
    // The oldest name, and the one its body backs: ranked first among names.
    let rounding = item(
        &pool,
        &s,
        "ticket",
        "OPS-2",
        "Fix the payout rounding",
        "payout payout payout",
        at(1),
    )
    .await;
    // Bodies alone.
    let loud = item(
        &pool,
        &s,
        "ticket",
        "OPS-3",
        "Ledger drift",
        "payout payout payout payout payout payout payout payout payout payout payout payout \
         payout payout payout payout payout payout payout payout",
        at(60),
    )
    .await;
    let quiet = item(
        &pool,
        &s,
        "ticket",
        "OPS-4",
        "Queue backlog",
        "one payout",
        at(70),
    )
    .await;
    let old = item(&pool, &s, "ticket", "OPS-5", "Cache key", "a payout", at(2)).await;
    item(&pool, &s, "ticket", "OPS-6", "Nothing here", "", at(80)).await;
    let repo = item(
        &pool,
        &s,
        "repo",
        "tidewater/payout-service",
        "tidewater/payout-service",
        "",
        at(3),
    )
    .await;
    let page = item(
        &pool,
        &s,
        "page",
        "page-1",
        "Runbook",
        "the payout runs nightly",
        at(4),
    )
    .await;

    // Seven rows hold it and three name it: past a bound of three.
    for raw in ["payout", "payo", "payout ", "PAYOUT"] {
        let names = kinds(&ranked(&pool, &s, raw, true).await);
        assert_eq!(
            names["ticket"]
                .iter()
                .map(|(id, _)| id.as_str())
                .collect::<Vec<_>>(),
            [rounding.as_str(), scheduler.as_str()],
            "{raw:?}: *Titles* ranks the name its body backs first"
        );
        let answer = bounded(&pool, &s, raw, false, (3, 500), 40).await;
        let drawn = read(&answer);
        let mut tickets = names["ticket"].clone();
        tickets.extend([
            (quiet.clone(), 0.0),
            (loud.clone(), 0.0),
            (old.clone(), 0.0),
        ]);
        let expected: Answer = vec![
            ("ticket".to_owned(), 5, tickets),
            ("repo".to_owned(), 1, names["repo"].clone()),
            ("page".to_owned(), 1, vec![(page.clone(), 0.0)]),
        ];
        assert_eq!(drawn, expected, "{raw:?}");
        assert_eq!(names["repo"][0].0, repo);
        assert_eq!(answer.total, 7, "{raw:?}: the rows the answer holds");

        // Under *Titles* three rows hold it: within the bound, ranked whole.
        let titled = bounded(&pool, &s, raw, true, (3, 500), 200).await;
        assert_eq!(
            said(&titled),
            said(&ranked(&pool, &s, raw, true).await),
            "{raw:?}"
        );

        // Within the bound the word is ranked whole, as it was: the loud
        // body first.
        let whole = bounded(&pool, &s, raw, false, (7, 500), 200).await;
        assert_eq!(
            said(&whole),
            said(&ranked(&pool, &s, raw, false).await),
            "{raw:?}"
        );
        assert_eq!(kinds(&whole)["ticket"][0].0, loud, "{raw:?}");
        // One row fewer in the bound and it is past it.
        let past = bounded(&pool, &s, raw, false, (6, 500), 200).await;
        assert_eq!(read(&past), read(&answer), "{raw:?}");
    }
}

/// **Names that fill a kind are that kind's answer.** Ten tickets the word
/// names leave the tickets no room: no ticket only whose body holds it is
/// drawn, however new. A kind they leave room in takes the text's rows.
#[tokio::test]
async fn names_that_fill_a_kind_leave_the_text_out_of_it() {
    let pool = pool().await;
    let s = source("full");
    let mut named = Vec::new();
    for n in 0..10 {
        named.push(
            item(
                &pool,
                &s,
                "ticket",
                &format!("OPS-{n}"),
                &format!("Payout step {n}"),
                "",
                at(n),
            )
            .await,
        );
    }
    for n in 10..13 {
        item(
            &pool,
            &s,
            "ticket",
            &format!("OPS-{n}"),
            "Ledger drift",
            "payout payout payout payout",
            at(100 + n),
        )
        .await;
    }
    let newer = item(&pool, &s, "page", "page-1", "Runbook", "payout", at(9)).await;
    let older = item(&pool, &s, "page", "page-2", "Notes", "payout", at(8)).await;

    // Fifteen rows hold it, ten name it: the names are ranked (a bound of
    // twelve), each at the one mention its title makes, so the newest first.
    let answer = bounded(&pool, &s, "payout", false, (12, 500), 40).await;
    let by_kind = kinds(&answer);
    named.reverse();
    assert_eq!(
        by_kind["ticket"]
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>(),
        named
    );
    assert!(by_kind["ticket"].iter().all(|(_, rank)| *rank > 0.0));
    assert_eq!(by_kind["page"], [(newer, 0.0), (older, 0.0)]);
}

/// **A word that names many rows too is answered by the newest of them.**
/// Past the bound in names (a project's key), no name is ranked: each kind's
/// newest ten, under *Titles* and under *Everything*, where the text's rows
/// follow them as they follow ranked names.
#[tokio::test]
async fn a_word_that_names_many_rows_is_answered_by_the_newest_of_them() {
    let pool = pool().await;
    let s = source("keyed");
    let mut named = Vec::new();
    for n in 0..6 {
        // The oldest says it most: ranked, it would be first.
        let body = "sclpay ".repeat(6 - n);
        named.push(
            item(
                &pool,
                &s,
                "ticket",
                &format!("SCLPAY-{n}"),
                &format!("Step {n}"),
                &body,
                at(i64::try_from(n).unwrap()),
            )
            .await,
        );
    }
    let mention = item(
        &pool,
        &s,
        "ticket",
        "OPS-1",
        "Ledger drift",
        "see SCLPAY-3",
        at(50),
    )
    .await;
    let ranked_first = kinds(&ranked(&pool, &s, "sclpay", true).await)["ticket"][0]
        .0
        .clone();
    assert_eq!(ranked_first, named[0], "ranked, the oldest is first");

    named.reverse();
    let newest: Vec<(String, f32)> = named.iter().map(|id| (id.clone(), 1.0)).collect();
    let titled = bounded(&pool, &s, "sclpay", true, (3, 500), 40).await;
    assert_eq!(
        read(&titled),
        vec![("ticket".to_owned(), 6, newest.clone())]
    );
    let everything = bounded(&pool, &s, "sclpay", false, (3, 500), 40).await;
    let mut with_text = newest;
    with_text.push((mention, 0.0));
    assert_eq!(read(&everything), vec![("ticket".to_owned(), 7, with_text)]);
}

/// **A kind the word is rare in is answered whole all the same.** The walk
/// of a kind's newest rows stops at its budget, and a kind it did not fill
/// by then is asked of the index: the pages whose text holds the word are
/// older than every page the walk looked at, and they are answered, newest
/// first, whatever the budget. So is a kind the budget cut mid-way.
#[tokio::test]
async fn a_kind_the_word_is_rare_in_is_answered_whole_whatever_the_walks_budget() {
    let pool = pool().await;
    let s = source("sparse");
    let mut pages = Vec::new();
    for n in 0..3 {
        pages.push(
            item(
                &pool,
                &s,
                "page",
                &format!("old-{n}"),
                "Runbook",
                "payout",
                at(n),
            )
            .await,
        );
    }
    for n in 0..5 {
        item(
            &pool,
            &s,
            "page",
            &format!("new-{n}"),
            "Notes",
            "nothing of it",
            at(50 + n),
        )
        .await;
    }
    let mut tickets = Vec::new();
    for n in 0..4 {
        tickets.push(
            item(
                &pool,
                &s,
                "ticket",
                &format!("OPS-{n}"),
                "Ledger drift",
                "payout",
                at(10 + n),
            )
            .await,
        );
    }
    pages.reverse();
    tickets.reverse();
    let unranked =
        |ids: &[String]| -> Vec<(String, f32)> { ids.iter().map(|id| (id.clone(), 0.0)).collect() };
    let expected: Answer = vec![
        ("ticket".to_owned(), 4, unranked(&tickets)),
        ("page".to_owned(), 3, unranked(&pages)),
    ];
    // 1 and 2: every kind is cut short. 6: the pages' walk finds one of the
    // three. 8: the pages' walk ends on their last row. 500: no kind is cut.
    for walk in [1, 2, 4, 6, 8, 500] {
        let answer = bounded(&pool, &s, "payout", false, (3, walk), 40).await;
        assert_eq!(read(&answer), expected, "a walk of {walk}");
    }
}

/// **A row that is not live is not answered, and a filter holds**, in each
/// of the bounded statement's three reads (the names, the walk, the index
/// for a kind the walk did not fill): an entity withdrawn, a source switched
/// off, a kind hidden, a kind asked for, a window of days.
#[tokio::test]
async fn the_bounded_reads_keep_to_what_is_live_and_asked_for() {
    let pool = pool().await;
    let s = source("livelong");
    let name = item(&pool, &s, "ticket", "OPS-1", "Payout scheduler", "", at(1)).await;
    let gone_name = item(&pool, &s, "ticket", "OPS-2", "Payout ledger", "", at(2)).await;
    let text = item(&pool, &s, "ticket", "OPS-3", "Ledger", "payout", at(3)).await;
    let gone_text = item(&pool, &s, "ticket", "OPS-4", "Drift", "payout", at(4)).await;
    let page = item(&pool, &s, "page", "page-1", "Runbook", "payout", at(5)).await;
    let build = item(&pool, &s, "build", "build-1", "Nightly", "payout", at(6)).await;
    for id in [&gone_name, &gone_text] {
        sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let ids = |rows: Answer| -> Vec<String> {
        let mut ids: Vec<String> = rows
            .into_iter()
            .flat_map(|(_, _, hits)| hits)
            .map(|(id, _)| id)
            .collect();
        ids.sort();
        ids
    };
    let sorted = |ids: &[&String]| -> Vec<String> {
        let mut ids: Vec<String> = ids.iter().map(|id| (*id).clone()).collect();
        ids.sort();
        ids
    };
    let plain = filters(&s);
    // 500: the walk finds the text. 1: the index does.
    for walk in [500, 1] {
        for names in [Names::Ranked, Names::Newest] {
            let ask = async |asked: &EffectiveFilters| -> Vec<String> {
                let built = broad_sql(
                    &[&LIVE_ITEM_FILLED],
                    "payout",
                    true,
                    asked,
                    10,
                    40,
                    names,
                    walk,
                )
                .expect("narrow");
                ids(answer(fetch_kept(&pool, built).await.unwrap()))
            };
            let what = format!("a walk of {walk}, names {names:?}");
            assert_eq!(
                ask(&plain).await,
                sorted(&[&name, &text, &page, &build]),
                "{what}"
            );
            let hidden = EffectiveFilters {
                hidden_kinds: vec!["build".to_owned()],
                ..plain.clone()
            };
            assert_eq!(ask(&hidden).await, sorted(&[&name, &text, &page]), "{what}");
            let asked = EffectiveFilters {
                kinds: vec!["page".to_owned(), "build".to_owned()],
                ..plain.clone()
            };
            assert_eq!(ask(&asked).await, sorted(&[&page, &build]), "{what}");
            // Every row here is dated the first of November 2026: none
            // within a day, all within a hundred thousand.
            let recent = EffectiveFilters {
                updated_within_days: Some(1),
                ..plain.clone()
            };
            assert_eq!(ask(&recent).await, Vec::<String>::new(), "{what}");
            let ever = EffectiveFilters {
                updated_within_days: Some(100_000),
                ..plain.clone()
            };
            assert_eq!(
                ask(&ever).await,
                sorted(&[&name, &text, &page, &build]),
                "{what}"
            );
        }
    }
    // Counted as answered: the withdrawn rows are not in the probe's count.
    let probe = probe_sql("payout", true, &plain, 100).expect("narrow");
    let counted: Vec<(i64, i64)> = fetch_kept(&pool, probe).await.unwrap();
    assert_eq!(counted, [(4, 0)]);

    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, enabled)
         values ($1, 'jira', $1, 'http://x', 'Pat', false)",
    )
    .bind(&s)
    .execute(&pool)
    .await
    .unwrap();
    for walk in [500, 1] {
        for names in [Names::Ranked, Names::Newest] {
            let built = broad_sql(
                &[&LIVE_ITEM_FILLED],
                "payout",
                true,
                &plain,
                10,
                40,
                names,
                walk,
            )
            .expect("narrow");
            assert_eq!(
                answer(fetch_kept(&pool, built).await.unwrap()),
                Answer::new(),
                "a walk of {walk}, names {names:?}"
            );
        }
    }
}

/// **The matches are counted no further than one past the bound**, and the
/// names only where the matches are past it.
#[tokio::test]
async fn the_probe_counts_no_further_than_one_past_the_bound() {
    let pool = pool().await;
    let s = source("probe");
    for n in 0..5 {
        item(
            &pool,
            &s,
            "ticket",
            &format!("OPS-{n}"),
            "Payout step",
            "",
            at(n),
        )
        .await;
    }
    for n in 5..9 {
        item(
            &pool,
            &s,
            "ticket",
            &format!("OPS-{n}"),
            "Ledger",
            "payout",
            at(n),
        )
        .await;
    }
    let count = async |cap: u32, titles: bool| -> (i64, i64) {
        let asked = EffectiveFilters {
            titles,
            ..filters(&s)
        };
        let probe = probe_sql("payout", true, &asked, cap).expect("narrow");
        let rows: Vec<(i64, i64)> = fetch_kept(&pool, probe).await.unwrap();
        assert_eq!(rows.len(), 1);
        rows[0]
    };
    // Nine rows hold it and five name it.
    assert_eq!(
        count(100, false).await,
        (9, 0),
        "within the bound: the count"
    );
    assert_eq!(count(9, false).await, (9, 0), "at the bound");
    assert_eq!(count(8, false).await, (9, 5), "past it: the names counted");
    assert_eq!(
        count(3, false).await,
        (4, 4),
        "each one past the bound, no further"
    );
    assert_eq!(
        count(100, true).await,
        (5, 0),
        "*Titles*: the names are the matches"
    );
    assert_eq!(count(4, true).await, (5, 5));
    assert_eq!(count(2, true).await, (3, 3));
}

/// The typed words as the statements read them: the last a prefix while it
/// is being typed. A literal, spliced by `concat!`: no statement here is
/// assembled at run time (`sql_containment.rs`).
macro_rules! tsq {
    () => {
        "case when right(websearch_to_tsquery('english', $1)::text, 1) = '''' and $2
              then (websearch_to_tsquery('english', $1)::text || ':*')::tsquery
              else websearch_to_tsquery('english', $1) end"
    };
}

/// **The bounded answer is the names and the newest text of every match.**
/// Over the scale seed's words, a word still being typed, a finished one,
/// two words and a phrase: the launcher's answer past the bound is, for each
/// kind, the rows the query names (as the ranking of every name gives them
/// where they are within the bound, the newest of them where they are not)
/// and then the rows only whose text holds it, newest first, ten a kind,
/// each read here with no bound at all. At a bound the matches are within,
/// the answer is the ranking's. And the walk's budget changes none of it.
#[tokio::test]
async fn the_bounded_answer_is_the_names_and_the_newest_text_of_every_match() {
    let pool = pool().await;
    let s = seed_scale_words(&pool, 300).await;
    let mut checked = (0, 0, 0);
    for raw in [
        "payout",
        "payo",
        "retry",
        "retry ",
        "sched",
        "ledger",
        "budget",
        "retry budget",
        "\"retry queue\"",
        "retry -queue",
        "scheduler runbook",
        "PAY-1",
        "tidewater",
        "zzzz",
    ] {
        let prefix_last = !raw.ends_with(' ');
        // Every match, by kind: the names newest first, and the rows only
        // whose text holds the query newest first.
        let every = async |named: bool| -> BTreeMap<String, Vec<String>> {
            let rows: Vec<(String, String)> = sqlx::query_as(concat!(
                "select sx.kind, sx.entity_id from sync.item_search sx
                  cross join (select ",
                tsq!(),
                " as tsq) q
                  where sx.source_id = $3 and sx.fts @@ q.tsq and (sx.fts_a @@ q.tsq) = $4
                  order by sx.item_updated_at desc nulls last, sx.entity_id"
            ))
            .bind(raw)
            .bind(prefix_last)
            .bind(&s)
            .bind(named)
            .fetch_all(&pool)
            .await
            .unwrap();
            let mut by_kind: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (kind, id) in rows {
                by_kind.entry(kind).or_default().push(id);
            }
            by_kind
        };
        let names = every(true).await;
        let text = every(false).await;
        let named: usize = names.values().map(Vec::len).sum();
        let matched: usize = named + text.values().map(Vec::len).sum::<usize>();
        let ranked_names = kinds(&ranked(&pool, &s, raw, true).await);
        let ranked_whole = ranked(&pool, &s, raw, false).await;
        // The names as the launcher counts them to choose their read: by the
        // names' index alone, so a name whose body holds a word the query
        // leaves out (`retry -queue`) is counted, though never answered.
        let counted: i64 = sqlx::query_scalar(concat!(
            "select count(*) from sync.item_search sx cross join (select ",
            tsq!(),
            " as tsq) q
              where sx.source_id = $3 and sx.fts_a @@ q.tsq"
        ))
        .bind(raw)
        .bind(prefix_last)
        .bind(&s)
        .fetch_one(&pool)
        .await
        .unwrap();
        let counted = usize::try_from(counted).unwrap();
        assert!(counted >= named, "{raw:?}");

        let mut bounds = vec![u32::try_from(matched).unwrap()];
        if matched > 0 {
            bounds.push(u32::try_from(matched - 1).unwrap());
        }
        if counted > 0 && counted < matched {
            bounds.push(u32::try_from(counted).unwrap());
            bounds.push(u32::try_from(counted - 1).unwrap());
        }
        bounds.push(0);
        for bound in bounds {
            let within = matched <= usize::try_from(bound).unwrap();
            let names_within = counted <= usize::try_from(bound).unwrap();
            let mut answers = Vec::new();
            for walk in [500, 3, 1] {
                let answer = bounded(&pool, &s, raw, false, (bound, walk), 200).await;
                answers.push(read(&answer));
                if within {
                    assert_eq!(
                        said(&answer),
                        said(&ranked_whole),
                        "{raw:?} at a bound of {bound}: ranked whole"
                    );
                    checked.0 += 1;
                    continue;
                }
                let mut expected: BTreeMap<String, Vec<(String, f32)>> = BTreeMap::new();
                for kind in names.keys().chain(text.keys()) {
                    let mut rows: Vec<(String, f32)> = if names_within {
                        ranked_names.get(kind).cloned().unwrap_or_default()
                    } else {
                        names
                            .get(kind)
                            .into_iter()
                            .flatten()
                            .map(|id| (id.clone(), 1.0))
                            .collect()
                    };
                    rows.extend(
                        text.get(kind)
                            .into_iter()
                            .flatten()
                            .map(|id| (id.clone(), 0.0)),
                    );
                    rows.truncate(10);
                    expected.insert(kind.clone(), rows);
                }
                assert_eq!(
                    kinds(&answer),
                    expected,
                    "{raw:?} at a bound of {bound}, a walk of {walk}"
                );
                for (kind, total, hits) in read(&answer) {
                    assert_eq!(
                        usize::try_from(total).unwrap(),
                        hits.len(),
                        "{raw:?}: {kind}'s total is the rows the answer holds"
                    );
                }
                if names_within {
                    checked.1 += 1;
                } else {
                    checked.2 += 1;
                }
            }
            assert!(
                answers.windows(2).all(|pair| pair[0] == pair[1]),
                "{raw:?} at a bound of {bound}: the walk's budget changed the answer"
            );
        }
    }
    // Each of the three answers was met: ranked whole, ranked names with the
    // text after, and the newest names with the text after.
    assert!(
        checked.0 >= 30 && checked.1 >= 15 && checked.2 >= 30,
        "{checked:?}"
    );
}

/// **The page is dealt a row a kind at a time, at every length.** Five kinds
/// hold ten rows each of a word few rows hold: a page of forty takes eight of
/// each, a kind's rows the first of its ranking, where the forty best ranked
/// of all left a kind out. A page of seven takes each kind's first, then the
/// two best of the seconds.
#[tokio::test]
async fn the_page_is_dealt_a_row_a_kind_at_a_time() {
    let pool = pool().await;
    let s = source("dealt");
    for (kind, mentions) in [
        ("ticket", 9),
        ("page", 7),
        ("pr", 5),
        ("repo", 3),
        ("monitor", 1),
    ] {
        for n in 0..10 {
            // A kind's rows rank apart from every other kind's, and from
            // each other.
            let body = "settlement ".repeat(mentions * 10 + n);
            item(
                &pool,
                &s,
                kind,
                &format!("{kind}-{n}"),
                "Row",
                &body,
                at(i64::try_from(n).unwrap()),
            )
            .await;
        }
    }
    let every = kinds(&ranked(&pool, &s, "settlement", false).await);
    assert_eq!(every.len(), 5);
    assert!(every.values().all(|rows| rows.len() == 10));

    for searcher in [
        Searcher::new(pool.clone()),
        // And where the word is past the bound.
        Searcher::new(pool.clone()).bounded_at(3, 500),
    ] {
        let page = searcher
            .search(query(&s, "settlement", false))
            .await
            .unwrap();
        let dealt = kinds(&page);
        assert_eq!(dealt.len(), 5, "every kind is on the page");
        for (kind, rows) in &dealt {
            assert_eq!(rows.len(), 8, "{kind}");
        }
    }
    let page = kinds(
        &Searcher::new(pool.clone())
            .search(query(&s, "settlement", false))
            .await
            .unwrap(),
    );
    for (kind, rows) in &page {
        assert_eq!(
            rows[..],
            every[kind][..8],
            "{kind}: the first of its ranking"
        );
    }
    let seven = kinds(
        &Searcher::new(pool.clone())
            .search(SearchQuery {
                limit: 7,
                ..query(&s, "settlement", false)
            })
            .await
            .unwrap(),
    );
    let held: BTreeMap<&str, usize> = seven
        .iter()
        .map(|(kind, rows)| (kind.as_str(), rows.len()))
        .collect();
    assert_eq!(
        held,
        BTreeMap::from([
            ("ticket", 2),
            ("page", 2),
            ("pr", 1),
            ("repo", 1),
            ("monitor", 1)
        ]),
        "each kind's first, then the best ranked of the seconds"
    );
}

/// **Each kind's rows are walked newest first off the index, and the names
/// are read through theirs.** Planned with sorts and bitmap reads switched
/// off for the walk, so a small mirror is planned as a large one is: the
/// kinds and the walk reach `sync.item_initial` through
/// `item_initial_newest_idx` and never sort, which holds only while the
/// walk's order is the index's. As the app plans it, the names come off the
/// index of the names, and the index's read for a kind the walk did not fill
/// is not run where every kind was filled.
#[tokio::test]
async fn a_kinds_rows_are_walked_in_the_index_order() {
    let pool = pool().await;
    let s = seed_scale_words(&pool, 300).await;
    let built = || {
        broad_sql(
            &[&LIVE_ITEM_FILLED],
            "payout",
            true,
            &filters(&s),
            10,
            40,
            Names::Ranked,
            500,
        )
        .expect("narrow")
    };
    let plan = explain_kept(&pool, built(), true).await.unwrap();
    for stage in ["CTE kinds", "CTE walk"] {
        let read = stage_of(&plan, stage);
        assert!(read.contains("item_initial_newest_idx"), "{stage}:\n{plan}");
        assert!(
            !read.contains("Sort"),
            "{stage} sorts its rows, where it should read them in order:\n{plan}"
        );
    }
    assert!(!plan.contains("Seq Scan on item_initial"), "{plan}");
    let walk = stage_of(&plan, "CTE walk");
    assert!(walk.contains("Index Only Scan"), "{plan}");
    assert!(walk.contains("item_search_pkey"), "{plan}");

    let plan = explain_kept(&pool, built(), false).await.unwrap();
    let names = stage_of(&plan, "CTE names");
    assert!(names.contains("item_search_fts_a_idx"), "{plan}");
    assert!(!names.contains("Seq Scan on item_search"), "{plan}");
    // Every kind of the seed holds the word in its newest rows: the index is
    // not asked for any.
    let late = stage_of(&plan, "CTE late");
    assert!(late.contains("item_search_fts_idx"), "{plan}");
    assert!(late.contains("never executed"), "{plan}");
}

/// **No statement the launcher's keys are held to plans a parallel worker.**
/// A worker is a process PostgreSQL starts for one query and ends with it:
/// about 2 ms on a Mac and 31 to 37 on Windows, for each. The count, the
/// ranking of few matches, the bounded answer either way its names are read,
/// and a word's first characters are each planned here with a worker made to
/// look free for any scan of any table, and none holds a `Gather`. The
/// one-letter read of the whole table, which asks for three, shows the
/// instrument sees one.
#[tokio::test]
async fn no_statement_a_key_is_held_to_plans_a_parallel_worker() {
    use knobas_search::sql::{
        explain_by_a_letter_at_any_size, explain_named_at_any_size, kept_search_sql, letter_sql,
        search_sql,
    };
    let pool = pool().await;
    let s = seed_scale_words(&pool, 300).await;
    let corpora = [&LIVE_ITEM_FILLED];
    for titles in [false, true] {
        let asked = EffectiveFilters {
            titles,
            ..filters(&s)
        };
        let mut kept = vec![
            ("the count", probe_sql("payout", true, &asked, 5).unwrap()),
            (
                "few matches ranked",
                kept_search_sql(&corpora, "payout", true, &asked, 10, 40).unwrap(),
            ),
        ];
        for names in [Names::Ranked, Names::Newest] {
            kept.push((
                "the bounded answer",
                broad_sql(&corpora, "payout", true, &asked, 10, 40, names, 500).unwrap(),
            ));
        }
        // The rows the picker chose, read by their keys (the fuzzy-search
        // decisions, 2026-11-19, D8 and D14).
        let ids: Vec<String> = sqlx::query_scalar(
            "select entity_id from sync.item where source_id = $1 order by entity_id limit 60",
        )
        .bind(&s)
        .fetch_all(&pool)
        .await
        .unwrap();
        let ranks: Vec<f32> = ids.iter().map(|_| 0.5).collect();
        kept.push((
            "the picker's rows",
            knobas_search::sql::picked_sql(&ids, &ranks, &asked, 10, 80),
        ));
        for (what, built) in kept {
            for at_any_size in [false, true] {
                let plan = explain_kept(&pool, built.clone(), at_any_size)
                    .await
                    .unwrap();
                assert!(
                    !plan.contains("Gather") && !plan.contains("Workers"),
                    "{what}, titles {titles}:\n{plan}"
                );
            }
        }
        let letters = letter_sql(&corpora, "pa", &asked, 10, 40).unwrap();
        let plan = explain_named_at_any_size(&pool, letters).await.unwrap();
        assert!(
            !plan.contains("Gather") && !plan.contains("Workers"),
            "a word's first characters, titles {titles}:\n{plan}"
        );
    }
    // The picker's load, which nobody waits on, all the same: planned as it
    // runs, with workers made to look free.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(knobas_search::picker::LOAD_PLANNING)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "select set_config('parallel_setup_cost', '0', true), \
                set_config('parallel_tuple_cost', '0', true), \
                set_config('min_parallel_table_scan_size', '0', true), \
                set_config('min_parallel_index_scan_size', '0', true)",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    let plan: Vec<(String,)> = sqlx::query_as(knobas_search::picker::NAMES_PLAN_SQL)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let plan = plan
        .into_iter()
        .map(|(line,)| line)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !plan.contains("Gather") && !plan.contains("Workers"),
        "the names' load:\n{plan}"
    );
    // The instrument: the one-letter read of the whole table asks for three.
    let whole = search_sql(&corpora, Some("p"), true, &filters(&s), 10, 40);
    let plan = explain_by_a_letter_at_any_size(&pool, whole, false)
        .await
        .unwrap();
    assert!(plan.contains("Workers Planned: 3"), "{plan}");
}

/// **Until every item has its stored rows, the launcher answers as it did
/// before them.** A mirror whose `sync.item_initial` is not marked whole
/// (one that held items before the migrations, until bring-up's fill has
/// passed) is asked a word's first characters and a word many rows hold by
/// the ranking of every match: the same answers a longer wait. The fill
/// (`knobas_core::item_initial::fill`) writes the rows of the items that have
/// none, marks the table, and finds nothing to do the next time. On a
/// database of its own: it unmarks the table for every reader.
#[tokio::test]
async fn until_the_stored_rows_are_whole_every_query_is_ranked_as_before() {
    let pool = knobas_db::test_util::scratch_database("long_words_whole")
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    let s = seed_scale_words(&pool, 60).await;
    let searcher = Searcher::new(pool.clone()).bounded_at(5, 500);
    let asked = ["p", "pa", "pay", "payout", "retry queue", "sched"];
    let differs = async || -> usize {
        let mut differing = 0;
        for raw in asked {
            for titles in [false, true] {
                let now = searcher.search(query(&s, raw, titles)).await.unwrap();
                let before = searcher
                    .search_by_words(query(&s, raw, titles))
                    .await
                    .unwrap();
                differing += usize::from(said(&now) != said(&before));
            }
        }
        differing
    };
    assert!(
        knobas_search::corpus::stored(&pool).await.unwrap().whole(),
        "a new mirror is whole: its items' own writes stored their rows"
    );
    assert!(differs().await >= 6, "whole: names and bounded answers");

    // An install that held these items before the migrations: no row of
    // theirs stored, the table not marked.
    sqlx::query("delete from sync.item_initial_whole")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from sync.item_initial where entity_id like $1 || ':%'")
        .bind(&s)
        .execute(&pool)
        .await
        .unwrap();
    let stored = knobas_search::corpus::stored(&pool).await.unwrap();
    assert!(!stored.names && !stored.whole());
    assert_eq!(differs().await, 0, "not whole: every answer the ranking's");

    let batches = knobas_core::item_initial::fill(&pool).await.unwrap();
    let items: i64 = sqlx::query_scalar("select count(*) from sync.item")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        i64::from(batches),
        (items + i64::from(knobas_core::item_initial::BATCH) - 1)
            / i64::from(knobas_core::item_initial::BATCH),
        "every item looked at, a batch at a time"
    );
    assert_eq!(super::one_letter::out_of_step(&pool, &s).await, 0);
    assert!(knobas_search::corpus::stored(&pool).await.unwrap().whole());
    assert!(differs().await >= 6, "whole again");
    assert_eq!(knobas_core::item_initial::fill(&pool).await.unwrap(), 0);

    // An item under the empty start: one row, whatever it is named.
    let (starts, items): (i64, i64) = sqlx::query_as(
        "select (select count(*) from sync.item_initial where initial = '' and leads),
                (select count(*) from sync.item)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(starts, items);
}
