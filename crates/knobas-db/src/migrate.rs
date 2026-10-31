//! Schema migrations, embedded in the binary and applied at startup.
//!
//! `sqlx::migrate!` bakes `migrations/` into the executable at compile time,
//! so a shipped build carries its own schema; `build.rs` reruns the build when
//! the directory changes.
//!
//! A migration's file, comments included, is frozen once released: sqlx
//! checksums the whole file, and an edited one fails startup on every database
//! that applied it (#2327). Change the schema with a new migration and append
//! its line to `migrations.sha384`. This module's test fails when a file no
//! longer matches its pinned line, or a file has no line; a changed line in the
//! list is a diff for review to catch.

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

#[cfg(test)]
mod tests {
    //! `migrations.sha384` pins the SHA-384 sqlx records for every migration.
    //! The test reads the checksums off the same `sqlx::migrate!` that [`run`]
    //! embeds and compares. A new migration fails it once, with the line to
    //! append.
    //!
    //! [`run`]: super::run

    use std::collections::BTreeMap;
    use std::fmt::Write as _;

    const LIST: &str = include_str!("../migrations.sha384");
    const LIST_PATH: &str = "crates/knobas-db/migrations.sha384";

    const FROZEN: &str = "A released migration may never be edited, comments included: \
         sqlx checksums the whole file, and every database that applied it fails to start \
         with \"migration N was previously applied but has been modified\" (#2327). \
         Restore the file's released bytes (`git checkout <last release tag> -- <file>`) \
         and make the change in a new migration.";

    /// One line of the list: version, checksum, file name.
    struct Pinned {
        checksum: &'static str,
        file: &'static str,
    }

    fn pinned() -> BTreeMap<i64, Pinned> {
        let mut out = BTreeMap::new();
        let mut last = 0_i64;
        for (n, line) in LIST.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split_whitespace().collect();
            let [version, checksum, file] = fields[..] else {
                panic!(
                    "{LIST_PATH}:{}: expected `<version> <sha384> <file>`, got {line:?}",
                    n + 1
                );
            };
            let version: i64 = version
                .parse()
                .unwrap_or_else(|_| panic!("{LIST_PATH}:{}: version {version:?}", n + 1));
            assert!(
                version > last,
                "{LIST_PATH}:{}: version {version} after {last}; the list is append only, in version order",
                n + 1
            );
            last = version;
            out.insert(version, Pinned { checksum, file });
        }
        out
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }

    #[test]
    fn every_migration_keeps_the_checksum_it_was_pinned_with() {
        let migrator = sqlx::migrate!("./migrations");
        let mut pinned = pinned();
        let mut failures = Vec::new();

        for migration in migrator.iter() {
            let actual = hex(&migration.checksum);
            let version = migration.version;
            match pinned.remove(&version) {
                Some(p) if p.checksum == actual => {}
                Some(p) => failures.push(format!(
                    "migration {version} ({}) changed: pinned {}, now {actual}. {FROZEN}",
                    p.file, p.checksum
                )),
                None => failures.push(format!(
                    "migration {version} ({description}) has no line in {LIST_PATH}. A new \
                     migration's line is appended at the end of the list, never inserted; \
                     append:\n{version:04} {actual} {version:04}_{file}.sql",
                    description = migration.description,
                    file = migration.description.replace(' ', "_"),
                )),
            }
        }
        for (version, p) in pinned {
            failures.push(format!(
                "migration {version} ({}) is pinned in {LIST_PATH} but its file is gone. {FROZEN}",
                p.file
            ));
        }

        assert!(failures.is_empty(), "\n{}\n", failures.join("\n\n"));
    }
}
