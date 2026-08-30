//! The mini board's granted read (#177), against a real PostgreSQL.
//!
//! The membership *rule* this read stands on is proven in `knobas-core`'s own
//! battery (`crates/knobas-core/tests/contexts.rs`); what is asserted here is
//! the command seam -- what a caller of `mini_board` gets back over a seeded
//! corpus: which tickets are on the board, which column each lands in, what
//! order the columns come in, and what the two payload reads do when they
//! miss.
//!
//! Every test shares one database (`test_util`), so fixtures carry ids unique
//! per run and every assertion is scoped to this test's own source names.

use std::collections::BTreeMap;

use knobas_app::commands::entity::{create_context_inner, create_link_inner, mini_board_inner};
use knobas_core::mini_board::MiniBoard;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A token no other test in this binary writes.
fn unique() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!("{nanos}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
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
    let id = format!("{source}:{key}");
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

/// A live ticket carrying a Jira-shaped status and priority.
async fn ticket(
    pool: &PgPool,
    source: &str,
    key: &str,
    status: Option<&str>,
    priority: Option<&str>,
) -> String {
    item(pool, source, "ticket", key, jira(status, priority)).await
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

/// An ad-hoc context with each entity explicitly added to it.
async fn room(pool: &PgPool, members: &[&str]) -> String {
    let ctx = create_context_inner(pool, &format!("room {}", unique()))
        .await
        .unwrap();
    for member in members {
        create_link_inner(pool, &ctx.id, member, None, None)
            .await
            .unwrap();
    }
    ctx.id
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
    let pool = pool().await;
    let source = format!("mbord-{}", unique());
    let members = [
        ticket(&pool, &source, "PAY-1", Some("Done"), None).await,
        ticket(&pool, &source, "PAY-2", Some("To Do"), None).await,
        ticket(&pool, &source, "PAY-3", Some("Blocked"), None).await,
        ticket(&pool, &source, "PAY-4", Some("In Review"), None).await,
        ticket(&pool, &source, "PAY-5", None, None).await,
        ticket(&pool, &source, "PAY-6", Some("Awaiting deploy"), None).await,
        ticket(&pool, &source, "PAY-7", Some("In Progress"), None).await,
    ];
    let refs: Vec<&str> = members.iter().map(String::as_str).collect();
    let ctx = room(&pool, &refs).await;

    let board = mini_board_inner(&pool, &ctx).await.unwrap();

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
        offered(&board, &source),
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
    let pool = pool().await;
    let source = format!("mbrecent-{}", unique());
    let old = ticket(&pool, &source, "PAY-70", Some("To Do"), None).await;
    let new = ticket(&pool, &source, "PAY-71", Some("To Do"), None).await;
    let middle = ticket(&pool, &source, "PAY-72", Some("To Do"), None).await;
    touched(&pool, &old, "2026-08-01T09:00:00Z").await;
    touched(&pool, &new, "2026-08-30T09:00:00Z").await;
    touched(&pool, &middle, "2026-08-15T09:00:00Z").await;
    let ctx = room(&pool, &[&old, &new, &middle]).await;

    let board = mini_board_inner(&pool, &ctx).await.unwrap();

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
    let pool = pool().await;
    let source = format!("mbmiss-{}", unique());
    let absent = ticket(&pool, &source, "PAY-10", None, None).await;
    // The path exists but does not lead to a string -- a source that spells its
    // status some other way, which this read must not stringify into a column.
    let misshapen = item(
        &pool,
        &source,
        "ticket",
        "PAY-11",
        serde_json::json!({ "fields": { "status": { "name": { "id": 3 } } } }),
    )
    .await;
    let blank = ticket(&pool, &source, "PAY-12", Some("   "), None).await;
    let ctx = room(&pool, &[&absent, &misshapen, &blank]).await;

    let board = mini_board_inner(&pool, &ctx).await.unwrap();

    assert_eq!(
        columns(&board),
        vec![(None, vec!["PAY-10", "PAY-11", "PAY-12"])],
        "every unreadable status belongs in the one terminal group"
    );
    assert!(
        offered(&board, &source).is_empty(),
        "the terminal group is not a status anything can be moved to"
    );
}

/// Miss direction two, pinned: an unreadable priority renders nothing at all.
#[tokio::test]
async fn a_ticket_with_no_recognizable_priority_carries_none() {
    let pool = pool().await;
    let source = format!("mbprio-{}", unique());
    let high = ticket(&pool, &source, "PAY-20", Some("To Do"), Some("High")).await;
    let absent = ticket(&pool, &source, "PAY-21", Some("To Do"), None).await;
    let misshapen = item(
        &pool,
        &source,
        "ticket",
        "PAY-22",
        serde_json::json!({ "fields": { "status": { "name": "To Do" }, "priority": [] } }),
    )
    .await;
    let ctx = room(&pool, &[&high, &absent, &misshapen]).await;

    let board = mini_board_inner(&pool, &ctx).await.unwrap();

    let priorities: BTreeMap<&str, Option<&str>> = board.columns[0]
        .cards
        .iter()
        .map(|card| (card.key.as_str(), card.priority.as_deref()))
        .collect();
    assert_eq!(
        priorities,
        BTreeMap::from([("PAY-20", Some("High")), ("PAY-21", None), ("PAY-22", None),])
    );
}

/// A ticket the source deleted leaves the board, though the rule still counts
/// it a member: the board is drawn from the mirror's live view.
#[tokio::test]
async fn a_tombstoned_ticket_is_off_the_board() {
    let pool = pool().await;
    let source = format!("mbtomb-{}", unique());
    let live = ticket(&pool, &source, "PAY-30", Some("To Do"), None).await;
    let gone = ticket(&pool, &source, "PAY-31", Some("Done"), None).await;
    let ctx = room(&pool, &[&live, &gone]).await;
    tombstone(&pool, &gone).await;

    let board = mini_board_inner(&pool, &ctx).await.unwrap();

    assert_eq!(columns(&board), vec![(Some("To Do"), vec!["PAY-30"])]);
    assert_eq!(
        offered(&board, &source),
        ["To Do"],
        "a tombstoned ticket's status is not one the source still shows"
    );
}

/// The board is the room's, and it is tickets: a ticket nobody added and a
/// member of another kind are both absent.
#[tokio::test]
async fn only_the_contexts_live_ticket_members_are_on_the_board() {
    let pool = pool().await;
    let source = format!("mbscope-{}", unique());
    let member = ticket(&pool, &source, "PAY-40", Some("To Do"), None).await;
    let pr = item(
        &pool,
        &source,
        "pr",
        "payout#1",
        serde_json::json!({ "fields": { "status": { "name": "In Review" } } }),
    )
    .await;
    ticket(&pool, &source, "PAY-41", Some("In Progress"), None).await;
    let ctx = room(&pool, &[&member, &pr]).await;

    let board = mini_board_inner(&pool, &ctx).await.unwrap();

    assert_eq!(columns(&board), vec![(Some("To Do"), vec!["PAY-40"])]);
}

/// One context's work reads as one board: two sources spelling a status the
/// same way share its column, and each card still says which source it is from.
#[tokio::test]
async fn identically_spelled_statuses_from_two_sources_share_a_column() {
    let pool = pool().await;
    let left = format!("mbleft-{}", unique());
    let right = format!("mbright-{}", unique());
    let a = ticket(&pool, &left, "PAY-50", Some("In Progress"), None).await;
    let b = ticket(&pool, &right, "OPS-50", Some("In Progress"), None).await;
    let ctx = room(&pool, &[&a, &b]).await;

    let board = mini_board_inner(&pool, &ctx).await.unwrap();

    assert_eq!(
        columns(&board),
        vec![(Some("In Progress"), vec!["OPS-50", "PAY-50"])],
        "one status, one column, whichever source spelled it"
    );
    let sources: Vec<&str> = board.sources.iter().map(|s| s.source_id.as_str()).collect();
    assert_eq!(
        sources
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>(),
        [left.as_str(), right.as_str()].into_iter().collect(),
        "and the board answers for both sources it drew from"
    );
}

/// The second consumer (#179): the statuses a source's own corpus shows, which
/// is wider than the room's columns and never includes the terminal group.
#[tokio::test]
async fn the_offered_statuses_are_the_sources_corpus_not_the_rooms_columns() {
    let pool = pool().await;
    let source = format!("mboffer-{}", unique());
    let other = format!("mbother-{}", unique());
    let member = ticket(&pool, &source, "PAY-60", Some("To Do"), None).await;
    // Same source, not in the room, carrying a status the room never shows --
    // the move the select has to be able to offer.
    ticket(&pool, &source, "PAY-61", Some("Done"), None).await;
    // Same source, no readable status: not something a ticket can be moved to.
    ticket(&pool, &source, "PAY-62", None, None).await;
    // A different source entirely: its statuses are not this one's.
    ticket(&pool, &other, "OPS-60", Some("Blocked"), None).await;
    let ctx = room(&pool, &[&member]).await;

    let board = mini_board_inner(&pool, &ctx).await.unwrap();

    assert_eq!(
        offered(&board, &source),
        ["To Do", "Done"],
        "the source's own statuses, in the board's order, with no terminal group"
    );
    assert!(
        offered(&board, &other).is_empty(),
        "a source with no card on this board is not one the board answers for"
    );
}

/// An address can outlive the room it names, and a room can be empty; both
/// answer with an empty board rather than a failure, so the tile can tell
/// "nothing here" from "this broke".
#[tokio::test]
async fn an_unknown_or_empty_context_is_an_empty_board() {
    let pool = pool().await;
    let empty = room(&pool, &[]).await;

    for ctx in [empty.as_str(), "ctx:nothing-here"] {
        let board = mini_board_inner(&pool, ctx).await.unwrap();
        assert!(board.columns.is_empty(), "{ctx} has no column");
        assert!(board.sources.is_empty(), "{ctx} has no source");
    }
}
