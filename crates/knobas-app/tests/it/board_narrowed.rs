//! A filter typed into Tickets narrows the board's own scan (the broad-keys
//! pass of PR #2479, 2026-11-13): the search's predicates go into the board's
//! statement (`knobas_search::Narrowing`, a
//! `knobas_core::mini_board::CardsNarrowing`), where the filter was two
//! statements, the search's ids and then the board of them.
//!
//! Held here to those two statements, which are still in the tree: the ids
//! ([`knobas_search::Narrowing::ids`]) and the board of exactly those tickets
//! ([`knobas_core::mini_board::Scope::only`]). Every word and key the grammar
//! has, each alone, left out and beside another, on a board with lanes and
//! without, cut and whole, narrowed to some ids too, with every ticket's card
//! stored, a third gone and none, under the declarations the database stored
//! and under ones it never stored (where every key and field walks the
//! payload), and under declarations that are not the search's, where the
//! narrowing reads its ids first.

use knobas_app::sources::Registry;
use knobas_core::mini_board::{LaneField, Lanes, Scope};
use knobas_core::payload::Declarations;
use knobas_search::{Admits, Narrowing, Vocabulary};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::item_card::{gitea_tickets, jira_tickets, mock_tickets, pool, source, upsert};

/// Every query asked: words that each reach another field (the key, the
/// title, the assignee, a parent's name), several words, a phrase, a word left
/// out, alternatives, a word nothing holds, a letter most tickets hold; each
/// key alone, left out and repeated; keys beside each other and beside words;
/// and the dimensions that are not ticket keys.
const QUERIES: &[&str] = &[
    "pay-1",
    "payout",
    "mara",
    "jon",
    "pay-0",
    "o/r/issues/9",
    "m-0",
    "mara payout",
    "\"retry the\"",
    "pay -mara",
    "mara or jonas",
    "zzqx",
    "a",
    "e",
    "-a",
    "ticket -1",
    "priority:high",
    "-priority:high",
    "priority:high priority:p1",
    "issuetype:story issuetype:epic",
    "-issuetype:story",
    "label:backend",
    "label:kind/bug label:payouts",
    "-label:backend",
    "status:\"to do\"",
    "status:open status:\"in progress\"",
    "-status:open",
    "-status:done",
    "assignee:mara",
    "-assignee:jonas",
    "assignee:@me",
    "-assignee:@me",
    "assigned:week",
    "-assigned:week",
    "sprint:current",
    "-sprint:current",
    "project:PAY",
    "-project:PAY",
    "priority:high pay",
    "-priority:high -status:done ticket",
    "status:\"to do\" label:backend mara",
    "-sprint:current -label:payouts -assignee:mara",
    "under:PAY-0",
    "under:PAY-0 retry",
    "reach:PAY-1",
    "updated:7d",
    "updated:7d -priority:high",
    "by:nobody",
    "name:PAY-1*",
    "has:blocker",
    "-has:blocker ticket",
    "type:ticket",
    "type:ticket pay",
];

/// Twelve more Jira tickets: two statuses, three priorities, a label on
/// every other one, an assignee on two in three, and in sprints active or
/// not, so each key keeps some and leaves some.
fn more_jira() -> Vec<(String, String, Value)> {
    (10..22)
        .map(|n| {
            let key = format!("PAY-{n}");
            let status = if n % 3 == 0 { "Done" } else { "To Do" };
            let priority = ["High", "Low", "Medium"][n % 3];
            let mut fields = json!({
                "status": { "name": status },
                "priority": { "name": priority },
                "issuetype": { "name": if n % 2 == 0 { "Story" } else { "Bug" } },
                "labels": if n % 2 == 0 { json!(["backend"]) } else { json!([]) },
            });
            if n % 3 != 1 {
                fields["assignee"] = json!({ "name": if n % 2 == 0 { "mara" } else { "jonas" } });
            }
            if n % 4 == 0 {
                fields["parent"] = json!({ "key": "PAY-0" });
            }
            let payload = json!({
                "key": key,
                "fields": fields,
                "sprints": if n % 5 == 0 { json!([{ "state": "active" }]) } else { json!([]) },
            });
            (key.clone(), format!("Ticket {n} payout"), payload)
        })
        .collect()
}

