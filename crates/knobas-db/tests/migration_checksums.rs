//! A released migration never changes, not by one byte (#2327).
//!
//! sqlx stores each applied migration's SHA-384 in `_sqlx_migrations` and, at
//! startup, refuses a database whose recorded checksum no longer matches the
//! embedded file. The checksum covers the whole file, comments included, so a
//! comment edit to an applied migration stops every existing database from
//! starting. That happened once: #2224 re-pointed doc paths in the comments of
//! 0031 to 0065 after `v3.2.6` had applied them.
//!
//! `migrations.sha384` pins the checksum of every migration. This test reads
//! the checksums off the same `sqlx::migrate!` the app embeds and compares.
//! A new migration fails it once, with the line to append.

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
struct Pinned<'a> {
    checksum: &'a str,
    file: &'a str,
}

fn pinned() -> BTreeMap<i64, Pinned<'static>> {
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
        match pinned.remove(&migration.version) {
            Some(p) if p.checksum == actual => {}
            Some(p) => failures.push(format!(
                "migration {} ({}) changed: pinned {}, now {actual}. {FROZEN}",
                migration.version, p.file, p.checksum
            )),
            None => failures.push(format!(
                "migration {} ({}) has no line in {LIST_PATH}. A new migration's line is \
                 appended at the end of the list, never inserted; append:\n{:04} {actual} {:04}_{}.sql",
                migration.version,
                migration.description,
                migration.version,
                migration.version,
                migration.description.replace(' ', "_"),
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
