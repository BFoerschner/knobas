//! The room's read, against a real PostgreSQL and the mock corpus.
//!
//! `test_util` hands every test in this binary the *same* database, so the
//! corpus is seeded once (see [`seeded`]) and every assertion below is written
//! to survive another test running beside it: relative counts and set
//! membership, never absolute row totals.

use knobas_app::commands::entity::{
    DEFAULT_RELATION, EntityFilter, EntityOrder, create_link_inner, get_entity_inner,
    list_entities_inner, list_projects_inner, recent_activity_inner, unlink_inner,
};
use knobas_source_mock::MockSource;
use sqlx::PgPool;

/// What a source declares about where it keeps a project (#277).
///
/// Built per source id, because the fixtures below name their sources at run
/// time and a declaration is keyed by the id that is also the entity
/// namespace -- which is what `knobas_app::sources::declared_paths` produces
/// from one adapter template and the configured rows.
///
/// Jira's `fields.project` for a ticket and TeamCity's top-level
/// `projectId`/`projectName` for a build configuration, which are the two
/// shapes these rooms are narrowed over.
fn declared_for(sources: &[&str]) -> knobas_core::payload::Declarations {
    use knobas_core::payload::{Declarations, KindPaths, PayloadPath};
    sources.iter().fold(Declarations::empty(), |declared, id| {
        declared.with(
            (*id).to_owned(),
            vec![
                KindPaths {
                    kind: "ticket".to_owned(),
                    project_key: vec![PayloadPath::of(["fields", "project", "key"])],
                    project_name: vec![PayloadPath::of(["fields", "project", "name"])],
                    ..KindPaths::default()
                },
                KindPaths {
                    kind: "build_config".to_owned(),
                    project_key: vec![PayloadPath::of(["projectId"])],
                    project_name: vec![PayloadPath::of(["projectName"])],
                    ..KindPaths::default()
                },
            ],
        )
    })
}

/// No source declares anything, which is all a list that does not narrow by
/// project needs -- and is the honest fixture for one: these rooms are scoped
/// by source, kind, context and recency, none of which is a payload read.
fn no_paths() -> knobas_core::payload::Declarations {
    knobas_core::payload::Declarations::empty()
}

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
        context: None,
        project: None,
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
    let page = list_entities_inner(&pool, &filter, 2, 0, &no_paths())
        .await
        .unwrap();

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
    let page = list_entities_inner(&pool, &all(), 5, 0, &no_paths())
        .await
        .unwrap();

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
    let first = list_entities_inner(&pool, &mine, 3, 0, &no_paths())
        .await
        .unwrap();
    let second = list_entities_inner(&pool, &mine, 3, 3, &no_paths())
        .await
        .unwrap();

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
    let beyond = list_entities_inner(&pool, &mine, 3, 100_000, &no_paths())
        .await
        .unwrap();
    assert!(beyond.rows.is_empty());
    assert_eq!(beyond.total, 0, "an empty page reports 0, not a guess");
}

