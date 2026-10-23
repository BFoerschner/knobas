//! The mini board's granted read (#177), against a real PostgreSQL.
//!
//! The one-hop membership *rule* is proven next door in `contexts.rs`; what is
//! asserted here is the read built on it -- which tickets a room's board draws,
//! which column each lands in, what order the columns come in, and what the two
//! payload reads do when they miss.
//!
//! Every test gets a database of its own, the trade `contexts.rs` records: the
//! board's second half is a pass over each source's whole ticket corpus, so a
//! shared database would let one test's fixture decide another's answer, and
//! fixtures that read as `PAY-231` are worth one database each.

use std::collections::BTreeMap;

use knobas_core::entity::EntityRef;
use knobas_core::link::Origin;
use knobas_core::mini_board::{self, MiniBoard};
use knobas_core::payload::{Declarations, KindPaths, PayloadPath};
use knobas_core::{context, link};
use sqlx::PgPool;

/// The source most fixtures below are synced under.
const SOURCE: &str = "jira";

/// What the sources in this file declare about where they keep a status and a
/// priority (#277) -- the read is path-driven now, so a fixture's source has
/// to have said where its own spellings are, the way its adapter's descriptor
/// does.
///
/// `jira` and `gitea` in Jira Data Center's shape, which is what the fixtures
/// below are written in; `mock` in the Tidewater fixture's flat shape, which
/// is `knobas_source_mock`'s own declaration. Two spellings, so the tests that
/// exercise each are exercising a real difference rather than one statement
/// twice.
fn declarations() -> Declarations {
    let nested = |kind: &str| KindPaths {
        kind: kind.to_owned(),
        status_name: vec![PayloadPath::of(["fields", "status", "name"])],
        priority: vec![PayloadPath::of(["fields", "priority", "name"])],
        project_key: vec![PayloadPath::of(["fields", "project", "key"])],
        ..KindPaths::default()
    };
    Declarations::empty()
        .with(SOURCE, vec![nested("ticket")])
        .with("gitea", vec![nested("ticket"), nested("pr")])
        .with(
            "mock",
            vec![KindPaths {
                kind: "ticket".to_owned(),
                status_name: vec![PayloadPath::of(["status"])],
                priority: vec![PayloadPath::of(["priority"])],
                ..KindPaths::default()
            }],
        )
}

/// A migrated, empty database of this test's own.
async fn scratch() -> PgPool {
    knobas_db::test_util::scratch_database("mini_board")
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database")
}

/// A Jira Data Center issue's shape, as far as this read cares:
/// `fields.status.name` and `fields.priority.name`. A `None` leaves the
/// container out entirely -- what an issue whose field is unset looks like.
fn jira(status: Option<&str>, priority: Option<&str>) -> serde_json::Value {
    let mut fields = serde_json::Map::new();
    if let Some(status) = status {
        fields.insert("status".to_owned(), serde_json::json!({ "name": status }));
    }
    if let Some(priority) = priority {
        fields.insert(
            "priority".to_owned(),
            serde_json::json!({ "name": priority }),
        );
    }
    serde_json::json!({ "fields": serde_json::Value::Object(fields) })
}

/// A Jira issue carrying a status and a project, which is the pair a project
/// room's board is drawn from.
fn jira_in(status: &str, project: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "fields": { "status": { "name": status }, "project": project }
    })
}

/// One live mirror item of the given kind, carrying the payload it is given.
async fn item(
    pool: &PgPool,
    source: &str,
    kind: &str,
    key: &str,
    payload: serde_json::Value,
) -> String {
    let id = EntityRef::new(source, key).to_string();
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(&id)
        .bind(kind)
        .bind(format!("{key} title"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'',$5)",
    )
    .bind(&id)
    .bind(source)
    .bind(kind)
    .bind(format!("{key} title"))
    .bind(payload)
    .execute(pool)
    .await
    .unwrap();
    id
}

/// A live ticket in [`SOURCE`], carrying a Jira-shaped status and priority.
async fn ticket(pool: &PgPool, key: &str, status: Option<&str>, priority: Option<&str>) -> String {
    item(pool, SOURCE, "ticket", key, jira(status, priority)).await
}

/// When the source last touched this item. Set explicitly wherever a test
/// asserts an order, so the assertion stands on the fixture rather than on how
/// fast the rows happened to be inserted.
async fn touched(pool: &PgPool, id: &str, at: &str) {
    sqlx::query("update sync.item set item_updated_at = $2::timestamptz where entity_id = $1")
        .bind(id)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
}

