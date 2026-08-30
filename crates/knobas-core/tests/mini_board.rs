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
//! fixtures that read as `PAY-231` are worth one `create database` each.

use std::collections::BTreeMap;

use knobas_core::entity::EntityRef;
use knobas_core::link::Origin;
use knobas_core::mini_board::{self, MiniBoard};
use knobas_core::{context, link};
use sqlx::PgPool;

/// The source most fixtures below are synced under.
const SOURCE: &str = "jira";

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
    mini_board::read(pool, Some(ctx), &[]).await.unwrap()
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

    let everything = mini_board::read(&pool, None, &[]).await.unwrap();
    assert_eq!(
        columns(&everything),
        vec![
            (Some("To Do"), vec!["PAY-90"]),
            (Some("In Progress"), vec!["OPS-90"]),
        ],
        "*All work* narrows by nothing at all"
    );

    let one_source = mini_board::read(&pool, None, &["gitea".to_owned()])
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
