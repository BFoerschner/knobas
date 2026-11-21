//! A ticket key typed with a space or none finds the ticket (ux25; Björn:
//! "typing a ticket key and id with space instead of - doesn't match. I want
//! to be able to type like pay 123 and it shows up pay-123").
//!
//! `pay 123`, `PAY 123`, `pay123`, `pay_123` and `pay.123` read as `PAY-123`
//! where `PAY` is a project the mirror's tickets are numbered under: the
//! launcher, the Docs search and the Tickets view's word filter answer the
//! key as they answer `PAY-123`, first, and keep what the words as typed
//! found below it. The projects are what the searcher's picker holds.
//!
//! Each test takes a database of its own: the projects are every ticket's,
//! so a row another test wrote would make its project one.

use chrono::{Duration, Utc};
use knobas_search::picker::Picker;
use knobas_search::{Reading, SearchFilters, SearchQuery, SearchResponse, Searcher};

async fn pool(name: &str) -> sqlx::PgPool {
    knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool on the scratch database")
}

/// One mirrored row, its id `<source>:<key>`, `age` days old.
async fn row(pool: &sqlx::PgPool, kind: &str, id: &str, title: &str, body: &str, age: i64) {
    let (source, _) = id.split_once(':').unwrap();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload,
                                item_updated_at)
         values ($1,$2,$3,$4,$5,'{}'::jsonb,$6)",
    )
    .bind(id)
    .bind(source)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(Utc::now() - Duration::days(age))
    .execute(pool)
    .await
    .unwrap();
}

async fn ticket(pool: &sqlx::PgPool, key: &str, title: &str, age: i64) {
    row(pool, "ticket", &format!("jira:{key}"), title, title, age).await;
}

async fn sources(pool: &sqlx::PgPool) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind)
         values ('jira', 'jira', 'Jira', 'http://x', 'Pat'),
                ('wiki', 'confluence', 'Wiki', 'http://y', 'Pat')",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// A searcher whose picker holds what the mirror holds now.
async fn searcher(pool: &sqlx::PgPool) -> Searcher {
    let picker = Picker::default();
    picker.reload(pool).await.unwrap();
    Searcher::new(pool.clone()).with_picker(picker)
}

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        titles: false,
        raw: raw.to_owned(),
        limit: 40,
        filters: SearchFilters::default(),
    }
}

fn kind(response: &SearchResponse, kind: &str) -> Vec<String> {
    response
        .groups
        .iter()
        .filter(|group| group.kind == kind)
        .flat_map(|group| group.hits.iter().map(|hit| hit.row.entity_id.clone()))
        .collect()
}

fn holds(response: &SearchResponse, id: &str) -> bool {
    response
        .groups
        .iter()
        .any(|group| group.hits.iter().any(|hit| hit.row.entity_id == id))
}

/// **Every spelling finds the ticket first**, as `PAY-123` does, and what
/// the words as typed found stays below it: the page that says *pay* and
/// *123*, and for `pay 123` the newer ticket that holds both words. The
/// echo says what the text was read as, and nothing is corrected.
#[tokio::test]
async fn every_spelling_finds_the_key_first() {
    let pool = pool("typed-key-spellings").await;
    sources(&pool).await;
    ticket(&pool, "PAY-123", "Retry failed SEPA payouts", 40).await;
    ticket(&pool, "PAY-1234", "Unrelated payout report", 30).await;
    ticket(&pool, "PAY-7", "Pay the 123 invoices of the vendor run", 1).await;
    row(
        &pool,
        "page",
        "wiki:9001",
        "Pay run 123 notes",
        "pay 123 pay 123",
        1,
    )
    .await;
    let s = searcher(&pool).await;

    let dashed = s.search(q("PAY-123")).await.unwrap();
    assert_eq!(
        kind(&dashed, "ticket").first().map(String::as_str),
        Some("jira:PAY-123")
    );
    assert_eq!(
        dashed.interpreted.keyed, None,
        "a key typed with its `-` needs no reading"
    );

    for typed in [
        "pay 123", "PAY 123", "Pay 123 ", "pay123", "PAY123", "pay_123", "pay.123",
    ] {
        let r = s.search(q(typed)).await.unwrap();
        let tickets = kind(&r, "ticket");
        assert_eq!(
            tickets.first().map(String::as_str),
            Some("jira:PAY-123"),
            "`{typed}`: {tickets:?}"
        );
        assert_eq!(r.interpreted.keyed.as_deref(), Some("PAY-123"), "`{typed}`");
        assert!(
            r.interpreted.text.contains(typed.trim()),
            "the echo keeps the words as typed"
        );
    }
    // The words' own answer stays, below the key's.
    let spaced = s.search(q("pay 123 ")).await.unwrap();
    assert!(holds(&spaced, "wiki:9001"), "{:?}", spaced.groups);
    let tickets = kind(&spaced, "ticket");
    let key = tickets.iter().position(|id| id == "jira:PAY-123").unwrap();
    let words = tickets
        .iter()
        .position(|id| id == "jira:PAY-7")
        .expect("the words' ticket");
    assert!(key < words, "{tickets:?}");
}

