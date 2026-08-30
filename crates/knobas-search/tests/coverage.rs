//! Issue #141's report, end to end against a real PostgreSQL.
//!
//! The claim under test is the one the launcher renders: an `author:` query
//! that comes back empty says *why* it came back empty, and says something
//! different for a source that answered than for a source that could not be
//! asked.
//!
//! Every test in this binary shares one database (`knobas_db::test_util`) and
//! `Vocabulary::load` reads **every enabled source**, so a source seeded by one
//! test is in every other test's vocabulary. Each test therefore owns its own
//! source ids and narrows the query with `source:` where the exact list is what
//! it asserts. Nothing truncates, and libtest does not order these, so no test
//! may assume which of the others have already run.

use knobas_search::{
    FilterAnswer, FilterDimension, KindCatalog, SearchFilters, SearchQuery, Searcher,
};

fn q(raw: &str) -> SearchQuery {
    SearchQuery {
        raw: raw.to_owned(),
        limit: 30,
        filters: SearchFilters::default(),
    }
}

fn token(tag: &str) -> String {
    format!("zc{tag}{}", uuid::Uuid::new_v4().simple())
}

async fn pool() -> sqlx::PgPool {
    let pool = knobas_db::test_util::test_pool().await;
    knobas_db::migrate::run(&pool).await.unwrap();
    pool
}

fn searcher(pool: &sqlx::PgPool) -> Searcher {
    Searcher::with_kinds(
        pool.clone(),
        KindCatalog::from_descriptors([knobas_source_mock::descriptor_template()]),
    )
}

/// Register one configured source instance, with the username `@me` resolves
/// through when it is given one.
async fn seed_source(
    pool: &sqlx::PgPool,
    id: &str,
    kind: &str,
    name: &str,
    username: Option<&str>,
) {
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

async fn seed_item(
    pool: &sqlx::PgPool,
    id: &str,
    kind: &str,
    source_id: &str,
    title: &str,
    author: Option<&str>,
) {
    sqlx::query("insert into knobas.entity (id, kind, title) values ($1,$2,$3)")
        .bind(id)
        .bind(kind)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into sync.item
           (entity_id, source_id, kind, title, body_text, author, item_updated_at,
            synced_at, payload)
         values ($1,$2,$3,$4,$4,$5, now(), now(), '{}'::jsonb)",
    )
    .bind(id)
    .bind(source_id)
    .bind(kind)
    .bind(title)
    .bind(author)
    .execute(pool)
    .await
    .unwrap();
}

/// The verdict for one source in the one reported dimension.
fn answer_for(response: &knobas_search::SearchResponse, source_id: &str) -> Option<FilterAnswer> {
    let coverage = response.coverage.first()?;
    assert_eq!(coverage.dimension, FilterDimension::Author);
    coverage
        .sources
        .iter()
        .find(|source| source.source_id == source_id)
        .map(|source| source.answer)
}

