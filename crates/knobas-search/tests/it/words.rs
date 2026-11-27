//! The mirror's words, and a word nobody wrote read as the nearest one
//! somebody did (the fuzzy-search decisions, 2026-11-19, D10;
//! `docs/decisions/2026-11-19-names-are-found-as-a-picker-finds-them.md`).
//!
//! `sync.word` (migration `0100`) holds every word the titles and bodies
//! hold, as written. The picker reads it whole once it is marked whole, and
//! under *Everything* the launcher asks its statements for the real words a
//! typed one is read as: the nearest written word where the text found
//! nothing, and the longer words a written word is part of beside the text.
//!
//! This module has a database of its own, shared by its tests, so each test
//! writes the rows of a source it made and asks for that source's rows, with
//! words of its own where the list's content matters. The tests that unmark
//! the list take a scratch database.

use knobas_search::picker::Picker;
use knobas_search::{SearchFilters, SearchQuery, SearchResponse, Searcher};

use super::item_search::source;
use super::one_letter::{at, item, pool, said};

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

fn ids(response: &SearchResponse) -> Vec<String> {
    response
        .groups
        .iter()
        .flat_map(|group| group.hits.iter().map(|hit| hit.row.entity_id.clone()))
        .collect()
}

async fn holds(pool: &sqlx::PgPool, word: &str) -> bool {
    sqlx::query_scalar("select exists (select from sync.word where word = $1)")
        .bind(word)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// **The list after an insert, a new body, a stamp and a delete**: a new
/// row's words and a changed body's are written with it, a stamp writes
/// nothing, and a delete takes no word out.
#[tokio::test]
async fn the_list_holds_what_the_rows_wrote() {
    let pool = pool().await;
    let s = source("list");
    let word = format!("zqw{}", &s[s.len() - 8..]);
    let id = item(
        &pool,
        &s,
        "ticket",
        "1",
        "Fix the ledger",
        &format!("{word}one and more"),
        at(0),
    )
    .await;
    assert!(holds(&pool, &format!("{word}one")).await);
    assert!(holds(&pool, "ledger").await);
    // Two letters, a digit first, a sign inside: no word.
    assert!(!holds(&pool, "an").await);
    sqlx::query("update sync.item set body_text = $2 where entity_id = $1")
        .bind(&id)
        .bind(format!("{word}two"))
        .execute(&pool)
        .await
        .unwrap();
    assert!(holds(&pool, &format!("{word}two")).await);
    // A stamp fires neither trigger: the row's word, taken out of the list,
    // is not written again by it.
    sqlx::query("delete from sync.word where word = $1")
        .bind(format!("{word}two"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("update sync.item set synced_at = now() where entity_id = $1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(!holds(&pool, &format!("{word}two")).await);
    sqlx::query("update sync.item set body_text = $2 where entity_id = $1")
        .bind(&id)
        .bind(format!("{word}two lantern"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from sync.item where entity_id = $1")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        holds(&pool, &format!("{word}two")).await,
        "a word is never taken out"
    );
}

/// **D10's table, on bodies written for it**: a word nobody wrote finds the
/// rows that say the nearest written word, a run-together pair the rows that
/// say both side by side, and a written word the rows that say it inside a
/// longer word, after the rows that say it.
#[tokio::test]
async fn a_word_nobody_wrote_finds_the_rows_that_say_the_nearest_one() {
    let pool = pool().await;
    let s = source("table");
    let tag = &s[s.len() - 6..];
    let scheduler = item(
        &pool,
        &s,
        "ticket",
        "1",
        "Night shift",
        &format!("the payout scheduler {tag}xa restarts"),
        at(0),
    )
    .await;
    let retry = item(
        &pool,
        &s,
        "ticket",
        "2",
        "Backoff",
        &format!("one retry {tag}xb a day"),
        at(0),
    )
    .await;
    let pair = item(
        &pool,
        &s,
        "ticket",
        "3",
        "Gateway",
        &format!("the auth proxy {tag}xc drops"),
        at(0),
    )
    .await;
    let plain = item(
        &pool,
        &s,
        "page",
        "4",
        "Notes",
        &format!("proxy {tag}xd and more proxy"),
        at(0),
    )
    .await;
    let inside = item(
        &pool,
        &s,
        "page",
        "5",
        "Runbook",
        &format!("restart ADProxy {tag}xe first"),
        at(-60),
    )
    .await;
    let picker = picker(&pool).await;
    for (typed, wanted) in [
        ("shceduler", &scheduler),
        ("retrry", &retry),
        ("authproxy", &pair),
    ] {
        let r = ask(&pool, &picker, query(&s, typed, false)).await;
        assert!(ids(&r).contains(wanted), "{typed}: {r:?}");
        // Under *Titles* nothing is read: those are bodies.
        let r = ask(&pool, &picker, query(&s, typed, true)).await;
        assert!(!ids(&r).contains(wanted), "{typed} under Titles: {r:?}");
    }
    // `proxy`: the page that says it, then the page that says `ADProxy`.
    let r = ask(&pool, &picker, query(&s, "proxy", false)).await;
    let pages: Vec<String> = r
        .groups
        .iter()
        .filter(|g| g.kind == "page")
        .flat_map(|g| g.hits.iter().map(|h| h.row.entity_id.clone()))
        .collect();
    assert_eq!(pages, [plain.clone(), inside.clone()], "{r:?}");
    // A word somebody wrote is never corrected: `payout` finds what says it.
    let r = ask(&pool, &picker, query(&s, "payout", false)).await;
    assert_eq!(ids(&r), std::slice::from_ref(&scheduler));
}

/// **A query that answers today answers byte for byte with the list
/// loaded**, where no word is read: every word written, none part of a
/// longer one.
#[tokio::test]
async fn an_answer_nothing_is_read_for_is_as_it_was() {
    let pool = pool().await;
    let s = source("same");
    let tag = &s[s.len() - 8..];
    let word = format!("qv{tag}");
    item(
        &pool,
        &s,
        "ticket",
        "1",
        "One",
        &format!("{word} once"),
        at(0),
    )
    .await;
    item(
        &pool,
        &s,
        "page",
        "2",
        "Two",
        &format!("{word} {word} twice"),
        at(-5),
    )
    .await;
    let picker = picker(&pool).await;
    assert!(picker.holds_words());
    for titles in [false, true] {
        let before = Searcher::new(pool.clone())
            .search(query(&s, &word, titles))
            .await
            .unwrap();
        let now = ask(&pool, &picker, query(&s, &word, titles)).await;
        assert_eq!(said(&now), said(&before), "titles {titles}");
    }
}

/// **No correction before the mark**: a list not yet whole is not read, so
/// a word nobody wrote finds nothing, as before the list; the fill reads the
/// rows mirrored before it, steps around a row a sync holds, and marks the
/// list whole once no row is past its place.
#[tokio::test]
async fn the_list_is_read_only_once_it_is_whole() {
    let pool = knobas_db::test_util::scratch_database("words_whole")
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database");
    let s = source("whole");
    let id = item(
        &pool,
        &s,
        "ticket",
        "1",
        "Night",
        "the payout scheduler restarts",
        at(0),
    )
    .await;
    item(
        &pool,
        &s,
        "ticket",
        "2",
        "Day",
        "the ledger writer stalls",
        at(0),
    )
    .await;
    // A mirror from before the migration: no word read, no mark.
    sqlx::query("delete from sync.word_whole")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from sync.word")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("delete from sync.word_filled_to")
        .execute(&pool)
        .await
        .unwrap();
    // One word the list already holds, as a sync's own write would have put
    // it there: the list is still not whole, so it is not read.
    sqlx::query("insert into sync.word values ('scheduler')")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!knobas_core::word_list::whole(&pool).await.unwrap());
    let unread = picker(&pool).await;
    assert!(!unread.holds_words());
    let r = ask(&pool, &unread, query(&s, "shceduler", false)).await;
    assert!(ids(&r).is_empty(), "{r:?}");
    let marked: bool = sqlx::query_scalar("select sync.mark_words_whole()")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!marked, "the mark is refused while a row is unread");

    // A row a sync holds: the batch stops before it and says so.
    let mut held = pool.begin().await.unwrap();
    sqlx::query("select 1 from sync.item where entity_id = $1 for update")
        .bind(&id)
        .execute(&mut *held)
        .await
        .unwrap();
    let (read, stopped): (i32, bool) =
        sqlx::query_as("select read, held from sync.fill_words_beside(150)")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(stopped, "it stopped at the held row");
    assert_eq!(read, 0, "the held row is the first in id order");
    assert!(!holds(&pool, "ledger").await);
    held.commit().await.unwrap();

    let batches = knobas_core::word_list::fill(&pool).await.unwrap();
    assert!(batches >= 1);
    assert!(knobas_core::word_list::whole(&pool).await.unwrap());
    assert!(holds(&pool, "scheduler").await && holds(&pool, "ledger").await);
    assert_eq!(
        knobas_core::word_list::fill(&pool).await.unwrap(),
        0,
        "whole: nothing to do"
    );
    let whole = picker(&pool).await;
    assert!(whole.holds_words());
    let r = ask(&pool, &whole, query(&s, "shceduler", false)).await;
    assert_eq!(ids(&r), [id]);
}

/// **A correction only where the answer was empty**: `mapped` is no word the
/// list holds, and finds every `mapping` row by its stem as it always did;
/// its nearest written word is never asked in its place.
#[tokio::test]
async fn a_word_the_text_answers_is_never_corrected() {
    let pool = pool().await;
    let s = source("stem");
    let mapping = item(
        &pool,
        &s,
        "ticket",
        "1",
        "Mapping",
        "the mapping table",
        at(0),
    )
    .await;
    item(&pool, &s, "ticket", "2", "Mapper", "the mapper tool", at(0)).await;
    let picker = picker(&pool).await;
    assert!(
        picker.reading("mapped", false).corrected.is_some(),
        "the list would read it as another word"
    );
    let r = ask(&pool, &picker, query(&s, "mapped", false)).await;
    assert!(ids(&r).contains(&mapping), "{r:?}");
}

/// The words `dl<tag>w0000` to `dl<tag>w0999`: zero-padded, so their order as
/// strings is their order as numbers.
fn numbered(tag: &str) -> Vec<String> {
    (0..1000).map(|n| format!("dl{tag}w{n:04}")).collect()
}

/// Holds `held` uncommitted in one transaction while `writer` runs in another
/// over words on both sides of it; once `writer` waits for `held`, writes
/// every word above `held` from the first transaction, then commits both.
///
/// A writer that takes its words in their order has written none above
/// `held` when it stops there, so the first transaction's words go in at
/// once. One that took a word above `held` first holds it, and the two wait
/// for each other until PostgreSQL ends one of them with `deadlock detected`.
async fn crossed_with<F>(pool: &sqlx::PgPool, words: &[String], held: usize, writer: F)
where
    F: FnOnce(
        sqlx::Transaction<'static, sqlx::Postgres>,
    ) -> tokio::task::JoinHandle<Result<(), sqlx::Error>>,
{
    let s = source("dlheld");
    let mut first = pool.begin().await.unwrap();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'dl'), ($2, 'ticket', 'dl')")
        .bind(format!("{s}:1"))
        .bind(format!("{s}:2"))
        .execute(&mut *first)
        .await
        .unwrap();
    let row = "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
               values ($1, $2, 'ticket', 'dl', $3, '{}'::jsonb, now())";
    sqlx::query(row)
        .bind(format!("{s}:1"))
        .bind(&s)
        .bind(&words[held])
        .execute(&mut *first)
        .await
        .unwrap();

    let second = pool.begin().await.unwrap();
    let mut probe = pool.acquire().await.unwrap();
    let mut second = second;
    let pid: i32 = sqlx::query_scalar("select pg_backend_pid()")
        .fetch_one(&mut *second)
        .await
        .unwrap();
    let writing = writer(second);
    let mut waits = false;
    for _ in 0..200 {
        let event: Option<String> =
            sqlx::query_scalar("select wait_event_type from pg_stat_activity where pid = $1")
                .bind(pid)
                .fetch_one(&mut *probe)
                .await
                .unwrap();
        if event.as_deref() == Some("Lock") {
            waits = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    drop(probe);
    assert!(waits, "the writer never waited for {}", words[held]);

    let above = words[held + 1..].join(" ");
    let crossed = sqlx::query(row)
        .bind(format!("{s}:2"))
        .bind(&s)
        .bind(&above)
        .execute(&mut *first)
        .await;
    let first_done = match crossed {
        Ok(_) => first.commit().await.map(|_| ()),
        Err(e) => Err(e),
    };
    let second_done = writing.await.unwrap();
    assert!(
        first_done.is_ok() && second_done.is_ok(),
        "the holder: {first_done:?}; the writer: {second_done:?}"
    );
    for word in [&words[0], &words[held], &words[words.len() - 1]] {
        assert!(holds(pool, word).await, "{word}");
    }
}

/// **A batch's new words are taken in their order** (#2551): two syncs that
/// both bring words nobody wrote yet wait for each other at most one way.
/// Before `0104` an insert's words came in hash order, and `knobas-sync`'s
/// two-item test and its 24-item one, which share 501 new words, failed a
/// whole-workspace gate with `deadlock detected`. Here the writer inserts ten
/// rows holding every word in one statement while another transaction holds
/// the middle one.
#[tokio::test]
async fn a_batchs_new_words_are_taken_in_their_order() {
    let pool = pool().await;
    let tag = &uuid::Uuid::new_v4().simple().to_string()[..10];
    let words = numbered(tag);
    let bodies: Vec<String> = (0..10)
        .map(|r| {
            words
                .iter()
                .skip(r)
                .step_by(10)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    crossed_with(&pool, &words, 505, |mut tx| {
        tokio::spawn(async move {
            let s = source("dlbatch");
            let ids: Vec<String> = (0..10).map(|r| format!("{s}:{r}")).collect();
            sqlx::query("insert into knobas.entity (id, kind, title) select i, 'ticket', 'dl' from unnest($1::text[]) i")
                .bind(&ids)
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "insert into sync.item (entity_id, source_id, kind, title, body_text, payload, item_updated_at)
                 select i, $2, 'ticket', 'dl', b, '{}'::jsonb, now() from unnest($1::text[], $3::text[]) as t(i, b)",
            )
            .bind(&ids)
            .bind(&s)
            .bind(&bodies)
            .execute(&mut *tx)
            .await?;
            tx.commit().await
        })
    })
    .await;
}

/// **A rewrite's new words are taken in their order, across its rows**
/// (#2551): `0100` wrote a changed row's words row by row, so an update of
/// several rows took the words of its first rows, above the held one, before
/// it reached the row that waits. Ten rows, each a tenth of the words; the
/// held word is in the sixth.
#[tokio::test]
async fn a_rewrites_new_words_are_taken_in_their_order_across_its_rows() {
    let pool = pool().await;
    let tag = &uuid::Uuid::new_v4().simple().to_string()[..10];
    let words = numbered(tag);
    let s = source("dlrewrite");
    let ids: Vec<String> = (0..10).map(|r| format!("{s}:{r}")).collect();
    for id in &ids {
        item(
            &pool,
            &s,
            "ticket",
            id.rsplit(':').next().unwrap(),
            "dl",
            "nothing new",
            at(0),
        )
        .await;
    }
    let bodies: Vec<String> = (0..10)
        .map(|r| {
            words
                .iter()
                .skip(r)
                .step_by(10)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    crossed_with(&pool, &words, 505, |mut tx| {
        tokio::spawn(async move {
            sqlx::query(
                "update sync.item i set body_text = t.b from unnest($1::text[], $2::text[]) as t(id, b)
                  where i.entity_id = t.id",
            )
            .bind(&ids)
            .bind(&bodies)
            .execute(&mut *tx)
            .await?;
            tx.commit().await
        })
    })
    .await;
}
