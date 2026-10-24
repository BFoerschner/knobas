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

/// The `knobas` tables that do not count as *occupancy*.
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
///
/// **The shared layers of Dependencies** (`dep_layer`, `dep_layer_kind`,
/// migration `0036`, #871) are the same kind of row: a default every machine
/// is born with (the migration seeds R-11.11's scheme), which a restore
/// replaces with the archive's when the archive carries it. Counted as
/// content, they would make every restore refuse, as `setting` would.
/// Children before parents, so a clear never trips a foreign key.
///
/// **The saved views of Dependencies** (`dep_view`, migration `0038`, #873)
/// are seeded with the four built-in views for the same reason.
const BOOKKEEPING_TABLES: &[&str] = &["setting", "dep_layer_kind", "dep_layer", "dep_view"];

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

    /// A dump was asked for with no tables in it.
    ///
    /// The refusal is here rather than in the caller because of what
    /// `pg_dump` does with the alternative: an argument list carrying neither
    /// `--schema` nor a single `--table` dumps **the whole database**, mirror
    /// and all. So a share export whose every part is switched off would
    /// silently become the largest archive knobas can produce, which is the
    /// one outcome nobody asking for it wants.
    #[error("a share export with no parts in it would dump the whole database")]
    EmptyScope,

    /// The target has never been migrated, so there is nothing to restore
    /// *into*.
    #[error(
        "the database has no `{OWNED_SCHEMA}` schema: knobas migrates on start, so restore \
         after the first start rather than before it"
    )]
    TargetNotMigrated,

    /// The archive holds rows that break a foreign key the target declares
    /// (#1930), so nothing of it was restored.
    ///
    /// The load runs with the keys held off (see [`restore`]), so this is
    /// found after it, and the loaded rows are taken out again. A backup knobas
    /// wrote does not do this: every key held in the database it came from,
    /// and a share export carries the table each of its parts points at. What
    /// does is a hand-edited archive, or one taken before a migration added
    /// the key and did not settle its orphans.
    #[error(
        "the archive holds {rows} row(s) in {OWNED_SCHEMA}.{table} that break its foreign key \
         {key}, so nothing was restored. A backup knobas wrote never does: was this archive \
         edited by hand?"
    )]
    BrokenKey {
        table: String,
        key: String,
        rows: i64,
    },

    /// A restore whose load failed, and whose putting back of the
    /// [`BOOKKEEPING_TABLES`] failed after it (#1635).
    ///
    /// Both are named: the load's error says why the restore failed, and the
    /// second says this machine's settings and Dependencies layers and views
    /// may now be missing.
    #[error(
        "the restore failed ({load}), and putting this knobas's settings and Dependencies \
         layers back failed too: {source}"
    )]
    NotPutBack {
        load: Box<BackupError>,
        #[source]
        source: sqlx::Error,
    },

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
    // The ratified scope, expressed as a *schema* and never as a table list --
    // so a table Links v1 or any later migration adds is in the backup without
    // anyone remembering to add it.
    dump_with(connector, archive, &[format!("--schema={OWNED_SCHEMA}")]).await
}

