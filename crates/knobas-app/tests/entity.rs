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
        knobas_app::demo::demo_load_inner(&pool).await.unwrap();
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
#[tokio::test]
async fn offset_walks_the_same_ordering_and_total_does_not_move() {
    let pool = seeded().await;
    let first = list_entities_inner(&pool, &all(), 3, 0).await.unwrap();
    let second = list_entities_inner(&pool, &all(), 3, 3).await.unwrap();

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
    let beyond = list_entities_inner(&pool, &all(), 3, 100_000)
        .await
        .unwrap();
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
#[tokio::test]
async fn the_recency_window_is_bound_as_a_parameter() {
    let pool = seeded().await;
    // The fixture's newest item is dated 2026-08-22; the tests run long after
    // it, so a one-day window necessarily excludes the corpus and a very wide
    // one necessarily includes it. Both directions, so a predicate that is
    // simply ignored fails.
    let narrow = EntityFilter {
        updated_within_days: Some(1),
        ..all()
    };
    let wide = EntityFilter {
        updated_within_days: Some(100_000),
        ..all()
    };
    assert_eq!(
        list_entities_inner(&pool, &narrow, 500, 0)
            .await
            .unwrap()
            .total,
        0
    );
    assert!(
        list_entities_inner(&pool, &wide, 500, 0)
            .await
            .unwrap()
            .total
            > 0
    );
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
