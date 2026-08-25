//! The room's read, against a real PostgreSQL and the mock corpus.
//!
//! `test_util` hands every test in this binary the *same* database, so the
//! corpus is seeded once (see [`seeded`]) and every assertion below is written
//! to survive another test running beside it: relative counts and set
//! membership, never absolute row totals.

use knobas_app::commands::entity::{
    EntityFilter, EntityOrder, get_entity_inner, list_entities_inner, recent_activity_inner,
};
use knobas_source_mock::MockSource;
use sqlx::PgPool;

/// The corpus every test reads: the demo load, plus the one tombstone the
/// fixture does not otherwise contain.
///
/// Seeded at most once per test binary. The guard is held across the whole
/// seed rather than around each half: two concurrent full syncs of the same
/// source would each be correct on their own, but the sweep of the plain
/// fixture and the tombstone run interleaved decide `PAY-198`'s state by
/// whichever committed last.
async fn seeded() -> PgPool {
    static SEEDED: tokio::sync::Mutex<bool> = tokio::sync::Mutex::const_new(false);

    let pool = knobas_db::test_util::test_pool().await;
    let mut done = SEEDED.lock().await;
    if !*done {
        knobas_db::migrate::run(&pool).await.unwrap();
        knobas_app::sources::demo::demo_load_inner(&pool)
            .await
            .unwrap();
        // The tombstone channel, from the reference adapter rather than from a
        // test-local source: `with_tombstone` reports `PAY-198` as deleted, so
        // `knobas.entity.deleted_at` is set while its mirror row stays.
        knobas_sync::run_once(&pool, &MockSource::with_tombstone(), None)
            .await
            .unwrap();
        *done = true;
    }
    drop(done);
    pool
}

/// A token no other test in this binary writes.
///
/// `knobas.activity` is shared with every other test here, so a run's own
/// lines have to be findable by something only it wrote -- a fixed verb would
/// make the counts below depend on which tests ran first.
fn unique() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{nanos}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/// Everything, newest first, nothing excluded.
fn all() -> EntityFilter {
    EntityFilter {
        sources: Vec::new(),
        kinds: Vec::new(),
        updated_within_days: None,
        order: EntityOrder::UpdatedDesc,
        include_deleted: false,
    }
}

#[tokio::test]
async fn lists_the_newest_first_and_reports_the_unpaged_total() {
    let pool = seeded().await;
    let filter = EntityFilter {
        kinds: vec!["ticket".to_owned()],
        ..all()
    };
    let page = list_entities_inner(&pool, &filter, 2, 0).await.unwrap();

    assert_eq!(page.rows.len(), 2, "limit is honoured");
    assert!(
        page.total > 2,
        "total counts the whole filtered set, not the page: {}",
        page.total
    );
    assert!(page.rows[0].updated_at >= page.rows[1].updated_at);
    assert!(page.rows.iter().all(|r| r.kind == "ticket"));
    assert!(page.rows.iter().all(|r| r.source_id == "mock"));
}

/// An empty `sources`/`kinds` means *no filter*, not *no rows*.
///
/// `= any('{}')` matches nothing, so an empty `Vec` bound as an empty array
/// would empty every room in the app while every query still succeeded. The
/// binding is `None`, and this is what says so.
#[tokio::test]
async fn an_empty_filter_list_means_unfiltered_not_empty() {
    let pool = seeded().await;
    let page = list_entities_inner(&pool, &all(), 5, 0).await.unwrap();

    assert_eq!(page.rows.len(), 5);
    assert!(page.total > 5, "the whole corpus, not one kind of it");
    assert!(
        page.rows
            .iter()
            .map(|r| &r.kind)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 1,
        "an unfiltered read spans more than one kind"
    );
}

