//! The launcher's answer, end to end, against a real PostgreSQL.
//!
//! `sql_shape.rs` runs the generated statement; this runs the whole pipeline --
//! vocabulary, grammar, statement, grouping -- through the public
//! [`Searcher`], which is what the IPC command calls.
//!
//! Every test in this binary shares one database (`knobas_db::test_util`), so
//! each seeds a token unique to itself and searches for *that*. Nothing
//! truncates, and no test can be made to pass by a row another one left behind.
//!
//! The database does **not** outlive the run, and getting that wrong once cost
//! this stream a misdiagnosed test failure: `test_util::run_nonce` is
//! `{pid}-{nanos}`, so a scratch directory an earlier run left behind can never
//! match this process's stamp, and `take_over` stops its server and deletes it.
//! What a test can see is exactly what the other tests *in this run of this
//! binary* have seeded -- and libtest does not order those, so no test may
//! assume which of them have already run.

use std::collections::HashSet;

use chrono::{DateTime, Duration, Utc};
use knobas_search::{KindCatalog, Prefix, SearchError, SearchFilters, SearchQuery, Searcher};

/// A query object with the defaults the launcher sends.
fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

fn token(tag: &str) -> String {
    format!("zt{tag}{}", uuid::Uuid::new_v4().simple())
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

/// A searcher labelling groups from the **mock adapter's own** `KindInfo`.
///
/// Not a hand-written table: spec §3a says the launcher renders a kind from
/// what the adapter declared, so the test asserts against the declaration
/// rather than against a copy of it.
fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::with_kinds(
        pool.clone(),
        KindCatalog::from_descriptors([knobas_source_mock::descriptor_template()]),
    )
}