/// **A part of the number** lists the keys it begins, as `PAY-12` does:
/// `pay 12` while typing finds PAY-12, PAY-123 and PAY-1234 first, and not
/// PAY-7 or a key that holds 12 elsewhere.
#[tokio::test]
async fn a_part_of_the_number_lists_the_keys_it_begins() {
    let pool = pool("typed-key-part").await;
    sources(&pool).await;
    ticket(&pool, "PAY-12", "Payout batches", 50).await;
    ticket(&pool, "PAY-123", "Retry failed SEPA payouts", 40).await;
    ticket(&pool, "PAY-1234", "Unrelated payout report", 30).await;
    ticket(&pool, "PAY-512", "Fees", 1).await;
    ticket(&pool, "PAY-7", "Fees again", 1).await;
    let s = searcher(&pool).await;
    let dashed = kind(&s.search(q("PAY-12")).await.unwrap(), "ticket");
    for typed in ["pay 12", "pay12", "PAY_12"] {
        let r = s.search(q(typed)).await.unwrap();
        let tickets = kind(&r, "ticket");
        assert_eq!(r.interpreted.keyed.as_deref(), Some("PAY-12"));
        let mut first: Vec<&str> = tickets.iter().take(3).map(String::as_str).collect();
        first.sort_unstable();
        assert_eq!(
            first,
            ["jira:PAY-12", "jira:PAY-123", "jira:PAY-1234"],
            "`{typed}`: {tickets:?}"
        );
        assert_eq!(
            &tickets[..3],
            &dashed[..3],
            "`{typed}` leads as `PAY-12` does"
        );
        assert!(
            !tickets.contains(&"jira:PAY-512".to_owned()),
            "`{typed}`: {tickets:?}"
        );
    }
}

/// **The number is never fuzzed**: `pay 132` reads `PAY-132`, which no
/// ticket is, and finds no PAY-123.
#[tokio::test]
async fn the_number_is_never_fuzzed() {
    let pool = pool("typed-key-fuzz").await;
    sources(&pool).await;
    ticket(&pool, "PAY-123", "Retry failed SEPA payouts", 40).await;
    ticket(&pool, "PAY-9", "Fees", 1).await;
    let s = searcher(&pool).await;
    for typed in ["pay 132", "pay132", "pay 132 "] {
        let r = s.search(q(typed)).await.unwrap();
        assert_eq!(r.interpreted.keyed.as_deref(), Some("PAY-132"));
        assert!(!holds(&r, "jira:PAY-123"), "`{typed}`: {:?}", r.groups);
    }
}

/// **A word that names no project stays a word**: `top 10` and `java 21`
/// read no key, and nothing is asked beside the words; the projects come
/// from tickets alone, so a page whose id is shaped like a key makes none.
#[tokio::test]
async fn a_word_that_names_no_project_stays_a_word() {
    let pool = pool("typed-key-no-project").await;
    sources(&pool).await;
    ticket(&pool, "PAY-10", "Top 10 payout risks", 40).await;
    row(
        &pool,
        "page",
        "wiki:TOP-10",
        "Top 10 risks",
        "java 21 upgrade",
        1,
    )
    .await;
    let s = searcher(&pool).await;
    for typed in ["top 10", "top10", "java 21", "java21"] {
        let r = s.search(q(typed)).await.unwrap();
        // Nothing read, nothing asked beside the words: the answer is the
        // words' and the picker's names, as it was.
        assert_eq!(r.interpreted.keyed, None, "`{typed}`");
        assert_eq!(
            kind(&r, "ticket").first().map(String::as_str),
            typed.starts_with("top").then_some("jira:PAY-10"),
            "`{typed}`"
        );
    }
    assert!(holds(&s.search(q("top 10")).await.unwrap(), "wiki:TOP-10"));
}

