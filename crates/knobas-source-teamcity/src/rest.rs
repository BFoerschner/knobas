//! TeamCity's wire shapes, the `fields=` selectors that produce them, and the
//! locator strings that select them. Pure data: nothing here performs I/O.
//!
//! # Why the selectors are narrower than the structs
//!
//! The structs accept everything a real TeamCity can send, because
//! `SyncItem::payload` keeps the record verbatim and a future selector should
//! not need a parse change. The **selectors** are narrower, and the rule is
//! exact in both directions: a selector asks for **every** name the mapping
//! reads and for **no** name it does not.
//!
//! The first half is the one with teeth. A name the mapping reads and the
//! selector omits is a field that is silently always `None` -- not a parse
//! error, not a violation, just a value that never arrives. That is what kept
//! `SyncItem::author` empty for every build until `triggered` was added here,
//! and it is why `the_field_selectors_cover_everything_the_mapping_reads`
//! exists.
//!
//! The second half is usually budget rather than correctness, and the
//! difference turns on **which type** a name is asked of, not on the name.
//! `knobas-mockd` validates `fields=` against a closed set per type and
//! answers an unknown one with 400 + an `UnknownField` violation (mockd
//! deviation 6). So `href` is served on builds *and* configurations and costs
//! only response size; `paused` is served on a configuration and is a 400 on a
//! **build**, where it is not a field at all. There is no list of "names we
//! leave out" that is true independently of the type it is asked of, and
//! `the_selectors_ask_for_nothing_no_reader_looks_at` pairs each name with its
//! selector for exactly that reason.
//!
//! Within one type it is budget: `triggered(type,user(username))` rather than
//! `triggered(type,date,user(username,name))`, which mockd would serve
//! happily.

use chrono::{DateTime, Utc};

/// What `/app/rest/server` is asked for -- the two fields
/// [`ConnectionInfo`](knobas_source::ConnectionInfo) shows.
pub(crate) const SERVER_FIELDS: &str = "version,buildNumber";

/// What `/app/rest/users/current` is asked for: whom knobas is authenticated
/// as, which is what *Test connection* puts on screen (P4).
pub(crate) const USER_FIELDS: &str = "username,name";

/// What `GET /app/rest/builds/id:{id}` is asked for.
///
/// One name, because the only thing that request asks is whether the build is
/// there at all -- `Rest::build_exists` reads the status, never the body. It
/// is still an explicit `fields=`: interfaces §4.2 requires one on every
/// TeamCity request, and TeamCity's default projection is not one any adapter
/// should rely on.
pub(crate) const BUILD_ID_FIELDS: &str = "id";

/// The record half of [`BUILD_FIELDS`], a macro so `concat!` can take it.
macro_rules! build_record {
    () => {
        concat!(
            "id,number,buildTypeId,state,status,statusText,branchName,",
            "queuedDate,startDate,finishDate,",
            "buildType(id,name,projectId,projectName),",
            "running-info(percentageComplete,currentStageText),",
            "triggered(type,user(username)),",
            "agent(name),",
            "testOccurrences(failed,passed,ignored),",
            "revisions(revision(version))"
        )
    };
}

/// What `/app/rest/buildTypes` is asked for. Without an explicit `fields=`,
/// real TeamCity answers a hyperlink stub (`id`, `href`) and mockd answers 400
/// -- the parameter is mandatory on the collections.
///
/// **No `webUrl`** (issue #516), for the reason `BUILD_FIELDS` below dropped
/// it at both levels one ticket earlier: a build configuration's URL is
/// knobas' own composition from the configured base URL now -- see
/// [`map::build_config_web_url`](crate::map) -- so the server's own is a name
/// no reader in `map` looks at, and `struct BuildType` no longer parses it.
pub(crate) const BUILD_TYPE_FIELDS: &str =
    "count,nextHref,buildType(id,name,projectId,projectName,description)";

/// What `/app/rest/projects` is asked for (#849): each project's id, its
/// name, and the project it sits in, which is the whole tree in one listing.
/// What the sync reads to write a configuration's `projectPath` and to take a
/// configured project's subprojects into the scope. `nextHref` for the reason
/// every collection selector asks for it (issue #114).
pub(crate) const PROJECT_FIELDS: &str = "count,nextHref,project(id,name,parentProjectId)";

/// What `/app/rest/builds` is asked for. The nested `buildType(...)` is what
/// makes client-side project scoping possible: the locator grammar has no
/// project dimension.
///
/// **No `webUrl`, at either level** (issue #495). A build's URL is knobas'
/// own composition from the configured base URL now -- see
/// [`map::build_web_url`](crate::map) for why -- so the server's own is a
/// name no reader in `map` looks at, and `struct Build` no longer parses it.
/// The `percentageComplete` precedent below is the same rule:
/// `the_selectors_ask_for_nothing_no_reader_looks_at` keeps both. The nested
/// `buildType(webUrl)` goes with it; nothing has ever read that one, and since
/// issue #516 a configuration's URL is composed too, so `BUILD_TYPE_FIELDS`
/// asks for no `webUrl` either.
///
/// **`revisions(revision(version))`** (#710): the sha each VCS root was built
/// at, and nothing else of it -- no branch, no root instance -- because the
/// only reader is the declared `revision` path (`descriptor.rs`), which the
/// detail of a commit finds its builds by. The build's own view asks for the
/// rest live (`BUILD_VIEW_FIELDS`).
///
/// **`triggered(type,user(username))`** (#961): the declared `trigger` and
/// `started_by`. The username was here before as `SyncItem::author`; the
/// type is what says a change or a schedule started a build no person did.
///
/// **`agent(name)`** (#1004): the declared `agent`, the machine the build ran
/// on, by name and nothing else. `running-info(currentStageText)` was already
/// here and is now also the declared `current_step`.
///
/// **`testOccurrences(failed,passed,ignored)`** (#1448): the declared test
/// counts, the summary a run row draws. Named, not left to TeamCity's
/// default, so a zero is served as `0` rather than left out.
pub(crate) const BUILD_FIELDS: &str = concat!("count,nextHref,build(", build_record!(), ")");

/// What one build is asked for by id when it is refreshed (ADR-0031, #2099):
/// [`BUILD_FIELDS`]' record, without the envelope a single build has not got.
/// One selector spelled once, so the record a refresh reads is the record a
/// run lists and `map::build_item` makes the same item of both.
pub(crate) const BUILD_RECORD_FIELDS: &str = build_record!();


