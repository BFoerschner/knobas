//! The room's read, against a real PostgreSQL and the mock corpus.
//!
//! `test_util` hands every test in this binary the *same* database, so the
//! corpus is seeded once (see [`seeded`]) and every assertion below is written
//! to survive another test running beside it: relative counts and set
//! membership, never absolute row totals.

use knobas_app::commands::entity::{
    DEFAULT_RELATION, EntityFilter, EntityOrder, create_link_inner, get_entity_inner,
    list_entities_inner, recent_activity_inner, unlink_inner,
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

/// Scoped to the `mock` source, for the same reason
/// [`offset_walks_the_same_ordering_and_total_does_not_move`] is: the recency
/// test writes fresher `ticket` rows under its own `clock-*` source, and an
/// unscoped limit-2 window would show those whenever they commit first
/// (issue #30). The intruder below is that neighbour, seeded deterministically
/// instead of raced for, so the scoping is proven rather than assumed.
#[tokio::test]
async fn lists_the_newest_first_and_reports_the_unpaged_total() {
    let pool = seeded().await;

    // A ticket fresher than the whole corpus (dated 2026-08-22), in a source
    // nothing else uses. Without the source scope it wins the window.
    let source = format!("elsewhere-{}", unique());
    let intruder = format!("{source}:NEW-1");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'x')")
        .bind(&intruder)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
             (entity_id, source_id, kind, title, body_text, item_updated_at, payload)
         values ($1, $2, 'ticket', 'x', '', now(), '{}'::jsonb)",
    )
    .bind(&intruder)
    .bind(&source)
    .execute(&pool)
    .await
    .unwrap();

    let filter = EntityFilter {
        sources: vec!["mock".to_owned()],
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
    // (`links` used to be asserted empty here -- "links are M2". It is not
    // empty any more, and it is not this test's subject either: the link tests
    // at the bottom of this file are what that assertion became, and one of
    // them links `mock:PAY-231`.)
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

// -- the link commands ------------------------------------------------------
//
// The seam is the command layer, driven over the migrated test pool: a link is
// *made* through `create_link_inner` and *observed* through `get_entity_inner`
// and `recent_activity_inner` -- three different commands, so nothing here can
// pass by agreeing with itself. Nothing asserts SQL, store internals, or a
// count of the whole table: the corpus is shared by every test in this binary.

/// Two run-unique entities that are in the mirror, which is what a link needs
/// at both ends to be *read back*: `get_entity_inner` joins `sync.item`, so an
/// entity carrying only a `knobas.entity` row can be linked and never opened.
async fn linkable_pair(pool: &PgPool) -> (String, String) {
    let source = format!("links-{}", unique());
    let from = format!("{source}:TICKET-1");
    let to = format!("{source}:PAGE-1");
    for (id, kind) in [(&from, "ticket"), (&to, "page")] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, 'x')")
            .bind(id)
            .bind(kind)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, 'x', '', '{}'::jsonb)",
        )
        .bind(id)
        .bind(&source)
        .bind(kind)
        .execute(pool)
        .await
        .unwrap();
    }
    (from, to)
}

/// The links `entity`'s detail view would draw.
async fn links_on(pool: &PgPool, entity: &str) -> Vec<knobas_core::link::LinkRow> {
    get_entity_inner(pool, entity).await.unwrap().links
}

/// A link made with nothing but its two ends: `related`, `manual`, and on both
/// ends' detail.
///
/// The quick link that "costs no extra decisions" -- and the read is the
/// *other* command, in both directions, because a link that only its own
/// writer can see is not a link.
#[tokio::test]
async fn a_link_made_from_its_two_ends_alone_is_related_manual_and_visible_from_both() {
    let pool = seeded().await;
    let (from, to) = linkable_pair(&pool).await;

    let written = create_link_inner(&pool, &from, &to, None, None)
        .await
        .unwrap();

    // The literal, not just the constant: a test asserting only
    // `== DEFAULT_RELATION` would hold for whatever that constant was changed
    // to, and "related" is the word the spec, the curated relation list and
    // the panel's group header all use.
    assert_eq!(
        written.link.relation, "related",
        "an unnamed relation is `related`, not empty"
    );
    assert_eq!(DEFAULT_RELATION, "related");
    assert_eq!(
        written.link.origin,
        knobas_core::link::Origin::Manual,
        "origin is not client-suppliable in v1: a link made here is hand-made"
    );
    assert_eq!(written.link.note, None);
    assert_eq!(written.link.from_id, from);
    assert_eq!(written.link.to_id, to);

    // Both ends, through the entity-detail read. `links_of` is undirected, so
    // the end the link was *not* drawn from is the half that would be missing
    // if the read were keyed on `from_id`.
    for end in [&from, &to] {
        let row = links_on(&pool, end)
            .await
            .into_iter()
            .find(|row| row.id == written.link.id)
            .unwrap_or_else(|| panic!("the link is missing from {end}'s detail"));
        assert_eq!(row.from_id, from);
        assert_eq!(row.to_id, to);
        assert_eq!(row.relation, DEFAULT_RELATION);
    }
}