/// **The issue, in one test.**
///
/// Two sources in one corpus, one query, and the two states #141 exists to
/// keep apart. The Jira answers and has nobody by that name; the TeamCity is
/// *unable to answer* -- and before this, both came back as an empty result
/// with nothing to tell them apart.
#[tokio::test]
async fn a_source_with_no_authors_is_reported_apart_from_one_that_simply_did_not_match() {
    let pool = pool().await;
    let t = token("apart");
    let jira = format!("jira-{t}");
    let teamcity = format!("tc-{t}");
    seed_source(&pool, &jira, "jira", "Jira", Some("mara.lindqvist")).await;
    seed_source(&pool, &teamcity, "teamcity", "Buildserver", None).await;

    seed_item(
        &pool,
        &format!("{jira}:PAY-1"),
        "ticket",
        &jira,
        &format!("Retry {t} payouts"),
        Some("mara.lindqvist"),
    )
    .await;
    // Real CI builds are VCS-triggered and name no user (issue #106 measured
    // 100 of 100 on a live server). That is the corpus this reports on.
    seed_item(
        &pool,
        &format!("{teamcity}:99"),
        "build",
        &teamcity,
        &format!("{t} deploy"),
        None,
    )
    .await;

    let response = searcher(&pool)
        .search(q(&format!(
            "{t} author:nobody.at.all source:{jira} source:{teamcity}"
        )))
        .await
        .unwrap();

    // The user's own reading of the answer: nothing came back.
    assert_eq!(response.total, 0, "{:?}", response.groups);
    // And now it says why, differently for each source.
    assert_eq!(answer_for(&response, &jira), Some(FilterAnswer::Answered));
    assert_eq!(
        answer_for(&response, &teamcity),
        Some(FilterAnswer::NoValues)
    );

    // The name is on the wire, because the launcher has no other way to say
    // "Buildserver" -- `CredentialHealth` carries no display name.
    let named = &response.coverage[0].sources;
    assert!(
        named
            .iter()
            .any(|source| source.source_id == teamcity && source.display_name == "Buildserver"),
        "{named:?}"
    );
    // Exactly the two sources the query named, in id order, and no others --
    // this binary's other tests have sources in the vocabulary too.
    let mut ids: Vec<&str> = named.iter().map(|s| s.source_id.as_str()).collect();
    ids.sort_unstable();
    let mut expected = [jira.as_str(), teamcity.as_str()];
    expected.sort_unstable();
    assert_eq!(ids, expected);
}

/// The miss direction the ruling names: a corpus whose sources all answered
/// has nothing to explain.
#[tokio::test]
async fn a_corpus_whose_sources_all_answered_reports_no_gap() {
    let pool = pool().await;
    let t = token("allok");
    let jira = format!("jira-{t}");
    let gitea = format!("gitea-{t}");
    seed_source(&pool, &jira, "jira", "Jira", None).await;
    seed_source(&pool, &gitea, "gitea", "Gitea", None).await;
    seed_item(
        &pool,
        &format!("{jira}:PAY-1"),
        "ticket",
        &jira,
        &format!("{t} ledger"),
        Some("jonas.k"),
    )
    .await;
    seed_item(
        &pool,
        &format!("{gitea}:11"),
        "pr",
        &gitea,
        &format!("{t} backoff"),
        Some("mara.lindqvist"),
    )
    .await;

    let response = searcher(&pool)
        .search(q(&format!("{t} @jonas.k source:{jira} source:{gitea}")))
        .await
        .unwrap();

    // It found the one item, so this is not vacuously green.
    assert_eq!(response.total, 1, "{:?}", response.groups);
    // Measured, and every source answered -- so the launcher has nothing to
    // explain. The dimension is still reported, which is what tells a reader
    // "asked and fine" from "never asked".
    let coverage = response.coverage.first().expect("the author dimension");
    assert!(
        coverage
            .sources
            .iter()
            .all(|source| source.answer == FilterAnswer::Answered),
        "{:?}",
        coverage.sources
    );
}

/// No author filter, no report -- and therefore no second round trip on the
/// ordinary keystroke.
#[tokio::test]
async fn a_query_that_named_nobody_is_not_reported_on() {
    let pool = pool().await;
    let t = token("quiet");
    let source = format!("tc-{t}");
    seed_source(&pool, &source, "teamcity", "Buildserver", None).await;
    seed_item(
        &pool,
        &format!("{source}:99"),
        "build",
        &source,
        &format!("{t} deploy"),
        None,
    )
    .await;
    let s = searcher(&pool);

    // The same unauthored corpus that *would* be reported on, searched without
    // an author filter: the build comes back and nothing is explained.
    let plain = s.search(q(&t)).await.unwrap();
    assert_eq!(plain.total, 1, "{:?}", plain.groups);
    assert!(plain.coverage.is_empty(), "{:?}", plain.coverage);

    // A different filter dimension is still not the author dimension.
    let by_source = s.search(q(&format!("{t} source:{source}"))).await.unwrap();
    assert!(by_source.coverage.is_empty(), "{:?}", by_source.coverage);
}