/// Mark an entity deleted at its source -- what a sweep or a purge does.
async fn tombstone(pool: &PgPool, id: &str) {
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

/// A confirmed link between two entities, drawn by hand -- `contexts.rs`'
/// `draw`, which is how an explicit *add to this context* is seeded.
async fn draw(pool: &PgPool, from: &str, to: &str) {
    link::create(
        pool,
        &EntityRef::parse(from).unwrap(),
        &EntityRef::parse(to).unwrap(),
        "related",
        Origin::Manual,
        None,
        "user",
    )
    .await
    .unwrap();
}

/// A stored room: an ad-hoc context with each entity explicitly added to it.
async fn stored_room(pool: &PgPool, members: &[&str]) -> String {
    let ctx = context::create_adhoc(pool, "Payout reliability")
        .await
        .unwrap();
    for member in members {
        draw(pool, &ctx.id, member).await;
    }
    ctx.id
}

/// The board of a stored room.
async fn board_of(pool: &PgPool, ctx: &str) -> MiniBoard {
    mini_board::read(pool, Some(ctx), &[], None, &declarations())
        .await
        .unwrap()
}

/// The columns as `(status, the keys in them)`, keys sorted: the spec pins the
/// order of the *columns*, and a test that also depended on the order within
/// one would be asserting something nothing promised.
/// `cards_in_a_column_come_newest_first` is where that order is pinned.
fn columns(board: &MiniBoard) -> Vec<(Option<&str>, Vec<&str>)> {
    board
        .columns
        .iter()
        .map(|column| {
            let mut keys: Vec<&str> = column.cards.iter().map(|card| card.key.as_str()).collect();
            keys.sort_unstable();
            (column.status.as_deref(), keys)
        })
        .collect()
}

/// What the board offers for one source -- the second consumer's read (#179).
fn offered<'b>(board: &'b MiniBoard, source: &str) -> &'b [String] {
    board
        .sources
        .iter()
        .find(|entry| entry.source_id == source)
        .map_or(&[], |entry| entry.statuses.as_slice())
}

/// The four the mockup draws lead in the order work moves through them; every
/// other observed status follows alphabetically; the terminal group is last.
#[tokio::test]
async fn the_columns_are_the_observed_statuses_with_the_four_leading() {
    let pool = scratch().await;
    let members = [
        ticket(&pool, "PAY-1", Some("Done"), None).await,
        ticket(&pool, "PAY-2", Some("To Do"), None).await,
        ticket(&pool, "PAY-3", Some("Blocked"), None).await,
        ticket(&pool, "PAY-4", Some("In Review"), None).await,
        ticket(&pool, "PAY-5", None, None).await,
        ticket(&pool, "PAY-6", Some("Awaiting deploy"), None).await,
        ticket(&pool, "PAY-7", Some("In Progress"), None).await,
    ];
    let refs: Vec<&str> = members.iter().map(String::as_str).collect();
    let ctx = stored_room(&pool, &refs).await;

    let board = board_of(&pool, &ctx).await;

    assert_eq!(
        columns(&board),
        vec![
            (Some("To Do"), vec!["PAY-2"]),
            (Some("In Progress"), vec!["PAY-7"]),
            (Some("In Review"), vec!["PAY-4"]),
            (Some("Done"), vec!["PAY-1"]),
            (Some("Awaiting deploy"), vec!["PAY-6"]),
            (Some("Blocked"), vec!["PAY-3"]),
            (None, vec!["PAY-5"]),
        ]
    );
    assert_eq!(
        offered(&board, SOURCE),
        [
            "To Do",
            "In Progress",
            "In Review",
            "Done",
            "Awaiting deploy",
            "Blocked"
        ],
        "the offered statuses read in the board's order too"
    );
}

/// Within a column the newest work is at the top, as the flat list the tile
/// used to draw had it.
#[tokio::test]
async fn cards_in_a_column_come_newest_first() {
    let pool = scratch().await;
    let old = ticket(&pool, "PAY-70", Some("To Do"), None).await;
    let new = ticket(&pool, "PAY-71", Some("To Do"), None).await;
    let middle = ticket(&pool, "PAY-72", Some("To Do"), None).await;
    touched(&pool, &old, "2026-08-01T09:00:00Z").await;
    touched(&pool, &new, "2026-08-30T09:00:00Z").await;
    touched(&pool, &middle, "2026-08-15T09:00:00Z").await;
    let ctx = stored_room(&pool, &[&old, &new, &middle]).await;

    let board = board_of(&pool, &ctx).await;

    let keys: Vec<&str> = board.columns[0]
        .cards
        .iter()
        .map(|card| card.key.as_str())
        .collect();
    assert_eq!(keys, ["PAY-71", "PAY-72", "PAY-70"]);
}

/// Miss direction one, pinned: a ticket whose record carries no status knobas
/// can read is *visible* in the terminal group -- never dropped, and never
/// guessed into a column of its own.
#[tokio::test]
async fn a_ticket_with_no_recognizable_status_lands_in_the_terminal_group() {
    let pool = scratch().await;
    let absent = ticket(&pool, "PAY-10", None, None).await;
    // The path exists but does not lead to a string -- a source that spells its
    // status some other way, which this read must not stringify into a column.
    let misshapen = item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-11",
        serde_json::json!({ "fields": { "status": { "name": { "id": 3 } } } }),
    )
    .await;
    let blank = ticket(&pool, "PAY-12", Some("   "), None).await;
    let ctx = stored_room(&pool, &[&absent, &misshapen, &blank]).await;

    let board = board_of(&pool, &ctx).await;

    assert_eq!(
        columns(&board),
        vec![(None, vec!["PAY-10", "PAY-11", "PAY-12"])],
        "every unreadable status belongs in the one terminal group"
    );
    assert!(
        board.columns[0]
            .cards
            .iter()
            .all(|card| card.status.is_none()),
        "a card in the terminal group carries no status either (#1423)"
    );
    assert!(
        offered(&board, SOURCE).is_empty(),
        "the terminal group is not a status anything can be moved to"
    );
}