/// Write a compressed logical dump of *some* of the `knobas` schema.
///
/// The share export of spec #427 (M4.2): the same custom-format archive
/// [`dump`] writes, restricted to `tables` -- unqualified names in
/// [`OWNED_SCHEMA`] -- and carrying its own DDL for them, so the archive is
/// still one `pg_restore` can read with no knobas involved.
///
/// **One thing a whole-schema dump carries and this does not**: `--table` is
/// documented to make no attempt at the objects the selected tables depend
/// upon, and `create schema knobas` is one of them. It costs nothing here --
/// [`restore`] loads `--data-only` into a database the migrations have
/// already made -- but a by-hand `pg_restore` of a share export wants the
/// schema to exist first, where a backup's would create it.
///
/// Returns the archive's size in bytes.
///
/// # Why a table list here and a schema there
///
/// The two arguments answer different questions. A *backup* is "everything
/// knobas owns", which is a property of the schema and must keep holding as
/// migrations add tables. A *share export* is "these parts and no others",
/// which is a property of a list somebody chose -- and a list that silently
/// grew when a migration landed would be the leak this feature exists to
/// prevent. So the backup is schema-scoped by design and this is not, and
/// [`crate::backup`]'s callers own the mapping from a part to its tables.
///
/// # Errors
///
/// [`BackupError::EmptyScope`] when `tables` is empty -- see there, it is not a
/// nicety -- [`BackupError::ToolMissing`] when the bundled `pg_dump` is not
/// there, [`BackupError::Tool`] when it refuses, [`BackupError::Io`] when the
/// archive cannot be written or measured.
pub async fn dump_tables(
    connector: &Connector,
    archive: &Path,
    tables: &[&str],
) -> Result<u64, BackupError> {
    if tables.is_empty() {
        return Err(BackupError::EmptyScope);
    }
    // `--table` takes a *pattern*, and an unquoted one is matched with the
    // wildcards of `psql`'s `\d`. Every name here is a crate-internal
    // constant, but quoting them costs nothing and keeps a future table whose
    // name contains a `_` -- which is a single-character wildcard -- from
    // pulling a sibling into a share export nobody meant to include.
    let args: Vec<String> = tables
        .iter()
        .map(|table| format!("--table=\"{OWNED_SCHEMA}\".\"{table}\""))
        .collect();
    dump_with(connector, archive, &args).await
}

/// The dump both entry points make, differing only in what they scope it to.
async fn dump_with(
    connector: &Connector,
    archive: &Path,
    scope: &[String],
) -> Result<u64, BackupError> {
    if let Some(parent) = archive.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| BackupError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
    }

    let mut args: Vec<&OsStr> = vec![
        // A compressed logical dump (§14). `custom` compresses by default
        // and is the only format `pg_restore` can filter.
        OsStr::new("--format=custom"),
    ];
    args.extend(scope.iter().map(|arg| OsStr::new(arg.as_str())));
    args.push(OsStr::new("--file"));
    args.push(archive.as_os_str());

    run_tool(connector, PG_DUMP, &args).await?;

    let bytes = tokio::fs::metadata(archive)
        .await
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
    // The one tool call that needs neither a server nor a password: a table of
    // contents is read off the file. Hence `tool_command` directly rather than
    // `run_tool`, which exists to attach a connection.
    let mut command = tool_command(PG_RESTORE).await?;
    command.arg("--list").arg(archive);
    let output = run(PG_RESTORE, command).await?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_toc_line)
        .collect())
}

