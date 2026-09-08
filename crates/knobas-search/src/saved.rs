//! The lists a reader saved: *Save as list* and what it costs to keep one
//! (#506, spec #491 stories 56-58 and 60).
//!
//! A **saved smart list is a launcher query somebody wrote down**. Migration
//! `0025`'s `knobas.smart_list` holds the raw box text and the name they gave
//! it; everything else on the launcher's rail -- the count, the change badge,
//! the rows, the badge clearing on open -- is computed the same way a built-in
//! computes it, which is the whole point of `CONTEXT.md`'s **Smart list**
//! entry naming both in one sentence.
//!
//! # Why the *text* is stored and not a parse of it
//!
//! Because the box is one line and the backend is what parses it (ruling P2).
//! A stored parse would be a snapshot of one version of §4's grammar, and
//! story 60 -- *"a saved list whose query the grammar no longer accepts shows
//! **needs attention** rather than an error"* -- would be unreachable, because
//! a parse has by construction already been accepted. Keeping the text means
//! the parser of the day is the judge, every time the rail is drawn.
//!
//! # The two answers a saved row can give
//!
//! [`plan`] runs the stored text through the same `query::parse` +
//! `query::merge` a keystroke goes through, and comes back with either
//! something to run or a [`Refusal`]. There is no third answer and no error
//! arm: a row this module cannot make sense of becomes one *needs attention*
//! line on the rail, and the board still draws.
//!
//! **A refusal is not reachable through [`create`].** Creating a list runs
//! [`plan`] first and refuses a query it could not run, so a stored row was
//! always runnable *when it was stored*. That is what makes *needs attention*
//! mean the world changed under a saved list rather than that knobas wrote
//! down something it never understood -- and it is why the test for it inserts
//! its row with SQL: there is no command that can make one.
//!
//! **And nothing ever rewrites a stored query.** There is no editor for one
//! and there is no migration that could be written for one: a data migration
//! that "upgraded" stored text to a newer grammar would be a parse in
//! disguise, committing the migration's reading of what the reader wrote, and
//! it would make *needs attention* a state no row can reach. `CONTEXT.md`'s
//! **Smart list** carries that rule. A refused row is deleted and the search
//! saved again, which is what [`Refusal::description`] offers.
//!
//! # What is *not* a refusal
//!
//! A database fault. [`summaries`] propagates [`SearchError::Db`] with `?` on
//! all three of its reads, so a statement that fails surfaces as `internal`
//! and the launcher says so. A rail of *needs attention* rows means the
//! grammar refused those queries and nothing else; it is never how a broken
//! database looks.
//!
//! # What this module deliberately does not do
//!
//! * **No `list:` inside a saved query.** A saved list of a saved list is an
//!   alias nobody asked for, and one that could be made to point at itself.
//!   [`create`] refuses it and [`plan`] reads an old one as a refusal.
//! * **No share export.** The archive's `smart_lists` part and the toggle the
//!   dialog shows once a saved list exists are #507's, and `CONTEXT.md`'s
//!   **Share** entry says so.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::corpus;
use crate::lists::{self, SmartListSummary};
use crate::query::{self, EffectiveFilters};
use crate::sql::{self, SavedQuery};
use crate::types::SearchFilters;
use crate::vocab::Vocabulary;
use crate::{MAX_FILTER_VALUES, SearchError};

/// What the rail says instead of a count when a saved query no longer runs.
///
/// Public so that this module, the seam tests and the Rust-side pin on the
/// `?fake-ipc` fixture (`lists::the_builtin_registry_matches_its_typescript_fixture`)
/// compare against **one** wording rather than four copies of it --
/// `lists::describe_missing_identity` exists for the same reason.
///
/// **The panel is not one of them, and cannot be**: `Board.svelte` draws these
/// two words as its own literal, because no Rust constant crosses the bridge.
/// What holds them together is `Launcher.test.svelte.ts`'s
/// *a saved list that needs attention says so and does not open*, which reads
/// them off the rendered count cell.
pub const NEEDS_ATTENTION: &str = "Needs attention";

