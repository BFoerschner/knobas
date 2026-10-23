//! A scratch database handed on to the next test instead of a new one per
//! test (#1897).
//!
//! A gate used to create about 930 databases, one `create database ...
//! template` per [`scratch_database`](super::scratch_database) call, and the
//! files of each -- several hundred relations' worth, 15 MB -- were most of
//! what a test run cost the machine. A database a test is done with now goes
//! on a [`Shelf`]; the next caller takes it from there, puts it back into the
//! state a fresh copy of the template has, and only creates one when the shelf
//! is empty or what is on it cannot be put back.
//!
//! # When a test is done with it
//!
//! A [`lease`] is held by the thread that asked and by every clone of the
//! connector it returned, and the database goes on the shelf when the last of
//! them lets go. The thread is what covers the common shape, a test that turns
//! the connector into a pool and drops it; the clones cover a connector a
//! `static` keeps for every test of a file (`tests/it/estate_exit.rs`), which
//! is then never handed on. A `#[tokio::test]` runs on its own thread, and a
//! multi-threaded one's workers are joined when its runtime is dropped, so
//! either way the thread ends with the test.
//!
//! # Why nobody from before can reach it
//!
//! The taker terminates every session still on the database and renames it
//! to a name of its own ([`take_over`]) before it looks inside. A connector, a
//! pool or a child process left over from the test before names a database
//! that no longer exists, so it fails rather than writing into the next
//! test's rows.
//!
//! # What "the state a fresh copy has" means
//!
//! A [`Snapshot`]: every system catalog's rows by `ctid` and `xmin`, plus the
//! database's own settings, and per user table its sizes, its planner
//! statistics and -- for a table the migrations wrote rows into -- its
//! contents; per sequence its position. Any DDL inserts, updates or deletes a
//! catalog row and changes the first part, and a database whose catalogs
//! changed is not reused: there is no telling what else to undo. `analyze` and
//! `vacuum` update `pg_class` in place, so they do not count as DDL; the
//! statistics they leave are cleared instead, because the plan tests
//! (`knobas_db::plan_shape`) assert on a mirror never analyzed. Every table
//! whose part differs is truncated, the template's rows go back into the ones
//! that had some, the sequences are set back, and the cumulative statistics
//! are reset.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, PoisonError};

use sqlx::{AssertSqlSafe, Connection, PgConnection, Row};

use crate::embedded::{Connector, MAINTENANCE_DATABASE};

/// How often a taker terminates the sessions on a returned database and tries
/// the rename again, when a session got back in between.
const TAKE_OVER_ATTEMPTS: usize = 5;

/// What a database is compared by; see the module docs.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Snapshot {
    /// The system catalogs' rows and the database's own settings, hashed.
    catalog: String,
    /// Every user table, by oid.
    tables: Vec<Table>,
    /// Every sequence, by oid.
    sequences: Vec<Sequence>,
    /// The planner's numbers for every user table and index, by oid.
    stats: Vec<Stats>,
}

/// One user table's physical and statistical state.
#[derive(Clone, Debug, PartialEq)]
struct Table {
    oid: i64,
    /// Schema-qualified and quoted, ready for a statement.
    name: String,
    /// `pg_class.relkind`: `r`, or `m` for a materialized view.
    kind: String,
    /// The heap's main fork, in bytes: zero for a table nothing was written to.
    main: i64,
    /// Every fork of the heap and its TOAST table.
    size: i64,
    indexes: i64,
    pages: i32,
    tuples: f32,
    visible: i32,
    /// Whether `pg_statistic` has a row for it or one of its indexes.
    analyzed: bool,
    /// Its rows, hashed, for a table the template has rows in.
    content: Option<String>,
}

/// One relation's planner numbers in `pg_class`. `truncate` sets an index's
/// to what a table's start as, not to what `create index` left.
#[derive(Clone, Debug, PartialEq)]
struct Stats {
    oid: i64,
    schema: String,
    relation: String,
    pages: i32,
    tuples: f32,
    visible: i32,
    frozen: i32,
}