/// The mirror every read below is asked of: three sources' tickets (Jira,
/// Gitea, the mock adapter), dates that tie, one ticket withdrawn, and a
/// fourth source, switched off, whose ticket every word here matches.
/// `(sources, ids)`, the ids of every ticket written.
async fn mirror(pool: &PgPool) -> (Vec<String>, Vec<String>) {
    let jira = source(pool, "jira").await;
    let gitea = source(pool, "gitea").await;
    let mock = source(pool, "mock").await;
    let off = source(pool, "jira").await;
    upsert(pool, &jira, &jira_tickets()).await;
    upsert(pool, &gitea, &gitea_tickets()).await;
    upsert(pool, &mock, &mock_tickets()).await;
    let more = more_jira();
    let rows: Vec<(&str, &str, Value)> = more
        .iter()
        .map(|(k, t, p)| (k.as_str(), t.as_str(), p.clone()))
        .collect();
    upsert(pool, &jira, &rows).await;
    upsert(
        pool,
        &off,
        &[(
            "PAY-1",
            "Retry the payout, mara",
            json!({ "key": "PAY-1", "fields": { "status": { "name": "To Do" },
                "priority": { "name": "High" }, "assignee": { "name": "mara" } } }),
        )],
    )
    .await;
    // Dated in pairs: ties the cut must break by id. Two of them older than
    // a week, for `updated:` and `assigned:`.
    sqlx::query(
        "update sync.item
            set item_updated_at = now() - ((right(entity_id, 2)::int / 2) || ' hours')::interval
                                  - case when right(entity_id, 2)::int > 19
                                         then interval '30 days' else interval '0' end
          where source_id = $1 and entity_id ~ 'PAY-1[0-9]$|PAY-2[01]$'",
    )
    .bind(&jira)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(format!("{jira}:PAY-11"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("update knobas.source_config set enabled = false where id = $1")
        .bind(&off)
        .execute(pool)
        .await
        .unwrap();
    // The reader is `mara` on the Jira source, so `@me` is somebody.
    sqlx::query("update knobas.source_config set config = $2 where id = $1")
        .bind(&jira)
        .bind(json!({ "username": "MARA" }))
        .execute(pool)
        .await
        .unwrap();
    let sources = vec![jira, gitea, mock, off];
    let ids: Vec<String> = sqlx::query_scalar(
        "select entity_id from sync.item where source_id = any($1) order by entity_id",
    )
    .bind(&sources)
    .fetch_all(pool)
    .await
    .unwrap();
    (sources, ids)
}

/// `stored` with one more candidate for each field the database stores of
/// `source`'s tickets and each its card is keyed by: the same values resolve,
/// under candidates no stored row and no card was written by, so every key
/// and every card field walks the payload.
fn never_stored(stored: &Declarations, source: &str) -> Declarations {
    let extra = knobas_core::payload::PayloadPath::of(["fields", "never-sent"]);
    let kinds = stored
        .of_source(source)
        .cloned()
        .map(|mut kind| {
            for paths in [
                &mut kind.status_name,
                &mut kind.assignee,
                &mut kind.project_key,
                &mut kind.project_name,
                &mut kind.priority,
            ] {
                if !paths.is_empty() {
                    paths.push(extra.clone());
                }
            }
            kind
        })
        .collect();
    stored.clone().with(source, kinds)
}

/// The boards the `n`th query is asked for, each `(lanes, cut, narrowed to
/// some ids too)`: without lanes and cut, whole and narrowed, and in lanes
/// by a field that turns with the query, so that across the queries every
/// kind of lane is asked cut and whole, all its lanes and its first two.
fn shapes(n: usize) -> [(Option<Lanes>, Option<u32>, bool); 3] {
    let by = [
        LaneField::Epic,
        LaneField::Assignee,
        LaneField::Priority,
        LaneField::IssueType,
        LaneField::Label,
    ][n % 5];
    let first = n.is_multiple_of(2).then_some(2);
    let cut = (!n.is_multiple_of(3)).then_some(1);
    [
        (None, Some(3), false),
        (None, None, true),
        (Some(Lanes { by, first }), cut, n.is_multiple_of(4)),
    ]
}

/// The narrowing `raw` asks of the tickets under `vocab`.
fn narrowing(vocab: &Vocabulary, raw: &str) -> Narrowing {
    match knobas_search::Searcher::admits_in(vocab, raw, "ticket").unwrap() {
        Admits::Some(narrowing) => *narrowing,
        other => panic!("{raw}: narrows the tickets, not {other:?}"),
    }
}

/// The boards of the `n`th query, `raw`, read with the narrowing in the
/// board's scan and from the ids, the two held equal. `board` are the
/// declarations the board binds. How many boards drew a card.
#[allow(clippy::too_many_arguments)]
async fn hold(
    pool: &PgPool,
    sources: &[String],
    ids: &[String],
    vocab: &Vocabulary,
    board: &Declarations,
    n: usize,
    raw: &str,
    when: &str,
) -> usize {
    let narrowing = narrowing(vocab, raw);
    let identity = vec!["mara".to_owned()];
    let some: Vec<String> = ids.iter().step_by(2).cloned().collect();
    let mut matched = narrowing.ids(pool).await.unwrap();
    matched.retain(|id| ids.contains(id));
    let mut drew = 0;
    for (lanes, per_column, narrowed) in shapes(n) {
        let only = narrowed.then_some(&some);
        // What `Scope::only` of the ids admits: the tickets the query keeps,
        // and of them the ones the scope's own ids keep.
        let admitted: Vec<String> = matched
            .iter()
            .filter(|id| only.is_none_or(|only| only.contains(id)))
            .cloned()
            .collect();
        let scope = |only| Scope {
            sources,
            only,
            identity: &identity,
            lanes,
            ..Scope::default()
        };
        let in_scan = knobas_core::mini_board::read_scoped_narrowed(
            pool,
            scope(only.map(Vec::as_slice)),
            board,
            per_column,
            &narrowing,
        )
        .await
        .unwrap_or_else(|e| panic!("{when}: {raw}: {e}"));
        let of_ids =
            knobas_core::mini_board::read_scoped(pool, scope(Some(&admitted)), board, per_column)
                .await
                .unwrap();
        assert_eq!(
            serde_json::to_string(&in_scan).unwrap(),
            serde_json::to_string(&of_ids).unwrap(),
            "{when}: {raw:?} in lanes {lanes:?}, cut {per_column:?}, narrowed {narrowed}: \
             the board narrowed in its scan is the board of the ids"
        );
        drew += usize::from(in_scan.columns.iter().any(|c| !c.cards.is_empty()));
    }
    drew
}

/// Every query's every board, under the declarations the search was parsed
/// with: the narrowing is in the board's scan.
async fn hold_every(
    pool: &PgPool,
    sources: &[String],
    ids: &[String],
    vocab: &Vocabulary,
    when: &str,
) {
    let mut drew = 0;
    for (n, raw) in QUERIES.iter().enumerate() {
        assert!(
            narrowing(vocab, raw).in_scan_under(&vocab.declarations),
            "{raw}: read in the board's scan"
        );
        drew += hold(pool, sources, ids, vocab, &vocab.declarations, n, raw, when).await;
    }
    // A query that kept nothing would hold trivially: most keep a card.
    let asked = QUERIES.len() * 3;
    assert!(
        drew * 2 > asked,
        "{when}: {drew} of {asked} boards drew a card"
    );
}

async fn vocabulary(pool: &PgPool) -> Vocabulary {
    knobas_app::commands::search::searcher(pool, &Registry::for_process())
        .await
        .unwrap()
        .vocabulary()
        .await
        .unwrap()
}

#[tokio::test]
async fn a_board_narrowed_in_its_scan_is_the_board_of_the_ids() {
    let pool = pool().await;
    let (sources, ids) = mirror(&pool).await;
    let vocab = vocabulary(&pool).await;
    assert!(
        vocab.identity.iter().any(|me| me == "MARA"),
        "the reader is somebody: {:?}",
        vocab.identity
    );
    let cards = async |pool: &PgPool| -> i64 {
        sqlx::query_scalar("select count(*) from sync.item_card where entity_id = any($1)")
            .bind(&ids)
            .fetch_one(pool)
            .await
            .unwrap()
    };
    assert_eq!(
        cards(&pool).await,
        ids.len() as i64,
        "every ticket written has its card"
    );
    hold_every(&pool, &sources, &ids, &vocab, "every card stored").await;

    let half: Vec<&String> = ids.iter().step_by(3).collect();
    sqlx::query("delete from sync.item_card where entity_id = any($1)")
        .bind(&half)
        .execute(&pool)
        .await
        .unwrap();
    hold_every(&pool, &sources, &ids, &vocab, "a third of the cards gone").await;

    sqlx::query("delete from sync.item_card where entity_id = any($1)")
        .bind(&ids)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(cards(&pool).await, 0);
    hold_every(&pool, &sources, &ids, &vocab, "no card stored").await;
}

/// Under declarations the database never stored, each stored key and each
/// field of a card walks the payload, in the board's scan as in the ids.
#[tokio::test]
async fn a_board_narrowed_under_declarations_never_stored_walks_as_the_ids_do() {
    let pool = pool().await;
    let (sources, ids) = mirror(&pool).await;
    let stored = vocabulary(&pool).await;
    let vocab = Vocabulary {
        declarations: never_stored(&stored.declarations, &sources[0]),
        ..stored.clone()
    };
    assert_ne!(
        vocab.declarations.as_param().0,
        stored.declarations.as_param().0
    );
    hold_every(&pool, &sources, &ids, &vocab, "declarations never stored").await;
}

/// A board bound to other declarations than the search's reads the ids first
/// and its own statement over them: the search's words and keys read a card
/// and stored fields under its declarations, and in the board's scan they
/// would read the board's.
#[tokio::test]
async fn a_board_bound_to_other_declarations_reads_the_ids_first() {
    let pool = pool().await;
    let (sources, ids) = mirror(&pool).await;
    let stored = vocabulary(&pool).await;
    let other = never_stored(&stored.declarations, &sources[0]);
    let unstored = Vocabulary {
        declarations: other.clone(),
        ..stored.clone()
    };
    let mut drew = 0;
    for (n, raw) in QUERIES.iter().enumerate() {
        // The search under the stored declarations and the board under the
        // others, and the other way round, turn about.
        let (vocab, board, when) = if n.is_multiple_of(2) {
            (&stored, &other, "the board's declarations never stored")
        } else {
            (&unstored, &stored.declarations, "the search's never stored")
        };
        assert!(
            !narrowing(vocab, raw).in_scan_under(board),
            "{raw}: the ids first"
        );
        drew += hold(&pool, &sources, &ids, vocab, board, n, raw, when).await;
    }
    assert!(drew > QUERIES.len(), "{drew} boards drew a card");
}

/// The app's board binds the declarations its search is parsed with, so the
/// Tickets view's filter is read in the board's scan, and the command answers
/// with the board of the ids.
#[tokio::test]
async fn the_command_reads_a_filter_in_the_boards_scan() {
    let pool = pool().await;
    let (sources, ids) = mirror(&pool).await;
    let registry = Registry::for_process();
    let vocab = vocabulary(&pool).await;
    let declared = crate::item_card::declarations(&pool).await;
    assert_eq!(
        declared.as_param().0,
        vocab.declarations.as_param().0,
        "the board's declarations are the search's"
    );
    let identity = vocab.identity.clone();
    for raw in [
        "payout",
        "-priority:high",
        "-project:PAY",
        "status:\"to do\" mara",
    ] {
        let narrowing = narrowing(&vocab, raw);
        assert!(narrowing.in_scan_under(&declared), "{raw}");
        let matched = narrowing.ids(&pool).await.unwrap();
        for per_column in [None, Some(2)] {
            let answered = knobas_app::commands::entity::mini_board_laned(
                &pool,
                &registry,
                knobas_app::commands::entity::Room::default(),
                per_column,
                Some(raw),
                None,
            )
            .await
            .unwrap();
            let of_ids = knobas_core::mini_board::read_scoped(
                &pool,
                Scope {
                    only: Some(&matched),
                    identity: &identity,
                    ..Scope::default()
                },
                &declared,
                per_column,
            )
            .await
            .unwrap();
            // Other tests write tickets of their own into this database
            // between the two reads; this test's are compared.
            let mine = |board: &knobas_core::mini_board::MiniBoard| -> Vec<String> {
                board
                    .columns
                    .iter()
                    .flat_map(|column| {
                        column
                            .cards
                            .iter()
                            .filter(|card| sources.contains(&card.source_id))
                            .map(|card| serde_json::to_string(card).unwrap())
                    })
                    .collect()
            };
            assert!(!mine(&answered).is_empty() || per_column.is_some(), "{raw}");
            if per_column.is_none() {
                assert_eq!(mine(&answered), mine(&of_ids), "{raw}");
                assert!(
                    mine(&answered).iter().all(|card| ids
                        .iter()
                        .any(|id| card.contains(&format!("\"entity_id\":\"{id}\"")))),
                    "{raw}"
                );
            }
        }
    }
    // A query that admits no ticket draws none, and none narrows nothing.
    let none = knobas_app::commands::entity::mini_board_laned(
        &pool,
        &registry,
        knobas_app::commands::entity::Room::default(),
        None,
        Some("type:page payout"),
        None,
    )
    .await
    .unwrap();
    assert!(none.columns.is_empty(), "a page is no ticket");
}