/// Register the source instances the grammar resolves `/ji` against.
///
/// Only ever `jira` and `gitea`, and only from this file: `Vocabulary::load`
/// reads every enabled source, so a second instance of the *jira* adapter kind
/// anywhere in this binary would make `/ji` plural and change what
/// `filters_narrow_and_are_echoed_back_as_the_launcher_will_chip_them`
/// asserts.
async fn seed_sources(pool: &sqlx::PgPool) {
    for (id, kind, name, username) in [
        ("jira", "jira", "Jira", Some("mara.lindqvist")),
        ("gitea", "gitea", "Gitea", None),
    ] {
        sqlx::query(
            "insert into knobas.source_config
               (id, kind, display_name, base_url, auth_kind, config)
             values ($1, $2, $3, 'http://x', 'Pat', coalesce($4::jsonb, '{}'::jsonb))
             on conflict (id) do update
                set kind = excluded.kind, display_name = excluded.display_name,
                    config = excluded.config, enabled = true",
        )
        .bind(id)
        .bind(kind)
        .bind(name)
        .bind(username.map(|name| format!(r#"{{"username":"{name}"}}"#)))
        .execute(pool)
        .await
        .unwrap();
    }
}

#[allow(clippy::too_many_arguments)]
async fn seed(
    pool: &sqlx::PgPool,
    id: &str,
    kind: &str,
    source_id: &str,
    title: &str,
    body: &str,
    author: Option<&str>,
    updated_at: DateTime<Utc>,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    // `synced_at` is seeded distinctly in the past: a mirror is as old as its
    // last run, and §4 renders "synced 4 min ago" from this column rather than
    // from the source's own stamp.
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, item_updated_at,
            synced_at, payload)
         values ($1,$2,$3,$4,$5,$6,$7, now() - interval '4 minutes', '{}'::jsonb)",
    )
    .bind(id)
    .bind(source_id)
    .bind(kind)
    .bind(title)
    .bind(body)
    .bind(author)
    .bind(updated_at)
    .execute(pool)
    .await
    .unwrap();
}

/// The round-3 corpus in miniature: three tickets, two PRs, one build, one
/// page -- and the page deliberately does **not** carry the token, so a group
/// order taken from the DB's row order has something to get wrong.
async fn seeded_corpus(pool: &sqlx::PgPool, tag: &str) -> String {
    let t = token(tag);
    let day = |n: i64| Utc::now() - Duration::days(n);
    seed(
        pool,
        &format!("jira:{t}-PAY-231"),
        "ticket",
        "jira",
        &format!("Retry failed {t} payouts"),
        &format!("The {t} batch gives up after one retry instead of backing off."),
        Some("mara.lindqvist"),
        day(1),
    )
    .await;
    seed(
        pool,
        &format!("jira:{t}-PAY-244"),
        "ticket",
        "jira",
        "Reconcile the ledger",
        &format!("Blocked on the {t} work above."),
        Some("jonas.k"),
        day(2),
    )
    .await;
    seed(
        pool,
        &format!("jira:{t}-PAY-250"),
        "ticket",
        "jira",
        "Ledger window",
        &format!("Mentions {t} once, in passing."),
        None,
        day(3),
    )
    .await;
    seed(
        pool,
        &format!("gitea:{t}-11"),
        "pr",
        "gitea",
        &format!("{t} retry backoff"),
        "Implements the backoff.",
        Some("mara.lindqvist"),
        day(1),
    )
    .await;
    seed(
        pool,
        &format!("gitea:{t}-12"),
        "pr",
        "gitea",
        "Ledger cleanup",
        &format!("Depends on {t}."),
        None,
        day(4),
    )
    .await;
    // The build ranks its token in the *title*, and "build" sorts before both
    // "pr" and "ticket": the fixed group order has to beat both the ranking
    // and the row order to come out right.
    seed(
        pool,
        &format!("tc:{t}-99"),
        "build",
        "teamcity",
        &format!("{t} {t} deploy"),
        &format!("{t} {t} {t}"),
        None,
        day(1),
    )
    .await;
    seed(
        pool,
        &format!("cf:{t}-page"),
        "page",
        "confluence",
        "Runbook for the ledger window",
        "Nothing here matches.",
        None,
        day(5),
    )
    .await;
    t
}

#[tokio::test]
async fn finds_by_full_text_and_groups_by_kind_in_a_fixed_order() {
    let pool = pool().await;
    seed_sources(&pool).await;
    let t = seeded_corpus(&pool, "order").await;
    let s = searcher(&pool);

    let r = s.search(q(&t)).await.unwrap();
    let kinds: Vec<&str> = r.groups.iter().map(|g| g.kind.as_str()).collect();
    // §4's order, not the DB's (which is alphabetical: build, pr, ticket) and
    // not the ranking's (the build carries the token twice, in its title).
    assert_eq!(kinds, ["ticket", "pr", "build"]);
    assert_eq!(r.groups[0].plural, "Tickets");
    assert_eq!(r.groups[0].monogram, "TK");
    assert_eq!(r.total, 6);
    assert_eq!(r.total, r.groups.iter().map(|g| g.total).sum::<u32>());
    assert!(r.groups[0].hits[0].row.entity_id.ends_with("PAY-231"));
    assert!(r.groups[0].hits[0].rank > 0.0);
    // The page is in the corpus and does not match, so it is not a group.
    assert!(r.groups.iter().all(|g| g.kind != "page"));
}

#[tokio::test]
async fn every_row_carries_its_provenance() {
    let pool = pool().await;
    seed_sources(&pool).await;
    let t = seeded_corpus(&pool, "prov").await;
    let s = searcher(&pool);

    let r = s.search(q(&t)).await.unwrap();
    let hit = &r.groups[0].hits[0];
    assert_eq!(hit.row.source_id, "jira"); // which system it came from
    assert!(hit.row.synced_at <= Utc::now()); // "synced 4 min ago" (§4)
    assert!(hit.row.updated_at.is_some()); // the source's own timestamp
    // The two stamps are different columns and must not be confused: the
    // source last touched PAY-231 a day ago, knobas saw it four minutes ago.
    assert!(hit.row.updated_at.unwrap() < hit.row.synced_at);
    // Provenance is per row, not per group: the PR came from Gitea.
    let pr = r
        .groups
        .iter()
        .find(|g| g.kind == "pr")
        .expect("a pr group");
    assert!(pr.hits.iter().all(|h| h.row.source_id == "gitea"));
}

/// The excerpt crosses the bridge as flagged segments, never as markup
/// (roadmap §4 gotcha 7).
///
/// Two separate facts, and only the first is a safety property:
///
/// * **Nothing the highlighter invented is in the text.** The sentinels are
///   two control characters that Rust consumes; a build that fell back to
///   `ts_headline`'s default `<b>…</b>` selectors would put literal tags into
///   a string the renderer prints as text.
/// * **Postgres elides source markup.** `ts_headline` rebuilds its fragment
///   from parsed tokens and the default parser classifies `<script>` as a tag
///   it does not emit, so `<script>alert(1)</script>` comes back as
///   ` alert(1) `. Recorded because it is a *fidelity* caveat -- an excerpt is
///   not byte-for-byte the source -- and explicitly **not** the reason the
///   bridge is safe. The safety is the flag-not-markup contract above plus a
///   renderer that prints `seg.text`; a source string with no tags in it at
///   all (`onclick=…`, a bare `<`) travels through untouched, which is exactly
///   why the frontend may never use `{@html}`.
#[tokio::test]
async fn the_snippet_is_segments_of_raw_text_with_the_match_flagged() {
    let pool = pool().await;
    let t = token("snip");
    seed(
        &pool,
        &format!("jira:{t}"),
        "ticket",
        "jira",
        &format!("Retry {t} now"),
        "A body that holds <script>alert(1)</script> and a <b>bold</b> claim.",
        None,
        Utc::now(),
    )
    .await;

    let r = searcher(&pool).search(q(&t)).await.unwrap();
    let hit = &r.groups[0].hits[0];
    assert!(
        hit.snippet
            .iter()
            .any(|seg| seg.hit && seg.text.to_lowercase().contains(&t)),
        "{:?}",
        hit.snippet
    );
    assert!(
        hit.snippet
            .iter()
            .all(|seg| !seg.text.contains(['\u{1}', '\u{2}']))
    );
    assert!(
        hit.snippet.iter().all(|seg| !seg.text.is_empty()),
        "an empty segment is a row the UI draws nothing into: {:?}",
        hit.snippet
    );

    let joined: String = hit.snippet.iter().map(|s| s.text.as_str()).collect();
    // Nothing the highlighter invented: the marks are the `hit` flag, and the
    // only reason a tag could appear here is a build that stopped passing the
    // sentinel selectors.
    assert!(
        !joined.contains("<b>") && !joined.contains("</b>"),
        "{joined:?}"
    );
    // What is not markup is untouched, punctuation and all -- so the renderer
    // is the thing that has to treat it as text.
    assert!(joined.contains("alert(1)"), "{joined:?}");
    // Exactly one run is flagged, and it is the match.
    let flagged: Vec<&str> = hit
        .snippet
        .iter()
        .filter(|s| s.hit)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!(flagged, [t.as_str()], "{:?}", hit.snippet);
}

#[tokio::test]
async fn a_deleted_item_disappears_from_the_index() {
    let pool = pool().await;
    seed_sources(&pool).await;
    let t = seeded_corpus(&pool, "del").await;
    let s = searcher(&pool);
    let id = format!("jira:{t}-PAY-231");

    let raw = t.clone();
    assert!(
        s.search(q(&raw))
            .await
            .unwrap()
            .groups
            .iter()
            .flat_map(|g| &g.hits)
            .any(|h| h.row.entity_id == id)
    );

    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&id)
        .execute(s.pool())
        .await
        .unwrap();

    let after = s.search(q(&raw)).await.unwrap();
    assert!(
        after
            .groups
            .iter()
            .flat_map(|g| &g.hits)
            .all(|h| h.row.entity_id != id)
    );
    // And the group's total drops with it -- a tombstone that only hid the
    // row would leave a header counting something nobody can open.
    assert_eq!(after.groups[0].total, 2);
}

