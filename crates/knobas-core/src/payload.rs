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

/// Where an item's **parent** sits in its payload, and the names that parent
/// answers to (ADR-0018, `CONTEXT.md`'s **Parent**).
///
/// The parent is the entity you can open that an item sits directly inside: a
/// repo for a commit, a build configuration for a build, the parent page for a
/// page, the epic for a ticket. It comes from the source, so the adapter says
/// where it is, the same way it says where a status is. It is neither a link
/// nor a project.
///
/// * `kind` is the parent's entity kind (`repo`, `build_config`). It is
///   declared rather than looked up because a parent need not be in the
///   mirror -- an epic outside the synced JQL is still the parent its children
///   name -- and a suggestion for `under:` still has to say what it is.
/// * `at` is where the parent is. It leads either to **one object**, the
///   direct parent, or to **an array of objects**, the whole chain outermost
///   first with the direct parent last (a Confluence page's `ancestors`). The
///   array form is what makes `under:` transitive for a source whose records
///   carry their chain.
/// * `names` are where, inside each parent object, the names it answers to
///   live, in the parent's own spelling -- a repo's `owner/name` and its bare
///   `name`, a configuration's title and its id. `under:<name>` matches them
///   exactly, ignoring case. The first name is the one a path line shows.
///
/// A declaration whose `at` leads nowhere, or to neither shape, is a miss: the
/// item has no parent as far as knobas knows, which is never a guess.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParentPath {
    /// The parent's entity kind.
    pub kind: String,
    /// Where the parent object, or the chain of them, is.
    pub at: PayloadPath,
    /// Where each name the parent answers to sits inside a parent object.
    #[serde(default)]
    pub names: Vec<PayloadPath>,
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
    /// Where this item's parent is and what it answers to (ADR-0018). Empty
    /// for a kind with no parent -- an Uptime Kuma monitor, a repo -- and for
    /// every descriptor serialized before the field existed, so `under:` misses
    /// on such a kind rather than guessing at one. Candidates in declaration
    /// order, the first whose `at` resolves winning, like every path here.
    #[serde(default)]
    pub parent: Vec<ParentPath>,
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
    pub const PARENT: &str = "parent";

    /// Every field that says where to look, in declaration order: plain
    /// paths, `reviewers`' list paths and `parent`'s parent declarations.
    /// `blocked_statuses` is not here: it is a set of names, not somewhere to
    /// look.
    pub const ALL: &[&str] = &[
        STATUS_NAME,
        PRIORITY,
        ASSIGNEE,
        REVIEWERS,
        MERGED,
        PROJECT_KEY,
        PROJECT_NAME,
        PARENT,
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

    /// Everything one source declares, kind by kind -- what
    /// [`resolve_names_as_parent`] reads, since a parent kind is named by the
    /// declarations of *other* kinds of the same source.
    pub fn of_source(&self, source_id: &str) -> impl Iterator<Item = &KindPaths> {
        self.0.get(source_id).into_iter().flat_map(BTreeMap::values)
    }

    /// Whether any kind of this source declares a parent -- the question
    /// `under:`'s coverage asks of a source (#595). A source that declares none
    /// has nothing the filter can match.
    #[must_use]
    pub fn declares_a_parent(&self, source_id: &str) -> bool {
        self.of_source(source_id)
            .any(|kind| !kind.parent.is_empty())
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

/// One parent an item names: its kind, and the names it answers to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedParent {
    /// The parent's entity kind, as the declaration says it.
    pub kind: String,
    /// Every name the declared `names` paths lead to inside this parent, in
    /// declaration order -- usable strings only, trimmed, under the same
    /// refusal [`resolve_string`] makes. Empty for a parent the record names
    /// without saying what it is called; such a parent is still in the chain.
    pub names: Vec<String>,
}

/// The parents a record names, outermost first, direct parent last (ADR-0018).
///
/// **One of the two places the two shapes of [`ParentPath::at`] are told
/// apart** -- the other is [`declared_parents!`](crate::declared_parents), and
/// `crates/knobas-core/tests/it/payload_paths.rs` holds them to one answer (the
/// deputy's ruling of 2026-10-11 on #594). No reader spells the branch itself.
///
/// Candidates in declaration order; the first whose `at` leads to **an object**
/// (the direct parent, a chain of one) or **an array** (the chain) wins. An
/// array's elements that are not objects are dropped from the chain, the
/// refusal [`resolve_list`] makes per element. An empty array resolves, to no
/// parent at all -- a top-level page saying it has no ancestors is an answer,
/// not a miss. Anything else is a miss and the next candidate is tried, and so
/// is the **empty path**: it addresses the record itself, which is an object,
/// and an item is never its own parent.
#[must_use]
pub fn resolve_parents(
    payload: &serde_json::Value,
    candidates: &[ParentPath],
) -> Vec<ResolvedParent> {
    for candidate in candidates {
        if candidate.at.segments().is_empty() {
            continue;
        }
        let objects: Vec<&serde_json::Value> = match at(payload, &candidate.at) {
            Some(object @ serde_json::Value::Object(_)) => vec![object],
            Some(serde_json::Value::Array(chain)) => {
                chain.iter().filter(|e| e.is_object()).collect()
            }
            _ => continue,
        };
        return objects
            .into_iter()
            .map(|object| ResolvedParent {
                kind: candidate.kind.clone(),
                names: parent_names(object, &candidate.names),
            })
            .collect();
    }
    Vec::new()
}

/// The names one parent object answers to: each declared path's usable
/// string, in declaration order.
fn parent_names(object: &serde_json::Value, names: &[PayloadPath]) -> Vec<String> {
    names
        .iter()
        .filter_map(|path| resolve_string(object, std::slice::from_ref(path)))
        .collect()
}

/// The names a **mirrored** record answers to as a parent of `kind`.
///
/// `under:` also returns the parent itself when it is mirrored (ADR-0018), and
/// the only thing that says what a parent is called is the children's
/// declaration. So the parent is found by reading those declared `names` off
/// its **own** record: every [`ParentPath`] of this source whose `kind` is the
/// record's kind contributes its names, read from the payload's root. That
/// holds because a source names a parent with the parent's own record shape --
/// a Gitea pull request's `base.repo` is a repository object, a Confluence
/// ancestor is a page, a Jira `fields.parent` is an issue. That assumption is
/// an obligation on every adapter, and contract battery clause 6 certifies it
/// against the adapter's own corpus (the deputy's ruling of 2026-10-11 on
/// #595). Distinct names, in the order first declared;
/// [`declared_names_as_parent!`](crate::declared_names_as_parent) is the same
/// rule in SQL, and yields a set.
#[must_use]
pub fn resolve_names_as_parent<'a>(
    payload: &serde_json::Value,
    kind: &str,
    declared: impl IntoIterator<Item = &'a KindPaths>,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for paths in declared {
        for parent in paths.parent.iter().filter(|p| p.kind == kind) {
            for name in parent_names(payload, &parent.names) {
                if !out.contains(&name) {
                    out.push(name);
                }
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
/// adapter's own declaration, and when this was written it was the one that
/// could not be: an ancestor path is a **list of strings joined in order**,
/// and `KindPaths` had no slot shaped like that. #594 added one --
/// [`KindPaths::parent`], whose `at` may lead to the chain -- and #596 put
/// every row's path on it through [`path_line_read!`], which expands this
/// only as its **fallback** for a kind that declares no parent. Confluence
/// declaring its page parent on `ancestors` (#599) is what lets this read
/// expire into the declaration. Until then it stays under ADR-0007's
/// *interim* discipline, and meets all three of its requirements:
///
/// 1. **It misses, never guesses.** A payload with no `ancestors`, an
///    `ancestors` that is not an array, elements that are not objects,
///    elements whose `title` is absent, not a string, or blank -- every one of
///    them contributes nothing, and a record with no usable segment at all
///    yields `null` rather than an empty string. `string_agg` over no rows is
///    `null`, which is what makes that true by construction rather than by a
///    guard someone has to remember.
/// 2. **One named statement**, this one, expanded inside [`path_line_read!`]
///    and nowhere else; that is what the reads that draw a row
///    (`knobas_search`'s launcher corpora, board and lists, and `knobas_app`'s
///    room and entity statements) expand.
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

/// SQL for "the parent chain this record declares, as one line": each parent's
/// **first** declared name, outermost first, joined by
/// [`ANCESTOR_SEPARATOR`] (#596, ADR-0018). `null` for a record whose kind
/// declares no parent, whose declared `at` misses, or whose parents carry no
/// usable name -- `string_agg` over no rows is `null`, so a miss is absence by
/// construction, as in [`ancestor_path_read!`].
///
/// Built on [`declared_parent_names!`] and nothing else, so the branch on
/// `at`'s two shapes stays in [`declared_parents!`] (the deputy's ruling of
/// 2026-10-11 on #594). [`parent_path`] is the same rule over
/// [`resolve_parents`], and `crates/knobas-core/tests/it/payload_paths.rs`
/// holds the two to one answer. A parent that answers to no name is left out
/// of the line rather than drawn as an empty segment, the refusal
/// [`ancestor_path_read!`] makes for an ancestor with no title.
///
/// Rows read the line through [`path_line_read!`], never through this alone.
#[macro_export]
macro_rules! declared_parent_path {
    ($decl:literal) => {
        concat!(
            "(select string_agg(f.name, ' \u{203a} ' order by f.pos) from \
              (select distinct on (pn.pos) pn.pos, pn.name from ",
            $crate::declared_parent_names!($decl),
            " pn order by pn.pos, pn.name_ord) f)"
        )
    };
}

/// SQL for "where this record sits", as the one line a row draws: the parent
/// chain the record's kind declares ([`declared_parent_path!`]), or, where it
/// declares none, the Confluence ancestor path [`ancestor_path_read!`] has read
/// since #284.
///
/// **The one expression every row's `path` is.** The launcher's search, its
/// board and its lists, a room's lines and the detail panel all expand this
/// and nothing else, so no two of them can disagree about where an item is.
/// `$decl` is where the declarations are bound, as for [`declared_string!`];
/// the row's alias must be `i`.
///
/// The ancestor read stays as the fallback until Confluence declares its page
/// parent on `ancestors` (#599). The two then read the same array and draw the
/// same line, and the fallback can expire into the declaration.
#[macro_export]
macro_rules! path_line_read {
    ($decl:literal) => {
        concat!(
            "coalesce(",
            $crate::declared_parent_path!($decl),
            ", ",
            $crate::ancestor_path_read!("i.payload"),
            ")"
        )
    };
}

/// The line a row draws for a parent chain: each parent's first name,
/// outermost first, joined by [`ANCESTOR_SEPARATOR`]; `None` when no parent
/// answers to a name. The Rust side of [`declared_parent_path!`].
#[must_use]
pub fn parent_path(chain: &[ResolvedParent]) -> Option<String> {
    let names: Vec<&str> = chain
        .iter()
        .filter_map(|parent| parent.names.first().map(String::as_str))
        .collect();
    (!names.is_empty()).then(|| names.join(ANCESTOR_SEPARATOR))
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

/// SQL for "the parents this record names", as a table expression of
/// `(pos, kind, obj, names)`: one row per parent object, `pos` ordering the
/// chain outermost first, `names` the winning candidate's declared name paths.
///
/// **The one SQL place the two shapes of [`ParentPath::at`] are told apart**
/// ([`resolve_parents`] is the Rust one). Written to be joined -- `cross join
/// lateral declared_parents!("$1") p` -- and read through
/// [`declared_parent_names!`] by every `under:` statement.
///
/// The winning candidate is the first, in declaration order, whose non-empty
/// `at` leads to an object or an array. An object becomes a one-element array
/// so both shapes walk alike, and elements that are not objects are dropped.
/// `jsonb_array_elements` raises on a scalar, so the `case` is what lets a
/// record whose `at` leads somewhere odd miss instead of failing the read for
/// every source.
#[macro_export]
macro_rules! declared_parents {
    ($decl:literal) => {
        concat!(
            "(select e.pos, w.kind, e.obj, w.names from (select c.kind, c.at_segs, c.names from \
              (select t.p->>'kind' as kind, ",
            $crate::declared_segments!("t.p->'at'"),
            " as at_segs, t.p->'names' as names, t.ord from jsonb_array_elements(",
            $crate::declared_array!($decl, "parent"),
            ") with ordinality as t(p, ord)) c \
              where cardinality(c.at_segs) > 0 \
                and jsonb_typeof(i.payload #> c.at_segs) in ('object', 'array') \
              order by c.ord limit 1) w \
             cross join lateral jsonb_array_elements(\
               case when jsonb_typeof(i.payload #> w.at_segs) = 'array' \
                    then i.payload #> w.at_segs \
                    else jsonb_build_array(i.payload #> w.at_segs) end) \
               with ordinality as e(obj, pos) \
             where jsonb_typeof(e.obj) = 'object')"
        )
    };
}

/// SQL for "every name every parent this record names answers to", as a table
/// expression of `(pos, kind, name_ord, name)`. `under:` matches against
/// `name`, and [`declared_parent_path!`] takes the first `name_ord` of each
/// `pos` for the row's path line.
///
/// Built on [`declared_parents!`] and nothing else, so the shape branch stays
/// in one place. A name path that lands on anything but a non-blank string
/// contributes nothing, the refusal [`declared_string!`] makes.
#[macro_export]
macro_rules! declared_parent_names {
    ($decl:literal) => {
        concat!(
            "(select pr.pos, pr.kind, n.name_ord, btrim(pr.obj #>> n.segs) as name from ",
            $crate::declared_parents!($decl),
            " pr cross join lateral (select ",
            $crate::declared_segments!("q.np"),
            " as segs, q.name_ord from jsonb_array_elements(\
               case when jsonb_typeof(pr.names) = 'array' then pr.names else '[]'::jsonb end) \
               with ordinality as q(np, name_ord)) n \
             where jsonb_typeof(pr.obj #> n.segs) = 'string' \
               and nullif(btrim(pr.obj #>> n.segs), '') is not null)"
        )
    };
}

/// SQL for "the names this mirrored record answers to as a parent", as a
/// table expression of one column, `name` -- see [`resolve_names_as_parent`]
/// for the rule. A set: `distinct`, in no order.
///
/// Reads every kind this record's source declares, not only its own: a repo
/// declares no parent, and it is the commit's declaration that says a repo
/// answers to `full_name` and `name`.
#[macro_export]
macro_rules! declared_names_as_parent {
    ($decl:literal) => {
        concat!(
            "(select distinct btrim(i.payload #>> s.segs) as name from (select ",
            $crate::declared_segments!("nm.np"),
            " as segs from jsonb_each(case when jsonb_typeof(",
            $decl,
            "::jsonb -> i.source_id) = 'object' then ",
            $decl,
            "::jsonb -> i.source_id else '{}'::jsonb end) as k(kind, kp) \
             cross join lateral jsonb_array_elements(\
               case when jsonb_typeof(k.kp->'parent') = 'array' then k.kp->'parent' \
                    else '[]'::jsonb end) as pp(p) \
             cross join lateral jsonb_array_elements(\
               case when jsonb_typeof(pp.p->'names') = 'array' then pp.p->'names' \
                    else '[]'::jsonb end) as nm(np) \
             where pp.p->>'kind' = i.kind) s \
             where jsonb_typeof(i.payload #> s.segs) = 'string' \
               and nullif(btrim(i.payload #>> s.segs), '') is not null)"
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

    /// A descriptor serialized by a peer built before ADR-0018 has no
    /// `parent` key at all. It must still decode, and decode as a kind that
    /// declares no parent -- which every `under:` read then misses on -- rather
    /// than failing the whole descriptor.
    #[test]
    fn a_descriptor_from_before_parents_decodes_as_declaring_no_parent() {
        let older = serde_json::json!({
            "kind": "commit",
            "status_name": [],
            "priority": [],
            "assignee": [],
            "reviewers": [],
            "merged": [],
            "project_key": [],
            "project_name": [],
            "blocked_statuses": []
        });
        let decoded: KindPaths = serde_json::from_value(older).unwrap();
        assert!(decoded.parent.is_empty());
        assert_eq!(
            decoded,
            KindPaths {
                kind: "commit".to_owned(),
                ..KindPaths::default()
            }
        );
    }

    /// The wire spelling a declared parent travels in: the parent's kind, where
    /// it sits in the payload, and the names it answers to, each a key path.
    #[test]
    fn a_declared_parent_travels_as_kind_at_and_names() {
        let wire = serde_json::json!({
            "kind": "build",
            "parent": [{
                "kind": "build_config",
                "at": ["buildType"],
                "names": [["name"], ["id"]]
            }]
        });
        let decoded: KindPaths = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(
            decoded.parent,
            vec![ParentPath {
                kind: "build_config".to_owned(),
                at: PayloadPath::of(["buildType"]),
                names: vec![PayloadPath::of(["name"]), PayloadPath::of(["id"])],
            }]
        );
        assert_eq!(
            serde_json::to_value(&decoded).unwrap()["parent"],
            wire["parent"]
        );
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