/// The SQL an archive would replay -- its DDL and every row it holds.
///
/// What `pg_restore --file=-` prints: the script a person with nothing but
/// PostgreSQL's own tools would get. (`--file` is not optional -- a
/// `pg_restore` given neither a database nor a file refuses rather than
/// defaulting to stdout, the same refusal [`restore`] relies on.)
///
/// **It exists so that "nothing secret is in the archive" can be asserted
/// against the archive** (#454). Nothing in the running app calls it; the
/// caller is `knobas-app`'s `tests/it/backup_ipc.rs`, and it is `pub` for that
/// reason and no other. That is not a test-only convenience but the only
/// reading of the claim that means anything: a `TABLE DATA` entry in the table
/// of contents says a table's rows are in the file and says nothing whatever
/// about what is in them, and an argument list cannot witness itself.
///
/// The whole script comes back as a `String`, so this is for reading an
/// archive, not for streaming a large one anywhere.
///
/// # Errors
///
/// [`BackupError::ToolMissing`] or [`BackupError::Tool`] if `pg_restore` will
/// not read the file.
pub async fn archive_sql(archive: &Path) -> Result<String, BackupError> {
    // Neither a server nor a password: the script is rendered from the file.
    let mut command = tool_command(PG_RESTORE).await?;
    command.arg("--file=-").arg(archive);
    let output = run(PG_RESTORE, command).await?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The [`BOOKKEEPING_TABLES`] whose rows `archive` carries in [`OWNED_SCHEMA`].
///
/// A **data** entry, not a declaration: a share export's DDL and its contents
/// are the same list today, but "the restore will write this table" is the
/// question being asked, and `TABLE` without `TABLE DATA` answers it *no*.
///
/// One `pg_restore --list` for all four, read before [`restore`] clears
/// anything: a listing that failed between two clears would return with no
/// put-back (#1635).
///
/// # Errors
///
/// Whatever [`archive_contents`] refuses with.
async fn carried_bookkeeping(archive: &Path) -> Result<Vec<&'static str>, BackupError> {
    let contents = archive_contents(archive).await?;
    Ok(BOOKKEEPING_TABLES
        .iter()
        .copied()
        .filter(|table| {
            contents.iter().any(|entry| {
                entry.kind == "TABLE DATA" && entry.schema == OWNED_SCHEMA && entry.name == *table
            })
        })
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
/// "Holds rows" means *content*; the [`BOOKKEEPING_TABLES`] are excluded and
/// replaced by the archive's own, for the reason recorded there.
///
/// # A partial archive
///
/// An archive missing tables restores as readily as a whole one (#454): the
/// share export of spec #427 is the same custom-format dump restricted to the
/// parts somebody chose, and `pg_restore` writes what the archive holds. The
/// occupancy refusal is unchanged and applies to a partial archive exactly as
/// it does to a full one -- a share export is not a merge either. The one
/// thing that has to notice the difference is the `knobas.setting` clear
/// above, which is now conditional on the archive carrying that table.
///
/// # Foreign keys
///
/// `--disable-triggers`. A data-only restore loads tables in the archive's
/// order, which is *not* a topological order for foreign keys -- `knobas.context`
/// references `knobas.entity` and sorts before it -- so the constraints are held
/// off for the load. This is the documented use of the flag and needs a
/// superuser, which the embedded server's connection is.
///
/// Held off means *not checked*: PostgreSQL does not check the loaded rows
/// against a key when its triggers come back on. So the restore checks every
/// key of the schema itself once the load is in (#1930,
/// [`ORPHANS_A_RESTORE_DELETES`] and [`BROKEN_KEYS`]). The rows a key's own
/// migration deletes are deleted, and any other broken key refuses the restore
/// with [`BackupError::BrokenKey`], which takes the loaded rows out again.
///
/// # A failed load
///
/// Leaves the bookkeeping rows as they were (#1635): they are read before the
/// clear and put back when `pg_restore` fails, and the rename shim's column is
/// dropped either way. The load's own error is returned after that. Nothing
/// else needs recovering: every other table was empty, and `pg_restore`'s
/// single transaction leaves it so. A load refused for a broken key ends the
/// same way: its transaction has committed by then, so the check empties
/// every table it wrote before the bookkeeping rows are put back.
///
/// # Errors
///
/// [`BackupError::TargetNotMigrated`], [`BackupError::TargetNotEmpty`],
/// [`BackupError::ToolMissing`], [`BackupError::Tool`],
/// [`BackupError::BrokenKey`]; [`BackupError::Sqlx`]
/// when a statement around the load fails; [`BackupError::NotPutBack`], naming
/// the load's error too, when the load failed and putting the bookkeeping rows
/// back failed after it.
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
    // rows here are the ones this knobas wrote about itself. A full archive
    // carries its own, keyed the same way, and a data-only load would collide
    // on the primary key -- `--disable-triggers` holds off *triggers*, not a
    // unique index. The archive's settings are the user's; these are a default
    // this machine invented on its way to asking for them back.
    //
    // **Only when the archive actually carries them** (#454). A share export
    // is restricted to the parts somebody chose and never includes
    // `knobas.setting`, so clearing the table unconditionally would delete
    // this machine's own schedule, first-run flag and retention stamp and put
    // nothing back in their place -- a restore that quietly resets the
    // settings of the knobas doing the restoring.
    //
    // The clears autocommit here, before `pg_restore` opens its own
    // transaction on another connection, so a failed load cannot roll them
    // back (#1635). The rows are read first and put back by
    // `put_back_bookkeeping` when the load fails (spec #1619, ruling B3).
    let carried = carried_bookkeeping(archive).await?;
    let snapshot = snapshot_bookkeeping(&mut conn).await?;
    for table in carried {
        // Both halves of the name are crate constants; nothing from a
        // caller reaches this string, which is what `AssertSqlSafe` is
        // asserting (the same use `test_util::scratch_database` makes of it).
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "delete from {OWNED_SCHEMA}.{table}"
        )))
        .execute(&mut conn)
        .await?;
    }
    // An archive taken before a column was renamed names the old column in
    // its `COPY`, and a data-only load into the renamed table fails the whole
    // restore. The old name stands for the length of the load and is folded
    // into the new one after it, whether the load worked or not.
    for (table, old, _, ty) in RENAMED_COLUMNS {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "alter table {OWNED_SCHEMA}.{table} add column if not exists {old} {ty}"
        )))
        .execute(&mut conn)
        .await?;
    }
    let _ = conn.close().await;

    let loaded = load(connector, archive).await;
    let mut conn = connector.connect().await?;
    // The keys the load held off, checked before anything else reads the
    // rows; a refusal takes the loaded rows out and falls through to the
    // putting back below, exactly as a failed load does.
    let loaded = match loaded {
        Ok(()) => hold_to_keys(&mut conn).await,
        failed => failed,
    };
    // The rows first: a fold-up that failed after them would leave a column
    // the next restore drops anyway, while rows lost here have no way back.
    let loaded = match loaded {
        Err(load) => {
            if let Err(source) = put_back_bookkeeping(&mut conn, &snapshot).await {
                return Err(BackupError::NotPutBack {
                    load: Box::new(load),
                    source,
                });
            }
            Err(load)
        }
        ok => ok,
    };
    for (table, old, new, _) in RENAMED_COLUMNS {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "update {OWNED_SCHEMA}.{table} set {new} = {old} where {old} is not null"
        )))
        .execute(&mut conn)
        .await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "alter table {OWNED_SCHEMA}.{table} drop column {old}"
        )))
        .execute(&mut conn)
        .await?;
    }
    let _ = conn.close().await;
    loaded
}

