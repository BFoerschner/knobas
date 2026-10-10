//! A deterministic corpus, for the perf gate and the bench to share.
//!
//! Behind `test-util`: `cfg(test)` is set for neither this crate's `tests/`
//! directory nor its `benches/`, and both need this.
//!
//! # Deterministic by construction
//!
//! No `rand`. Every value is modular arithmetic over a fixed word array, so
//! two runs at the same size produce byte-identical rows and two measurements
//! are comparable. A seeded RNG would be reproducible too, and would make the
//! corpus depend on an implementation detail of a crate version.
//!
//! # Why [`seed_corpus`] vacuums, and why that is not a detail
//!
//! **`ANALYZE` is not enough to benchmark a GIN index.** `fastupdate` (on by
//! default) parks freshly inserted entries in an unsorted **pending list**
//! that every index scan reads linearly until a `VACUUM` merges it, and
//! `ANALYZE` does not flush it. The plan for this task said `analyze
//! sync.item`; that alone would have measured the pending list rather than the
//! index.
//!
//! This is not a hypothetical. It is [`crate::lists`]' recorded finding, taken
//! across eleven readings, five corpus sizes, two harness shapes and both
//! autovacuum settings, and the numbers are large and *not a trend*: fresh
//! single loads inflated 182× / 121× at 12,800 rows, 58.8× at 100,000, 24.2×
//! at 200,000 and **86.5× at 300,000**. The severity is governed by the
//! increment since the last merge -- crossing `gin_pending_list_limit` (4 MB)
//! merges most of the list and the residue is bounded by that limit, not by
//! row count -- so **there is no size exemption and no harness exemption**,
//! and the largest fixture looking saner is the trap rather than the
//! reassurance. Autovacuum does not rescue it either: measured as a 2×2 at
//! 100,000 rows, the setting moved the ratio by under 2% while harness shape
//! moved it 2.8×.
//!
//! So the corpus generator owns the vacuum rather than leaving it to each
//! caller to remember. A benchmark whose fixture preparation is a step someone
//! has to recall is a benchmark that will one day be run without it, and the
//! result will look plausible.
//!
//! # What task 10 added to that record: the penalty is paid **per index scan**
//!
//! The inherited record says the artifact's severity is governed by the
//! increment since the last merge. That is the size of the penalty. It is only
//! half the model, and the missing half is why one statement in this stream
//! inflated 182x while another barely moves.
//!
//! Measured here on one 100,000-row fixture, `pgstatginindex` read directly
//! rather than inferred from a timing. **Two curves, four points each.**
//!
//! *The penalty is linear in the number of index scans.* A correlated `exists`
//! probe, one index search per candidate row, against a 222-page pending list:
//!
//! | probes | clean | dirty | delta | per probe |
//! |---|---|---|---|---|
//! | 500 | 21 ms | 49 ms | 28 ms | 56.0 us |
//! | 1,000 | 22 ms | 87 ms | 65 ms | 65.0 us |
//! | 2,000 | 25 ms | 162 ms | 137 ms | 68.5 us |
//! | 4,000 | 29 ms | 313 ms | 284 ms | 71.0 us |
//!
//! *And linear in the size of the pending list*, at 2,000 probes:
//!
//! | rows dirtied | pending pages | per probe |
//! |---|---|---|
//! | 2,500 | 157 | 62.0 us |
//! | 5,000 | 313 | 151.0 us |
//! | 10,000 | **111** | 32.0 us |
//! | 20,000 | 221 | 61.0 us |
//!
//! So **penalty ~ pending_pages x 0.3-0.5 us x scans**, and the consequence is
//! the one that matters for this crate: [`crate::Searcher::search`] performs
//! **one** index scan, so on this fixture an unvacuumed index costs it about
//! 1 ms -- a ratio of 1.0-1.1x, not 182x. The removed `cross-key` list probed
//! once per ticket in a 14-day window, which is where the large ratios in
//! [`crate::lists`]' table come from. Neither number was wrong; they were
//! measuring statements with a thousandfold difference in scan count.
//!
//! Two things worth carrying:
//!
//! * **The `10,000` row above is the inherited "not a trend" finding
//!   reproduced from a new angle.** Dirtying four times as many rows as the
//!   2,500 case left *fewer* pending pages (111 against 157), because crossing
//!   `gin_pending_list_limit` merges most of the list unasked. Pages, not rows
//!   written, is the quantity that governs -- and pages are not monotonic in
//!   anything a benchmark author controls.
//! * **Vacuum anyway.** A one-scan statement is nearly immune *on this
//!   fixture*, and that is a measurement rather than a licence: the immunity is
//!   a property of the scan count, and the next smart list, suggestion pass or
//!   link detector to be written may well probe per row. The generator vacuums
//!   so that no future author has to know which shape they wrote.