/// `@me` is the same predicate and the same gap.
///
/// Both halves matter. A source configured with a username that its corpus
/// never names cannot answer `@me` either -- and the report has to survive the
/// state where knobas does not know who the user is at all, which is exactly
/// where `identity_authors` is empty and a check written against it would read
/// the query as unfiltered.
#[tokio::test]
async fn mine_is_an_author_query_even_when_nobody_is_configured() {
    let pool = pool().await;
    let t = token("mine");
    let source = format!("tc-{t}");
    // No username: `@me` resolves to nobody on this source.
    seed_source(&pool, &source, "teamcity", "Buildserver", None).await;
    seed_item(
        &pool,
        &format!("{source}:99"),
        "build",
        &source,
        &format!("{t} deploy"),
        None,
    )
    .await;

    let response = searcher(&pool)
        .search(SearchQuery {
            raw: format!("{t} source:{source}"),
            limit: 30,
            filters: SearchFilters {
                mine: true,
                ..SearchFilters::default()
            },
        })
        .await
        .unwrap();

    assert_eq!(response.total, 0, "{:?}", response.groups);
    assert_eq!(answer_for(&response, &source), Some(FilterAnswer::NoValues));
}

/// A source that has mirrored nothing is not accused of having no authors.
///
/// "It has synced nothing yet" and "nothing it syncs names a person" are
/// different facts with different remedies, and a report that called the first
/// one the second would put *Buildserver cannot answer that* on screen for a
/// source that has never run. It gets no verdict at all instead: whatever it is
/// missing from the results, authorship is not the reason.
#[tokio::test]
async fn a_source_that_has_synced_nothing_gets_no_verdict() {
    let pool = pool().await;
    let t = token("empty");
    let source = format!("tc-{t}");
    seed_source(&pool, &source, "teamcity", "Buildserver", None).await;

    let response = searcher(&pool)
        .search(q(&format!("{t} author:someone source:{source}")))
        .await
        .unwrap();

    assert_eq!(answer_for(&response, &source), None);
    // Not merely absent from the list -- there is no list, because the only
    // source in scope contributed no corpus.
    assert!(response.coverage.is_empty(), "{:?}", response.coverage);
}

/// **The report is narrowed by the query's kind scope, not only by `source:`.**
///
/// `note:` searches knobas' own notes and nothing else. A build source
/// contributes no row to that query because of the *kind* filter, so naming it
/// here would explain an absence authorship had nothing to do with -- which is
/// the very failure `Searcher::search`'s `asset:` short-circuit refuses, arrived
/// at through the other scoping dimension.
#[tokio::test]
async fn a_source_the_kind_scope_excluded_is_not_accused() {
    let pool = pool().await;
    let t = token("kinds");
    let source = format!("tc-{t}");
    seed_source(&pool, &source, "teamcity", "Buildserver", None).await;
    seed_item(
        &pool,
        &format!("{source}:99"),
        "build",
        &source,
        &format!("{t} deploy"),
        None,
    )
    .await;
    let s = searcher(&pool);

    // The control: with no kind scope, this same corpus *is* reported on.
    let unscoped = s
        .search(q(&format!("{t} author:someone source:{source}")))
        .await
        .unwrap();
    assert_eq!(
        answer_for(&unscoped, &source),
        Some(FilterAnswer::NoValues),
        "the fixture has something to report in the first place"
    );

    // A notes-only search, and a kind filter typed the other way. Neither can
    // return a build, and neither is the build source's fault.
    //
    // The assertion names *this* source rather than demanding an empty report:
    // these two queries carry no `source:` filter, so they legitimately report
    // on whatever else this binary's other tests have configured (a Jira with
    // tickets in it answers `kind:ticket` perfectly well). What must not happen
    // is a verdict on the build source.
    for raw in [
        format!("note: {t} author:someone"),
        format!("kind:ticket {t} author:someone"),
    ] {
        let response = s.search(q(&raw)).await.unwrap();
        assert_eq!(response.total, 0, "{raw}: {:?}", response.groups);
        assert_eq!(
            answer_for(&response, &source),
            None,
            "{raw} accused a source its kind scope had already excluded: {:?}",
            response.coverage
        );
    }
}