/// Miss direction two, pinned: an unreadable priority renders nothing at all.
#[tokio::test]
async fn a_ticket_with_no_recognizable_priority_carries_none() {
    let pool = scratch().await;
    let high = ticket(&pool, "PAY-20", Some("To Do"), Some("High")).await;
    let absent = ticket(&pool, "PAY-21", Some("To Do"), None).await;
    // The path leads somewhere, but not to a string: the shape this read must
    // not stringify onto a card as `{"id":3}`.
    let misshapen = item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-22",
        serde_json::json!({
            "fields": { "status": { "name": "To Do" }, "priority": { "name": { "id": 3 } } }
        }),
    )
    .await;
    let blank = ticket(&pool, "PAY-23", Some("To Do"), Some("   ")).await;
    let ctx = stored_room(&pool, &[&high, &absent, &misshapen, &blank]).await;

    let board = board_of(&pool, &ctx).await;

    let priorities: BTreeMap<&str, Option<&str>> = board.columns[0]
        .cards
        .iter()
        .map(|card| (card.key.as_str(), card.priority.as_deref()))
        .collect();
    assert_eq!(
        priorities,
        BTreeMap::from([
            ("PAY-20", Some("High")),
            ("PAY-21", None),
            ("PAY-22", None),
            ("PAY-23", None),
        ])
    );
}

/// A ticket the source deleted leaves the board, though the rule still counts
/// it a member: the board is drawn from the mirror's live view.
#[tokio::test]
async fn a_tombstoned_ticket_is_off_the_board() {
    let pool = scratch().await;
    let live = ticket(&pool, "PAY-30", Some("To Do"), None).await;
    let gone = ticket(&pool, "PAY-31", Some("Done"), None).await;
    let ctx = stored_room(&pool, &[&live, &gone]).await;
    tombstone(&pool, &gone).await;

    let board = board_of(&pool, &ctx).await;

    assert_eq!(columns(&board), vec![(Some("To Do"), vec!["PAY-30"])]);
    assert_eq!(
        offered(&board, SOURCE),
        ["To Do"],
        "a tombstoned ticket's status is not one the source still shows"
    );
}

/// A stored room's board is its members', and it is tickets: a ticket nobody
/// added and a member of another kind are both absent.
#[tokio::test]
async fn only_the_contexts_live_ticket_members_are_on_a_stored_rooms_board() {
    let pool = scratch().await;
    let member = ticket(&pool, "PAY-40", Some("To Do"), None).await;
    let pr = item(
        &pool,
        SOURCE,
        "pr",
        "payout#1",
        serde_json::json!({ "fields": { "status": { "name": "In Review" } } }),
    )
    .await;
    ticket(&pool, "PAY-41", Some("In Progress"), None).await;
    let ctx = stored_room(&pool, &[&member, &pr]).await;

    let board = board_of(&pool, &ctx).await;

    assert_eq!(columns(&board), vec![(Some("To Do"), vec!["PAY-40"])]);
}

/// The rooms every session can start in, neither of which is a stored context
/// (`app/src/lib/shell/contexts.ts`): *All work* narrows by nothing, and a
/// source's room narrows by that source. A board that could only answer for a
/// context would be empty in both.
#[tokio::test]
async fn a_derived_room_scopes_by_its_sources_and_all_work_by_nothing() {
    let pool = scratch().await;
    ticket(&pool, "PAY-90", Some("To Do"), None).await;
    item(
        &pool,
        "gitea",
        "ticket",
        "OPS-90",
        jira(Some("In Progress"), None),
    )
    .await;
    // In no room's scope by kind, whichever way the room is narrowed.
    item(&pool, "gitea", "pr", "payout#9", jira(Some("Done"), None)).await;

    let everything = mini_board::read(&pool, None, &[], None, &declarations())
        .await
        .unwrap();
    assert_eq!(
        columns(&everything),
        vec![
            (Some("To Do"), vec!["PAY-90"]),
            (Some("In Progress"), vec!["OPS-90"]),
        ],
        "*All work* narrows by nothing at all"
    );

    let one_source = mini_board::read(&pool, None, &["gitea".to_owned()], None, &declarations())
        .await
        .unwrap();
    assert_eq!(
        columns(&one_source),
        vec![(Some("In Progress"), vec!["OPS-90"])]
    );
    assert!(
        offered(&one_source, SOURCE).is_empty(),
        "a source with no card on this board is not one the board answers for"
    );
}

/// One room's work reads as one board: two sources spelling a status the same
/// way share its column, and each card still says which source it is from.
#[tokio::test]
async fn identically_spelled_statuses_from_two_sources_share_a_column() {
    let pool = scratch().await;
    let a = ticket(&pool, "PAY-50", Some("In Progress"), None).await;
    let b = item(
        &pool,
        "gitea",
        "ticket",
        "OPS-50",
        jira(Some("In Progress"), None),
    )
    .await;
    let ctx = stored_room(&pool, &[&a, &b]).await;

    let board = board_of(&pool, &ctx).await;

    assert_eq!(
        columns(&board),
        vec![(Some("In Progress"), vec!["OPS-50", "PAY-50"])],
        "one status, one column, whichever source spelled it"
    );
    let carried: Vec<Option<&str>> = board.columns[0]
        .cards
        .iter()
        .map(|card| card.status.as_deref())
        .collect();
    assert_eq!(
        carried,
        [Some("In Progress"), Some("In Progress")],
        "each card carries the status it stands in (#1423)"
    );
    let sources: Vec<&str> = board.sources.iter().map(|s| s.source_id.as_str()).collect();
    assert_eq!(
        sources,
        ["gitea", SOURCE],
        "and the board answers for both sources it drew from"
    );
}

