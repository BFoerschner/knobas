//! *Create monitor* on *Assets nothing watches* (#864) once its create is
//! queued (#1959): the read names the create on its way on every asset and
//! route that records the monitor's name, until the mirror holds the monitor
//! or the write is held on drift, refused or withdrawn; one held as unconfirmed
//! stays on its way until the reader discards it (#1966). So a reload before the poll
//! draws *Monitor queued* rather than offering *Create monitor* again, and
//! Uptime Kuma, which takes a second monitor of the same name, is not sent one.
//!
//! Over a real mirror, with the queue rows written the way the flush writes
//! them (`knobas_core::write_queue`), as `run_ticket_ipc` writes them.

use knobas_app::assets::{self, AssetEdit, RouteEdit, UnmonitoredAsset, Visibility};
use knobas_core::entity::EntityRef;
use knobas_core::write_queue::WriteState;
use sqlx::PgPool;

async fn pool(label: &str) -> PgPool {
    knobas_db::test_util::scratch_database(label)
        .await
        .pool(2)
        .await
        .expect("a pool on the scratch database")
}

/// An asset named `name` that records `monitor` as the name that attaches its
/// monitor, as the dialog's first write leaves it.
async fn recording(pool: &PgPool, name: &str, monitor: &str) -> String {
    let id = assets::create(pool, None, "database_server", name, &[])
        .await
        .expect("the asset")
        .value
        .id;
    assets::edit(
        pool,
        &id,
        &[AssetEdit::Monitors {
            added: vec![monitor.to_owned()],
        }],
    )
    .await
    .expect("the name recorded");
    id
}

/// A `CreateMonitor` for `name` in source `kuma`, as `submit_write` queues it:
/// the payload is the op serialized, as `knobas_sync::write_queue::queue`
/// stores it, so a renamed variant or field fails these tests rather than
/// leaving the read's `payload->'CreateMonitor'->>'name'` naming nothing
/// (ADR-0007, requirement 3: the read fails open).
async fn create(pool: &PgPool, name: &str) -> i64 {
    let op = knobas_source::WriteOp::CreateMonitor {
        entity: "kuma:monitors".to_owned(),
        name: name.to_owned(),
        url: "db.internal:5432".to_owned(),
        kind: knobas_source::MonitorKind::Tcp,
    };
    knobas_core::write_queue::queue(
        pool,
        "kuma",
        &EntityRef::parse("kuma:monitors").unwrap(),
        op.identifier(),
        serde_json::to_value(&op).unwrap(),
        serde_json::json!({}),
    )
    .await
    .unwrap()
    .id
}

/// A monitor the poll mirrored, with no `monitored-by` link drawn: the arrival
/// alone, so the read's own predicate is what is under test.
async fn mirrored(pool: &PgPool, id: &str, title: &str) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,'monitor',$2)")
        .bind(id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item (entity_id, source_id, kind, title, body_text, payload)
         values ($1,'kuma','monitor',$2,'','{}'::jsonb)",
    )
    .bind(id)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
}

/// Each listed row's name and the create on its way for it, by id and state.
async fn on_its_way(pool: &PgPool) -> Vec<(String, Option<(i64, WriteState)>)> {
    assets::unmonitored_assets(pool)
        .await
        .expect("the unmonitored list")
        .into_iter()
        .map(|row: UnmonitoredAsset| {
            let write = row.monitor_on_its_way.map(|write| (write.id, write.state));
            (row.name, write)
        })
        .collect()
}

/// The issue's criterion: queued, then sent, the create is on its way on the
/// asset that records its name and on no other row. Once the mirror holds the
/// monitor its receipt named, it is not.
#[tokio::test]
async fn a_queued_create_is_on_its_way_on_the_row_that_records_its_name_until_its_monitor_is_mirrored()
 {
    let pool = pool("monitor-on-its-way").await;
    recording(&pool, "pg-main", "pg-main").await;
    recording(&pool, "redis", "redis").await;
    let id = create(&pool, "pg-main").await;
    assert_eq!(
        on_its_way(&pool).await,
        [
            ("pg-main".to_owned(), Some((id, WriteState::Pending))),
            ("redis".to_owned(), None),
        ],
        "queued"
    );

    knobas_core::write_queue::sent(&pool, id, Some("31"))
        .await
        .unwrap();
    assert_eq!(
        on_its_way(&pool).await[0],
        ("pg-main".to_owned(), Some((id, WriteState::Sent))),
        "sent, and the monitor is not mirrored yet"
    );

    // Named otherwise than the create asked, so only the receipt finds it.
    mirrored(&pool, "kuma:31", "pg-main (renamed in Kuma)").await;
    assert_eq!(
        on_its_way(&pool).await[0],
        ("pg-main".to_owned(), None),
        "the mirror holds the monitor the receipt named"
    );
}

/// A create whose source named nothing has arrived once the source's mirror
/// holds a monitor of its name.
#[tokio::test]
async fn a_sent_create_with_no_receipt_has_arrived_once_a_monitor_of_its_name_is_mirrored() {
    let pool = pool("monitor-on-its-way-no-receipt").await;
    recording(&pool, "pg-main", "pg-main").await;
    let id = create(&pool, "pg-main").await;
    knobas_core::write_queue::sent(&pool, id, None)
        .await
        .unwrap();
    assert_eq!(
        on_its_way(&pool).await,
        [("pg-main".to_owned(), Some((id, WriteState::Sent)))]
    );
    mirrored(&pool, "kuma:32", "pg-main").await;
    assert_eq!(on_its_way(&pool).await, [("pg-main".to_owned(), None)]);
}