/// A tombstoned row does not vouch for a capability the live corpus lost.
///
/// The probe reads `sync.live_item`, not `sync.item` -- the same rule every
/// other read of the mirror follows, and here it is what stops a build a source
/// has since deleted from claiming its whole source can answer author queries.
#[tokio::test]
async fn an_author_a_source_has_deleted_does_not_count() {
    let pool = pool().await;
    let t = token("tomb");
    let source = format!("tc-{t}");
    seed_source(&pool, &source, "teamcity", "Buildserver", None).await;
    let live = format!("{source}:99");
    let gone = format!("{source}:98");
    seed_item(&pool, &live, "build", &source, &format!("{t} deploy"), None).await;
    // The only authored row this source has, and the source has withdrawn it.
    seed_item(
        &pool,
        &gone,
        "build",
        &source,
        &format!("{t} manual"),
        Some("mara.lindqvist"),
    )
    .await;
    sqlx::query("update knobas.entity set deleted_at = now() where id = $1")
        .bind(&gone)
        .execute(&pool)
        .await
        .unwrap();

    let response = searcher(&pool)
        .search(q(&format!("{t} author:mara.lindqvist source:{source}")))
        .await
        .unwrap();

    assert_eq!(response.total, 0, "{:?}", response.groups);
    assert_eq!(answer_for(&response, &source), Some(FilterAnswer::NoValues));
}

/// An author column that is present and blank is nobody.
///
/// `Vocabulary::load` already drops a blank configured username for this
/// reason; the corpus side has to agree, or a source that writes `''` where it
/// means "no user" reads as able to answer a question it cannot.
#[tokio::test]
async fn a_blank_author_is_not_an_author() {
    let pool = pool().await;
    let t = token("blank");
    let source = format!("tc-{t}");
    seed_source(&pool, &source, "teamcity", "Buildserver", None).await;
    seed_item(
        &pool,
        &format!("{source}:99"),
        "build",
        &source,
        &format!("{t} deploy"),
        Some(""),
    )
    .await;

    let response = searcher(&pool)
        .search(q(&format!("{t} author:someone source:{source}")))
        .await
        .unwrap();

    assert_eq!(answer_for(&response, &source), Some(FilterAnswer::NoValues));
}

/// The report is about the **corpus**, not about the match set.
///
/// This is the design decision that keeps the two states apart, and it is only
/// visible from a query whose text excludes every authored row: the Jira here
/// has an author, the searched word is in an item that does not, and the source
/// must still read as `answered`. A probe narrowed by the query's text would
/// call it `no_values` and put "Jira cannot answer that" on screen.
#[tokio::test]
async fn a_source_whose_authored_items_did_not_match_still_answered() {
    let pool = pool().await;
    let t = token("scope");
    let jira = format!("jira-{t}");
    seed_source(&pool, &jira, "jira", "Jira", None).await;
    seed_item(
        &pool,
        &format!("{jira}:PAY-1"),
        "ticket",
        &jira,
        &format!("{t} ledger"),
        None,
    )
    .await;
    // Authored, and deliberately without the searched-for token in it.
    seed_item(
        &pool,
        &format!("{jira}:PAY-2"),
        "ticket",
        &jira,
        "Reconcile the ledger",
        Some("jonas.k"),
    )
    .await;

    let response = searcher(&pool)
        .search(q(&format!("{t} author:jonas.k source:{jira}")))
        .await
        .unwrap();

    assert_eq!(response.total, 0, "{:?}", response.groups);
    assert_eq!(answer_for(&response, &jira), Some(FilterAnswer::Answered));
}