#[tokio::test]
async fn filters_narrow_and_are_echoed_back_as_the_launcher_will_chip_them() {
    let pool = pool().await;
    seed_sources(&pool).await;
    let t = seeded_corpus(&pool, "chip").await;
    let s = searcher(&pool);

    let unfiltered = s.search(q(&t)).await.unwrap();
    assert!(
        unfiltered
            .groups
            .iter()
            .flat_map(|g| &g.hits)
            .any(|h| h.row.source_id == "gitea"),
        "the filter has to have something to remove"
    );

    let r = s.search(q(&format!("/ji {t}"))).await.unwrap();
    assert_eq!(r.interpreted.filters.sources, ["jira"]);
    assert!(
        r.groups
            .iter()
            .flat_map(|g| &g.hits)
            .all(|h| h.row.source_id == "jira")
    );
    assert_eq!(r.interpreted.prefix, Some(Prefix::Source));
    // The totals are the *filtered* corpus, not the whole one.
    assert_eq!(r.total, 3);
}

/// A named person narrows, in all four spellings, with text and without it
/// (ruling **E-Q1**).
///
/// The no-text half is the one that can regress silently. [`Searcher::search`]
/// refuses a query with neither text nor a filter -- that answer belongs to
/// the board -- so an author list the refusal does not count reads as "no
/// filter", and `@jonas` on its own comes back empty instead of with jonas's
/// work. Nothing else in this file would notice.
///
/// The two authors are tokens rather than names, because every test in this
/// binary shares one database: a filter on a plain `jonas` would be answered
/// with whatever another test seeded under that name.
#[tokio::test]
async fn a_named_person_filters_with_or_without_search_text() {
    let pool = pool().await;
    let t = token("author");
    let hers = format!("ada-{t}");
    let his = format!("bob-{t}");
    seed(
        &pool,
        &format!("jira:{t}-1"),
        "ticket",
        "jira",
        &format!("{t} hers"),
        "body",
        Some(&hers),
        Utc::now() - Duration::days(1),
    )
    .await;
    seed(
        &pool,
        &format!("gitea:{t}-2"),
        "pr",
        "gitea",
        &format!("{t} his"),
        "body",
        Some(&his),
        Utc::now() - Duration::days(2),
    )
    .await;
    let s = searcher(&pool);

    let ids = |r: &knobas_search::SearchResponse| -> Vec<String> {
        r.groups
            .iter()
            .flat_map(|g| &g.hits)
            .map(|h| h.row.entity_id.clone())
            .collect()
    };

    // The control: without the author filter the text matches both rows, so
    // the filter has something to remove rather than an empty world to be
    // trivially right in.
    assert_eq!(ids(&s.search(q(&t)).await.unwrap()).len(), 2);

    for raw in [
        format!("@{hers} {t}"),
        format!("author:{hers} {t}"),
        format!("owner:{hers} {t}"),
        format!("by:{hers} {t}"),
    ] {
        let r = s.search(q(&raw)).await.unwrap();
        assert_eq!(ids(&r), [format!("jira:{t}-1")], "{raw:?}");
        assert_eq!(r.interpreted.filters.authors, [hers.as_str()], "{raw:?}");
        assert!(r.interpreted.unknown_tokens.is_empty(), "{raw:?}");
        assert_eq!(r.interpreted.prefix, Some(Prefix::Person), "{raw:?}");
        // A named person is not `mine`: reading it as the identity filter
        // would answer with the wrong person's work.
        assert!(!r.interpreted.filters.mine, "{raw:?}");
    }

    // And with no search word at all: a browse of that person's items.
    let r = s.search(q(&format!("@{his}"))).await.unwrap();
    assert_eq!(ids(&r), [format!("gitea:{t}-2")]);
    assert_eq!(r.total, 1);
}