/// Longest id [`create`] will generate.
///
/// A saved list's default label is the query itself, so an id derived from it
/// can be as long as the box: 512 characters is not a name, and `list:` has to
/// stay typeable.
const MAX_ID_CHARS: usize = 48;

/// How many saved lists one installation may keep.
///
/// Every one of them is a `count(*)` over the corpora on the launcher's board
/// path (`sql::saved_summary_sql` folds them into one round trip, which saves
/// the latency and not the scans). The cap is here so that the board's cost
/// has a stated ceiling rather than an unbounded one.
///
/// # Sixteen is a measured number (#533)
///
/// It was **64**, chosen as generous on the reasoning that nobody curates
/// sixty-four saved lists. `knobas-search`'s perf gate now fills the rail
/// through [`create`] over the launcher's own query shapes and times
/// `smart_lists` and `launcher_board` at 100 k items -- spec §14's corpus, and
/// the size the 100 ms budget names. `launcher_board` p90 in milliseconds, on
/// an unloaded machine, 2026-09-08:
///
/// ```text
/// saved lists       0    1    2    4    8   16   32   64
/// the run at 64    41   53   55   60   81   84  132  212
/// the run at 16    42   53   57   60   81   87    -    -
/// ```
///
/// **Two runs and not one**, because no run can reach past the cap it is taken
/// at -- [`create`] refuses the row. The first was taken with this constant
/// still at 64 and is what condemned it; the second at the value below. They
/// agree to within 3 ms, which is what this machine's noise is worth.
///
/// 64 costs **212 ms**, twice the budget, so the guess was wrong by a factor
/// of two and nothing measured it until now. A saved list costs **under 3 ms**
/// on top of an empty board's 41 to 42, which leaves room for **about twenty**;
/// sixteen is that with margin.
///
/// **Eight would not have been meaningfully cheaper** -- 81 ms, against
/// sixteen's 84 and 87 -- because what a rail costs depends on the *shapes* on
/// it as much as on how many: one saved browse over a whole source outweighs
/// several saved searches for a word. Halving the allowance to buy three
/// milliseconds is the trade this number declined.
///
/// Lowering it stays cheap and raising it is not: the budget does not move, so
/// a larger rail is a cheaper statement's to earn, not a constant's.
pub const MAX_SAVED_LISTS: i64 = 16;

/// One row of `knobas.smart_list`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SavedList {
    pub id: String,
    pub label: String,
    /// The raw box text, exactly as it was when *Save as list* was pressed.
    pub query: String,
}

/// Why a saved list cannot be run by today's grammar.
///
/// Each variant is a bound or a rule the *engine* enforces on a live query, so
/// none of them is invented here: a saved row hits exactly the refusals a
/// keystroke would hit, plus the one rule that is this module's own (a saved
/// query may not name another list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The engine itself refused the stored text, with this message.
    ///
    /// Produced by calling `crate::validate` rather than by restating its
    /// rules: that function is where the launcher's bounds on a query live,
    /// and a bound added to it has to reach a saved list on the same commit or
    /// the two drift. Its message is carried through because it already names
    /// the rule and the numbers.
    Rejected(String),
    /// A dimension the **parse** produced carries more values than the engine
    /// accepts.
    ///
    /// Not the same check `crate::validate` makes, which is over the chips a
    /// *caller* sent and which a saved query has none of: this one is over
    /// what §4's grammar made of the stored text.
    TooManyValues {
        dimension: &'static str,
        values: usize,
    },
    /// The query names a smart list, so running it would be a list of a list.
    NamesAnotherList,
    /// The prefix names no corpus at all -- `t `, `>`, `?` (`empty_corpus`).
    NotASearch,
    /// Neither text nor a filter survived the parse, which is the board's
    /// question and not a query.
    NothingToSearchFor,
}

