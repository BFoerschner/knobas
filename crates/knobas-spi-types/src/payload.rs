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
//! [`declared_string!`](crate::declared_string), [`declared_flag!`],
//! [`declared_list!`], [`declared_state!`] and [`declared_project_path!`]
//! rather than by pulling every payload into Rust. The Rust resolvers below
//! ([`resolve_string`], [`resolve_flag`], [`resolve_list`], [`resolve_state`],
//! [`resolve_project_path`]) are
//! the same rule for callers that already hold the payload -- the contract
//! battery is the one that matters, since it checks an adapter's declarations
//! against its own corpus with no database in sight.
//!
//! Two implementations of one rule is a drift risk, and it is pinned rather
//! than hoped: `crates/knobas-core/tests/it/payload_paths.rs` runs the same
//! payload/declaration pairs through both and asserts they agree, so a change
//! to either that the other does not make turns a test red. A declared state
//! has a third spelling, the signals' `knobas_core::resolved_state!`, held to the
//! same cases there.
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
//! The one field that asks more of a candidate is [`KindPaths::state`]
//! (#666): a value of the right type is not enough, it must also be one the
//! candidate's own value map names, or the next candidate is tried. That is
//! what lets a merged flag that is `false` hand over to the state word.
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(transparent)]
#[ts(export_to = "sources.ts")]
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct ParentPath {
    /// The parent's entity kind.
    pub kind: String,
    /// Where the parent object, or the chain of them, is.
    pub at: PayloadPath,
    /// Where each name the parent answers to sits inside a parent object.
    #[serde(default)]
    pub names: Vec<PayloadPath>,
}

/// Where the **chain of projects** an item sits in is, and what each project
/// in it is keyed and called (#1019, ADR-0010).
///
/// A source whose projects nest (a TeamCity project and its subprojects)
/// names the item's own project with [`KindPaths::project_key`] and
/// [`KindPaths::project_name`]; this says where the whole chain is, so a
/// project takes in everything under it. Not a parent: a project is a
/// grouping, not something you open (ADR-0018).
///
/// * `at` leads to an **array of objects**, outermost first, ending in the
///   item's own project. Anything else is a miss.
/// * `key` and `name` are candidate paths inside each object, read as
///   [`KindPaths::project_key`] is: the first non-blank string wins. A
///   project with no readable key is left out of the chain.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct ProjectPath {
    /// Where the array of project objects is.
    pub at: PayloadPath,
    /// Where a project's key sits inside its object.
    #[serde(default)]
    pub key: Vec<PayloadPath>,
    /// Where a project's name sits inside its object.
    #[serde(default)]
    pub name: Vec<PayloadPath>,
}

/// Where a record keeps **the files attached to it**, and where each file's
/// facts sit inside its element (#2404): Jira's `fields.attachment`.
///
/// * `at` leads to an **array of objects**, in the source's order. Anything
///   else is a miss.
/// * every other field is a list of candidate paths inside one element, read
///   as [`KindPaths::project_key`] is: the first non-blank string wins, and
///   `size` the first whole number. An element with no readable `id`, `name`
///   or `content` is left out: a file without an address is not one a reader
///   can open.
///
/// The list, not the bytes: the mirror holds what Jira says about each file,
/// and the bytes come live through the source's credential.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct AttachmentPath {
    /// Where the array of attachment objects is.
    pub at: PayloadPath,
    /// The source's id for the file, unique within the source.
    #[serde(default)]
    pub id: Vec<PayloadPath>,
    /// The file name.
    #[serde(default)]
    pub name: Vec<PayloadPath>,
    /// The media type the source reports (`image/png`).
    #[serde(default)]
    pub mime: Vec<PayloadPath>,
    /// The size in bytes, a whole JSON number.
    #[serde(default)]
    pub size: Vec<PayloadPath>,
    /// Who attached it, by the name a person reads.
    #[serde(default)]
    pub author: Vec<PayloadPath>,
    /// When it was attached, an ISO 8601 instant ([`parse_instant`]).
    #[serde(default)]
    pub created: Vec<PayloadPath>,
    /// The address of the file's bytes at the source.
    #[serde(default)]
    pub content: Vec<PayloadPath>,
    /// The address of a small preview the source renders, for images.
    #[serde(default)]
    pub thumbnail: Vec<PayloadPath>,
}

/// One file attached to an item, as [`resolve_attachments`] reads it off the
/// record (#2404). `EntityDetail.attachments` on the wire. Every optional
/// fact is `null` where the source did not say, never a guess.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Attachment {
    /// The source's id for the file: what a later read of its bytes names.
    pub id: String,
    pub name: String,
    pub mime: Option<String>,
    /// Bytes.
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    pub author: Option<String>,
    pub created: Option<chrono::DateTime<chrono::Utc>>,
    /// Where the source serves the bytes: what *Open* hands the browser.
    pub content_url: String,
    /// Where the source serves a small preview, for an image.
    pub thumbnail_url: Option<String>,
}

