//! Where a source keeps the things knobas reads, said by the adapter once
//! (ADR-0007's destination; issue #277).
//!
//! # The problem this ends
//!
//! Contract §4.1 normalizes four fields -- `title`, `body_text`, `author`,
//! `updated_at` -- so a status, a priority, an assignee, a requested reviewer,
//! a merged flag and a project live only in the verbatim `payload`, in the
//! source's own shape. Every reader outside an adapter therefore had to hold a
//! little table of per-source spellings: `coalesce(fields.status.name,
//! status)` in the mini board, three arms for a project key in the census, two
//! assignee spellings in the inbox. ADR-0007 ratified that as an *interim*
//! discipline -- miss, one named statement, a pinned failure direction -- and
//! recorded the destination: **the adapter declares the paths, and each read
//! expires into the declaration.** This module is that destination.
//!
//! A declaration is data: a list of object keys to walk
//! ([`PayloadPath`]), per entity kind ([`KindPaths`]), carried on
//! `SourceDescriptor::payload_paths`. knobas resolves it and never guesses;
//! a kind that declares no path for a field simply has no value for it, which
//! is `CONTEXT.md`'s **miss**.
//!
//! # One rule, two implementations, held together by a test
//!
//! These reads are SQL -- the project census is a pass over the whole live
//! corpus and the mini board narrows inside its statement -- so a declaration
//! is bound as **one jsonb parameter** and resolved by
//! [`declared_string!`](crate::declared_string), [`declared_flag!`] and
//! [`declared_list!`] rather than by pulling every payload into Rust. The Rust
//! resolvers below ([`resolve_string`], [`resolve_flag`], [`resolve_list`]) are
//! the same rule for callers that already hold the payload -- the contract
//! battery is the one that matters, since it checks an adapter's declarations
//! against its own corpus with no database in sight.
//!
//! Two implementations of one rule is a drift risk, and it is pinned rather
//! than hoped: `crates/knobas-core/tests/it/payload_paths.rs` runs the same
//! payload/declaration pairs through both and asserts they agree, so a change
//! to either that the other does not make turns a test red.
//!
//! # What a declaration may say, and what it may not
//!
//! Every scalar field is a **list of candidate paths, most specific first**,
//! and the first candidate that lands on a value of the right type wins --
//! which is exactly the `coalesce` the readers used to spell, moved to the one
//! place that knows the answer. The candidates are *one adapter's* alternative
//! spellings of its own field (Jira Data Center names an assignee at
//! `fields.assignee.name` on one instance and `fields.assignee.key` on
//! another), never knobas' guesses about a source it has not met.
//!
//! A value that is not a string where a string is declared, a blank or
//! whitespace-only string, an absent key: all three are a miss, and the next
//! candidate is tried. These are the three refusals ADR-0007's interim
//! `string_at!` made and this module inherited whole when its last call site
//! expired (#277) -- `->>` yields an object's *text form* rather than nothing,
//! so without the type check a source that spells a field some other way
//! arrives as a value like `{"id":3}`, a guess dressed as an observation.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Where one field lives in a payload: the object keys to walk, in order.
///
/// `["fields", "status", "name"]` is Jira Data Center's status. An empty path
/// addresses the payload itself, which is an object and therefore never a
/// value any of the resolvers below accept -- so an adapter cannot declare
/// "the whole record" as its status.
///
/// **Keys, not a pointer string or a JSONPath expression.** A segment list has
/// no escaping rules to get wrong (a Jira custom field is `customfield_10008`,
/// but a source is free to name a key `a/b`), no syntax that can fail to
/// parse at run time, and it is what PostgreSQL's `#>` operator takes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PayloadPath(pub Vec<String>);

impl PayloadPath {
    /// A path from its segments: `PayloadPath::of(["fields", "status", "name"])`.
    pub fn of<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        PayloadPath(segments.into_iter().map(Into::into).collect())
    }

    /// The segments, for a caller walking them itself.
    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.0
    }
}

