//! The built-in smart lists, against a real PostgreSQL.
//!
//! # Why these tests serialize, and what that does and does not buy
//!
//! A smart list is a **global aggregate**: "changed today" counts every row in
//! the mirror, and the change badge is one shared `knobas.setting` row. Two
//! tests seeding concurrently into the one shared database would each see the
//! other's rows mid-flight, so there is no per-test token that can isolate a
//! count.
//!
//! Every test therefore takes [`SERIAL`] and measures a **delta**: read the
//! summary, seed n rows, read it again, assert it moved by exactly n. A delta
//! is sound whatever the other tests in this run have already seeded, which no
//! absolute count is.
//!
//! **The mutex serialises; it does not order.** libtest picks the order, so a
//! test may run before or after any other test in this binary and must be
//! correct either way. That distinction is not pedantry: it is the actual cause
//! of the one failure this file had. `changed_today_starts_at_midnight_...`
//! originally looked for its row on a page of 200, and the row it looks for is
//! by construction the *oldest* thing in the list -- so it passed when it ran
//! before `a_list_page_is_bounded_...` (which seeds 250 rows) and failed when it
//! ran after. It was misdiagnosed as state surviving between runs; the database
//! is fresh per run (`test_util::run_nonce` is `{pid}-{nanos}`, so an earlier
//! run's directory can never match this process's stamp and is deleted), and
//! the real variable was the ordering inside a single run.
//!
//! The two rules that follow, and that every test here obeys:
//!
//! * assert **deltas**, never absolute counts;
//! * never assert that a specific row is *on a page*, because what else is on
//!   that page depends on which tests have already run. Assert through `total`,
//!   or assert an ordering property over whatever came back.

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

/// Seed one asset into each of the three estate lists (#504).
///
/// By hand and not through `knobas_app::assets`, which this crate does not
/// depend on and must not: `knobas-search` takes a `PgPool` and reads. What
/// that costs is `path_text`, which the store maintains and nothing here needs
/// -- these lists are asserted on membership, and the wire-level check that an
/// estate row carries its path is `crates/knobas-app/tests/search_ipc.rs`'
/// business, where the rows are made through the real door.
///
/// Two assets, because the three lists are three different rules and one asset
/// cannot be a negative for any of them:
///
/// * `bare` -- nothing attached. *Not monitored*.
/// * `watched` -- a monitor linked to it, whose payload carries a certificate
///   with five days left, and an open alert; and an unarchived context holding
///   it. On *both* other lists, and **off** *Not monitored*, which is the
///   negative this seed also buys.
///
/// Returns the two asset ids.
async fn estate(pool: &sqlx::PgPool, tag: &str) -> (String, String) {
    let t = token(tag);
    let bare = format!("asset:{t}-bare");
    let watched = format!("asset:{t}-watched");
    let monitor = format!("kuma:{t}");
    let context = format!("ctx:{t}");

    for (id, kind, title) in [
        (&bare, "asset", "bare"),
        (&watched, "asset", "watched"),
        (&monitor, "monitor", "the check"),
        (&context, "ctx", "the room"),
    ] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
            .bind(id)
            .bind(kind)
            .bind(title)
            .execute(pool)
            .await
            .expect("the entity row");
    }
    for (id, name) in [(&bare, "bare"), (&watched, "watched")] {
        sqlx::query(
            "insert into knobas.asset (id, type_id, name, path_text) values ($1,'vm',$2,'')",
        )
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .expect("the asset row");
    }
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, item_updated_at, synced_at, payload)
         values ($1,'kuma','monitor','the check','', now(), now(),
                 jsonb_build_object('cert_days_remaining', 5))",
    )
    .bind(&monitor)
    .execute(pool)
    .await
    .expect("the mirrored monitor");
    sqlx::query("insert into knobas.context (id, kind, title) values ($1,'adhoc','the room')")
        .bind(&context)
        .execute(pool)
        .await
        .expect("the context");
    for (from, to, relation) in [
        (&watched, &monitor, "monitored-by"),
        (&context, &watched, "related"),
    ] {
        sqlx::query(
            "insert into knobas.link (from_id, to_id, relation, origin, created_by, confirmed_at)
             values ($1,$2,$3,'manual','test', now())",
        )
        .bind(from)
        .bind(to)
        .bind(relation)
        .execute(pool)
        .await
        .expect("the confirmed link");
    }
    sqlx::query("insert into knobas.monitor_alert (entity_id, state) values ($1,'down')")
        .bind(&monitor)
        .execute(pool)
        .await
        .expect("the open alert");
    (bare, watched)
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
    // And something in each of the three estate lists, which no mirror row can
    // reach (#504).
    estate(&pool, "all").await;

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

    // Every list found the rows just seeded, so "the statement parses" is not
    // being mistaken for "the statement is right".
    for id in lists::BUILTINS.iter().map(|l| l.id) {
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

    let count = count_of(&s, "changed-today").await;
    assert_eq!(
        count - before,
        1,
        "the first instant of today counts and the last instant of yesterday does not"
    );

    // The rows obey the same boundary as the count. Asserted through `total`
    // rather than by looking for the two ids on the page: the row at exactly
    // midnight is by construction the *oldest* thing in the list, so on a
    // corpus with more than a page of items it is correctly not on the first
    // page -- and a membership assertion would then be testing the paging, not
    // the boundary.
    let rows = s.smart_list_items("changed-today", 200).await.unwrap();
    assert_eq!(u32::try_from(count).unwrap(), rows.total);
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
                -- Synced within the hour (so `just-synced` holds them) but
                -- last *changed* days ago, so 250 rows do not land in every
                -- other list's window and crowd out the tests that own it.
                now() - interval '3 days' - make_interval(secs => g), now(),
                '{}'::jsonb
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
