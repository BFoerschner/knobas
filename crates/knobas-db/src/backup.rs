//! Backup export and restore: a compressed logical dump of the `knobas`
//! schema, and the way back in.
//!
//! Spec §14, ratified as §16.12: a **backup** is everything in `knobas`,
//! notes included; the `sync` mirror is left out because it re-syncs, and
//! leaving it out is what keeps archives small. Entity references are the
//! stable ids of §5a, which is why an archive taken on one machine restores on
//! another, and why one taken before a re-sync still resolves after it: the
//! mirror is rebuilt under links that never changed.
//!
//! # Why this lives in `knobas-db`
//!
//! Because the two things it needs are both here and both private. The
//! binaries are the ones `postgresql_embedded` unpacked beside the server
//! (ADR-0001: "export comes free" is exactly this), and reaching them means
//! knowing the installation directory. The connection needs a **password**,
//! and [`Connector`] exists precisely so that no other crate can get one --
//! see its docs. A backup module anywhere else would have to be handed the
//! secret to do its job.
//!
//! # Which `pg_dump`
//!
//! The one that came out of the same archive as the server, found under
//! `~/.theseus/postgresql/<version>/bin`. Not a `pg_dump` on `PATH`: PostgreSQL
//! refuses to dump a server newer than the client, so a machine with an older
//! Homebrew PostgreSQL in front of it would fail on a knobas that works
//! perfectly. knobas ships its own and uses its own.
//!
//! The one case where they are not there is [`DbConfig::existing_url`]: a user
//! pointing knobas at a server they manage may never have made knobas download
//! an embedded PostgreSQL at all. That is [`BackupError::ToolMissing`], which
//! says which tool, where it was looked for, and why.
//!
//! # Format
//!
//! `--format=custom`, which is a compressed logical dump and the only format
//! `pg_restore` can be selective about. The archive carries its own DDL, so
//! `pg_restore` into a genuinely empty database works by hand with no knobas
//! involved -- a backup nobody but knobas can read is not a backup.
//!
//! [`Connector`]: crate::embedded::Connector
//! [`DbConfig::existing_url`]: crate::DbConfig::existing_url

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Output;

use sqlx::Connection;

use crate::embedded::Connector;

/// The schema a backup contains. The mirror (`sync`) is deliberately not in
/// it: §16.12, ratified 2026-08-27.
pub const OWNED_SCHEMA: &str = "knobas";

/// The extension spec §14 gives a knobas archive.
pub const ARCHIVE_EXTENSION: &str = "knobas";

/// The one `knobas` table that does not count as *occupancy*.
///
/// `knobas.setting` is knobas's own bookkeeping -- migration `0002` comment 6
/// names "first-run completion, the last opened context, later the export
/// schedule" -- and the backup service writes it by itself, within seconds of
/// a first start. Counting it would make [`restore`] refuse the machine it
/// exists for: nobody restores into a knobas that has never run, so by the
/// time a user has an archive to pick, the nightly task has already recorded
/// one. A guard that fires on every machine is not conservative, it is a dead
/// command.
///
/// It is still *in* the archive -- the ratified scope is the whole schema --
/// and [`restore`] therefore clears it first, because the rows collide on the
/// primary key.
const BOOKKEEPING_TABLE: &str = "setting";

const PG_DUMP: &str = if cfg!(windows) {
    "pg_dump.exe"
} else {
    "pg_dump"
};
const PG_RESTORE: &str = if cfg!(windows) {
    "pg_restore.exe"
} else {
    "pg_restore"
};