/// One sequence's position.
#[derive(Clone, Debug, PartialEq)]
struct Sequence {
    oid: i64,
    start: i64,
    /// `None` before the first `nextval`.
    last: Option<i64>,
}

/// The rows the migrations left in one table.
struct Seed {
    oid: i64,
    name: String,
    /// The columns an `insert` may name: every one that is not generated.
    columns: String,
    /// The rows, as a JSON array of objects.
    rows: String,
}

/// This process's migrated template, and what a fresh copy of it looks like.
pub(super) struct Template {
    pub(super) name: String,
    pub(super) snapshot: Snapshot,
    seeds: Vec<Seed>,
    /// The statement hashing the catalogs, built once from their list.
    catalog_query: String,
}

/// The template `name`, read over `conn`, a session on it.
///
/// # Errors
///
/// Any error of the catalog reads.
pub(super) async fn capture(
    conn: &mut PgConnection,
    name: String,
) -> Result<Template, sqlx::Error> {
    let catalog_query = catalog_query(conn).await?;
    let unseeded = snapshot(conn, &catalog_query, &[]).await?;
    let seeded: Vec<i64> = unseeded
        .tables
        .iter()
        .filter(|t| t.main > 0)
        .map(|t| t.oid)
        .collect();
    let mut seeds = Vec::new();
    for table in unseeded.tables.iter().filter(|t| t.main > 0) {
        let rows: String = sqlx::query_scalar(AssertSqlSafe(format!(
            "select coalesce(json_agg(t), '[]')::text from {} t",
            table.name
        )))
        .fetch_one(&mut *conn)
        .await?;
        let columns: String = sqlx::query_scalar(
            "select coalesce(string_agg(quote_ident(attname), ', ' order by attnum), '')
               from pg_attribute
              where attrelid = $1::oid and attnum > 0 and not attisdropped
                and attgenerated = ''",
        )
        .bind(table.oid)
        .fetch_one(&mut *conn)
        .await?;
        seeds.push(Seed {
            oid: table.oid,
            name: table.name.clone(),
            columns,
            rows,
        });
    }
    let snapshot = snapshot(conn, &catalog_query, &seeded).await?;
    Ok(Template {
        name,
        snapshot,
        seeds,
        catalog_query,
    })
}

/// The statement hashing every system catalog of a database by `ctid` and
/// `xmin`, and the database's own row and settings.
///
/// `pg_statistic` and `pg_statistic_ext_data` are left out: `analyze` writes
/// them without any DDL, and [`reset`] clears them anyway. Shared catalogs
/// are left out because they hold every database's rows; the three things in
/// them that belong to this database are added by hand. The database's name
/// and oid are not part of it, so a copy hashes as its template does.
async fn catalog_query(conn: &mut PgConnection) -> Result<String, sqlx::Error> {
    let catalogs: String = sqlx::query_scalar(
        "select string_agg(format(
                  'select %L as catalog, count(*) as rows,
                          coalesce(sum(hashtext(ctid::text || '':'' || xmin::text)), 0) as hash
                     from pg_catalog.%I', relname, relname),
                ' union all ' order by relname)
           from pg_class
          where relnamespace = 'pg_catalog'::regnamespace and relkind = 'r'
            and not relisshared
            and relname not in ('pg_statistic', 'pg_statistic_ext_data')",
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(format!(
        "select md5(string_agg(catalog || ' ' || rows || ' ' || hash, ',' order by catalog))
           from ({catalogs}
                 union all
                 select 'pg_database', count(*), coalesce(sum(hashtext(row(
                          d.datdba, d.datallowconn, d.datconnlimit, d.datacl,
                          d.dattablespace, d.datistemplate)::text)), 0)
                   from pg_database d where d.datname = current_database()
                 union all
                 select 'pg_db_role_setting', count(*), 0 from pg_db_role_setting
                  where setdatabase = (select oid from pg_database
                                        where datname = current_database())
                 union all
                 select 'pg_shdescription', count(*), 0 from pg_shdescription
                  where classoid = 'pg_database'::regclass
                    and objoid = (select oid from pg_database
                                   where datname = current_database())) s"
    ))
}