/// Held on drift (never sent), refused and withdrawn creates wait on the
/// reader in the queue, or will not go at all, so the row offers *Create
/// monitor*. A sent create the
/// mirror has not brought in within the patience is not taken for on its way.
#[tokio::test]
async fn a_held_refused_withdrawn_or_long_lost_create_is_not_on_its_way() {
    let pool = pool("monitor-on-its-way-stopped").await;
    recording(&pool, "pg-main", "pg-main").await;
    let held = create(&pool, "pg-main").await;
    knobas_core::write_queue::hold(&pool, held, serde_json::json!({}))
        .await
        .unwrap();
    let refused = create(&pool, "pg-main").await;
    knobas_core::write_queue::refuse(&pool, refused, "no Docker host")
        .await
        .unwrap();
    let withdrawn = create(&pool, "pg-main").await;
    knobas_core::write_queue::discard(&pool, withdrawn)
        .await
        .unwrap();
    let lost = create(&pool, "pg-main").await;
    knobas_core::write_queue::sent(&pool, lost, Some("40"))
        .await
        .unwrap();
    sqlx::query(
        "update knobas.write_queue set settled_at = now() - interval '8 days' where id = $1",
    )
    .bind(lost)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(on_its_way(&pool).await, [("pg-main".to_owned(), None)]);

    let again = create(&pool, "pg-main").await;
    assert_eq!(
        on_its_way(&pool).await,
        [("pg-main".to_owned(), Some((again, WriteState::Pending)))],
        "the press after them is on its way"
    );
}

/// A create held as unconfirmed (#1125) reached Uptime Kuma and got no answer,
/// so the monitor may exist: it is on its way, and the row draws *Monitor
/// queued*, until the reader sends it again (pending, still on its way) or
/// discards it (#1966). A drift hold beside it was never sent, and is not.
#[tokio::test]
async fn a_create_held_as_unconfirmed_is_on_its_way_until_the_reader_discards_it() {
    let pool = pool("monitor-on-its-way-unconfirmed").await;
    recording(&pool, "pg-main", "pg-main").await;
    let drift = create(&pool, "pg-main").await;
    knobas_core::write_queue::hold(&pool, drift, serde_json::json!({}))
        .await
        .unwrap();
    assert_eq!(on_its_way(&pool).await, [("pg-main".to_owned(), None)], "drift");

    let unanswered = create(&pool, "pg-main").await;
    knobas_core::write_queue::hold(
        &pool,
        unanswered,
        knobas_core::write_queue::unconfirmed(
            "create_monitor",
            serde_json::json!({}),
            "Uptime Kuma",
            "the connection closed before an answer",
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        on_its_way(&pool).await,
        [("pg-main".to_owned(), Some((unanswered, WriteState::Held)))],
        "unconfirmed"
    );

    knobas_core::write_queue::apply_anyway(&pool, unanswered)
        .await
        .unwrap();
    assert_eq!(
        on_its_way(&pool).await,
        [("pg-main".to_owned(), Some((unanswered, WriteState::Pending)))],
        "sent again"
    );

    knobas_core::write_queue::discard(&pool, unanswered)
        .await
        .unwrap();
    assert_eq!(on_its_way(&pool).await, [("pg-main".to_owned(), None)], "discarded");
}

/// A route records the names that attach its monitors too (#976), and two
/// subjects recording one name are both attached by the one monitor
/// (`knobas_sync::attach`), so the create is on its way on both.
#[tokio::test]
async fn a_create_is_on_its_way_on_every_asset_and_route_that_records_its_name() {
    let pool = pool("monitor-on-its-way-route").await;
    let gitea = recording(&pool, "gitea-vm", "Gitea /health").await;
    let route = assets::create_route(
        &pool,
        &gitea,
        "Gitea",
        "https://gitea.test/",
        None,
        Visibility::Public,
        &[],
    )
    .await
    .expect("the route")
    .value
    .id;
    assets::edit_route(
        &pool,
        &route,
        &[RouteEdit::Monitors {
            added: vec!["Gitea /health".to_owned()],
        }],
    )
    .await
    .expect("the name recorded on the route");
    let id = create(&pool, "Gitea /health").await;
    assert_eq!(
        on_its_way(&pool).await,
        [
            ("gitea-vm".to_owned(), Some((id, WriteState::Pending))),
            ("Gitea".to_owned(), Some((id, WriteState::Pending))),
        ]
    );
}

/// Two creates of one name on their way at once, as a second press queued
/// them before `submit_write` refused it: the row names the newer, the one a
/// reader pressed last.
#[tokio::test]
async fn of_two_creates_of_one_name_on_their_way_the_row_names_the_newer() {
    let pool = pool("monitor-on-its-way-two").await;
    recording(&pool, "pg-main", "pg-main").await;
    let older = create(&pool, "pg-main").await;
    knobas_core::write_queue::sent(&pool, older, Some("31"))
        .await
        .unwrap();
    let newer = create(&pool, "pg-main").await;
    assert_eq!(
        on_its_way(&pool).await,
        [("pg-main".to_owned(), Some((newer, WriteState::Pending)))]
    );
}