/// The page is a window on the filtered set, and `total` describes the set.
///
/// Scoped to the `mock` source, and that is not tidiness: the database is
/// shared by every test in this binary, so an unfiltered read is a moving
/// target — a row another test inserts between the two calls below shifts the
/// window and makes page two repeat a row from page one. The mock corpus is
/// written once, by [`seeded`], under a mutex.
#[tokio::test]
async fn offset_walks_the_same_ordering_and_total_does_not_move() {
    let pool = seeded().await;
    let mine = EntityFilter {
        sources: vec!["mock".to_owned()],
        ..all()
    };
    let first = list_entities_inner(&pool, &mine, 3, 0).await.unwrap();
    let second = list_entities_inner(&pool, &mine, 3, 3).await.unwrap();

    assert_eq!(
        first.total, second.total,
        "total is of the set, not the page"
    );
    let overlap = first
        .rows
        .iter()
        .filter(|a| second.rows.iter().any(|b| b.entity_id == a.entity_id))
        .count();
    assert_eq!(overlap, 0, "offset skipped nothing");

    // Past the end: no rows, and the total still describes the set.
    let beyond = list_entities_inner(&pool, &mine, 3, 100_000).await.unwrap();
    assert!(beyond.rows.is_empty());
    assert_eq!(beyond.total, 0, "an empty page reports 0, not a guess");
}

#[tokio::test]
async fn a_tombstoned_entity_is_absent_unless_asked_for() {
    let pool = seeded().await;
    let live = list_entities_inner(&pool, &all(), 500, 0).await.unwrap();
    assert!(
        !live.rows.iter().any(|r| r.entity_id == "mock:PAY-198"),
        "a withdrawn entity is not part of the room"
    );

    let with_dead = EntityFilter {
        include_deleted: true,
        ..all()
    };
    let dead = list_entities_inner(&pool, &with_dead, 500, 0)
        .await
        .unwrap();
    assert!(
        dead.rows.iter().any(|r| r.entity_id == "mock:PAY-198"),
        "include_deleted must reach past the live-item view"
    );
    assert!(
        dead.total > live.total,
        "including the withdrawn changes the count too: {} vs {}",
        dead.total,
        live.total
    );
}

#[tokio::test]
async fn title_order_is_a_second_statement_not_string_interpolation() {
    let pool = seeded().await;
    let filter = EntityFilter {
        order: EntityOrder::TitleAsc,
        ..all()
    };
    let page = list_entities_inner(&pool, &filter, 500, 0).await.unwrap();

    let titles = page
        .rows
        .iter()
        .map(|r| r.title.clone())
        .collect::<Vec<_>>();
    let mut sorted = titles.clone();
    sorted.sort();
    assert_eq!(titles, sorted);

    // ...and it is a different order from the default, or the assertion above
    // would hold for a `match` that returned the same statement twice.
    let by_date = list_entities_inner(&pool, &all(), 500, 0).await.unwrap();
    assert_ne!(
        by_date
            .rows
            .iter()
            .map(|r| r.title.clone())
            .collect::<Vec<_>>(),
        titles,
        "title_asc and updated_desc produced the same ordering"
    );
}

/// `updated_within_days` is a window on the source's own timestamp.
///
/// Two rows of this test's own, one dated now and one dated a month ago, in a
/// source id nothing else uses. Deliberately **not** the fixture: its items
/// are dated 2026-08-22, so "a one-day window excludes the corpus" is a fact
/// about what today's date happens to be, and a test that starts failing on a
/// particular Saturday is worse than no test.
#[tokio::test]
async fn the_recency_window_is_bound_as_a_parameter() {
    let pool = seeded().await;
    let source = format!("clock-{}", unique());
    let fresh = format!("{source}:FRESH");
    let stale = format!("{source}:STALE");

    for (id, age_days) in [(&fresh, 0_i32), (&stale, 30)] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'x')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item
                 (entity_id, source_id, kind, title, body_text, item_updated_at, payload)
             values ($1, $2, 'ticket', 'x', '', now() - make_interval(days => $3), '{}'::jsonb)",
        )
        .bind(id)
        .bind(&source)
        .bind(age_days)
        .execute(&pool)
        .await
        .unwrap();
    }

    let within = |days: Option<u32>| EntityFilter {
        sources: vec![source.clone()],
        updated_within_days: days,
        ..all()
    };

    let ids = |page: knobas_app::commands::entity::EntityPage| {
        page.rows
            .into_iter()
            .map(|row| row.entity_id)
            .collect::<std::collections::BTreeSet<_>>()
    };

    // Both directions, so a predicate that is simply ignored fails: the narrow
    // window drops the stale row, the absent window keeps it.
    let narrow = ids(list_entities_inner(&pool, &within(Some(1)), 500, 0)
        .await
        .unwrap());
    assert!(
        narrow.contains(&fresh),
        "the fresh row is inside a one-day window"
    );
    assert!(!narrow.contains(&stale), "the stale row is not");

    let unfiltered = ids(list_entities_inner(&pool, &within(None), 500, 0)
        .await
        .unwrap());
    assert!(unfiltered.contains(&fresh));
    assert!(unfiltered.contains(&stale));

    // ...and a window wide enough to reach past it takes it back.
    let wide = ids(list_entities_inner(&pool, &within(Some(365)), 500, 0)
        .await
        .unwrap());
    assert!(wide.contains(&stale));
}