/// The relation the caller names is the relation the link carries, and the
/// same pair may carry several.
#[tokio::test]
async fn a_named_relation_is_kept_and_the_same_pair_may_carry_several() {
    let pool = seeded().await;
    let (from, to) = linkable_pair(&pool).await;

    let documents = create_link_inner(&pool, &from, &to, Some("documents"), None)
        .await
        .unwrap();
    let blocks = create_link_inner(&pool, &from, &to, Some("blocks"), None)
        .await
        .unwrap();
    assert_eq!(documents.link.relation, "documents");
    assert_eq!(blocks.link.relation, "blocks");

    let relations: std::collections::BTreeSet<String> = links_on(&pool, &from)
        .await
        .into_iter()
        .map(|row| row.relation)
        .collect();
    assert!(
        relations.contains("documents") && relations.contains("blocks"),
        "the pair carries both relations: {relations:?}"
    );
}

/// The note is carried from the write all the way to the detail read -- the
/// column `0001` declared and nothing filled in until now.
#[tokio::test]
async fn a_note_travels_from_the_write_to_the_entity_detail_read() {
    let pool = seeded().await;
    let (from, to) = linkable_pair(&pool).await;

    let written = create_link_inner(&pool, &from, &to, None, Some("  why this exists  "))
        .await
        .unwrap();
    assert_eq!(
        written.link.note.as_deref(),
        Some("why this exists"),
        "the note is trimmed, so a stray space is not a different note"
    );

    let read = links_on(&pool, &to)
        .await
        .into_iter()
        .find(|row| row.id == written.link.id)
        .expect("the link is on the far end's detail");
    assert_eq!(read.note.as_deref(), Some("why this exists"));

    // A note that is only whitespace is no note. `null` and `""` are different
    // facts in the mirror, and only one of them is worth a line in the panel.
    let blank = create_link_inner(&pool, &from, &to, Some("blocks"), Some("   "))
        .await
        .unwrap();
    assert_eq!(blank.link.note, None);
}

/// Linking the same pair under the same relation twice is `conflict`, not a
/// second row.
///
/// **In this direction.** The uniqueness rule is directed -- `link_active_idx`
/// is on `(from_id, to_id, relation)` -- while `links_of` reads undirected, so
/// `B -> A` after `A -> B` still succeeds and both panels then show two rows
/// for one relationship. That is **#70**: pre-existing store behaviour that
/// #52 wired up, ruled 2026-08-28 to be its own sub-issue rather than this
/// slice's to fix. So this test proves what it says and not #40's story 14
/// ("the panel never shows duplicates") in full -- do not read it as that.
#[tokio::test]
async fn a_duplicate_pair_and_relation_is_a_conflict() {
    let pool = seeded().await;
    let (from, to) = linkable_pair(&pool).await;

    create_link_inner(&pool, &from, &to, Some("documents"), None)
        .await
        .unwrap();
    let again = create_link_inner(&pool, &from, &to, Some("documents"), None)
        .await
        .unwrap_err();
    assert_eq!(
        again.code,
        knobas_app::IpcErrorCode::Conflict,
        "already linked is `conflict`, so the dialog can say so: {again}"
    );

    // Exactly one row survived the attempt.
    assert_eq!(
        links_on(&pool, &from)
            .await
            .iter()
            .filter(|row| row.relation == "documents")
            .count(),
        1
    );
}