impl Refusal {
    /// The sentence the rail shows under the list's name.
    ///
    /// Begins with [`NEEDS_ATTENTION`] so that the words a reader sees and the
    /// words a test asserts on are the same string, produced once.
    ///
    /// **It ends by offering what actually clears the state, which is not a
    /// rename.** A rename changes the label; the query is what today's grammar
    /// refuses, and nothing edits a stored query -- see the module docs and
    /// `CONTEXT.md`'s **Smart list**, which is where the rule that no
    /// migration ever rewrites one lives. So the way out is to delete the row
    /// and save the search again, which is one search away in the box the
    /// reader is already looking at.
    #[must_use]
    pub fn description(&self) -> String {
        let reason = match self {
            Self::Rejected(message) => format!("the launcher refuses the saved query: {message}"),
            Self::TooManyValues { dimension, values } => format!(
                "the saved query narrows by {values} {dimension} and the launcher accepts \
                 {MAX_FILTER_VALUES}"
            ),
            Self::NamesAnotherList => "the saved query names another smart list".to_owned(),
            Self::NotASearch => {
                "the saved query starts with a prefix that is not a search".to_owned()
            }
            Self::NothingToSearchFor => {
                "the saved query has no words and no filters left in it".to_owned()
            }
        };
        format!("{NEEDS_ATTENTION}: {reason}. Delete it and save the search again.")
    }
}

/// A saved query the engine can answer, as the SQL layer wants it.
#[derive(Debug, Clone)]
pub struct Runnable {
    /// The search terms, or `None` for a query that is nothing but chips.
    pub text: Option<String>,
    pub filters: EffectiveFilters,
    /// The parse, echoed back so that opening a saved list draws the chips the
    /// saved query meant -- story 56's *"keeping its text, chips and
    /// prefixes"*.
    pub interpreted: crate::types::ParsedQuery,
}

/// Read a saved query the way a keystroke is read.
///
/// # Errors
///
/// Never fails: a query this cannot run comes back as a [`Refusal`], which is
/// what keeps one bad row from taking the launcher's board down with it.
pub fn plan(raw: &str, vocab: &Vocabulary) -> Result<Runnable, Refusal> {
    // The engine's own bounds, applied to the stored text before the parser
    // sees it -- and applied by *calling* `crate::validate` rather than by
    // restating what it checks, so a bound added there reaches a saved list on
    // the same commit. The limit it clamps is irrelevant here (nothing is
    // fetched) and its refusal is what this needs.
    crate::validate(crate::SearchQuery {
        raw: raw.to_owned(),
        limit: 1,
        filters: SearchFilters::default(),
    })
    .map_err(|error| {
        // The message and not the `Display` of the whole error: that prefixes
        // "invalid query:", and the sentence this ends up in already says the
        // launcher refused it.
        Refusal::Rejected(match error {
            SearchError::Invalid(message) => message,
            other => other.to_string(),
        })
    })?;

    let parsed = query::parse(raw, vocab);
    if parsed.list_id.is_some() {
        return Err(Refusal::NamesAnotherList);
    }
    // `crate::empty_corpus` and not a second copy of its list: that function is
    // where "this prefix names no corpus at all" is decided, and a prefix that
    // leaves it -- as `note:` did with #46 and `asset:` with #436 -- must stop
    // making saved lists need attention on the same commit.
    if crate::empty_corpus(parsed.query.prefix) {
        return Err(Refusal::NotASearch);
    }

    let filters = query::merge(&parsed, &SearchFilters::default());
    for (dimension, values) in [
        ("sources", filters.sources.len()),
        ("kinds", filters.kinds.len()),
        ("authors", filters.authors().len()),
    ] {
        if values > MAX_FILTER_VALUES {
            return Err(Refusal::TooManyValues { dimension, values });
        }
    }

    let text = (!parsed.query.text.is_empty()).then(|| parsed.query.text.clone());
    if text.is_none() && filters.is_empty() {
        return Err(Refusal::NothingToSearchFor);
    }

    Ok(Runnable {
        interpreted: crate::types::ParsedQuery {
            filters: filters.echo(),
            ..parsed.query
        },
        text,
        filters,
    })
}