/// A chip the user clicked is part of the query, so it has to come back in the
/// echo -- otherwise the launcher redraws a narrower query than the one it has
/// results for.
#[tokio::test]
async fn a_clicked_chip_is_applied_and_echoed_like_a_typed_one() {
    let pool = pool().await;
    seed_sources(&pool).await;
    let t = seeded_corpus(&pool, "clicked").await;

    let mut query = q(&t);
    query.filters.kinds = vec!["pr".to_owned()];
    let r = searcher(&pool).search(query).await.unwrap();
    assert_eq!(r.interpreted.filters.kinds, ["pr"]);
    assert_eq!(
        r.groups.iter().map(|g| g.kind.as_str()).collect::<Vec<_>>(),
        ["pr"]
    );
}

#[tokio::test]
async fn typing_a_word_matches_before_it_is_finished() {
    let pool = pool().await;
    let t = token("typing");
    seed(
        &pool,
        &format!("jira:{t}"),
        "ticket",
        "jira",
        &format!("Retry failed {t} payouts"),
        "body",
        None,
        Utc::now(),
    )
    .await;
    let s = searcher(&pool);

    let half = &t[..t.len() - 4];
    assert!(!s.search(q(half)).await.unwrap().groups.is_empty()); // still typing
    assert!(
        s.search(q(&format!("{half} ")))
            .await
            .unwrap()
            .groups
            .is_empty()
    ); // a finished word
}

