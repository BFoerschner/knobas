//! Against the binary's own server: a database recycled from one a test used
//! is compared with an untouched copy of the same template.

use std::sync::atomic::{AtomicU32, Ordering};

use sqlx::{AssertSqlSafe, PgConnection};

use super::{Returned, Shelf, lease, recycle};
use crate::embedded::Connector;
use crate::test_util::{create_database, shared_server, template};

/// A name no other test in this binary uses.
fn unique(label: &str) -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    format!(
        "knobas_recycle_{label}_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// The server, and a fresh copy of the template, named after `label`.
async fn fresh(label: &str) -> (&'static Connector, Returned) {
    let server = &shared_server().await.connector;
    let template = template(server).await;
    let name = unique(label);
    create_database(server, &name, Some(&template.name)).await;
    (
        server,
        Returned {
            name,
            snapshot: template.snapshot.clone(),
        },
    )
}

async fn connect(server: &Connector, name: &str) -> PgConnection {
    server
        .with_database(name)
        .connect()
        .await
        .expect("connect to a test database")
}

/// `returned` recycled under a new name, which is returned.
async fn recycled(server: &Connector, returned: &Returned) -> Option<String> {
    let name = unique("again");
    recycle(server, returned, &name, template(server).await)
        .await
        .expect("recycle")
        .map(|_| name)
}

/// One text answer of `sql`.
async fn text(conn: &mut PgConnection, sql: &'static str) -> String {
    sqlx::query_scalar::<_, Option<String>>(sql)
        .fetch_one(conn)
        .await
        .expect(sql)
        .unwrap_or_default()
}

/// What a test sees of the state a database starts in: the rows of every
/// table the migrations seed, a table they leave empty, what the next
/// identity value is, and the planner's statistics.
async fn observed(conn: &mut PgConnection) -> Vec<(&'static str, String)> {
    let mut seen = Vec::new();
    for sql in [
        "select string_agg(t::text, ';' order by t::text) from knobas.dep_layer t",
        "select string_agg(t::text, ';' order by t::text) from knobas.dep_layer_kind t",
        "select string_agg(t::text, ';' order by t::text) from knobas.dep_view t",
        "select count(*)::text from knobas.setting",
        "select string_agg(relname || '=' || reltuples || '/' || relpages, ',' order by relname)
           from pg_class where relnamespace in ('knobas'::regnamespace, 'sync'::regnamespace)",
        "select count(*)::text from pg_stats where schemaname in ('knobas', 'sync')",
        "select count(*)::text from pg_stat_user_tables
          where last_analyze is not null or n_tup_ins > 0",
    ] {
        seen.push((sql, text(conn, sql).await));
    }
    let next: i64 = sqlx::query_scalar("insert into knobas.heartbeat default values returning id")
        .fetch_one(conn)
        .await
        .expect("the next heartbeat id");
    seen.push(("the next heartbeat id", next.to_string()));
    seen
}

#[tokio::test]
async fn a_recycled_database_reads_as_a_fresh_copy_does() {
    let (server, used) = fresh("used").await;
    let mut conn = connect(server, &used.name).await;
    sqlx::raw_sql(
        "insert into knobas.setting (key, value) select 'k' || g, '1' from generate_series(1, 500) g;
         insert into knobas.heartbeat default values;
         insert into knobas.heartbeat default values;
         update knobas.dep_view set name = 'renamed by a test';
         delete from knobas.dep_layer where id = 'code';
         analyze;",
    )
    .execute(&mut conn)
    .await
    .expect("use the database the way a test does");
    drop(conn);

    let name = recycled(server, &used)
        .await
        .expect("nothing but rows changed");
    let (_, control) = fresh("control").await;
    let recycled = observed(&mut connect(server, &name).await).await;
    let fresh = observed(&mut connect(server, &control.name).await).await;
    for ((what, recycled), (_, fresh)) in recycled.iter().zip(&fresh) {
        assert_eq!(recycled, fresh, "{what}, recycled and fresh");
    }
}

#[tokio::test]
async fn a_recycled_database_cannot_be_reached_by_its_old_name() {
    let (server, used) = fresh("old_name").await;
    let mut left_over = connect(server, &used.name).await;

    let name = recycled(server, &used).await.expect("recycled");

    assert!(
        sqlx::query("select 1")
            .execute(&mut left_over)
            .await
            .is_err(),
        "a session left over from the test before is cut"
    );
    assert!(
        server.with_database(&used.name).connect().await.is_err(),
        "the old name reaches nothing"
    );
    connect(server, &name).await;
}

#[tokio::test]
async fn a_database_whose_schema_changed_is_not_recycled() {
    for ddl in [
        "alter table knobas.setting add column extra int",
        "create index on knobas.note (title)",
        "create temporary table scratch (x int)",
    ] {
        let (server, used) = fresh("ddl").await;
        sqlx::raw_sql(ddl)
            .execute(&mut connect(server, &used.name).await)
            .await
            .expect(ddl);
        assert_eq!(recycled(server, &used).await, None, "after `{ddl}`");
    }
}

#[tokio::test]
async fn a_database_with_settings_of_its_own_is_not_recycled() {
    let (server, used) = fresh("setting").await;
    sqlx::raw_sql(AssertSqlSafe(format!(
        "alter database {} set work_mem = '8MB'",
        used.name
    )))
    .execute(&mut connect(server, &used.name).await)
    .await
    .expect("a database setting");
    assert_eq!(recycled(server, &used).await, None);
}

#[tokio::test]
async fn a_dropped_database_is_not_recycled() {
    let (server, used) = fresh("dropped").await;
    sqlx::raw_sql(AssertSqlSafe(format!("drop database {}", used.name)))
        .execute(&mut connect(server, crate::embedded::MAINTENANCE_DATABASE).await)
        .await
        .expect("drop it");
    assert_eq!(recycled(server, &used).await, None);
}

/// Whether `shelf` has a database on it, put back if so.
fn on_the_shelf(shelf: &'static Shelf) -> bool {
    shelf.take().map(|returned| shelf.put(returned)).is_some()
}

#[tokio::test]
async fn a_lease_ends_with_the_thread_that_took_it() {
    let shelf: &'static Shelf = Box::leak(Box::new(Shelf::new()));
    let server = &shared_server().await.connector;
    let snapshot = template(server).await.snapshot.clone();
    let (taken, take) = std::sync::mpsc::channel();
    let (finish, finished) = std::sync::mpsc::channel::<()>();

    let thread = std::thread::spawn(move || {
        // A pool's worth of use: the connector itself is gone at once.
        drop(lease(shelf, server, "x".to_owned(), snapshot));
        taken.send(()).expect("report");
        finished.recv().expect("wait to finish");
    });
    take.recv().expect("taken");
    assert!(!on_the_shelf(shelf), "held while the thread runs");
    finish.send(()).expect("finish");
    thread.join().expect("the thread");
    assert!(on_the_shelf(shelf), "returned once it ended");
}

#[tokio::test]
async fn a_kept_connector_keeps_its_database_off_the_shelf() {
    let shelf: &'static Shelf = Box::leak(Box::new(Shelf::new()));
    let server = &shared_server().await.connector;
    let snapshot = template(server).await.snapshot.clone();

    let kept = std::thread::spawn(move || lease(shelf, server, "x".to_owned(), snapshot))
        .join()
        .expect("the thread");
    assert!(!on_the_shelf(shelf), "a `static` holding the connector still uses it");
    drop(kept);
    assert!(on_the_shelf(shelf), "returned with the last clone");
}

/// The whole path, through `scratch_database`: nothing else in this binary
/// takes from its shelf.
#[tokio::test]
async fn a_scratch_database_a_finished_test_held_is_handed_on_empty() {
    let oid = "select oid::int8 from pg_database where datname = current_database()";
    let first: i64 = std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime")
            .block_on(async {
                let pool = crate::test_util::scratch_database("first")
                    .await
                    .pool(2)
                    .await
                    .expect("a pool");
                sqlx::query("insert into knobas.setting (key, value) values ('mine', '1')")
                    .execute(&pool)
                    .await
                    .expect("a row of the first test");
                sqlx::query_scalar(oid).fetch_one(&pool).await.expect("oid")
            })
    })
    .join()
    .expect("the first test");

    let mut second = crate::test_util::scratch_database("second")
        .await
        .connect()
        .await
        .expect("connect");
    let again: i64 = sqlx::query_scalar(oid)
        .fetch_one(&mut second)
        .await
        .expect("oid");
    assert_eq!(again, first, "the same database, handed on");
    assert_eq!(
        text(&mut second, "select count(*)::text from knobas.setting").await,
        "0",
        "with none of the first test's rows"
    );
}