/// One project of a declared chain ([`resolve_project_path`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChainProject {
    pub key: String,
    pub name: Option<String>,
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
/// Every path field is a list of *candidates*, most specific first: one
/// adapter's alternative spellings of its own field, never knobas guessing.
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
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
    /// The chain of projects the item sits in, outermost first, ending in its
    /// own (#1019): what lets a project take in its subprojects. TeamCity's
    /// `projectPath[]`, `id` and `name`. Empty for a source whose projects do
    /// not nest, and a miss leaves the item in its own project only.
    #[serde(default)]
    pub project_path: Vec<ProjectPath>,
    /// The status names this source considers **blocked-like**, in its own
    /// spelling (`Blocked`, `Waiting for support`). Names, not a path: which
    /// statuses mean "stuck" is a property of the source's workflow, not of
    /// one record. Matched case-insensitively by its reader.
    #[serde(default)]
    pub blocked_statuses: Vec<String>,
    /// The status names this source considers **done-like**, in its own
    /// spelling (`Done`, `Closed`, `Resolved`): the same shape as
    /// [`blocked_statuses`](Self::blocked_statuses), and matched the same way,
    /// case-insensitively against [`status_name`](Self::status_name). While an
    /// item's status is one of them, the job context anchored on it is
    /// archived (#1501); a workflow spelling done otherwise never archives,
    /// which is the safe miss. Empty for a source with no such status.
    #[serde(default)]
    pub done_statuses: Vec<String>,
    /// Where this item's parent is and what it answers to (ADR-0018). Empty
    /// for a kind with no parent -- an Uptime Kuma monitor, a repo -- and for
    /// every descriptor serialized before the field existed, so `under:` misses
    /// on such a kind rather than guessing at one. Candidates in declaration
    /// order, the first whose `at` resolves winning, like every path here.
    #[serde(default)]
    pub parent: Vec<ParentPath>,
    /// Where the state a row's chip shows sits, and what each value means
    /// (#666): a pull request's open / merged / closed, a build's running or
    /// outcome. Candidates in declaration order; the first whose value its
    /// own map names wins ([`resolve_state`]). Empty for a kind with no
    /// state, which draws no chip.
    #[serde(default)]
    pub state: Vec<StatePath>,
    /// The branch a pull request is opened **from**, by the name the branch's
    /// own record is titled with (#712): Gitea's `head.ref`. What tells a
    /// room's Code tile which branches already have an open pull request, so
    /// a branch nobody has proposed yet is drawn as one. Empty for every kind
    /// that is not a proposal, and a miss leaves a branch unproposed.
    #[serde(default)]
    pub head: Vec<PayloadPath>,
    /// The commit a record of code stands at, by its full object id (#710):
    /// a pull request's head commit, a branch's tip, a commit itself. What
    /// the detail asks the mirror's builds about: those whose declared
    /// [`revision`](Self::revision) is this commit. Empty for every kind that
    /// is not code, and a miss draws no builds.
    #[serde(default)]
    pub commit: Vec<PayloadPath>,
    /// The commits a build ran on, by their full object ids (#710): one per
    /// VCS root, so a list. TeamCity's `revisions.revision[].version`. Empty
    /// for every kind that is not a build, and a miss is a build the detail
    /// of no commit draws.
    #[serde(default)]
    pub revision: Vec<ListPath>,
    /// The repo a pull request is opened **from**, by the name a branch's
    /// path line draws it with (#718): Gitea's `head.repo.full_name`, which
    /// is a fork's and not the base repo's when the pull request comes from
    /// a fork. With [`head`](Self::head), what names the one branch a pull
    /// request stands for. Empty for every kind that is not a proposal, and
    /// a miss (a deleted fork) proposes no branch.
    #[serde(default)]
    pub head_repo: Vec<PayloadPath>,
    /// The name of the default branch of the repo a branch sits in (#718):
    /// for Gitea, `repository.default_branch` as `map` writes it into a
    /// branch's payload. A branch titled so is its repo's default, which
    /// the Code tile never draws as one wanting a pull request. On a
    /// repository, its own default branch (#2307, Gitea's `default_branch`):
    /// what its detail carries to the repository page. Empty for every other
    /// kind, and a miss draws the branch as any other.
    #[serde(default)]
    pub default_branch: Vec<PayloadPath>,
    /// The ticket's type in the source's own spelling (#811): Jira's
    /// `fields.issuetype.name` (`Bug`, `Story`). What `issuetype:` matches,
    /// ignoring case. Its own field and not the entity kind: `type:` already
    /// means the kind. Empty for every kind with no such type, and a miss
    /// matches no `issuetype:` value.
    #[serde(default)]
    pub issue_type: Vec<PayloadPath>,
    /// The labels an item carries (#811): Jira's `fields.labels`, an array
    /// of strings, so `entry` is empty. What `label:` matches, ignoring case.
    #[serde(default)]
    pub labels: Vec<ListPath>,
    /// The states of the sprints an item is in, one per sprint (#811):
    /// `active`, `future`, `closed`. For Jira the sprint is a custom field
    /// whose id is the instance's own, so `map` stores the sprints beside the
    /// record as `sprints: [{id, name, state}]` and this points there.
    /// `sprint:current` matches an item with an `active` one, ignoring case.
    /// Empty for a kind with no sprints, and a miss is in no sprint.
    #[serde(default)]
    pub sprint_state: Vec<ListPath>,
    /// A ticket's original estimate, **in seconds** (#816): Jira's
    /// `fields.timeoriginalestimate`. A JSON number; the job card draws it.
    /// Seconds and nothing else, so a source that keeps hours or a phrase
    /// (`"3h"`) declares no estimate rather than one every reader would
    /// misread. Empty for every kind with no estimate, and a miss draws none.
    #[serde(default)]
    pub estimate: Vec<PayloadPath>,
    /// How many comments a ticket has (#894): Jira's `fields.comment.total`.
    /// A whole JSON number, read as `estimate` is; the ticket card draws
    /// "N comments" above zero. Empty for every kind with no count, and a
    /// miss draws nothing.
    #[serde(default)]
    pub comment_count: Vec<PayloadPath>,
    /// A ticket's description **as the source wrote it** (#2308): Jira's
    /// `fields.description`, Gitea's `body`. A string, read as `issue_type`
    /// is; the ticket's reader draws it by [`markup`](Self::markup). Not
    /// `body_text`, which is the search text: title, description and
    /// comments joined. Empty for every kind with no description, and a miss
    /// draws none.
    #[serde(default)]
    pub description: Vec<PayloadPath>,
    /// How the source writes this kind's prose (#2308): its
    /// [`description`](Self::description) and the comments of its thread.
    /// A property of the source, not a path, like
    /// [`blocked_statuses`](Self::blocked_statuses). Defaults to
    /// [`Markup::Text`], which the shell draws as text: a source that says
    /// nothing is never parsed as markup it did not write.
    #[serde(default)]
    pub markup: Markup,
    /// Where a page sits among its siblings in the source's own order
    /// (#953): a whole JSON number, lowest first. The Docs tree orders a
    /// parent's children by it, and a child without one follows, by title.
    /// Empty for every kind the source does not order.
    #[serde(default)]
    pub position: Vec<PayloadPath>,
    /// Which part of a shell-owned frame this kind is: of the Builds frame
    /// (ADR-0022, #848), a [`Frame::Pipeline`] the Builds view lists or a
    /// [`Frame::Run`] of one; or a ticket system's [`Frame::SavedView`]
    /// (#1207). A role, not a path, like
    /// [`blocked_statuses`](Self::blocked_statuses). `None` for every kind no
    /// frame draws.
    #[serde(default)]
    pub frame: Option<Frame>,
    /// The ticket system a [`Frame::SavedView`] kind's views are listed under
    /// (#1207): Jira's is `Jira`, so its boards and filters sit under *Jira
    /// boards* and *Jira filters* whatever a person named the source. Not the
    /// descriptor's `name`, which is the instance's and changes with it.
    /// `None` lists them under the source's name; `None` for every kind that
    /// is not a saved view.
    #[serde(default)]
    pub system_name: Option<String>,
    /// The query a [`Frame::SavedView`] holds, in the source's own language
    /// (#2050): Jira's `jql`, the board's saved filter or the filter's own
    /// JQL. What the Tickets screen's source line and *View settings* draw;
    /// `sync.saved_view.source_query`. Required of a saved-view kind by the
    /// ticket-system minimum, and a view whose record carries none draws no
    /// query. Empty for every kind that is not a saved view.
    #[serde(default)]
    pub source_query: Vec<PayloadPath>,
    /// A saved view's board type in the source's own word (#2050): Jira's
    /// `type` (`scrum`, `kanban`, `simple`). Optional; a filter has none.
    #[serde(default)]
    pub board_type: Vec<PayloadPath>,
    /// The query a saved view narrows [`source_query`](Self::source_query)
    /// with (#2050): a Jira Kanban board's `sub_query`. Optional.
    #[serde(default)]
    pub sub_query: Vec<PayloadPath>,
    /// A saved view's active sprint (#2050), a JSON **object**
    /// `{"id", "name", "end_date"}` as the shell draws it: Jira's `sprint`
    /// on a Scrum board. Optional; a miss is no sprint.
    #[serde(default)]
    pub sprint: Vec<PayloadPath>,
    /// What a pipeline is called (#848): TeamCity's configuration `name`.
    /// Empty for every kind that is not a pipeline.
    #[serde(default)]
    pub name: Vec<PayloadPath>,
    /// The group a pipeline sits in, in the source's own word (#848): a
    /// TeamCity project, a Jenkins folder. TeamCity's `projectName`.
    #[serde(default)]
    pub group: Vec<PayloadPath>,
    /// The chain of groups a pipeline sits in, **outermost first**, ending in
    /// the one [`group`](Self::group) names (#849, R-9.3): a TeamCity project
    /// and every project above it, below the server's root. What the Builds
    /// list draws its project tree from. A list of names, so a [`ListPath`];
    /// TeamCity's `projectPath[].name`, which the adapter writes beside the
    /// record from `/app/rest/projects`. Empty for every kind that is not a
    /// pipeline, and a miss draws the pipeline under its `group` alone.
    #[serde(default)]
    pub group_path: Vec<ListPath>,
    /// The pipeline's identity as **both** of its kinds spell it (#848): on a
    /// pipeline its own id, on a run the id of the pipeline it ran. A run
    /// belongs to the pipeline of its own source whose `pipeline` is equal.
    /// TeamCity: a configuration's `id`, a build's `buildTypeId`.
    #[serde(default)]
    pub pipeline: Vec<PayloadPath>,
    /// A run's number as the source writes it (#848), a string: TeamCity's
    /// `number`, which a queued build does not have yet.
    #[serde(default)]
    pub number: Vec<PayloadPath>,
    /// The branch a run built, by name (#848): TeamCity's `branchName`.
    #[serde(default)]
    pub branch: Vec<PayloadPath>,
    /// When a run started (#848): an ISO 8601 instant, extended
    /// (`2026-08-22T10:20:00Z`) or basic (`20260822T102000+0000`, TeamCity's
    /// `startDate`). A queued run has none.
    #[serde(default)]
    pub started: Vec<PayloadPath>,
    /// When a run finished (#848), as [`started`](Self::started) is written:
    /// TeamCity's `finishDate`. A run in flight has none; with `started` it
    /// is the run's duration.
    #[serde(default)]
    pub finished: Vec<PayloadPath>,
    /// How a run was started (#961), mapped onto the shell's own words: a
    /// change pushed, a schedule, or a person. A [`TriggerPath`] per
    /// candidate, read as [`state`](Self::state) is: the value at `at`,
    /// trimmed, matched exactly against the map, the first hit winning; a
    /// value no map names is a miss. TeamCity's `triggered.type`, `vcs`,
    /// `schedule` and `user`.
    #[serde(default)]
    pub trigger: Vec<TriggerPath>,
    /// Who started a run, as the source spells the account (#961), so it can
    /// be matched against the configured usernames as
    /// [`assignee`](Self::assignee) is. TeamCity's `triggered.user.username`;
    /// a run a change or a schedule started has none.
    #[serde(default)]
    pub started_by: Vec<PayloadPath>,
    /// The machine a run ran on, by the name the source gives it (#1004):
    /// TeamCity's `agent.name`. A run no agent has taken yet (a queued
    /// TeamCity build) has none.
    #[serde(default)]
    pub agent: Vec<PayloadPath>,
    /// What a run in flight is doing now, in the source's own words (#1004):
    /// TeamCity's `running-info.currentStageText` (*step 3/5 `cargo test`*).
    /// Only a run that has started and not finished has one; a finished or a
    /// queued run misses.
    #[serde(default)]
    pub current_step: Vec<PayloadPath>,
    /// How many of a run's tests failed (#1448, R-9.6): a whole JSON number,
    /// TeamCity's `testOccurrences.failed`. With
    /// [`tests_passed`](Self::tests_passed) and
    /// [`tests_ignored`](Self::tests_ignored) it is the run's test summary
    /// (*1 failed, 41 passed*). A run that ran no tests misses all three;
    /// TeamCity then serves no `testOccurrences` at all.
    #[serde(default)]
    pub tests_failed: Vec<PayloadPath>,
    /// How many of a run's tests passed (#1448): TeamCity's
    /// `testOccurrences.passed`. Read as [`tests_failed`](Self::tests_failed).
    #[serde(default)]
    pub tests_passed: Vec<PayloadPath>,
    /// How many of a run's tests were ignored (#1448): TeamCity's
    /// `testOccurrences.ignored`. Read as [`tests_failed`](Self::tests_failed).
    #[serde(default)]
    pub tests_ignored: Vec<PayloadPath>,
    /// The language a repository is written in, in the source's own word
    /// (#966, R-8.1): Gitea's `language`, which it works out from the
    /// default branch's files. Gitea writes `""` for a repository it found
    /// no language in, which every reader takes as a miss. Empty for every
    /// kind that is not a repository.
    #[serde(default)]
    pub language: Vec<PayloadPath>,
    /// The teams a repository belongs to, by name (#966, R-8.1). A list: a
    /// repository may sit in more than one. For Gitea, the organization teams
    /// the repository was added to, which `map` writes beside the record from
    /// `/repos/{owner}/{repo}/teams`, leaving out a team that holds every
    /// repository of its organization (*Owners*). Empty for every kind that
    /// is not a repository, and a miss names no team.
    #[serde(default)]
    pub teams: Vec<ListPath>,
    /// Who owns a repository, by the account's name (#1020): Gitea's
    /// `owner.login`, the organization or user the repository sits under.
    /// What a set's repository row draws where no team is declared. Empty
    /// for every kind that is not a repository.
    #[serde(default)]
    pub owner: Vec<PayloadPath>,
    /// How a monitor checks, in the source's own word (#1020): Uptime
    /// Kuma's `type` (`http`, `ping`, `dns`). Its own field and not
    /// [`issue_type`](Self::issue_type), which `issuetype:` matches. Empty
    /// for every kind that is not a monitor.
    #[serde(default)]
    pub monitor_type: Vec<PayloadPath>,
    /// How long a monitor's last check took, **in milliseconds** (#1224):
    /// Uptime Kuma's `response_time_ms`, as its `map` writes it. A JSON
    /// number, read by [`resolve_number`]; what knobas' samples are taken
    /// from beside the state, and what the Monitors tab draws. Milliseconds
    /// and nothing else, as [`estimate`](Self::estimate) is seconds. Empty for
    /// every kind that is not a monitor, and a miss is a check with no
    /// reading.
    #[serde(default)]
    pub response_time: Vec<PayloadPath>,
    /// The branch a pull request is opened **into**, by the name the
    /// branch's own record is titled with, as [`head`](Self::head) is the one
    /// it is opened from (#1227): Gitea's `base.ref`. Empty for every kind
    /// that is not a proposal.
    #[serde(default)]
    pub base: Vec<PayloadPath>,
    /// The files attached to a ticket (#2404): Jira's `fields.attachment`,
    /// each file's facts declared inside its element. The ticket page's
    /// Attachments section reads it. Empty for every kind with no
    /// attachments, and a miss lists none.
    #[serde(default)]
    pub attachments: Vec<AttachmentPath>,
}