/// Where a **list** of strings lives: the array, and where the string sits
/// inside each element.
///
/// Gitea's requested reviewers are `requested_reviewers`, each element an
/// object whose `login` is the account -- so `at` is `["requested_reviewers"]`
/// and `entry` is `["login"]`. An empty `entry` says the elements are the
/// strings themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPath {
    /// Where the array is.
    pub at: PayloadPath,
    /// Where the string is inside each element; empty means the element itself.
    #[serde(default)]
    pub entry: PayloadPath,
}

/// What one adapter's records of one entity kind carry, and where.
///
/// **Per kind**, because the answer is: a TeamCity *build* names its project
/// one level down on the `buildType` it ran, while a build *configuration*'s
/// record **is** that `buildType` object and names it at the top level. Before
/// this that difference was a `case when i.kind = 'build_config'` guard in
/// knobas' own SQL (#232); now it is two entries in the adapter's own
/// declaration, which is where the knowledge belongs.
///
/// Every field defaults to empty and empty means **the adapter says nothing
/// about it**, which is a miss and never a guess. `#[serde(default)]`
/// throughout, so a descriptor serialized by an older peer -- an out-of-process
/// adapter built before this grew -- still decodes, as an adapter that declares
/// nothing.
///
/// The field *names* are load-bearing beyond Rust: they are the keys the SQL
/// macros look the declaration up by, as string literals. `the_field_names_the_sql_macros_use_are_the_serialized_ones`
/// pins the two together, so a rename here turns a test red instead of turning
/// every path-driven read into a silent miss.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindPaths {
    /// Which entity kind this describes -- one of the descriptor's
    /// `entity_kinds`, which the contract battery enforces.
    pub kind: String,
    /// The status a ticket stands in, in the source's own spelling
    /// (`fields.status.name`). What the mini board's columns are.
    #[serde(default)]
    pub status_name: Vec<PayloadPath>,
    /// The priority a card shows (`fields.priority.name`).
    #[serde(default)]
    pub priority: Vec<PayloadPath>,
    /// The account an item is assigned to, spelled the way the source spells
    /// an account elsewhere -- the inbox matches it against the configured
    /// usernames, so a display name or an internal id declared here would be a
    /// rule that never fires.
    #[serde(default)]
    pub assignee: Vec<PayloadPath>,
    /// The accounts a review has been requested from.
    #[serde(default)]
    pub reviewers: Vec<ListPath>,
    /// Whether a pull request has been merged. A **boolean** in the payload:
    /// a timestamp that is present when merged is a different fact with a
    /// different absence, and an adapter that has one declares no merged flag
    /// rather than pointing this at it.
    #[serde(default)]
    pub merged: Vec<PayloadPath>,
    /// The key of the project this item belongs to (ADR-0010), which is what a
    /// project room is scoped by.
    #[serde(default)]
    pub project_key: Vec<PayloadPath>,
    /// The project's display name, for the room's label.
    #[serde(default)]
    pub project_name: Vec<PayloadPath>,
    /// The status names this source considers **blocked-like**, in its own
    /// spelling (`Blocked`, `Waiting for support`). Names, not a path: which
    /// statuses mean "stuck" is a property of the source's workflow, not of
    /// one record. Matched case-insensitively by its reader.
    #[serde(default)]
    pub blocked_statuses: Vec<String>,
}

/// The [`KindPaths`] field names, as the SQL macros spell them.
///
/// Named constants for the Rust side; the macros need literals, and
/// `the_field_names_the_sql_macros_use_are_the_serialized_ones` is what holds
/// the literals to these.
pub mod field {
    pub const STATUS_NAME: &str = "status_name";
    pub const PRIORITY: &str = "priority";
    pub const ASSIGNEE: &str = "assignee";
    pub const REVIEWERS: &str = "reviewers";
    pub const MERGED: &str = "merged";
    pub const PROJECT_KEY: &str = "project_key";
    pub const PROJECT_NAME: &str = "project_name";

    /// Every field that is a path, in declaration order. `blocked_statuses` is
    /// not here: it is a set of names, not somewhere to look.
    pub const ALL: &[&str] = &[
        STATUS_NAME,
        PRIORITY,
        ASSIGNEE,
        REVIEWERS,
        MERGED,
        PROJECT_KEY,
        PROJECT_NAME,
    ];
}