/// One source's room shows one source's work.
#[tokio::test]
async fn the_source_filter_selects_a_room() {
    let pool = seeded().await;
    let mine = EntityFilter {
        sources: vec!["mock".to_owned()],
        ..all()
    };
    let nobody = EntityFilter {
        sources: vec!["no-such-source".to_owned()],
        ..all()
    };
    assert!(
        list_entities_inner(&pool, &mine, 500, 0)
            .await
            .unwrap()
            .total
            > 0
    );
    assert_eq!(
        list_entities_inner(&pool, &nobody, 500, 0)
            .await
            .unwrap()
            .total,
        0
    );
}

// -- get_entity -------------------------------------------------------------

#[tokio::test]
async fn returns_the_row_its_source_and_the_raw_payload() {
    let pool = seeded().await;
    let d = get_entity_inner(&pool, "mock:PAY-231").await.unwrap();

    assert_eq!(d.row.entity_id, "mock:PAY-231");
    assert_eq!(d.row.kind, "ticket");
    assert_eq!(d.source.id, "mock");
    assert_eq!(
        d.source.display_name, "Tidewater (mock)",
        "the configured display name, not the id"
    );
    assert_eq!(d.source.adapter_kind, "mock");
    assert!(d.body_text.contains("SEPA"));
    assert_eq!(
        d.payload["key"], "PAY-231",
        "payload is the source record verbatim (§3a)"
    );
    assert!(d.links.is_empty(), "links are M2");
    assert!(d.deleted_at.is_none());
    assert!(
        d.kind_info.is_none(),
        "resolving kind_info needs the adapter registry -- task 21"
    );
}

/// P5's *Open in browser*: the URL is the adapter's, persisted by `0002`.
#[tokio::test]
async fn the_web_url_the_adapter_reported_survives_the_mirror() {
    let pool = seeded().await;
    let d = get_entity_inner(&pool, "mock:PAY-231").await.unwrap();
    assert_eq!(
        d.web_url.as_deref(),
        Some("https://tidewater.example/browse/PAY-231")
    );

    // ...and an item the adapter gave no page for has none, rather than a
    // fabricated one. The button is absent exactly there.
    let withdrawn = get_entity_inner(&pool, "mock:PAY-198").await.unwrap();
    assert_eq!(withdrawn.web_url, None);
}

#[tokio::test]
async fn an_unknown_id_is_not_found_not_internal() {
    let pool = seeded().await;
    let err = get_entity_inner(&pool, "mock:NOPE-1").await.unwrap_err();
    assert_eq!(err.code, knobas_app::IpcErrorCode::NotFound, "{err}");
}

/// A bad deep link reports itself as a bad address, not as a 500.
#[tokio::test]
async fn a_malformed_id_is_invalid() {
    let pool = seeded().await;
    for bad in ["no-colon-here", "", ":PAY-1", "mock:"] {
        let err = get_entity_inner(&pool, bad).await.unwrap_err();
        assert_eq!(
            err.code,
            knobas_app::IpcErrorCode::Invalid,
            "{bad:?} produced {err}"
        );
    }
}