/// How a run was started, in the shell's words (#961, #1146).
///
/// - `change`: a change pushed to what the pipeline builds: TeamCity's `vcs`.
/// - `schedule`: a schedule: TeamCity's `schedule`.
/// - `person`: a person, who [`KindPaths::started_by`] names: TeamCity's
///   `user`.
/// - `chain`: a step of a chain, queued because another step of it was
///   (#1146): TeamCity's `snapshotDependency`. [`KindPaths::started_by`] may
///   still name whoever queued the chain.
///
/// The triggers are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "sources.ts")]
pub enum Trigger {
    Change,
    Schedule,
    Person,
    Chain,
}

/// Where a run's [`Trigger`] sits, and the source's words for each (#961).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct TriggerPath {
    /// Where the source's word is: a string.
    pub at: PayloadPath,
    /// The words this candidate reads, matched exactly after trimming.
    #[serde(default)]
    pub values: Vec<TriggerValue>,
}

/// One source word a [`TriggerPath`] reads, and the trigger it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct TriggerValue {
    /// The word as the source writes it (`vcs`).
    pub value: String,
    /// What the shell calls it.
    pub trigger: Trigger,
}

impl TriggerValue {
    /// `TriggerValue::of("vcs", Trigger::Change)`.
    pub fn of(value: impl Into<String>, trigger: Trigger) -> Self {
        TriggerValue {
            value: value.into(),
            trigger,
        }
    }
}