#[tokio::test]
async fn a_kind_the_limit_cut_still_reports_its_count() {
    let pool = pool().await;
    let t = token("cut");
    for n in 0..12 {
        seed(
            &pool,
            &format!("jira:{t}-t{n}"),
            "ticket",
            "jira",
            &format!("{t} payout {n}"),
            "body",
            None,
            Utc::now() - Duration::minutes(n),
        )
        .await;
        seed(
            &pool,
            &format!("gitea:{t}-p{n}"),
            "pr",
            "gitea",
            &format!("{t} payout pr {n}"),
            "body",
            None,
            Utc::now() - Duration::minutes(n),
        )
        .await;
    }

    let mut query = q(&t);
    query.limit = 3;
    let r = searcher(&pool).search(query).await.unwrap();
    assert_eq!(r.groups.iter().map(|g| g.hits.len()).sum::<usize>(), 3);
    // The truth, not the page. Both kinds are reported even though at most one
    // of them can have a row on a page of three.
    assert_eq!(r.groups.iter().map(|g| g.total).sum::<u32>(), 24);
    assert_eq!(r.total, 24);
    assert_eq!(
        r.groups
            .iter()
            .map(|g| (g.kind.as_str(), g.total))
            .collect::<Vec<_>>(),
        [("ticket", 12), ("pr", 12)]
    );
}

#[tokio::test]
async fn corpora_that_m1_does_not_have_return_nothing_rather_than_pretending() {
    let pool = pool().await;
    seed_sources(&pool).await;
    let t = seeded_corpus(&pool, "absent").await;
    let s = searcher(&pool);

    for raw in [
        format!("asset: {t}"),
        format!("note: {t}"),
        format!("t {t}"),
        format!("> {t}"),
        format!("? {t}"),
    ] {
        let r = s.search(q(&raw)).await.unwrap();
        assert!(r.groups.is_empty() && r.total == 0, "{raw}");
        assert!(
            r.interpreted.prefix.is_some(),
            "{raw} must still be understood"
        );
    }
    // The control: the same text with no prefix does find the corpus, so the
    // emptiness above is the prefix's doing and not the token's.
    assert!(!s.search(q(&t)).await.unwrap().groups.is_empty());
}

#[tokio::test]
async fn an_empty_query_is_the_boards_job_not_a_full_table_count() {
    let pool = pool().await;
    seed_sources(&pool).await;
    seeded_corpus(&pool, "board").await;
    let s = searcher(&pool);
    for raw in ["", "   "] {
        let r = s.search(q(raw)).await.unwrap();
        assert!(r.groups.is_empty() && r.total == 0, "{raw:?}");
    }
}

/// A filter with no text is still a query -- "everything from Jira, last week"
/// is a browse the user asked for, and it is bounded by the filter.
///
/// The source id is unique to this test on purpose: a browse has no text to
/// narrow it, so it returns the newest rows of whatever it is pointed at, and
/// pointing it at a shared `jira` would return whatever the rest of this
/// binary seeded most recently.
#[tokio::test]
async fn a_filter_with_no_text_browses_rather_than_answering_nothing() {
    let pool = pool().await;
    let t = token("browse");
    let mine = format!("jira-{t}");
    for (n, kind) in [(0_i64, "ticket"), (1, "ticket"), (2, "pr")] {
        seed(
            &pool,
            &format!("{mine}:{n}"),
            kind,
            &mine,
            &format!("{t} item {n}"),
            "body",
            None,
            Utc::now() - Duration::minutes(n),
        )
        .await;
    }
    // A row of the same vintage under another source, so the filter has
    // something to exclude rather than an empty world to be trivially right in.
    seed(
        &pool,
        &format!("other-{t}:9"),
        "ticket",
        &format!("other-{t}"),
        &format!("{t} elsewhere"),
        "body",
        None,
        Utc::now(),
    )
    .await;

    let mut query = q("");
    query.filters.sources = vec![mine.clone()];
    let r = searcher(&pool).search(query).await.unwrap();
    let ids: Vec<&str> = r
        .groups
        .iter()
        .flat_map(|g| &g.hits)
        .map(|h| h.row.entity_id.as_str())
        .collect();
    assert_eq!(ids.len(), 3, "a source chip alone must browse that source");
    assert!(
        r.groups
            .iter()
            .flat_map(|g| &g.hits)
            .all(|h| h.row.source_id == mine),
        "{ids:?}"
    );
    // Browse mode ranks nothing: the order within a group is recency.
    let stamps: Vec<_> = r.groups[0].hits.iter().map(|h| h.row.updated_at).collect();
    assert!(stamps.windows(2).all(|w| w[0] >= w[1]), "{stamps:?}");
    assert!(r.groups[0].hits.iter().all(|h| h.rank == 0.0));
    assert!(r.groups[0].hits.iter().all(|h| h.snippet.is_empty()));
}