/// The mock source spells a ticket's status and priority flat, where Jira puts
/// them under `fields` -- and the mock is what the demo profile's board is
/// drawn from, so both spellings are one `coalesce` in one statement. Without
/// this the second arm of each read would be untested code.
#[tokio::test]
async fn the_mock_sources_flat_spelling_reads_as_well_as_jiras() {
    let pool = scratch().await;
    // The Tidewater fixture's own shape, which `knobas_source_mock` puts in
    // `payload` verbatim.
    let flat = item(
        &pool,
        "mock",
        "ticket",
        "PAY-80",
        serde_json::json!({
            "key": "PAY-80",
            "summary": "Retry failed SEPA payouts",
            "type": "Story",
            "status": "In Progress",
            "priority": "High",
        }),
    )
    .await;
    let ctx = stored_room(&pool, &[&flat]).await;

    let board = board_of(&pool, &ctx).await;

    assert_eq!(columns(&board), vec![(Some("In Progress"), vec!["PAY-80"])]);
    assert_eq!(board.columns[0].cards[0].priority.as_deref(), Some("High"));
    assert_eq!(offered(&board, "mock"), ["In Progress"]);
}

/// The second consumer (#179): the statuses a source's own corpus shows, which
/// is wider than the room's columns and never includes the terminal group.
#[tokio::test]
async fn the_offered_statuses_are_the_sources_corpus_not_the_rooms_columns() {
    let pool = scratch().await;
    let member = ticket(&pool, "PAY-60", Some("To Do"), None).await;
    // Same source, not in the room, carrying a status the room never shows --
    // the move the select has to be able to offer.
    ticket(&pool, "PAY-61", Some("Done"), None).await;
    // Same source, no readable status: not something a ticket can be moved to.
    ticket(&pool, "PAY-62", None, None).await;
    // A different source entirely: its statuses are not this one's.
    item(
        &pool,
        "gitea",
        "ticket",
        "OPS-60",
        jira(Some("Blocked"), None),
    )
    .await;
    let ctx = stored_room(&pool, &[&member]).await;

    let board = board_of(&pool, &ctx).await;

    assert_eq!(
        columns(&board),
        vec![(Some("To Do"), vec!["PAY-60"])],
        "the room's own column, not the corpus'"
    );
    assert_eq!(
        offered(&board, SOURCE),
        ["To Do", "Done"],
        "the source's own statuses, in the board's order, with no terminal group"
    );
    assert!(
        offered(&board, "gitea").is_empty(),
        "a source with no card on this board is not one the board answers for"
    );
}

/// An address can outlive the context it names, and a room can be empty; both
/// answer with an empty board rather than a failure, so the tile can tell
/// "nothing here" from "this broke".
#[tokio::test]
async fn an_unknown_or_empty_context_is_an_empty_board() {
    let pool = scratch().await;
    ticket(&pool, "PAY-99", Some("To Do"), None).await;
    let empty = stored_room(&pool, &[]).await;

    for ctx in [empty.as_str(), "ctx:nothing-here"] {
        let board = board_of(&pool, ctx).await;
        assert!(
            board.columns.is_empty(),
            "{ctx} has no column, though the corpus does"
        );
        assert!(board.sources.is_empty(), "{ctx} has no source");
    }
}

/// A project room draws one project's work, and one project's only.
///
/// The room narrows **within** its sources: a project key is unique only
/// inside its own source, so a project room names both and the board honours
/// both.
#[tokio::test]
async fn a_project_room_draws_only_that_projects_tickets() {
    let pool = scratch().await;
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-1",
        jira_in(
            "To Do",
            serde_json::json!({ "key": "PAY", "name": "Payout" }),
        ),
    )
    .await;
    item(
        &pool,
        SOURCE,
        "ticket",
        "INT-1",
        jira_in(
            "In Progress",
            serde_json::json!({ "key": "INT", "name": "Integrations" }),
        ),
    )
    .await;
    // Another source's `PAY`, which is another project entirely.
    item(
        &pool,
        "jira-eu",
        "ticket",
        "PAY-1",
        jira_in(
            "Done",
            serde_json::json!({ "key": "PAY", "name": "Payments" }),
        ),
    )
    .await;

    let board = mini_board::read(
        &pool,
        None,
        &[SOURCE.to_owned()],
        Some("PAY"),
        &declarations(),
    )
    .await
    .unwrap();

    assert_eq!(columns(&board), vec![(Some("To Do"), vec!["PAY-1"])]);
}

/// The failure direction, on the board: a ticket whose record carries no
/// readable project is on **no** project room's board, and is still on the
/// source room's and on *All work*'s.
///
/// Absence, never a wrong room -- and nothing is hidden, because two other
/// rooms still hold it (ADR-0010, which is also why there is no "No project"
/// room to draw).
#[tokio::test]
async fn a_ticket_with_no_readable_project_is_on_no_project_rooms_board() {
    let pool = scratch().await;
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-1",
        jira_in(
            "To Do",
            serde_json::json!({ "key": "PAY", "name": "Payout" }),
        ),
    )
    .await;
    // The path leads somewhere, but not to a string: the shape a room must not
    // be headed by.
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-2",
        jira_in("To Do", serde_json::json!({ "key": { "id": 3 } })),
    )
    .await;
    item(&pool, SOURCE, "ticket", "PAY-3", jira(Some("To Do"), None)).await;

    let project_room = mini_board::read(
        &pool,
        None,
        &[SOURCE.to_owned()],
        Some("PAY"),
        &declarations(),
    )
    .await
    .unwrap();
    assert_eq!(
        columns(&project_room),
        vec![(Some("To Do"), vec!["PAY-1"])],
        "a record that names no project belongs to no project room"
    );

    let source_room = mini_board::read(&pool, None, &[SOURCE.to_owned()], None, &declarations())
        .await
        .unwrap();
    assert_eq!(
        columns(&source_room),
        vec![(Some("To Do"), vec!["PAY-1", "PAY-2", "PAY-3"])],
        "and is still in its source's room"
    );

    let all_work = mini_board::read(&pool, None, &[], None, &declarations())
        .await
        .unwrap();
    assert_eq!(
        columns(&all_work),
        vec![(Some("To Do"), vec!["PAY-1", "PAY-2", "PAY-3"])],
        "and in *All work*"
    );
}