/// Everything that can go wrong taking or restoring a backup.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// The bundled PostgreSQL tools are not on this machine.
    #[error(
        "{tool} is not installed under {}: knobas ships PostgreSQL's backup tools with its \
         embedded server, so this means the server is one knobas does not manage",
        looked_in.display()
    )]
    ToolMissing { tool: String, looked_in: PathBuf },

    /// The tool ran and refused.
    #[error("{tool} failed ({status}): {stderr}")]
    Tool {
        tool: String,
        status: String,
        stderr: String,
    },

    #[error("{tool} could not be started: {source}")]
    Spawn {
        tool: String,
        #[source]
        source: io::Error,
    },

    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// The target of a restore already holds knobas-owned data.
    ///
    /// Merging an archive into a populated database is M4 (spec §14: "restore
    /// into a populated one merges by id with a preview"). Until then the
    /// refusal is the honest answer -- the alternative is overwriting somebody's
    /// links.
    #[error(
        "the database already holds knobas data ({rows} row(s) in {OWNED_SCHEMA}.{table}); \
         restoring into it would overwrite that. Merge-restore is not built yet -- restore \
         into an empty knobas instead"
    )]
    TargetNotEmpty { table: String, rows: i64 },

    /// The target has never been migrated, so there is nothing to restore
    /// *into*.
    #[error(
        "the database has no `{OWNED_SCHEMA}` schema: knobas migrates on start, so restore \
         after the first start rather than before it"
    )]
    TargetNotMigrated,

    #[error("postgres: {0}")]
    Sqlx(#[from] sqlx::Error),
}

/// One line of an archive's table of contents.
///
/// What `pg_restore --list` prints, parsed down to the three fields that say
/// what is in the archive: `3345; 0 16389 TABLE DATA knobas note postgres`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    /// `TABLE`, `TABLE DATA`, `INDEX`, `SEQUENCE SET`, …
    pub kind: String,
    /// The schema the object belongs to.
    pub schema: String,
    /// The object's name.
    pub name: String,
}

/// Write a compressed logical dump of the `knobas` schema to `archive`.
///
/// Returns the archive's size in bytes.
///
/// The parent directory is created if it is missing, so a caller can name a
/// backup directory that has never been used.
///
/// # Errors
///
/// [`BackupError::ToolMissing`] when the bundled `pg_dump` is not there,
/// [`BackupError::Tool`] when it refuses, [`BackupError::Io`] when the archive
/// cannot be written or measured.
pub async fn dump(connector: &Connector, archive: &Path) -> Result<u64, BackupError> {
    if let Some(parent) = archive.parent() {
        std::fs::create_dir_all(parent).map_err(|source| BackupError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    run_tool(
        connector,
        PG_DUMP,
        &[
            // A compressed logical dump (§14). `custom` compresses by default
            // and is the only format `pg_restore` can filter.
            OsStr::new("--format=custom"),
            // The ratified scope, expressed as a *schema* and never as a table
            // list -- so a table Links v1 or any later migration adds is in the
            // backup without anyone remembering to add it.
            OsStr::new(&format!("--schema={OWNED_SCHEMA}")),
            OsStr::new("--file"),
            archive.as_os_str(),
        ],
    )
    .await?;

    let bytes = std::fs::metadata(archive)
        .map_err(|source| BackupError::Io {
            path: archive.to_path_buf(),
            source,
        })?
        .len();
    Ok(bytes)
}

/// What an archive says it contains.
///
/// The archive's own table of contents, as `pg_restore --list` prints it. This
/// is the seam a test can hold the ratified scope against: the argument list
/// that produced the dump is what is under test, so reading it back off the
/// file is the only observation that cannot agree with a wrong one.
///
/// # Errors
///
/// [`BackupError::ToolMissing`] or [`BackupError::Tool`] if `pg_restore` will
/// not read the file.
pub async fn archive_contents(archive: &Path) -> Result<Vec<TocEntry>, BackupError> {
    let tool = locate(PG_RESTORE)?;
    let output = tokio::process::Command::new(&tool)
        .arg("--list")
        .arg(archive)
        .output()
        .await
        .map_err(|source| BackupError::Spawn {
            tool: PG_RESTORE.to_owned(),
            source,
        })?;
    check(PG_RESTORE, &output)?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_toc_line)
        .collect())
}

