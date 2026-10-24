//! Fixture helpers the integration modules share, beside the fixture files
//! they embed with `include_str!`.
//!
//! Each takes the pool the calling test built (`knobas_db::test_util` keys the
//! database by the calling source file, so a helper here never opens one).

use serde_json::Value;
use sqlx::PgPool;

/// `registry` the way a read that builds an adapter takes it: shared, so the
/// build can run on the blocking pool (#1705).
pub fn shared(
    registry: impl knobas_sync::scheduler::AdapterRegistry,
) -> std::sync::Arc<dyn knobas_sync::scheduler::AdapterRegistry> {
    std::sync::Arc::new(registry)
}

/// One `knobas.source_config` row: `id` configured as adapter `kind`, shown as
/// `display_name`, at `http://x` with a `Pat`, holding `config`.
pub async fn source_config(
    pool: &PgPool,
    id: &str,
    kind: &str,
    display_name: &str,
    config: &Value,
) {
    sqlx::query(
        "insert into knobas.source_config (id, kind, display_name, base_url, auth_kind, config)
         values ($1, $2, $3, 'http://x', 'Pat', $4)",
    )
    .bind(id)
    .bind(kind)
    .bind(display_name)
    .bind(config)
    .execute(pool)
    .await
    .expect("the source_config row");
}

/// One standing `sync.role_check` miss (ADR-0028, #1199): `source` fills
/// `role` but its payloads lack `missed_field` of `missed_kind`, declared at
/// `missed_path`.
pub async fn role_miss(
    pool: &PgPool,
    source: &str,
    role: &str,
    missed_kind: &str,
    missed_field: &str,
    missed_path: &str,
) {
    sqlx::query(
        "insert into sync.role_check (source_id, role, missed_kind, missed_field, missed_path)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(source)
    .bind(role)
    .bind(missed_kind)
    .bind(missed_field)
    .bind(missed_path)
    .execute(pool)
    .await
    .expect("the role_check miss");
}

/// Wait until a session is waiting on a lock of `relation` in this database:
/// the server lists its request, not granted (#1927). Asked on `conn`, which
/// may be the very connection holding the lock. A hang guard of a minute, not
/// a measurement.
pub async fn await_lock_waiter(conn: &mut sqlx::PgConnection, relation: &str) {
    let waited = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let waiting: i64 = sqlx::query_scalar(
                "select count(*) from pg_locks l join pg_class c on c.oid = l.relation
                  where c.relname = $1 and not l.granted
                    and l.database = (select oid from pg_database
                                       where datname = current_database())",
            )
            .bind(relation)
            .fetch_one(&mut *conn)
            .await
            .expect("the locks read");
            if waiting > 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        waited.is_ok(),
        "nothing waited on a lock of {relation} in a minute"
    );
}