#[tokio::test]
async fn hostile_input_is_answered_not_executed() {
    let pool = pool().await;
    seed_sources(&pool).await;
    seeded_corpus(&pool, "hostile").await;
    let s = searcher(&pool);

    for raw in [
        "'; drop schema knobas cascade; --",
        "\\",
        "&&&",
        "\"unclosed",
        "  ",
        "!!!",
        "sepa'::text||(select 1)",
    ] {
        s.search(q(raw)).await.expect(raw); // no error, no damage
    }

    let mut long = q(&"x".repeat(600));
    long.limit = 10_000;
    let err = s.search(long).await.unwrap_err();
    assert!(matches!(err, SearchError::Invalid(_)), "{err:?}"); // bounded, on purpose

    assert!(
        sqlx::query_scalar::<_, i64>("select count(*) from sync.item")
            .fetch_one(s.pool())
            .await
            .unwrap()
            > 0
    ); // still there
}

/// A kind the fixed order has never heard of is still grouped, labelled and
/// placed -- adding an adapter must not need an entry anywhere (spec §3a).
#[tokio::test]
async fn a_kind_no_one_declared_is_still_grouped_and_labelled() {
    let pool = pool().await;
    let t = token("unknown");
    seed(
        &pool,
        &format!("acme:{t}-1"),
        "widget_run",
        "acme",
        &format!("{t} nightly"),
        "body",
        None,
        Utc::now(),
    )
    .await;
    seed(
        &pool,
        &format!("jira:{t}-2"),
        "ticket",
        "jira",
        &format!("{t} ticket"),
        "body",
        None,
        Utc::now(),
    )
    .await;

    let r = searcher(&pool).search(q(&t)).await.unwrap();
    let kinds: Vec<&str> = r.groups.iter().map(|g| g.kind.as_str()).collect();
    assert_eq!(kinds, ["ticket", "widget_run"], "unknown kinds sort last");
    let widget = &r.groups[1];
    assert_eq!(widget.label, "Widget run");
    assert_eq!(widget.plural, "Widget runs");
    assert_eq!(widget.monogram, "WR");
    assert_eq!(
        r.groups
            .iter()
            .map(|g| g.kind.clone())
            .collect::<HashSet<_>>()
            .len(),
        2
    );
}

/// One query, both corpora: what knobas synced and what it owns come back
/// together (#46 stories 12 and 13).
///
/// Written through `knobas_core::note`, not by seeding two rows, because the
/// claim is that a note the user *wrote* is findable -- and the store is what
/// writes one. Story 15's "as soon as I have written it" is the same
/// assertion: the corpus is `knobas.note` itself and `fts` is a stored
/// generated column, so there is no index to catch up.
#[tokio::test]
async fn a_note_is_found_by_the_same_search_that_finds_a_ticket() {
    let pool = pool().await;
    seed_sources(&pool).await;
    let t = token("note");

    seed(
        &pool,
        &format!("jira:{t}-1"),
        "ticket",
        "jira",
        &format!("{t} SEPA retry"),
        "the retry counter is off by one",
        Some("mara.lindqvist"),
        Utc::now(),
    )
    .await;
    let note = knobas_core::note::create(
        &pool,
        &format!("{t} investigation"),
        "the counter starts at zero, not one",
        "user",
    )
    .await
    .unwrap();

    // Story 13: the *body* is searchable, not just the title -- the phrase
    // below is in neither title.
    let by_body = searcher(&pool).search(q(&format!("{t} counter"))).await.unwrap();
    let found: Vec<&str> = by_body
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .map(|hit| hit.row.entity_id.as_str())
        .collect();
    assert!(found.contains(&note.id.as_str()), "{found:?}");
    assert!(found.contains(&format!("jira:{t}-1").as_str()), "{found:?}");

    // Story 12: one search box, and the note is grouped and labelled like
    // anything else -- from the catalog, which knows the kind knobas owns
    // even though no adapter declares it.
    let both = searcher(&pool).search(q(&t)).await.unwrap();
    let groups: Vec<(&str, &str)> = both
        .groups
        .iter()
        .map(|group| (group.kind.as_str(), group.plural.as_str()))
        .collect();
    assert_eq!(groups, [("ticket", "Tickets"), ("note", "Notes")]);
    assert_eq!(both.total, 2);
}

