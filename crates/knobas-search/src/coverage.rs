//! Which sources can answer a filter, and which merely have nothing to say
//! (issue #141).
//!
//! # The gap this closes
//!
//! `author:` and `@` shipped in #39. TeamCity authorship shipped in #33, and
//! #106 measured what it is worth: **100 of 100** newest finished builds on a
//! live server name no user, because real CI builds are VCS-triggered. The
//! field is correctly absent, not mis-read -- so `author:someone` over a corpus
//! that includes TeamCity comes back empty, and until now *nothing on the wire
//! distinguished that from a token that is broken*.
//!
//! Björn ruled option 1 on 2026-08-31: the answer belongs on the response,
//! per-source and per-query. This module is the measurement behind it.
//!
//! # Measured on the corpus, and deliberately not on the match set
//!
//! The probe asks one question per source: **does anything this source
//! contributed to this query's corpus carry an author?** The line between what
//! narrows it and what does not is the whole design, and it is not the line
//! between "filter" and "no filter":
//!
//! * **the query's structural scope narrows it** -- `source:` and `kinds`, the
//!   two dimensions that decide *which of a source's rows are in this search at
//!   all*. A report about sources the query excluded is a report about somebody
//!   else's question, and `note: @jonas` naming a build server would explain an
//!   absence that authorship had nothing to do with;
//! * **the match set does not** -- neither the text nor the `updated:` window.
//!   Narrowed by those, a source whose authored items simply did not match the
//!   words typed would report as unable to answer, which collapses the two
//!   states this report exists to keep apart. Measured over the corpus,
//!   [`FilterAnswer::NoValues`] means *"an author query cannot be answered
//!   here"*, a claim that is still true on the next keystroke.
//!
//! `updated:` is on the match-set side of that line and it is the one judgement
//! call here. It is a *recency* window rather than a scope: "nothing in the last
//! week names a person" is a fact about a week, and reporting it as "this source
//! cannot answer an author query" would be the sparsity threshold the next
//! section refuses, arrived at sideways.
//!
//! A source with **no rows at all** in that scope is left out of the report
//! rather than given a verdict. It contributed no corpus, so authorship is not
//! why it is absent from the results, and saying anything about it here would
//! explain the wrong absence -- the same reason `Searcher::search`'s
//! short-circuits report nothing for `asset:`.
//!
//! # Why existence and not sparsity
//!
//! One authored row flips a source to [`FilterAnswer::Answered`], and that is
//! the intended line. "Under n% of this corpus names a person" would be a
//! threshold -- a judgement nobody ruled, and one that would call a source
//! unable to answer a query it can in fact answer. Strict existence is the only
//! non-arbitrary boundary, and it is the boundary the ruling named.
//!
//! # What it costs, measured
//!
//! One statement, and only for a query that actually filtered by author -- an
//! ordinary keystroke pays nothing.
//!
//! **There is no index that can answer this and the shape was chosen on
//! measurements, not on taste.** "Does this source have an authored row" has no
//! index behind it (`item_source_updated_idx` is `(source_id, item_updated_at)`
//! and carries no author), so proving an *absence* reads that source's rows
//! however it is written. Measured on the 100,000-row fixture with a third of
//! it unauthored, `explain (analyze)` on four formulations:
//!
//! | formulation | execution |
//! |---|---|
//! | correlated `exists` per source over `unnest` | 28.6 ms |
//! | one grouped aggregate over `sync.live_item` | **30.0 ms** |
//! | one grouped aggregate over `sync.item` | 13.7 ms |
//! | a single `exists`, scalar bind, one source | 16.7 ms |
//!
//! The correlated form is not chosen even though it measured slightly faster,
//! because its cost is *per source*: it plans two sub-scans each, and PostgreSQL
//! will not use the source index for them (a source holding a third of the
//! mirror is cheaper to scan than to look up 33,000 times, and it stops at the
//! first row it finds -- which for the source being reported on never comes). A
//! sixth configured source would double it. The grouped aggregate is **one
//! parallel scan whatever the source count**, which is the property worth
//! having.
//!
//! `sync.item` is the cheap one and is **not** used: half of that statement's
//! cost is the tombstone join, and dropping it would let an authored item a
//! source has since withdrawn vouch for a capability the live corpus no longer
//! has -- reinstating this issue's own silence in the one case where it matters
//! most, a source that has *stopped* naming people. The 16 ms buys correctness.
//!
//! The end-to-end reading is
//! `tests/coverage.rs::the_author_probe_stays_inside_the_launchers_budget`:
//! **47 ms** for the whole keystroke over the exit criterion's 100,000-row
//! corpus with a third of it a source that names nobody, against 28 ms for the
//! same query unfiltered. It is `#[ignore]`d and therefore **not an automatic
//! gate** -- seeding 100,000 rows costs tens of seconds and a timing on a shared
//! runner is a coin flip, which is `tests/perf.rs`'s reasoning and the same
//! trade. It is a measurement anyone can re-run, not a promise CI keeps.