/// The rows of a broken key a restore deletes rather than refuses (#1930):
/// the key's name, and the statement that deletes them.
///
/// The rule is the migration's. A restore of an archive taken before a key
/// existed ends where upgrading the database the archive came from would have
/// ended: where the migration that added the key deleted the rows that broke
/// it, the restore deletes them too; where it did not, upgrading would have
/// failed on them, and the restore refuses ([`BackupError::BrokenKey`]).
///
/// One row, `0060`'s orphan rule: a context whose id has no entity row. It
/// has no members and can get none, because membership is a link and both
/// ends of a link are keyed to `knobas.entity`. Every other key in the schema
/// either came with its table or, as `note_entity_fk` (`0006`) and
/// `block_worklog_fk` (`0014`) did, was added with no orphan rule. A note is
/// also the one content knobas cannot fetch again, which is reason enough not
/// to drop one quietly.
const ORPHANS_A_RESTORE_DELETES: &[(&str, &str)] = &[(
    "context_entity_fk",
    "delete from knobas.context c
      where not exists (select 1 from knobas.entity e where e.id = c.id)",
)];

/// Every foreign key of the `knobas` schema, with the number of rows that
/// break it.
///
/// Read off the catalog, as [`OCCUPANCY`] reads the tables, so a key a later
/// migration adds is checked with no change here; and one static statement
/// for the same reason, since each per-key probe is built by PostgreSQL's
/// `format('%I')`. A row breaks a key the way PostgreSQL's own check (`MATCH
/// SIMPLE`) has it: every referencing column set, and no referenced row
/// equal on all of them.
///
/// Only keys *from* `knobas` tables. The load writes nothing else, and a key
/// from a `sync` table into `knobas` (`sync.item` to `knobas.entity`) cannot
/// break: its referencing rows were checked when they were written, and the
/// load only adds to the table they point at.
const BROKEN_KEYS: &str = r"
select c.relname::text as table_name,
       k.conname::text as key_name,
       (xpath(
          '/row/c/text()',
          query_to_xml(
            format('select count(*) as c from %I.%I r where %s and not exists (select 1 from %I.%I t where %s)',
                   n.nspname, c.relname,
                   (select string_agg(format('r.%I is not null', a.attname), ' and ')
                      from unnest(k.conkey) as col(attnum)
                      join pg_attribute a on a.attrelid = k.conrelid and a.attnum = col.attnum),
                   tn.nspname, t.relname,
                   (select string_agg(format('t.%I = r.%I', ta.attname, a.attname), ' and ')
                      from unnest(k.conkey, k.confkey) as col(attnum, target_attnum)
                      join pg_attribute a on a.attrelid = k.conrelid and a.attnum = col.attnum
                      join pg_attribute ta on ta.attrelid = k.confrelid and ta.attnum = col.target_attnum)),
            false, true, '')
       ))[1]::text::bigint as rows
  from pg_constraint k
  join pg_class c on c.oid = k.conrelid
  join pg_namespace n on n.oid = c.relnamespace
  join pg_class t on t.oid = k.confrelid
  join pg_namespace tn on tn.oid = t.relnamespace
 where k.contype = 'f'
   and n.nspname = $1
 order by c.relname, k.conname