use sqlx::PgPool;

use crate::SearchError;

/// How wide the filler vocabulary is.
///
/// **This number is what makes a query's selectivity a property of the fixture
/// rather than an accident of it**, and getting it wrong is how the plan's
/// original generator produced a case list that lied. That generator drew 25
/// body words from a **20-word** vocabulary, so every word was in 40-75% of the
/// rows: measured on it, `payout` ("common word") matched 40,000 of 100,000 and
/// `tombstone` ("rare word") matched **60,000**. The rare case was the
/// commonest one on the list, and nothing on the face of the benchmark said so.
///
/// With 512 filler words and 20 drawn per row, one filler word is in about
/// 20/512 = 3.9% of rows. The domain markers below are then placed at chosen
/// frequencies on top, so "common" and "rare" mean what they say.
/// Named for the reader; the statement below spells the same 512 literally,
/// because it has to be one `&'static str` (see [`INSERT_ITEMS`]).
pub const FILLER_WORDS: i64 = 512;

/// How many filler words each row's `body_text` carries.
///
/// **This is the fixture's text richness, and it is the variable that decides
/// the headline number.** The stream's own record puts one statement at 57 ms
/// on a light fixture and 174 ms on a rich one -- a range, not a point, with
/// richness as the deciding variable. Twenty filler words plus the markers is a
/// deliberate middle: a ticket description is prose, not a title, and a corpus
/// of titles would report a number no real mirror can reproduce.
pub const BODY_WORDS: i64 = 20;

/// A filler word is in about [`BODY_WORDS`]/[`FILLER_WORDS`] of the rows.
///
/// Stated so a case that searches for one can say what it expects, and
/// measured by [`match_count`] so the expectation is never the evidence.
#[must_use]
pub fn filler_share() -> f64 {
    BODY_WORDS as f64 / FILLER_WORDS as f64
}

/// The markers, how often each appears, and why these frequencies.
///
/// | marker | every nth row | share | at 100 k |
/// |---|---|---|---|
/// | `ledger` | 1 | 100% | 100,000 |
/// | `payout` | 20 | 5% | 5,000 |
/// | `backoff` | 40 | 2.5% | 2,500 |
/// | `settlement` | 35 | 2.9% | 2,857 |
/// | `mandate` | 60 | 1.7% | 1,667 |
/// | `sepa retry` (adjacent) | 150 | 0.67% | 667 |
/// | `tombstone` | 1000 | 0.1% | 100 |
///
/// Chosen on what a real launcher query looks like rather than on what makes a
/// number pass: a person searching a 100,000-item mirror is looking for tens to
/// a few thousand things. A term matching 40% of the corpus is not a search, it
/// is a browse -- and the launcher has a separate, separately measured browse
/// path for that.
///
/// `ledger` is in **every** row on purpose. It is the pathological case the
/// budget cannot cover, it is measured and printed by `tests/perf.rs`, and it
/// is what stops this fixture being a fixture chosen because it passes.
///
/// **The table above is documentation; the perf test prints the *measured*
/// count for every case** ([`match_count`]), so a frequency that drifts from
/// this comment shows up in the output rather than in nobody's memory.
///
/// The statement below is one `&'static str`. It is written out rather than
/// formatted from the table above because the runtime-SQL wrapper is confined
/// to [`crate::sql`], and `tests/it/sql_containment.rs` fails the build if any
/// other file in this crate so much as **names** that type -- prose included,
/// which is why this sentence does not. An earlier draft of this module built
/// the statement with `format!` and reached for the wrapper; the scan caught
/// it, which is the whole reason the scan reads raw text rather than stripped
/// comments.
const INSERT_ITEMS: &str = r"
insert into sync.item (entity_id, source_id, kind, title, body_text, author,
                       item_updated_at, synced_at, payload)