use sqlx::PgPool;

use crate::SearchError;
use crate::query::EffectiveFilters;
use crate::types::{FilterAnswer, FilterCoverage, FilterDimension, SourceAnswer};
use crate::vocab::{SourceVocab, Vocabulary};

/// One row per named source that put **anything** into this query's corpus.
///
/// A source with nothing in scope contributes no row, and [`coverage_of`] leaves
/// it out of the report entirely: the grouping saying "there was nothing here to
/// aggregate" is exactly the state where authorship is not the reason a source
/// is absent from the results.
///
/// **`$2` is the kind scope, and the `cardinality` guard is what keeps this one
/// statement.** An empty array means "every kind", so the predicate has to
/// disappear rather than match nothing -- and writing that as a conditional
/// fragment would be runtime SQL, which roadmap §4 gotcha 2 confines to
/// [`crate::sql`]. A guard the planner folds away is the cheaper answer and
/// keeps this module out of that business entirely.
///
/// `nullif(author, '')` rather than `author is not null`: an author column that
/// is present and blank is nobody, the same reading `Vocabulary::load` already
/// gives a blank configured username. A source that writes `''` where it means
/// "no user" must not read as able to answer a question it cannot.
///
/// `sync.live_item`, never `sync.item` -- see the module docs for what that
/// costs and why it is worth it. The columns are named rather than `select *`ed:
/// the view carries a `tsvector` and reading one into a `FromRow` struct panics
/// at run time (interfaces §1).
const AUTHOR_COVERAGE_SQL: &str = r"
select i.source_id                            as source_id,
       count(nullif(i.author, '')) > 0        as authored
  from sync.live_item i
 where i.source_id = any($1)
   and (cardinality($2::text[]) = 0 or i.kind = any($2))
 group by i.source_id
";

/// What the probe answered about one source that put rows in scope.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
struct SourceProbe {
    source_id: String,
    /// Whether any of them carries a non-empty author.
    authored: bool,
}

/// The author-dimension coverage for one query, or nothing to report.
///
/// Empty -- and **no round trip** -- unless the query actually filtered by
/// author. `mine` counts: `@me` runs through the same `author = any(...)`
/// predicate and hits the same gap.
///
/// # Errors
///
/// [`SearchError::Db`] if the probe fails.
pub(crate) async fn author_coverage(
    pool: &PgPool,
    vocab: &Vocabulary,
    filters: &EffectiveFilters,
) -> Result<Vec<FilterCoverage>, SearchError> {
    if !filters.filters_by_author() {
        return Ok(Vec::new());
    }
    let scoped = in_scope(vocab, filters);
    if scoped.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = scoped.iter().map(|source| source.id.clone()).collect();
    let probes: Vec<SourceProbe> = sqlx::query_as(AUTHOR_COVERAGE_SQL)
        .bind(&ids)
        .bind(&filters.kinds)
        .fetch_all(pool)
        .await?;

    let coverage = coverage_of(&scoped, &probes);
    // Every source in scope contributed nothing to this query's corpus, so
    // there is no author gap to explain -- only a kind or a source filter that
    // matched nothing, which the results already say.
    Ok(if coverage.sources.is_empty() {
        Vec::new()
    } else {
        vec![coverage]
    })
}

/// The configured sources this query is narrowed to, in vocabulary order.
///
/// No `source:` filter is every source in the vocabulary -- the enabled ones,
/// the world the grammar can name. A **disabled** source's rows are still in
/// the mirror and can still match a plain search, but `/alias` and `source:`
/// stop resolving to it, and a verdict on a source the user turned off would
/// accuse something the sources list no longer shows. A `source:` filter
/// naming something the vocabulary does not have contributes nothing -- the
/// parser already reports such a token as unknown, and inventing a coverage
/// row for it would have the report claim a source exists.
fn in_scope<'a>(vocab: &'a Vocabulary, filters: &EffectiveFilters) -> Vec<&'a SourceVocab> {
    vocab
        .sources
        .iter()
        .filter(|source| filters.sources.is_empty() || filters.sources.contains(&source.id))
        .collect()
}

