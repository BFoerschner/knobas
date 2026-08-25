//! The built-in smart lists, against a real PostgreSQL.
//!
//! # Why these tests serialize
//!
//! A smart list is a **global aggregate**: "changed today" counts every row in
//! the mirror, and the change badge is one shared `knobas.setting` row. Two
//! tests seeding concurrently into the one shared database would each see the
//! other's rows, so there is no per-test token that can isolate a count.
//!
//! Every test therefore takes [`SERIAL`] and measures a **delta**: read the
//! summary, seed n rows, read it again, assert it moved by exactly n. That is
//! sound against rows this run left behind *and* against rows an earlier run
//! left behind, which no absolute count is.

use std::collections::HashSet;
use std::sync::LazyLock;

use chrono::{DateTime, Duration, Utc};
use knobas_search::{KindCatalog, Prefix, SearchError, Searcher, lists};
use tokio::sync::Mutex;

/// Smart lists are whole-corpus aggregates and one shared setting row, so the
/// tests that touch them cannot run concurrently against one database.
static SERIAL: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// The account the `mine` lists are seeded against.
const ME: &str = "mara.lindqvist";

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    // `@me` is the username a source was configured with (interfaces §4.2).
    sqlx::query(
        "insert into knobas.source_config
           (id, kind, display_name, base_url, auth_kind, config)
         values ('jira','jira','Jira','http://x','Pat', $1::jsonb)
         on conflict (id) do update set config = excluded.config, enabled = true",
    )
    .bind(format!(r#"{{"username":"{ME}"}}"#))
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::with_kinds(pool.clone(), KindCatalog::default())
}

fn token(tag: &str) -> String {
    format!("zl{tag}{}", uuid::Uuid::new_v4().simple())
}

/// Seed one mirror row.
///
/// The two timestamps are **bound**, not formatted into the statement: they are
/// the only thing that decides which lists a row lands in, and this crate's one
/// runtime-SQL module is `src/sql.rs` (roadmap §4 gotcha 2, enforced by
/// `tests/sql_containment.rs` -- which scans `tests/` too).
#[allow(clippy::too_many_arguments)]
async fn seed(
    pool: &sqlx::PgPool,
    id: &str,
    kind: &str,
    source_id: &str,
    title: &str,
    author: Option<&str>,
    updated: DateTime<Utc>,
    synced: DateTime<Utc>,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, item_updated_at,
            synced_at, payload)
         values ($1,$2,$3,$4,'body',$5,$6,$7,'{}'::jsonb)",
    )
    .bind(id)
    .bind(source_id)
    .bind(kind)
    .bind(title)
    .bind(author)
    .bind(updated)
    .bind(synced)
    .execute(pool)
    .await
    .unwrap();
}

/// Whether one list is currently badged.
async fn is_changed(s: &Searcher, id: &str) -> bool {
    s.smart_lists()
        .await
        .unwrap()
        .into_iter()
        .find(|l| l.id == id)
        .unwrap_or_else(|| panic!("no list {id}"))
        .changed
}

async fn count_of(s: &Searcher, id: &str) -> i64 {
    s.smart_lists()
        .await
        .unwrap()
        .into_iter()
        .find(|l| l.id == id)
        .unwrap_or_else(|| panic!("no list {id}"))
        .count
}

/// Table-driven over `BUILTINS`: a typo in any one list's SQL fails here, not
/// in the launcher. This is the reason the lists are hand-written constants and
/// not a second little query language.
#[tokio::test]
async fn every_builtin_runs_and_decodes() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let s = searcher(&pool);

    // Something in every list, so a statement that returns no rows is not
    // mistaken for one that parses.
    let t = token("all");
    seed(
        &pool,
        &format!("jira:{t}-1"),
        "ticket",
        "jira",
        &format!("{t} today"),
        Some(ME),
        Utc::now(),
        Utc::now(),
    )
    .await;
    seed(
        &pool,
        &format!("jira:{t}-2"),
        "ticket",
        "jira",
        &format!("{t} stale"),
        Some(ME),
        Utc::now() - Duration::days(30),
        Utc::now(),
    )
    .await;

    for list in lists::BUILTINS {
        let r = s.smart_list_items(list.id, 20).await.expect(list.id);
        assert!(
            r.groups.iter().all(|g| g.hits.len() as u32 <= 20),
            "{}",
            list.id
        );
        assert_eq!(r.interpreted.prefix, Some(Prefix::List), "{}", list.id);
        // The group totals are the list's, not the page's, and the response
        // totals agree with them.
        assert_eq!(
            r.total,
            r.groups.iter().map(|g| g.total).sum::<u32>(),
            "{}",
            list.id
        );
        // A list row is a list row: no ranking, no excerpt.
        assert!(
            r.groups
                .iter()
                .flat_map(|g| &g.hits)
                .all(|h| h.rank == 0.0 && h.snippet.is_empty()),
            "{}",
            list.id
        );
    }

    // The four scan-based lists each found the rows just seeded; `cross-key`
    // needs a second source and gets its own test.
    for id in ["changed-today", "mine", "mine-stale", "just-synced"] {
        assert!(
            !s.smart_list_items(id, 20).await.unwrap().groups.is_empty(),
            "{id} came back empty with rows seeded into it"
        );
    }

    assert!(matches!(
        s.smart_list_items("nope", 20).await,
        Err(SearchError::UnknownList(_))
    ));
    // And the same id typed into the box, since `list:` routes here.
    assert!(matches!(
        s.search(knobas_search::SearchQuery {
            raw: "list:nope".to_owned(),
            limit: 20,
            filters: knobas_search::SearchFilters::default(),
        })
        .await,
        Err(SearchError::UnknownList(_))
    ));
}