/// **A project that is also a common word** reads as the key, its ticket
/// first, and the rows the words found stay: `top 10` finds TOP-10 first
/// and still the page about the top ten.
#[tokio::test]
async fn a_project_that_is_a_common_word_reads_as_the_key() {
    let pool = pool("typed-key-common-word").await;
    sources(&pool).await;
    ticket(&pool, "TOP-10", "Quarterly objectives", 40).await;
    ticket(&pool, "TOP-3", "Top 10 customers by volume", 1).await;
    row(
        &pool,
        "page",
        "wiki:9002",
        "Top 10 risks",
        "the top 10 risks",
        1,
    )
    .await;
    let s = searcher(&pool).await;
    let r = s.search(q("top 10")).await.unwrap();
    assert_eq!(r.interpreted.keyed.as_deref(), Some("TOP-10"));
    let tickets = kind(&r, "ticket");
    assert_eq!(
        tickets.first().map(String::as_str),
        Some("jira:TOP-10"),
        "{tickets:?}"
    );
    assert!(tickets.contains(&"jira:TOP-3".to_owned()), "{tickets:?}");
    assert!(holds(&r, "wiki:9002"), "{:?}", r.groups);
}

/// **Two projects, one the start of the other**: `payout 12` is PAYOUT-12
/// and `pay 12` is PAY-12, in every spelling.
#[tokio::test]
async fn a_project_that_begins_another_reads_as_itself() {
    let pool = pool("typed-key-prefixes").await;
    sources(&pool).await;
    ticket(&pool, "PAY-12", "Fees", 40).await;
    ticket(&pool, "PAYOUT-12", "Batches", 40).await;
    let s = searcher(&pool).await;
    for (typed, key) in [
        ("payout 12", "PAYOUT-12"),
        ("payout12", "PAYOUT-12"),
        ("payout_12 ", "PAYOUT-12"),
        ("pay 12", "PAY-12"),
        ("pay12 ", "PAY-12"),
        ("pay.12", "PAY-12"),
    ] {
        let r = s.search(q(typed)).await.unwrap();
        assert_eq!(r.interpreted.keyed.as_deref(), Some(key), "`{typed}`");
        let tickets = kind(&r, "ticket");
        assert_eq!(
            tickets.first().map(String::as_str),
            Some(format!("jira:{key}").as_str()),
            "`{typed}`: {tickets:?}"
        );
    }
}

/// **The Docs search reads the key too**: `pay123` finds the page that names
/// PAY-123, first, and the pages the words find stay.
#[tokio::test]
async fn the_docs_search_reads_the_key() {
    let pool = pool("typed-key-docs").await;
    sources(&pool).await;
    ticket(&pool, "PAY-123", "Retry failed SEPA payouts", 40).await;
    row(
        &pool,
        "page",
        "wiki:1",
        "SEPA retries",
        "Blocked by PAY-123 until retries land.",
        30,
    )
    .await;
    row(&pool, "page", "wiki:2", "Pay day", "pay 123 pay", 1).await;
    let s = searcher(&pool).await;
    let found = s
        .page_search("pay123", None, 20)
        .await
        .unwrap()
        .expect("words");
    let ids: Vec<&str> = found
        .hits
        .iter()
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert_eq!(
        ids,
        ["wiki:1"],
        "only the page that names the key holds `pay123`"
    );
    let spaced = s
        .page_search("pay 123", None, 20)
        .await
        .unwrap()
        .expect("words");
    let ids: Vec<&str> = spaced
        .hits
        .iter()
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert_eq!(ids.first(), Some(&"wiki:1"), "{ids:?}");
    assert!(ids.contains(&"wiki:2"), "{ids:?}");
    // Without a picker nothing is read: `pay123` names no page.
    let plain = Searcher::new(pool.clone())
        .page_search("pay123", None, 20)
        .await
        .unwrap()
        .expect("words");
    assert!(plain.hits.is_empty());
}