select 'bench:' || g,
       ($3::text[])[1 + (g % 3)],
       ($2::text[])[1 + (g % 5)],
       'BEN-' || g || ' w' || ((g * 37 + 101) % 512),
       (select string_agg('w' || ((g * 37 + i * 101) % 512), ' ')
          from generate_series(1, 20) i)
         || ' ledger'
         || case when g % 20   = 0 then ' payout'     else '' end
         || case when g % 40   = 0 then ' backoff'    else '' end
         || case when g % 35   = 0 then ' settlement' else '' end
         || case when g % 60   = 0 then ' mandate'    else '' end
         || case when g % 1000 = 0 then ' tombstone'  else '' end
         || case when g % 150  = 0 then ' sepa retry' else '' end,
       ($4::text[])[1 + (g % 3)],
       now() - make_interval(mins => (g % 100000)::int),
       now() - make_interval(secs => (g % 3600)::int),
       jsonb_build_object('n', g)
  from generate_series(1, $1) g
on conflict (entity_id) do nothing";

/// The kinds the corpus spreads across, in the launcher's group order.
const KINDS: [&str; 5] = ["ticket", "pr", "build", "page", "commit"];

/// The sources the corpus spreads across.
const SOURCES: [&str; 3] = ["jira", "gitea", "teamcity"];

/// The authors the corpus spreads across. The first is what `@me` resolves to
/// when [`seed_sources`] has run.
const AUTHORS: [&str; 3] = ["mara.lindqvist", "jonas.becker", "priya.nair"];

/// Fill the mirror with `rows` deterministic items, and make it measurable.
///
/// One transaction for the inserts, then `vacuum (analyze)` outside it --
/// `VACUUM` cannot run inside a transaction block, which is the mechanical
/// reason the plan's single-statement `analyze` was easier to write and the
/// wrong thing to write.
///
/// # Errors
///
/// [`SearchError::Db`] if any statement fails.
///
/// # Panics
///
/// Never; `rows` is clamped to a positive count by `generate_series`.
pub async fn seed_corpus(pool: &PgPool, rows: i64) -> Result<(), SearchError> {
    let kinds = KINDS.map(str::to_owned);
    let sources = SOURCES.map(str::to_owned);
    let authors = AUTHORS.map(str::to_owned);

    let mut tx = pool.begin().await?;
    sqlx::query(
        r"
        insert into knobas.entity (id, kind, title, updated_at)
        select 'bench:' || g,
               ($2::text[])[1 + (g % 5)],
               'BEN-' || g,
               now() - make_interval(mins => (g % 100000)::int)
          from generate_series(1, $1) g
        on conflict (id) do nothing",
    )
    .bind(rows)
    .bind(&kinds[..])
    .execute(&mut *tx)
    .await?;

    sqlx::query(INSERT_ITEMS)
        .bind(rows)
        .bind(&kinds[..])
        .bind(&sources[..])
        .bind(&authors[..])
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    prepare(pool).await
}

/// Make the mirror measurable: merge the GIN pending list, then re-estimate.
///
/// Separate and public because it is the step a caller who seeds by some other
/// route still has to take, and because naming it is how the reason survives.
///
/// `vacuum (analyze)` rather than two statements: the vacuum is what merges
/// `item_fts_idx`'s pending list (see the module docs -- without it a scan
/// reads that list linearly and the measurement is of the list, not the
/// index), and the analyze is what stops the planner working from an
/// empty-table estimate and choosing a plan nobody will ever run in
/// production.
///
/// # Errors
///
/// [`SearchError::Db`] if the statement fails.
pub async fn prepare(pool: &PgPool) -> Result<(), SearchError> {
    sqlx::query("vacuum (analyze) sync.item")
        .execute(pool)
        .await?;
    sqlx::query("analyze knobas.entity").execute(pool).await?;
    Ok(())
}

