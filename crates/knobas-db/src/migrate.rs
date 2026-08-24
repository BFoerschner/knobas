//! Schema migrations, embedded in the binary and applied at startup.
//!
//! `sqlx::migrate!` bakes `migrations/` into the executable at compile time,
//! so a shipped build carries its own schema; `build.rs` reruns the build when
//! the directory changes.

/// Bring `pool`'s database up to the current schema.
///
/// Re-entrant: sqlx records what it applied in `_sqlx_migrations` and takes an
/// advisory lock, so running this repeatedly -- or concurrently from several
/// connections -- applies each migration exactly once.
///
/// # Errors
///
/// Returns [`crate::DbError::Migrate`] if a migration fails to apply, or if an
/// already-applied migration's checksum no longer matches the embedded file.
pub async fn run(pool: &sqlx::PgPool) -> Result<(), crate::DbError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