/// Story 14: `note:` narrows to what the user wrote, and does not narrow it
/// away.
#[tokio::test]
async fn the_note_prefix_returns_notes_and_only_notes() {
    let pool = pool().await;
    seed_sources(&pool).await;
    let t = token("pfx");

    seed(
        &pool,
        &format!("jira:{t}-1"),
        "ticket",
        "jira",
        &format!("{t} a ticket"),
        "",
        None,
        Utc::now(),
    )
    .await;
    let note = knobas_core::note::create(&pool, &format!("{t} a note"), "", "user")
        .await
        .unwrap();

    for raw in [format!("note: {t}"), format!("type:note {t}")] {
        let answer = searcher(&pool).search(q(&raw)).await.unwrap();
        let ids: Vec<&str> = answer
            .groups
            .iter()
            .flat_map(|group| &group.hits)
            .map(|hit| hit.row.entity_id.as_str())
            .collect();
        assert_eq!(ids, [note.id.as_str()], "{raw:?}");
        assert_eq!(answer.total, 1, "{raw:?}");
        // The prefix is understood and echoed, not silently dropped.
        assert!(answer.interpreted.unknown_tokens.is_empty(), "{raw:?}");
        assert!(
            answer
                .interpreted
                .filters
                .kinds
                .iter()
                .any(|kind| kind == "note"),
            "{raw:?}"
        );
    }
    // And `note:` is no longer a prefix that answers with nothing on purpose.
    assert_eq!(
        searcher(&pool)
            .search(q(&format!("note: {t}")))
            .await
            .unwrap()
            .interpreted
            .prefix,
        Some(Prefix::Note)
    );
}

/// Story 16, and the standing rule behind it: a snippet is **segments**, and
/// the text in them is text.
///
/// Markdown in a note is the case that makes the rule easy to forget, because
/// an excerpt full of `#` and `[[…]]` looks like something to render. Nothing
/// here renders it: the highlight is the `hit` flag, the sentinels the
/// highlighter used are gone, and what the user typed is in `text` for the
/// frontend to print as text.
///
/// Not asserted, and deliberately so: that a `<b>` the user typed comes back.
/// `ts_headline` elides *well-formed* tags, which `snippet.rs` records as a
/// fidelity accident and explicitly not a sanitiser -- `onclick=` travels
/// through untouched. Asserting on the elision would pin the accident.
#[tokio::test]
async fn a_notes_snippet_is_segments_of_the_markdown_the_user_typed() {
    let pool = pool().await;
    seed_sources(&pool).await;
    let t = token("snip");

    // The ref sits *next to* the match, so it is inside whatever window
    // `ts_headline` chooses rather than left to luck.
    let body = format!(
        "## Runbook\n\nThe {t} escalation [[jira:PAY-231]] path is not the on-call rota."
    );
    let note = knobas_core::note::create(&pool, &format!("{t} runbook"), &body, "user")
        .await
        .unwrap();

    let answer = searcher(&pool)
        .search(q(&format!("{t} escalation")))
        .await
        .unwrap();
    let hit = answer
        .groups
        .iter()
        .flat_map(|group| &group.hits)
        .find(|hit| hit.row.entity_id == note.id)
        .expect("the note is in the answer");

    assert!(
        hit.snippet.iter().any(|segment| segment.hit),
        "something in the excerpt is marked as the match: {:?}",
        hit.snippet
    );
    let joined: String = hit.snippet.iter().map(|s| s.text.as_str()).collect();
    // Nothing the *highlighter* added survives: no sentinel, no tag of its own.
    assert!(!joined.contains(knobas_search::snippet::HIT_START));
    assert!(!joined.contains(knobas_search::snippet::HIT_STOP));
    assert!(!joined.contains("<b>escalation</b>"));
    assert!(joined.contains(&t), "{joined:?}");
    assert!(joined.contains("escalation"), "{joined:?}");
    // And the markdown is **markdown source**, verbatim: the ref is four
    // brackets and an id, not a chip and not stripped. Rendering is the
    // frontend's decision and it makes it the same way for every corpus --
    // by printing `segment.text` as text.
    assert!(
        joined.contains("[[jira:PAY-231]]"),
        "the excerpt quotes what the user typed: {joined:?}"
    );
}