/// One `pg_restore --list` line, or `None` for a comment or a header.
///
/// The archiver prints `%d; %u %u %s %s %s %s` -- dump id, two catalog oids,
/// **description**, namespace, tag, owner -- and two of those `%s` contain
/// spaces of their own: a description is `TABLE DATA` or `FK CONSTRAINT`, and
/// a constraint's tag is `<table> <constraint>`. So neither end of the line is
/// a fixed number of fields, and splitting from either one alone gets the
/// middle wrong.
///
/// What separates them is case. Every description PostgreSQL emits is upper
/// case (`TABLE`, `SEQUENCE SET`, `MATERIALIZED VIEW DATA`); a namespace and a
/// tag are identifiers. The description is therefore the leading run of
/// upper-case tokens, the namespace is the one after it, and the tag is what
/// is left once the owner is taken off the end. `-` is what the archiver
/// prints for an object that belongs to no schema.
fn parse_toc_line(line: &str) -> Option<TocEntry> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(';') {
        return None;
    }
    let (_dump_id, rest) = line.split_once(';')?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // Two catalog oids, then description, namespace, tag and owner.
    if fields.len() < 6 {
        return None;
    }
    let after_oids = &fields[2..];
    let described = after_oids
        .iter()
        .take_while(|word| is_description_word(word))
        .count();
    // A description, a namespace, a tag and an owner have to be left.
    if described == 0 || after_oids.len() < described + 3 {
        return None;
    }
    let kind = after_oids[..described].join(" ");
    let schema = after_oids[described].to_owned();
    let name = after_oids[described + 1..after_oids.len() - 1].join(" ");
    Some(TocEntry { kind, schema, name })
}

/// Whether a token is part of an archive entry's description.
///
/// Upper case with no lower-case letter in it. `MATERIALIZED VIEW DATA` is the
/// longest of them; an identifier that happened to be spelled in capitals
/// would be misread, which knobas has none of and which the schema-scoped
/// assertions in `tests/backup.rs` would show up.
fn is_description_word(word: &str) -> bool {
    !word.is_empty()
        && word.chars().all(|c| !c.is_lowercase())
        && word.chars().any(char::is_alphabetic)
}