/// A source the user turned off has its work off a project room's board, the
/// way a tombstoned ticket does: the board reads `sync.live_item`, and since
/// migration `0012` that view has two reasons to hide a row (#202).
#[tokio::test]
async fn a_disabled_sources_ticket_is_off_a_project_rooms_board() {
    let pool = scratch().await;
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-1",
        jira_in(
            "To Do",
            serde_json::json!({ "key": "PAY", "name": "Payout" }),
        ),
    )
    .await;
    let withdrawn = item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-2",
        jira_in(
            "In Progress",
            serde_json::json!({ "key": "PAY", "name": "Payout" }),
        ),
    )
    .await;
    tombstone(&pool, &withdrawn).await;
    item(
        &pool,
        "jira-eu",
        "ticket",
        "PAY-9",
        jira_in(
            "Done",
            serde_json::json!({ "key": "PAY", "name": "Payout" }),
        ),
    )
    .await;
    sqlx::query(
        "insert into knobas.source_config
             (id, kind, display_name, base_url, auth_kind, enabled)
         values ('jira-eu', 'jira', 'Jira EU', 'http://localhost', 'pat', false)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let board = mini_board::read(&pool, None, &[], Some("PAY"), &declarations())
        .await
        .unwrap();

    assert_eq!(
        columns(&board),
        vec![(Some("To Do"), vec!["PAY-1"])],
        "neither the withdrawn ticket nor the disabled source's is on it"
    );
}

/// The status select's offer is **not** narrowed by the project, and that is a
/// decision rather than an omission.
///
/// `MiniBoard::sources` is the statuses a ticket's *source corpus* shows
/// (#179): a project room with nothing finished still has to be able to offer
/// *Done*, and narrowing the offer by the room would make the move available
/// only where it had already been made.
#[tokio::test]
async fn the_status_select_still_offers_the_whole_sources_corpus() {
    let pool = scratch().await;
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-1",
        jira_in(
            "To Do",
            serde_json::json!({ "key": "PAY", "name": "Payout" }),
        ),
    )
    .await;
    item(
        &pool,
        SOURCE,
        "ticket",
        "INT-1",
        jira_in(
            "Done",
            serde_json::json!({ "key": "INT", "name": "Integrations" }),
        ),
    )
    .await;

    let board = mini_board::read(
        &pool,
        None,
        &[SOURCE.to_owned()],
        Some("PAY"),
        &declarations(),
    )
    .await
    .unwrap();

    assert_eq!(columns(&board), vec![(Some("To Do"), vec!["PAY-1"])]);
    assert_eq!(
        offered(&board, SOURCE),
        ["To Do", "Done"],
        "a room with nothing finished can still move a ticket to Done"
    );
}

/// **A capped board draws each column's newest cards and counts them all**
/// (#753). The All room of a mirror with 15,000 tickets answered 2 MB and drew
/// every card; the tile asks for the newest `per_column` of each column, and
/// the column's `total` is what its header says. Three columns of different
/// depths, so a cap applied to the board rather than per column, or a total
/// counted after the cut, gives a different answer than this one.
#[tokio::test]
async fn a_capped_board_keeps_each_columns_newest_cards_and_counts_them_all() {
    let pool = scratch().await;
    for n in 0..5 {
        let id = ticket(&pool, &format!("PAY-{n}"), Some("To Do"), None).await;
        touched(&pool, &id, &format!("2026-10-0{}T09:00:00Z", n + 1)).await;
    }
    ticket(&pool, "PAY-10", Some("In Progress"), None).await;
    for n in 20..23 {
        let id = ticket(&pool, &format!("PAY-{n}"), Some("Done"), None).await;
        touched(&pool, &id, &format!("2026-09-0{}T09:00:00Z", n - 19)).await;
    }

    let board = mini_board::read_capped(&pool, None, &[], None, &declarations(), Some(2))
        .await
        .unwrap();
    let shape: Vec<(Option<&str>, Vec<&str>, u32)> = board
        .columns
        .iter()
        .map(|c| {
            (
                c.status.as_deref(),
                c.cards.iter().map(|card| card.key.as_str()).collect(),
                c.total,
            )
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            (Some("To Do"), vec!["PAY-4", "PAY-3"], 5),
            (Some("In Progress"), vec!["PAY-10"], 1),
            (Some("Done"), vec!["PAY-22", "PAY-21"], 3),
        ]
    );

    // Uncapped, it is the board it always was: every card, and the total is
    // the count of them.
    let whole = mini_board::read(&pool, None, &[], None, &declarations())
        .await
        .unwrap();
    for column in &whole.columns {
        assert_eq!(column.total as usize, column.cards.len());
    }
    assert_eq!(whole.columns[0].cards.len(), 5);
}

