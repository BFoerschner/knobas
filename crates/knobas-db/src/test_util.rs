//! One embedded PostgreSQL per test binary, shared by every test in it.
//!
//! Enabled by the `test-util` feature (not `cfg(test)` -- that is not set for
//! a crate's own `tests/` directory, nor for downstream crates).
//!
//! Every caller gets the *same* database, so tests must isolate themselves
//! with unique keys. Truncating shared tables would break tests running
//! concurrently in the same binary.

use sqlx::PgPool;

use crate::{DbConfig, EmbeddedDb};

/// The shared pool for this test binary, starting the server on first use.
///
/// # Panics
///
/// Panics if the database cannot be started -- there is no useful way for a
/// test to continue without one.
pub async fn test_pool() -> &'static PgPool {
    static DB: tokio::sync::OnceCell<EmbeddedDb> = tokio::sync::OnceCell::const_new();

    let db = DB
        .get_or_init(|| async {
            let root_dir = std::env::temp_dir().join(format!("knobas-test-{}", std::process::id()));
            EmbeddedDb::start(DbConfig {
                root_dir,
                existing_url: None,
            })
            .await
            .expect("start embedded postgres for tests")
        })
        .await;

    db.pool()
}