/// What every configured source declares, ready to be handed to a read.
///
/// Assembled where the descriptors are -- `knobas_app::sources::declared_paths`
/// -- and passed **into** the reads, rather than persisted beside the mirror.
/// The declaration is a property of the running binary's adapter, so a copy in
/// a table would be a second answer that goes stale the moment an adapter
/// learns a new spelling, and it would cost a migration on a frozen surface for
/// nothing. `knobas-core` still knows no adapter: it is handed the answer and
/// does not go looking for it.
///
/// Keyed by **source id and kind**, in that order, which is also the shape the
/// SQL macros index: `$n #> array[i.source_id, i.kind, '<field>']`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Declarations(BTreeMap<String, BTreeMap<String, KindPaths>>);

impl Declarations {
    /// No source declares anything: every path-driven read misses.
    ///
    /// The honest value for a caller that has no registry to ask -- a test
    /// fixture, a scratch database -- and the reason it is spelled rather than
    /// defaulted into existence: a read that quietly declared nothing would
    /// look like a bug in the payload rather than in the wiring.
    #[must_use]
    pub fn empty() -> Self {
        Declarations(BTreeMap::new())
    }

    /// The declarations of one source, added under its **instance id** --
    /// `jira-eu`, not `jira`. That id is the entity namespace every item of
    /// that source carries, which is what the reads join on.
    #[must_use]
    pub fn with(mut self, source_id: impl Into<String>, kinds: Vec<KindPaths>) -> Self {
        self.0.insert(
            source_id.into(),
            kinds.into_iter().map(|k| (k.kind.clone(), k)).collect(),
        );
        self
    }

    /// What this source declares for this kind, if anything.
    #[must_use]
    pub fn get(&self, source_id: &str, kind: &str) -> Option<&KindPaths> {
        self.0.get(source_id)?.get(kind)
    }

    /// The jsonb every path-driven statement binds.
    ///
    /// One parameter for the whole map rather than one per field: a read
    /// narrows over many sources at once (the project census is a pass over the
    /// entire live corpus), so the statement has to be able to look the
    /// declaration up per row.
    ///
    /// Wrapped here rather than at each `.bind` because there are eight of
    /// them across two crates, and a `bind` that forgot the `Json` wrapper
    /// would bind a JSON *string* -- which `#>` reads as no declaration at
    /// all, so every path-driven read would quietly miss.
    #[must_use]
    pub fn as_param(&self) -> sqlx::types::Json<serde_json::Value> {
        sqlx::types::Json(self.as_json())
    }

    /// The same declaration as plain JSON, for a caller that is not binding it
    /// to a statement.
    #[must_use]
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
    }
}

/// The value one path leads to, if it leads to one.
///
/// A miss is a miss: an absent key, a null, a container the source did not
/// write, a scalar where the path expected an object. Which of those it was is
/// not a distinction any reader may act on -- that is the whole of ADR-0007's
/// requirement 1 -- so this answers with the value or with nothing.
///
/// The **contract battery** asks one thing more, and asks it of the corpus
/// rather than of a record: where nothing of a kind resolved a declared path,
/// no record of that kind may show the path naming a key it does not have. See
/// [`names_a_missing_key`], which is how it asks.
#[must_use]
pub fn at<'a>(payload: &'a serde_json::Value, path: &PayloadPath) -> Option<&'a serde_json::Value> {
    let mut at = payload;
    for segment in path.segments() {
        at = at.as_object()?.get(segment)?;
        if at.is_null() {
            return None;
        }
    }
    Some(at)
}