/// **The Tickets view's and the boards' word filter** admits the key beside
/// what the words admit: `pay123` admits PAY-123 and the keys it begins, as
/// `PAY-123` does, and `pay 123` what it admitted before. The views' counts
/// read the same.
#[tokio::test]
async fn the_word_filter_admits_the_key() {
    let pool = pool("typed-key-filter").await;
    sources(&pool).await;
    ticket(&pool, "PAY-123", "Retry failed SEPA payouts", 40).await;
    ticket(&pool, "PAY-1234", "Unrelated payout report", 30).await;
    ticket(&pool, "PAY-12", "Fees", 30).await;
    ticket(&pool, "TOP-1", "Quarterly objectives", 30).await;
    let s = searcher(&pool).await;
    let sorted = |ids: Option<Vec<String>>| {
        let mut ids = ids.expect("narrowed");
        ids.sort();
        ids
    };
    let dashed = sorted(s.matching_ids("PAY-123", "ticket").await.unwrap());
    assert_eq!(dashed, ["jira:PAY-123", "jira:PAY-1234"]);
    for typed in ["pay123", "pay_123", "pay.123", "PAY 123", "pay 123"] {
        let ids = sorted(s.matching_ids(typed, "ticket").await.unwrap());
        assert_eq!(ids, dashed, "`{typed}`");
    }
    assert_eq!(
        sorted(s.matching_ids("pay 132", "ticket").await.unwrap()),
        Vec::<String>::new()
    );
    // Without a picker, `pay123` is the word it was.
    let plain = Searcher::new(pool.clone());
    assert_eq!(
        sorted(plain.matching_ids("pay123", "ticket").await.unwrap()),
        Vec::<String>::new()
    );
    // The views' counts read the texts as the filter does.
    let readings = s
        .readings(&["pay123", "PAY-123", "top 1"], "ticket")
        .await
        .unwrap();
    assert_eq!(
        readings,
        [Reading::Count(2), Reading::Count(2), Reading::Count(1)]
    );
}

/// **A key that answered is no word nobody wrote**: `pay123` is not in the
/// mirror's words, and the nearest one (`pay124`, which a page says) is not
/// asked in its place, because the key found the ticket (D10: a correction is
/// only for text that found nothing). Without the key read, it would be.
#[tokio::test]
async fn a_key_that_answered_is_not_corrected() {
    let pool = pool("typed-key-corrected").await;
    sources(&pool).await;
    ticket(&pool, "PAY-123", "Retry failed SEPA payouts", 40).await;
    row(&pool, "page", "wiki:1", "Vendor run", "the pay124 batch", 1).await;
    knobas_core::word_list::fill(&pool).await.unwrap();
    let s = searcher(&pool).await;
    let r = s.search(q("pay123 ")).await.unwrap();
    assert_eq!(r.interpreted.keyed.as_deref(), Some("PAY-123"));
    assert_eq!(kind(&r, "ticket"), ["jira:PAY-123"]);
    assert!(!holds(&r, "wiki:1"), "{:?}", r.groups);
    // The same word where no project reads it is corrected, as before.
    let other = s.search(q("pqy124 ")).await.unwrap();
    assert!(holds(&other, "wiki:1"), "{:?}", other.groups);
}

/// **The first key typed after a start reads as the key** (ux26): a picker
/// whose names are not read yet (its first reload still running at
/// bring-up) reads the projects on their own for the first search, so
/// `pay123` finds PAY-123 first then too; it keeps them, and reads no name.
/// A reload reads them before the names.
#[tokio::test]
async fn a_key_reads_as_the_key_before_the_names_are_read() {
    let pool = pool("typed-key-early").await;
    sources(&pool).await;
    ticket(&pool, "PAY-123", "Retry failed SEPA payouts", 40).await;
    ticket(&pool, "PAY-7", "Pay the 123 invoices of the vendor run", 1).await;
    // A page whose id is shaped as a key is no ticket: its key names no project.
    row(&pool, "page", "wiki:TOP-1", "Quarterly notes", "top", 1).await;
    let picker = Picker::default();
    assert!(picker.projects().is_empty(), "nothing read yet");
    let s = Searcher::new(pool.clone()).with_picker(picker.clone());

    let r = s.search(q("pay123")).await.unwrap();
    assert_eq!(
        kind(&r, "ticket").first().map(String::as_str),
        Some("jira:PAY-123"),
        "{:?}",
        r.groups
    );
    assert_eq!(r.interpreted.keyed.as_deref(), Some("PAY-123"));
    assert!(!picker.holds_names(), "the projects alone were read");
    assert!(picker.projects().holds("pay"));
    assert!(
        !picker.projects().holds("top"),
        "a page's key names no project"
    );
    let sorted = |ids: Option<Vec<String>>| {
        let mut ids = ids.expect("narrowed");
        ids.sort();
        ids
    };
    assert_eq!(
        sorted(s.matching_ids("pay 123", "ticket").await.unwrap()),
        ["jira:PAY-123", "jira:PAY-7"]
    );

    // Read once: a project the mirror gains later comes with the names.
    ticket(&pool, "OPS-5", "Rotate the keys", 1).await;
    assert!(!picker.projects_read(&pool).await.unwrap().holds("ops"));
    picker.reload(&pool).await.unwrap();
    assert!(picker.projects().holds("ops"));
    assert!(picker.projects().holds("pay"));
}