#[tokio::test]
async fn a_tombstoned_entity_is_absent_unless_asked_for() {
    let pool = seeded().await;
    let live = list_entities_inner(&pool, &all(), 500, 0, &no_paths())
        .await
        .unwrap();
    assert!(
        !live.rows.iter().any(|r| r.entity_id == "mock:PAY-198"),
        "a withdrawn entity is not part of the room"
    );

    let with_dead = EntityFilter {
        include_deleted: true,
        ..all()
    };
    let dead = list_entities_inner(&pool, &with_dead, 500, 0, &no_paths())
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

/// Scoped to the `mock` source, for the same class of reason
/// [`lists_the_newest_first_and_reports_the_unpaged_total`] is, and for one
/// worth naming because it is not about freshness.
///
/// The oracle used to be Rust's `sort()`, which is **byte order**, and the
/// database's is its own collation -- and the two disagree the moment a title
/// starts with a lowercase letter. Every title in the mock corpus happened to
/// start with a capital, so the disagreement never showed; the first test in
/// this binary to write a lowercase title made it fail here, in a test that has
/// nothing to do with that test's subject (#442, whose monitors really are
/// called `gitea` and `canary`), and scoping this read to `mock` was the fix.
///
/// **#537 took that scoping's protection away**, exactly as the paragraph above
/// predicted it could: the demo corpus gained the fixture's repositories and
/// branches, and `payout-service`, `main` and `feature/PAY-231-sepa-retry` are
/// mock titles that start with a lowercase letter. Byte order puts all six
/// after `Standup protocols`; the database interleaves them, and it also sorts
/// `Ledger_Deploy_Staging #412` before `ledger-api`, which no case-folded byte
/// sort does either -- punctuation is weak in its collation and strong in
/// Rust's. So the oracle moved rather than the corpus: the titles that came
/// back are handed to a **hand-written statement of this test's own** to sort,
/// and the two orders must agree.
///
/// That keeps what this test is for. The subject is that `TitleAsc` selects a
/// *second SQL statement* rather than interpolating a column name into one, and
/// an `order by` the test wrote itself is an independent answer to that: an
/// implementation that ignored the order, or ordered by anything else, still
/// fails. What it stops being able to see is a collation change under the whole
/// database -- which would move both sides together, and is not this test's
/// subject or this milestone's risk.
#[tokio::test]
async fn title_order_is_a_second_statement_not_string_interpolation() {
    let pool = seeded().await;
    let filter = EntityFilter {
        order: EntityOrder::TitleAsc,
        sources: vec!["mock".to_owned()],
        ..all()
    };
    let page = list_entities_inner(&pool, &filter, 500, 0, &no_paths())
        .await
        .unwrap();

    let titles = page
        .rows
        .iter()
        .map(|r| r.title.clone())
        .collect::<Vec<_>>();
    let (sorted,): (Vec<String>,) =
        sqlx::query_as("select array(select t from unnest($1::text[]) as t order by t)")
            .bind(&titles)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(titles, sorted);

    // ...and it is a different order from the default, or the assertion above
    // would hold for a `match` that returned the same statement twice. **The
    // same scope**, or the two lists would differ because they cover different
    // rows and this would pass however `TitleAsc` was implemented.
    let by_date = list_entities_inner(
        &pool,
        &EntityFilter {
            sources: vec!["mock".to_owned()],
            ..all()
        },
        500,
        0,
        &no_paths(),
    )
    .await
    .unwrap();
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
    let narrow = ids(
        list_entities_inner(&pool, &within(Some(1)), 500, 0, &no_paths())
            .await
            .unwrap(),
    );
    assert!(
        narrow.contains(&fresh),
        "the fresh row is inside a one-day window"
    );
    assert!(!narrow.contains(&stale), "the stale row is not");

    let unfiltered = ids(
        list_entities_inner(&pool, &within(None), 500, 0, &no_paths())
            .await
            .unwrap(),
    );
    assert!(unfiltered.contains(&fresh));
    assert!(unfiltered.contains(&stale));

    // ...and a window wide enough to reach past it takes it back.
    let wide = ids(
        list_entities_inner(&pool, &within(Some(365)), 500, 0, &no_paths())
            .await
            .unwrap(),
    );
    assert!(wide.contains(&stale));
}

/// A project room's list read shows one project's work, within its source.
///
/// The room hands its filter to every tile, so this is the same narrowing the
/// mini board does and the same one the room's own kinds-and-count read makes
/// -- there is no per-tile special case, which is the whole of why the
/// dimension is on the filter (#208).
///
/// Rows of this test's own in a source id nothing else uses, for the reason
/// [`the_recency_window_is_bound_as_a_parameter`] seeds its own: the corpus is
/// shared with every other test in this binary.
#[tokio::test]
async fn a_project_narrows_the_room_within_its_sources() {
    let pool = seeded().await;
    let source = format!("proj-{}", unique());
    let elsewhere = format!("{source}-eu");
    let declarations = declared_for(&[source.as_str(), elsewhere.as_str()]);

    // (source, key, payload) -- one project, another project in the same
    // source, the same project key in a *different* source, and a record whose
    // project is unreadable.
    let seed: [(&str, &str, serde_json::Value); 4] = [
        (
            source.as_str(),
            "PAY-1",
            serde_json::json!({ "fields": { "project": { "key": "PAY", "name": "Payout" } } }),
        ),
        (
            source.as_str(),
            "INT-1",
            serde_json::json!({ "fields": { "project": { "key": "INT" } } }),
        ),
        (
            elsewhere.as_str(),
            "PAY-9",
            serde_json::json!({ "fields": { "project": { "key": "PAY" } } }),
        ),
        (
            source.as_str(),
            "NOP-1",
            serde_json::json!({ "fields": { "project": { "key": { "id": 3 } } } }),
        ),
    ];
    for (source_id, key, payload) in &seed {
        let id = format!("{source_id}:{key}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'x')")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, 'ticket', 'x', '', $3)",
        )
        .bind(&id)
        .bind(source_id)
        .bind(payload)
        .execute(&pool)
        .await
        .unwrap();
    }

    let room = |sources: Vec<String>, project: Option<&str>| EntityFilter {
        sources,
        project: project.map(str::to_owned),
        ..all()
    };
    let ids = |page: knobas_app::commands::entity::EntityPage| {
        page.rows
            .into_iter()
            .map(|row| row.entity_id)
            .collect::<std::collections::BTreeSet<_>>()
    };

    // Over both orderings, the way the `include_deleted` test below runs: the
    // two live statements are two constants, and a predicate honoured by one
    // of them would be a room that changes meaning when the caller re-sorts.
    for order in [EntityOrder::UpdatedDesc, EntityOrder::TitleAsc] {
        let project_room = ids(list_entities_inner(
            &pool,
            &EntityFilter {
                order,
                ..room(vec![source.clone()], Some("PAY"))
            },
            500,
            0,
            &declarations,
        )
        .await
        .unwrap());
        assert_eq!(
            project_room,
            std::collections::BTreeSet::from([format!("{source}:PAY-1")]),
            "one project's work, within its own source"
        );
    }

    // The dimension narrows within `sources`, so unscoped by source it reaches
    // both `PAY` projects -- which is why a project room names both halves.
    let both =
        ids(
            list_entities_inner(&pool, &room(Vec::new(), Some("PAY")), 500, 0, &declarations)
                .await
                .unwrap(),
        );
    assert!(both.contains(&format!("{source}:PAY-1")));
    assert!(both.contains(&format!("{elsewhere}:PAY-9")));

    // Absence, never a wrong room: the unreadable record is in no project
    // room, and is still in its source's.
    let source_room = ids(list_entities_inner(
        &pool,
        &room(vec![source.clone()], None),
        500,
        0,
        &declarations,
    )
    .await
    .unwrap());
    assert!(source_room.contains(&format!("{source}:NOP-1")));
    for project in ["PAY", "INT", "NOP"] {
        let narrowed = ids(list_entities_inner(
            &pool,
            &room(vec![source.clone()], Some(project)),
            500,
            0,
            &declarations,
        )
        .await
        .unwrap());
        assert!(
            !narrowed.contains(&format!("{source}:NOP-1")),
            "a record with no readable project is in no project room, {project} included"
        );
    }
}

/// A project room built from a configuration-only project shows that
/// configuration (#232).
///
/// The room's predicate and the census are one macro, so this is the room
/// half of what `knobas-core/tests/projects.rs` pins for the census: a
/// TeamCity project whose configurations have no synced build has a room,
/// and the room holds the configuration. The `build_config` record spells its
/// project at the top level -- the shape the adapter stores verbatim -- and
/// the ticket beside it is the same source's other project, so the room is
/// narrowed rather than merely non-empty. Over both orderings for the reason
/// [`a_project_narrows_the_room_within_its_sources`] is.
#[tokio::test]
async fn a_project_room_shows_a_configuration_only_project() {
    let pool = seeded().await;
    let source = format!("projcfg-{}", unique());
    let declarations = declared_for(&[source.as_str()]);
    let configuration = format!("{source}:buildType:Payout_Build");
    let other = format!("{source}:INT-1");

    let seed: [(&str, &str, serde_json::Value); 2] = [
        (
            configuration.as_str(),
            "build_config",
            serde_json::json!({
                "id": "Payout_Build",
                "name": "Build",
                "projectId": "Payout",
                "projectName": "Payout pipeline",
            }),
        ),
        (
            other.as_str(),
            "ticket",
            serde_json::json!({ "fields": { "project": { "key": "INT" } } }),
        ),
    ];
    for (id, kind, payload) in &seed {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, 'x')")
            .bind(id)
            .bind(kind)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, 'x', '', $4)",
        )
        .bind(id)
        .bind(&source)
        .bind(kind)
        .bind(payload)
        .execute(&pool)
        .await
        .unwrap();
    }

    for order in [EntityOrder::UpdatedDesc, EntityOrder::TitleAsc] {
        let page = list_entities_inner(
            &pool,
            &EntityFilter {
                sources: vec![source.clone()],
                project: Some("Payout".to_owned()),
                order,
                ..all()
            },
            500,
            0,
            &declarations,
        )
        .await
        .unwrap();
        let ids: std::collections::BTreeSet<String> =
            page.rows.into_iter().map(|row| row.entity_id).collect();
        assert_eq!(
            ids,
            std::collections::BTreeSet::from([configuration.clone()]),
            "the configuration is the project's work, and the other project's ticket is not"
        );
    }
}

