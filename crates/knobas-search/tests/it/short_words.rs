//! Two and three typed characters (the two-letters pass, 2026-11-16;
//! `docs/decisions/2026-11-16-two-letters-by-names-and-keys-wait-for-their-answer.md`):
//! the launcher answers them as it answers one, with the names that hold a
//! word beginning with them, those that begin with them first, then the
//! newest, read off `sync.item_initial` (migrations `0093` and `0094`) a
//! kind's first rows at a time. Under *Everything*, where those names are
//! few, the text is asked as well and its rows follow the names.
//!
//! Held here: the answer as decided, on rows written for it; what the typed
//! characters are read as (the `english` lexeme, a stop word kept); the text
//! after few names and none after names that fill a kind; the bounded read
//! to the full ranking it stands for, over the scale seed's words; and the
//! stored characters, spelled out. `one_letter.rs` holds one character, the
//! stored rows after every kind of write, and every query the names do not
//! answer to the answer it had before.
//!
//! This module has a database of its own (`knobas_db::test_util` keys it by
//! file), shared by its tests, so each test reads the rows of a source it
//! made.

use knobas_search::corpus::{LIVE_ITEM, LIVE_ITEM_FILLED};
use knobas_search::query::EffectiveFilters;
use knobas_search::sql::{explain_named_at_any_size, fetch_matched, letter_sql, search_sql};
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

use super::item_search::{seed_scale_words, source};
use super::one_letter::{
    Answer, answer, ask, at, both, filters, hits, item, out_of_step, pool, query, read, said,
    stage_of,
};

/// Each group's kind and its hits' ids, in order.
fn ids(response: &SearchResponse) -> Vec<(String, Vec<String>)> {
    response
        .groups
        .iter()
        .map(|g| {
            (
                g.kind.clone(),
                g.hits.iter().map(|h| h.row.entity_id.clone()).collect(),
            )
        })
        .collect()
}

/// The launcher's answer ranked by words alone, as every query was answered
/// before the names answered any.
async fn by_words(pool: &sqlx::PgPool, source: &str, raw: &str, titles: bool) -> SearchResponse {
    Searcher::new(pool.clone())
        .search_by_words(query(source, raw, titles))
        .await
        .unwrap()
}

