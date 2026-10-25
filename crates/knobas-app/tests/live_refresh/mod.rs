//! The landed-write refresh's live witness (#2096, #2099, ADR-0031), one
//! module for every suite that sends a write through the queue and asserts
//! what the mirror held when its refresh ended, before the run after it.
//!
//! `mod live_refresh;` from the suite; the suite builds its app on
//! `knobas_app::sources::start_scheduler` with [`Snapshots`] as its events.

use std::sync::Arc;

use knobas_sync::scheduler::{RefreshEnded, SyncEvents};

/// What the mirror held the moment a refresh ended: the payload of the item
/// refreshed, and how many runs of its source had started since the write it
/// followed settled.
pub struct Snapshot {
    pub ended: RefreshEnded,
    pub payload: Option<serde_json::Value>,
    pub runs_since_the_write: i64,
}

/// Hears `item:refreshed` and reads the mirror **inside** the call. It is
/// told before the run after the write is asked for
/// (`sources::write_queue::after_landing`), so what it reads is what the
/// refresh wrote, not what that run did; the run count checks it rather than
/// trusting the order.
#[derive(Clone)]
pub struct Snapshots {
    pub pool: sqlx::PgPool,
    pub taken: Arc<std::sync::Mutex<Vec<Snapshot>>>,
}

impl SyncEvents for Snapshots {
    fn sync_state(&self, _status: knobas_sync::SourceSyncStatus) {}
    fn source_health(&self, _health: knobas_sync::CredentialHealth) {}
    fn activity_new(&self, _row: knobas_core::activity::ActivityRow) {}
    fn refresh_ended(&self, ended: &RefreshEnded) {
        let pool = self.pool.clone();
        let id = ended.entity_id.clone();
        let write = ended.write_id;
        let (payload, runs_since_the_write) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let payload: Option<serde_json::Value> =
                    sqlx::query_scalar("select payload from sync.live_item where entity_id = $1")
                        .bind(&id)
                        .fetch_optional(&pool)
                        .await
                        .expect("the mirror is readable");
                let runs: i64 = sqlx::query_scalar(
                    "select count(*) from knobas.sync_run r, knobas.write_queue w
                      where w.id = $1 and r.source_id = w.source_id
                        and r.started_at > w.settled_at",
                )
                .bind(write)
                .fetch_one(&pool)
                .await
                .expect("the runs are readable");
                (payload, runs)
            })
        });
        self.taken.lock().unwrap().push(Snapshot {
            ended: ended.clone(),
            payload,
            runs_since_the_write,
        });
    }
}

impl Snapshots {
    /// The snapshot of write `id`'s refresh, waited for: the refresh follows
    /// the send in the background.
    pub async fn of(&self, id: i64) -> Snapshot {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            {
                let mut taken = self.taken.lock().unwrap();
                if let Some(at) = taken.iter().position(|s| s.ended.write_id == Some(id)) {
                    return taken.remove(at);
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "write {id} landed but no refresh of it ended within a minute"
            );
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }
}

/// The id the source answered write `id` with, as the settle stamped it: a
/// page or comment id, a build key.
pub async fn remote_id(pool: &sqlx::PgPool, id: i64) -> String {
    sqlx::query_scalar::<_, Option<String>>(
        "select remote_id from knobas.write_queue where id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("the queue row")
    .unwrap_or_else(|| panic!("write {id} carries no remote id"))
}
