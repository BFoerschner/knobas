//! Schema migrations, embedded in the binary and applied at startup.
//!
//! `sqlx::migrate!` bakes `migrations/` into the executable at compile time,
//! so a shipped build carries its own schema; `build.rs` reruns the build when
//! the directory changes.
//!
//! A migration's file, comments included, is frozen once released: sqlx
//! checksums the whole file, and an edited one fails startup on every database
//! that applied it (#2327). Change the schema with a new migration and append
//! its line to `migrations.sha384`; `tests/migration_checksums.rs` enforces both.

/// Bring `pool`'s database up to the current schema.
///
/// Re-entrant: sqlx records what it applied in `_sqlx_migrations` and takes an
/// advisory lock, so running this repeatedly -- or concurrently from several
/// connections -- applies each migration exactly once.
///
/// On a one-connection pool of its own, opened with `pool`'s options but no
/// `statement_timeout`: the app's pool carries one
/// ([`crate::embedded::UI_STATEMENT_TIMEOUT`], #745), and a migration that
/// rewrites the mirror on a large database may legitimately run past it
/// (`0027` measured 6.9 s at 100,000 rows). A pool of its own rather than a
/// connection borrowed from `pool`, so a cleared limit never goes back into
/// the app's pool.
///
/// # Errors
///
/// Returns [`crate::DbError::Migrate`] if a migration fails to apply, or if an
/// already-applied migration's checksum no longer matches the embedded file;
/// [`crate::DbError::Sqlx`] if no connection can be had.
pub async fn run(pool: &sqlx::PgPool) -> Result<(), crate::DbError> {
    let options = (*pool.connect_options())
        .clone()
        .options([("statement_timeout", "0")]);
    let migrating = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let outcome = sqlx::migrate!("./migrations").run(&migrating).await;
    migrating.close().await;
    outcome?;
    Ok(())
}