/// Every saved list, in the order the rail draws them.
///
/// Creation order, and `id` breaks the tie a shared timestamp would leave: a
/// reader's lists stay where they were put, and a rail that re-ordered itself
/// between two openings would be one nobody could learn.
///
/// # Errors
///
/// [`SearchError::Db`] if the table cannot be read.
pub async fn all(pool: &PgPool) -> Result<Vec<SavedList>, SearchError> {
    Ok(sqlx::query_as::<_, SavedList>(
        "select id, label, query from knobas.smart_list order by created_at, id",
    )
    .fetch_all(pool)
    .await?)
}

/// One saved list, if there is one by that id.
///
/// # Errors
///
/// [`SearchError::Db`] if the table cannot be read.
pub async fn find(pool: &PgPool, id: &str) -> Result<Option<SavedList>, SearchError> {
    Ok(sqlx::query_as::<_, SavedList>(
        "select id, label, query from knobas.smart_list where id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

/// Save a launcher query as a list.
///
/// The id is **generated**, never supplied: it is what `list:<id>` names, it
/// shares one namespace with the built-ins, and a caller that could choose it
/// could shadow *My items*. [`slug`] derives it from the label and
/// [`free_id`] walks past whatever is taken.
///
/// # Errors
///
/// [`SearchError::Invalid`] for a blank label, for a query today's grammar
/// cannot run (with the [`Refusal`]'s own sentence, so the reader is told
/// *which* rule), and when [`MAX_SAVED_LISTS`] are already saved;
/// [`SearchError::Db`] if the row cannot be written.
pub async fn create(
    pool: &PgPool,
    vocab: &Vocabulary,
    label: &str,
    raw: &str,
) -> Result<SavedList, SearchError> {
    let label = label.trim();
    if label.is_empty() {
        return Err(SearchError::Invalid("a saved list needs a name".to_owned()));
    }
    // Trimmed here and not at the caller, so it is true of every caller: the
    // stored text is the list's own blurb on the rail, and trailing space in
    // it is invisible there and carried into every count the list is ever
    // asked for.
    let raw = raw.trim();
    // Refused here rather than stored and shown as *needs attention*: the box
    // has just answered this query, so a query that cannot run is a caller
    // bug, and a row nothing could have produced is a state no reader should
    // ever meet.
    if let Err(refusal) = plan(raw, vocab) {
        return Err(SearchError::Invalid(refusal.description()));
    }

    let taken: i64 = sqlx::query_scalar("select count(*) from knobas.smart_list")
        .fetch_one(pool)
        .await?;
    if taken >= MAX_SAVED_LISTS {
        return Err(SearchError::Invalid(format!(
            "there are already {MAX_SAVED_LISTS} saved lists; delete one first"
        )));
    }

    let id = free_id(pool, &slug(label)).await?;
    sqlx::query("insert into knobas.smart_list (id, label, query) values ($1, $2, $3)")
        .bind(&id)
        .bind(label)
        .bind(raw)
        .execute(pool)
        .await?;
    Ok(SavedList {
        id,
        label: label.to_owned(),
        query: raw.to_owned(),
    })
}

/// Give a saved list a different name.
///
/// **The id does not move.** It is what `list:<id>` names and what the
/// seen-stamp behind the change badge is keyed on, so re-deriving it from the
/// new label would silently un-read the list and break any `list:` the reader
/// had learned to type.
///
/// # Errors
///
/// [`SearchError::Invalid`] for a blank name, [`SearchError::UnknownList`] if
/// nobody saved a list by that id, [`SearchError::Db`] if the write fails.
pub async fn rename(pool: &PgPool, id: &str, label: &str) -> Result<SavedList, SearchError> {
    let label = label.trim();
    if label.is_empty() {
        return Err(SearchError::Invalid("a saved list needs a name".to_owned()));
    }
    sqlx::query_as::<_, SavedList>(
        "update knobas.smart_list
            set label = $2, updated_at = now()
          where id = $1
      returning id, label, query",
    )
    .bind(id)
    .bind(label)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| SearchError::UnknownList(id.to_owned()))
}

/// Forget a saved list.
///
/// # Errors
///
/// [`SearchError::UnknownList`] if nobody saved a list by that id,
/// [`SearchError::Db`] if the delete fails.
pub async fn delete(pool: &PgPool, id: &str) -> Result<(), SearchError> {
    let deleted = sqlx::query("delete from knobas.smart_list where id = $1")
        .bind(id)
        .execute(pool)
        .await?
        .rows_affected();
    if deleted == 0 {
        return Err(SearchError::UnknownList(id.to_owned()));
    }
    Ok(())
}

/// One row of `sql::saved_summary_sql`.
#[derive(sqlx::FromRow)]
struct SavedCount {
    list_id: String,
    total: i64,
    /// `null` for a list with nothing in it, which is exactly the state
    /// [`lists::changed`] reads as *no badge*.
    newest: Option<DateTime<Utc>>,
}

/// Every saved list's summary, in the shape the rail draws built-ins in.
///
/// **One round trip to read the table, and two more only if it had anything in
/// it**: the counts -- one statement for every runnable list however many
/// there are, `union all`-ed by [`sql::saved_summary_sql`] -- and the
/// seen-stamps. So a launcher with nothing saved costs the board exactly one
/// small select, and a list whose query is refused costs it nothing beyond
/// that: it is not in the counting statement at all, which is what makes one
/// broken row cheap rather than fatal.
///
/// # Errors
///
/// [`SearchError::Db`] if the table, the counts or the stamps cannot be read.
pub async fn summaries(
    pool: &PgPool,
    vocab: &Vocabulary,
) -> Result<Vec<SmartListSummary>, SearchError> {
    let rows = all(pool).await?;
    if rows.is_empty() {
        return Ok(Vec::new());
    }

    let planned: Vec<(SavedList, Result<Runnable, Refusal>)> = rows
        .into_iter()
        .map(|row| {
            let plan = plan(&row.query, vocab);
            (row, plan)
        })
        .collect();

    let queries: Vec<SavedQuery<'_>> = planned
        .iter()
        .filter_map(|(row, plan)| {
            plan.as_ref().ok().map(|runnable| SavedQuery {
                id: &row.id,
                text: runnable.text.as_deref(),
                filters: &runnable.filters,
            })
        })
        .collect();

    let mut counted: HashMap<String, SavedCount> = HashMap::new();
    if let Some(built) = sql::saved_summary_sql(corpus::ALL, &queries) {
        for row in sql::query_as_with::<SavedCount>(built)
            .fetch_all(pool)
            .await?
        {
            counted.insert(row.list_id.clone(), row);
        }
    }
    let seen = lists::seen_stamps(pool).await?;

    Ok(planned
        .into_iter()
        .map(|(row, plan)| match plan {
            // The query itself is the description: a saved list's blurb is
            // what it searches for, and a reader who has forgotten what
            // `#@me updated:7d` meant is one glance from the answer.
            Ok(_) => {
                let count = counted.get(&row.id);
                SmartListSummary {
                    count: count.map_or(0, |row| row.total),
                    changed: lists::changed(
                        count.and_then(|row| row.newest),
                        seen.get(&row.id).map(String::as_str),
                    ),
                    description: row.query,
                    id: row.id,
                    label: row.label,
                    saved: true,
                    needs_attention: false,
                }
            }
            Err(refusal) => SmartListSummary {
                id: row.id,
                label: row.label,
                // Nothing was asked, so there is nothing to count and nothing
                // to badge. A `0` here means *not measured*, and the
                // description is what says so.
                count: 0,
                changed: false,
                description: refusal.description(),
                saved: true,
                needs_attention: true,
            },
        })
        .collect())
}

/// The id a label becomes: lower case, one hyphen between runs of what
/// `list:` can carry, and nothing on either end.
///
/// The character class is migration `0025`'s own check constraint, which is
/// what stops a row the launcher cannot address from reaching the table by
/// another route.
fn slug(label: &str) -> String {
    let mut out = String::new();
    for character in label.chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
        if out.trim_end_matches('-').chars().count() >= MAX_ID_CHARS {
            break;
        }
    }
    let slug = out.trim_matches('-').to_owned();
    // A label of nothing but punctuation or non-Latin script leaves no slug at
    // all, and an id is still owed: `free_id` turns this into `list-2` and so
    // on, which is addressable even though it says nothing.
    if slug.is_empty() {
        "list".to_owned()
    } else {
        slug
    }
}

/// The first id in the `<slug>`, `<slug>-2`, `<slug>-3` sequence that nobody
/// holds.
///
/// **The built-ins are checked as well as the table**, because `list:<id>` has
/// one meaning: a saved list called *My items* must not shadow the built-in
/// that answers `list:mine`, and the id is the only thing that could.
async fn free_id(pool: &PgPool, slug: &str) -> Result<String, SearchError> {
    let taken: Vec<String> =
        sqlx::query_scalar("select id from knobas.smart_list where id = $1 or id like $2")
            .bind(slug)
            .bind(format!("{slug}-%"))
            .fetch_all(pool)
            .await?;
    let mut candidate = slug.to_owned();
    let mut suffix = 1_u32;
    while lists::find(&candidate).is_some() || taken.iter().any(|id| id == &candidate) {
        suffix += 1;
        candidate = format!("{slug}-{suffix}");
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ids this makes are ids `list:` can name and migration `0025` will
    /// accept -- the constraint is `^[a-z0-9]+(-[a-z0-9]+)*$`.
    #[test]
    fn a_slug_is_addressable_whatever_the_label_was() {
        for (label, expected) in [
            ("My items", "my-items"),
            ("  Payments   retries  ", "payments-retries"),
            ("#@me updated:7d", "me-updated-7d"),
            ("SEPA/retry — backoff", "sepa-retry-backoff"),
            ("...", "list"),
            ("\u{4e2d}\u{6587}", "list"),
        ] {
            let slug = slug(label);
            assert_eq!(slug, expected, "{label:?}");
        }
        // Every one of them against migration `0025`'s own character class --
        // `^[a-z0-9]+(-[a-z0-9]+)*$`, read out here as the four things it
        // says, because the constraint is what an id has to survive and a
        // hand-written expectation of what this function does is not.
        for label in [
            "My items",
            "#@me updated:7d",
            "...",
            "a",
            "----",
            "9",
            "\u{4e2d}\u{6587}",
            &"long ".repeat(80),
        ] {
            let slug = slug(label);
            assert!(!slug.is_empty(), "{label:?} left no id at all");
            assert!(
                slug.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{label:?} -> {slug:?}"
            );
            assert!(
                !slug.starts_with('-') && !slug.ends_with('-'),
                "{label:?} -> {slug:?}"
            );
            assert!(!slug.contains("--"), "{label:?} -> {slug:?}");
            assert!(slug.chars().count() <= MAX_ID_CHARS, "{slug:?}");
        }
    }

    /// Every refusal says which rule it is, and says it under one heading.
    ///
    /// The heading is the wording the rail draws and the IPC-seam test asserts
    /// on; the rest is the part that has to differ, or a reader is told
    /// something is wrong and never what.
    #[test]
    fn every_refusal_names_its_own_rule_under_one_heading() {
        let refusals = [
            Refusal::Rejected("query is 900 characters".to_owned()),
            Refusal::TooManyValues {
                dimension: "sources",
                values: 40,
            },
            Refusal::NamesAnotherList,
            Refusal::NotASearch,
            Refusal::NothingToSearchFor,
        ];
        let mut seen: Vec<String> = Vec::new();
        for refusal in &refusals {
            let description = refusal.description();
            assert!(
                description.starts_with(NEEDS_ATTENTION),
                "{refusal:?} -> {description}"
            );
            assert!(
                !seen.contains(&description),
                "two refusals say the same thing: {description}"
            );
            seen.push(description);
        }
        assert_eq!(seen.len(), refusals.len());
    }
}
