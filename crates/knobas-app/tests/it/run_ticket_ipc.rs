//! *Create ticket* on a failed run (#1451) after a reload (#1953): the run
//! page's read of the tickets filed from the run that it does not link yet
//! (`owed_tickets`). A create's row holds the run as its `link_to` until the
//! ticket is mirrored and linked, so the read is the queue's, not the page's
//! session, and a reload before the sync draws *Ticket queued* rather than
//! offering *Create ticket* again.
//!
//! Over a real mirror, with the queue rows written the way the flush writes
//! them (`knobas_core::write_queue`), as `link_on_arrival_ipc` writes them.

use knobas_app::commands::entity::owed_tickets_inner;
use knobas_app::sources::write_queue::hold_arrival_link;
use knobas_core::entity::EntityRef;
use knobas_core::payload::Declarations;
use knobas_core::write_queue::WriteState;
use sqlx::PgPool;

const RUN: &str = "teamcity:build:Payout_IntegrationTests-1187";
const OTHER_RUN: &str = "teamcity:build:Payout_IntegrationTests-1188";
const NEW_TICKET: &str = "jira:PAY-342";
const FAILED_TITLE: &str = "sepa_retry_backs_off fails in Payout_IntegrationTests #1187";

async fn corpus(name: &str) -> PgPool {
    let pool = knobas_db::test_util::scratch_database(name)
        .await
        .pool(4)
        .await
        .expect("a pool onto this test's own database");
    item(&pool, RUN, "build", "Payout_IntegrationTests #1187").await;
    item(&pool, OTHER_RUN, "build", "Payout_IntegrationTests #1188").await;
    pool
}

/// One live mirror item.
async fn item(pool: &PgPool, id: &str, kind: &str, title: &str) {
    let entity = EntityRef::parse(id).expect("an entity id");
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,$2,$3,$4,'','{}'::jsonb)",
    )
    .bind(id)
    .bind(&entity.namespace)
    .bind(kind)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
}

/// A `CreateTicket` filed from `run`, as `submit_write` with `link_to` leaves it.
async fn filed_from(pool: &PgPool, run: &str) -> i64 {
    let id = knobas_core::write_queue::queue(
        pool,
        "jira",
        &EntityRef::parse("jira:PAY").unwrap(),
        "create_ticket",
        serde_json::json!({ "CreateTicket": {
            "entity": "jira:PAY", "title": FAILED_TITLE, "body": "assertion failed", "ticket_type": "Task"
        }}),
        serde_json::json!({}),
    )
    .await
    .unwrap()
    .id;
    hold_arrival_link(pool, id, run).await.unwrap();
    id
}

/// The owed tickets' ids and states.
async fn owed(pool: &PgPool, run: &str) -> Vec<(i64, WriteState)> {
    owed_tickets_inner(pool, &Declarations::empty(), run)
        .await
        .unwrap()
        .into_iter()
        .map(|write| (write.id, write.state))
        .collect()
}

/// The tickets linked to the run, either end.
async fn linked(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "select case when from_id = $1 then to_id else from_id end from knobas.link
          where $1 in (from_id, to_id) and deleted_at is null order by 1",
    )
    .bind(RUN)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The issue's criterion: a ticket filed from the run is named while it is
/// queued and once it was sent, until a sync mirrors it. The read that finds
/// it mirrored links it, and names it no more.
#[tokio::test]
async fn a_ticket_filed_from_a_run_is_named_until_its_ticket_is_mirrored_and_linked() {
    let pool = corpus("owed_run_ticket").await;
    let id = filed_from(&pool, RUN).await;
    assert_eq!(owed(&pool, RUN).await, vec![(id, WriteState::Pending)], "queued");

    knobas_core::write_queue::sent(&pool, id, Some("PAY-342"))
        .await
        .unwrap();
    assert_eq!(
        owed(&pool, RUN).await,
        vec![(id, WriteState::Sent)],
        "sent, and the ticket is not mirrored yet"
    );
    assert!(linked(&pool).await.is_empty());

    item(&pool, NEW_TICKET, "ticket", FAILED_TITLE).await;
    assert!(
        owed(&pool, RUN).await.is_empty(),
        "the read found the ticket and linked it"
    );
    assert_eq!(linked(&pool).await, vec![NEW_TICKET.to_owned()]);
    let still: Option<String> =
        sqlx::query_scalar("select link_to from knobas.write_queue where id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        still, None,
        "drawn, so owed no more: a link the reader removes is not drawn again"
    );
}

/// Only the run's own creates, and none withdrawn. A held or refused one is
/// named with its state: the page decides how it draws it.
#[tokio::test]
async fn a_withdrawn_create_or_one_filed_from_another_run_is_not_named() {
    let pool = corpus("owed_run_ticket_scope").await;
    let withdrawn = filed_from(&pool, RUN).await;
    knobas_core::write_queue::discard(&pool, withdrawn)
        .await
        .unwrap();
    let elsewhere = filed_from(&pool, OTHER_RUN).await;
    let refused = filed_from(&pool, RUN).await;
    knobas_core::write_queue::refuse(&pool, refused, "summary is required")
        .await
        .unwrap();

    assert_eq!(owed(&pool, RUN).await, vec![(refused, WriteState::Refused)]);
    assert_eq!(
        owed(&pool, OTHER_RUN).await,
        vec![(elsewhere, WriteState::Pending)]
    );
}