/// Whether this path names a key that an object the source really wrote does
/// not have.
///
/// The contract battery's question, and the reason it is asked this way. A
/// declared path that resolves nowhere is either a source that carries no such
/// value -- an unassigned issue, a pull request nobody was asked to review --
/// or an adapter naming a key its own records lack, and a reader cannot tell
/// those apart afterwards. This is what tells them apart:
///
/// * the walk stops at a **null**, or at something that is not an object: the
///   source wrote the container and said there is nothing in it, or wrote a
///   different shape entirely. Not the declaration's fault. `false`.
/// * the walk reaches an object the source really wrote and the next key is
///   **not in it**: the declaration names a key these records do not have.
///   `true` -- and that is as true of `fieldz.status.name`, which stops at the
///   payload root, as of `fields.status.nam`, which stops one level deeper.
///
/// One record cannot be judged on this alone: a key absent from every record
/// of a corpus is also what a field the source omits when empty looks like.
/// The battery therefore asks it of the **corpus**, and only where nothing
/// resolved -- so one item carrying the value settles the question for the
/// whole kind.
///
/// `false` for the empty path, which addresses the payload itself and names no
/// key.
#[must_use]
pub fn names_a_missing_key(payload: &serde_json::Value, path: &PayloadPath) -> bool {
    let mut at = payload;
    for segment in path.segments() {
        let Some(fields) = at.as_object() else {
            // A null, or a scalar where this expected a container: the source
            // is saying there is nothing down here.
            return false;
        };
        match fields.get(segment) {
            Some(next) => at = next,
            None => return true,
        }
    }
    false
}

/// The first candidate that lands on a usable string.
///
/// Usable is the same triple refusal [`declared_string!`](crate::declared_string)
/// makes in SQL: it must be a JSON **string**, and trimming it must leave
/// something. A candidate that misses is passed over for the next one; all of
/// them missing is a miss.
#[must_use]
pub fn resolve_string(payload: &serde_json::Value, candidates: &[PayloadPath]) -> Option<String> {
    candidates.iter().find_map(|path| {
        let text = at(payload, path)?.as_str()?.trim();
        (!text.is_empty()).then(|| text.to_owned())
    })
}

/// The first candidate that lands on a JSON **boolean**.
///
/// A flag is a boolean or it is nothing: `"merged": "true"` is a source
/// spelling a flag as a word, and reading it would be knobas deciding what a
/// word means. An adapter whose records carry the fact some other way -- a
/// nullable merge timestamp, a state word -- declares no flag, and the reader
/// misses, which is what it did before this existed.
#[must_use]
pub fn resolve_flag(payload: &serde_json::Value, candidates: &[PayloadPath]) -> Option<bool> {
    candidates
        .iter()
        .find_map(|path| at(payload, path)?.as_bool())
}

/// Every usable string every candidate list yields, in declaration order.
///
/// A **union** rather than "the first list that resolves", which is where this
/// deliberately differs from the scalars: two candidates are two places the
/// same kind of thing is recorded, and a reviewer requested in one of them is
/// requested. Elements that are not strings, and strings that are blank, are
/// dropped -- the same refusal, applied per element, so one odd entry costs
/// that entry and not the list.
///
/// **Declaration order is this function's, not the rule's.**
/// [`declared_list!`](crate::declared_list) is a table expression whose rows
/// come out in the join's order, so the two implementations agree on the
/// *values* and not on their sequence -- which is why the agreement test sorts
/// both sides, and why the one reader of it (`inbox`'s review-request rule)
/// compares with `= any(...)` rather than by position. A caller that needs an
/// order must impose one.
#[must_use]
pub fn resolve_list(payload: &serde_json::Value, candidates: &[ListPath]) -> Vec<String> {
    let mut out = Vec::new();
    for candidate in candidates {
        let Some(elements) = at(payload, &candidate.at).and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for element in elements {
            let Some(text) = at(element, &candidate.entry).and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            if !text.trim().is_empty() {
                out.push(text.trim().to_owned());
            }
        }
    }
    out
}