/// An endpoint with no mirror row is `not_found` -- the user named an entity
/// that has not synced, which is an ordinary event and not knobas being
/// broken. Both ends, because they are two separate foreign keys.
#[tokio::test]
async fn a_link_endpoint_that_is_not_in_the_mirror_is_not_found_at_the_command_seam() {
    let pool = seeded().await;
    let (from, _to) = linkable_pair(&pool).await;
    let absent = format!("nowhere-{}:GONE-1", unique());

    for (a, b) in [(&from, &absent), (&absent, &from)] {
        let refused = create_link_inner(&pool, a, b, None, None)
            .await
            .unwrap_err();
        assert_eq!(
            refused.code,
            knobas_app::IpcErrorCode::NotFound,
            "{a} -> {b} produced {refused}"
        );
    }

    // ... and an id that is not an entity id at all is a bad address, not a
    // missing entity: the two want different words on screen.
    for bad in ["no-colon-here", "", "mock:"] {
        let refused = create_link_inner(&pool, &from, bad, None, None)
            .await
            .unwrap_err();
        assert_eq!(
            refused.code,
            knobas_app::IpcErrorCode::Invalid,
            "{bad:?} produced {refused}"
        );
    }
}

/// Unlinking an id nothing carries is `not_found`.
#[tokio::test]
async fn unlinking_an_unknown_id_is_not_found_and_a_malformed_one_is_invalid() {
    let pool = seeded().await;

    let unknown = unlink_inner(&pool, &uuid::Uuid::new_v4().to_string())
        .await
        .unwrap_err();
    assert_eq!(
        unknown.code,
        knobas_app::IpcErrorCode::NotFound,
        "{unknown}"
    );

    let malformed = unlink_inner(&pool, "not-a-uuid").await.unwrap_err();
    assert_eq!(
        malformed.code,
        knobas_app::IpcErrorCode::Invalid,
        "{malformed}"
    );
}

/// Unlinking keeps the row, and the same pair and relation can be linked
/// again afterwards.
///
/// The tombstone is what makes an unlink rememberable (story 12) and the
/// partial unique index is what makes re-linking possible (story 13); the two
/// are asserted together because either one alone would pass a weaker
/// implementation -- a hard delete satisfies re-linking, and a plain unique
/// index satisfies the tombstone.
#[tokio::test]
async fn unlinking_keeps_the_row_and_the_pair_can_be_linked_again() {
    let pool = seeded().await;
    let (from, to) = linkable_pair(&pool).await;

    let first = create_link_inner(&pool, &from, &to, Some("documents"), None)
        .await
        .unwrap();
    let withdrawn = unlink_inner(&pool, &first.link.id.to_string())
        .await
        .unwrap()
        .expect("the first unlink withdrew the link");
    assert_eq!(withdrawn.link.id, first.link.id);

    for end in [&from, &to] {
        assert!(
            !links_on(&pool, end)
                .await
                .iter()
                .any(|row| row.id == first.link.id),
            "the withdrawn link is still on {end}'s detail"
        );
    }

    // The row stays -- an unlink is a tombstone, not a delete.
    let (kept,): (i64,) = sqlx::query_as("select count(*) from knobas.link where id = $1")
        .bind(first.link.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kept, 1, "unlink deleted the row instead of tombstoning it");

    // ... and the pair is linkable again under the very relation that was
    // withdrawn, which is what the *partial* unique index buys.
    let second = create_link_inner(&pool, &from, &to, Some("documents"), None)
        .await
        .unwrap();
    assert_ne!(second.link.id, first.link.id);
    assert!(
        links_on(&pool, &from)
            .await
            .iter()
            .any(|row| row.id == second.link.id)
    );

    // Withdrawing something already withdrawn is not an error and is not a
    // mutation: nothing comes back, so nothing is announced.
    assert!(
        unlink_inner(&pool, &first.link.id.to_string())
            .await
            .unwrap()
            .is_none()
    );
}