/// **What the second round trip costs**, measured rather than asserted to be
/// fine.
///
/// Issue #141 adds one probe per in-scope source to every query that filters by
/// author, on the launcher's hot path -- so the claim "two `exists` probes are
/// affordable" is one this repo does not get to make without a number behind
/// it. `#[ignore]`d for the same reason `tests/perf.rs` is: seeding 100,000
/// rows costs tens of seconds and a timing on a shared runner is a coin flip.
///
/// ```text
/// cargo test -p knobas-search --test coverage -- --ignored --nocapture
/// ```
///
/// # The two directions, and why both are printed
///
/// The probe is cheap exactly when it finds what it is looking for. So the
/// fixture measures both:
///
/// * **`answered`** -- a source whose corpus carries authors. The `exists`
///   stops at the first row, and the reading is a fixed small cost whatever the
///   corpus size.
/// * **`no_values`** -- the case #141 exists for. There is no row to stop at,
///   so proving the absence reads that source's rows. This is the expensive
///   direction, it is the one that will actually be hit by a TeamCity, and it
///   is the number worth watching.
///
/// The gate is on the sum against the launcher's own budget (spec §14: local
/// search under 100 ms), because the user does not experience the probe on its
/// own -- they experience the keystroke.
#[tokio::test]
#[ignore = "seeds 100k rows; run deliberately with --ignored"]
async fn the_author_probe_stays_inside_the_launchers_budget() {
    /// Spec §14's budget, which the whole keystroke has to fit inside.
    const BUDGET_MS: u128 = 100;
    /// The exit criterion's corpus size, and one smaller point, so a cost that
    /// grows with the corpus is distinguishable from a fixed one.
    const SIZES: [i64; 2] = [25_000, 100_000];

    let pool = pool().await;
    knobas_search::testing::seed_sources(&pool).await.unwrap();

    for size in SIZES {
        knobas_search::testing::seed_corpus(&pool, size)
            .await
            .unwrap();
        // The bench corpus authors every row. TeamCity's share of it is
        // stripped, which is the corpus #106 measured on a live server: builds
        // are VCS-triggered and name no user.
        sqlx::query("update sync.item set author = null where source_id = 'teamcity'")
            .execute(&pool)
            .await
            .unwrap();
        knobas_search::testing::prepare(&pool).await.unwrap();

        let s = searcher(&pool);
        // The same query with and without the author filter, so the difference
        // *is* the probe rather than an absolute nobody can attribute.
        let plain = worst_of_ten(&s, "payout").await;
        let authored = worst_of_ten(&s, "@mara.lindqvist payout").await;
        let (mirrored,): (i64,) = sqlx::query_as("select count(*) from sync.live_item")
            .fetch_one(&pool)
            .await
            .unwrap();
        let (unauthored,): (i64,) =
            sqlx::query_as("select count(*) from sync.live_item where source_id = 'teamcity'")
                .fetch_one(&pool)
                .await
                .unwrap();

        println!(
            "{mirrored:>7} rows ({unauthored} of them a source with no authors at all): \
             plain {plain:>3} ms, author-filtered {authored:>3} ms, probe {} ms",
            authored.saturating_sub(plain)
        );
        assert!(
            authored <= BUDGET_MS,
            "an author query over {mirrored} rows took {authored} ms, past the \
             {BUDGET_MS} ms budget"
        );
    }
}

/// The slowest of ten runs of one query, in milliseconds.
///
/// The worst rather than the mean: one keystroke in ten taking half a second is
/// a launcher that feels broken, however good the average is. It also asserts
/// the query it timed actually carried a report, so a loop that quietly stopped
/// exercising the probe cannot go on printing a number.
async fn worst_of_ten(searcher: &Searcher, raw: &str) -> u128 {
    let mut max = 0_u128;
    for _ in 0..10 {
        let started = std::time::Instant::now();
        let response = searcher.search(q(raw)).await.unwrap();
        max = max.max(started.elapsed().as_millis());
        assert_eq!(
            response.coverage.is_empty(),
            !raw.contains('@'),
            "{raw} measured a path that is not the one it is named for"
        );
    }
    max
}