/// **The board's statements are planned for their parameters every time**
/// (#753). Kept as prepared statements, PostgreSQL moved them to a generic
/// plan on their sixth execution, and an All room of 15,000 tickets went from
/// 0.25 s to 6.6 s, every time after. What is pinned is the mechanism: after
/// eight board reads on one connection, that connection holds no prepared
/// statement of the board's.
#[tokio::test]
async fn the_boards_statements_are_never_kept_as_prepared_statements() {
    let connector = knobas_db::test_util::scratch_database("mini_board").await;
    let pool = connector.pool(1).await.expect("a one-connection pool");
    ticket(&pool, "PAY-1", Some("To Do"), None).await;
    for _ in 0..8 {
        mini_board::read_capped(&pool, None, &[], None, &declarations(), Some(50))
            .await
            .unwrap();
        mini_board::read(&pool, None, &[SOURCE.to_owned()], None, &declarations())
            .await
            .unwrap();
    }
    let kept: i64 = sqlx::query_scalar(
        "select count(*) from pg_prepared_statements where statement like '%status_paths%'",
    )
    .persistent(false)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        kept, 0,
        "a board statement was kept, and goes generic on its sixth run"
    );
}

/// **One source's offered statuses** (#760, through the board since #1612).
/// The offer is the source's live corpus, in the board's order, with no
/// terminal group, no other source's statuses, and nothing a tombstoned
/// ticket said; a source with no tickets offers nothing.
#[tokio::test]
async fn a_sources_offered_statuses_are_its_live_corpus_in_the_boards_order() {
    let pool = scratch().await;
    ticket(&pool, "PAY-1", Some("Done"), None).await;
    ticket(&pool, "PAY-2", Some("To Do"), None).await;
    ticket(&pool, "PAY-3", Some("blocked"), None).await;
    ticket(&pool, "PAY-4", None, None).await;
    let gone = ticket(&pool, "PAY-5", Some("Abandoned"), None).await;
    tombstone(&pool, &gone).await;
    item(
        &pool,
        "gitea",
        "ticket",
        "OPS-1",
        jira(Some("Elsewhere"), None),
    )
    .await;

    let board = mini_board::read(&pool, None, &[SOURCE.to_owned()], None, &declarations())
        .await
        .unwrap();
    assert_eq!(offered(&board, SOURCE), ["To Do", "Done", "blocked"]);
    assert!(offered(&board, "gitea").is_empty());

    let nowhere = mini_board::read(&pool, None, &["nowhere".to_owned()], None, &declarations())
        .await
        .unwrap();
    assert!(offered(&nowhere, "nowhere").is_empty());
}

// -- the Tickets view's board (#810) ----------------------------------------

/// What the Jira declaration says about a ticket beyond its status (#810):
/// its type, its assignee and its epic, the fields a ticket card draws.
fn ticket_declarations() -> Declarations {
    Declarations::empty().with(
        SOURCE,
        vec![KindPaths {
            kind: "ticket".to_owned(),
            status_name: vec![PayloadPath::of(["fields", "status", "name"])],
            priority: vec![PayloadPath::of(["fields", "priority", "name"])],
            assignee: vec![PayloadPath::of(["fields", "assignee", "name"])],
            issue_type: vec![PayloadPath::of(["fields", "issuetype", "name"])],
            comment_count: vec![PayloadPath::of(["fields", "comment", "total"])],
            parent: vec![knobas_core::payload::ParentPath {
                kind: "ticket".to_owned(),
                at: PayloadPath::of(["epic"]),
                names: vec![PayloadPath::of(["key"])],
            }],
            ..KindPaths::default()
        }],
    )
}

/// A card carries what the Tickets view draws on it: type, assignee, when it
/// changed, the epic it sits under, and whether it is assigned to me. A
/// ticket whose record names none of them carries none, never a guess.
#[tokio::test]
async fn a_tickets_card_carries_its_type_assignee_epic_and_whether_it_is_mine() {
    let pool = scratch().await;
    let full = item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-231",
        serde_json::json!({
            "fields": {
                "status": { "name": "In Progress" },
                "issuetype": { "name": "Story" },
                "assignee": { "name": "Mara.Lindqvist" }
            },
            "epic": { "key": "PAY-200" }
        }),
    )
    .await;
    touched(&pool, &full, "2026-08-22T11:48:00Z").await;
    item(&pool, SOURCE, "ticket", "PAY-1", jira(Some("To Do"), None)).await;

    let board = mini_board::read_scoped(
        &pool,
        mini_board::Scope {
            identity: &["mara.lindqvist".to_owned()],
            ..mini_board::Scope::default()
        },
        &ticket_declarations(),
        None,
    )
    .await
    .unwrap();

    let card = |key: &str| {
        board
            .columns
            .iter()
            .flat_map(|column| &column.cards)
            .find(|card| card.key == key)
            .unwrap()
            .clone()
    };
    let full = card("PAY-231");
    assert_eq!(full.issue_type.as_deref(), Some("Story"));
    assert_eq!(full.assignee.as_deref(), Some("Mara.Lindqvist"));
    assert_eq!(full.path.as_deref(), Some("PAY-200"));
    assert_eq!(
        full.updated_at.map(|at| at.to_rfc3339()),
        Some("2026-08-22T11:48:00+00:00".to_owned())
    );
    assert!(full.mine, "the identity matches ignoring case");

    let bare = card("PAY-1");
    assert_eq!(bare.issue_type, None);
    assert_eq!(bare.assignee, None);
    assert_eq!(bare.path, None);
    assert!(!bare.mine, "an unassigned ticket is nobody's");
}