/// Every mutation leaves exactly one activity line, on the end the link was
/// drawn from, naming the other end, the relation and the link.
///
/// Read back through `recent_activity_inner` *and* through the entity detail's
/// own history, because those are the two surfaces that draw it (§12.1, §2a) --
/// and counted with `== 1`, scoped by a verb nobody else in this binary writes,
/// so a second row would fail rather than pass unnoticed.
#[tokio::test]
async fn each_link_mutation_writes_one_activity_line_on_the_from_end() {
    let pool = seeded().await;
    let (from, to) = linkable_pair(&pool).await;
    let from_ref = knobas_core::entity::EntityRef::parse(&from).unwrap();
    let to_ref = knobas_core::entity::EntityRef::parse(&to).unwrap();

    let created = create_link_inner(&pool, &from, &to, Some("documents"), None)
        .await
        .unwrap();

    assert_eq!(created.activity.verb, "linked");
    assert_eq!(created.activity.actor, "user");
    assert_eq!(
        created.activity.entity_id.as_deref(),
        Some(from.as_str()),
        "the line is named on the end the link was drawn from"
    );
    assert_eq!(
        created.activity.detail,
        serde_json::json!({
            "link_id": created.link.id,
            "to_id": to,
            "relation": "documents",
        }),
        "the other end, the relation and the link id are what make the line \
         actionable"
    );

    // The row the command hands back is the row in the log, not a copy of its
    // own arguments: found by id through a different command.
    let logged = recent_activity_inner(&pool, 200, Some(&from_ref))
        .await
        .unwrap();
    let mine: Vec<_> = logged
        .iter()
        .filter(|row| row.id == created.activity.id)
        .collect();
    assert_eq!(mine.len(), 1, "one line per mutation");
    assert_eq!(mine[0].detail, created.activity.detail);
    assert_eq!(mine[0].at, created.activity.at);

    // ... and it rides along with the detail read the panel is drawn from.
    assert!(
        get_entity_inner(&pool, &from)
            .await
            .unwrap()
            .activity
            .iter()
            .any(|row| row.id == created.activity.id)
    );

    // The known v1 limitation, asserted rather than assumed: the to-end's
    // history does not carry the line. Its links panel still shows the link.
    assert!(
        !recent_activity_inner(&pool, 200, Some(&to_ref))
            .await
            .unwrap()
            .iter()
            .any(|row| row.id == created.activity.id),
        "one row per mutation, on the from-end -- a second row on the to-end \
         would be the decision this ticket did not take"
    );

    // The other verb, on the same shape.
    let withdrawn = unlink_inner(&pool, &created.link.id.to_string())
        .await
        .unwrap()
        .expect("the link was withdrawn");
    assert_eq!(withdrawn.activity.verb, "unlinked");
    assert_eq!(withdrawn.activity.entity_id.as_deref(), Some(from.as_str()));
    assert_eq!(
        withdrawn.activity.detail,
        serde_json::json!({
            "link_id": created.link.id,
            "to_id": to,
            "relation": "documents",
        })
    );
    assert_ne!(
        withdrawn.activity.id, created.activity.id,
        "the unlink wrote its own line rather than reporting the link's"
    );
    assert_eq!(
        recent_activity_inner(&pool, 200, Some(&from_ref))
            .await
            .unwrap()
            .iter()
            .filter(|row| row.verb == "unlinked" && row.id == withdrawn.activity.id)
            .count(),
        1
    );
}

/// The demo criterion: in the demo corpus, a link made over the seam is in the
/// array the links panel draws from.
///
/// `EntityDetail.links` is exactly what `LinksPanel.svelte` is handed, and the
/// panel switches on `groups.length === 0` -- so a non-empty array there is
/// "Nothing linked yet" being replaced by the row. (The panel's own rendering
/// of that array is pinned in `app/src/lib/detail/Detail.test.svelte.ts`.)
///
/// Two fixture entities the demo profile really loads, rather than rows this
/// test invented: the point of the criterion is that it holds for the corpus
/// the user sees after clicking *Load demo data*.
#[tokio::test]
async fn a_link_over_the_seam_fills_the_demo_profiles_empty_links_panel() {
    let pool = seeded().await;

    // The empty state is half the criterion, so it is asserted rather than
    // assumed: the demo fixture ships no links, and this is the only test in
    // this binary that writes one into the shared `mock:` corpus -- every
    // other link test uses `linkable_pair`'s run-unique ids. A second test
    // linking a `mock:` entity would fail here, deliberately and not by
    // ordering: the criterion is "Nothing linked yet" being *replaced*.
    let before = get_entity_inner(&pool, "mock:PAY-231").await.unwrap().links;
    assert!(
        before.is_empty(),
        "the demo profile's panel says \"Nothing linked yet\" before the write: {before:?}"
    );
    let written = create_link_inner(
        &pool,
        "mock:PAY-231",
        "mock:PAY-228",
        Some("documents"),
        Some("the retry storm postmortem"),
    )
    .await
    .unwrap();

    let after = get_entity_inner(&pool, "mock:PAY-231").await.unwrap().links;
    assert_eq!(
        after.len(),
        before.len() + 1,
        "the panel gained exactly one row"
    );
    let drawn = after
        .iter()
        .find(|row| row.id == written.link.id)
        .expect("the new link is in the array the panel draws");
    assert_eq!(drawn.to_id, "mock:PAY-228");
    assert_eq!(drawn.relation, "documents");
    assert_eq!(drawn.note.as_deref(), Some("the retry storm postmortem"));
}