/// Restore an archive's `knobas` data into a migrated, empty database.
///
/// The primary path of spec §14 -- "restore into an empty database" -- applied
/// to the one database knobas has: the schema is already there (knobas
/// migrates on start), so what the archive puts back is the *data*. Restoring
/// its DDL on top of the migrated schema would collide with every object it
/// declares; the archive still carries that DDL, for a human restoring into a
/// database knobas has never touched.
///
/// Refuses rather than merges when the target holds knobas rows
/// ([`BackupError::TargetNotEmpty`]): merge-restore with a preview is M4.
/// "Holds rows" means *content*; [`BOOKKEEPING_TABLE`] is excluded and
/// replaced by the archive's own, for the reason recorded there.
///
/// # Foreign keys
///
/// `--disable-triggers`. A data-only restore loads tables in the archive's
/// order, which is *not* a topological order for foreign keys -- `knobas.context`
/// references `knobas.entity` and sorts before it -- so the constraints are held
/// off for the load and enforced again after. This is the documented use of the
/// flag and needs a superuser, which the embedded server's connection is.
///
/// # Errors
///
/// [`BackupError::TargetNotMigrated`], [`BackupError::TargetNotEmpty`],
/// [`BackupError::ToolMissing`], [`BackupError::Tool`].
pub async fn restore(connector: &Connector, archive: &Path) -> Result<(), BackupError> {
    let mut conn = connector.connect().await?;
    match owned_rows(&mut conn).await? {
        Occupancy::NotMigrated => return Err(BackupError::TargetNotMigrated),
        Occupancy::Occupied { table, rows } => {
            return Err(BackupError::TargetNotEmpty { table, rows });
        }
        Occupancy::Empty => {}
    }
    // Every other table in the schema has just been proved empty, so the only
    // rows here are the ones this knobas wrote about itself. The archive
    // carries its own, keyed the same way, and a data-only load would collide
    // on the primary key -- `--disable-triggers` holds off *triggers*, not a
    // unique index. The archive's settings are the user's; these are a default
    // this machine invented on its way to asking for them back.
    // Both halves of the name are crate constants; nothing from a caller
    // reaches this string, which is what `AssertSqlSafe` is asserting (the
    // same use `test_util::scratch_database` makes of it).
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "delete from {OWNED_SCHEMA}.{BOOKKEEPING_TABLE}"
    )))
    .execute(&mut conn)
    .await?;
    let _ = conn.close().await;

    // Explicit, although `PGDATABASE` is set: `pg_restore` treats "no
    // `--dbname` and no `--file`" as "print the SQL to stdout" and refuses
    // outright rather than falling back to the environment, so a restore
    // without it is not a restore into the wrong database -- it is no restore
    // at all, reported as an error.
    let dbname = format!(
        "--dbname={}",
        connector.database().unwrap_or(crate::DATABASE_NAME)
    );

    run_tool(
        connector,
        PG_RESTORE,
        &[
            OsStr::new(&dbname),
            OsStr::new("--data-only"),
            OsStr::new("--disable-triggers"),
            OsStr::new("--single-transaction"),
            // Without this a failed statement is *counted* and the restore
            // reports success, which is how a half-restored database gets
            // mistaken for a restored one.
            OsStr::new("--exit-on-error"),
            OsStr::new(&format!("--schema={OWNED_SCHEMA}")),
            // An archive from another machine names that machine's role. The
            // data is what is being restored, not the ownership.
            OsStr::new("--no-owner"),
            OsStr::new("--no-privileges"),
            archive.as_os_str(),
        ],
    )
    .await?;
    Ok(())
}

/// How much knobas-owned data a database holds.
enum Occupancy {
    NotMigrated,
    Empty,
    Occupied { table: String, rows: i64 },
}

/// Whether the target has a `knobas` schema, and whether anything is in it.
///
/// The table list comes from the catalog rather than from a list in this file,
/// for the reason the dump is schema-scoped: a table a later migration adds is
/// checked without anyone remembering to add it here. `query_to_xml` is what
/// lets that stay **one static statement** -- the per-table probe is built by
/// PostgreSQL's own `format('%I')`, so no identifier from the catalog is ever
/// concatenated into SQL on this side.
///
/// The probe is `limit 1`, not a count: the question is "is there anything",
/// and a real count over a populated database would be work done only to
/// produce a refusal.
const OCCUPANCY: &str = r"
select c.relname::text as table_name,
       (xpath(
          '/row/c/text()',
          query_to_xml(
            format('select count(*) as c from (select 1 from %I.%I limit 1) probe',
                   n.nspname, c.relname),
            false, true, '')
       ))[1]::text::bigint as rows
  from pg_class c
  join pg_namespace n on n.oid = c.relnamespace
 where n.nspname = $1
   and c.relkind = 'r'
   and c.relname <> $2
 order by c.relname
";

async fn owned_rows(conn: &mut sqlx::PgConnection) -> Result<Occupancy, sqlx::Error> {
    let migrated: bool = sqlx::query_scalar("select to_regnamespace($1) is not null")
        .bind(OWNED_SCHEMA)
        .fetch_one(&mut *conn)
        .await?;
    if !migrated {
        return Ok(Occupancy::NotMigrated);
    }

    let occupied: Option<(String, i64)> = sqlx::query_as(OCCUPANCY)
        .bind(OWNED_SCHEMA)
        .bind(BOOKKEEPING_TABLE)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .find(|(_, rows): &(String, i64)| *rows > 0);

    Ok(match occupied {
        Some((table, rows)) => Occupancy::Occupied { table, rows },
        None => Occupancy::Empty,
    })
}

