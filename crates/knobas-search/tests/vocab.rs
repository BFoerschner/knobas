//! The vocabulary the launcher grammar parses against, read from a real
//! PostgreSQL.
//!
//! Every test in this binary shares one database (`knobas_db::test_util`), so
//! each seeds source ids unique to *itself* and nothing truncates. That matters
//! more here than elsewhere: `Vocabulary::load` reads **every** configured
//! source -- disabled ones included, since #200 -- so a row another test in
//! this run left behind is a row this one sees, whether or not it turned it
//! off. Which is also why the prefixes are per-test (`vta-`, `vtd-`, `vtb-`,
//! `vti-`) rather
//! than one shared `vt-`. The database itself is fresh per run:
//! `test_util::run_nonce` is `{pid}-{nanos}`, so an earlier run's scratch
//! directory can never match this process's stamp and is deleted.

use knobas_search::{KindCatalog, Vocabulary};

/// Insert (or re-enable) one source configuration.
async fn seed_source(pool: &sqlx::PgPool, id: &str, adapter_kind: &str, name: &str, cfg: &str) {
    sqlx::query(
        "insert into knobas.source_config
           (id, kind, display_name, base_url, auth_kind, config)
         values ($1, $2, $3, 'http://x', 'Pat', $4::jsonb)
         on conflict (id) do update
            set kind = excluded.kind,
                display_name = excluded.display_name,
                config = excluded.config,
                enabled = true",
    )
    .bind(id)
    .bind(adapter_kind)
    .bind(name)
    .bind(cfg)
    .execute(pool)
    .await
    .unwrap();
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn aliases_resolve_across_instances_and_identity_comes_from_the_configs() {
    let pool = pool().await;
    for (id, kind, name, cfg) in [
        (
            "vta-jira",
            "vta-jira-kind",
            "VTA Jira",
            r#"{"username":"mara.lindqvist"}"#,
        ),
        (
            "vta-jira-eu",
            "vta-jira-kind",
            "VTA Jira EU",
            r#"{"username":"mara.lindqvist"}"#,
        ),
        (
            "vta-gitea",
            "vta-gitea-kind",
            "VTA Gitea",
            r#"{"username":"zoe.bright"}"#,
        ),
    ] {
        seed_source(&pool, id, kind, name, cfg).await;
    }

    let vocab = Vocabulary::load(&pool, KindCatalog::default())
        .await
        .unwrap();

    // One adapter kind, two instances: the adapter kind typed out means "the
    // instances of it", which is what makes `/ji` plural (the built-in alias
    // table maps aliases onto exactly this step).
    assert_eq!(
        vocab.resolve_source("vta-jira-kind"),
        ["vta-jira", "vta-jira-eu"]
    );
    // An exact id is that instance and nothing else.
    assert_eq!(vocab.resolve_source("vta-jira-eu"), ["vta-jira-eu"]);
    assert_eq!(vocab.resolve_source("vta-gitea"), ["vta-gitea"]);
    assert!(vocab.resolve_source("vta-nope").is_empty());

    // "Who am I" is not a setting anyone has to fill in twice: it is the
    // username each source was configured with (interfaces §4.2 config
    // schema), de-duplicated and sorted.
    //
    // The seed above is built so that both halves of that sentence are
    // load-bearing: the two Jiras share one account (so a missing `dedup`
    // shows), and the Gitea's account sorts *after* it while its id sorts
    // *before* (so a missing `sort` shows too -- row order alone would put
    // `zoe.bright` first).
    let mine: Vec<&str> = vocab
        .identity
        .iter()
        .filter(|name| ["mara.lindqvist", "zoe.bright"].contains(&name.as_str()))
        .map(String::as_str)
        .collect();
    assert_eq!(mine, ["mara.lindqvist", "zoe.bright"]);

    // Only this test's rows are asserted on, but every seeded one must be
    // present -- `load` reads the whole table, not a page of it.
    let seeded: Vec<&str> = vocab
        .sources
        .iter()
        .filter(|s| s.id.starts_with("vta-"))
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(seeded, ["vta-gitea", "vta-jira", "vta-jira-eu"]);
}

/// A source the user switched off is **unnameable, and still present**.
///
/// The guarantee is unchanged and is the first assertion: its rows are still in
/// the mirror, so answering `/x` with "every instance of x" would narrow to a
/// source the sources view shows as off, and the token must therefore be
/// unresolvable for the parser to report.
///
/// What changed in #200 is the *mechanism*, and this test used to pin the
/// mechanism as if it were the guarantee. The row was dropped in SQL, which
/// made the disabled source invisible to **every** consumer -- including
/// `coverage::in_scope`, which asks a different question ("which sources put
/// rows in the corpus this query scanned") and for which the honest answer
/// includes it. So it is in the list now, flagged, and
/// `Vocabulary::resolve_source` is what enforces the guarantee. Both halves are
/// asserted, because a fix that made it present and *nameable* would satisfy
/// the second alone.
#[tokio::test]
async fn a_disabled_source_is_unnameable_but_still_in_the_vocabulary() {
    let pool = pool().await;
    seed_source(&pool, "vtd-off", "vtd-off-kind", "VTD Off", r"{}").await;
    let before = Vocabulary::load(&pool, KindCatalog::default())
        .await
        .unwrap();
    assert_eq!(before.resolve_source("vtd-off"), ["vtd-off"]);
    assert!(
        before
            .sources
            .iter()
            .any(|s| s.id == "vtd-off" && s.enabled)
    );

    sqlx::query("update knobas.source_config set enabled = false where id = 'vtd-off'")
        .execute(&pool)
        .await
        .unwrap();

    let vocab = Vocabulary::load(&pool, KindCatalog::default())
        .await
        .unwrap();
    // The guarantee: the grammar cannot name it, by any of the four routes
    // `resolve_source` offers -- exact id, alias, adapter kind, unique prefix.
    for token in ["vtd-off", "vtd-off-kind", "vtd-o", "VTD Off"] {
        assert!(
            vocab.resolve_source(token).is_empty(),
            "{token} resolved to a source the user turned off"
        );
    }
    // The mechanism: present and flagged, so a reader that legitimately needs
    // the wider population can have it.
    assert!(
        vocab
            .sources
            .iter()
            .any(|s| s.id == "vtd-off" && !s.enabled),
        "a disabled source left the vocabulary, so nothing can see the rows it \
         left in the mirror"
    );
}

/// Turning a source off does not change who `@me` is.
///
/// #200 widened `Vocabulary::sources` to carry disabled sources, and `identity`
/// is derived from that list -- so without an explicit filter the widening
/// would have silently changed what `@me` matches. Whether a disabled source's
/// username should count is a product question that issue deliberately did not
/// answer; this is what keeps it unanswered rather than answered by accident.
#[tokio::test]
async fn disabling_a_source_does_not_widen_the_identity_behind_me() {
    let pool = pool().await;
    seed_source(
        &pool,
        "vti-off",
        "vti-off-kind",
        "VTI Off",
        r#"{"username":"vti.only.here"}"#,
    )
    .await;
    let before = Vocabulary::load(&pool, KindCatalog::default())
        .await
        .unwrap();
    assert!(before.identity.iter().any(|n| n == "vti.only.here"));

    sqlx::query("update knobas.source_config set enabled = false where id = 'vti-off'")
        .execute(&pool)
        .await
        .unwrap();

    let vocab = Vocabulary::load(&pool, KindCatalog::default())
        .await
        .unwrap();
    assert!(
        !vocab.identity.iter().any(|n| n == "vti.only.here"),
        "a disabled source's username leaked into @me: {:?}",
        vocab.identity
    );
    // ...and it is still in the source list, or this test would pass for the
    // wrong reason -- the pre-#200 behaviour satisfied the assertion above too.
    assert!(vocab.sources.iter().any(|s| s.id == "vti-off"));
}

/// A configured source with no `username` contributes no identity -- and an
/// *empty* one contributes none either.
///
/// The trap is the empty string: `config ->> 'username'` on `{"username":""}`
/// is `Some("")`, and an empty name in `identity` would make `@me` bind an
/// array containing `''`. Nothing matches it today, but it is a filter the user
/// cannot see and cannot have meant.
#[tokio::test]
async fn a_blank_username_is_nobody() {
    let pool = pool().await;
    seed_source(&pool, "vtb-anon", "vtb-anon-kind", "VTB Anon", r"{}").await;
    seed_source(
        &pool,
        "vtb-blank",
        "vtb-blank-kind",
        "VTB Blank",
        r#"{"username":"   "}"#,
    )
    .await;

    let vocab = Vocabulary::load(&pool, KindCatalog::default())
        .await
        .unwrap();
    assert!(vocab.identity.iter().all(|name| !name.trim().is_empty()));
    for id in ["vtb-anon", "vtb-blank"] {
        let source = vocab.sources.iter().find(|s| s.id == id).expect(id);
        assert_eq!(source.username, None, "{id}");
    }
}