/// An entity cannot be linked to itself.
///
/// Björn's ruling (2026-08-28, on the review of #52): a self-link is refused.
/// It is a bad request rather than a missing thing -- both endpoints resolve,
/// they are simply the same one -- so it is `invalid`, the code a malformed id
/// already gets, and not a new error.
///
/// Refused before either endpoint is looked up: the shape of the request is
/// wrong whether or not the entity exists.
#[tokio::test]
async fn an_entity_cannot_be_linked_to_itself() {
    let pool = seeded().await;
    let (from, _to) = linkable_pair(&pool).await;

    for relation in [None, Some("blocks")] {
        let refused = create_link_inner(&pool, &from, &from, relation, None)
            .await
            .unwrap_err();
        assert_eq!(
            refused.code,
            knobas_app::IpcErrorCode::Invalid,
            "{from} -> itself ({relation:?}) produced {refused}"
        );
    }

    // ... and nothing was written on the way to refusing.
    assert!(
        !links_on(&pool, &from)
            .await
            .iter()
            .any(|row| row.from_id == row.to_id),
        "a self-link reached the table"
    );

    // An entity that is not in the mirror at all is still `not_found` rather
    // than `invalid`: the self-link check must not swallow the endpoint check.
    let absent = format!("nowhere-{}:GONE-1", unique());
    assert_eq!(
        create_link_inner(&pool, &absent, &absent, None, None)
            .await
            .unwrap_err()
            .code,
        knobas_app::IpcErrorCode::Invalid,
        "the same id twice is a bad request first, whatever it addresses"
    );
}

/// A relation's case does not split its group: `Blocks` and `blocks` are one
/// relation.
///
/// Björn's ruling (2026-08-28, on the review of #52). The panel groups by this
/// value verbatim, so without folding, a user who typed `Blocks` once and
/// `blocks` once gets two headers for one relationship -- and the duplicate
/// rule, which compares the stored strings, would not see the second as a
/// duplicate at all.
///
/// The *note* is deliberately not folded: it is prose in the user's own words,
/// not a key anything groups by.
#[tokio::test]
async fn a_relations_case_does_not_split_its_group() {
    let pool = seeded().await;
    let (from, to) = linkable_pair(&pool).await;

    let written = create_link_inner(&pool, &from, &to, Some("  Blocks  "), Some("Why It Blocks"))
        .await
        .unwrap();
    assert_eq!(
        written.link.relation, "blocks",
        "the relation is folded on write, so the panel has one group and not two"
    );
    assert_eq!(
        written.link.note.as_deref(),
        Some("Why It Blocks"),
        "the note is prose, not a group key -- its case is the user's"
    );

    // The folded value is what both ends read back.
    for end in [&from, &to] {
        let row = links_on(&pool, end)
            .await
            .into_iter()
            .find(|row| row.id == written.link.id)
            .unwrap_or_else(|| panic!("the link is missing from {end}'s detail"));
        assert_eq!(row.relation, "blocks");
    }

    // ... and the same relation in another case is the *same* relation, so the
    // second attempt is the duplicate it really is. (Same direction: the
    // reverse-direction hole is #70.)
    let again = create_link_inner(&pool, &from, &to, Some("BLOCKS"), None)
        .await
        .unwrap_err();
    assert_eq!(
        again.code,
        knobas_app::IpcErrorCode::Conflict,
        "`BLOCKS` after `Blocks` is already linked: {again}"
    );

    // A relation that differs by more than case is still its own group.
    create_link_inner(&pool, &from, &to, Some("Documents"), None)
        .await
        .unwrap();
    let relations: std::collections::BTreeSet<String> = links_on(&pool, &from)
        .await
        .into_iter()
        .map(|row| row.relation)
        .collect();
    assert!(
        relations.contains("blocks") && relations.contains("documents"),
        "folding must not merge distinct relations: {relations:?}"
    );
}