";

/// Check every foreign key the load held off (#1930).
///
/// The rows [`ORPHANS_A_RESTORE_DELETES`] names go first; then the first key
/// still broken refuses the restore with [`BackupError::BrokenKey`]. Any
/// refusal, a failed check included, takes every row the load wrote back out
/// before it returns, so a restore that fails leaves the target as a failed
/// load does: its bookkeeping rows are put back by the caller, and every
/// other table is empty, as it was proved to be before the load.
async fn hold_to_keys(conn: &mut sqlx::PgConnection) -> Result<(), BackupError> {
    let checked = delete_orphans_then_check_keys(conn).await;
    if checked.is_err() {
        empty_owned_tables(conn).await?;
    }
    checked
}

/// The check half of [`hold_to_keys`], in one transaction: the orphan deletes
/// are committed only when no key is left broken.
async fn delete_orphans_then_check_keys(conn: &mut sqlx::PgConnection) -> Result<(), BackupError> {
    let mut tx = conn.begin().await?;
    for (key, delete) in ORPHANS_A_RESTORE_DELETES {
        let deleted = sqlx::query(*delete)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if deleted > 0 {
            tracing::warn!(
                key,
                deleted,
                "the restored archive held rows breaking a key; deleted, as its migration does"
            );
        }
    }
    let broken: Option<(String, String, i64)> = sqlx::query_as(BROKEN_KEYS)
        .bind(OWNED_SCHEMA)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .find(|(_, _, rows): &(String, String, i64)| *rows > 0);
    if let Some((table, key, rows)) = broken {
        return Err(BackupError::BrokenKey { table, key, rows });
    }
    tx.commit().await?;
    Ok(())
}

/// Delete every row of every `knobas` table: the archive's rows, taken back
/// out of a target that was empty before the load.
///
/// With triggers off, as the load wrote them (`session_replication_role`, the
/// session-wide form of `--disable-triggers`, and as superuser for the same
/// reason): the rows break a key, so deleting them in key order is not on
/// offer, and nothing the load did not fire should fire on the way out. The
/// bookkeeping tables are emptied too; the caller puts their rows back. Table
/// names are quoted by PostgreSQL's `format('%I.%I')`, not on this side.
async fn empty_owned_tables(conn: &mut sqlx::PgConnection) -> Result<(), sqlx::Error> {
    let mut tx = conn.begin().await?;
    sqlx::raw_sql("set local session_replication_role = replica")
        .execute(&mut *tx)
        .await?;
    let tables: Vec<String> = sqlx::query_scalar(
        "select format('%I.%I', n.nspname, c.relname)
           from pg_class c
           join pg_namespace n on n.oid = c.relnamespace
          where n.nspname = $1 and c.relkind = 'r'",
    )
    .bind(OWNED_SCHEMA)
    .fetch_all(&mut *tx)
    .await?;
    for table in tables {
        // Quoted by `format('%I.%I')` above, and read off the catalog: nothing
        // from a caller or an archive reaches this string.
        sqlx::query(sqlx::AssertSqlSafe(format!("delete from {table}")))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

/// Every row of each of the [`BOOKKEEPING_TABLES`], as the text of a `jsonb`
/// array beside the table it was read from.
///
/// All four, not only the ones the archive carries: clearing `dep_layer`
/// cascades into `dep_layer_kind`, so a table the restore never names can
/// still lose rows. `to_jsonb` and `jsonb_populate_recordset` are the two
/// halves of one row-type round trip, so no column list is written here and
/// a column a later migration adds is carried with no change.
async fn snapshot_bookkeeping(
    conn: &mut sqlx::PgConnection,
) -> Result<Vec<(&'static str, String)>, sqlx::Error> {
    let mut snapshot = Vec::with_capacity(BOOKKEEPING_TABLES.len());
    for table in BOOKKEEPING_TABLES {
        // Crate constants only, as for the clears in `restore`.
        let rows: String = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "select coalesce(jsonb_agg(to_jsonb(t)), '[]'::jsonb)::text from {OWNED_SCHEMA}.{table} t"
        )))
        .fetch_one(&mut *conn)
        .await?;
        snapshot.push((*table, rows));
    }
    Ok(snapshot)
}