/// SQL for "the string the declared path leads to, or nothing" -- the
/// path-driven successor to ADR-0007's `string_at!`, which read a *literal*
/// path and expanded nowhere once these reads landed.
///
/// `$decl` is the placeholder the [`Declarations`] jsonb is bound at
/// (`"$4"`), `$field` the [`KindPaths`] field name as a literal
/// (`"status_name"`). Both are literals because that is what `concat!` folds:
/// every statement built with this is a `&'static str`, so nothing here can
/// concatenate a value into SQL, and the declaration reaches the database as a
/// **parameter** like every other value.
///
/// The row's alias must be `i`, and must carry `payload`, `source_id` and
/// `kind` -- the same requirement `project_key_read!` has always had, and true
/// of every statement here: they all select from `sync.item` or
/// `sync.live_item` aliased `i`.
///
/// Reads as: of the candidate paths this source declares for this kind, in
/// declaration order, take the first whose value is a non-blank string.
#[macro_export]
macro_rules! declared_string {
    ($decl:literal, $field:literal) => {
        concat!(
            "(select nullif(btrim(i.payload #>> c.segs), '') from ",
            $crate::declared_candidates!($decl, $field),
            " c where jsonb_typeof(i.payload #> c.segs) = 'string' \
              and nullif(btrim(i.payload #>> c.segs), '') is not null \
              order by c.ord limit 1)"
        )
    };
}

/// What separates two segments of an [`ancestor_path_read!`] path.
///
/// A single-glyph guillemet with a space either side.
///
/// The separator is chosen **once, in SQL**: the launcher row and the detail
/// panel each render whatever string [`ancestor_path_read!`] joined, so
/// neither of them holds a copy of it and neither can disagree about it. What
/// this constant is for is that the macro cannot use it -- `concat!` folds
/// literals and not `const` items, so the glyph has to be written a second
/// time inside the statement. `the_ancestor_path_joins_on_the_one_separator`
/// is what keeps that second spelling honest: it asserts the SQL joins on
/// exactly this value, so the two cannot drift apart unnoticed.
pub const ANCESTOR_SEPARATOR: &str = " \u{203a} ";

/// SQL for "where this record sits inside its source, as one line" -- the
/// titles of an item's `ancestors`, outermost first, joined by
/// [`ANCESTOR_SEPARATOR`].
///
/// # Why this is not a declared read
///
/// #277 moved every payload read a `KindPaths` field can express onto the
/// adapter's own declaration, and this is the one that cannot be: an ancestor
/// path is a **list of strings joined in order**, and `KindPaths` has no slot
/// shaped like that -- `reviewers` is the only list it carries and it is an
/// unordered set of accounts, not a path. Adding a slot is a `knobas-source`
/// change and therefore a §10.8 conversation of its own. So this stays under
/// ADR-0007's *interim* discipline, and meets all three of its requirements:
///
/// 1. **It misses, never guesses.** A payload with no `ancestors`, an
///    `ancestors` that is not an array, elements that are not objects,
///    elements whose `title` is absent, not a string, or blank -- every one of
///    them contributes nothing, and a record with no usable segment at all
///    yields `null` rather than an empty string. `string_agg` over no rows is
///    `null`, which is what makes that true by construction rather than by a
///    guard someone has to remember.
/// 2. **One named statement**, this one, expanded by the two reads that draw a
///    row (`knobas_search`'s launcher corpora and `knobas_app`'s room and
///    entity statements) and nowhere else.
/// 3. **The failure direction is absence**, pinned by
///    `knobas-core/tests/it/ancestor_path.rs` across all six unusable shapes.
///
/// `$payload` is the payload expression, as a literal (`"i.payload"`), because
/// that is what `concat!` folds: every statement built with this is a
/// `&'static str`, so nothing here can concatenate a value into SQL.
///
/// The `jsonb_typeof(... ) = 'array'` guard is not decoration:
/// `jsonb_array_elements` **raises** on a non-array, so without it a single
/// Jira ticket whose payload happened to carry an `ancestors` object would
/// abort the whole launcher query rather than miss.
///
/// # Do not "simplify" the `order by a.ordinality` away
///
/// A mutation check found that removing it changes no observable behaviour:
/// PostgreSQL happens to aggregate in scan order, and for
/// `jsonb_array_elements ... with ordinality` that is array order, so every
/// test here stays green without it. That is a true result with a
/// precondition attached, and the precondition is not a guarantee:
/// `string_agg` **without** an `ORDER BY` has no defined order at all, and
/// the plan that produces scan order today is free to change under a parallel
/// or reordered scan tomorrow.
///
/// Order is the entire meaning of a path -- two ancestors joined the other way
/// round name a different place -- and one half of that is now witnessed.
/// Since #396 nested one seeded Confluence page two deep, that page's path has
/// two segments, so the live test
/// `the_launcher_finds_the_seeded_page_with_its_ancestor_path` in
/// `crates/knobas-app/tests/atlassian_live.rs` reads both ancestors off the
/// real server and asserts the outermost-first join: turning the clause into
/// `order by a.ordinality desc` fails it (measured in #399, one failure in an
/// otherwise green `atlassian_live` binary). That witness is contingent
/// rather than structural: it holds because the two seeded ancestors carry
/// different titles, and the test guards only that they are different *pages*
/// (`assert_ne!` on their ids).
///
/// *Removing* the clause is still invisible, and removal is the simplification
/// the heading warns against: a path that comes out in order because the
/// planner scanned in order reads exactly like one the statement ordered. So
/// the clause stays on the argument above rather than on a test that can see
/// it go. The same treatment `crates/knobas-source-jira/src/time.rs` gives
/// `jql_floor`, and for the same reason: "unobservable today" is a fact about
/// this planner, not about this statement.
#[macro_export]
macro_rules! ancestor_path_read {
    ($payload:literal) => {
        concat!(
            "(case when jsonb_typeof(",
            $payload,
            "->'ancestors') = 'array' then (select string_agg(btrim(a.value->>'title'), ",
            "' \u{203a} ' order by a.ordinality) from jsonb_array_elements(",
            $payload,
            "->'ancestors') with ordinality a where jsonb_typeof(a.value) = 'object' \
              and jsonb_typeof(a.value->'title') = 'string' \
              and btrim(a.value->>'title') <> '') end)"
        )
    };
}