/// **A space is Confluence's project** (ADR-0010), and the census can tell one
/// from a Jira project standing in the same corpus.
///
/// Both sources in one test, which is the whole point: each record's project
/// is resolved through the declaration **its own adapter** makes (#277), so a
/// read that spelled either source's path literally -- `space.key` for
/// everything, or `fields.project.key` for everything -- would report one of
/// these two projects and silently lose the other. Neither declaration is
/// written here: they come out of `declared_paths` over the real registry, the
/// way the running binary's `list_projects` gets them, so dropping `space.key`
/// from the Confluence descriptor fails *this* test and not only the
/// descriptor's own.
///
/// The **room** half is asserted beside the census because the two are one
/// macro (`knobas_core::project_key_read!`): a space room holds that space's
/// pages, and neither the wiki's other space nor the Jira project's ticket.
/// The room hands its filter to every tile, so narrowing the filter is
/// narrowing every tile -- there is no per-tile special case (#208).
#[tokio::test]
async fn a_confluence_space_is_a_project_room_and_a_jira_project_is_another() {
    let pool = seeded().await;
    let token = unique();
    // `conf-` sorts before `jira-`, which is the order the census promises and
    // the order the switcher offers the rooms in.
    let wiki = format!("conf-{token}");
    let tracker = format!("jira-{token}");

    // Configured rows, because `declared_paths` resolves a declaration per
    // *instance* off `knobas.source_config.kind` -- the adapter kind. Without
    // these two rows neither source declares anything and every project read
    // misses, which is this seam's stated failure direction.
    for (id, adapter_kind) in [
        (&wiki, knobas_source_confluence::ADAPTER_KIND),
        (&tracker, "jira"),
    ] {
        sqlx::query(
            "insert into knobas.source_config
                 (id, kind, display_name, base_url, auth_kind)
             values ($1, $2, $1, 'http://localhost', 'pat')",
        )
        .bind(id)
        .bind(adapter_kind)
        .execute(&pool)
        .await
        .unwrap();
    }

    // The shapes the two adapters store verbatim: a Confluence page names its
    // space in the expanded `space` object (`knobas_source_confluence::map`),
    // a Jira issue names its project under `fields`.
    let seed: [(&str, &str, &str, serde_json::Value); 4] = [
        (
            wiki.as_str(),
            knobas_source_confluence::KIND_PAGE,
            "98307",
            serde_json::json!({
                "id": "98307",
                "space": { "key": "ENG", "name": "Engineering", "type": "global" },
            }),
        ),
        (
            wiki.as_str(),
            knobas_source_confluence::KIND_PAGE,
            "98404",
            serde_json::json!({
                "id": "98404",
                "space": { "key": "OPS", "name": "Operations", "type": "global" },
            }),
        ),
        // A page filed in no space knobas can read is in no space room, and is
        // still in the wiki's own room: absence, never a wrong room
        // (ADR-0007 requirement 3).
        (
            wiki.as_str(),
            knobas_source_confluence::KIND_PAGE,
            "98500",
            serde_json::json!({ "id": "98500", "space": { "name": "Engineering" } }),
        ),
        (
            tracker.as_str(),
            "ticket",
            "PAY-231",
            serde_json::json!({ "fields": { "project": { "key": "PAY", "name": "Payout" } } }),
        ),
    ];
    for (source_id, kind, key, payload) in &seed {
        let id = format!("{source_id}:{key}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(&id)
            .bind(kind)
            .bind(*key)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, $4, '', $5)",
        )
        .bind(&id)
        .bind(source_id)
        .bind(kind)
        .bind(*key)
        .bind(payload)
        .execute(&pool)
        .await
        .unwrap();
    }

    // What the running binary resolves: every configured source's declaration,
    // off the compiled-in registry. Wanted here for the *room* half below --
    // the census reaches it through the command seam, which resolves its own.
    let declarations = knobas_app::sources::paths::declared_paths(
        &pool,
        &knobas_app::sources::Registry::builtin(),
    )
    .await
    .expect("what the configured sources declare");

    // -- the census, which is what the switcher's rooms are built from -------
    //
    // Through `list_projects_inner`, which *is* `list_projects` with its pool
    // handed in: a test that called `project::list` directly would be
    // re-typing the command's body, and would go on passing after the command
    // stopped resolving declarations at all.
    let census = list_projects_inner(&pool)
        .await
        .expect("the census `list_projects` answers with");
    let mine: Vec<(String, String, Option<String>)> = census
        .into_iter()
        .filter(|project| project.source_id == wiki || project.source_id == tracker)
        .map(|project| (project.source_id, project.key, project.name))
        .collect();
    assert_eq!(
        mine,
        vec![
            (
                wiki.clone(),
                "ENG".to_owned(),
                Some("Engineering".to_owned())
            ),
            (
                wiki.clone(),
                "OPS".to_owned(),
                Some("Operations".to_owned())
            ),
            (tracker.clone(), "PAY".to_owned(), Some("Payout".to_owned())),
        ],
        "two spaces under the wiki and one project under the tracker, each by \
         its source's own spelling"
    );

    // -- the room, narrowed by the same read --------------------------------
    let ids = |page: knobas_app::commands::entity::EntityPage| {
        page.rows
            .into_iter()
            .map(|row| row.entity_id)
            .collect::<std::collections::BTreeSet<_>>()
    };
    let room = |sources: Vec<String>, project: Option<&str>, kinds: Vec<String>| EntityFilter {
        sources,
        project: project.map(str::to_owned),
        kinds,
        ..all()
    };

    // Unscoped by kind, and scoped to the *Docs* tile's kind: the room hands
    // one filter to every tile, so both answers are the space's pages.
    for kinds in [
        Vec::new(),
        vec![knobas_source_confluence::KIND_PAGE.to_owned()],
    ] {
        let space_room = ids(list_entities_inner(
            &pool,
            &room(vec![wiki.clone()], Some("ENG"), kinds.clone()),
            500,
            0,
            &declarations,
        )
        .await
        .unwrap());
        assert_eq!(
            space_room,
            std::collections::BTreeSet::from([format!("{wiki}:98307")]),
            "one space's pages, whatever kinds the tile asked for ({kinds:?})"
        );
    }

    // The Jira project's room is the tracker's, and the space room is not it.
    let project_room = ids(list_entities_inner(
        &pool,
        &room(vec![tracker.clone()], Some("PAY"), Vec::new()),
        500,
        0,
        &declarations,
    )
    .await
    .unwrap());
    assert_eq!(
        project_room,
        std::collections::BTreeSet::from([format!("{tracker}:PAY-231")]),
        "a space key and a project key are two rooms in two sources"
    );

    // The spaceless page is in the wiki's room and in neither space's.
    let wiki_room = ids(list_entities_inner(
        &pool,
        &room(vec![wiki.clone()], None, Vec::new()),
        500,
        0,
        &declarations,
    )
    .await
    .unwrap());
    assert!(wiki_room.contains(&format!("{wiki}:98500")));
    for space in ["ENG", "OPS", "NOPE"] {
        let narrowed = ids(list_entities_inner(
            &pool,
            &room(vec![wiki.clone()], Some(space), Vec::new()),
            500,
            0,
            &declarations,
        )
        .await
        .unwrap());
        assert!(
            !narrowed.contains(&format!("{wiki}:98500")),
            "a page filed in no readable space is in no space room, {space} included"
        );
    }
}