/// What one parameter is asked for when `set_parameter` checks it against
/// the value the reader edited (#668): what the view draws a parameter from,
/// and nothing else. The same fields as each `property` of
/// [`BUILD_CONFIG_VIEW_FIELDS`]'s `parameters`.
pub(crate) const PARAMETER_FIELDS: &str = "name,value,inherited,type(rawValue)";

/// What `GET /app/rest/buildTypes/id:{id}` is asked for when a build
/// configuration's source view is drawn (#664): the one request that answers
/// every tab but *Overview*'s recent builds, read by [`ConfigRecord`].
///
/// Parameters come with their `inherited` flag and their type's `rawValue`,
/// which is where TeamCity says `password` -- the one thing that decides
/// whether a value may be shown at all. A step's and a trigger's
/// `properties` are asked for whole because which of them matter depends on
/// the runner or trigger type; the view picks by name and draws nothing it
/// does not know.
///
/// A VCS root's `properties` are asked for whole too, though the view reads
/// only `url` and `branch`: `fields=` selects by field name, and a property's
/// name is a *value* of the list, so there is no selector for two of them. The
/// list includes `secure:password`, which TeamCity answers by name with no
/// value (measured on the seeded 2026.1.3 server, 2026-10-12); the view draws
/// neither it nor anything else but those two.
pub(crate) const BUILD_CONFIG_VIEW_FIELDS: &str = concat!(
    "id,projectId,projectName,description,paused,",
    "parameters(property(name,value,inherited,type(rawValue))),",
    "steps(step(id,name,type,disabled,properties(property(name,value)))),",
    "triggers(trigger(id,type,disabled,properties(property(name,value)))),",
    "vcs-root-entries(vcs-root-entry(id,checkout-rules,",
    "vcs-root(id,name,vcsName,properties(property(name,value)))))"
);

/// What `/app/rest/builds` is asked for when a build configuration's view
/// lists its recent builds (#664): what a row says -- number, outcome,
/// branch, duration, age -- and whether the branch is the default one, which
/// *Run…* does not suggest by name (#669). Parsed into [`Build`], whose
/// other fields stay `None`.
pub(crate) const RECENT_BUILD_FIELDS: &str = concat!(
    "count,build(id,number,state,status,statusText,branchName,defaultBranch,",
    "queuedDate,startDate,finishDate)"
);

/// What `GET /app/rest/builds/id:{id}` is asked for when a build's source
/// view is drawn (#665), read by [`BuildRecord`]: the overview's facts, the
/// failed-test count *Tests* shows before it is read, and the parameters the
/// build ran with -- beside its configuration's parameter specs, which is
/// where TeamCity says `password` (a build's own parameter carries no spec)
/// -- and the revisions it ran on, each with its VCS root instance's name and
/// `url` (#683). Measured on the seeded 2026.1.3 server, 2026-10-12 and
/// 2026-10-13.
pub(crate) const BUILD_VIEW_FIELDS: &str = concat!(
    "number,buildTypeId,state,status,statusText,branchName,",
    "queuedDate,startDate,finishDate,",
    "buildType(id,name,projectName,parameters(property(name,type(rawValue)))),",
    "triggered(type,user(username)),agent(name),running-info(currentStageText),",
    "testOccurrences(failed),properties(property(name,value)),",
    "revisions(revision(version,vcsBranchName,",
    "vcs-root-instance(name,properties(property(name,value)))))"
);

/// What `/app/rest/changes?locator=build:(id:N)` is asked for (#665): a
/// change's sha, author, time and message, and its VCS root instance's
/// properties -- for `url`, the repository the commit is in. The list
/// includes `secure:password`, which TeamCity answers by name with no value;
/// the view reads `url` and nothing else from it.
pub(crate) const BUILD_CHANGE_FIELDS: &str = concat!(
    "count,change(id,version,username,date,comment,",
    "vcsRootInstance(properties(property(name,value))))"
);

/// What `/app/rest/testOccurrences` is asked for when a build's *Tests* tab
/// is read (#665).
pub(crate) const FAILED_TEST_FIELDS: &str = "count,testOccurrence(name,details,newFailure,muted)";

/// What `/app/rest/builds/id:N/artifacts` is asked for (#665): a top-level
/// file's name and size; a directory answers `children` and no size.
pub(crate) const ARTIFACT_FIELDS: &str = "file(name,size,children(count))";

/// One build as [`BUILD_VIEW_FIELDS`] answers it.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuildRecord {
    pub number: Option<String>,
    pub build_type_id: Option<String>,
    pub state: Option<String>,
    pub status: Option<String>,
    pub status_text: Option<String>,
    pub branch_name: Option<String>,
    pub queued_date: Option<String>,
    pub start_date: Option<String>,
    pub finish_date: Option<String>,
    pub build_type: Option<BuildRecordType>,
    pub triggered: Option<TriggeredBy>,
    pub agent: Option<Named>,
    #[serde(rename = "running-info")]
    pub running_info: Option<RunningInfo>,
    pub test_occurrences: Option<TestCounts>,
    #[serde(default)]
    pub properties: Properties,
    #[serde(default)]
    pub revisions: Revisions,
}

/// `revisions`: the VCS revision a build ran on, one per VCS root (#683).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Revisions {
    #[serde(default)]
    pub revision: Vec<Revision>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Revision {
    /// The commit's sha.
    pub version: Option<String>,
    /// The full ref, `refs/heads/main`.
    pub vcs_branch_name: Option<String>,
    #[serde(rename = "vcs-root-instance")]
    pub vcs_root_instance: Option<RootInstance>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuildRecordType {
    pub id: String,
    pub name: Option<String>,
    pub project_name: Option<String>,
    /// The configuration's parameters, for their specs only.
    #[serde(default)]
    pub parameters: Properties,
}

/// `triggered`: `user`, `vcs`, `schedule`, `buildType`, ... and the user
/// where a person did it.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct TriggeredBy {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub user: Option<User>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Named {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct TestCounts {
    pub failed: Option<u64>,
}

