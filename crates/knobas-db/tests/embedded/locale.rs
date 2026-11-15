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
    let env = [
        (CHILD, "1"),
        ("LC_ALL", "C"),
        ("LANG", "C"),
        ("LC_CTYPE", "C"),
        ("LC_COLLATE", "C"),
        ("LC_MESSAGES", "C"),
    ];
    let (_, output) = super::run_a_child_with(test, None, &env);
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

/// The app's database on a new cluster is the builtin provider's `C.UTF-8`,
/// with the C library's collation and ctype fixed per platform, not `C`.
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
    println!(
        "{database}: provider {provider}, locale {locale:?}, collate {collate}, ctype {ctype}"
    );
    assert_eq!(database, "knobas", "the app's own database");
    assert_eq!(provider, "b", "the builtin provider");
    assert_eq!(locale.as_deref(), Some("C.UTF-8"));
    // The C library's categories: `C.UTF-8`, which Windows does not have,
    // and a fixed Windows name there.
    let expected = if cfg!(windows) { "en-US" } else { "C.UTF-8" };
    assert_eq!((collate.as_str(), ctype.as_str()), (expected, expected));
}

/// A new cluster folds `Ü` and `İ` in `lower()`, lowercases the launcher's
/// `tsvector`, and keeps `Ü`'s trigrams, which `pg_trgm` takes from the C
/// library's ctype rather than the builtin provider.
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
    assert_eq!(
        dotted_i, "i",
        "lower() folds İ as #1843's key matching expects"
    );
    assert_eq!(
        tsvector, "'überweisung':1",
        "the launcher's index is lowercased"
    );
    assert!(
        with_u > without_u,
        "pg_trgm keeps Ü's trigrams: {with_u} for Überweisung, {without_u} for berweisung"
    );
}

/// The server writes its messages and its log in English whatever language
/// the machine is set to (the ux12 Windows pass, 2026-11-15: Björn's
/// server log was German, `Dauer:` for `duration:`). knobas reads one thing
/// out of the server's words, the `FATAL:` line of a failed start
/// (`start_reason`), and every error it acts on by its SQLSTATE; the start
/// log a user sends with a report should read the same everywhere. `initdb`
/// records the language of its environment as the cluster's `lc_messages`,
/// so a cluster made under a German one spoke German until the server is
/// told `C` at every start.
#[tokio::test]
async fn the_server_speaks_c_whatever_language_it_was_started_under() {
    let test = "locale::the_server_speaks_c_whatever_language_it_was_started_under";
    if std::env::var_os(CHILD).is_none() {
        let env = [
            (CHILD, "1"),
            ("LC_ALL", "de_DE.UTF-8"),
            ("LANG", "de_DE.UTF-8"),
            ("LC_MESSAGES", "de_DE.UTF-8"),
        ];
        let (_, output) = super::run_a_child_with(test, None, &env);
        print!("{}", super::stdout_of_a_passed_child(test, &output));
        return;
    }
    let (db, dir) = fresh_cluster().await;
    let setting: Result<(String,), _> = sqlx::query_as("show lc_messages")
        .fetch_one(db.pool())
        .await;
    db.stop().await.unwrap();
    let conf = std::fs::read_to_string(dir.path().join("data/postgresql.conf")).unwrap();
    let recorded = conf
        .lines()
        .find_map(|line| line.strip_prefix("lc_messages"))
        .and_then(|rest| rest.split('#').next())
        .map(|value| {
            value
                .trim()
                .trim_start_matches('=')
                .trim()
                .trim_matches('\'')
        })
        .unwrap_or_default()
        .to_owned();
    println!("lc_messages {setting:?}; postgresql.conf: {recorded}");
    assert_eq!(setting.unwrap().0, "C", "the running server's");
    assert_eq!(
        recorded, "C",
        "the cluster's own, for a server started without knobas"
    );
}