/// A project room reaching past the tombstone filter is still that project's.
///
/// The `include_deleted` statements are the detail's way in (§5a) and narrow
/// by the same dimensions the live ones do; a filter honoured by two of four
/// statements is a room that changes meaning when a caller asks to see
/// withdrawn work.
#[tokio::test]
async fn a_project_narrows_the_include_deleted_statements_too() {
    let pool = seeded().await;
    let source = format!("projdel-{}", unique());
    let declarations = declared_for(&[source.as_str()]);
    let live = format!("{source}:PAY-1");
    let gone = format!("{source}:PAY-2");
    let other = format!("{source}:INT-1");

    for (id, key) in [(&live, "PAY"), (&gone, "PAY"), (&other, "INT")] {
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'x')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, 'ticket', 'x', '', $3)",
        )
        .bind(id)
        .bind(&source)
        .bind(serde_json::json!({ "fields": { "project": { "key": key } } }))
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&gone)
        .execute(&pool)
        .await
        .unwrap();

    for order in [EntityOrder::UpdatedDesc, EntityOrder::TitleAsc] {
        let page = list_entities_inner(
            &pool,
            &EntityFilter {
                sources: vec![source.clone()],
                project: Some("PAY".to_owned()),
                include_deleted: true,
                order,
                ..all()
            },
            500,
            0,
            &declarations,
        )
        .await
        .unwrap();
        let ids: std::collections::BTreeSet<String> =
            page.rows.into_iter().map(|row| row.entity_id).collect();
        assert_eq!(
            ids,
            std::collections::BTreeSet::from([live.clone(), gone.clone()]),
            "the withdrawn ticket is this project's too, and the other project's is not"
        );
    }
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
        list_entities_inner(&pool, &mine, 500, 0, &no_paths())
            .await
            .unwrap()
            .total
            > 0
    );
    assert_eq!(
        list_entities_inner(&pool, &nobody, 500, 0, &no_paths())
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
    // #204's miss direction: an entity from an enabled source carries no
    // marker, or every ordinary detail would open with a false banner.
    assert!(d.source.enabled);
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
    // §3a end to end: the mock declares `ticket`, and the detail carries the
    // adapter's own words rather than the frontend humanising the id.
    let info = d
        .kind_info
        .as_ref()
        .expect("the mock declares `ticket`, so the registry resolves it");
    assert_eq!(info.id, "ticket");
    assert!(
        !info.label.is_empty() && !info.plural.is_empty() && !info.monogram.is_empty(),
        "a resolved KindInfo with an empty field is worse than none: {info:?}"
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
    // ...and the sentence names the id and the store it looked in, in the
    // glossary's own word: `CONTEXT.md`'s **Mirror** entry lists *index* under
    // `_Avoid_` (#518). This is the sentence the detail panel prints under its
    // heading, so the wording is a surface and not an implementation detail --
    // and until #518 nothing asserted on it. The webview suites that draw it
    // each hard-code their own copy of the literal (`Detail.test.svelte.ts`,
    // `LinkDialog.test.svelte.ts`, `links.test.ts`, `Room.test.svelte.ts`),
    // and so does `fake-tauri.ts`; none of them reads this statement. Pinning
    // it here is what makes those copies copies of something.
    assert_eq!(
        err.message, "mock:NOPE-1 is not in the mirror",
        "the refusal names the entity and the mirror"
    );
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
    // Half of #204's three-state guarantee: withdrawn upstream is not "source
    // turned off", and a tombstoned entity of an enabled source must not
    // trip the second banner too.
    assert!(
        d.source.enabled,
        "withdrawn upstream must not claim the source is off"
    );
}

/// Issue #204's second surface: a disabled source's entity still opens by
/// direct address (the `DETAIL` statement reaches past `sync.live_item` on
/// purpose, per §5a), but where a tombstoned one carries `deleted_at` for the
/// banner, "source turned off" carried nothing at all. `source.enabled` is
/// that marker -- derived in the same statement as the row, never stored,
/// which the re-read after the toggle proves: no re-sync, no write, only the
/// answer changes.
#[tokio::test]
async fn a_disabled_sources_entity_opens_with_the_marker_until_reenabled() {
    let pool = seeded().await;
    let source = format!("dark-{}", unique());
    let entity = format!("{source}:DK-1");

    sqlx::query("insert into knobas.entity (id, kind, title) values ($1, 'ticket', 'In the dark')")
        .bind(&entity)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1, $2, 'ticket', 'In the dark', '', '{}'::jsonb)",
    )
    .bind(&entity)
    .bind(&source)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, enabled)
         values ($1, 'mock', 'Dark corner', '', 'none', false)",
    )
    .bind(&source)
    .execute(&pool)
    .await
    .unwrap();

    let d = get_entity_inner(&pool, &entity).await.unwrap();
    assert!(
        !d.source.enabled,
        "the banner has nothing to say without this"
    );
    assert!(
        d.deleted_at.is_none(),
        "turned off is not withdrawn: collapsing the two recreates #204"
    );

    sqlx::query("update knobas.source_config set enabled = true where id = $1")
        .bind(&source)
        .execute(&pool)
        .await
        .unwrap();
    let back = get_entity_inner(&pool, &entity).await.unwrap();
    assert!(
        back.source.enabled,
        "re-enabling alone must clear the marker -- the proof nothing was stored"
    );
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
    // ...and there is no adapter to ask what its kind is called. §3a's whole
    // point is that this still renders: the frontend's humaniser takes it from
    // here, so `None` is the designed path rather than a degradation. This is
    // also the case `run_once` produces in ordinary use, so a resolution that
    // unwrapped `adapter_kind` would panic on a perfectly normal corpus.
    assert!(
        d.kind_info.is_none(),
        "no configuration row means no adapter to ask"
    );
    assert_eq!(d.source.adapter_kind, source);
    // The coalesce direction #204 inherits from migration 0012: no
    // configuration row is not a decision the user made, so it reads enabled
    // rather than off.
    assert!(d.source.enabled);
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

/// The link entries `entity`'s detail view would draw -- each one the link
/// record plus the end the viewer is *not* on.
async fn links_on(pool: &PgPool, entity: &str) -> Vec<knobas_core::link::LinkEntry> {
    get_entity_inner(pool, entity).await.unwrap().links
}