/// The counts are real, and they come back in one pass with the summary.
///
/// Measured as a delta because the corpus is shared -- see the module docs.
#[tokio::test]
async fn counts_are_real_and_come_back_in_one_pass() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let s = searcher(&pool);
    let t = token("count");

    let before = s.smart_lists().await.unwrap();
    let base = |id: &str| before.iter().find(|l| l.id == id).unwrap().count;

    // Two items changed today, one of them mine; one of mine long stale.
    seed(
        &pool,
        &format!("jira:{t}-a"),
        "ticket",
        "jira",
        &format!("{t} a"),
        Some(ME),
        Utc::now(),
        Utc::now(),
    )
    .await;
    seed(
        &pool,
        &format!("jira:{t}-b"),
        "ticket",
        "jira",
        &format!("{t} b"),
        Some("someone.else"),
        Utc::now(),
        Utc::now(),
    )
    .await;
    seed(
        &pool,
        &format!("jira:{t}-c"),
        "ticket",
        "jira",
        &format!("{t} c"),
        Some(ME),
        Utc::now() - Duration::days(40),
        Utc::now() - Duration::hours(2),
    )
    .await;

    let after = s.smart_lists().await.unwrap();
    let by = |id: &str| after.iter().find(|l| l.id == id).unwrap().count;

    assert_eq!(by("changed-today") - base("changed-today"), 2);
    // `mine` is the last 30 days, so the 40-day-old row is not in it...
    assert_eq!(by("mine") - base("mine"), 1);
    // ...but it is exactly what `mine-stale` is for.
    assert_eq!(by("mine-stale") - base("mine-stale"), 1);
    // Synced within the hour: two of the three.
    assert_eq!(by("just-synced") - base("just-synced"), 2);

    assert_eq!(after.len(), lists::BUILTINS.len());
    assert_eq!(
        after.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
        lists::BUILTINS.iter().map(|l| l.id).collect::<Vec<_>>()
    );

    // The count and the rows are the same predicate: a summary computed from a
    // different `where` than the list it labels is a wrong number on screen.
    let rows = s.smart_list_items("mine", 200).await.unwrap();
    assert_eq!(u32::try_from(by("mine")).unwrap(), rows.total);
}

/// Opening a list clears its badge; a sync bringing something new raises it
/// again.
#[tokio::test]
async fn the_change_badge_clears_when_the_list_is_opened() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let s = searcher(&pool);
    let t = token("badge");
    let id = format!("jira:{t}-1");
    seed(
        &pool,
        &id,
        "ticket",
        "jira",
        &format!("{t} fresh"),
        None,
        Utc::now(),
        Utc::now(),
    )
    .await;

    assert!(
        is_changed(&s, "changed-today").await,
        "a list holding something new is news"
    );
    s.smart_list_items("changed-today", 20).await.unwrap(); // looked at it
    assert!(
        !is_changed(&s, "changed-today").await,
        "and stops being news once it is opened"
    );

    // A sync brings something new.
    sqlx::query(
        "update sync.item set synced_at = now(), item_updated_at = now() where entity_id = $1",
    )
    .bind(&id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(is_changed(&s, "changed-today").await);

    // Opening one list must not forget that another was opened. The five
    // stamps share one `knobas.setting` row, so a write that *replaced* the
    // object instead of merging into it would wipe every other list's stamp --
    // and the symptom is not a badge that stays on, it is one that comes back.
    //
    // The order is the whole test: `changed-today` has to be **quiet** first,
    // or a stamp that was wiped is indistinguishable from one that was never
    // written (both read as "never opened", which is `changed`).
    s.smart_list_items("changed-today", 20).await.unwrap();
    assert!(!is_changed(&s, "changed-today").await);
    s.smart_list_items("just-synced", 20).await.unwrap();
    assert!(
        !is_changed(&s, "changed-today").await,
        "opening `just-synced` forgot that `changed-today` had been opened"
    );
    assert!(!is_changed(&s, "just-synced").await);
}