/// `/app/rest/changes`'s answer.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Changes {
    #[serde(default)]
    pub change: Vec<Change>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Change {
    pub id: i64,
    /// The commit's sha.
    pub version: Option<String>,
    pub username: Option<String>,
    pub date: Option<String>,
    pub comment: Option<String>,
    pub vcs_root_instance: Option<RootInstance>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct RootInstance {
    /// Asked for on a build's revisions (#683), not on a change.
    pub name: Option<String>,
    #[serde(default)]
    pub properties: Properties,
}

/// `/app/rest/testOccurrences`'s answer.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestOccurrences {
    #[serde(default)]
    pub test_occurrence: Vec<TestOccurrence>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestOccurrence {
    pub name: Option<String>,
    pub details: Option<String>,
    #[serde(default)]
    pub new_failure: bool,
    #[serde(default)]
    pub muted: bool,
}

/// `/app/rest/builds/id:N/artifacts`'s answer.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Files {
    #[serde(default)]
    pub file: Vec<FileEntry>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct FileEntry {
    pub name: String,
    pub size: Option<u64>,
    /// Present on a directory.
    pub children: Option<Children>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Children {
    pub count: Option<u64>,
}

/// One build configuration as [`BUILD_CONFIG_VIEW_FIELDS`] answers it.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigRecord {
    pub id: String,
    pub project_id: Option<String>,
    /// The project's full name -- `Parent / Child` for a nested one.
    pub project_name: Option<String>,
    pub description: Option<String>,
    pub paused: Option<bool>,
    #[serde(default)]
    pub parameters: Properties,
    #[serde(default)]
    pub steps: Steps,
    #[serde(default)]
    pub triggers: Triggers,
    #[serde(default, rename = "vcs-root-entries")]
    pub vcs_root_entries: VcsRootEntries,
}

/// TeamCity's `{"property": [...]}` list, on parameters and on the
/// properties of a step, a trigger or a VCS root.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Properties {
    #[serde(default)]
    pub property: Vec<Property>,
}