/// The other end arrives hydrated, and a target the source withdrew still
/// resolves.
///
/// Story 9 and story 10 of #40 in one read, because they are one query: the
/// panel draws a kind and a title rather than a raw id, and it can only do
/// that if the read reaches **past** `sync.live_item` -- the documented
/// exception, since a link may point at an entity that was tombstoned upstream
/// and must not silently dangle.
///
/// Both directions, because the hydrated end is chosen relative to the entity
/// being read: a join keyed on `to_id` alone would pass the first half of this
/// test and hand the withdrawn ticket's own panel a row describing itself.
#[tokio::test]
async fn a_links_other_end_arrives_hydrated_and_a_withdrawn_target_still_resolves() {
    let pool = seeded().await;
    let (from, _unused) = linkable_pair(&pool).await;
    // The fixture's withdrawn ticket: `knobas.entity.deleted_at` is set and its
    // mirror row is kept (`a_tombstoned_entity_is_still_readable_and_says_so`).
    let withdrawn = "mock:PAY-198";

    let written = create_link_inner(&pool, &from, withdrawn, Some("documents"), None)
        .await
        .unwrap();

    let entry = links_on(&pool, &from)
        .await
        .into_iter()
        .find(|entry| entry.link.id == written.link.id)
        .expect("the link is on the viewed entity's detail");
    assert_eq!(entry.other.entity_id, withdrawn);
    assert_eq!(
        entry.other.kind, "ticket",
        "the panel draws the other end's kind, not a raw id"
    );
    assert_eq!(
        entry.other.title, "Legacy payout reconciliation (withdrawn)",
        "the other end's own title, so the reader recognises what they linked"
    );
    assert!(
        entry.other.deleted_at.is_some(),
        "a withdrawn target still resolves, and carries what marks it withdrawn"
    );

    // ... and read from the withdrawn end, the hydrated end is the live one.
    let back = links_on(&pool, withdrawn)
        .await
        .into_iter()
        .find(|entry| entry.link.id == written.link.id)
        .expect("the link is on the far end's detail too");
    assert_eq!(back.other.entity_id, from);
    assert!(
        back.other.deleted_at.is_none(),
        "the live end is not marked withdrawn"
    );
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

    // Both ends, through the entity-detail read. `entries_of` is undirected, so
    // the end the link was *not* drawn from is the half that would be missing
    // if the read were keyed on `from_id`.
    for end in [&from, &to] {
        let entry = links_on(&pool, end)
            .await
            .into_iter()
            .find(|entry| entry.link.id == written.link.id)
            .unwrap_or_else(|| panic!("the link is missing from {end}'s detail"));
        assert_eq!(entry.link.from_id, from);
        assert_eq!(entry.link.to_id, to);
        assert_eq!(entry.link.relation, DEFAULT_RELATION);
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
        .map(|entry| entry.link.relation)
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
        .find(|entry| entry.link.id == written.link.id)
        .expect("the link is on the far end's detail");
    assert_eq!(read.link.note.as_deref(), Some("why this exists"));

    // A note that is only whitespace is no note. `null` and `""` are different
    // facts in the mirror, and only one of them is worth a line in the panel.
    let blank = create_link_inner(&pool, &from, &to, Some("blocks"), Some("   "))
        .await
        .unwrap();
    assert_eq!(blank.link.note, None);
}

/// Linking the same pair under the same relation twice is `conflict`, not a
/// second row -- **in either direction** (#70, migration `0011`).
///
/// The reverse used to succeed: the rule was directed (`link_active_idx` on
/// `(from_id, to_id, relation)`) while `entries_of` reads undirected, so `B -> A`
/// after `A -> B` landed and both panels then drew two rows for one
/// relationship. This is #40's story 14 in full -- "a duplicate link attempt
/// (same pair, same relation) reported as 'already linked', so that the panel
/// never shows duplicates" -- and the reversed half is the assertion that was
/// missing.
#[tokio::test]
async fn a_duplicate_pair_and_relation_is_a_conflict() {
    let pool = seeded().await;
    let (from, to) = linkable_pair(&pool).await;

    create_link_inner(&pool, &from, &to, Some("documents"), None)
        .await
        .unwrap();

    for (a, b) in [(&from, &to), (&to, &from)] {
        let again = create_link_inner(&pool, a, b, Some("documents"), None)
            .await
            .unwrap_err();
        assert_eq!(
            again.code,
            knobas_app::IpcErrorCode::Conflict,
            "already linked is `conflict`, so the dialog can say so: {again}"
        );
    }

    // Exactly one row survived both attempts, and it is on both panels.
    for end in [&from, &to] {
        assert_eq!(
            links_on(&pool, end)
                .await
                .iter()
                .filter(|entry| entry.link.relation == "documents")
                .count(),
            1,
            "the panel for {end} must show one row for one link"
        );
    }
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
                .any(|entry| entry.link.id == first.link.id),
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
            .any(|entry| entry.link.id == second.link.id)
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
    // this binary that links `mock:PAY-231` -- every other link test uses
    // `linkable_pair`'s run-unique ids for at least one end. A second test
    // linking this entity would fail here, deliberately and not by ordering:
    // the criterion is "Nothing linked yet" being *replaced*.
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
        .find(|entry| entry.link.id == written.link.id)
        .expect("the new link is in the array the panel draws");
    assert_eq!(drawn.link.to_id, "mock:PAY-228");
    assert_eq!(drawn.link.relation, "documents");
    assert_eq!(
        drawn.link.note.as_deref(),
        Some("the retry storm postmortem")
    );
    // Hydrated, which is what the panel draws instead of the id: `PAY-228` is
    // a real fixture ticket and the panel names it.
    assert_eq!(drawn.other.entity_id, "mock:PAY-228");
    assert_eq!(drawn.other.kind, "ticket");
    assert!(
        !drawn.other.title.is_empty(),
        "the row would show a raw id with nothing to recognise it by"
    );
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
            .any(|entry| entry.link.from_id == entry.link.to_id),
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
        let entry = links_on(&pool, end)
            .await
            .into_iter()
            .find(|entry| entry.link.id == written.link.id)
            .unwrap_or_else(|| panic!("the link is missing from {end}'s detail"));
        assert_eq!(entry.link.relation, "blocks");
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
        .map(|entry| entry.link.relation)
        .collect();
    assert!(
        relations.contains("blocks") && relations.contains("documents"),
        "folding must not merge distinct relations: {relations:?}"
    );
}

/// The whole note lifecycle over the command seam, in the order a person does
/// it: *New note*, type, refer to something, look at it from the other end.
///
/// The ref points at a **run-unique mirrored entity** (`linkable_pair`) rather
/// than at a fixture key, for the reason every link test in this file does:
/// the database is shared by the whole binary, and
/// `a_link_over_the_seam_fills_the_demo_profiles_empty_links_panel` asserts
/// that `mock:PAY-231` has *no* links -- a note that referred to it would break
/// that test, from here, by ordering. It is still a real mirror row and not a
/// row invented for the occasion.
#[tokio::test]
async fn a_note_written_over_the_seam_carries_its_refs_and_its_backlink() {
    let pool = seeded().await;
    use knobas_app::commands::entity::{create_note_inner, get_note_inner, save_note_inner};

    let (ticket, _unused) = linkable_pair(&pool).await;

    // Story 2: *New note* writes the row before anything is typed into it.
    let fresh = create_note_inner(&pool, None, None, &[]).await.unwrap();
    assert_eq!(fresh.note.title, knobas_core::note::UNTITLED);
    assert!(fresh.note.body_md.is_empty());
    assert!(fresh.refs.is_empty() && fresh.links.is_empty());
    let id = fresh.note.id.clone();

    // Stories 1, 3, 5, 6, 7: a title, a body, and a ref to something real.
    let saved = save_note_inner(
        &pool,
        &id,
        "SEPA retry investigation",
        &format!("off-by-one in [[{ticket}]], and [[mock:NOPE-1]] is a typo"),
    )
    .await
    .unwrap();
    assert_eq!(saved.note.title, "SEPA retry investigation");

    // Story 7: the chip has the target's kind and title to draw, not an id.
    assert_eq!(
        saved
            .refs
            .iter()
            .map(|r| r.target_id.as_str())
            .collect::<Vec<_>>(),
        [ticket.as_str(), "mock:NOPE-1"]
    );
    let resolved = saved.refs[0]
        .target
        .as_ref()
        .expect("the ticket is in the mirror");
    assert_eq!(resolved.kind, "ticket");
    assert!(!resolved.title.is_empty());
    // Story 10: the typo is a ref with nothing behind it, and it is *shown*.
    assert!(saved.refs[1].target.is_none());

    // Story 11: from the ticket, the note is a backlink -- the same panel #53
    // built, filled by the same read.
    let back = links_on(&pool, &ticket).await;
    let entry = back
        .iter()
        .find(|entry| entry.other.entity_id == id)
        .expect("the ticket shows the note that names it");
    assert_eq!(entry.other.kind, "note");
    assert_eq!(entry.other.title, "SEPA retry investigation");
    assert_eq!(entry.link.origin, knobas_core::link::Origin::Implied);
    assert_eq!(entry.link.relation, knobas_core::note::REF_RELATION);
    // ...and the note's own read shows the same link from its end.
    let read = get_note_inner(&pool, &id).await.unwrap();
    assert_eq!(read.note.body_md, saved.note.body_md);
    assert!(
        read.links
            .iter()
            .any(|entry| entry.other.entity_id == ticket)
    );
}

/* ------------------------------------- a note is born with its links (#502) */

/// The two relations a capture attaches (`CONTEXT.md`, **Capture**; spec #491
/// stories 40--43), spelled here as the ticket spells them.
///
/// Literals rather than a constant imported from somewhere: the frontend is
/// where these words live (`app/src/lib/detail/relations.ts`, the curated menu,
/// pinned to what *New note* sends by `shell/Room.test.svelte.ts`), because
/// nothing in Rust filters on them -- the command draws the relation it is
/// handed, the way `create_link_inner` does. What this file is entitled to
/// check is that the words survive the seam and read back on both ends.
const CAPTURED_IN: &str = "captured-in";
const CAPTURED_FROM: &str = "captured-from";

/// A note born in a **stored** room with a detail open carries both links,
/// and the `captured-in` one makes it a member of that context.
///
/// Membership is the point of criterion 1's first clause and it is not a
/// second mechanism: ADR-0008's seed is *"every confirmed link touching the
/// context's own `ctx:` entity"*, so the link **is** the add. Asserted through
/// `context::member_ids` -- the one statement every membership surface reads --
/// rather than by re-querying `knobas.link`, which would only restate the row
/// this test already wrote.
#[tokio::test]
async fn a_note_born_in_a_stored_room_carries_both_links_and_is_a_member() {
    let pool = seeded().await;
    use knobas_app::commands::entity::{NoteLinkInput, create_context_inner, create_note_inner};

    let ctx = create_context_inner(&pool, &format!("Capture {}", unique()))
        .await
        .unwrap();
    let (ticket, _unused) = linkable_pair(&pool).await;

    let born = create_note_inner(
        &pool,
        None,
        None,
        &[
            NoteLinkInput {
                target_id: ctx.id.clone(),
                relation: CAPTURED_IN.to_owned(),
            },
            NoteLinkInput {
                target_id: ticket.clone(),
                relation: CAPTURED_FROM.to_owned(),
            },
        ],
    )
    .await
    .unwrap();
    let note_id = born.note.id.clone();

    // Story 2 still holds: the row exists before anything is typed, and the
    // links arrived with it rather than after a first save.
    assert!(born.note.body_md.is_empty());
    assert!(
        born.refs.is_empty(),
        "a capture link is not a [[ref]]: the body names nothing"
    );

    let drawn: Vec<(&str, &str, knobas_core::link::Origin)> = born
        .links
        .iter()
        .map(|entry| {
            assert_eq!(
                entry.link.from_id, note_id,
                "the note is the from end, so `captured in` reads on the note"
            );
            (
                entry.other.entity_id.as_str(),
                entry.link.relation.as_str(),
                entry.link.origin,
            )
        })
        .collect();
    assert_eq!(drawn.len(), 2, "two links and no third: {drawn:?}");
    assert!(
        drawn.contains(&(
            ctx.id.as_str(),
            CAPTURED_IN,
            knobas_core::link::Origin::Manual
        )),
        "{drawn:?}"
    );
    assert!(
        drawn.contains(&(
            ticket.as_str(),
            CAPTURED_FROM,
            knobas_core::link::Origin::Manual
        )),
        "{drawn:?}"
    );

    // ADR-0008: an explicit add is a link touching the context's `ctx:` entity,
    // so the note is a member of it without a second write anywhere.
    let members = knobas_core::context::member_ids(&pool, &ctx.id)
        .await
        .unwrap();
    assert!(
        members.contains(&note_id),
        "the note is a member of the context it was captured in: {members:?}"
    );

    // ...and the thing the reader was looking at shows the thought it produced.
    let back = links_on(&pool, &ticket).await;
    let entry = back
        .iter()
        .find(|entry| entry.other.entity_id == note_id)
        .expect("the ticket shows the note captured from it");
    assert_eq!(entry.link.relation, CAPTURED_FROM);
    assert_eq!(entry.other.kind, "note");
}

/// A note born in a **derived** room with nothing open carries no links.
///
/// Both halves of criterion 1's negative, and on this seam they are one fact:
/// a derived room has no context (`CONTEXT.md`, **Room**) and an absent
/// foreground has nothing to point at, so the caller sends nothing and the
/// command draws nothing. *Which* rooms send nothing is the frontend's
/// decision and is pinned in `shell/Room.test.svelte.ts`; what is checked here
/// is that nothing means nothing -- no `related` fallback, no link to the room
/// the note happens to be listed in.
#[tokio::test]
async fn a_note_born_with_no_links_is_born_with_none() {
    let pool = seeded().await;
    use knobas_app::commands::entity::create_note_inner;

    let born = create_note_inner(&pool, Some("Loose thought"), None, &[])
        .await
        .unwrap();
    assert!(born.links.is_empty(), "{:?}", born.links);

    let read = knobas_app::commands::entity::get_note_inner(&pool, &born.note.id)
        .await
        .unwrap();
    assert!(read.links.is_empty(), "and still none on a fresh read");
}

/// Criterion 1's third case on its own: a foreground entity and no room.
///
/// Its own test rather than a clause of the two above, because the interesting
/// thing about it is that the two links are **independent**. A command that
/// drew the second only alongside the first -- or that took the first target
/// as the note's context and hung the second off it -- would pass both of the
/// tests above and fail here, which is the whole reason a reader standing in
/// *All work* over an open ticket still gets the link that says where the
/// thought came from.
#[tokio::test]
async fn a_note_born_with_only_a_foreground_carries_only_that_link() {
    let pool = seeded().await;
    use knobas_app::commands::entity::{NoteLinkInput, create_note_inner};

    let (ticket, _unused) = linkable_pair(&pool).await;
    let born = create_note_inner(
        &pool,
        None,
        None,
        &[NoteLinkInput {
            target_id: ticket.clone(),
            relation: CAPTURED_FROM.to_owned(),
        }],
    )
    .await
    .unwrap();

    let drawn: Vec<(&str, &str)> = born
        .links
        .iter()
        .map(|entry| (entry.other.entity_id.as_str(), entry.link.relation.as_str()))
        .collect();
    assert_eq!(drawn, [(ticket.as_str(), CAPTURED_FROM)]);
}

/// A target that is not an entity id is refused; a well-formed id nothing
/// carries draws no link and the note is written anyway.
///
/// The two directions are deliberately different, and the difference is who
/// can act on it. A malformed id and a blank relation are caller bugs,
/// deterministic, and worth refusals a test can pin. An id with no
/// `knobas.entity` row is not a bug anybody can act on, and refusing there
/// would make *New note* a button that stays broken while the reader can do
/// nothing about it -- the heartbeat's rule, quoted in `note::create`:
/// *"losing the attribution is honest, losing the observation is not"*.
///
/// **The absent case is narrower than it looks**, which is why the tombstone
/// below is here too: an entity the source withdrew still has its row, since a
/// purge tombstones and never deletes, so its born link **is** drawn and comes
/// back marked. What draws nothing is an id no row ever carried.
#[tokio::test]
async fn a_born_link_is_refused_for_a_bad_address_and_skipped_for_an_absent_one() {
    let pool = seeded().await;
    use knobas_app::commands::entity::{NoteLinkInput, create_note_inner};

    for bad in ["no-colon-here", "", ":x"] {
        let refused = create_note_inner(
            &pool,
            None,
            None,
            &[NoteLinkInput {
                target_id: bad.to_owned(),
                relation: CAPTURED_IN.to_owned(),
            }],
        )
        .await
        .unwrap_err();
        assert_eq!(
            refused.code,
            knobas_app::IpcErrorCode::Invalid,
            "{bad:?} is not an address"
        );
    }

    for blank in ["", "   "] {
        let refused = create_note_inner(
            &pool,
            None,
            None,
            &[NoteLinkInput {
                target_id: "mock:PAY-231".to_owned(),
                relation: blank.to_owned(),
            }],
        )
        .await
        .unwrap_err();
        assert_eq!(
            refused.code,
            knobas_app::IpcErrorCode::Invalid,
            "a link drawn without the reader seeing a dialog has no relation to default to"
        );
    }

    let gone = format!("ctx:{}", uuid::Uuid::new_v4());
    let born = create_note_inner(
        &pool,
        Some("Captured in a context nothing ever carried"),
        None,
        &[NoteLinkInput {
            target_id: gone,
            relation: CAPTURED_IN.to_owned(),
        }],
    )
    .await
    .unwrap();
    assert!(
        born.links.is_empty(),
        "no link, and the thought is still saved: {:?}",
        born.links
    );

    // ...and the case that is *not* that one. `mock:PAY-198` is the fixture's
    // genuinely tombstoned row -- the same one #53 pinned its hydration
    // against -- so this is the real state of a withdrawn entity and not a
    // `deleted_at` a test wrote by hand.
    let withdrawn = create_note_inner(
        &pool,
        Some("Captured from something the source dropped"),
        None,
        &[NoteLinkInput {
            target_id: "mock:PAY-198".to_owned(),
            relation: CAPTURED_FROM.to_owned(),
        }],
    )
    .await
    .unwrap();
    let entry = withdrawn
        .links
        .first()
        .expect("a purge tombstones and never deletes, so the row is there to link to");
    assert_eq!(entry.other.entity_id, "mock:PAY-198");
    assert_eq!(entry.link.relation, CAPTURED_FROM);
    assert!(
        entry.other.deleted_at.is_some(),
        "and the panel has what marks it withdrawn rather than a link that dangles silently"
    );
}

/// A born link survives the note's **first autosave**, which is #502's own
/// flow and one keystroke away from every note this feature makes.
///
/// `withdraw_refs_other_than` is scoped to `(this note, relation `references`,
/// origin `implied`)`, and a born link is outside it **twice over**: its
/// relation is `captured-in` or `captured-from` and its origin is `manual`.
/// Either clause alone would be enough, which is worth knowing rather than
/// assuming -- it means neither clause can be shown to matter by removing it,
/// and the thing this test pins is the conjunction: remove both and every born
/// link a reader ever made is withdrawn by their next keystroke.
///
/// One keystroke, not a hypothetical: *New note* opens the born note in
/// `NoteView.svelte`, whose `saveAfterMs` is 700, so the reader's first
/// character calls `save_note` on a body naming no `[[ref]]` at all. If that
/// withdrew born links, criterion 1 would be true at the instant of birth and
/// false a second later -- the version of this feature that passes every other
/// test in this file.
#[tokio::test]
async fn a_born_link_survives_the_notes_first_autosave() {
    let pool = seeded().await;
    use knobas_app::commands::entity::{
        NoteLinkInput, create_context_inner, create_note_inner, save_note_inner,
    };

    let ctx = create_context_inner(&pool, &format!("Capture {}", unique()))
        .await
        .unwrap();
    let (ticket, _unused) = linkable_pair(&pool).await;
    let born = create_note_inner(
        &pool,
        None,
        None,
        &[
            NoteLinkInput {
                target_id: ctx.id.clone(),
                relation: CAPTURED_IN.to_owned(),
            },
            NoteLinkInput {
                target_id: ticket.clone(),
                relation: CAPTURED_FROM.to_owned(),
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(born.links.len(), 2);

    // The first keystroke, as the editor sends it: a title it derived and a
    // body that refers to nothing.
    let saved = save_note_inner(&pool, &born.note.id, "S", "S")
        .await
        .unwrap();

    let still: Vec<(&str, &str)> = saved
        .links
        .iter()
        .map(|entry| (entry.other.entity_id.as_str(), entry.link.relation.as_str()))
        .collect();
    assert!(
        still.contains(&(ctx.id.as_str(), CAPTURED_IN))
            && still.contains(&(ticket.as_str(), CAPTURED_FROM)),
        "the body governs the links the body derived, and nothing else: {still:?}"
    );
    assert!(
        saved.refs.is_empty(),
        "and the body really did name nothing, so the reconciliation really did run"
    );
    // Membership is what the link buys, so it has to survive with it.
    let members = knobas_core::context::member_ids(&pool, &ctx.id)
        .await
        .unwrap();
    assert!(members.contains(&born.note.id), "{members:?}");
}

/// Story 9 over the seam: a ref whose target the source withdrew stays
/// visible and marked.
///
/// `mock:PAY-198` is the fixture's genuinely tombstoned row -- the same one
/// #53 pinned its hydration against -- so this is the real state and not a
/// `deleted_at` a test wrote by hand.
#[tokio::test]
async fn a_note_ref_to_a_withdrawn_ticket_resolves_and_is_marked() {
    let pool = seeded().await;
    use knobas_app::commands::entity::create_note_inner;

    let detail = create_note_inner(
        &pool,
        Some("Runbook"),
        Some("superseded: [[mock:PAY-198]]"),
        &[],
    )
    .await
    .unwrap();

    let target = detail.refs[0]
        .target
        .as_ref()
        .expect("a withdrawn entity still resolves -- that is the point");
    assert!(
        target.deleted_at.is_some(),
        "and the chip has what marks it withdrawn"
    );
    assert!(!target.title.is_empty(), "with its last-known title");
}

/// Story 4, and what happens to an editor that was open on the note.
#[tokio::test]
async fn deleting_a_note_over_the_seam_is_idempotent_and_a_stale_editor_is_told() {
    let pool = seeded().await;
    use knobas_app::commands::entity::{
        create_note_inner, delete_note_inner, get_note_inner, save_note_inner,
    };

    let detail = create_note_inner(&pool, Some("Scratch"), Some("a thought"), &[])
        .await
        .unwrap();
    let id = detail.note.id.clone();

    assert!(delete_note_inner(&pool, &id).await.unwrap());
    assert!(
        !delete_note_inner(&pool, &id).await.unwrap(),
        "a second delete deleted nothing, and says so"
    );

    for err in [
        get_note_inner(&pool, &id).await.unwrap_err(),
        save_note_inner(&pool, &id, "back?", "").await.unwrap_err(),
    ] {
        assert_eq!(
            err.code,
            knobas_app::IpcErrorCode::NotFound,
            "a deleted note is not resurrected by an editor that had not heard: {err}"
        );
    }
}

/// A bad address is a bad address here too, and not a 500.
#[tokio::test]
async fn a_malformed_note_id_is_invalid_and_an_unknown_one_is_not_found() {
    let pool = seeded().await;
    use knobas_app::commands::entity::{delete_note_inner, get_note_inner, save_note_inner};

    for bad in ["no-colon-here", "", ":x", "note:"] {
        assert_eq!(
            get_note_inner(&pool, bad).await.unwrap_err().code,
            knobas_app::IpcErrorCode::Invalid,
            "{bad:?}"
        );
        assert_eq!(
            save_note_inner(&pool, bad, "t", "b")
                .await
                .unwrap_err()
                .code,
            knobas_app::IpcErrorCode::Invalid,
            "{bad:?}"
        );
        assert_eq!(
            delete_note_inner(&pool, bad).await.unwrap_err().code,
            knobas_app::IpcErrorCode::Invalid,
            "{bad:?}"
        );
    }

    let nobody = format!("note:{}", uuid::Uuid::new_v4());
    assert_eq!(
        get_note_inner(&pool, &nobody).await.unwrap_err().code,
        knobas_app::IpcErrorCode::NotFound
    );
    // ...but deleting one that is not there is not an error at all.
    assert!(!delete_note_inner(&pool, &nobody).await.unwrap());
}

/// **The Kuma source has its room, and it has no project rooms** (spec #427,
/// *The Kuma room and tiles*: "No switcher exception: the Kuma source has its
/// room; it declares no projects").
///
/// The sibling of `a_confluence_space_is_a_project_room_and_a_jira_project_is_another`,
/// and it asserts the *other* direction of the same one read. That test shows
/// two adapters whose declarations produce rooms; this one shows an adapter
/// whose declaration produces none, which is the direction a hardcoded
/// per-source table would get wrong by omission and nothing else here would
/// catch: the census is built from `payload_paths`, so a source declaring no
/// `project_key` contributes nothing to it and the switcher offers its room
/// alone.
///
/// The declarations come out of `declared_paths` over the real registry, the
/// way the running binary's `list_projects` gets them -- so a `project_key`
/// added to the Kuma descriptor fails this test and not only the descriptor's
/// own.
#[tokio::test]
async fn a_kuma_source_has_a_room_of_its_own_and_no_project_rooms() {
    let pool = seeded().await;
    let watchtower = format!("kuma-{}", unique());

    sqlx::query(
        "insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind)
         values ($1, $2, $1, 'http://127.0.0.1:3001', 'api_token')",
    )
    .bind(&watchtower)
    .bind(knobas_source_kuma::ADAPTER_KIND)
    .execute(&pool)
    .await
    .unwrap();

    // The payload shape `knobas_source_kuma::map` writes: the state at the top
    // level, which is where the descriptor declares it, and nothing that looks
    // like a project anywhere.
    for (key, title, payload) in [
        (
            "7",
            "gitea",
            serde_json::json!({ "id": "7", "name": "gitea", "type": "http", "state": "up" }),
        ),
        (
            "8",
            "canary",
            serde_json::json!({ "id": "8", "name": "canary", "type": "http", "state": "down" }),
        ),
    ] {
        let id = format!("{watchtower}:{key}");
        sqlx::query("insert into knobas.entity (id, kind, title) values ($1, $2, $3)")
            .bind(&id)
            .bind(knobas_source_kuma::KIND_MONITOR)
            .bind(title)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
             values ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&id)
        .bind(&watchtower)
        .bind(knobas_source_kuma::KIND_MONITOR)
        .bind(title)
        .bind(format!("{title} {}", payload["state"].as_str().unwrap()))
        .bind(&payload)
        .execute(&pool)
        .await
        .unwrap();
    }

    let census = list_projects_inner(&pool)
        .await
        .expect("the census `list_projects` answers with");
    assert!(
        census.iter().all(|p| p.source_id != watchtower),
        "a source that declares no project_key offers no project rooms: {:?}",
        census
            .iter()
            .filter(|p| p.source_id == watchtower)
            .collect::<Vec<_>>()
    );

    let declarations = knobas_app::sources::paths::declared_paths(
        &pool,
        &knobas_app::sources::Registry::builtin(),
    )
    .await
    .expect("what the configured sources declare");

    // The source room itself: both monitors, under the one filter the room
    // hands every tile.
    let room = EntityFilter {
        sources: vec![watchtower.clone()],
        ..all()
    };
    let rows = list_entities_inner(&pool, &room, 500, 0, &declarations)
        .await
        .unwrap();
    let ids: std::collections::BTreeSet<String> =
        rows.rows.into_iter().map(|row| row.entity_id).collect();
    assert_eq!(
        ids,
        std::collections::BTreeSet::from([format!("{watchtower}:7"), format!("{watchtower}:8")]),
        "the Kuma room holds this source's monitors"
    );

    // And the tile the room draws for them is the adapter's own -- `monitor`
    // is not one of the buckets `app/src/lib/shell/kinds.ts` names, so what
    // labels it is the `KindInfo` this descriptor declares (§3a).
    let kind = knobas_source_kuma::descriptor_template()
        .entity_kinds
        .into_iter()
        .find(|k| k.id == knobas_source_kuma::KIND_MONITOR)
        .expect("the descriptor declares the kind its items carry");
    assert_eq!(kind.plural, "Monitors");
}