/// A card carries its comment count and its epic's name (#894): the count at
/// the declared `comment_count` path, the name the title of the mirrored item
/// the declared parent names. A record with no count, a count Jira did not
/// write as a whole number, an epic the mirror does not hold and an item that
/// does not answer to the name the child gives all carry nothing (ADR-0007).
#[tokio::test]
async fn a_tickets_card_carries_its_comment_count_and_its_epics_name() {
    let pool = scratch().await;
    let epic = item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-200",
        serde_json::json!({ "key": "PAY-200", "fields": { "status": { "name": "In Progress" } } }),
    )
    .await;
    sqlx::query("update sync.item set title = 'Payout reliability' where entity_id = $1")
        .bind(&epic)
        .execute(&pool)
        .await
        .unwrap();
    // An item at the address the name would give, which does not answer to
    // that name: its record carries no `key`, so it is not PAY-300 as far as
    // the declaration says.
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-300",
        serde_json::json!({ "fields": { "status": { "name": "To Do" } } }),
    )
    .await;
    let child = |epic: &str, comments: serde_json::Value| {
        serde_json::json!({
            "fields": { "status": { "name": "To Do" }, "comment": { "total": comments } },
            "epic": { "key": epic }
        })
    };
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-231",
        child("PAY-200", 3.into()),
    )
    .await;
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-232",
        child("PAY-999", 0.into()),
    )
    .await;
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-233",
        child("PAY-300", "2".into()),
    )
    .await;
    item(&pool, SOURCE, "ticket", "PAY-1", jira(Some("To Do"), None)).await;
    // An epic the source withdrew: tombstoned, so no name.
    let gone = item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-400",
        serde_json::json!({ "key": "PAY-400" }),
    )
    .await;
    tombstone(&pool, &gone).await;
    item(
        &pool,
        SOURCE,
        "ticket",
        "PAY-234",
        child("PAY-400", 1.into()),
    )
    .await;

    let board = mini_board::read_scoped(
        &pool,
        mini_board::Scope::default(),
        &ticket_declarations(),
        None,
    )
    .await
    .unwrap();
    let card = |key: &str| {
        board
            .columns
            .iter()
            .flat_map(|column| &column.cards)
            .find(|card| card.key == key)
            .unwrap()
            .clone()
    };

    let named = card("PAY-231");
    assert_eq!(named.comment_count, Some(3));
    assert_eq!(named.path.as_deref(), Some("PAY-200"));
    assert_eq!(named.parent_title.as_deref(), Some("Payout reliability"));

    let unmirrored = card("PAY-232");
    assert_eq!(
        unmirrored.comment_count,
        Some(0),
        "zero is an answer, not a miss"
    );
    assert_eq!(unmirrored.path.as_deref(), Some("PAY-999"));
    assert_eq!(
        unmirrored.parent_title, None,
        "an epic the mirror lacks has no name"
    );

    let unanswered = card("PAY-233");
    assert_eq!(
        unanswered.comment_count, None,
        "a count written as text is a miss"
    );
    assert_eq!(
        unanswered.parent_title, None,
        "an item that does not answer to the name is not the epic"
    );

    assert_eq!(
        card("PAY-234").parent_title,
        None,
        "a withdrawn epic has no name"
    );

    let bare = card("PAY-1");
    assert_eq!(bare.comment_count, None);
    assert_eq!(bare.parent_title, None);
    assert_eq!(
        card("PAY-200").parent_title,
        None,
        "the epic itself names no parent"
    );
}

/// `only` narrows the board to the tickets a search matched (#810), inside
/// whatever else the scope narrows by, and every column counts only those.
#[tokio::test]
async fn a_board_narrowed_to_matches_draws_and_counts_only_them() {
    let pool = scratch().await;
    let a = ticket(&pool, "PAY-1", Some("To Do"), None).await;
    ticket(&pool, "PAY-2", Some("To Do"), None).await;
    let c = ticket(&pool, "PAY-3", Some("Done"), None).await;
    ticket(&pool, "PAY-4", Some("In Review"), None).await;

    let only = vec![a, c];
    let board = mini_board::read_scoped(
        &pool,
        mini_board::Scope {
            only: Some(&only),
            ..mini_board::Scope::default()
        },
        &declarations(),
        None,
    )
    .await
    .unwrap();

    assert_eq!(
        columns(&board),
        vec![
            (Some("To Do"), vec!["PAY-1"]),
            (Some("Done"), vec!["PAY-3"])
        ]
    );
    assert!(board.columns.iter().all(|column| column.total == 1));

    let none: Vec<String> = Vec::new();
    let empty = mini_board::read_scoped(
        &pool,
        mini_board::Scope {
            only: Some(&none),
            ..mini_board::Scope::default()
        },
        &declarations(),
        None,
    )
    .await
    .unwrap();
    assert!(
        empty.columns.is_empty(),
        "no match is an empty board, not the whole one"
    );
}

/// A ticket assigned to `who` (or nobody), in `status`, changed at `at`.
async fn assigned(pool: &PgPool, key: &str, status: &str, who: Option<&str>, at: &str) {
    let mut fields = serde_json::json!({ "status": { "name": status } });
    if let Some(who) = who {
        fields["assignee"] = serde_json::json!({ "name": who });
    }
    let id = item(
        pool,
        SOURCE,
        "ticket",
        key,
        serde_json::json!({ "fields": fields }),
    )
    .await;
    touched(pool, &id, at).await;
}