/// SQL for "the boolean the declared path leads to, or nothing" -- see
/// [`declared_string!`] for the arguments and [`resolve_flag`] for why only a
/// JSON boolean counts.
#[macro_export]
macro_rules! declared_flag {
    ($decl:literal, $field:literal) => {
        concat!(
            "(select (i.payload #>> c.segs) = 'true' from ",
            $crate::declared_candidates!($decl, $field),
            " c where jsonb_typeof(i.payload #> c.segs) = 'boolean' order by c.ord limit 1)"
        )
    };
}

/// SQL for "every usable string the declared list paths yield", as a table
/// expression of one column, `value`.
///
/// Written to be joined rather than selected -- `cross join lateral
/// (declared_list!(…)) as r` -- so a rule that wants one row per value keeps
/// the shape it had when it walked `jsonb_array_elements` itself.
///
/// `jsonb_array_elements` **raises** on a scalar or an object rather than
/// returning no rows, and a payload is a verbatim source record, so both walks
/// below are guarded by a `jsonb_typeof` check: the day a source spells its
/// reviewers as an object, this yields nothing for that record instead of
/// failing the whole read for every source.
#[macro_export]
macro_rules! declared_list {
    ($decl:literal, $field:literal) => {
        concat!(
            "(select nullif(btrim(e.elem #>> c.entry_segs), '') as value from (select ",
            $crate::declared_segments!("cand->'at'"),
            " as at_segs, ",
            $crate::declared_segments!("cand->'entry'"),
            " as entry_segs from jsonb_array_elements(",
            $crate::declared_array!($decl, $field),
            ") as cands(cand)) c cross join lateral jsonb_array_elements(\
             case when jsonb_typeof(i.payload #> c.at_segs) = 'array' \
                  then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) \
             where jsonb_typeof(e.elem #> c.entry_segs) = 'string' \
               and nullif(btrim(e.elem #>> c.entry_segs), '') is not null)"
        )
    };
}

/// The declared candidates for one field as a numbered relation of `text[]`
/// paths: `(segs, ord)`, in declaration order.
///
/// An implementation detail of the three macros above, exported only because
/// they are.
#[doc(hidden)]
#[macro_export]
macro_rules! declared_candidates {
    ($decl:literal, $field:literal) => {
        concat!(
            "(select ",
            $crate::declared_segments!("p"),
            " as segs, ord from jsonb_array_elements(",
            $crate::declared_array!($decl, $field),
            ") with ordinality as t(p, ord))"
        )
    };
}