/// Turn the probes into the report: every scoped source that has a corpus here,
/// in the scope's order.
///
/// Pure, and separate from the round trip, so every verdict is testable without
/// a database. The **scope** drives the order and the names, and the **probe**
/// decides membership: a source with no rows in scope has no verdict to give,
/// because whatever it is missing from the results, authorship is not the
/// reason. Iterating the probes instead would order the report by whatever the
/// grouping happened to return.
fn coverage_of(scoped: &[&SourceVocab], probes: &[SourceProbe]) -> FilterCoverage {
    FilterCoverage {
        dimension: FilterDimension::Author,
        sources: scoped
            .iter()
            .filter_map(|source| {
                let probe = probes.iter().find(|probe| probe.source_id == source.id)?;
                Some(SourceAnswer {
                    source_id: source.id.clone(),
                    display_name: source.display_name.clone(),
                    answer: if probe.authored {
                        FilterAnswer::Answered
                    } else {
                        FilterAnswer::NoValues
                    },
                })
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filters(mine: bool, named: &[&str], sources: &[&str]) -> EffectiveFilters {
        EffectiveFilters {
            sources: sources.iter().map(|s| (*s).to_owned()).collect(),
            mine,
            named_authors: named.iter().map(|s| (*s).to_owned()).collect(),
            ..EffectiveFilters::default()
        }
    }

    fn probe(source_id: &str, authored: bool) -> SourceProbe {
        SourceProbe {
            source_id: source_id.to_owned(),
            authored,
        }
    }

    /// Only a query that asked an author question is reported on -- which is
    /// also what keeps the probe off the ordinary hot path.
    ///
    /// The predicate is `EffectiveFilters`' own, shared with the statement
    /// builder so the two cannot drift; this is coverage's stake in it.
    #[test]
    fn only_an_author_query_is_reported_on() {
        assert!(filters(false, &["jonas"], &[]).filters_by_author());
        // `@me` is the same predicate and the same gap, and it stays an author
        // query even when knobas does not know who the user is.
        assert!(filters(true, &[], &[]).filters_by_author());
        assert!(
            EffectiveFilters {
                mine: true,
                identity_authors: Vec::new(),
                ..EffectiveFilters::default()
            }
            .filters_by_author()
        );
        assert!(!EffectiveFilters::default().filters_by_author());
        assert!(!filters(false, &[], &["jira"]).filters_by_author());
    }

    #[test]
    fn a_source_filter_narrows_the_report_and_no_filter_covers_everything() {
        let vocab = Vocabulary::fixture();
        let all: Vec<&str> = in_scope(&vocab, &filters(false, &["jonas"], &[]))
            .iter()
            .map(|source| source.id.as_str())
            .collect();
        assert_eq!(all, ["gitea", "jira", "jira-eu", "teamcity"]);

        let narrowed: Vec<&str> = in_scope(&vocab, &filters(false, &["jonas"], &["teamcity"]))
            .iter()
            .map(|source| source.id.as_str())
            .collect();
        assert_eq!(narrowed, ["teamcity"]);

        // A source nobody configured is not invented into the report.
        assert!(in_scope(&vocab, &filters(false, &["jonas"], &["nope"])).is_empty());
    }

    /// Both verdicts, in the scope's order -- and the sources with nothing in
    /// scope left out rather than accused.
    #[test]
    fn a_source_with_nothing_in_scope_gets_no_verdict_at_all() {
        let vocab = Vocabulary::fixture();
        let scoped = in_scope(&vocab, &filters(false, &["jonas"], &[]));
        // Deliberately out of probe order, and two of the four sources
        // deliberately absent: a source with no rows in scope has no group.
        let probes = [probe("teamcity", false), probe("jira", true)];

        let coverage = coverage_of(&scoped, &probes);
        assert_eq!(coverage.dimension, FilterDimension::Author);
        assert_eq!(
            coverage
                .sources
                .iter()
                .map(|s| (s.source_id.as_str(), s.display_name.as_str(), s.answer))
                .collect::<Vec<_>>(),
            [
                ("jira", "Jira", FilterAnswer::Answered),
                // The one the issue is about: a corpus with nothing in it that
                // names a person, which is not the same as an answer of
                // "nobody by that name".
                ("teamcity", "Buildserver", FilterAnswer::NoValues),
            ],
            "gitea and jira-eu put no rows in this query's corpus, so \
             authorship is not why they are absent from the results"
        );
    }
}