/// One lane as `(key, count, [(status, ids' keys, total)])`.
type LaneShape = (String, u32, Vec<(Option<String>, Vec<String>, u32)>);

fn lanes(board: &MiniBoard) -> Vec<LaneShape> {
    board
        .lanes
        .iter()
        .map(|lane| {
            (
                lane.key.clone(),
                lane.count,
                lane.columns
                    .iter()
                    .map(|cell| {
                        let keys = cell
                            .ids
                            .iter()
                            .map(|id| id.trim_start_matches("jira:").to_owned())
                            .collect();
                        (cell.status.clone(), keys, cell.total)
                    })
                    .collect(),
            )
        })
        .collect()
}

/// The fixture the lane tests share: Mara has three to do and one done,
/// Omar one to do, and one ticket in progress is nobody's.
async fn three_lanes(pool: &PgPool) {
    for (key, status, who, at) in [
        ("PAY-1", "To Do", Some("mara"), "2026-10-01T09:00:00Z"),
        ("PAY-2", "To Do", Some("mara"), "2026-10-02T09:00:00Z"),
        ("PAY-3", "To Do", Some("mara"), "2026-10-03T09:00:00Z"),
        ("PAY-4", "Done", Some("mara"), "2026-10-04T09:00:00Z"),
        ("PAY-5", "To Do", Some("omar"), "2026-10-05T09:00:00Z"),
        ("PAY-6", "In Progress", None, "2026-10-06T09:00:00Z"),
    ] {
        assigned(pool, key, status, who, at).await;
    }
}

fn by_assignee(first: Option<u32>) -> Option<mini_board::Lanes> {
    Some(mini_board::Lanes {
        by: mini_board::LaneField::Assignee,
        first,
    })
}

/// **A laned board is grouped and capped on the server** (#939, #1098):
/// each lane x status cell keeps its newest `per_column` and counts them
/// all, a lane counts its tickets, and lanes come biggest first, a tie
/// going to the lane with the newer ticket. The board's own columns still
/// count every ticket and hold the cards the lanes keep. Mara's to-do cell
/// of three under a cap of two is what a cap per lane rather than per cell,
/// or a count taken after the cut, would get wrong.
#[tokio::test]
async fn a_laned_board_caps_each_lanes_cell_and_counts_them_all() {
    let pool = scratch().await;
    three_lanes(&pool).await;

    let board = mini_board::read_scoped(
        &pool,
        mini_board::Scope {
            lanes: by_assignee(None),
            ..mini_board::Scope::default()
        },
        &ticket_declarations(),
        Some(2),
    )
    .await
    .unwrap();

    let s = |status: &str| Some(status.to_owned());
    let k = |keys: &[&str]| keys.iter().map(|key| (*key).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        lanes(&board),
        vec![
            (
                "mara".to_owned(),
                4,
                vec![
                    (s("To Do"), k(&["PAY-3", "PAY-2"]), 3),
                    (s("Done"), k(&["PAY-4"]), 1)
                ]
            ),
            (String::new(), 1, vec![(s("In Progress"), k(&["PAY-6"]), 1)]),
            ("omar".to_owned(), 1, vec![(s("To Do"), k(&["PAY-5"]), 1)]),
        ]
    );
    assert_eq!(board.more_lanes, 0);
    assert_eq!(
        columns(&board),
        vec![
            (Some("To Do"), vec!["PAY-2", "PAY-3", "PAY-5"]),
            (Some("In Progress"), vec!["PAY-6"]),
            (Some("Done"), vec!["PAY-4"]),
        ]
    );
    let totals: Vec<u32> = board.columns.iter().map(|column| column.total).collect();
    assert_eq!(
        totals,
        vec![4, 1, 1],
        "the board's columns count every ticket"
    );
}

/// Only the first lanes carry cards (#939): a board laned by epic on a
/// large mirror is hundreds of small lanes, which a cap per cell does not
/// cut. The lanes past `first` are not sent; `more_lanes` says how many.
#[tokio::test]
async fn a_laned_board_sends_only_its_first_lanes_and_counts_the_rest() {
    let pool = scratch().await;
    three_lanes(&pool).await;

    let board = mini_board::read_scoped(
        &pool,
        mini_board::Scope {
            lanes: by_assignee(Some(1)),
            ..mini_board::Scope::default()
        },
        &ticket_declarations(),
        None,
    )
    .await
    .unwrap();

    let keys: Vec<(String, u32)> = lanes(&board)
        .into_iter()
        .map(|(key, count, _)| (key, count))
        .collect();
    assert_eq!(keys, vec![("mara".to_owned(), 4)]);
    assert_eq!(board.more_lanes, 2);
    assert_eq!(
        columns(&board),
        vec![
            (Some("To Do"), vec!["PAY-1", "PAY-2", "PAY-3"]),
            (Some("In Progress"), vec![]),
            (Some("Done"), vec!["PAY-4"]),
        ],
        "every column stays, counted whole, with the first lanes' cards"
    );
}

/// A board without lanes carries none.
#[tokio::test]
async fn a_board_without_lanes_carries_none() {
    let pool = scratch().await;
    three_lanes(&pool).await;
    let board = mini_board::read_capped(&pool, None, &[], None, &ticket_declarations(), Some(2))
        .await
        .unwrap();
    assert!(board.lanes.is_empty());
    assert_eq!(board.more_lanes, 0);
}