/// One declared field's candidate array, or an empty one where the source, the
/// kind or the field is not declared. `jsonb_array_elements` raises on a
/// non-array, so this guard is what makes an absent declaration a miss rather
/// than a failed query.
#[doc(hidden)]
#[macro_export]
macro_rules! declared_array {
    ($decl:literal, $field:literal) => {
        concat!(
            "case when jsonb_typeof(",
            $decl,
            "::jsonb #> array[i.source_id, i.kind, '",
            $field,
            "']) = 'array' then ",
            $decl,
            "::jsonb #> array[i.source_id, i.kind, '",
            $field,
            "'] else '[]'::jsonb end"
        )
    };
}

/// One candidate path as a `text[]`, or the empty path where it is not an
/// array of strings. The empty path addresses the payload itself, which is an
/// object and so satisfies none of the type checks above -- a malformed
/// declaration misses.
#[doc(hidden)]
#[macro_export]
macro_rules! declared_segments {
    ($candidate:literal) => {
        concat!(
            "array(select jsonb_array_elements_text(case when jsonb_typeof(",
            $candidate,
            ") = 'array' then ",
            $candidate,
            " else '[]'::jsonb end))"
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jira_issue() -> serde_json::Value {
        serde_json::json!({
            "fields": {
                "status": { "name": "In Progress" },
                "priority": serde_json::Value::Null,
                "assignee": { "key": "mara.lindqvist" },
                "project": { "key": "PAY", "name": "Payments Platform" }
            }
        })
    }

    /// The SQL macros address a field by a string literal. This is what keeps
    /// those literals and the struct's serialized keys one set: a rename that
    /// the macro call sites do not follow would otherwise turn every
    /// path-driven read into a permanent miss, silently.
    #[test]
    fn the_field_names_the_sql_macros_use_are_the_serialized_ones() {
        let wire = serde_json::to_value(KindPaths::default()).unwrap();
        let mut keys: Vec<&str> = wire
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut expected: Vec<&str> = field::ALL.to_vec();
        expected.push("kind");
        expected.push("blocked_statuses");
        expected.sort_unstable();
        assert_eq!(keys, expected);
    }

    #[test]
    fn the_first_candidate_that_lands_on_a_string_wins() {
        let paths = vec![
            PayloadPath::of(["fields", "status", "name"]),
            PayloadPath::of(["status"]),
        ];
        assert_eq!(
            resolve_string(&jira_issue(), &paths).as_deref(),
            Some("In Progress")
        );
        let flat = serde_json::json!({ "status": "To Do" });
        assert_eq!(resolve_string(&flat, &paths).as_deref(), Some("To Do"));
    }

    /// The three refusals a usable string has to survive, one candidate at a
    /// time.
    #[test]
    fn a_value_that_is_not_a_usable_string_is_passed_over() {
        let payload = serde_json::json!({
            "a": { "id": 3 }, "b": "   ", "c": ["x"], "d": "kept"
        });
        let candidates = vec![
            PayloadPath::of(["a"]),
            PayloadPath::of(["b"]),
            PayloadPath::of(["c"]),
            PayloadPath::of(["d"]),
        ];
        assert_eq!(
            resolve_string(&payload, &candidates).as_deref(),
            Some("kept")
        );
    }

    #[test]
    fn a_kind_that_declares_nothing_misses() {
        assert_eq!(resolve_string(&jira_issue(), &[]), None);
        assert_eq!(resolve_flag(&jira_issue(), &[]), None);
        assert!(resolve_list(&jira_issue(), &[]).is_empty());
    }

    #[test]
    fn a_flag_is_a_boolean_or_it_is_nothing() {
        let merged = vec![PayloadPath::of(["merged"])];
        assert_eq!(
            resolve_flag(&serde_json::json!({ "merged": true }), &merged),
            Some(true)
        );
        assert_eq!(
            resolve_flag(&serde_json::json!({ "merged": false }), &merged),
            Some(false)
        );
        assert_eq!(
            resolve_flag(&serde_json::json!({ "merged": "true" }), &merged),
            None,
            "a word is not a flag"
        );
        assert_eq!(
            resolve_flag(
                &serde_json::json!({ "merged": "2026-09-01T00:00:00Z" }),
                &merged
            ),
            None
        );
    }

    #[test]
    fn a_list_keeps_the_usable_strings_and_drops_the_rest() {
        let payload = serde_json::json!({
            "requested_reviewers": [
                { "login": "mara" }, { "login": "  " }, { "id": 4 }, { "login": "tom" }
            ]
        });
        let candidates = vec![ListPath {
            at: PayloadPath::of(["requested_reviewers"]),
            entry: PayloadPath::of(["login"]),
        }];
        assert_eq!(resolve_list(&payload, &candidates), vec!["mara", "tom"]);
    }

    #[test]
    fn a_list_that_is_not_an_array_yields_nothing() {
        let candidates = vec![ListPath {
            at: PayloadPath::of(["requested_reviewers"]),
            entry: PayloadPath::of(["login"]),
        }];
        for payload in [
            serde_json::json!({ "requested_reviewers": serde_json::Value::Null }),
            serde_json::json!({ "requested_reviewers": { "login": "mara" } }),
            serde_json::json!({}),
        ] {
            assert!(resolve_list(&payload, &candidates).is_empty(), "{payload}");
        }
    }

    /// The corpus-level question the contract battery asks, and the record
    /// that cannot answer it alone: `fields.assignee` being null says nothing
    /// about whether `name` is the right key inside one.
    #[test]
    fn a_key_an_object_lacks_is_named_and_a_null_is_not() {
        let issue = jira_issue();
        assert!(
            names_a_missing_key(&issue, &PayloadPath::of(["fields", "status", "nam"])),
            "the object a status name would sit in is on this record and has no `nam` in it"
        );
        assert!(
            names_a_missing_key(&issue, &PayloadPath::of(["fieldz", "status", "name"])),
            "a typo in any segment is a key missing from an object the source wrote -- the \
             payload root, here"
        );
        assert!(
            names_a_missing_key(&issue, &PayloadPath::of(["statuss"])),
            "and so is a one-segment path, which sits in the payload itself"
        );
        assert!(
            !names_a_missing_key(&issue, &PayloadPath::of(["fields", "priority", "name"])),
            "an unset Jira field is null: this record demands nothing of the declaration"
        );
        assert!(
            !names_a_missing_key(
                &issue,
                &PayloadPath::of(["fields", "status", "name", "deeper"])
            ),
            "the walk stopped on a string the source wrote, not on a key it lacks"
        );
        assert!(
            !names_a_missing_key(&issue, &PayloadPath::of(["fields", "status", "name"])),
            "a path that resolves names no missing key"
        );
        assert!(!names_a_missing_key(
            &issue,
            &PayloadPath::of([] as [&str; 0])
        ));
    }

    #[test]
    fn a_miss_is_a_miss_however_the_source_spelled_it() {
        let issue = jira_issue();
        for path in [
            PayloadPath::of(["fields", "status", "nam"]),
            PayloadPath::of(["fields", "priority", "name"]),
            PayloadPath::of(["fields", "nothing", "here"]),
            PayloadPath::of(["fields", "status", "name", "deeper"]),
        ] {
            assert_eq!(at(&issue, &path), None, "{path:?}");
        }
        assert!(at(&issue, &PayloadPath::of(["fields", "status", "name"])).is_some());
    }

    #[test]
    fn declarations_are_keyed_by_source_then_kind() {
        let declarations = Declarations::empty().with(
            "jira-eu",
            vec![KindPaths {
                kind: "ticket".to_owned(),
                status_name: vec![PayloadPath::of(["fields", "status", "name"])],
                ..KindPaths::default()
            }],
        );
        assert!(declarations.get("jira-eu", "ticket").is_some());
        assert_eq!(declarations.get("jira-eu", "pr"), None);
        assert_eq!(declarations.get("jira", "ticket"), None);
        assert_eq!(
            declarations.as_json()["jira-eu"]["ticket"]["status_name"],
            serde_json::json!([["fields", "status", "name"]])
        );
    }
}