/// Find one of the bundled tools, run it against `connector`, and turn a
/// non-zero exit into a [`BackupError::Tool`] carrying its stderr.
async fn run_tool(
    connector: &Connector,
    tool: &str,
    args: &[&OsStr],
) -> Result<Output, BackupError> {
    let path = locate(tool)?;
    let mut command = tokio::process::Command::new(&path);
    command.args(args);
    for (key, value) in connector.libpq_env() {
        command.env(key, value);
    }
    let output = command
        .output()
        .await
        .map_err(|source| BackupError::Spawn {
            tool: tool.to_owned(),
            source,
        })?;
    check(tool, &output)?;
    Ok(output)
}

fn locate(tool: &str) -> Result<PathBuf, BackupError> {
    let installation_dir = crate::embedded::installation_dir();
    crate::embedded::find_tool(&installation_dir, tool).ok_or(BackupError::ToolMissing {
        tool: tool.to_owned(),
        looked_in: installation_dir,
    })
}

fn check(tool: &str, output: &Output) -> Result<(), BackupError> {
    if output.status.success() {
        return Ok(());
    }
    Err(BackupError::Tool {
        tool: tool.to_owned(),
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `TABLE DATA` is what a backup is made of, and a parser that assumed a
    /// one-word description would report every data entry with the schema in
    /// the kind field and the table name in the schema field -- wrong in a way
    /// that still looks like a parse.
    #[test]
    fn a_two_word_description_stays_in_the_kind() {
        assert_eq!(
            parse_toc_line("3345; 0 16389 TABLE DATA knobas note postgres"),
            Some(TocEntry {
                kind: "TABLE DATA".to_owned(),
                schema: "knobas".to_owned(),
                name: "note".to_owned(),
            })
        );
        assert_eq!(
            parse_toc_line("215; 1259 16389 TABLE knobas entity postgres"),
            Some(TocEntry {
                kind: "TABLE".to_owned(),
                schema: "knobas".to_owned(),
                name: "entity".to_owned(),
            })
        );
        assert_eq!(
            parse_toc_line("3346; 0 0 SEQUENCE SET knobas activity_id_seq postgres"),
            Some(TocEntry {
                kind: "SEQUENCE SET".to_owned(),
                schema: "knobas".to_owned(),
                name: "activity_id_seq".to_owned(),
            })
        );
    }

    /// The other end of the same ambiguity: a constraint's *tag* is two words,
    /// so a parser anchored on the owner would report the table as the schema
    /// -- and an entry whose schema is misread is an entry a scope assertion
    /// silently stops covering.
    #[test]
    fn a_two_word_tag_does_not_shift_the_schema() {
        assert_eq!(
            parse_toc_line("4012; 2606 16401 FK CONSTRAINT sync item item_entity_id_fkey postgres"),
            Some(TocEntry {
                kind: "FK CONSTRAINT".to_owned(),
                schema: "sync".to_owned(),
                name: "item item_entity_id_fkey".to_owned(),
            })
        );
    }

    /// An object that belongs to no schema -- the `CREATE SCHEMA` entries
    /// themselves.
    #[test]
    fn an_object_with_no_schema_reads_a_dash() {
        assert_eq!(
            parse_toc_line("8; 2615 16388 SCHEMA - knobas postgres"),
            Some(TocEntry {
                kind: "SCHEMA".to_owned(),
                schema: "-".to_owned(),
                name: "knobas".to_owned(),
            })
        );
    }

    /// Everything `pg_restore --list` prints around the entries.
    #[test]
    fn the_header_and_the_comments_are_not_entries() {
        for line in [
            "",
            ";",
            "; Archive created at 2026-08-28 03:00:00 CEST",
            ";     dbname: knobas",
            "; Selected TOC Entries:",
        ] {
            assert_eq!(parse_toc_line(line), None, "{line:?}");
        }
    }
}