/// Which part of a shell-owned frame a kind is: the Builds frame (ADR-0022,
/// #848), or a ticket system's saved views (ADR-0029, #1207).
///
/// - `pipeline`: what a CI source runs: a TeamCity build configuration.
/// - `run`: one execution of a pipeline: a TeamCity build.
/// - `saved_view`: a view a ticket system keeps of its own tickets (#1207): a
///   Jira board or filter. Each item of the kind becomes a ticket view,
///   whichever adapter emits it; `knobas_sync::saved_views` reads its payload.
///   The kind must declare where a view's query is (`source_query`, the
///   ticket-system minimum, #2050) and may declare its `board_type`,
///   `sub_query` and `sprint`; those are read there and at no key of the
///   frame's choosing. The frame's own keys are fixed: `kind` (`board` or
///   `filter`), `id`, `columns` (`[{"name", "statuses", "limit"}]`), `keys`
///   (the tickets' keys in the source's order) and `query`.
///
/// The parts are documented here and not on each variant because the
/// generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
/// type's and a field's doc but not a variant's (#1325).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "sources.ts")]
pub enum Frame {
    Pipeline,
    Run,
    #[serde(rename = "saved_view")]
    SavedView,
}

/// How a source writes a kind's prose (#2308): `text` (drawn as it is),
/// `markdown` (CommonMark with GitHub's extensions, Gitea's), or `jira_wiki`
/// (Jira's wiki markup: `h1.`, `{code}`, `*bold*`). The shell draws the last
/// two with its Markdown component, converting Jira's wiki markup first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "sources.ts")]
pub enum Markup {
    #[default]
    Text,
    Markdown,
    JiraWiki,
}

/// An ISO 8601 instant in any spelling a declared time may use (#848):
/// extended, as RFC 3339 writes it, extended with a colonless offset, as
/// Jira does (`2026-08-12T10:00:00.000+0200`, #2404), or basic, as TeamCity
/// does (`20260822T102000+0000`). Anything else is a miss.
#[must_use]
pub fn parse_instant(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    let raw = raw.trim();
    chrono::DateTime::parse_from_rfc3339(raw)
        .or_else(|_| chrono::DateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f%z"))
        .or_else(|_| chrono::DateTime::parse_from_str(raw, "%Y%m%dT%H%M%S%z"))
        .ok()
        .map(|t| t.with_timezone(&chrono::Utc))
}

/// Where a row's **state** sits, and the values it is drawn for (#666).
///
/// `at` leads to a string or a boolean, so a merged flag can be a candidate
/// beside a state word. `values` is the map: a value it does not name is a miss for
/// this candidate, and the next one is tried. So a TeamCity build declares
/// its `state` with only `running` and `queued` mapped, and a finished build
/// falls through to its `status`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct StatePath {
    /// Where the value is.
    pub at: PayloadPath,
    /// The values this candidate draws a chip for, matched exactly after
    /// trimming.
    #[serde(default)]
    pub values: Vec<StateValue>,
}

/// One value a [`StatePath`] draws, and how.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct StateValue {
    /// The value as the source writes it: a JSON string (`"open"`,
    /// `"FAILURE"`) or a JSON boolean (`true`). Typed, so a source writing
    /// the word `"true"` is not taken for a flag (the refusal
    /// [`resolve_flag`] makes). A string is matched after trimming.
    #[ts(type = "string | boolean")]
    pub value: serde_json::Value,
    /// How loud the chip is.
    pub tone: crate::tone::Tone,
    /// What the chip says, where the value itself is not a word a reader
    /// should see (`merged` for a merged flag's `true`). `None` shows the
    /// value.
    #[serde(default)]
    pub text: Option<String>,
}

impl StateValue {
    /// A value drawn as itself: `StateValue::of("FAILURE", Tone::Err)`, or
    /// `StateValue::of(true, Tone::Ok)` for a flag.
    pub fn of(value: impl Into<serde_json::Value>, tone: crate::tone::Tone) -> Self {
        StateValue {
            value: value.into(),
            tone,
            text: None,
        }
    }