/// §5a: links and notes point at entities that vanished upstream, so the
/// detail view has to be able to show what the user linked to.
#[tokio::test]
async fn a_tombstoned_entity_is_still_readable_and_says_so() {
    let pool = seeded().await;
    let d = get_entity_inner(&pool, "mock:PAY-198").await.unwrap();
    assert!(
        d.deleted_at.is_some(),
        "the banner has nothing to say without this"
    );
    assert_eq!(d.row.title, "Legacy payout reconciliation (withdrawn)");
}

#[tokio::test]
async fn activity_can_be_scoped_to_one_entity() {
    let pool = seeded().await;
    // The activity table is shared with every other test in this binary, so
    // this run's lines are found by a verb nobody else writes.
    let verb = format!("opened-{}", unique());
    let mine = knobas_core::entity::EntityRef::parse("mock:PAY-231").unwrap();
    let other = knobas_core::entity::EntityRef::parse("mock:PAY-228").unwrap();
    for entity in [&mine, &other] {
        knobas_core::activity::record(&pool, "user", &verb, Some(entity), serde_json::json!({}))
            .await
            .unwrap();
    }

    let scoped = recent_activity_inner(&pool, 200, Some(&mine))
        .await
        .unwrap();
    assert_eq!(
        scoped.iter().filter(|r| r.verb == verb).count(),
        1,
        "the filter dropped the other entity's line"
    );
    assert!(
        scoped
            .iter()
            .all(|r| r.entity_id.as_deref() == Some("mock:PAY-231")),
        "a scoped read returns only that entity's lines"
    );

    let global = recent_activity_inner(&pool, 500, None).await.unwrap();
    assert_eq!(
        global.iter().filter(|r| r.verb == verb).count(),
        2,
        "the unscoped read still sees both"
    );
}

/// `get_entity` hands the detail view the entity's own history without a
/// second round trip.
#[tokio::test]
async fn the_detail_carries_the_entitys_own_activity() {
    let pool = seeded().await;
    let verb = format!("noted-{}", unique());
    let entity = knobas_core::entity::EntityRef::parse("mock:PAY-231").unwrap();
    knobas_core::activity::record(&pool, "user", &verb, Some(&entity), serde_json::json!({}))
        .await
        .unwrap();

    let d = get_entity_inner(&pool, "mock:PAY-231").await.unwrap();
    assert!(d.activity.iter().any(|r| r.verb == verb));
    assert!(
        d.activity
            .iter()
            .all(|r| r.entity_id.as_deref() == Some("mock:PAY-231"))
    );
}

/// A source with no configuration row still names itself.
///
/// `run_once` syncs sources that were never configured (interfaces §1: tests,
/// ad-hoc imports), and deleting a source leaves its mirror rows behind. The
/// join to `knobas.source_config` is therefore a **left** join, and the
/// fallback is the source id — an inner join would make those entities
/// unopenable, and a `display_name` of the empty string would make them
/// nameless.
///
/// Seeded by hand rather than through an adapter, because that is exactly the
/// state being tested: rows in the mirror with nothing in `source_config`.
#[tokio::test]
async fn an_entity_whose_source_was_never_configured_is_still_readable() {
    let pool = seeded().await;
    let source = format!("ghost-{}", unique());
    let entity = format!("{source}:GH-1");

    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'A ghost')")
        .bind(&entity)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, $2, 'ticket', 'A ghost', '', '{}'::jsonb)",
    )
    .bind(&entity)
    .bind(&source)
    .execute(&pool)
    .await
    .unwrap();

    let d = get_entity_inner(&pool, &entity).await.unwrap();
    assert_eq!(d.source.id, source);
    assert_eq!(
        d.source.display_name, source,
        "an unconfigured source falls back to its id, not to an empty name"
    );
    assert_eq!(d.source.adapter_kind, source);
}