/// Register the three sources the corpus attributes rows to.
///
/// Without these, `/ji` resolves to nothing and `@me` has no identity, so two
/// of the benchmark's ten cases would silently measure an *empty* filter
/// rather than the filtered path they are named after.
///
/// # Errors
///
/// [`SearchError::Db`] if the statement fails.
pub async fn seed_sources(pool: &PgPool) -> Result<(), SearchError> {
    for (id, kind, name) in [
        ("jira", "jira", "Jira"),
        ("gitea", "gitea", "Gitea"),
        ("teamcity", "teamcity", "TeamCity"),
    ] {
        sqlx::query(
            r#"insert into knobas.source_config
                 (id, kind, display_name, base_url, auth_kind, config)
               values ($1, $2, $3, 'http://x', 'Pat',
                       case when $1 = 'jira'
                            then '{"username":"mara.lindqvist"}'::jsonb
                            else '{}'::jsonb end)
               on conflict (id) do update set enabled = true"#,
        )
        .bind(id)
        .bind(kind)
        .bind(name)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Fill `knobas.note` with `rows` deterministic notes, and make them
/// measurable.
///
/// **The branch of the launcher's union nothing else seeds.** [`seed_corpus`]
/// fills the mirror and [`seed_estate`] fills the estate's two corpora; without
/// this one a statement built over `corpus::ALL` reaches `knobas.note` at zero
/// rows, and a branch with no rows is a branch the planner has no choice to
/// make in. A plan pinned over such a fixture is a plan pinned over one corpus
/// wearing the name of four, which is what issue #541 was filed about.
///
/// # The shape, and why
///
/// The same `w<n>` filler vocabulary and the same twenty body words per row as
/// the mirror ([`FILLER_WORDS`], [`BODY_WORDS`]), so a query written for one
/// fixture is selective in the same way over the other and two branches of one
/// union are comparable -- [`seed_estate`]'s rule, for [`seed_estate`]'s
/// reason.
///
/// **`ledger` is in every row**, exactly as it is in every row of the mirror
/// and for the same purpose: it is the pathological term `tests/perf.rs` takes
/// its plan pin on, and a note branch matching none of it would contribute an
/// index probe and no work behind it. None of the mirror's other markers are
/// here. A note is not a ticket, and a fixture whose every corpus answered
/// every case would have stopped being able to tell them apart.
///
/// `body_md` is markdown, as [`crate::corpus::NOTE`] says it is, and the filler
/// is prose: what the index holds is the words, and `#` and `[[ref]]`
/// punctuation would add a shape to the fixture without adding a lexeme to it.
///
/// Idempotent: every id is `note:bench-<n>`, so two calls insert once.
/// `vacuum (analyze)` outside the transaction, for [`prepare`]'s reason -- a
/// GIN index whose pending list has not been merged is read linearly, and the
/// measurement is then of the list.
///
/// # Errors
///
/// [`SearchError::Db`] if any statement fails.
pub async fn seed_notes(pool: &PgPool, rows: i64) -> Result<(), SearchError> {
    let mut tx = pool.begin().await?;
    // A note lives in the address space (`note_entity_fk`, migration 0006), so
    // the entity row comes first and the body hangs off it -- the order
    // `knobas_core::note::create` writes in.
    sqlx::query(
        r"
        insert into knobas.entity (id, kind, title, updated_at)
        select 'note:bench-' || g, 'note', 'NOTE-' || g,
               now() - make_interval(mins => (g % 100000)::int)
          from generate_series(1, $1) g
        on conflict (id) do nothing",
    )
    .bind(rows)
    .execute(&mut *tx)
    .await?;

    sqlx::query(INSERT_NOTES)
        .bind(rows)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    sqlx::query("vacuum (analyze) knobas.note")
        .execute(pool)
        .await?;
    sqlx::query("analyze knobas.entity").execute(pool).await?;
    Ok(())
}

/// One `&'static str`, for [`INSERT_ITEMS`]' reason: `tests/it/sql_containment.rs`
/// fails the build if any file in this crate but `crate::sql` so much as names
/// the runtime-SQL wrapper, and a statement built with `format!` reaches for
/// it. So the `512` and the `20` below are [`FILLER_WORDS`] and [`BODY_WORDS`]
/// spelled literally, exactly as [`INSERT_ITEMS`] spells them, and the two
/// statements have to be changed together or the two fixtures stop being drawn
/// from one vocabulary.
const INSERT_NOTES: &str = r"
insert into knobas.note (id, title, body_md, created_at, updated_at)
select 'note:bench-' || g,
       'NOTE-' || g || ' w' || ((g * 37 + 101) % 512),
       (select string_agg('w' || ((g * 37 + i * 101) % 512), ' ')
          from generate_series(1, 20) i)
         || ' ledger',
       now() - make_interval(mins => (g % 100000)::int),
       now() - make_interval(mins => (g % 100000)::int)
  from generate_series(1, $1) g
on conflict (id) do nothing";

/// How many rows one query matches -- the fixture property the timings depend
/// on.
///
/// Printed beside every case by the perf test, because a case called "rare
/// word" that matches 60% of the corpus is a label standing in for a fact, and
/// the plan's original fixture produced exactly that.
///
/// # Errors
///
/// [`SearchError::Db`] if the statement fails.
pub async fn match_count(pool: &PgPool, text: &str) -> Result<i64, SearchError> {
    let (n,): (i64,) = sqlx::query_as(
        "select count(*) from sync.live_item i
          where i.fts @@ websearch_to_tsquery('english', $1)",
    )
    .bind(text)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// How much of `item_fts_idx` is sitting unmerged in the pending list.
///
/// **The mechanism itself, not a proxy for it.** `fastupdate` (on by default --
/// `item_fts_idx` declares no `reloptions`, so it takes the default) parks
/// freshly inserted entries in an unsorted pending list that every index scan
/// reads linearly until a `VACUUM` merges it, and `ANALYZE` does not flush it.
///
/// An earlier version of this helper returned the index's size in **pages**,
/// which conflates the pending list with the index itself: on a run where the
/// list had already auto-merged, the number was identical before and after the
/// vacuum and the caller could not tell "there was nothing pending" from "the
/// probe cannot see it". `pgstatginindex` reports the pending list directly,
/// which turns the harness's own claim from a citation into a measurement.
///
/// Needs `pgstattuple`, which the embedded server ships and this creates on
/// demand. Test support only, so an extension is a fair price for an exact
/// observable.
///
/// # Errors
///
/// [`SearchError::Db`] if the extension cannot be created or the index cannot
/// be read.
pub async fn pending_list(pool: &PgPool) -> Result<(i32, i64), SearchError> {
    sqlx::query("create extension if not exists pgstattuple")
        .execute(pool)
        .await?;
    let row: (i32, i64) = sqlx::query_as(
        "select pending_pages, pending_tuples from pgstatginindex('sync.item_fts_idx')",
    )
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// Leave `rows` rows' worth of entries unmerged in the FTS pending list.
///
/// The state [`prepare`] exists to clear, created on purpose so a test can show
/// that clearing it does something -- *when you build a detector, mutate the
/// detector*. The increment is deliberately modest: crossing
/// `gin_pending_list_limit` (4 MB) makes PostgreSQL merge most of the list
/// unasked, so a large dirtying step can leave the index **cleaner** than a
/// small one. That is the same mechanism that makes the artifact's severity
/// depend on where a load stopped rather than on corpus size.
///
/// # Errors
///
/// [`SearchError::Db`] if the update fails.
pub async fn dirty_the_pending_list(pool: &PgPool, rows: i64) -> Result<(), SearchError> {
    sqlx::query(
        "update sync.item set body_text = body_text || ' reindexed'
          where entity_id like 'bench:%'
            and (regexp_replace(entity_id, '^bench:', '')::bigint) <= $1",
    )
    .bind(rows)
    .execute(pool)
    .await?;
    // The half of `prepare` that is *not* the vacuum, so the planner is honest
    // and the only difference left is the pending list.
    sqlx::query("analyze sync.item").execute(pool).await?;
    Ok(())
}

/// How many assets one estate level holds, from the root down.
///
/// A **shape** rather than a count, because that is what the estate is: three
/// sites, a hundred machines under each of them, and ten containers on every
/// machine. Multiplied out it is 3 + 300 + 3,000 = 3,303 assets, which is
/// larger than any real estate the launcher will meet and is chosen for that
/// reason -- the number it is measured against has to be an upper bound rather
/// than a plausible one.
///
/// Three levels and not two, because the depth is what `path_text` costs: a
/// container's path carries its VM's name *and* its site's, and weight B is
/// matched against the whole of it.
const ESTATE_SHAPE: [i64; 3] = [3, 100, 10];

/// One route per this many assets, on the deepest level only.
///
/// Routes hang off containers in the real estate -- every one of the nine in
/// `testenv/hetzner/estate.json` does -- and one in two is a good deal denser
/// than that file, which is again the point of an upper bound.
const ROUTES_PER: i64 = 2;

/// Fill the estate with a deterministic tree of assets and the routes they
/// expose, and make it measurable.
///
/// The half of the launcher's corpus [`seed_corpus`] cannot reach: `asset:` and
/// a plain query both union `corpus::ASSET` and `corpus::ROUTE`, and a mirror
/// of 100 k rows beside an estate of nothing measures three of the four
/// branches at zero. Property values and an ancestor path per row, because
/// those are what migrations `0017` and `0019` put in the index and therefore
/// what a keystroke actually matches against.
///
/// The names carry the same `w<n>` filler vocabulary the mirror's do, so a
/// query written for one fixture is selective in the same way over the other
/// and the two halves of a union are comparable.
///
/// Idempotent per `tag`: every id is `asset:<tag>-<n>`, so two calls with the
/// same tag insert once. `vacuum (analyze)` outside the transaction, for
/// [`prepare`]'s reason -- a GIN index whose pending list has not been merged
/// is read linearly, and the measurement is then of the list.
///
/// # Errors
///
/// [`SearchError::Db`] if any statement fails.
pub async fn seed_estate(pool: &PgPool, tag: &str) -> Result<(), SearchError> {
    let [sites, per_site, per_vm] = ESTATE_SHAPE;

    let mut tx = pool.begin().await?;
    // Level by level, so a child can read its parent's `path_text` from the
    // row that is already there -- the same order `knobas_app::assets::create`
    // writes in, and the only order in which the column can be built by a
    // statement rather than by a recursive CTE.
    let mut parents: Vec<(String, String, String)> =
        vec![(String::new(), String::new(), String::new())];
    let mut counter: i64 = 0;
    for (level, (width, type_id)) in [(sites, "site"), (per_site, "vm"), (per_vm, "container")]
        .into_iter()
        .enumerate()
    {
        let mut next = Vec::with_capacity(parents.len() * usize::try_from(width).unwrap_or(1));
        for (parent_id, parent_name, parent_path) in &parents {
            for _ in 0..width {
                counter += 1;
                let id = format!("asset:{tag}-{counter}");
                let name = format!("{type_id}-{counter} w{}", (counter * 37 + 101) % 512);
                let path = if level == 0 {
                    String::new()
                } else if parent_path.is_empty() {
                    parent_name.clone()
                } else {
                    format!("{parent_path} / {parent_name}")
                };
                sqlx::query(
                    "insert into knobas.entity (id, kind, title) values ($1, 'asset', $2)
                     on conflict (id) do nothing",
                )
                .bind(&id)
                .bind(&name)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "insert into knobas.asset
                       (id, parent_id, type_id, name, properties, path_text)
                     values ($1, $2, $3, $4,
                             jsonb_build_object('hostname', $5::text,
                                                'ip', '10.' || ($6::bigint % 250) || '.0.1',
                                                'os', 'Debian 13'),
                             $7)
                     on conflict (id) do nothing",
                )
                .bind(&id)
                .bind(if level == 0 {
                    None
                } else {
                    Some(parent_id.as_str())
                })
                .bind(type_id)
                .bind(&name)
                .bind(format!("host-{counter}"))
                .bind(counter)
                .bind(&path)
                .execute(&mut *tx)
                .await?;
                if level == 2 && counter % ROUTES_PER == 0 {
                    let route = format!("route:{tag}-{counter}");
                    sqlx::query(
                        "insert into knobas.entity (id, kind, title) values ($1, 'route', $2)
                         on conflict (id) do nothing",
                    )
                    .bind(&route)
                    .bind(&name)
                    .execute(&mut *tx)
                    .await?;
                    // `http://127.0.0.1:<port>/`, which is the shape every
                    // route in `testenv/hetzner/estate.json` has -- and the
                    // shape a port is *reachable* in: a path fuses the port
                    // into one lexeme with it (`corpus::ROUTE`'s table), so a
                    // fixture with one would measure a query that matches
                    // nothing while looking like it had found a route.
                    sqlx::query(
                        "insert into knobas.route (id, asset_id, name, url)
                         values ($1, $2, $3, 'http://127.0.0.1:' || (30000 + ($4::bigint % 9000)) || '/')
                         on conflict (id) do nothing",
                    )
                    .bind(&route)
                    .bind(&id)
                    .bind(&name)
                    .bind(counter)
                    .execute(&mut *tx)
                    .await?;
                }
                next.push((id, name, path));
            }
        }
        parents = next;
    }
    tx.commit().await?;

    // Both GIN indexes, for `prepare`'s reason: a pending list that has not
    // been merged is read linearly and the measurement is of the list.
    sqlx::query("vacuum (analyze) knobas.asset")
        .execute(pool)
        .await?;
    sqlx::query("vacuum (analyze) knobas.route")
        .execute(pool)
        .await?;
    sqlx::query("analyze knobas.entity").execute(pool).await?;
    Ok(())
}

/// How many assets and routes [`seed_estate`] writes.
///
/// Derived from [`ESTATE_SHAPE`] rather than restated, so the report a bench
/// prints cannot disagree with the fixture it measured.
#[must_use]
pub fn estate_size() -> (i64, i64) {
    let [sites, per_site, per_vm] = ESTATE_SHAPE;
    let containers = sites * per_site * per_vm;
    (
        sites + sites * per_site + containers,
        containers / ROUTES_PER,
    )
}