/// Put the bookkeeping tables back as [`snapshot_bookkeeping`] read them, in
/// one transaction.
///
/// Each table is cleared first, children before parents, so whatever a failed
/// load left in them gives way to exactly the rows they held; the inserts run
/// parents before children, so `dep_layer_kind` finds its layer.
async fn put_back_bookkeeping(
    conn: &mut sqlx::PgConnection,
    snapshot: &[(&'static str, String)],
) -> Result<(), sqlx::Error> {
    let mut tx = conn.begin().await?;
    for (table, _) in snapshot {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "delete from {OWNED_SCHEMA}.{table}"
        )))
        .execute(&mut *tx)
        .await?;
    }
    for (table, rows) in snapshot.iter().rev() {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "insert into {OWNED_SCHEMA}.{table}
             select * from jsonb_populate_recordset(null::{OWNED_SCHEMA}.{table}, $1::jsonb)"
        )))
        .bind(rows)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Columns of the `knobas` schema a migration renamed after archives were
/// already being taken: table, old name, new name, type.
///
/// `0053` (#1245) renamed `ticket_view.jira_ref` to `saved_view`, and every
/// backup and share export taken before it carries `knobas.ticket_view` with
/// the old column (a `COPY` names its columns, even for an empty table).
/// Without this row such an archive no longer restores at all. A later rename
/// of a `knobas` column adds its row here.
const RENAMED_COLUMNS: &[(&str, &str, &str, &str)] =
    &[("ticket_view", "jira_ref", "saved_view", "text")];

/// The `pg_restore` half of [`restore`]: the archive's `knobas` data, loaded
/// into the migrated database `connector` names.
async fn load(connector: &Connector, archive: &Path) -> Result<(), BackupError> {
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
   and c.relname <> all($2::text[])
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
        .bind(BOOKKEEPING_TABLES)
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
    let mut command = tool_command(tool).await?;
    command.args(args);
    // The connection, and in particular the password, reaches the child in its
    // *environment* and never in `argv` -- see `Connector::libpq_env`.
    for (key, value) in connector.libpq_env() {
        command.env(key, value);
    }
    run(tool, command).await
}

/// The bundled `tool` as a command with no arguments yet.
///
/// Looking the tool up stats the installation directory, so it runs under
/// `spawn_blocking`; a caller dropped meanwhile does not cut the lookup off. A
/// lookup task that panicked or was cancelled is a [`BackupError::Spawn`].
async fn tool_command(tool: &str) -> Result<tokio::process::Command, BackupError> {
    let name = tool.to_owned();
    let found = tokio::task::spawn_blocking(move || locate(&name))
        .await
        .map_err(|error| BackupError::Spawn {
            tool: tool.to_owned(),
            source: io::Error::other(error),
        })??;
    Ok(tokio::process::Command::new(found))
}

/// Run `command`, turning a non-zero exit into a [`BackupError::Tool`]
/// carrying its stderr.
///
/// `Command::output()` forks on the calling worker, so the spawn runs under
/// `spawn_blocking` and the wait stays async. A dropped caller does not cut the
/// spawn off; the child is dropped with its result (no `kill_on_drop`, as
/// before). A spawn task that panicked or was cancelled is a
/// [`BackupError::Spawn`].
async fn run(tool: &str, mut command: tokio::process::Command) -> Result<Output, BackupError> {
    let spawn_error = |source| BackupError::Spawn {
        tool: tool.to_owned(),
        source,
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = tokio::task::spawn_blocking(move || command.spawn())
        .await
        .map_err(|error| spawn_error(io::Error::other(error)))?
        .map_err(spawn_error)?;
    let output = child.wait_with_output().await.map_err(spawn_error)?;
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