impl Properties {
    /// The value of the property called `name`, if it has a non-blank one.
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.property
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| p.value.as_deref())
            .filter(|v| !v.trim().is_empty())
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Property {
    pub name: String,
    pub value: Option<String>,
    /// Set on a configuration's parameter that a project above it defines.
    #[serde(default)]
    pub inherited: bool,
    /// The parameter's spec -- `password display='hidden'` for a secret.
    #[serde(rename = "type")]
    pub spec: Option<ParameterType>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ParameterType {
    pub raw_value: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Steps {
    #[serde(default)]
    pub step: Vec<Step>,
}

/// One build step. `type` is the runner (`simpleRunner` is *Command Line*).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Step {
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub runner: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub properties: Properties,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Triggers {
    #[serde(default)]
    pub trigger: Vec<Trigger>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct Trigger {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub properties: Properties,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct VcsRootEntries {
    #[serde(default, rename = "vcs-root-entry")]
    pub entry: Vec<VcsRootEntry>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct VcsRootEntry {
    pub id: Option<String>,
    #[serde(rename = "checkout-rules")]
    pub checkout_rules: Option<String>,
    #[serde(rename = "vcs-root")]
    pub root: Option<VcsRoot>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VcsRoot {
    pub id: Option<String>,
    pub name: Option<String>,
    pub vcs_name: Option<String>,
    #[serde(default)]
    pub properties: Properties,
}

/// Every TeamCity list response: `{count, href, nextHref, <element>: [...]}`.
/// One type for both collections -- the element key is the only difference.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct ListEnvelope {
    #[serde(default, rename = "build", alias = "buildType", alias = "project")]
    pub items: Vec<serde_json::Value>,
    /// **The server's own statement that this page is not the whole answer**,
    /// and the one thing in the response that says so (issue #114).
    ///
    /// TeamCity serves it exactly when the collection has more entries than
    /// the page it just answered with, computed from the page it actually
    /// produced rather than from the `count:` the request asked for -- so a
    /// server that serves fewer rows than were asked for still says there is
    /// more. Measured read-only against JetBrains' public instance
    /// (2026-08-29): `sinceBuild` over 42 matches answered `count:41` and
    /// `count:42` with a `nextHref`, and `count:43` with none. That is the
    /// signal [`sync::all_of`](crate::sync) ends a walk on; a short page alone
    /// is not one. Re-measured 2026-08-29 against a second, bounded query:
    /// `buildType:(id:AndroidStudioReleasesList),state:finished` over exactly
    /// 103 matches answered `count:102` and `count:103` with a `nextHref` and
    /// `count:104` with none.
    ///
    /// **One measured exception, which this adapter never meets:** a
    /// single-value locator resolves to one build and carries no `nextHref`
    /// however small the `count:` -- `id:6520690,count:1` answers one row and
    /// no such key. [`Locator`] has no `id` dimension, and the by-id request
    /// this adapter does send goes to `/app/rest/builds/id:{id}`, which is one
    /// record rather than a collection and never reaches this envelope.
    ///
    /// Absent unless `fields=` asks for it -- `count,build(id)` comes back
    /// with no `nextHref` key whatever the server has left, which is why
    /// [`BUILD_FIELDS`] and [`BUILD_TYPE_FIELDS`] both name it and
    /// `the_field_selectors_cover_everything_the_mapping_reads` pins that.
    ///
    /// Kept as the href it is rather than parsed. Nothing here follows it:
    /// it is an offset walk (`start:`), and `/app/rest/builds` grows at the
    /// front, so walking one skips rows. Only its presence is read.
    #[serde(default, rename = "nextHref")]
    pub next_href: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Server {
    pub version: Option<String>,
    pub build_number: Option<String>,
}

/// `/app/rest/users/current`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrentUser {
    pub username: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BuildType {
    pub id: String,
    pub name: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    /// Prose a human wrote about the configuration, and part of what
    /// `map::build_config_item` puts in the search blob.
    pub description: Option<String>,
}

/// `GET /app/rest/users/current/permissions` (#2040): the user's permission
/// assignments, one per permission and scope, each `isGlobalScope` or naming
/// the `project` it was given on. Measured on 2026.1.3: 21 of them for a
/// `PROJECT_VIEWER` at global scope.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PermissionAssignments {
    #[serde(default)]
    pub permission_assignment: Vec<PermissionAssignment>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PermissionAssignment {
    pub permission: PermissionId,
    pub is_global_scope: Option<bool>,
    pub project: Option<ProjectId>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct PermissionId {
    pub id: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct ProjectId {
    pub id: String,
}

/// One project as [`PROJECT_FIELDS`] answers it (#849). The server's own
/// root project is `_Root` and has no parent.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Project {
    pub id: String,
    pub name: Option<String>,
    pub parent_project_id: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Build {
    pub id: i64,
    pub number: Option<String>,
    pub build_type_id: Option<String>,
    /// `queued` | `running` | `finished`.
    pub state: Option<String>,
    /// `SUCCESS` | `FAILURE` | `ERROR` | `UNKNOWN`.
    pub status: Option<String>,
    pub status_text: Option<String>,
    pub branch_name: Option<String>,
    /// Whether `branch_name` is the configuration's default branch. Asked for
    /// only by the view's recent builds (#669).
    pub default_branch: Option<bool>,
    pub queued_date: Option<String>,
    pub start_date: Option<String>,
    pub finish_date: Option<String>,
    pub build_type: Option<BuildType>,
    /// Present while a build runs; `currentStageText` is the one line that
    /// says what it is doing right now.
    #[serde(rename = "running-info")]
    pub running_info: Option<RunningInfo>,
    /// Who or what started the build. The only place TeamCity names the
    /// person, and so the only source of `SyncItem::author` for a build.
    pub triggered: Option<Triggered>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunningInfo {
    pub current_stage_text: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Triggered {
    pub user: Option<User>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct User {
    pub username: Option<String>,
}

/// TeamCity stamps `yyyyMMdd'T'HHmmssZ` (`20260822T101018+0000`), not RFC
/// 3339 -- feeding those to `DateTime::parse_from_rfc3339` yields `None` for
/// every timestamp the server ever sends, and `updated_at` would silently be
/// null on every item.
pub(crate) fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(raw, "%Y%m%dT%H%M%S%z")
        .or_else(|_| DateTime::parse_from_rfc3339(raw))
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Which builds a query asks for -- and, via [`StateFilter::matches`], which
/// records a run counts as finished.
///
/// There is no single-`Queued` or single-`Running` variant on purpose: this
/// adapter never wants one without the other, and the pair has to travel as
/// **one** locator. `state` is a single locator dimension that takes a nested
/// boolean form for a combination; repeating the dimension
/// (`state:running,state:queued`) is not a locator TeamCity accepts, and
/// knobas-mockd records it as a violation with a message naming the right
/// form. Making the wrong form unrepresentable is cheaper than catching it in
/// review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateFilter {
    Finished,
    /// Queued **and** running, in one query.
    InFlight,
}

impl StateFilter {
    fn as_str(self) -> &'static str {
        match self {
            StateFilter::Finished => "finished",
            StateFilter::InFlight => "(queued:true,running:true)",
        }
    }

    /// Does a build's `state` field fall inside this filter?
    ///
    /// The same question the server answered when it selected the page, asked
    /// again locally: the in-flight poll is global, so its results are
    /// re-classified here alongside the scope filter.
    pub(crate) fn matches(self, state: Option<&str>) -> bool {
        match self {
            StateFilter::Finished => state == Some("finished"),
            StateFilter::InFlight => matches!(state, Some("queued" | "running")),
        }
    }
}

/// A `/app/rest/builds` locator, restricted to the dimensions the contract
/// defines: `buildType:`, `state:`, `sinceBuild:`, `canceled:`,
/// `failedToStart:`, `branch:`, `defaultFilter:`, `count:`. Anything else -- `project:`,
/// `affectedProject:` -- is recorded as a violation by the mock and must not
/// be sent. Each dimension appears **at most once**; see [`StateFilter`].
#[derive(Debug, Clone, Default)]
pub(crate) struct Locator {
    pub build_type_id: Option<String>,
    pub state: Option<StateFilter>,
    pub since_build_id: Option<i64>,
    /// Send `untilBuild:(id:N)`: builds older than build `N`, which the
    /// Builds frame's older history asks for beyond the sync's window (#851).
    /// TeamCity counts `N` itself in, so the reader drops it.
    pub until_build_id: Option<i64>,
    /// Send `canceled:any`, which puts canceled builds back into a page
    /// TeamCity's default filter would have hidden them from (issue #105).
    ///
    /// **`any`, not `true`.** `canceled:true` asks for canceled builds and
    /// *only* those; `any` says the dimension does not narrow at all, so the
    /// page carries both classes. The default -- the dimension absent -- is
    /// the server's own filter, which is `canceled:false`.
    ///
    /// Deliberately **not** [`Self::default_filter`]`: Some(false)`. Measured
    /// read-only against JetBrains' public instance on 2026-08-29, over one
    /// window: `state:finished,canceled:any,count:100` re-opened its own facet
    /// and nothing else -- one canceled build, no failed-to-start build, no
    /// personal build -- while `state:finished,defaultFilter:false,count:100`
    /// answered the same canceled build *plus* a failed-to-start one, and
    /// disables the personal facet and every facet nobody has enumerated along
    /// with it. Personal builds are the class this adapter deliberately keeps
    /// out, so it names the two facets it wants rather than turning the whole
    /// filter off.
    pub canceled_any: bool,
    /// Send `failedToStart:any`, the same widening for the other class the
    /// default filter hides (issue #105).
    ///
    /// It travels with [`Self::canceled_any`] on both item-producing queries
    /// for one reason: a queued build the in-flight poll already mirrored can
    /// terminate as failed-to-start, and a build the mirror shows as queued
    /// that no later query can ever return is a row that says "queued" for
    /// ever. That is the same staleness the canceled class has, so ruling one
    /// in and the other out would be incoherent.
    pub failed_to_start_any: bool,
    /// Send `branch:default:any`, which puts the builds of every branch back
    /// into a page TeamCity's default filter would have narrowed to the
    /// **default branch** (issue #266).
    ///
    /// The third facet of the default filter, and the one the public-instance
    /// suite could not see: measured on the seeded TeamCity 2026.1.3 in
    /// `testenv` on 2026-09-02, every locator whose state set includes
    /// `finished` -- `state:finished`, `state:any`, no `state:` at all --
    /// answers only the default branch of a branched configuration, so the
    /// fixture's failed build 1187 on `feature/PAY-231-sepa-retry` came back
    /// from **neither** item-producing query and was never mirrored. The
    /// vendored `testenv/specs/teamcity.json` says the same in prose: "When
    /// looking for builds, TeamCity processes only builds for the default
    /// branch. Add the `branch:<any>` dimension to process all builds
    /// instead." Same shape as [`Self::canceled_any`]: `any`, not a name, and
    /// the dimension re-opens its own facet only -- personal builds stay out.
    ///
    /// **Not on the in-flight poll.** Measured the same day: a locator
    /// restricted to `queued`/`running` answered a non-default-branch build
    /// in both states without the dimension, so the poll takes the server's
    /// filter unchanged, as it does for the other two facets.
    pub branch_any: bool,
    /// TeamCity's default filter hides everything that is not a finished,
    /// non-personal, non-canceled, non-failed-to-start build on the default
    /// branch. `Some(false)` turns it off **whole**, which is the only way to
    /// ask a question about every build regardless of state; `None` sends the
    /// dimension not at all and takes the default.
    ///
    /// Distinct from `state`: `state:` names the states wanted and is the
    /// right dimension when the answer is a set of builds to emit.
    /// `defaultFilter:false` widens the population a *stateless* question is
    /// asked over, which is what the run's opening probe needs -- see
    /// [`sync::probe`](crate::sync).
    ///
    /// Distinct from [`Self::canceled_any`] and [`Self::failed_to_start_any`]
    /// too, and only the probe uses this one: those name a single facet each,
    /// this disables all of them at once -- personal builds included, and any
    /// facet a future TeamCity adds. A query whose answer becomes items must
    /// not send it (issue #105).
    pub default_filter: Option<bool>,
    pub count: u32,
}

impl Locator {
    /// Deliberately not `to_string`: an inherent `to_string` shadows `Display`
    /// and clippy rejects it, and this string is a wire format rather than a
    /// human rendering.
    pub(crate) fn render(&self) -> String {
        let mut parts = Vec::new();
        if let Some(id) = &self.build_type_id {
            parts.push(format!("buildType:(id:{id})"));
        }
        if let Some(state) = self.state {
            parts.push(format!("state:{}", state.as_str()));
        }
        if let Some(id) = self.since_build_id {
            parts.push(format!("sinceBuild:(id:{id})"));
        }
        if let Some(id) = self.until_build_id {
            parts.push(format!("untilBuild:(id:{id})"));
        }
        if self.canceled_any {
            parts.push("canceled:any".to_owned());
        }
        if self.failed_to_start_any {
            parts.push("failedToStart:any".to_owned());
        }
        if self.branch_any {
            parts.push("branch:default:any".to_owned());
        }
        if let Some(on) = self.default_filter {
            parts.push(format!("defaultFilter:{on}"));
        }
        parts.push(format!("count:{}", self.count));
        parts.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `/app/rest/buildTypes` answer, shaped the way a real server sends it
    /// -- richer than [`BUILD_TYPE_FIELDS`] asks for, because the parse must
    /// survive a server that answers with more than it was asked.
    fn build_types_json() -> serde_json::Value {
        serde_json::json!({
            "count": 2,
            "href": "/app/rest/buildTypes",
            "buildType": [
                { "id": "Payout_IntegrationTests", "name": "Integration Tests",
                  "projectId": "Payout", "projectName": "Payout",
                  "description": "Runs the SEPA suite",
                  "webUrl": "https://ci.example.com/buildConfiguration/Payout_IntegrationTests",
                  "paused": false },
                { "id": "Ledger_Deploy_Staging", "name": "Deploy Staging",
                  "projectId": "Ledger", "projectName": "Ledger" }
            ]
        })
    }

    /// A finished build and a running one.
    fn builds_json() -> serde_json::Value {
        serde_json::json!({
            "count": 2,
            "build": [
                { "id": 1188, "number": "1188", "buildTypeId": "Payout_Build",
                  "state": "running", "status": "SUCCESS", "statusText": "Running",
                  "branchName": "feature/PAY-231-sepa-retry", "percentageComplete": 60,
                  "webUrl": "https://ci.example.com/build/1188",
                  "queuedDate": "20260822T114000+0000", "startDate": "20260822T114500+0000",
                  "running-info": { "percentageComplete": 60, "currentStageText": "step 3/5 `cargo test`" },
                  "buildType": { "id": "Payout_Build", "name": "Build",
                                 "projectId": "Payout", "projectName": "Payout" },
                  "triggered": { "type": "user", "date": "20260822T114000+0000",
                                 "user": { "username": "mara.lindqvist", "name": "Mara Lindqvist" } } },
                { "id": 1187, "number": "1187", "buildTypeId": "Payout_IntegrationTests",
                  "state": "finished", "status": "FAILURE",
                  "statusText": "Tests failed: 1 (1 new), passed: 41",
                  "branchName": "feature/PAY-231-sepa-retry",
                  "webUrl": "https://ci.example.com/build/1187",
                  "queuedDate": "20260822T100500+0000", "startDate": "20260822T100600+0000",
                  "finishDate": "20260822T101018+0000",
                  "buildType": { "id": "Payout_IntegrationTests", "name": "Integration Tests",
                                 "projectId": "Payout", "projectName": "Payout" } }
            ]
        })
    }

    #[test]
    fn a_build_types_listing_parses() {
        let env: ListEnvelope = serde_json::from_value(build_types_json()).expect("envelope");
        assert_eq!(env.items.len(), 2);
        let bt: BuildType = serde_json::from_value(env.items[0].clone()).expect("buildType");
        assert_eq!(bt.id, "Payout_IntegrationTests");
        assert_eq!(bt.name.as_deref(), Some("Integration Tests"));
        assert_eq!(bt.project_id.as_deref(), Some("Payout"));
        assert_eq!(bt.description.as_deref(), Some("Runs the SEPA suite"));
        // The record still *carries* a `webUrl` -- this fixture is a page from
        // before issue #516 dropped the name from `BUILD_TYPE_FIELDS` -- and
        // the parse drops it on the floor rather than erroring, which is what
        // makes the drop safe for a mirror still holding older raw pages.
        assert!(
            env.items[0]["webUrl"].is_string(),
            "the fixture page names a URL, so the parse above is what dropped it"
        );
        // Everything but the id is optional: a trimmed or older server omits
        // fields rather than sending nulls, and a missing description must not
        // fail the whole run.
        let bare: BuildType =
            serde_json::from_value(env.items[1].clone()).expect("sparse buildType");
        assert_eq!(bare.description, None);
    }

    #[test]
    fn a_builds_listing_parses_running_and_finished() {
        let env: ListEnvelope = serde_json::from_value(builds_json()).expect("envelope");
        let running: Build = serde_json::from_value(env.items[0].clone()).expect("running");
        assert_eq!(running.id, 1188);
        assert_eq!(running.state.as_deref(), Some("running"));
        assert_eq!(running.finish_date, None);
        // The hyphen makes this the one field name serde cannot derive.
        assert_eq!(
            running
                .running_info
                .as_ref()
                .and_then(|r| r.current_stage_text.as_deref()),
            Some("step 3/5 `cargo test`")
        );
        assert_eq!(
            running
                .triggered
                .and_then(|t| t.user)
                .and_then(|u| u.username)
                .as_deref(),
            Some("mara.lindqvist")
        );
        let finished: Build = serde_json::from_value(env.items[1].clone()).expect("finished");
        assert_eq!(finished.id, 1187);
        assert_eq!(finished.status.as_deref(), Some("FAILURE"));
        assert!(
            finished.running_info.is_none(),
            "a finished build has no running-info"
        );
        assert_eq!(
            finished.build_type.expect("nested").project_name.as_deref(),
            Some("Payout")
        );
    }

    /// A field this adapter does not know about must not fail the parse: the
    /// server is free to send more than `fields=` asked for, and a future
    /// TeamCity will.
    #[test]
    fn unknown_fields_are_ignored() {
        let b: Build = serde_json::from_value(serde_json::json!({
            "id": 7, "state": "finished", "somethingNew": { "x": 1 }
        }))
        .expect("forward compatible");
        assert_eq!(b.id, 7);
    }

    /// TeamCity does not speak RFC 3339. This is the format it does speak.
    #[test]
    fn teamcity_timestamps_parse() {
        let t = parse_ts("20260822T101018+0000").expect("teamcity format");
        assert_eq!(t.to_rfc3339(), "2026-08-22T10:10:18+00:00");
        let t = parse_ts("20260822T121018+0200").expect("non-utc offset");
        assert_eq!(
            t.to_rfc3339(),
            "2026-08-22T10:10:18+00:00",
            "the offset is applied, not ignored"
        );
        // Tolerated, because a proxy or a future version may normalise it.
        assert!(parse_ts("2026-08-22T10:10:18Z").is_some());
        assert!(parse_ts("").is_none());
        assert!(parse_ts("yesterday").is_none());
        // A date with no zone at all: TeamCity always sends one, and guessing
        // UTC for a server on +02:00 would be two hours of silent skew.
        assert!(parse_ts("20260822T101018").is_none());
    }

    /// The locator is a wire string; its shape is the contract with mockd's
    /// locator grammar, so it is asserted literally.
    #[test]
    fn locators_render_in_a_fixed_order() {
        assert_eq!(
            Locator {
                build_type_id: Some("Payout_Build".to_owned()),
                state: Some(StateFilter::Finished),
                count: 100,
                ..Locator::default()
            }
            .render(),
            "buildType:(id:Payout_Build),state:finished,count:100"
        );
        assert_eq!(
            Locator {
                state: Some(StateFilter::Finished),
                since_build_id: Some(412),
                count: 100,
                ..Locator::default()
            }
            .render(),
            "state:finished,sinceBuild:(id:412),count:100"
        );
        // Queued and running in ONE query. `state` is a single locator
        // dimension with a nested boolean form for combinations; repeating it
        // (`state:running,state:queued`) is not a locator TeamCity accepts,
        // and knobas-mockd records it as a violation.
        assert_eq!(
            Locator {
                state: Some(StateFilter::InFlight),
                count: 100,
                ..Locator::default()
            }
            .render(),
            "state:(queued:true,running:true),count:100"
        );
        assert_eq!(
            Locator {
                state: Some(StateFilter::InFlight),
                count: 50,
                ..Locator::default()
            }
            .render(),
            "state:(queued:true,running:true),count:50"
        );
        // The spelling the interfaces doc printed before stream T corrected it
        // -- pinned so it cannot creep back in.
        for locator in [
            Locator {
                state: Some(StateFilter::InFlight),
                count: 100,
                ..Locator::default()
            },
            Locator {
                state: Some(StateFilter::Finished),
                count: 100,
                ..Locator::default()
            },
        ] {
            let rendered = locator.render();
            assert!(!rendered.contains("state:running"), "{rendered}");
            assert!(!rendered.contains("state:queued"), "{rendered}");
        }
        // The two **item-producing** locators, and the one difference issue
        // #105 makes to them: `canceled:any,failedToStart:any` re-opens the
        // two facets TeamCity's default filter closes, so a canceled or
        // failed-to-start build reaches the mirror instead of being hidden
        // from every query that emits an item.
        // ...and the third facet the same filter closes, measured on a
        // TeamCity we own (issue #266): every locator whose state set includes
        // `finished` answers only the **default branch** of a branched
        // configuration, and `branch:default:any` is the dimension that
        // re-opens exactly that facet. The two item-producing locators carry
        // all three.
        assert_eq!(
            Locator {
                build_type_id: Some("Payout_Build".to_owned()),
                state: Some(StateFilter::Finished),
                canceled_any: true,
                failed_to_start_any: true,
                branch_any: true,
                count: 100,
                ..Locator::default()
            }
            .render(),
            "buildType:(id:Payout_Build),state:finished,canceled:any,failedToStart:any,\
             branch:default:any,count:100"
        );
        assert_eq!(
            Locator {
                state: Some(StateFilter::Finished),
                since_build_id: Some(412),
                canceled_any: true,
                failed_to_start_any: true,
                branch_any: true,
                count: 100,
                ..Locator::default()
            }
            .render(),
            "state:finished,sinceBuild:(id:412),canceled:any,failedToStart:any,\
             branch:default:any,count:100"
        );
        // The branch dimension is independent of the other two, so it renders
        // alone -- which is what makes dropping it visible in the wire string.
        assert_eq!(
            Locator {
                state: Some(StateFilter::Finished),
                branch_any: true,
                count: 100,
                ..Locator::default()
            }
            .render(),
            "state:finished,branch:default:any,count:100"
        );
        // `any` and not `true`: the dimension re-**includes** its class
        // alongside the ordinary builds, where `canceled:true` would return
        // canceled builds and nothing else. And the two are independent
        // dimensions, so one without the other renders alone -- which is what
        // makes a mutation that drops either one visible in the wire string.
        assert_eq!(
            Locator {
                state: Some(StateFilter::Finished),
                canceled_any: true,
                count: 100,
                ..Locator::default()
            }
            .render(),
            "state:finished,canceled:any,count:100"
        );
        assert_eq!(
            Locator {
                state: Some(StateFilter::Finished),
                failed_to_start_any: true,
                count: 100,
                ..Locator::default()
            }
            .render(),
            "state:finished,failedToStart:any,count:100"
        );
        // Neither is sent by default: `Locator::default()` takes the server's
        // own filter, which is what the in-flight poll wants.
        for absent in ["canceled:", "failedToStart:", "branch:"] {
            assert!(
                !Locator {
                    state: Some(StateFilter::InFlight),
                    count: 100,
                    ..Locator::default()
                }
                .render()
                .contains(absent),
                "{absent} must not be sent unless a locator asks for it"
            );
        }
        // The run's opening probe: no `state`, the default filter explicitly
        // off, one page. `state` would answer a different question -- see the
        // field's doc -- and the page is read for its maximum id rather than
        // its first row, which is why its width is the whole page
        // (`sync::probe`, `sync::ceiling`).
        assert_eq!(
            Locator {
                default_filter: Some(false),
                count: 100,
                ..Locator::default()
            }
            .render(),
            "defaultFilter:false,count:100"
        );
        // No dimension may repeat: the mock rejects a locator that names one
        // twice, whichever one it is.
        let rendered = Locator {
            build_type_id: Some("Payout_Build".to_owned()),
            state: Some(StateFilter::InFlight),
            since_build_id: Some(9),
            until_build_id: Some(7),
            canceled_any: true,
            failed_to_start_any: true,
            branch_any: true,
            default_filter: Some(false),
            count: 100,
        }
        .render();
        assert!(rendered.contains("untilBuild:(id:7)"), "{rendered}");
        for dimension in [
            "buildType:",
            "state:",
            "sinceBuild:",
            "untilBuild:",
            "canceled:",
            "failedToStart:",
            "branch:",
            "defaultFilter:",
            "count:",
        ] {
            assert_eq!(
                rendered.matches(dimension).count(),
                1,
                "{dimension} appears more than once in {rendered}"
            );
        }
    }

    /// The filter has to answer the same question about a record that the
    /// server answered about the query -- `sync` classifies finished from
    /// in-flight builds with it.
    #[test]
    fn a_state_filter_matches_the_records_the_server_would_return() {
        assert!(StateFilter::Finished.matches(Some("finished")));
        assert!(!StateFilter::Finished.matches(Some("running")));
        assert!(!StateFilter::Finished.matches(Some("queued")));
        assert!(StateFilter::InFlight.matches(Some("running")));
        assert!(StateFilter::InFlight.matches(Some("queued")));
        assert!(!StateFilter::InFlight.matches(Some("finished")));
        // A record with no `state` at all belongs to neither: guessing would
        // move a watermark on a build nobody can classify.
        assert!(!StateFilter::Finished.matches(None));
        assert!(!StateFilter::InFlight.matches(None));
        // Nor does an unknown one.
        assert!(!StateFilter::Finished.matches(Some("deleted")));
        assert!(!StateFilter::InFlight.matches(Some("deleted")));
    }

    /// `fields=` is mandatory on every request, and what it asks for is what
    /// the mapping reads.
    ///
    /// This is a *representation* check and it cannot see the thing that
    /// actually matters -- whether the server recognises the names. That is
    /// `tests/it/mockd.rs`'s job: mockd answers an unknown name with 400 and a
    /// recorded violation, so a selector that drifts from the contract fails
    /// there, loudly, rather than here.
    #[test]
    fn the_field_selectors_cover_everything_the_mapping_reads() {
        for needed in [
            "id",
            "number",
            "state",
            "status",
            "statusText",
            "branchName",
            "finishDate",
            "startDate",
            "queuedDate",
            "buildType(",
            "running-info(",
            // The declared `trigger` and `started_by` (#961).
            "triggered(type,user(username))",
            // The declared `agent` and `current_step` (#1004).
            "agent(name)",
            "running-info(percentageComplete,currentStageText)",
            // The declared test counts (#1448).
            "testOccurrences(failed,passed,ignored)",
            // The declared `revision` (#710): without it every build's
            // revision is a miss, and no commit's detail draws a build.
            "revisions(revision(version))",
        ] {
            assert!(
                BUILD_FIELDS.contains(needed),
                "BUILD_FIELDS misses {needed}"
            );
        }
        for needed in ["id", "name", "projectId", "projectName", "description"] {
            assert!(
                BUILD_TYPE_FIELDS.contains(needed),
                "BUILD_TYPE_FIELDS misses {needed}, so `map::build_config_item` reads it as None \
                 on every configuration"
            );
        }
        // The tree (#849): `crate::projects` reads all three.
        for needed in ["project(", "id", "name", "parentProjectId"] {
            assert!(
                PROJECT_FIELDS.contains(needed),
                "PROJECT_FIELDS misses {needed}"
            );
        }
        assert!(SERVER_FIELDS.contains("version"));
        assert!(USER_FIELDS.contains("username"));
        // Every collection selector must ask for `count` and `nextHref` too,
        // or the envelope the parse reads back is not the one the server sent.
        //
        // `nextHref` is the one with teeth (issue #114). A real TeamCity
        // serves it **only** when `fields=` names it -- `count,build(id)`
        // comes back with no `nextHref` key whatever the server still has,
        // measured on JetBrains' public instance 2026-08-29 -- and it is the
        // signal `sync::last_page` ends a walk on. Dropping it from a
        // selector would put every walk back on the page-length assumption
        // alone, silently, with the suite still green because the fakes
        // answer the field either way.
        for f in [BUILD_FIELDS, BUILD_TYPE_FIELDS, PROJECT_FIELDS] {
            assert!(
                f.starts_with("count,nextHref,"),
                "{f:?} must select the envelope, `nextHref` included: without it the server \
                 never sends one and `sync::last_page` loses the only signal that tells a \
                 capped page from an exhausted query"
            );
        }
        // The two source-view selectors (#664). `paused`, a parameter's
        // `inherited` and its `type(rawValue)` are what the overview's
        // warning, the inherited mark and the password mask are read from.
        for needed in [
            "paused",
            "inherited",
            "type(rawValue)",
            "projectName",
            "checkout-rules",
        ] {
            assert!(
                BUILD_CONFIG_VIEW_FIELDS.contains(needed),
                "BUILD_CONFIG_VIEW_FIELDS misses {needed}"
            );
        }
        for needed in [
            "number",
            "state",
            "status",
            "statusText",
            "branchName",
            "startDate",
            "finishDate",
        ] {
            assert!(
                RECENT_BUILD_FIELDS.contains(needed),
                "RECENT_BUILD_FIELDS misses {needed}"
            );
        }
        // The build view (#665): what `view::build_overview`,
        // `view::build_parameters` and the *Tests* stub read.
        for needed in [
            "number",
            "buildTypeId",
            "statusText",
            "branchName",
            "queuedDate",
            "startDate",
            "finishDate",
            "projectName",
            "parameters(property(name,type(rawValue)))",
            "triggered(type,user(username))",
            "agent(name)",
            "running-info(currentStageText)",
            "testOccurrences(failed)",
            "properties(property(name,value))",
        ] {
            assert!(
                BUILD_VIEW_FIELDS.contains(needed),
                "BUILD_VIEW_FIELDS misses {needed}"
            );
        }
        for needed in ["version", "username", "date", "comment", "vcsRootInstance"] {
            assert!(
                BUILD_CHANGE_FIELDS.contains(needed),
                "BUILD_CHANGE_FIELDS misses {needed}"
            );
        }
        for needed in ["name", "details", "newFailure", "muted"] {
            assert!(
                FAILED_TEST_FIELDS.contains(needed),
                "FAILED_TEST_FIELDS misses {needed}"
            );
        }
        for needed in ["name", "size", "children(count)"] {
            assert!(
                ARTIFACT_FIELDS.contains(needed),
                "ARTIFACT_FIELDS misses {needed}"
            );
        }
        // No whitespace: these go into a query string verbatim.
        for f in [
            BUILD_FIELDS,
            BUILD_RECORD_FIELDS,
            BUILD_TYPE_FIELDS,
            SERVER_FIELDS,
            USER_FIELDS,
            BUILD_CONFIG_VIEW_FIELDS,
            RECENT_BUILD_FIELDS,
            BUILD_VIEW_FIELDS,
            BUILD_CHANGE_FIELDS,
            FAILED_TEST_FIELDS,
            ARTIFACT_FIELDS,
        ] {
            assert!(!f.contains(' '), "{f:?} must not contain spaces");
            assert_eq!(
                f.matches('(').count(),
                f.matches(')').count(),
                "{f:?} is unbalanced"
            );
        }
    }

    /// The other direction: a selector must not ask for a name **no** reader
    /// in `map` looks at.
    ///
    /// Paired name-with-selector rather than iterated as a cross-product,
    /// because the reason a name is left out is a property of the *pair* and
    /// not of the name. `paused` is a real field on a build configuration and
    /// is not a field on a build at all, so `BUILD_TYPE_FIELDS` omitting it is
    /// budget and `BUILD_FIELDS` omitting it is the mock contract -- mockd
    /// answers `build(paused)` with 400 + an `UnknownField` violation, exactly
    /// as it answers a name nobody has ever heard of. A cross-product cannot
    /// say two different things about two cells, so it said the wrong one
    /// about that one, twice. Each row below carries its own reason and its
    /// own consequence.
    ///
    /// **The fix is never "delete the name from the selector" when a reader is
    /// what is missing.** `description` used to be on this list, and it was
    /// the *list* that was wrong: `map::build_config_item` has always put
    /// `bt.description` in the search blob, so the selector omitting it meant
    /// a configuration's prose silently never reached the index. It is asked
    /// for now.
    #[test]
    fn the_selectors_ask_for_nothing_no_reader_looks_at() {
        // (selector, its name, the unasked name, what asking would cost)
        let unread = [
            (
                BUILD_FIELDS,
                "BUILD_FIELDS",
                "webUrl",
                "a build's URL is composed from the configured base URL since issue #495, so \
                 `struct Build` no longer parses the server's own and no reader in `map` looks \
                 at it; the nested `buildType(webUrl)` was never read either, and a \
                 configuration's URL comes from BUILD_TYPE_FIELDS",
            ),
            (
                BUILD_FIELDS,
                "BUILD_FIELDS",
                "href",
                "mockd serves `build(href)`, so this is not a violation -- it is response size \
                 and a payload key nothing in `map` reads",
            ),
            (
                BUILD_TYPE_FIELDS,
                "BUILD_TYPE_FIELDS",
                "webUrl",
                "a build configuration's URL is composed from the configured base URL since \
                 issue #516 -- `<base>/buildConfiguration/<id>?mode=builds` -- so `struct \
                 BuildType` no longer parses the server's own and no reader in `map` looks at \
                 it; TeamCity fills that field in from the server's *Server URL* setting, which \
                 names whatever host the server thinks it is rather than the host this source \
                 is configured at",
            ),
            (
                BUILD_TYPE_FIELDS,
                "BUILD_TYPE_FIELDS",
                "href",
                "mockd serves `buildType(href)`, so this is not a violation -- it is response \
                 size and a payload key nothing in `map` reads",
            ),
            (
                BUILD_TYPE_FIELDS,
                "BUILD_TYPE_FIELDS",
                "paused",
                "mockd serves `buildType(paused)`, so this is not a violation -- it is response \
                 size and a payload key nothing in `map` reads",
            ),
        ];
        for (selector, name, missing, why) in unread {
            assert!(
                !selector.contains(missing),
                "{name} asks for {missing}: {why}. If you added a reader for it, add the name \
                 here too; if you did not, drop it."
            );
        }
        // ...and the one that is *not* budget: `paused` is not a field on a
        // build, so `BUILD_FIELDS` asking for it is a 400 and a recorded
        // violation, and `tests/it/mockd.rs` would fail on
        // `assert_no_violations()`. Adding a reader would not make this legal,
        // which is the opposite of the advice above -- hence its own case.
        assert!(
            !BUILD_FIELDS.contains("paused"),
            "BUILD_FIELDS asks for `paused`, which is not a field on a build: mockd answers 400 \
             + an UnknownField violation, the same as a misspelling. Adding a reader would not \
             help; the name does not exist on this type."
        );
        // `triggered` is asked for at exactly the depth its readers read it,
        // and no deeper: `map::build_item` and the declared `started_by` read
        // the username, the declared `trigger` the type (#961). mockd would
        // serve `triggered(type,date,user(username,name))` without complaint.
        assert!(
            BUILD_FIELDS.contains("triggered(type,user(username))"),
            "BUILD_FIELDS must ask for the trigger and the triggerer, or every build's author \
             and trigger is None"
        );
        for deeper in ["triggered(type,date", "user(username,"] {
            assert!(
                !BUILD_FIELDS.contains(deeper),
                "BUILD_FIELDS asks for {deeper}; nothing reads below triggered.type and \
                 triggered.user.username"
            );
        }
        assert!(
            !BUILD_TYPE_FIELDS.contains("triggered"),
            "a build configuration has no triggerer"
        );
        // A name the selector asks for and no struct field parses is the same
        // waste one step earlier: it cannot reach a reader at all. Top-level
        // `percentageComplete` was exactly that -- asked for, never parsed
        // (`running-info(percentageComplete)` is the one the mapping reads).
        assert!(
            !BUILD_FIELDS.contains(",percentageComplete"),
            "BUILD_FIELDS asks for a top-level percentageComplete, which `struct Build` does \
             not parse; the mapping reads running-info(percentageComplete)"
        );
        // ...and no preset: mockd honours only `$long`, and asking for the
        // whole subtree defeats the point of a selector.
        for f in [
            BUILD_FIELDS,
            BUILD_RECORD_FIELDS,
            BUILD_TYPE_FIELDS,
            SERVER_FIELDS,
            USER_FIELDS,
        ] {
            assert!(!f.contains('$'), "{f:?} uses a fields= preset");
        }
    }
}