    /// The same value drawn as `text`: `StateValue::of(true, Tone::Ok).shown_as("merged")`.
    #[must_use]
    pub fn shown_as(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

/// The chip a row draws: the resolved [`KindPaths::state`].
///
/// `EntityRow.state` on the wire. `text` is source text (or the adapter's
/// rename of it) and is drawn as text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct StateChip {
    pub text: String,
    pub tone: crate::tone::Tone,
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
    pub const PROJECT_PATH: &str = "project_path";
    pub const PARENT: &str = "parent";
    pub const STATE: &str = "state";
    pub const HEAD: &str = "head";
    pub const COMMIT: &str = "commit";
    pub const REVISION: &str = "revision";
    pub const HEAD_REPO: &str = "head_repo";
    pub const DEFAULT_BRANCH: &str = "default_branch";
    pub const ISSUE_TYPE: &str = "issue_type";
    pub const LABELS: &str = "labels";
    pub const SPRINT_STATE: &str = "sprint_state";
    pub const ESTIMATE: &str = "estimate";
    pub const COMMENT_COUNT: &str = "comment_count";
    pub const DESCRIPTION: &str = "description";
    pub const POSITION: &str = "position";
    pub const NAME: &str = "name";
    pub const GROUP: &str = "group";
    pub const GROUP_PATH: &str = "group_path";
    pub const PIPELINE: &str = "pipeline";
    pub const NUMBER: &str = "number";
    pub const BRANCH: &str = "branch";
    pub const STARTED: &str = "started";
    pub const FINISHED: &str = "finished";
    pub const TRIGGER: &str = "trigger";
    pub const STARTED_BY: &str = "started_by";
    pub const AGENT: &str = "agent";
    pub const CURRENT_STEP: &str = "current_step";
    pub const TESTS_FAILED: &str = "tests_failed";
    pub const TESTS_PASSED: &str = "tests_passed";
    pub const TESTS_IGNORED: &str = "tests_ignored";
    pub const LANGUAGE: &str = "language";
    pub const TEAMS: &str = "teams";
    pub const OWNER: &str = "owner";
    pub const MONITOR_TYPE: &str = "monitor_type";
    pub const RESPONSE_TIME: &str = "response_time";
    pub const BASE: &str = "base";
    pub const SOURCE_QUERY: &str = "source_query";
    pub const BOARD_TYPE: &str = "board_type";
    pub const SUB_QUERY: &str = "sub_query";
    pub const SPRINT: &str = "sprint";
    pub const ATTACHMENTS: &str = "attachments";

    /// Every field that says where to look, in declaration order: plain
    /// paths, `reviewers`' list paths, `parent`'s parent declarations and
    /// `state`'s candidates with their value maps.
    /// `blocked_statuses` and `done_statuses` are not here: they are sets of
    /// names, not somewhere to look; nor is `markup`, a word about the source.
    pub const ALL: &[&str] = &[
        STATUS_NAME,
        PRIORITY,
        ASSIGNEE,
        REVIEWERS,
        MERGED,
        PROJECT_KEY,
        PROJECT_NAME,
        PROJECT_PATH,
        PARENT,
        STATE,
        HEAD,
        COMMIT,
        REVISION,
        HEAD_REPO,
        DEFAULT_BRANCH,
        ISSUE_TYPE,
        LABELS,
        SPRINT_STATE,
        ESTIMATE,
        COMMENT_COUNT,
        DESCRIPTION,
        POSITION,
        NAME,
        GROUP,
        GROUP_PATH,
        PIPELINE,
        NUMBER,
        BRANCH,
        STARTED,
        FINISHED,
        TRIGGER,
        STARTED_BY,
        AGENT,
        CURRENT_STEP,
        TESTS_FAILED,
        TESTS_PASSED,
        TESTS_IGNORED,
        LANGUAGE,
        TEAMS,
        OWNER,
        MONITOR_TYPE,
        RESPONSE_TIME,
        BASE,
        SOURCE_QUERY,
        BOARD_TYPE,
        SUB_QUERY,
        SPRINT,
        ATTACHMENTS,
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
    #[cfg(feature = "sqlx")]
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

/// The chain of projects a record declares, outermost first: the first
/// candidate whose non-empty `at` leads to an array, each object element of
/// it with a readable key. Empty when nothing is declared or nothing reads.
/// [`declared_project_path!`](crate::declared_project_path) is the same rule
/// in SQL, which the migration that stores `sync.item_project.path` carries.
#[must_use]
pub fn resolve_project_path(
    payload: &serde_json::Value,
    candidates: &[ProjectPath],
) -> Vec<ChainProject> {
    let Some((candidate, elements)) = candidates.iter().find_map(|c| {
        if c.at.segments().is_empty() {
            return None;
        }
        at(payload, &c.at)?.as_array().map(|a| (c, a))
    }) else {
        return Vec::new();
    };
    elements
        .iter()
        .filter(|e| e.is_object())
        .filter_map(|e| {
            Some(ChainProject {
                key: resolve_string(e, &candidate.key)?,
                name: resolve_string(e, &candidate.name),
            })
        })
        .collect()
}

/// SQL for "the chain of projects this record declares", as a jsonb array of
/// `{key, name}` objects outermost first ([`resolve_project_path`]'s rule).
/// `$decl` is the declarations; the row's alias must be `i`, as for
/// [`declared_string!`]. Migration `0043` stores its expansion with `$decl` =
/// `dd.decl`, and `knobas-app`'s `the_stored_resolver_is_the_macros_expansion`
/// holds the two together.
#[macro_export]
macro_rules! declared_project_path {
    ($decl:literal) => {
        concat!(
            "(select coalesce(jsonb_agg(jsonb_build_object('key', x.key, 'name', x.name) \
             order by x.pos), '[]'::jsonb) from (select e.pos, ",
            $crate::project_path_part!("w.keys"),
            " as key, ",
            $crate::project_path_part!("w.names"),
            " as name from (select c.at_segs, c.keys, c.names from (select ",
            $crate::declared_segments!("t.p->'at'"),
            " as at_segs, t.p->'key' as keys, t.p->'name' as names, t.ord \
             from jsonb_array_elements(",
            $crate::declared_array!($decl, "project_path"),
            ") with ordinality as t(p, ord)) c where cardinality(c.at_segs) > 0 \
             and jsonb_typeof(i.payload #> c.at_segs) = 'array' order by c.ord limit 1) w \
             cross join lateral jsonb_array_elements(i.payload #> w.at_segs) \
             with ordinality as e(obj, pos) where jsonb_typeof(e.obj) = 'object') x \
             where x.key is not null)"
        )
    };
}

/// One project's key or name inside a chain element `e.obj`, from the
/// candidate paths at `$paths`. Part of [`declared_project_path!`].
#[doc(hidden)]
#[macro_export]
macro_rules! project_path_part {
    ($paths:literal) => {
        concat!(
            "(select nullif(btrim(e.obj #>> k.segs), '') from (select ",
            $crate::declared_segments!("q.p"),
            " as segs, q.ord from jsonb_array_elements(case when jsonb_typeof(",
            $paths,
            ") = 'array' then ",
            $paths,
            " else '[]'::jsonb end) with ordinality as q(p, ord)) k \
             where jsonb_typeof(e.obj #> k.segs) = 'string' \
             and nullif(btrim(e.obj #>> k.segs), '') is not null order by k.ord limit 1)"
        )
    };
}

/// The files a record declares it carries ([`AttachmentPath`], #2404): the
/// first candidate whose non-empty `at` leads to an array, each object
/// element of it with a readable id, name and content address, in the
/// record's order. Empty when nothing is declared or nothing reads.
#[must_use]
pub fn resolve_attachments(
    payload: &serde_json::Value,
    candidates: &[AttachmentPath],
) -> Vec<Attachment> {
    let Some((candidate, elements)) = candidates.iter().find_map(|c| {
        if c.at.segments().is_empty() {
            return None;
        }
        at(payload, &c.at)?.as_array().map(|a| (c, a))
    }) else {
        return Vec::new();
    };
    elements
        .iter()
        .filter(|e| e.is_object())
        .filter_map(|e| {
            Some(Attachment {
                id: resolve_string(e, &candidate.id)?,
                name: resolve_string(e, &candidate.name)?,
                mime: resolve_string(e, &candidate.mime),
                size: candidate
                    .size
                    .iter()
                    .find_map(|path| at(e, path)?.as_u64()),
                author: resolve_string(e, &candidate.author),
                created: resolve_string(e, &candidate.created)
                    .and_then(|t| parse_instant(&t)),
                content_url: resolve_string(e, &candidate.content)?,
                thumbnail_url: resolve_string(e, &candidate.thumbnail),
            })
        })
        .collect()
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

/// The first candidate that lands on a usable JSON **number** (#1224): finite
/// and not negative, so Uptime Kuma's `-1` for a check that did not answer is
/// a miss and never a reading. A string spelling a number (`"13"`) is passed
/// over, as [`resolve_flag`] passes over the word `"true"`: reading it would be
/// knobas deciding what a word means.
#[must_use]
pub fn resolve_number(payload: &serde_json::Value, candidates: &[PayloadPath]) -> Option<f64> {
    candidates.iter().find_map(|path| {
        let n = at(payload, path)?.as_f64()?;
        (n.is_finite() && n >= 0.0).then_some(n)
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

/// The chip a record's declared state draws, if any (#666).
///
/// Candidates in declaration order. A candidate's value is the string its
/// `at` leads to, trimmed and non-blank, or the boolean it leads to; anything
/// else misses. The value must be one its own `values` names, string for
/// string and boolean for boolean, or the candidate misses and the next is
/// tried. The chip's text is the entry's `text`, else the value as text. [`declared_state!`](crate::declared_state) is the
/// same rule in SQL, held to this one by
/// `crates/knobas-core/tests/it/payload_paths.rs`.
#[must_use]
pub fn resolve_state(payload: &serde_json::Value, candidates: &[StatePath]) -> Option<StateChip> {
    candidates.iter().find_map(|candidate| {
        let (value, text) = match at(payload, &candidate.at)? {
            serde_json::Value::String(text) if !text.trim().is_empty() => {
                let text = text.trim().to_owned();
                (serde_json::Value::String(text.clone()), text)
            }
            serde_json::Value::Bool(flag) => (serde_json::Value::Bool(*flag), flag.to_string()),
            _ => return None,
        };
        let entry = candidate.values.iter().find(|v| v.value == value)?;
        Some(StateChip {
            text: entry.text.clone().unwrap_or(text),
            tone: entry.tone,
        })
    })
}

/// How a run was started (#961): the first candidate whose `at` leads to a
/// string some entry of its map names, trimmed and matched exactly. Anything
/// else is a miss, as [`resolve_state`] misses.
#[must_use]
pub fn resolve_trigger(payload: &serde_json::Value, candidates: &[TriggerPath]) -> Option<Trigger> {
    candidates.iter().find_map(|candidate| {
        let word = at(payload, &candidate.at)?.as_str()?.trim();
        candidate
            .values
            .iter()
            .find(|v| v.value == word)
            .map(|v| v.trigger)
    })
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
        $crate::first_string_of!($crate::declared_candidates!($decl, $field))
    };
}

/// [`declared_string!`]'s rule over a relation of candidates the statement
/// already holds: `(segs, ord)` rows, the shape [`declared_candidates!`]
/// yields (#753).
///
/// What a statement that reads one field for every row of a large set uses:
/// it resolves the candidates once a source into a CTE of its own, and each
/// row then walks its source's few paths rather than the whole declaration.
/// The mini board's status over 15,000 tickets was about 17 µs a row through
/// [`declared_string!`]. One spelling of the rule, which both macros share.
#[doc(hidden)]
#[macro_export]
macro_rules! first_string_of {
    ($($candidates:tt)+) => {
        concat!(
            "(select nullif(btrim(i.payload #>> c.segs), '') from ",
            $($candidates)+,
            " c where jsonb_typeof(i.payload #> c.segs) = 'string' \
              and nullif(btrim(i.payload #>> c.segs), '') is not null \
              order by c.ord limit 1)"
        )
    };
}

/// What separates two segments of a row's path line ([`path_line_read!`]).
///
/// A single-glyph guillemet with a space either side.
///
/// The separator is chosen **once, in SQL**: the launcher row and the detail
/// panel each render whatever string [`declared_parent_path!`] joined, so
/// neither of them holds a copy of it and neither can disagree about it. What
/// this constant is for is that the macro cannot use it -- `concat!` folds
/// literals and not `const` items, so the glyph has to be written a second
/// time inside the statement. `the_path_line_joins_on_the_one_separator`
/// (`crates/knobas-core/tests/it/ancestor_path.rs`) is what keeps that second
/// spelling honest: it asserts the SQL joins on exactly this value, so the two
/// cannot drift apart unnoticed. [`parent_path`], the Rust side, joins on the
/// constant itself.
///
/// #284's `ancestor_path_read!` drew Confluence's `ancestors` on this
/// separator until #646 retired it: since #599 Confluence declares that array
/// as its page parent, so the declared chain draws the same line.
pub const ANCESTOR_SEPARATOR: &str = " \u{203a} ";

/// SQL for "the parent chain this record declares, as one line": each parent's
/// **first** declared name, outermost first, joined by
/// [`ANCESTOR_SEPARATOR`] (#596, ADR-0018). `null` for a record whose kind
/// declares no parent, whose declared `at` misses, or whose parents carry no
/// usable name -- `string_agg` over no rows is `null`, so a miss is absence by
/// construction rather than by a guard anybody has to remember.
///
/// Built on [`declared_parent_names!`] and nothing else, so the branch on
/// `at`'s two shapes stays in [`declared_parents!`] (the deputy's ruling of
/// 2026-10-11 on #594). [`parent_path`] is the same rule over
/// [`resolve_parents`], and `crates/knobas-core/tests/it/payload_paths.rs`
/// holds the two to one answer. A parent that answers to no name is left out
/// of the line rather than drawn as an empty segment: a path reading
/// `Engineering ›  › SEPA` is worse than the segment simply not being there.
///
/// Rows read the line through [`path_line_read!`], never through this alone.
///
/// **Do not "simplify" the `order by f.pos` away.** The reasoning #284 wrote
/// for `ancestor_path_read!`'s `order by a.ordinality` carries over: a
/// *reversed* order fails the path-line tests (`Engineering › Payments ›
/// SEPA` in `knobas-core/tests/it/ancestor_path.rs`), but a *removed* one may
/// not, because PostgreSQL happens to aggregate in scan order today, and
/// `string_agg` without an `ORDER BY` has no defined order at all. Order is
/// the whole meaning of a path.
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
/// chain the record's kind declares ([`declared_parent_path!`]), and nothing
/// else. A kind that declares no parent, or a declared one that misses, draws
/// no line.
///
/// **The one expression every row's `path` is.** The launcher's search, its
/// board and its lists, a room's lines and the detail panel all expand this
/// and nothing else, so no two of them can disagree about where an item is.
/// `$decl` is where the declarations are bound, as for [`declared_string!`];
/// the row's alias must be `i`.
///
/// Until #646 this fell back to #284's `ancestor_path_read!`, the Confluence
/// ancestor titles read off the payload with no declaration. Since #599
/// Confluence declares its page parent on that same `ancestors` array, so the
/// declared chain draws a page's line, and every production read binds the
/// declarations; the fallback served only a read bound to none, and it is
/// gone with the macro.
#[macro_export]
macro_rules! path_line_read {
    ($decl:literal) => {
        $crate::declared_parent_path!($decl)
    };
}

/// A `body_text` column as words a reader is shown: without the Docs search's
/// marks (#860, `knobas_source::docs::indexed`). A comment's author line
/// (`U+001F`) and a page-link line (`U+001D`) go whole, a heading's mark
/// (`U+001E`) alone, so what is left is the text the page held before the
/// marks. Every reader that shows `body_text` rather than indexing it (the
/// launcher's excerpt, the proposal evidence, the detail panel) expands this;
/// `knobas-search`'s `the_shown_body_text_macro_names_the_three_marks` holds
/// the characters to the SPI's.
#[macro_export]
macro_rules! body_text_shown {
    ($column:literal) => {
        concat!(
            "regexp_replace(",
            $column,
            ", '[\\x1d\\x1f][^\\n]*\\n?|\\x1e', '', 'g')"
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

/// SQL for "the chip this record's declared state draws": a jsonb
/// `{"text", "tone"}`, or `null` (#666). See [`resolve_state`] for the rule,
/// which this is the SQL side of. `$decl` is where the declarations are
/// bound, as for [`declared_string!`]; the row's alias must be `i`.
///
/// **The one expression every row's `state` is**, expanded beside
/// [`path_line_read!`] wherever a row is read, so no two rows can disagree
/// about what a state is. A kind that declares none, a value no map names, a
/// value that is neither a string nor a boolean: all `null`, the ADR-0007
/// miss, failing toward no chip rather than a wrong one.
///
/// The `order by s.ord, v.vord` is the declaration order, and it is what
/// makes the first candidate win: a merged pull request's flag comes before
/// its `closed` state word.
#[macro_export]
macro_rules! declared_state {
    ($decl:literal) => {
        $crate::state_of!($crate::declared_state_candidates!($decl))
    };
}

/// The declared state candidates of the row `i`, as a relation of `(segs,
/// vals, ord)`: [`declared_state!`]'s, exported for a statement that
/// resolves them once a source and reads them through [`state_of!`] (#939).
#[doc(hidden)]
#[macro_export]
macro_rules! declared_state_candidates {
    ($decl:literal) => {
        concat!(
            "(select ",
            $crate::declared_segments!("t.p->'at'"),
            " as segs, t.p->'values' as vals, t.ord from jsonb_array_elements(",
            $crate::declared_array!($decl, "state"),
            ") with ordinality as t(p, ord))"
        )
    };
}

/// [`declared_state!`]'s rule over a relation of candidates the statement
/// already holds, `(segs, vals, ord)` rows: the one spelling of the rule,
/// as [`first_string_of!`] is for a string -- save the signals' reader's,
/// [`resolved_state!`], which reads the values flattened once a statement for
/// speed (#1148); `payload_paths.rs`'s state-chip cases hold it to this one.
#[doc(hidden)]
#[macro_export]
macro_rules! state_of {
    ($($candidates:tt)+) => {
        concat!(
            "(select jsonb_build_object('text', coalesce(v.e->>'text', s.val #>> '{}'), 'tone', v.e->>'tone') \
              from (select c.ord, c.vals, case jsonb_typeof(i.payload #> c.segs) \
                      when 'string' then to_jsonb(nullif(btrim(i.payload #>> c.segs), '')) \
                      when 'boolean' then i.payload #> c.segs end as val \
                    from ",
            $($candidates)+,
            " c) s \
              cross join lateral jsonb_array_elements(\
                case when jsonb_typeof(s.vals) = 'array' then s.vals else '[]'::jsonb end) \
                with ordinality as v(e, vord) \
             where s.val is not null and v.e->'value' = s.val \
               and jsonb_typeof(v.e->'tone') = 'string' \
             order by s.ord, v.vord limit 1)"
        )
    };
}

/// SQL for "how this run was started": the [`Trigger`] word (`change`,
/// `schedule`, `person`, `chain`) or `null` (#961, #1146). The SQL side of
/// [`resolve_trigger`]; `$decl` and the row alias `i` as for
/// [`declared_state!`], whose shape this follows.
#[macro_export]
macro_rules! declared_trigger {
    ($decl:literal) => {
        concat!(
            "(select v.e->>'trigger' \
              from (select c.ord, c.vals, case jsonb_typeof(i.payload #> c.segs) \
                      when 'string' then nullif(btrim(i.payload #>> c.segs), '') end as val \
                    from (select ",
            $crate::declared_segments!("t.p->'at'"),
            " as segs, t.p->'values' as vals, t.ord from jsonb_array_elements(",
            $crate::declared_array!($decl, "trigger"),
            ") with ordinality as t(p, ord)) c) s \
              cross join lateral jsonb_array_elements(\
                case when jsonb_typeof(s.vals) = 'array' then s.vals else '[]'::jsonb end) \
                with ordinality as v(e, vord) \
             where s.val is not null and v.e->>'value' = s.val \
               and v.e->>'trigger' in ('change', 'schedule', 'person', 'chain') \
             order by s.ord, v.vord limit 1)"
        )
    };
}

/// SQL for "this record's kind declares `$field` at all": `true` when its
/// source names at least one candidate for the field, whatever the record
/// itself holds (#680).
///
/// A question about the **declaration**, not the payload, and so not a
/// read ADR-0007 governs: it is what lets a rule treat a kind its source
/// never gave a state differently from a record whose declared state missed.
/// A room's Builds tile groups a build under its configuration only where the
/// kind declares a state; a kind that declares none keeps every row, which
/// is §3a's recency fallback. `$decl` and the row alias `i` as for
/// [`declared_string!`].
#[macro_export]
macro_rules! declares_field {
    ($decl:literal, $field:literal) => {
        concat!(
            "(jsonb_array_length(",
            $crate::declared_array!($decl, $field),
            ") > 0)"
        )
    };
}

/// SQL for "the whole number the declared path leads to, or nothing" -- see
/// [`declared_string!`] for the arguments (#816). Only a JSON number counts,
/// and only an integral one: a string that looks like a number is a value the
/// source did not write as one, and reading it would be knobas guessing.
#[macro_export]
macro_rules! declared_integer {
    ($decl:literal, $field:literal) => {
        concat!(
            "(select (i.payload #>> c.segs)::bigint from ",
            $crate::declared_candidates!($decl, $field),
            " c where jsonb_typeof(i.payload #> c.segs) = 'number' \
              and (i.payload #>> c.segs) ~ '^-?[0-9]{1,18}$' order by c.ord limit 1)"
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
        $crate::list_values_of!($crate::declared_list_candidates!($decl, $field))
    };
}

/// [`declared_list!`]'s rule over a relation of list candidates the statement
/// already holds: `(at_segs, entry_segs)` rows, the shape
/// [`declared_list_candidates!`] yields (#811). What a statement that reads a
/// list for many rows uses, resolving the candidates once a source rather
/// than once a row, as [`first_string_of!`] is for a string. One spelling of
/// the rule, which both macros share.
#[doc(hidden)]
#[macro_export]
macro_rules! list_values_of {
    ($($candidates:tt)+) => {
        concat!(
            "(select nullif(btrim(e.elem #>> c.entry_segs), '') as value from ",
            $($candidates)+,
            " c cross join lateral jsonb_array_elements(\
             case when jsonb_typeof(i.payload #> c.at_segs) = 'array' \
                  then i.payload #> c.at_segs else '[]'::jsonb end) as e(elem) \
             where jsonb_typeof(e.elem #> c.entry_segs) = 'string' \
               and nullif(btrim(e.elem #>> c.entry_segs), '') is not null)"
        )
    };
}

/// The declared list candidates for one field as a relation of `(at_segs,
/// entry_segs)`, in declaration order. An implementation detail of
/// [`declared_list!`], exported for a statement that resolves them once a
/// source (#811).
#[doc(hidden)]
#[macro_export]
macro_rules! declared_list_candidates {
    ($decl:literal, $field:literal) => {
        concat!(
            "(select ",
            $crate::declared_segments!("cand->'at'"),
            " as at_segs, ",
            $crate::declared_segments!("cand->'entry'"),
            " as entry_segs from jsonb_array_elements(",
            $crate::declared_array!($decl, $field),
            ") as cands(cand))"
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

/// Every declared source's ticket candidates for one field, as
/// `(source_id, segs, ord)`: [`declared_candidates!`] walked once a source
/// rather than once a row (#753), for a statement that reads the field over
/// many tickets and resolves each row against its own source's few paths
/// with [`first_string_of!`]. The mini board's status and lanes and the
/// inbox's new-assignment rule (#1164) read through it.
#[doc(hidden)]
#[macro_export]
macro_rules! declared_ticket_paths {
    ($decl:literal, $field:literal) => {
        concat!(
            "select i.source_id, c.segs, c.ord from ",
            $crate::declared_tickets!($decl),
            " i cross join lateral ",
            $crate::declared_candidates!($decl, $field),
            " c"
        )
    };
}

/// One row per source the declarations name, aliased as a ticket row
/// (`source_id`, `kind`), which is what the declaration macros read.
#[doc(hidden)]
#[macro_export]
macro_rules! declared_tickets {
    ($decl:literal) => {
        concat!(
            "(select d.source_id, 'ticket'::text as kind
                from jsonb_object_keys(case when jsonb_typeof(",
            $decl,
            "::jsonb) = 'object' then ",
            $decl,
            "::jsonb else '{}'::jsonb end) as d(source_id))"
        )
    };
}

/// The declared candidates for one field as a numbered relation of `text[]`
/// paths: `(segs, ord)`, in declaration order.
///
/// An implementation detail of the macros above, exported only because
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
        expected.push("done_statuses");
        expected.push("frame");
        expected.push("system_name");
        expected.push("markup");
        expected.sort_unstable();
        assert_eq!(keys, expected);
    }

    /// `opened` and `merged_at` were declared for the conversation's
    /// pull-request lines (#929) and retired with them (#2173). An adapter
    /// built before the retirement still sends both: its descriptor decodes,
    /// keeps every other declaration, and carries neither on the way out.
    #[test]
    fn a_peer_that_still_declares_pull_request_times_decodes_without_them() {
        let older = serde_json::json!({
            "kind": "pr",
            "merged": [["merged"]],
            "opened": [["created_at"]],
            "merged_at": [["merged_at"]],
        });
        let paths: KindPaths = serde_json::from_value(older).unwrap();
        assert_eq!(paths.merged, vec![PayloadPath::of(["merged"])]);
        let wire = serde_json::to_value(&paths).unwrap();
        let wire = wire.as_object().unwrap();
        assert!(!wire.contains_key("opened"), "{wire:?}");
        assert!(!wire.contains_key("merged_at"), "{wire:?}");
    }

    /// A monitor's response time (#1224) is the first candidate that lands on
    /// a finite JSON number; a string, a null, an absent key or a negative
    /// reading is a miss, and the next candidate is tried.
    #[test]
    fn a_number_is_the_first_candidate_that_lands_on_one() {
        let at = |key: &str| PayloadPath::of([key]);
        let paths = [at("ms"), at("fallback")];
        let of = |v: serde_json::Value| resolve_number(&v, &paths);
        assert_eq!(of(serde_json::json!({ "ms": 13.5 })), Some(13.5));
        assert_eq!(of(serde_json::json!({ "ms": 0 })), Some(0.0));
        assert_eq!(
            of(serde_json::json!({ "ms": "13", "fallback": 7 })),
            Some(7.0),
            "a string is passed over for the next candidate"
        );
        assert_eq!(of(serde_json::json!({ "ms": null })), None);
        assert_eq!(of(serde_json::json!({ "ms": -1 })), None, "Kuma's sentinel");
        assert_eq!(of(serde_json::json!({})), None);
        assert_eq!(resolve_number(&serde_json::json!({ "ms": 13 }), &[]), None);
    }

    /// A kind's place in the Builds frame (#848) travels as a lowercase word,
    /// and a descriptor from before it decodes as no part of the frame.
    #[test]
    fn a_frame_travels_as_a_lowercase_word_and_defaults_to_none() {
        let run = KindPaths {
            kind: "build".to_owned(),
            frame: Some(Frame::Run),
            ..KindPaths::default()
        };
        let wire = serde_json::to_value(&run).unwrap();
        assert_eq!(wire["frame"], "run");
        assert_eq!(
            serde_json::to_value(Frame::Pipeline).unwrap(),
            serde_json::json!("pipeline")
        );
        assert_eq!(
            serde_json::to_value(Frame::SavedView).unwrap(),
            serde_json::json!("saved_view")
        );
        let older: KindPaths =
            serde_json::from_value(serde_json::json!({ "kind": "build" })).unwrap();
        assert_eq!(older.frame, None);
    }

    /// A run's trigger is the source's word through its map (#961): a word
    /// the map names reads as the shell's, trimmed; one it does not, a
    /// non-string or a missing key is a miss, and the first candidate wins.
    #[test]
    fn a_trigger_reads_the_source_word_through_its_map() {
        let teamcity = vec![TriggerPath {
            at: PayloadPath::of(["triggered", "type"]),
            values: vec![
                TriggerValue::of("vcs", Trigger::Change),
                TriggerValue::of("schedule", Trigger::Schedule),
                TriggerValue::of("user", Trigger::Person),
            ],
        }];
        let of = |v: serde_json::Value| resolve_trigger(&v, &teamcity);
        assert_eq!(
            of(serde_json::json!({ "triggered": { "type": " vcs " } })),
            Some(Trigger::Change)
        );
        assert_eq!(
            of(serde_json::json!({ "triggered": { "type": "schedule" } })),
            Some(Trigger::Schedule)
        );
        assert_eq!(
            of(serde_json::json!({ "triggered": { "type": "user", "user": {} } })),
            Some(Trigger::Person)
        );
        assert_eq!(
            of(serde_json::json!({ "triggered": { "type": "buildType" } })),
            None
        );
        assert_eq!(
            of(serde_json::json!({ "triggered": { "type": true } })),
            None
        );
        assert_eq!(of(serde_json::json!({})), None);
        assert_eq!(serde_json::to_value(Trigger::Change).unwrap(), "change");

        let two = vec![
            TriggerPath {
                at: PayloadPath::of(["first"]),
                values: vec![TriggerValue::of("x", Trigger::Schedule)],
            },
            teamcity[0].clone(),
        ];
        let both = serde_json::json!({ "first": "x", "triggered": { "type": "vcs" } });
        assert_eq!(resolve_trigger(&both, &two), Some(Trigger::Schedule));
    }

    /// A declared time is ISO 8601 in either spelling (#848): TeamCity's
    /// basic form and RFC 3339 read as the same instant; a date alone, a
    /// word or a blank is a miss.
    #[test]
    fn an_instant_reads_in_the_basic_and_the_extended_spelling() {
        let expected = chrono::DateTime::parse_from_rfc3339("2026-08-22T10:20:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(parse_instant("20260822T122000+0200"), Some(expected));
        assert_eq!(parse_instant("2026-08-22T10:20:00Z"), Some(expected));
        assert_eq!(parse_instant(" 20260822T102000+0000 "), Some(expected));
        assert_eq!(parse_instant("2026-08-22"), None);
        assert_eq!(parse_instant("yesterday"), None);
        assert_eq!(parse_instant(""), None);
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

    /// Jira Data Center's `fields.attachment`, declared the way the Jira
    /// adapter declares it (#2404).
    fn jira_attachments() -> Vec<AttachmentPath> {
        vec![AttachmentPath {
            at: PayloadPath::of(["fields", "attachment"]),
            id: vec![PayloadPath::of(["id"])],
            name: vec![PayloadPath::of(["filename"])],
            mime: vec![PayloadPath::of(["mimeType"])],
            size: vec![PayloadPath::of(["size"])],
            author: vec![
                PayloadPath::of(["author", "displayName"]),
                PayloadPath::of(["author", "name"]),
            ],
            created: vec![PayloadPath::of(["created"])],
            content: vec![PayloadPath::of(["content"])],
            thumbnail: vec![PayloadPath::of(["thumbnail"])],
        }]
    }

    /// The attachment list (#2404): each element of the declared array with
    /// an id, a name and a content address, in the record's order; Jira's
    /// date with its colonless offset reads, a size is a whole number, a PDF
    /// has no thumbnail, and an element missing its content address is left
    /// out rather than drawn with a link to nowhere.
    #[test]
    fn the_declared_attachments_resolve_in_order_and_an_unusable_one_is_left_out() {
        let payload = serde_json::json!({ "fields": { "attachment": [
            { "id": "50000", "filename": "psp-409-trace.png", "mimeType": "image/png",
              "size": 186_368, "author": { "name": "lena.hoffmann", "displayName": "Lena Hoffmann" },
              "created": "2026-08-12T10:00:00.000+0200",
              "content": "https://jira.example/secure/attachment/50000/psp-409-trace.png",
              "thumbnail": "https://jira.example/secure/thumbnail/50000/psp-409-trace.png" },
            { "id": "50001", "filename": "broken.bin" },
            { "id": "50002", "filename": "sepa-error-codes.pdf", "mimeType": "application/pdf",
              "size": "big", "author": { "name": "priya.nair" },
              "created": "yesterday",
              "content": "https://jira.example/secure/attachment/50002/sepa-error-codes.pdf" },
            "not an object"
        ] } });

        let list = resolve_attachments(&payload, &jira_attachments());

        assert_eq!(
            list,
            vec![
                Attachment {
                    id: "50000".to_owned(),
                    name: "psp-409-trace.png".to_owned(),
                    mime: Some("image/png".to_owned()),
                    size: Some(186_368),
                    author: Some("Lena Hoffmann".to_owned()),
                    created: Some("2026-08-12T08:00:00Z".parse().unwrap()),
                    content_url: "https://jira.example/secure/attachment/50000/psp-409-trace.png"
                        .to_owned(),
                    thumbnail_url: Some(
                        "https://jira.example/secure/thumbnail/50000/psp-409-trace.png".to_owned()
                    ),
                },
                Attachment {
                    id: "50002".to_owned(),
                    name: "sepa-error-codes.pdf".to_owned(),
                    mime: Some("application/pdf".to_owned()),
                    size: None,
                    author: Some("priya.nair".to_owned()),
                    created: None,
                    content_url: "https://jira.example/secure/attachment/50002/sepa-error-codes.pdf"
                        .to_owned(),
                    thumbnail_url: None,
                },
            ]
        );
    }

    /// A miss is a miss (ADR-0007): no declaration, a declaration whose
    /// array is absent or null, and a value that is not an array all give no
    /// attachments, never an error.
    #[test]
    fn no_declaration_and_no_array_give_no_attachments() {
        let with = serde_json::json!({ "fields": { "attachment": [
            { "id": "1", "filename": "a.txt", "content": "https://x/a.txt" } ] } });
        assert!(resolve_attachments(&with, &[]).is_empty());
        for payload in [
            serde_json::json!({ "fields": {} }),
            serde_json::json!({ "fields": { "attachment": null } }),
            serde_json::json!({ "fields": { "attachment": { "id": "1" } } }),
        ] {
            assert!(resolve_attachments(&payload, &jira_attachments()).is_empty(), "{payload}");
        }
    }

    /// Jira writes its instants as `2026-08-12T10:00:00.000+0200`, an offset
    /// without its colon, which RFC 3339 refuses (#2404).
    #[test]
    fn a_jira_instant_with_a_colonless_offset_parses() {
        assert_eq!(
            parse_instant("2026-08-12T10:00:00.000+0200"),
            Some("2026-08-12T08:00:00Z".parse().unwrap())
        );
    }
}