/// **The answer as decided.** A row answers two or three characters when a
/// word of its title or key begins with them, and under *Titles* never by
/// its body; the rows a name of which begins with them come first (a title
/// past its stop words, the text after a title's last `/`, a ticket's key),
/// then the newest; typed in either case. Under *Everything* the same names
/// stand first, each where it stood, and the row only whose body holds the
/// characters follows them, since the names are few.
#[tokio::test]
async fn two_and_three_characters_are_answered_by_names_leading_first_then_newest() {
    let pool = pool().await;
    let s = source("short");
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
    // The newest name of all, and none of it begins with `pa`.
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
    // Only its body holds the characters, many times over, and it is the
    // newest row there is.
    let body = item(
        &pool,
        &s,
        "ticket",
        "OPS-5",
        "Ledger drift returns",
        "payout partner payment payout payout payout payout payout payout payout payout payout \
         payout payout payout payout payout payout payout payout payout payout payout payout",
        at(60),
    )
    .await;
    item(&pool, &s, "ticket", "OPS-6", "Queue backlog", "", at(70)).await;
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

    // `pa`: the partner ticket leads with it; `pay`: it holds no such word.
    let names_of = |raw: &str| -> Answer {
        let tickets = if raw.eq_ignore_ascii_case("pa") {
            vec![
                (partner.clone(), 1.0),
                (scheduler.clone(), 1.0),
                (keyed.clone(), 1.0),
                (rounding.clone(), 0.0),
            ]
        } else {
            vec![
                (scheduler.clone(), 1.0),
                (keyed.clone(), 1.0),
                (rounding.clone(), 0.0),
            ]
        };
        vec![
            (
                "ticket".into(),
                u32::try_from(tickets.len()).unwrap(),
                tickets,
            ),
            ("repo".into(), 1, vec![(repo.clone(), 1.0)]),
            ("page".into(), 1, vec![(page.clone(), 0.0)]),
        ]
    };
    for raw in ["pa", "PA", "pay", "Pay"] {
        let names = names_of(raw);
        assert_eq!(read(&ask(&pool, &s, raw, true).await), names, "{raw:?}");

        // Under *Everything* the names are few, so the text follows: the
        // body's row after every ticket a name gave, ranked as the words
        // rank it, and far above any of them.
        let everything = ask(&pool, &s, raw, false).await;
        let ranked = by_words(&pool, &s, raw, false).await;
        let rank_of_body = ranked
            .groups
            .iter()
            .flat_map(|g| &g.hits)
            .find(|h| h.row.entity_id == body)
            .map(|h| h.rank)
            .expect("the words find the body");
        assert!(rank_of_body > 1.0, "{rank_of_body}");
        let mut with_text = names.clone();
        with_text[0].1 += 1;
        with_text[0].2.push((body.clone(), rank_of_body));
        assert_eq!(read(&everything), with_text, "{raw:?}");
        // The tickets, the repository and the page.
        assert_eq!(everything.total, with_text[0].1 + 2, "{raw:?}");
        // Ranked as words, the body's row was first: the answer this pass
        // replaced, so the decided one is not the old one by chance.
        assert_eq!(ranked.groups[0].hits[0].row.entity_id, body, "{raw:?}");
    }

    // A page shorter than the kinds fill is dealt a row a kind at a time.
    let dealt = Searcher::new(pool.clone())
        .search(SearchQuery {
            limit: 4,
            ..query(&s, "pa", true)
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
    // And under *Everything* a full page of names asks no text: the names
    // alone, as under *Titles*.
    let full = Searcher::new(pool.clone())
        .search(SearchQuery {
            limit: 4,
            ..query(&s, "pa", false)
        })
        .await
        .unwrap();
    assert_eq!(read(&full), read(&dealt));
}

/// **What the typed characters are read as.** The `english` lexeme, as the
/// words' ranking reads them, so `ops` asks for `op` as it always did and
/// finds what *Titles* found; and where `english` reads a stop word, the
/// characters themselves, which then name rows where they matched nothing.
#[tokio::test]
async fn the_characters_are_read_as_the_words_ranking_reads_them_and_a_stop_word_is_kept() {
    let pool = pool().await;
    let s = source("lexed");
    let keyed = item(&pool, &s, "ticket", "OPS-1", "Ledger drift", "", at(1)).await;
    let open = item(&pool, &s, "ticket", "WEB-2", "Open the valve", "", at(5)).await;
    let reopen = item(
        &pool,
        &s,
        "ticket",
        "WEB-3",
        "Reopen ops channel",
        "",
        at(9),
    )
    .await;
    let invoice = item(&pool, &s, "ticket", "WEB-4", "Invoice renderer", "", at(2)).await;
    // `in` and `The` are no lexemes: neither row is named by them.
    item(&pool, &s, "ticket", "WEB-5", "Error in payout", "", at(8)).await;
    item(&pool, &s, "ticket", "WEB-6", "The valve", "", at(7)).await;
    let theme = item(&pool, &s, "ticket", "WEB-7", "Theme switcher", "", at(3)).await;

    // `ops` is the lexeme `op`: the names that begin with it first (the
    // title that opens with `Open`, the key `OPS-1`), newest first, then
    // the row that holds `ops` further in.
    let op: Answer = vec![(
        "ticket".into(),
        3,
        vec![
            (open.clone(), 1.0),
            (keyed.clone(), 1.0),
            (reopen.clone(), 0.0),
        ],
    )];
    for raw in ["op", "ops", "OPS"] {
        assert_eq!(read(&ask(&pool, &s, raw, true).await), op, "{raw:?}");
    }
    // The same rows *Titles* matched `ops` by, in the ranking's order.
    let ranked = by_words(&pool, &s, "ops", true).await;
    let mut ranked_ids = ids(&ranked).remove(0).1;
    ranked_ids.sort();
    let mut named_ids = vec![open.clone(), keyed.clone(), reopen.clone()];
    named_ids.sort();
    assert_eq!(ranked_ids, named_ids);

    // A stop word matched nothing as a word, and names rows as the start of
    // one.
    for (raw, named) in [("in", &invoice), ("In", &invoice), ("the", &theme)] {
        for titles in [false, true] {
            assert_eq!(
                read(&by_words(&pool, &s, raw, titles).await),
                Answer::new(),
                "{raw:?}"
            );
            assert_eq!(
                read(&ask(&pool, &s, raw, titles).await),
                vec![("ticket".into(), 1, vec![(named.clone(), 1.0)])],
                "{raw:?}, titles {titles}"
            );
        }
    }
}

/// **Few names are followed by the text; names that fill a kind are the
/// answer.** Nine tickets named `Payout …` leave room, so under
/// *Everything* the page whose body alone holds the word joins, and the
/// ticket whose body does, after the nine. A tenth such ticket fills the
/// kind: the answer is then the names, and no text is asked. Under *Titles*
/// the names are the answer either way.
#[tokio::test]
async fn few_names_are_followed_by_the_text_and_names_that_fill_a_kind_are_not() {
    let pool = pool().await;
    let s = source("few");
    let mut named = Vec::new();
    for n in 0..9 {
        let key = format!("OPS-{n}");
        named.push(item(&pool, &s, "ticket", &key, "Payout ledger", "", at(n)).await);
    }
    // Newest first.
    named.reverse();
    let in_body = item(
        &pool,
        &s,
        "ticket",
        "WEB-1",
        "Queue backlog",
        "payout",
        at(90),
    )
    .await;
    let page = item(&pool, &s, "page", "page-1", "Runbook", "payout", at(91)).await;

    for raw in ["pa", "pay"] {
        let titles = ask(&pool, &s, raw, true).await;
        assert_eq!(ids(&titles), [("ticket".to_owned(), named.clone())]);

        let everything = ask(&pool, &s, raw, false).await;
        let mut tickets = named.clone();
        tickets.push(in_body.clone());
        assert_eq!(
            ids(&everything),
            [
                ("ticket".to_owned(), tickets),
                ("page".to_owned(), vec![page.clone()]),
            ],
            "{raw:?}"
        );
        // Each kind's total is the rows the page holds of it.
        assert_eq!(
            everything
                .groups
                .iter()
                .map(|g| g.total)
                .collect::<Vec<_>>(),
            [10, 1]
        );
        assert_eq!(everything.total, 11);

        // Room for one row more than the names: it is dealt to the kind
        // that has no row yet, the page, and not to the tickets' tenth.
        let one_more = Searcher::new(pool.clone())
            .search(SearchQuery {
                limit: 10,
                ..query(&s, raw, false)
            })
            .await
            .unwrap();
        assert_eq!(
            ids(&one_more),
            [
                ("ticket".to_owned(), named.clone()),
                ("page".to_owned(), vec![page.clone()]),
            ],
            "{raw:?}"
        );
    }

    // The tenth name fills the tickets.
    let tenth = item(&pool, &s, "ticket", "OPS-9", "Payout ledger", "", at(100)).await;
    let mut ten = vec![tenth];
    ten.extend(named.clone());
    for raw in ["pa", "pay"] {
        for titles in [false, true] {
            assert_eq!(
                ids(&ask(&pool, &s, raw, titles).await),
                [("ticket".to_owned(), ten.clone())],
                "{raw:?}, titles {titles}"
            );
        }
        // The words still find the page: it is the names that left it out.
        assert!(
            ids(&by_words(&pool, &s, raw, false).await)
                .iter()
                .any(|(kind, _)| kind == "page"),
            "{raw:?}"
        );
    }
}

/// **Characters no name holds.** Under *Everything* the text is asked, as
/// it was, and its answer is the one the words' ranking gives, whole; under
/// *Titles* the names were the question and none is the answer.
#[tokio::test]
async fn characters_no_name_holds_are_asked_of_the_text_under_everything() {
    let pool = pool().await;
    let s = source("unnamed");
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
    for raw in ["ze", "zeb"] {
        let everything = ask(&pool, &s, raw, false).await;
        assert_eq!(
            ids(&everything),
            [("ticket".to_owned(), vec![ticket.clone()])]
        );
        assert_eq!(
            said(&everything),
            said(&by_words(&pool, &s, raw, false).await),
            "{raw:?}"
        );
        assert_eq!(
            read(&ask(&pool, &s, raw, true).await),
            Answer::new(),
            "{raw:?}"
        );
    }
}

/// **A corpus that stores no characters answers by its own lexemes.** The
/// notes hold a handful of rows and no row of `sync.item_initial`: a note is
/// named by two or three characters when a word of its title begins with
/// them, and leads when its title's first word does. Under *Everything* the
/// note whose body alone holds them follows, since the names are few.
#[tokio::test]
async fn a_note_is_named_by_the_first_characters_of_its_titles_words() {
    let pool = pool().await;
    let tag = source("note");
    let mut notes = Vec::new();
    for (n, (title, body)) in [
        ("Daily payout", ""),
        ("Payout cut-off", ""),
        ("Ledger", "the payout window"),
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("note:{tag}-{n}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'note', $2)")
            .bind(&id)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("insert into knobas.note (id, title, body_md) values ($1, $2, $3)")
            .bind(&id)
            .bind(title)
            .bind(body)
            .execute(&pool)
            .await
            .unwrap();
        notes.push(id);
    }
    let asked = |raw: &str, titles: bool| SearchQuery {
        titles,
        raw: raw.to_owned(),
        limit: 40,
        filters: SearchFilters {
            kinds: vec!["note".to_owned()],
            ..SearchFilters::default()
        },
    };
    let searcher = Searcher::new(pool.clone());
    for raw in ["pa", "pay"] {
        let named = searcher.search(asked(raw, true)).await.unwrap();
        assert_eq!(
            read(&named),
            vec![(
                "note".into(),
                2,
                vec![(notes[1].clone(), 1.0), (notes[0].clone(), 0.0)]
            )],
            "{raw:?}"
        );
        let everything = searcher.search(asked(raw, false)).await.unwrap();
        assert_eq!(
            ids(&everything),
            [(
                "note".to_owned(),
                vec![notes[1].clone(), notes[0].clone(), notes[2].clone()]
            )],
            "{raw:?}"
        );
    }
}

/// The characters the scale corpus is asked for: the starts of its words
/// and keys, stems shorter than what was typed, stop words, numbers,
/// capitals, and some no row of it holds.
const ASKED: &[&str] = &[
    "pa", "pay", "re", "ret", "de", "dep", "fi", "fix", "ca", "car", "se", "sep", "fx", "ky",
    "kyc", "op", "ops", "we", "web", "ti", "tid", "sc", "sch", "qu", "que", "le", "led", "ru",
    "run", "id", "ide", "ib", "iba", "to", "tok", "st", "sta", "th", "thr", "in", "the", "an",
    "and", "or", "is", "it", "12", "10", "100", "50", "500", "PA", "Pay", "é1", "zz", "zzz",
];

/// **The bounded read is the full ranking's first rows.** Each kind's first
/// rows off the index are the rows a ranking of every name match keeps
/// (`letter_sql_unstored`: each match's lead worked out from its row, the
/// statement's own stages cutting them), for two and three characters as
/// `one_letter.rs` holds it for one: the launcher's page, a page shorter
/// than the kinds fill, and every match there is, where the totals are the
/// same too. Narrowed by a kind, by a kind left out, by a window of days and
/// by a kind answered as a pipeline.
///
/// And the names the characters match are the ones *Titles* matched them
/// by: wherever the words' statement reads them as a word, its rows under
/// *Titles* are the same rows.
#[tokio::test]
async fn the_bounded_read_is_the_full_rankings_first_rows() {
    let pool = pool().await;
    let s = seed_scale_words(&pool, 900).await;
    // Older rows, which a window of days leaves out, leading with `pay`.
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

    let (mut compared, mut matched, mut cut, mut as_titles) = (0, Vec::new(), 0, 0);
    for typed in ASKED {
        for (n, filters) in narrowed.iter().enumerate() {
            for (per_group, limit) in [(10, 40), (3, 5), (100_000, 100_000)] {
                let (stored, unstored) = both(&pool, typed, filters, per_group, limit).await;
                let what = format!("{typed:?}, filters {n}, {per_group} a kind, {limit} in all");
                if per_group == 100_000 {
                    assert_eq!(stored, unstored, "{what}");
                } else {
                    assert_eq!(hits(&stored), hits(&unstored), "{what}");
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
                    matched.push(*typed);
                }
            }
        }

        // The names *Titles* matched the characters by, where they are a word.
        let titles = EffectiveFilters {
            titles: true,
            ..plain.clone()
        };
        let ranked = search_sql(&[&LIVE_ITEM], Some(typed), true, &titles, 100_000, 100_000);
        let ranked = answer(fetch_matched(&pool, ranked).await.unwrap());
        let (stored, _) = both(&pool, typed, &plain, 100_000, 100_000).await;
        let ids = |answer: &Answer| {
            let mut ids: Vec<String> = answer
                .iter()
                .flat_map(|(_, _, hits)| hits.iter().map(|(id, _)| id.clone()))
                .collect();
            ids.sort();
            ids
        };
        if !ranked.is_empty() {
            assert_eq!(
                ids(&stored),
                ids(&ranked),
                "{typed:?}: the names Titles matched"
            );
            as_titles += 1;
        }
    }
    assert_eq!(compared, ASKED.len() * narrowed.len() * 3);
    // Not vacuous: the starts of the seed's words name rows, the stems and
    // the stop words among them, and a page of ten is a cut of what there is.
    for typed in [
        "pa", "pay", "re", "ret", "ops", "we", "sch", "in", "to", "12", "PA",
    ] {
        assert!(matched.contains(&typed), "{typed:?} of {matched:?}");
    }
    for typed in ["é1", "zz", "zzz"] {
        assert!(!matched.contains(&typed), "{typed:?} of {matched:?}");
    }
    assert!(matched.len() >= 30, "{matched:?}");
    assert!(as_titles >= 25, "compared with Titles: {as_titles}");
    assert!(
        cut >= 40,
        "a bounded read's totals are the rows it holds: {cut}"
    );

    // A row of each lead and of none is in the answer for `pa` and for
    // `pay`: the order has both tiers to get wrong.
    for typed in ["pa", "pay"] {
        let (stored, _) = both(&pool, typed, &plain, 100_000, 100_000).await;
        let ranks: Vec<f32> = stored
            .iter()
            .flat_map(|(_, _, hits)| hits.iter().map(|(_, rank)| *rank))
            .collect();
        assert!(
            ranks.contains(&1.0) && ranks.contains(&0.0),
            "{typed}: {ranks:?}"
        );
    }
}

/// `id`'s stored rows of two and three characters as `(characters, leads)`,
/// in order.
async fn starts(pool: &sqlx::PgPool, id: &str) -> Vec<(String, bool)> {
    sqlx::query_as(
        "select initial, leads from sync.item_initial
          where entity_id = $1 and length(initial) > 1 order by initial",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// **The stored characters, spelled out.** A row holds the first two and the
/// first three characters of each word it is named by, with whether a name
/// of it begins with them: a title past its stop words, a ticket's key, the
/// text after a title's last `/`, a pull request's number. A word of two
/// characters has no row of three, and the number of a ticket's key, lexed
/// with its hyphen, has none of two or three. They follow a new title as
/// the single characters do.
#[tokio::test]
async fn a_row_holds_the_first_two_and_three_characters_of_its_names() {
    let pool = pool().await;
    let s = source("starts");
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
    // `payout`, `servic`, and the key's `op` and `-9`.
    let own = |rows: &[(&str, bool)]| -> Vec<(String, bool)> {
        rows.iter()
            .map(|(start, leads)| ((*start).to_owned(), *leads))
            .collect()
    };
    assert_eq!(
        starts(&pool, &service).await,
        own(&[
            ("op", true),
            ("pa", true),
            ("pay", true),
            ("se", false),
            ("ser", false),
        ]),
    );
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
    let of_repo = starts(&pool, &repo).await;
    for start in [("ti", true), ("tid", true), ("pa", true), ("pay", true)] {
        assert!(
            of_repo.contains(&(start.0.to_owned(), start.1)),
            "{of_repo:?}"
        );
    }
    assert!(of_repo.contains(&("ser".to_owned(), false)), "{of_repo:?}");
    let pull = item(
        &pool,
        &s,
        "pr",
        "tidewater/payout-service#142",
        "Retry the batch",
        "",
        at(2),
    )
    .await;
    let of_pull = starts(&pool, &pull).await;
    for start in [("14", true), ("142", true), ("re", true), ("ret", true)] {
        assert!(
            of_pull.contains(&(start.0.to_owned(), start.1)),
            "{of_pull:?}"
        );
    }
    assert!(of_pull.contains(&("bat".to_owned(), false)), "{of_pull:?}");
    assert_eq!(out_of_step(&pool, &s).await, 0);

    sqlx::query("update sync.item set title = 'Quota alerts' where entity_id = $1")
        .bind(&service)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        starts(&pool, &service).await,
        own(&[
            ("al", false),
            ("ale", false),
            ("op", true),
            ("qu", true),
            ("quo", true),
        ]),
    );
    assert_eq!(out_of_step(&pool, &s).await, 0);
}

/// **One statement for every length, and each kind read in the index's
/// order.** What was typed is a bind and nothing else of the statement
/// depends on it, so one, two and three characters are one statement and
/// one kept plan; planned as a large mirror is, three characters reach the
/// stored rows through `item_initial_newest_idx` and never sort them.
#[tokio::test]
async fn one_statement_reads_every_length_in_the_index_order() {
    let pool = pool().await;
    let s = seed_scale_words(&pool, 300).await;
    let asked = filters(&s);
    let sql_of = |typed: &str| {
        letter_sql(&[&LIVE_ITEM_FILLED], typed, &asked, 10, 40)
            .unwrap()
            .sql()
            .to_owned()
    };
    assert_eq!(sql_of("p"), sql_of("pa"));
    assert_eq!(sql_of("p"), sql_of("pay"));

    let built = letter_sql(&[&LIVE_ITEM_FILLED], "pay", &asked, 10, 40).unwrap();
    let plan = explain_named_at_any_size(&pool, built).await.unwrap();
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
