//! A new cluster's locale does not depend on how knobas was started (#2013).
//!
//! `initdb` takes the locale categories it is not given from its
//! environment. An app started from Finder inherits launchd's, which sets no
//! `LANG` or `LC_*`, so with no flags the cluster came out libc `C`: `lower()`
//! left `Ü` alone, `to_tsvector` indexed `Überweisung` un-lowercased, and
//! `pg_trgm` dropped `Ü` as a non-word character. Each test here re-runs this
//! binary as a child under `LC_ALL=C` and brings a fresh cluster up in it.

use knobas_db::{DbConfig, EmbeddedDb};

/// Set in the child: it brings the cluster up instead of launching itself.
const CHILD: &str = "KNOBAS_TEST_LOCALE_CHILD";

/// Re-run this binary on `test` alone with every locale variable at `C`,
/// as launchd's empty environment leaves them, and return `true`; or, in
/// that child, return `false` so the test goes on.
fn ran_in_a_c_locale_child(test: &str) -> bool {
    if std::env::var_os(CHILD).is_some() {
        return false;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", test, "--nocapture"])
        .env(CHILD, "1")
        .env_remove("KNOBAS_TEST_DB_URL");
    for variable in ["LC_ALL", "LANG", "LC_CTYPE", "LC_COLLATE", "LC_MESSAGES"] {
        child.env(variable, "C");
    }
    let output = child.output().expect("re-run this binary as a child");
    print!("{}", super::stdout_of_a_passed_child(test, &output));
    true
}

/// A fresh cluster in a scratch directory, and the directory.
async fn fresh_cluster() -> (EmbeddedDb, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = EmbeddedDb::start(DbConfig {
        root_dir: dir.path().to_path_buf(),
        existing_url: None,
    })
    .await
    .unwrap();
    (db, dir)
}

#[tokio::test]
async fn a_new_clusters_database_has_the_builtin_c_utf8_locale_whatever_it_was_started_under() {
    let test = "locale::a_new_clusters_database_has_the_builtin_c_utf8_locale_whatever_it_was_started_under";
    if ran_in_a_c_locale_child(test) {
        return;
    }
    let (db, _dir) = fresh_cluster().await;
    let row: Result<(String, String, Option<String>, String, String), _> = sqlx::query_as(
        "select current_database()::text, datlocprovider::text, datlocale, datcollate, datctype
           from pg_database where datname = current_database()",
    )
    .fetch_one(db.pool())
    .await;
    db.stop().await.unwrap();
    let (database, provider, locale, collate, ctype) = row.unwrap();
    println!("{database}: provider {provider}, locale {locale:?}, collate {collate}, ctype {ctype}");
    assert_eq!(database, "knobas", "the app's own database");
    assert_eq!(provider, "b", "the builtin provider");
    assert_eq!(locale.as_deref(), Some("C.UTF-8"));
    // The C library's categories: `C.UTF-8`, which Windows does not have,
    // and a fixed Windows name there.
    let expected = if cfg!(windows) { "en-US" } else { "C.UTF-8" };
    assert_eq!((collate.as_str(), ctype.as_str()), (expected, expected));
}

#[tokio::test]
async fn a_new_cluster_lowercases_and_keeps_non_ascii_letters_whatever_it_was_started_under() {
    let test = "locale::a_new_cluster_lowercases_and_keeps_non_ascii_letters_whatever_it_was_started_under";
    if ran_in_a_c_locale_child(test) {
        return;
    }
    let (db, _dir) = fresh_cluster().await;
    let probe = async {
        // What migration `0049` installs, for the mention rule's index.
        sqlx::query("create extension if not exists pg_trgm")
            .execute(db.pool())
            .await?;
        sqlx::query_as::<_, (String, String, String, i32, i32)>(
            "select lower('ÜÄÖ'), lower('İ'), to_tsvector('english', 'Überweisung')::text,
                    cardinality(show_trgm('Überweisung')), cardinality(show_trgm('berweisung'))",
        )
        .fetch_one(db.pool())
        .await
    }
    .await;
    db.stop().await.unwrap();
    let (umlauts, dotted_i, tsvector, with_u, without_u) = probe.unwrap();
    assert_eq!(umlauts, "üäö", "lower() folds umlauts");
    assert_eq!(dotted_i, "i", "lower() folds İ as #1843's key matching expects");
    assert_eq!(tsvector, "'überweisung':1", "the launcher's index is lowercased");
    assert!(
        with_u > without_u,
        "pg_trgm keeps Ü's trigrams: {with_u} for Überweisung, {without_u} for berweisung"
    );
}