/// The database `conn` is on, with the contents of the tables in `seeded`.
async fn snapshot(
    conn: &mut PgConnection,
    catalog_query: &str,
    seeded: &[i64],
) -> Result<Snapshot, sqlx::Error> {
    let catalog: String = sqlx::query_scalar(AssertSqlSafe(catalog_query))
        .fetch_one(&mut *conn)
        .await?;
    let tables = sqlx::query(
        r"select c.oid::int8 as oid, format('%I.%I', n.nspname, c.relname) as name,
                 c.relkind::text as kind, pg_relation_size(c.oid) as main,
                 pg_table_size(c.oid) as size, pg_indexes_size(c.oid) as indexes,
                 c.relpages as pages, c.reltuples as tuples, c.relallvisible as visible,
                 exists (select from pg_statistic s
                          where s.starelid = c.oid
                             or s.starelid in (select i.indexrelid from pg_index i
                                                where i.indrelid = c.oid)) as analyzed,
                 case when c.oid::int8 = any($1) then
                   (xpath('/row/h/text()', query_to_xml(format(
                      'select md5(coalesce(string_agg(t::text, %L order by t::text), %L)) as h
                         from %s t', E'\n', '', c.oid::regclass),
                      false, true, '')))[1]::text
                 end as content
            from pg_class c join pg_namespace n on n.oid = c.relnamespace
           where c.relkind in ('r', 'm')
             and n.nspname <> 'information_schema' and n.nspname !~ '^pg_'
           order by c.oid",
    )
    .bind(seeded)
    .fetch_all(&mut *conn)
    .await?
    .iter()
    .map(|row| {
        Ok(Table {
            oid: row.try_get("oid")?,
            name: row.try_get("name")?,
            kind: row.try_get("kind")?,
            main: row.try_get("main")?,
            size: row.try_get("size")?,
            indexes: row.try_get("indexes")?,
            pages: row.try_get("pages")?,
            tuples: row.try_get("tuples")?,
            visible: row.try_get("visible")?,
            analyzed: row.try_get("analyzed")?,
            content: row.try_get("content")?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let sequences = sqlx::query(
        "select seqrelid::int8 as oid, seqstart as start,
                pg_sequence_last_value(seqrelid) as last
           from pg_sequence order by seqrelid",
    )
    .fetch_all(&mut *conn)
    .await?
    .iter()
    .map(|row| {
        Ok(Sequence {
            oid: row.try_get("oid")?,
            start: row.try_get("start")?,
            last: row.try_get("last")?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(Snapshot {
        catalog,
        tables,
        sequences,
        stats: stats(conn).await?,
    })
}

/// The planner's numbers for every user table and index. TOAST tables are
/// left out: `pg_restore_relation_stats` refuses them, and no plan reads them.
async fn stats(conn: &mut PgConnection) -> Result<Vec<Stats>, sqlx::Error> {
    sqlx::query(
        "select c.oid::int8 as oid, n.nspname::text as schema, c.relname::text as relation,
                c.relpages as pages, c.reltuples as tuples, c.relallvisible as visible,
                c.relallfrozen as frozen
           from pg_class c join pg_namespace n on n.oid = c.relnamespace
          where c.relkind in ('r', 'm', 'i')
            and n.nspname <> 'information_schema' and n.nspname !~ '^pg_'
          order by c.oid",
    )
    .fetch_all(&mut *conn)
    .await?
    .iter()
    .map(|row| {
        Ok(Stats {
            oid: row.try_get("oid")?,
            schema: row.try_get("schema")?,
            relation: row.try_get("relation")?,
            pages: row.try_get("pages")?,
            tuples: row.try_get("tuples")?,
            visible: row.try_get("visible")?,
            frozen: row.try_get("frozen")?,
        })
    })
    .collect()
}

/// A database a test is done with: its name and what it looked like when
/// that test got it.
pub(super) struct Returned {
    pub(super) name: String,
    pub(super) snapshot: Snapshot,
}

/// The databases no test holds any more.
pub(super) struct Shelf(Mutex<Vec<Returned>>);

impl Shelf {
    pub(super) const fn new() -> Shelf {
        Shelf(Mutex::new(Vec::new()))
    }

    /// One returned database, if there is one.
    pub(super) fn take(&self) -> Option<Returned> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).pop()
    }

    fn put(&self, returned: Returned) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(returned);
    }
}

/// One database a test holds; on the shelf when the last holder drops it.
struct Lease {
    shelf: &'static Shelf,
    returned: Option<Returned>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(returned) = self.returned.take() {
            self.shelf.put(returned);
        }
    }
}

thread_local! {
    /// The leases this thread took, dropped when it ends.
    static HELD: RefCell<Vec<Arc<Lease>>> = const { RefCell::new(Vec::new()) };
}

/// `connector`, onto database `name` as `snapshot` describes it, leased to the
/// calling thread and to the connector's clones; see the module docs.
pub(super) fn lease(
    shelf: &'static Shelf,
    connector: Connector,
    name: String,
    snapshot: Snapshot,
) -> Connector {
    let lease = Arc::new(Lease {
        shelf,
        returned: Some(Returned { name, snapshot }),
    });
    // Fails only on a thread already tearing its locals down; the clones then
    // hold it alone.
    let _ = HELD.try_with(|held| held.borrow_mut().push(Arc::clone(&lease)));
    connector.keeping(lease)
}

/// `returned`, under the name `name` and as a fresh copy of `template`
/// looks; `None` when it cannot be put back -- DDL ran in it, or a test
/// dropped it. A database declined is left where it is, for the scratch
/// root's removal like any other.
///
/// # Errors
///
/// Any error but those, for the caller to report before creating a database
/// instead.
pub(super) async fn recycle(
    server: &Connector,
    returned: &Returned,
    name: &str,
    template: &Template,
) -> Result<Option<Snapshot>, sqlx::Error> {
    if !take_over(server, &returned.name, name).await? {
        return Ok(None);
    }
    let mut conn = server.with_database(name).connect().await?;
    let outcome = reset(&mut conn, &returned.snapshot, template).await;
    let _ = conn.close().await;
    outcome
}

/// Database `from` renamed to `to`, once no session is left on it; false when
/// it no longer exists or sessions kept coming back.
async fn take_over(server: &Connector, from: &str, to: &str) -> Result<bool, sqlx::Error> {
    let mut admin = server.with_database(MAINTENANCE_DATABASE).connect().await?;
    let mut outcome = Ok(false);
    for _ in 0..TAKE_OVER_ATTEMPTS {
        let terminated = sqlx::query(
            "select pg_terminate_backend(pid, 5000) from pg_stat_activity
              where datname = $1 and pid <> pg_backend_pid()",
        )
        .bind(from)
        .execute(&mut admin)
        .await;
        if let Err(error) = terminated {
            outcome = Err(error);
            break;
        }
        // Both names are this module's own: a slug, a pid and a counter.
        let renamed = sqlx::raw_sql(AssertSqlSafe(format!(
            "alter database {from} rename to {to}"
        )))
        .execute(&mut admin)
        .await;
        match renamed {
            Ok(_) => {
                outcome = Ok(true);
                break;
            }
            // `object_in_use`: a session got in between the two statements.
            Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("55006") => {}
            // `invalid_catalog_name`: the test dropped it.
            Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("3D000") => {
                break;
            }
            Err(error) => {
                outcome = Err(error);
                break;
            }
        }
    }
    let _ = admin.close().await;
    outcome
}

/// The database `conn` is on, last seen as `before`, put back into
/// `template`'s state; its snapshot afterwards, or `None` if its catalogs
/// changed.
async fn reset(
    conn: &mut PgConnection,
    before: &Snapshot,
    template: &Template,
) -> Result<Option<Snapshot>, sqlx::Error> {
    let seeded: Vec<i64> = template.seeds.iter().map(|seed| seed.oid).collect();
    let now = snapshot(conn, &template.catalog_query, &seeded).await?;
    let same_relations = now.tables.len() == before.tables.len()
        && now.tables.iter().zip(&before.tables).all(|(n, b)| n.oid == b.oid)
        && now.sequences.len() == before.sequences.len()
        && now
            .sequences
            .iter()
            .zip(&before.sequences)
            .all(|(n, b)| n.oid == b.oid)
        && now.stats.len() == before.stats.len()
        && now.stats.iter().zip(&before.stats).all(|(n, b)| n.oid == b.oid);
    if now.catalog != before.catalog || !same_relations {
        return Ok(None);
    }
    let dirty: Vec<&Table> = now
        .tables
        .iter()
        .zip(&before.tables)
        .filter(|(n, b)| n != b)
        .map(|(n, _)| n)
        .collect();
    if dirty.iter().any(|table| table.kind != "r") {
        return Ok(None);
    }

    let mut tx = conn.begin().await?;
    // No triggers, foreign keys included: the template's rows go back in
    // whatever order, and nothing derives rows from them a second time.
    sqlx::raw_sql("set local session_replication_role = replica")
        .execute(&mut *tx)
        .await?;
    if !dirty.is_empty() {
        let names: Vec<&str> = dirty.iter().map(|table| table.name.as_str()).collect();
        sqlx::raw_sql(AssertSqlSafe(format!(
            "truncate {} cascade",
            names.join(", ")
        )))
        .execute(&mut *tx)
        .await?;
    }
    // Truncated, or emptied by a cascade from one that was.
    let emptied: Vec<i64> = sqlx::query_scalar(
        "select oid::int8 from pg_class where oid::int8 = any($1) and pg_relation_size(oid) = 0",
    )
    .bind(&seeded)
    .fetch_all(&mut *tx)
    .await?;
    for seed in template
        .seeds
        .iter()
        .filter(|seed| emptied.contains(&seed.oid))
    {
        sqlx::query(AssertSqlSafe(format!(
            "insert into {name} ({columns}) overriding system value
             select {columns} from json_populate_recordset(null::{name}, $1::json)",
            name = seed.name,
            columns = seed.columns,
        )))
        .bind(&seed.rows)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::raw_sql(
        "delete from pg_statistic s using pg_class c join pg_namespace n on n.oid = c.relnamespace
          where s.starelid = c.oid
            and n.nspname <> 'information_schema' and n.nspname !~ '^pg_'",
    )
    .execute(&mut *tx)
    .await?;
    // In-transaction `stats` sees what the truncation left.
    for (stats, was) in stats(&mut tx).await?.iter().zip(&before.stats) {
        if stats != was {
            sqlx::query(
                "select pg_restore_relation_stats(
                   'schemaname', $1::text, 'relname', $2::text, 'relpages', $3::int,
                   'reltuples', $4::real, 'relallvisible', $5::int, 'relallfrozen', $6::int)",
            )
            .bind(&was.schema)
            .bind(&was.relation)
            .bind(was.pages)
            .bind(was.tuples)
            .bind(was.visible)
            .bind(was.frozen)
            .execute(&mut *tx)
            .await?;
        }
    }
    for (sequence, was) in now.sequences.iter().zip(&before.sequences) {
        if sequence != was {
            sqlx::query("select setval($1::oid::regclass, $2, $3)")
                .bind(was.oid)
                .bind(was.last.unwrap_or(was.start))
                .bind(was.last.is_some())
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;

    let after = snapshot(conn, &template.catalog_query, &seeded).await?;
    // This session's own counts first, or they would land after the reset.
    sqlx::raw_sql("select pg_stat_force_next_flush()")
        .execute(&mut *conn)
        .await?;
    sqlx::raw_sql("select pg_stat_reset()")
        .execute(&mut *conn)
        .await?;
    Ok(Some(after))
}

#[cfg(test)]
mod tests;