/// A ticket key mentioned in another system's text: the join spec §4 sells as
/// "a query JQL cannot express", done without links (which are M2).
#[tokio::test]
async fn the_cross_source_list_finds_what_no_single_source_could() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let s = searcher(&pool);
    let t = token("cross");
    let key = format!("{t}-PAY-231");
    let ticket = format!("jira:{key}");

    seed(
        &pool,
        &ticket,
        "ticket",
        "jira",
        "Retry failed SEPA payouts",
        None,
        Utc::now(),
        Utc::now(),
    )
    .await;
    // A ticket nobody else mentions, so the list is a filter and not a listing
    // of every recent ticket.
    let lonely = format!("jira:{t}-PAY-999");
    seed(
        &pool,
        &lonely,
        "ticket",
        "jira",
        "Nobody links to this one",
        None,
        Utc::now(),
        Utc::now(),
    )
    .await;

    let found = |r: &knobas_search::SearchResponse, id: &str| {
        r.groups
            .iter()
            .flat_map(|g| &g.hits)
            .any(|h| h.row.entity_id == id)
    };

    // Before the mention exists, neither ticket qualifies.
    let before = s.smart_list_items("cross-key", 20).await.unwrap();
    assert!(!found(&before, &ticket), "nothing mentions it yet");

    // A PR in *another* source that names the ticket by key.
    seed(
        &pool,
        &format!("gitea:{t}-pr"),
        "pr",
        "gitea",
        &format!("{key} sepa retry"),
        None,
        Utc::now(),
        Utc::now(),
    )
    .await;

    let r = s.smart_list_items("cross-key", 20).await.unwrap();
    assert!(
        found(&r, &ticket),
        "the mentioned ticket is the whole point"
    );
    assert!(
        !found(&r, &lonely),
        "a ticket nothing mentions must not be in a cross-reference list"
    );
    // The PR itself is not in the list: it is the evidence, not the answer.
    assert!(
        r.groups.iter().all(|g| g.kind == "ticket"),
        "{:?}",
        r.groups
    );
    // And the same source mentioning its own key is not a cross-reference.
    assert!(count_of(&s, "cross-key").await >= 1);
}

/// A list is bounded by its limit, and the group totals still tell the truth
/// about how big it is.
#[tokio::test]
async fn the_limit_cuts_the_page_and_not_the_count() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let s = searcher(&pool);
    let t = token("limit");
    for n in 0..5 {
        seed(
            &pool,
            &format!("jira:{t}-{n}"),
            "ticket",
            "jira",
            &format!("{t} {n}"),
            None,
            Utc::now() - Duration::minutes(n),
            Utc::now(),
        )
        .await;
    }

    let full = s.smart_list_items("just-synced", 200).await.unwrap();
    let paged = s.smart_list_items("just-synced", 2).await.unwrap();
    assert_eq!(
        paged.groups.iter().map(|g| g.hits.len()).sum::<usize>(),
        2,
        "the page is the limit"
    );
    assert_eq!(paged.total, full.total, "the totals are not the page");
    assert!(full.total >= 5);

    // And the page is the *newest* two, not any two. Asserted as "nothing that
    // was dropped is newer than anything that was kept" rather than as a list
    // of ids, because the corpus is shared and this test does not own every
    // row in the list -- but it does own the ordering.
    let kept: HashSet<&str> = paged
        .groups
        .iter()
        .flat_map(|g| &g.hits)
        .map(|h| h.row.entity_id.as_str())
        .collect();
    let oldest_kept = paged
        .groups
        .iter()
        .flat_map(|g| &g.hits)
        .map(|h| h.row.updated_at)
        .min()
        .expect("the page has rows");
    let newest_dropped = full
        .groups
        .iter()
        .flat_map(|g| &g.hits)
        .filter(|h| !kept.contains(h.row.entity_id.as_str()))
        .map(|h| h.row.updated_at)
        .max()
        .expect("more rows than fit on the page");
    assert!(
        oldest_kept >= newest_dropped,
        "the limit kept {oldest_kept:?} and dropped {newest_dropped:?}"
    );

    // A limit of nothing is a caller asking for a page smaller than the
    // launcher draws, not for silence.
    let clamped = s.smart_list_items("just-synced", 0).await.unwrap();
    assert_eq!(
        clamped.groups.iter().map(|g| g.hits.len()).sum::<usize>(),
        1
    );

    // Newest first within the group.
    let stamps: Vec<_> = full.groups[0]
        .hits
        .iter()
        .map(|h| h.row.updated_at)
        .collect();
    assert!(stamps.windows(2).all(|w| w[0] >= w[1]), "{stamps:?}");
    // And a list row still carries its provenance.
    let hit = &full.groups[0].hits[0];
    assert!(!hit.row.source_id.is_empty());
    assert!(hit.row.synced_at <= Utc::now());
}

/// `list:<id>` in the box is the same code path as the command, including the
/// badge write.
#[tokio::test]
async fn typing_a_list_in_the_box_answers_like_the_command() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let s = searcher(&pool);

    let typed = s
        .search(knobas_search::SearchQuery {
            raw: "list:MINE".to_owned(),
            limit: 20,
            filters: knobas_search::SearchFilters::default(),
        })
        .await
        .unwrap();
    let called = s.smart_list_items("mine", 20).await.unwrap();
    assert_eq!(typed.interpreted.prefix, Some(Prefix::List));
    assert_eq!(typed.total, called.total);
    assert_eq!(
        typed
            .groups
            .iter()
            .flat_map(|g| &g.hits)
            .map(|h| h.row.entity_id.as_str())
            .collect::<Vec<_>>(),
        called
            .groups
            .iter()
            .flat_map(|g| &g.hits)
            .map(|h| h.row.entity_id.as_str())
            .collect::<Vec<_>>()
    );
}

/// With an account configured, `mine` says what it is rather than why it is
/// empty. The other half of this -- the install with no username at all -- is
/// `tests/lists_no_identity.rs`, which needs a database no source is
/// configured in and therefore a test binary of its own.
#[tokio::test]
async fn a_configured_identity_leaves_the_blurb_alone() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let mine = searcher(&pool)
        .smart_lists()
        .await
        .unwrap()
        .into_iter()
        .find(|l| l.id == "mine")
        .unwrap();
    assert_eq!(mine.description, lists::find("mine").unwrap().blurb);
    assert!(!mine.description.contains("username"));
}

/// "Changed today" starts at **midnight**, not 24 hours ago.
///
/// The distinction is invisible for most of the day and is exactly what a
/// user notices: a ticket touched at 23:59 yesterday is not something that
/// changed today, however recently it happened. Both rows are placed against
/// the server's own `date_trunc('day', now())` rather than a Rust clock, so
/// the test does not depend on the session's time zone matching UTC.
#[tokio::test]
async fn changed_today_starts_at_midnight_and_is_not_a_rolling_day() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let s = searcher(&pool);
    let t = token("midnight");
    let at_midnight = format!("jira:{t}-in");
    let just_before = format!("jira:{t}-out");

    let before = count_of(&s, "changed-today").await;
    for id in [&at_midnight, &just_before] {
        seed(
            &pool,
            id,
            "ticket",
            "jira",
            id,
            None,
            Utc::now(),
            Utc::now(),
        )
        .await;
    }
    sqlx::query(
        "update sync.item set item_updated_at = date_trunc('day', now()) where entity_id = $1",
    )
    .bind(&at_midnight)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "update sync.item
            set item_updated_at = date_trunc('day', now()) - interval '1 microsecond'
          where entity_id = $1",
    )
    .bind(&just_before)
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        count_of(&s, "changed-today").await - before,
        1,
        "the first instant of today counts and the last instant of yesterday does not"
    );
    let rows = s.smart_list_items("changed-today", 200).await.unwrap();
    let ids: Vec<&str> = rows
        .groups
        .iter()
        .flat_map(|g| &g.hits)
        .map(|h| h.row.entity_id.as_str())
        .collect();
    assert!(ids.contains(&at_midnight.as_str()), "{ids:?}");
    assert!(!ids.contains(&just_before.as_str()), "{ids:?}");
}

/// A caller asking for more rows than the launcher draws gets the page the
/// launcher draws.
///
/// The bound is the engine's, not the UI's: `smart_list_items` is an IPC
/// command, so "how many rows" is a number a caller sends, and an unbounded
/// one is an unbounded response over the bridge.
#[tokio::test]
async fn a_list_page_is_bounded_however_much_the_caller_asks_for() {
    let _guard = SERIAL.lock().await;
    let pool = pool().await;
    let s = searcher(&pool);
    let t = token("bound");

    sqlx::query(
        "insert into knobas.entity (id, kind, title)
         select $1 || ':' || g, 'ticket', 'row ' || g from generate_series(1, $2) g",
    )
    .bind(&t)
    .bind(250_i32)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, item_updated_at, synced_at, payload)
         select $1 || ':' || g, 'jira', 'ticket', 'row ' || g, 'body',
                now() - make_interval(secs => g), now(), '{}'::jsonb
           from generate_series(1, $2) g",
    )
    .bind(&t)
    .bind(250_i32)
    .execute(&pool)
    .await
    .unwrap();

    let r = s.smart_list_items("just-synced", 10_000).await.unwrap();
    assert_eq!(
        r.groups.iter().map(|g| g.hits.len()).sum::<usize>(),
        200,
        "the engine's page cap, not the caller's number"
    );
    assert!(r.total >= 250, "and the total still counts the whole list");
}
