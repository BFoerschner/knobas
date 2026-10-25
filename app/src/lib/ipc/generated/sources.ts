// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).
import type { KindInfo, Tone } from "./entity";

/**
 * The limits every call into the adapter is held to (#1204): what the
 * manifest asks for, each absent one at the host's default.
 */
export type AdapterLimits = { 
/**
 * The deadline of one call, in seconds.
 */
deadline_secs: number, 
/**
 * Whether the manifest asks for the deadline; `false` is the default.
 */
deadline_asked: boolean, 
/**
 * The memory cap of one call, in MiB.
 */
memory_mib: number, 
/**
 * Whether the manifest asks for the memory cap; `false` is the default.
 */
memory_asked: boolean, };

/**
 * What the reader is shown before an install or an update (#2031): every
 * check has passed and nothing is written. `confirm_sha256` is what the
 * confirm sends back, so what installs is what was shown.
 */
export type AdapterPreview = { adapter_kind: string, 
/**
 * The descriptor's name, or the kind for an installed adapter refused at load.
 */
name: string, adapter_version: string, interface_version: string, source_url: string, 
/**
 * The release's tag; `None` for a local manifest, which is no release.
 */
tag: string | null, artifact_sha256: string, 
/**
 * What the confirm sends back: [`confirm_sha256`] of this release.
 */
confirm_sha256: string, 
/**
 * The hosts it may reach, as the manifest writes them (`{base_url}`).
 */
hosts: Array<string>, 
/**
 * The roles its descriptor claims; empty for an installed adapter
 * refused at load, whose descriptor was never read.
 */
roles: Array<RoleClaim>, limits: AdapterLimits, };

/**
 * A source of an installed adapter, for uninstall's question.
 */
export type AdapterSource = { id: string, display_name: string, };

/**
 * One installed external adapter, as Settings lists it (#1209, #1210): its
 * pin, its sources, why it was refused at load or restore, and the last
 * update check.
 */
export type AdapterStatus = { adapter_kind: string, 
/**
 * The descriptor's name, or the kind for an adapter refused at load.
 */
name: string, adapter_version: string, interface_version: string, source_url: string, tag: string | null, artifact_sha256: string, hosts: Array<string>, sources: Array<AdapterSource>, 
/**
 * Why the adapter was refused at load or restore; `None` when it runs.
 */
refused: string | null, 
/**
 * The last update check; `None` before one ran.
 */
update: UpdateCheck | null, };

/**
 * How an adapter authenticates against its remote system (spec §3).
 *
 * The descriptor declares which of these it accepts; the secret itself is
 * stored in the OS keychain, never in the source's configuration. Spelled in
 * PascalCase on the wire, unchanged since M0.
 */
export type AuthMethod = "UserPassword" | "Pat" | "ApiToken" | "OAuth";

/**
 * What the last attempt to use a source's credential established.
 *
 * - `ok`: the credential worked.
 * - `unauthorized`: 401/403, the credential is wrong, expired, or locked
 *   out. **No automatic retry** -- a human must act (interfaces §8 P7).
 *   The one state the UI must act on: it is what puts *Re-enter
 *   password* on the row rather than a shrug.
 * - `unreachable`: the system could not be reached. Retried with
 *   backoff.
 * - `missing_secret`: the source is configured but the keychain has no
 *   secret for it, so the scheduler skips it entirely rather than
 *   churning backoff on a request it knows will fail (§3). A different
 *   prompt from a secret that was rejected.
 * - `unknown`: never tested, or not tested since something changed.
 *
 * The wire spellings are the same five literals
 * `source_config_auth_state_chk` allows -- one list, two places, pinned
 * by tests on both sides.
 *
 * The states are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type AuthState = "ok" | "unauthorized" | "unreachable" | "missing_secret" | "unknown";

/**
 * One parameter of a [`RunBuild`](WriteOp::RunBuild) (#669): its full name
 * (`env.SEPA_RETRIES`) and the value this run takes for it. A struct rather
 * than a pair so the wire reads `{"name": .., "value": ..}`, the shape a
 * source's own parameter lists have.
 */
export type BuildParameter = { name: string, value: string, };

/**
 * Which typed address of an instance a repository is cloned from: the
 * answer an adapter gives to the code-host read `clone_urls` (#1197).
 *
 * Plain serde data, as everything crossing the SPI: `"base_url"`, or
 * `{"config": "<key>"}`.
 *
 * - `base_url`: the instance's base URL.
 * - `{"config": <key>}`: the string the instance's config holds under this
 *   key, which the adapter's `config_schema` declares as a string property
 *   ([`config_declares`]).
 *
 * The addresses are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type CloneFrom = "base_url" | { "config": string };

/**
 * What *Test connection* found.
 */
export type ConnectionReport = { ok: boolean, account: string | null, server_version: string | null, secret_expires_at: string | null, 
/**
 * One line for the form, or `None` when it connected.
 *
 * Interfaces §2.2 spells this `Option<SourceError>`. A `String` plus
 * [`code`](Self::code) instead, and it is the **one** declared deviation
 * in this shape: serializing `SourceError` puts an untagged Rust enum on
 * the bridge for the frontend to pattern-match, and P1 says branch on a
 * code.
 */
error: string | null, 
/**
 * The adapter's **connection note** (`CONTEXT.md`): the one line it says
 * about the far end that nothing else on this report already says --
 * Jira's Epic Link clause is the first. Copied through from
 * [`knobas_source::ConnectionInfo::detail`] on a successful test and
 * `None` on a failed one, where [`error`](Self::error) is the line.
 *
 * It belongs to the moment of *Test connection*: the Add-source dialog
 * and a saved source's row show it beside the test result, and nothing
 * stores it. It is not credential health, which a good sync may rewrite
 * -- a note is true of the far end as just found, not of the credential
 * (#326, §10.8).
 */
detail: string | null, 
/**
 * The class the UI branches on -- `unauthorized` is what turns *Test*
 * into *Re-enter*, and a message is not something to branch on.
 */
code: IpcErrorCode | null, elapsed_ms: number, 
/**
 * Config values the adapter learned from the far end
 * ([`knobas_source::ConnectionInfo::discovered`]), keyed by the
 * `config_schema` property each belongs in: Jira's Epic Link custom
 * field id, whose value is different on every instance (#297).
 *
 * Carried through verbatim, and **acted on by the dialog, not here**:
 * `AddSource.svelte` puts a value into the matching form field when the
 * reader left it empty, the same way it fills `username` from
 * [`Self::account`] (#82). Nothing on this side writes a config -- *Test
 * connection* writes nothing at all, which is the property this whole
 * command is built around.
 *
 * Empty on a failed test: an adapter that could not connect learned
 * nothing, and a stale map would fill the form with another server's ids.
 * Empty too for an adapter that discovers nothing.
 */
discovered: { [key in string]: string }, };

/**
 * One source's credential health -- the cheap poll the top strip's sync
 * monograms render from, the payload of the `source:health` event, and the
 * answer of `credential_health`.
 */
export type CredentialHealth = { source_id: string, state: AuthState, 
/**
 * When the credential was last checked; `None` if it never has been.
 */
checked_at: string | null, 
/**
 * One line of detail for the sources view -- the server's own message
 * where there is one. Never a secret.
 */
detail: string | null, 
/**
 * When the credential expires, if the source says. Feeds §3's credential
 * expiry countdown.
 */
secret_expires_at: string | null, };

/**
 * The diagnostics view's numbers.
 */
export type DbStats = { db_bytes: number, entity_count: number, item_count: number, per_source: Array<SourceCount>, oldest_synced_at: string | null, newest_synced_at: string | null, };

/**
 * Which part of a shell-owned frame a kind is: the Builds frame (ADR-0022,
 * #848), or a ticket system's saved views (ADR-0029, #1207).
 *
 * - `pipeline`: what a CI source runs: a TeamCity build configuration.
 * - `run`: one execution of a pipeline: a TeamCity build.
 * - `saved_view`: a view a ticket system keeps of its own tickets (#1207): a
 *   Jira board or filter. Each item of the kind becomes a ticket view,
 *   whichever adapter emits it; `knobas_sync::saved_views` reads its payload.
 *   The kind must declare where a view's query is (`source_query`, the
 *   ticket-system minimum, #2050) and may declare its `board_type`,
 *   `sub_query` and `sprint`; those are read there and at no key of the
 *   frame's choosing. The frame's own keys are fixed: `kind` (`board` or
 *   `filter`), `id`, `columns` (`[{"name", "statuses", "limit"}]`), `keys`
 *   (the tickets' keys in the source's order) and `query`.
 *
 * The parts are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type Frame = "pipeline" | "run" | "saved_view";

/**
 * What an install recorded (#1202, ADR-0028), and the adapter's descriptor
 * template: what the Add-source dialog chooses once the install answers, and
 * what `list_adapters` answers from then on.
 */
export type InstalledAdapter = { adapter_kind: string, adapter_version: string, interface_version: string, source_url: string, 
/**
 * The release's tag; `None` for a local manifest, which is no release.
 */
tag: string | null, artifact_sha256: string, hosts: Array<string>, template: SourceDescriptor, };

/**
 * The failure classes the frontend branches on.
 *
 * - `unauthorized`: the remote system refused the credential (401/403). A
 *   human must act.
 * - `unreachable`: the remote system could not be reached: connect, DNS, TLS,
 *   timeout.
 * - `not_found`: the thing addressed does not exist.
 * - `conflict`: the write lost a race, or a uniqueness rule rejected it.
 * - `invalid`: the arguments cannot be honoured -- a malformed id, an
 *   unsupported combination, a mode the app is not in.
 * - `not_ready`: the app is not there yet: the database is still starting, or
 *   the source has never been configured. Retrying later is the right move.
 * - `internal`: everything else. Nothing downstream can do anything but show
 *   it.
 *
 * The codes are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type IpcErrorCode = "unauthorized" | "unreachable" | "not_found" | "conflict" | "invalid" | "not_ready" | "internal";

/**
 * What one adapter's records of one entity kind carry, and where.
 *
 * **Per kind**, because the answer is: a TeamCity *build* names its project
 * one level down on the `buildType` it ran, while a build *configuration*'s
 * record **is** that `buildType` object and names it at the top level. Before
 * this that difference was a `case when i.kind = 'build_config'` guard in
 * knobas' own SQL (#232); now it is two entries in the adapter's own
 * declaration, which is where the knowledge belongs.
 *
 * Every path field is a list of *candidates*, most specific first: one
 * adapter's alternative spellings of its own field, never knobas guessing.
 *
 * Every field defaults to empty and empty means **the adapter says nothing
 * about it**, which is a miss and never a guess. `#[serde(default)]`
 * throughout, so a descriptor serialized by an older peer -- an out-of-process
 * adapter built before this grew -- still decodes, as an adapter that declares
 * nothing.
 *
 * The field *names* are load-bearing beyond Rust: they are the keys the SQL
 * macros look the declaration up by, as string literals. `the_field_names_the_sql_macros_use_are_the_serialized_ones`
 * pins the two together, so a rename here turns a test red instead of turning
 * every path-driven read into a silent miss.
 */
export type KindPaths = { 
/**
 * Which entity kind this describes -- one of the descriptor's
 * `entity_kinds`, which the contract battery enforces.
 */
kind: string, 
/**
 * The status a ticket stands in, in the source's own spelling
 * (`fields.status.name`). What the mini board's columns are.
 */
status_name: Array<PayloadPath>, 
/**
 * The priority a card shows (`fields.priority.name`).
 */
priority: Array<PayloadPath>, 
/**
 * The account an item is assigned to, spelled the way the source spells
 * an account elsewhere -- the inbox matches it against the configured
 * usernames, so a display name or an internal id declared here would be a
 * rule that never fires.
 */
assignee: Array<PayloadPath>, 
/**
 * The accounts a review has been requested from.
 */
reviewers: Array<ListPath>, 
/**
 * Whether a pull request has been merged. A **boolean** in the payload:
 * a timestamp that is present when merged is a different fact with a
 * different absence, and an adapter that has one declares no merged flag
 * rather than pointing this at it.
 */
merged: Array<PayloadPath>, 
/**
 * The key of the project this item belongs to (ADR-0010), which is what a
 * project room is scoped by.
 */
project_key: Array<PayloadPath>, 
/**
 * The project's display name, for the room's label.
 */
project_name: Array<PayloadPath>, 
/**
 * The chain of projects the item sits in, outermost first, ending in its
 * own (#1019): what lets a project take in its subprojects. TeamCity's
 * `projectPath[]`, `id` and `name`. Empty for a source whose projects do
 * not nest, and a miss leaves the item in its own project only.
 */
project_path: Array<ProjectPath>, 
/**
 * The status names this source considers **blocked-like**, in its own
 * spelling (`Blocked`, `Waiting for support`). Names, not a path: which
 * statuses mean "stuck" is a property of the source's workflow, not of
 * one record. Matched case-insensitively by its reader.
 */
blocked_statuses: Array<string>, 
/**
 * The status names this source considers **done-like**, in its own
 * spelling (`Done`, `Closed`, `Resolved`): the same shape as
 * [`blocked_statuses`](Self::blocked_statuses), and matched the same way,
 * case-insensitively against [`status_name`](Self::status_name). While an
 * item's status is one of them, the job context anchored on it is
 * archived (#1501); a workflow spelling done otherwise never archives,
 * which is the safe miss. Empty for a source with no such status.
 */
done_statuses: Array<string>, 
/**
 * Where this item's parent is and what it answers to (ADR-0018). Empty
 * for a kind with no parent -- an Uptime Kuma monitor, a repo -- and for
 * every descriptor serialized before the field existed, so `under:` misses
 * on such a kind rather than guessing at one. Candidates in declaration
 * order, the first whose `at` resolves winning, like every path here.
 */
parent: Array<ParentPath>, 
/**
 * Where the state a row's chip shows sits, and what each value means
 * (#666): a pull request's open / merged / closed, a build's running or
 * outcome. Candidates in declaration order; the first whose value its
 * own map names wins ([`resolve_state`]). Empty for a kind with no
 * state, which draws no chip.
 */
state: Array<StatePath>, 
/**
 * The branch a pull request is opened **from**, by the name the branch's
 * own record is titled with (#712): Gitea's `head.ref`. What tells a
 * room's Code tile which branches already have an open pull request, so
 * a branch nobody has proposed yet is drawn as one. Empty for every kind
 * that is not a proposal, and a miss leaves a branch unproposed.
 */
head: Array<PayloadPath>, 
/**
 * The commit a record of code stands at, by its full object id (#710):
 * a pull request's head commit, a branch's tip, a commit itself. What
 * the detail asks the mirror's builds about: those whose declared
 * [`revision`](Self::revision) is this commit. Empty for every kind that
 * is not code, and a miss draws no builds.
 */
commit: Array<PayloadPath>, 
/**
 * The commits a build ran on, by their full object ids (#710): one per
 * VCS root, so a list. TeamCity's `revisions.revision[].version`. Empty
 * for every kind that is not a build, and a miss is a build the detail
 * of no commit draws.
 */
revision: Array<ListPath>, 
/**
 * The repo a pull request is opened **from**, by the name a branch's
 * path line draws it with (#718): Gitea's `head.repo.full_name`, which
 * is a fork's and not the base repo's when the pull request comes from
 * a fork. With [`head`](Self::head), what names the one branch a pull
 * request stands for. Empty for every kind that is not a proposal, and
 * a miss (a deleted fork) proposes no branch.
 */
head_repo: Array<PayloadPath>, 
/**
 * The name of the default branch of the repo a branch sits in (#718):
 * for Gitea, `repository.default_branch` as `map` writes it into a
 * branch's payload. A branch titled so is its repo's default, which
 * the Code tile never draws as one wanting a pull request. Empty for
 * every kind that is not a branch, and a miss draws the branch as any
 * other.
 */
default_branch: Array<PayloadPath>, 
/**
 * The ticket's type in the source's own spelling (#811): Jira's
 * `fields.issuetype.name` (`Bug`, `Story`). What `issuetype:` matches,
 * ignoring case. Its own field and not the entity kind: `type:` already
 * means the kind. Empty for every kind with no such type, and a miss
 * matches no `issuetype:` value.
 */
issue_type: Array<PayloadPath>, 
/**
 * The labels an item carries (#811): Jira's `fields.labels`, an array
 * of strings, so `entry` is empty. What `label:` matches, ignoring case.
 */
labels: Array<ListPath>, 
/**
 * The states of the sprints an item is in, one per sprint (#811):
 * `active`, `future`, `closed`. For Jira the sprint is a custom field
 * whose id is the instance's own, so `map` stores the sprints beside the
 * record as `sprints: [{id, name, state}]` and this points there.
 * `sprint:current` matches an item with an `active` one, ignoring case.
 * Empty for a kind with no sprints, and a miss is in no sprint.
 */
sprint_state: Array<ListPath>, 
/**
 * A ticket's original estimate, **in seconds** (#816): Jira's
 * `fields.timeoriginalestimate`. A JSON number; the job card draws it.
 * Seconds and nothing else, so a source that keeps hours or a phrase
 * (`"3h"`) declares no estimate rather than one every reader would
 * misread. Empty for every kind with no estimate, and a miss draws none.
 */
estimate: Array<PayloadPath>, 
/**
 * How many comments a ticket has (#894): Jira's `fields.comment.total`.
 * A whole JSON number, read as `estimate` is; the ticket card draws
 * "N comments" above zero. Empty for every kind with no count, and a
 * miss draws nothing.
 */
comment_count: Array<PayloadPath>, 
/**
 * Where a page sits among its siblings in the source's own order
 * (#953): a whole JSON number, lowest first. The Docs tree orders a
 * parent's children by it, and a child without one follows, by title.
 * Empty for every kind the source does not order.
 */
position: Array<PayloadPath>, 
/**
 * Which part of a shell-owned frame this kind is: of the Builds frame
 * (ADR-0022, #848), a [`Frame::Pipeline`] the Builds view lists or a
 * [`Frame::Run`] of one; or a ticket system's [`Frame::SavedView`]
 * (#1207). A role, not a path, like
 * [`blocked_statuses`](Self::blocked_statuses). `None` for every kind no
 * frame draws.
 */
frame: Frame | null, 
/**
 * The ticket system a [`Frame::SavedView`] kind's views are listed under
 * (#1207): Jira's is `Jira`, so its boards and filters sit under *Jira
 * boards* and *Jira filters* whatever a person named the source. Not the
 * descriptor's `name`, which is the instance's and changes with it.
 * `None` lists them under the source's name; `None` for every kind that
 * is not a saved view.
 */
system_name: string | null, 
/**
 * The query a [`Frame::SavedView`] holds, in the source's own language
 * (#2050): Jira's `jql`, the board's saved filter or the filter's own
 * JQL. What the Tickets screen's source line and *View settings* draw;
 * `sync.saved_view.source_query`. Required of a saved-view kind by the
 * ticket-system minimum, and a view whose record carries none draws no
 * query. Empty for every kind that is not a saved view.
 */
source_query: Array<PayloadPath>, 
/**
 * A saved view's board type in the source's own word (#2050): Jira's
 * `type` (`scrum`, `kanban`, `simple`). Optional; a filter has none.
 */
board_type: Array<PayloadPath>, 
/**
 * The query a saved view narrows [`source_query`](Self::source_query)
 * with (#2050): a Jira Kanban board's `sub_query`. Optional.
 */
sub_query: Array<PayloadPath>, 
/**
 * A saved view's active sprint (#2050), a JSON **object**
 * `{"id", "name", "end_date"}` as the shell draws it: Jira's `sprint`
 * on a Scrum board. Optional; a miss is no sprint.
 */
sprint: Array<PayloadPath>, 
/**
 * What a pipeline is called (#848): TeamCity's configuration `name`.
 * Empty for every kind that is not a pipeline.
 */
name: Array<PayloadPath>, 
/**
 * The group a pipeline sits in, in the source's own word (#848): a
 * TeamCity project, a Jenkins folder. TeamCity's `projectName`.
 */
group: Array<PayloadPath>, 
/**
 * The chain of groups a pipeline sits in, **outermost first**, ending in
 * the one [`group`](Self::group) names (#849, R-9.3): a TeamCity project
 * and every project above it, below the server's root. What the Builds
 * list draws its project tree from. A list of names, so a [`ListPath`];
 * TeamCity's `projectPath[].name`, which the adapter writes beside the
 * record from `/app/rest/projects`. Empty for every kind that is not a
 * pipeline, and a miss draws the pipeline under its `group` alone.
 */
group_path: Array<ListPath>, 
/**
 * The pipeline's identity as **both** of its kinds spell it (#848): on a
 * pipeline its own id, on a run the id of the pipeline it ran. A run
 * belongs to the pipeline of its own source whose `pipeline` is equal.
 * TeamCity: a configuration's `id`, a build's `buildTypeId`.
 */
pipeline: Array<PayloadPath>, 
/**
 * A run's number as the source writes it (#848), a string: TeamCity's
 * `number`, which a queued build does not have yet.
 */
number: Array<PayloadPath>, 
/**
 * The branch a run built, by name (#848): TeamCity's `branchName`.
 */
branch: Array<PayloadPath>, 
/**
 * When a run started (#848): an ISO 8601 instant, extended
 * (`2026-08-22T10:20:00Z`) or basic (`20260822T102000+0000`, TeamCity's
 * `startDate`). A queued run has none.
 */
started: Array<PayloadPath>, 
/**
 * When a run finished (#848), as [`started`](Self::started) is written:
 * TeamCity's `finishDate`. A run in flight has none; with `started` it
 * is the run's duration.
 */
finished: Array<PayloadPath>, 
/**
 * How a run was started (#961), mapped onto the shell's own words: a
 * change pushed, a schedule, or a person. A [`TriggerPath`] per
 * candidate, read as [`state`](Self::state) is: the value at `at`,
 * trimmed, matched exactly against the map, the first hit winning; a
 * value no map names is a miss. TeamCity's `triggered.type`, `vcs`,
 * `schedule` and `user`.
 */
trigger: Array<TriggerPath>, 
/**
 * Who started a run, as the source spells the account (#961), so it can
 * be matched against the configured usernames as
 * [`assignee`](Self::assignee) is. TeamCity's `triggered.user.username`;
 * a run a change or a schedule started has none.
 */
started_by: Array<PayloadPath>, 
/**
 * The machine a run ran on, by the name the source gives it (#1004):
 * TeamCity's `agent.name`. A run no agent has taken yet (a queued
 * TeamCity build) has none.
 */
agent: Array<PayloadPath>, 
/**
 * What a run in flight is doing now, in the source's own words (#1004):
 * TeamCity's `running-info.currentStageText` (*step 3/5 `cargo test`*).
 * Only a run that has started and not finished has one; a finished or a
 * queued run misses.
 */
current_step: Array<PayloadPath>, 
/**
 * How many of a run's tests failed (#1448, R-9.6): a whole JSON number,
 * TeamCity's `testOccurrences.failed`. With
 * [`tests_passed`](Self::tests_passed) and
 * [`tests_ignored`](Self::tests_ignored) it is the run's test summary
 * (*1 failed, 41 passed*). A run that ran no tests misses all three;
 * TeamCity then serves no `testOccurrences` at all.
 */
tests_failed: Array<PayloadPath>, 
/**
 * How many of a run's tests passed (#1448): TeamCity's
 * `testOccurrences.passed`. Read as [`tests_failed`](Self::tests_failed).
 */
tests_passed: Array<PayloadPath>, 
/**
 * How many of a run's tests were ignored (#1448): TeamCity's
 * `testOccurrences.ignored`. Read as [`tests_failed`](Self::tests_failed).
 */
tests_ignored: Array<PayloadPath>, 
/**
 * The language a repository is written in, in the source's own word
 * (#966, R-8.1): Gitea's `language`, which it works out from the
 * default branch's files. Gitea writes `""` for a repository it found
 * no language in, which every reader takes as a miss. Empty for every
 * kind that is not a repository.
 */
language: Array<PayloadPath>, 
/**
 * The teams a repository belongs to, by name (#966, R-8.1). A list: a
 * repository may sit in more than one. For Gitea, the organization teams
 * the repository was added to, which `map` writes beside the record from
 * `/repos/{owner}/{repo}/teams`, leaving out a team that holds every
 * repository of its organization (*Owners*). Empty for every kind that
 * is not a repository, and a miss names no team.
 */
teams: Array<ListPath>, 
/**
 * Who owns a repository, by the account's name (#1020): Gitea's
 * `owner.login`, the organization or user the repository sits under.
 * What a set's repository row draws where no team is declared. Empty
 * for every kind that is not a repository.
 */
owner: Array<PayloadPath>, 
/**
 * How a monitor checks, in the source's own word (#1020): Uptime
 * Kuma's `type` (`http`, `ping`, `dns`). Its own field and not
 * [`issue_type`](Self::issue_type), which `issuetype:` matches. Empty
 * for every kind that is not a monitor.
 */
monitor_type: Array<PayloadPath>, 
/**
 * When a pull request was opened (#929), an ISO 8601 instant as
 * [`started`](Self::started) is written: Gitea's `created_at`. The
 * conversation draws it as a line on the tickets the pull request is
 * linked to.
 */
opened: Array<PayloadPath>, 
/**
 * When a pull request was merged (#929), as [`opened`](Self::opened) is
 * written: Gitea's `merged_at`, absent until it is. The instant beside
 * the [`merged`](Self::merged) flag, a second fact and not a second
 * candidate for it.
 */
merged_at: Array<PayloadPath>, 
/**
 * How long a monitor's last check took, **in milliseconds** (#1224):
 * Uptime Kuma's `response_time_ms`, as its `map` writes it. A JSON
 * number, read by [`resolve_number`]; what knobas' samples are taken
 * from beside the state, and what the Monitors tab draws. Milliseconds
 * and nothing else, as [`estimate`](Self::estimate) is seconds. Empty for
 * every kind that is not a monitor, and a miss is a check with no
 * reading.
 */
response_time: Array<PayloadPath>, 
/**
 * The branch a pull request is opened **into**, by the name the
 * branch's own record is titled with, as [`head`](Self::head) is the one
 * it is opened from (#1227): Gitea's `base.ref`. Empty for every kind
 * that is not a proposal.
 */
base: Array<PayloadPath>, };

/**
 * Where a **list** of strings lives: the array, and where the string sits
 * inside each element.
 *
 * Gitea's requested reviewers are `requested_reviewers`, each element an
 * object whose `login` is the account -- so `at` is `["requested_reviewers"]`
 * and `entry` is `["login"]`. An empty `entry` says the elements are the
 * strings themselves.
 */
export type ListPath = { 
/**
 * Where the array is.
 */
at: PayloadPath, 
/**
 * Where the string is inside each element; empty means the element itself.
 */
entry: PayloadPath, };

/**
 * How a [`MergePullRequest`](WriteOp::MergePullRequest) lands its commits on
 * the base branch. Serialized as the lowercase word (`"squash"`), which is
 * also what a source view's merge action offers as its options.
 *
 * - `merge`: a merge commit joining head into base.
 * - `rebase`: the head's commits replayed onto base, no merge commit.
 * - `squash`: the head's commits folded into one commit on base.
 *
 * The styles are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type MergeStyle = "merge" | "rebase" | "squash";

/**
 * What kind of check a [`CreateMonitor`](WriteOp::CreateMonitor) asks for
 * (#864, spec #808 story 154), in knobas' words rather than one server's:
 * Uptime Kuma calls a TCP check `port`. Serialized as the lowercase word.
 *
 * The four a reader is offered for an asset (R-11.4): HTTP for a route or a
 * service, TCP for a database, Docker for a container, Ping for a VM. A
 * closed set, so a payload naming a check no adapter makes fails to decode at
 * `submit_write` rather than reaching a source.
 *
 * - `http`: an http(s) URL fetched and expected to answer 2xx.
 * - `tcp`: a TCP connection to `host:port`.
 * - `ping`: an ICMP ping to a host.
 * - `docker`: a container, by name, asked of the monitoring source's Docker
 *   host.
 *
 * The kinds are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type MonitorKind = "http" | "tcp" | "ping" | "docker";

/**
 * What the Add-source form submits. Carries the typed secret, which goes to
 * the keychain and never to Postgres (§14).
 */
export type NewSource = { id: string, adapter_kind: string, display_name: string, base_url: string, auth_kind: AuthMethod, config: unknown, secret: SecretInput, sync_interval_secs: number, enabled: boolean, };

/**
 * Where an item's **parent** sits in its payload, and the names that parent
 * answers to (ADR-0018, `CONTEXT.md`'s **Parent**).
 *
 * The parent is the entity you can open that an item sits directly inside: a
 * repo for a commit, a build configuration for a build, the parent page for a
 * page, the epic for a ticket. It comes from the source, so the adapter says
 * where it is, the same way it says where a status is. It is neither a link
 * nor a project.
 *
 * * `kind` is the parent's entity kind (`repo`, `build_config`). It is
 *   declared rather than looked up because a parent need not be in the
 *   mirror -- an epic outside the synced JQL is still the parent its children
 *   name -- and a suggestion for `under:` still has to say what it is.
 * * `at` is where the parent is. It leads either to **one object**, the
 *   direct parent, or to **an array of objects**, the whole chain outermost
 *   first with the direct parent last (a Confluence page's `ancestors`). The
 *   array form is what makes `under:` transitive for a source whose records
 *   carry their chain.
 * * `names` are where, inside each parent object, the names it answers to
 *   live, in the parent's own spelling -- a repo's `owner/name` and its bare
 *   `name`, a configuration's title and its id. `under:<name>` matches them
 *   exactly, ignoring case. The first name is the one a path line shows.
 *
 * A declaration whose `at` leads nowhere, or to neither shape, is a miss: the
 * item has no parent as far as knobas knows, which is never a guess.
 */
export type ParentPath = { 
/**
 * The parent's entity kind.
 */
kind: string, 
/**
 * Where the parent object, or the chain of them, is.
 */
at: PayloadPath, 
/**
 * Where each name the parent answers to sits inside a parent object.
 */
names: Array<PayloadPath>, };

/**
 * Where one field lives in a payload: the object keys to walk, in order.
 *
 * `["fields", "status", "name"]` is Jira Data Center's status. An empty path
 * addresses the payload itself, which is an object and therefore never a
 * value any of the resolvers below accept -- so an adapter cannot declare
 * "the whole record" as its status.
 *
 * **Keys, not a pointer string or a JSONPath expression.** A segment list has
 * no escaping rules to get wrong (a Jira custom field is `customfield_10008`,
 * but a source is free to name a key `a/b`), no syntax that can fail to
 * parse at run time, and it is what PostgreSQL's `#>` operator takes.
 */
export type PayloadPath = Array<string>;

/**
 * Where the **chain of projects** an item sits in is, and what each project
 * in it is keyed and called (#1019, ADR-0010).
 *
 * A source whose projects nest (a TeamCity project and its subprojects)
 * names the item's own project with [`KindPaths::project_key`] and
 * [`KindPaths::project_name`]; this says where the whole chain is, so a
 * project takes in everything under it. Not a parent: a project is a
 * grouping, not something you open (ADR-0018).
 *
 * * `at` leads to an **array of objects**, outermost first, ending in the
 *   item's own project. Anything else is a miss.
 * * `key` and `name` are candidate paths inside each object, read as
 *   [`KindPaths::project_key`] is: the first non-blank string wins. A
 *   project with no readable key is left out of the chain.
 */
export type ProjectPath = { 
/**
 * Where the array of project objects is.
 */
at: PayloadPath, 
/**
 * Where a project's key sits inside its object.
 */
key: Array<PayloadPath>, 
/**
 * Where a project's name sits inside its object.
 */
name: Array<PayloadPath>, };

/**
 * The state an [`EditPullRequest`](WriteOp::EditPullRequest) puts a pull
 * request in. Serialized as the lowercase word (`"closed"`), which is also
 * Gitea's.
 */
export type PullRequestState = "open" | "closed";

/**
 * How many writes are in each open state.
 *
 * One statement rather than three, and separate numbers rather than a total:
 * the shell has to say that something needs a *decision* rather than
 * patience, so "3 waiting" may never absorb a held write.
 */
export type QueueCounts = { pending: number, held: number, refused: number, };

/**
 * One write knobas owes a source.
 *
 * The vocabulary is `CONTEXT.md`'s: a row in [`WriteState::Pending`] is a
 * **pending write**, one in [`WriteState::Held`] is a **held write**.
 */
export type QueuedWrite = { 
/**
 * Identity and queue order in one value -- see migration `0005`.
 */
id: number, 
/**
 * Which source owes the write.
 */
source_id: string, 
/**
 * The target, as an entity id.
 */
entity_id: string, 
/**
 * `knobas_source::WriteOp::identifier()`: the stable snake_case
 * operation name, `"comment"`.
 */
op: string, 
/**
 * The serialized `WriteOp`; `amend_write` takes it back, edited.
 */
payload: WriteOpPayload, 
/**
 * The target as it was when the write was queued -- [`project`]'s output.
 *
 * One half of the two versions a held write is shown with. Its shape is
 * per-op; for `"comment"` it carries `{op, live, text}`, where `text` is
 * the item's indexed text -- which every adapter builds from its title,
 * description and comment bodies, so a new reply is exactly what changes
 * it.
 */
target_snapshot: unknown, state: WriteState, 
/**
 * Why a pending write is waiting; `None` while nothing has been tried,
 * and again once a person acting on the source -- a new credential, a
 * new address or configuration -- has released an `unauthorized` wait.
 */
wait_reason: WaitReason | null, 
/**
 * What the source said, in its own words. Untrusted source text: render
 * it as text, never as markup.
 */
detail: string | null, queued_at: string, attempted_at: string | null, attempts: number, 
/**
 * The target as it stood when the write was held -- the other half of
 * "both versions side by side". `None` until a write is held; kept
 * afterwards.
 */
held_snapshot: unknown | null, settled_at: string | null, 
/**
 * Whether the user has this write's source turned on, **as of this read**
 * -- derived by [`queue_columns!`], never stored (issue #204).
 *
 * Since migration `0012` a disabled source's items leave
 * `sync.live_item`, so its queued writes go [held](WriteState::Held) by
 * the same mechanism as a withdrawn target's -- [`target_of`] reads
 * `None` either way. The two are not the same fact: one is upstream's
 * doing and resolved by choosing a version, the other is the user's own
 * and undone by re-enabling the source. This flag is what keeps them
 * distinguishable downstream; storing it instead would leave the wrong
 * answer standing after the click that re-enables.
 *
 * `false` only when a configuration row exists and says off. A source
 * with no row at all reads `true`, matching `0012`'s
 * `coalesce(enabled, true)`: absence of configuration is not a decision
 * the user made.
 *
 * The remedies differ as the facts do: a target that changed wants the
 * two-versions choice, a source that is off wants re-enabling. Say
 * which, and never offer *Send mine anyway* for a source that cannot
 * take it. Flips on the next read once the source is back on.
 */
source_enabled: boolean, };

/**
 * Uninstall's answer for one source: remove its items, or keep them.
 */
export type Removal = { source_id: string, purge_items: boolean, };

/**
 * One line comment of a [`SubmitReview`](WriteOp::SubmitReview): the file's
 * path in the change, the line number on `side`, and what it says.
 */
export type ReviewComment = { path: string, line: number, side: ReviewSide, body: string, };

/**
 * Which side of a diff a [`ReviewComment`]'s `line` counts on: the file as it
 * was (`old`, a removed or unchanged line's old number) or as it becomes
 * (`new`).
 */
export type ReviewSide = "old" | "new";

/**
 * What a [`SubmitReview`](WriteOp::SubmitReview) says of the pull request.
 * Serialized as the snake_case word (`"request_changes"`); each adapter
 * spells it its own way (Gitea's `event`).
 *
 * - `approve`: the change may land.
 * - `request_changes`: the change must change before it lands.
 * - `comment`: words, and no verdict.
 *
 * The verdicts are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type ReviewVerdict = "approve" | "request_changes" | "comment";

/**
 * One role a source claims.
 *
 * `role` is a built-in role ([`CODE_HOST`], [`TICKET_SYSTEM`], [`CI`],
 * [`WIKI`], [`MONITORING`]) or a **custom role** namespaced to the adapter
 * kind that declares it (`grafana:dashboard-host`). `reads` names the live
 * reads the source answers for the role: every read the role requires, and
 * whichever of its optional reads the source has. A custom role has no
 * reads of its own; its view is the `view` read.
 *
 * Plain serde data, like everything crossing the SPI (spec §3a).
 */
export type RoleClaim = { role: string, 
/**
 * The live reads the source answers for the role, by identifier
 * (`run_log`).
 */
reads: Array<string>, };

/**
 * One declared path of a claimed role that missed on the records the
 * source's syncs emitted (#1199, ADR-0028), so the shell does not draw that
 * role for the source.
 */
export type RoleMiss = { 
/**
 * The role, as claimed (`ticket-system`).
 */
role: string, 
/**
 * The kind whose records missed (`ticket`).
 */
kind: string, 
/**
 * The [`KindPaths`] field (`priority`).
 */
field: string, 
/**
 * The declared path that missed, its segments joined by `.`
 * (`fields.priorityy.name`).
 */
path: string, };

/**
 * One run a role view's *Sync now* started or joined (#2106): the source it
 * runs, and the run's id, which its `sync:state` events name.
 */
export type RoleRun = { source_id: string, run_id: number, };

/**
 * A typed account on its way in -- the wire twin of
 * [`knobas_source::instance::Account`].
 *
 * Its own type rather than that one re-used, because this is an IPC argument
 * and the SPI's struct is a frozen surface: the day a stored account grows a
 * field, the form and the keychain should be able to move one at a time.
 */
export type SecretAccount = { username: string, password: string, };

/**
 * A typed credential on its way in. Never logged, never returned.
 *
 * Both fields are optional and **absent means "keep what is stored"**
 * (issue #452). That is one rule, and it is read in both directions: a reader
 * adding an account to an Uptime Kuma cannot retype an API key Kuma showed
 * them once, and a reader replacing an expired key must not silently lose the
 * account beside it. `add_source` is the one caller for which "keep" has no
 * meaning -- nothing is stored yet -- and it refuses a `value` of `None` by
 * name.
 */
export type SecretInput = { value?: string | null, 
/**
 * The optional second credential of the version-2 keychain envelope: a
 * username and password stored *beside* [`value`](Self::value), not
 * instead of it. Only Uptime Kuma reads one today (spec #427, story 69),
 * and only for its write half: the API key opens `/metrics`, the account
 * opens socket.io, and a source with only the key reads every monitor
 * and pauses none of them.
 */
account?: SecretAccount | null, };

/**
 * One source's line of [`DbStats`].
 */
export type SourceCount = { source_id: string, items: number, synced_at: string | null, };

/**
 * Everything knobas needs to know about a configured adapter instance: what
 * the Add-source form is generated from.
 *
 * A *template* (one per adapter kind, what `list_adapters` answers) has
 * `id == adapter_kind`; a configured instance's descriptor carries the
 * instance id instead.
 */
export type SourceDescriptor = { 
/**
 * Instance id, e.g. `"jira"`. Doubles as the [`EntityRef`] namespace for
 * every item this instance emits, so it must be non-blank and free of `:`.
 *
 * [`EntityRef`]: knobas_spi_types::entity::EntityRef
 */
id: string, 
/**
 * Which adapter this is an instance of, e.g. `"jira"`, `"mock"`. Distinct
 * from [`Self::entity_kinds`], which is about the *items* it emits.
 */
adapter_kind: string, 
/**
 * Human-readable name for the source list.
 */
name: string, capabilities: Array<string>, adapter_version: string, 
/**
 * How this adapter can authenticate (spec §3, §3a). The Add-source form
 * offers these; the chosen method's secret goes to the OS keychain and
 * never into [`Self::config_schema`]'s config blob.
 */
auth_methods: Array<AuthMethod>, 
/**
 * Whether this adapter can use a **second** credential beside the one
 * [`auth_methods`](Self::auth_methods) names: an optional username and
 * password, stored in the same keychain item
 * ([`instance::Account`], M4.1, issue #452).
 *
 * `false` for every adapter but Uptime Kuma, and it is a *declaration*
 * rather than a fact the forms could work out: what the Add-source and
 * re-enter forms need to know is whether to draw the optional account
 * fields at all, and the alternative to declaring it is a
 * `adapter_kind === "kuma"` in a Svelte component -- the per-adapter
 * table §3a exists to forbid.
 *
 * **Not an [`AuthMethod`]**, for the reason `instance::Account` argues:
 * the auth method is how a source authenticates its ordinary traffic,
 * one per source, and offering the account as a rival method would let a
 * reader pick it *instead of* the key.
 *
 * `#[serde(default)]`, so a descriptor built by a peer that predates this
 * still decodes -- as the adapter with one credential it was.
 */
accepts_account: boolean, 
/**
 * The shortest sync interval, in seconds, a source of this adapter may
 * be scheduled at: its **floor** (#1187). The source commands refuse an
 * interval under it, and the settings UI offers nothing under it.
 *
 * A declaration for `accepts_account`'s reason: the alternative is a
 * per-adapter table of rate limits in knobas core or in a Svelte
 * component. Uptime Kuma declares 45 and TeamCity 20; every other
 * adapter [`DEFAULT_MIN_SYNC_INTERVAL_SECS`].
 *
 * `#[serde(default)]` to that constant, so a descriptor built by a peer
 * that predates this decodes with the floor every source had before it.
 */
min_sync_interval_secs: number, 
/**
 * Which [`WriteOp`]s this adapter supports, as the stable snake_case
 * identifiers documented on that enum (`Comment` → `"comment"`).
 *
 * The UI renders its action bar from this rather than from a hardcoded
 * per-adapter table, which is why [`Capability::Write`] alone is not
 * enough: once `WriteOp` grows, "supports writes" no longer says *which*
 * actions to offer. An adapter must reject any op absent from this list
 * with [`SourceError::Protocol`], and the write queue and the WASM host
 * refuse one before it is handed over ([`Self::declares`], #1950).
 *
 * This and [`Capability::Write`] are two signals for one fact and must
 * agree: declaring `Write` with no ops leaves the UI nothing to offer,
 * and listing ops without `Write` makes the source read as read-only
 * while advertising actions. [`contract::battery`] enforces both
 * directions.
 */
write_ops: Array<string>, 
/**
 * Entity kinds this adapter emits, with display metadata -- the UI renders
 * a new source's items (launcher groups, chips, monograms) from this
 * alone, never from hardcoded kind lists (spec §3a extensibility).
 * [`SyncItem::kind`] must name one of these.
 *
 * Each entry also carries [`KindInfo::full_sync_exhaustive`], which is
 * the sweep's precondition and is declared **per kind** (ADR-0003).
 */
entity_kinds: Array<KindInfo>, 
/**
 * JSON Schema for this adapter's configuration; the Add-source form is
 * generated from it (spec §3a), never hand-built per adapter. Never
 * holds secrets -- those live in the OS keychain, keyed by the chosen
 * [`AuthMethod`]. M0: the mock declares an empty object schema.
 */
config_schema: unknown, 
/**
 * Where this adapter's records keep the things knobas reads but §4.1 does
 * not normalize: a status, a priority, an assignee, requested reviewers,
 * a merged flag, a project key and name -- and which of its status names
 * mean blocked. One entry per entity kind that has any of them.
 *
 * **This is the self-describing property applied to payloads** (M3.1,
 * issue #277, ADR-0007's recorded destination). Until it existed, every
 * reader outside an adapter held a little table of per-source spellings:
 * `coalesce(fields.status.name, status)` in the mini board, three arms
 * for a project key in the census, two assignee spellings in the inbox --
 * the same coupling [`Self::entity_kinds`] and [`Self::write_ops`] exist
 * to prevent, in the one place §4.1 left knobas nothing normalized to
 * read. A third source's spelling is now one declaration here rather than
 * one more arm in four statements.
 *
 * The rules a declaration is bound by, all enforced by
 * [`contract::battery`]:
 *
 * * every [`KindPaths::kind`] names one of [`Self::entity_kinds`] -- an
 *   adapter cannot declare paths for a kind it does not emit;
 * * a declared path lands on a value of its type on this adapter's own
 *   items, or the source says nothing there. A path into an object the
 *   source really wrote, naming a key that object does not have, is the
 *   adapter pointing at a field its records lack;
 * * a field a kind does not declare is a **miss** for every reader --
 *   never a guess, never a knobas-side fallback.
 *
 * `#[serde(default)]`: a descriptor from a peer built before this grew --
 * an out-of-process adapter, a stored blob -- decodes as an adapter that
 * declares nothing, which every reader already handles.
 *
 * [`KindPaths::kind`]: knobas_spi_types::payload::KindPaths::kind
 */
payload_paths: Array<KindPaths>, 
/**
 * The [roles](role) this adapter claims (ADR-0029, issue #1193): which
 * of the shell's surfaces it fills. Each claim is held to its role's
 * [`role::Minimum`] by [`role::check`], which [`contract::battery`]
 * runs as clause 10: the kinds it names are in [`Self::entity_kinds`],
 * the paths it needs are in [`Self::payload_paths`], and the reads it
 * needs are in the claim. A role's writes are declared, one by one, in
 * [`Self::write_ops`] as every write is. A surface reads the claim,
 * never the adapter kind.
 *
 * `#[serde(default)]`, so a descriptor from a peer that predates this
 * decodes as an adapter that claims nothing.
 */
roles: Array<RoleClaim>, 
/**
 * The [source actions](WriteOp::SourceAction) this adapter performs, by
 * its own id (`"add_label"`; ADR-0029, issue #1194): writes outside
 * every role's vocabulary. Non-empty exactly when [`Self::write_ops`]
 * lists [`SOURCE_ACTION`], and an adapter refuses a source action whose
 * id is not here with [`SourceError::Protocol`]; [`contract::battery`]
 * holds both (clause 11). Each action's dialog is drawn by the view that
 * offers it, in ADR-0020's action vocabulary, as every action's is: its
 * views offer them as `source_action` actions, and the shell never reads
 * this list.
 *
 * `#[serde(default)]`, so a descriptor from a peer that predates this
 * decodes as an adapter with none.
 */
source_actions: Array<string>, 
/**
 * Which of an instance's typed addresses a repository is cloned from,
 * in order of preference: the code-host read `clone_urls` (#1197,
 * [`clone_urls::CloneFrom`]). *Clone* appends the checked
 * `owner/repo.git` to the first that ADR-0023's rules admit. Non-empty
 * exactly when a [`role::CODE_HOST`] claim reads `clone_urls`, and a
 * config key named here is a string property of
 * [`Self::config_schema`]; [`role::check`] holds both (clause 10).
 * *Clone* reads it on the Rust side; the shell never does.
 *
 * `#[serde(default)]`, so a descriptor from a peer that predates this
 * decodes as an adapter that names no address to clone from.
 */
clone_from: Array<CloneFrom>, 
/**
 * The other forms of address the source serves an item's page under,
 * beside the one it reports as [`SyncItem::web_url`] (#1475, spec #491's
 * *"a descriptor slot with its own entry"*). The paste resolver reads a
 * pasted address as each form and opens the item whose payload holds
 * the values it names; it never fetches, and it never knows which
 * adapter declared what ([`url_forms`] says how a form reads).
 * [`url_forms::check`] holds each to its rules (contract battery clause
 * 12).
 *
 * `#[serde(default)]`, so a descriptor from a peer that predates this
 * -- an external adapter's JSON among them -- decodes as an adapter
 * whose items open by their stored address alone.
 */
url_forms: Array<UrlForm>, };

/**
 * An unsaved (or saved) source to test a connection for.
 *
 * `secret: None` **with** `source_id` set re-tests the stored credential,
 * which is what *Test connection* on an existing source does.
 */
export type SourceDraft = { 
/**
 * Set, with `secret: None`, to re-test the stored credential.
 */
source_id: string | null, adapter_kind: string, base_url: string, 
/**
 * The frozen shape's `AuthMethod`, not an `Option`.
 *
 * An earlier draft widened it so a no-credential source could be tested.
 * That was a mistake with teeth: for a **saved** source the draft's fields
 * then described a configuration the scheduled run would not use, so *Test
 * connection* could pass against auth the run never attempts. The fix is
 * the other way round -- a saved source is tested against its **stored**
 * configuration (see [`crud::test`]), and this field applies only to a
 * draft that has not been saved yet. Every source the Add-source form can
 * create has an auth method by construction ([`NewSource::auth_kind`]).
 */
auth_kind: AuthMethod, config: unknown, secret: SecretInput | null, };

/**
 * What *Edit source* may change.
 *
 * `id` and `adapter_kind` are absent **on purpose**: the instance id is the
 * entity namespace, baked into every entity id, link and activity row, and is
 * therefore immutable (P10). `display_name` is the renameable one.
 */
export type SourcePatch = { display_name?: string | null, base_url?: string | null, config?: unknown, sync_interval_secs?: number | null, enabled?: boolean | null, };

/**
 * One row of the sources view.
 */
export type SourceSummary = { id: string, adapter_kind: string, display_name: string, base_url: string, enabled: boolean, sync_interval_secs: number, config: unknown, health: CredentialHealth, last_run: SyncRunRow | null, 
/**
 * When the scheduler runs this source next; `None` when the source is
 * disabled, running, or needs a human.
 */
next_run_at: string | null, item_count: number, 
/**
 * Which kind of credential this source authenticates with, or `None`
 * when it needs none.
 *
 * The **same union** as [`NewSource::auth_kind`] and
 * [`SourceDraft::auth_kind`], widened by `None` rather than spelled a
 * second way: every source the Add-source form can create has an
 * `AuthMethod` by construction, but a *stored* row need not -- the
 * compiled-in mock reaches nothing and stores `auth_kind = 'none'`. That
 * is what `None` is here, and it is also what an `auth_kind` column
 * written by a newer knobas reads as, because
 * [`AuthKind::from_db`](knobas_sync::config::AuthKind::from_db) refuses to
 * guess. Both say the same thing to the sources view: there is no
 * credential kind to name.
 *
 * Carries no secret and cannot: it is the *kind*, and the value itself
 * lives in the OS keychain with no command that reads it back (§14).
 */
auth_kind: AuthMethod | null, 
/**
 * From the adapter's descriptor template, so the sources view labels a
 * source's kinds without a hardcoded table (§3a).
 */
kinds: Array<KindInfo>, 
/**
 * The roles the adapter claims, from its descriptor template like
 * `kinds` (#1196, ADR-0029): what the shell draws each surface for.
 * Empty for an adapter that claims none, and for a row whose adapter is
 * no longer compiled in.
 *
 * The mirror narrows a role's reads by these claims itself; the shell
 * reads them only for which optional reads a source answers
 * (`tf/roles.svelte.ts`, #1300).
 */
roles: Array<RoleClaim>, 
/**
 * The claimed roles whose declared paths missed on the records this
 * source's syncs emitted (#1199, ADR-0028 *The checks*): the mirror
 * leaves the source out of those roles' reads, the shell draws none of
 * these roles for the source and offers none of their optional reads
 * (`tf/roles.svelte.ts`), and its health line names each path. Only
 * roles still in [`roles`](Self::roles); empty when every claimed role
 * is filled or not judged yet.
 */
role_misses: Array<RoleMiss>, };

/**
 * The coarse state one source's syncing is in -- the `sync:state` payload
 * (interfaces §2.3).
 *
 * **Coarse by rule.** Roadmap §4: events are not for throughput. At most a
 * handful of these per run -- a transition each -- and per-item progress goes
 * on a [`Channel`](crate::progress::ProgressSink) and nowhere else.
 *
 * Lives here rather than in [`scheduler`](crate::scheduler) because the log is
 * what it is read out of: `status_for` builds one from `knobas.sync_run` plus
 * `knobas.source_config`, and the scheduler re-exports it.
 */
export type SourceSyncStatus = { source_id: string, 
/**
 * Whether a run is in flight right now.
 */
running: boolean, 
/**
 * The run this status is about: the one in flight, or -- once
 * [`running`](Self::running) is false -- the last one to finish, which is
 * the run [`last_outcome`](Self::last_outcome) describes. `None` only for
 * a source that has never run.
 *
 * That is the **listing's** answer, which is all `status_for` can give:
 * it is about the source. On a `sync:state` **event** it is stronger --
 * the run that event is about, whatever else the source has open (#304).
 */
run_id: number | null, 
/**
 * When the run this status is about started -- set **while
 * [`running`](Self::running)**, and `None` once that run is over. The
 * TypeScript mirror says the same thing in its own units ("RFC 3339,
 * while `running`"), and `scheduler::state_of_run` relies on it: a
 * terminal emit carries the run's time in
 * [`last_finished_at`](Self::last_finished_at) rather than here (#304).
 */
started_at: string | null, 
/**
 * When the last run that finished, finished.
 */
last_finished_at: string | null, last_outcome: SyncOutcome | null, 
/**
 * When the scheduler will run this source next: last finish + interval,
 * clamped up by [`backoff_until`](Self::backoff_until). **Derived, never
 * stored** (ruling P7). `None` while a run is in flight, and for a source
 * that is disabled or needs a human -- a countdown to a run that will
 * never start is a lie the sources view would render.
 */
next_run_at: string | null, 
/**
 * `knobas.source_config.backoff_until` -- how long a failing source is
 * held off for. Never set by a 401 (P7).
 */
backoff_until: string | null, 
/**
 * Whether the run [`last_outcome`](Self::last_outcome) describes wrote
 * anything to the mirror: its `upserted`, `deleted` or `swept` count is
 * above zero (#759). `None` while no run of the source has finished.
 *
 * What a listener that re-reads the mirror after a run keys on, so a run
 * that changed nothing costs it nothing. **An over-approximation, in the
 * safe direction**: `upserted` counts an item re-written unchanged, so
 * `true` may be a run that moved nothing, but `false` never hides a run
 * that did.
 */
last_changed: boolean | null, };

/**
 * Where a row's **state** sits, and the values it is drawn for (#666).
 *
 * `at` leads to a string or a boolean, so a merged flag can be a candidate
 * beside a state word. `values` is the map: a value it does not name is a miss for
 * this candidate, and the next one is tried. So a TeamCity build declares
 * its `state` with only `running` and `queued` mapped, and a finished build
 * falls through to its `status`.
 */
export type StatePath = { 
/**
 * Where the value is.
 */
at: PayloadPath, 
/**
 * The values this candidate draws a chip for, matched exactly after
 * trimming.
 */
values: Array<StateValue>, };

/**
 * One value a [`StatePath`] draws, and how.
 */
export type StateValue = { 
/**
 * The value as the source writes it: a JSON string (`"open"`,
 * `"FAILURE"`) or a JSON boolean (`true`). Typed, so a source writing
 * the word `"true"` is not taken for a flag (the refusal
 * [`resolve_flag`] makes). A string is matched after trimming.
 */
value: string | boolean, 
/**
 * How loud the chip is.
 */
tone: Tone, 
/**
 * What the chip says, where the value itself is not a word a reader
 * should see (`merged` for a merged flag's `true`). `None` shows the
 * value.
 */
text: string | null, };

/**
 * How a run ended.
 *
 * Stored in `knobas.sync_run.outcome`, whose `sync_run_outcome_chk`
 * allows exactly these spellings. The scheduler's backoff branches on it,
 * so a variant the constraint does not know about is not a display bug:
 * the `UPDATE` that closes the run is refused, `settle` logs and swallows
 * that (it must not mask the run's own failure), and the row never closes.
 */
export type SyncOutcome = "ok" | "unauthorized" | "unreachable" | "error";

/**
 * Where a run is.
 *
 * - `fetching`: waiting on the remote system.
 * - `writing`: writing a batch into Postgres.
 *
 * The phases are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type SyncPhase = "started" | "fetching" | "writing" | "finished" | "failed";

/**
 * One progress message.
 */
export type SyncProgress = { 
/**
 * `knobas.sync_run.id` -- on every message, so a UI watching two runs can
 * tell them apart.
 */
run_id: number, source_id: string, phase: SyncPhase, 
/**
 * Items pushed so far this run.
 */
items: number, elapsed_ms: number, message: string | null, 
/**
 * How many units of its outer walk the run has done (#1379): the latest
 * `Sink::walked` reading. Absent when the adapter never reported one,
 * and then `of` and `unit` are absent too.
 */
walked?: number, 
/**
 * How many units the walk has, when the adapter knows. With it the bar
 * is determinate; without it, indeterminate.
 */
of?: number, 
/**
 * The walk's unit, a plural noun printed as is: "120 of 300
 * repositories".
 */
unit?: string, };

/**
 * What one run wrote.
 *
 * Counts are per *entity*, deduplicated across the whole run: an adapter that
 * emits the same entity twice contributes one. `deleted` is a subset of
 * `upserted` -- every item is written to both tables and `deleted` says how
 * many of them ended the run tombstoned -- so after a full sync,
 * `upserted == count(sync.item where source_id = <id>)`.
 */
export type SyncReport = { source_id: string, 
/**
 * Distinct entities written this run.
 */
upserted: number, 
/**
 * How many of them ended the run tombstoned.
 */
deleted: number, 
/**
 * Rows a **full** sync tombstoned because this run did not see them
 * (hard-delete reconciliation). Counts only rows of a kind that declared
 * `full_sync_exhaustive` *and* emitted something this run, so it is 0 for
 * an incremental run, for a kind that did not declare the flag, and for a
 * kind that declared it and emitted nothing -- see [`SWEEP`].
 *
 * A run over a source with both kinds of kind reports one number for the
 * exhaustive half; the budgeted half is never in it. What that leaves
 * unreachable is on [`run_once`], under *Limitations*.
 */
swept: number, 
/**
 * Where the source says the next run should resume. Opaque to the
 * frontend: it is handed back, unread, to resume an incremental sync.
 */
cursor: string, };

/**
 * One row of the sync log, as the diagnostics view reads it.
 */
export type SyncRunRow = { id: number, source_id: string, trigger: SyncTrigger, started_at: string, 
/**
 * `None` while the run is in flight -- which is what "running" means.
 */
finished_at: string | null, outcome: SyncOutcome | null, upserted: number, deleted: number, 
/**
 * Rows the full-sync sweep tombstoned.
 */
swept: number, error: string | null, cursor_after: string | null, };

/**
 * Why a run happened: the `trigger` of a `knobas.sync_run` row, which
 * the diagnostics list labels a run with.
 *
 * - `schedule`: the scheduler's interval elapsed.
 * - `manual`: the user's *Sync now*.
 * - `first_run`: the first sync after a source was added, the one the
 *   first-run wizard draws a progress bar for.
 * - `backfill`: a deliberate cursor-less run that widens the payload of
 *   items an incremental would never re-fetch (#32), and is **forbidden
 *   to reconcile**: ratified non-sweeping by Björn 2026-08-28, because
 *   the way a cursor-less run goes wrong produces no error -- a
 *   credential that quietly loses sight of a project answers with a
 *   smaller corpus and a 200, and a sweeping backfill would read that as
 *   "those items are gone". So unlike every other cursor-less run it
 *   *cannot* have tombstoned anything. Its own spelling because logged as
 *   `manual` it would be indistinguishable from *Sync now*, and when a
 *   suspicious `swept` count needs explaining, this spelling is what
 *   rules a run out.
 *
 * Stored in `knobas.sync_run.trigger`, whose `sync_run_trigger_chk`
 * allows exactly these spellings -- as last defined, which is 0004 for
 * this vocabulary and 0002 for `SyncOutcome`.
 *
 * The triggers are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type SyncTrigger = "schedule" | "manual" | "first_run" | "backfill";

/**
 * How a run was started, in the shell's words (#961, #1146).
 *
 * - `change`: a change pushed to what the pipeline builds: TeamCity's `vcs`.
 * - `schedule`: a schedule: TeamCity's `schedule`.
 * - `person`: a person, who [`KindPaths::started_by`] names: TeamCity's
 *   `user`.
 * - `chain`: a step of a chain, queued because another step of it was
 *   (#1146): TeamCity's `snapshotDependency`. [`KindPaths::started_by`] may
 *   still name whoever queued the chain.
 *
 * The triggers are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type Trigger = "change" | "schedule" | "person" | "chain";

/**
 * Where a run's [`Trigger`] sits, and the source's words for each (#961).
 */
export type TriggerPath = { 
/**
 * Where the source's word is: a string.
 */
at: PayloadPath, 
/**
 * The words this candidate reads, matched exactly after trimming.
 */
values: Array<TriggerValue>, };

/**
 * One source word a [`TriggerPath`] reads, and the trigger it is.
 */
export type TriggerValue = { 
/**
 * The word as the source writes it (`vcs`).
 */
value: string, 
/**
 * What the shell calls it.
 */
trigger: Trigger, };

/**
 * What the last update check of an installed adapter found (#1209).
 *
 * - `current`: the installed release is the latest.
 * - `available`: a newer release exists, which nothing has switched to;
 *   nothing changes until the reader updates.
 * - `failed`: the check could not read the latest release.
 * - `refused`: the reader updated, and the new release's static checks
 *   refused it: the installed one stays.
 *
 * The states are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type UpdateCheck = { "state": "current" } | { "state": "available", tag: string | null, adapter_version: string, } | { "state": "failed", reason: string, } | { "state": "refused", tag: string | null, reason: string, };

/**
 * An update before it switches (#2031): what is installed, and the latest
 * release, which has passed every check.
 */
export type UpdatePreview = { installed: AdapterPreview, release: AdapterPreview, };

/**
 * How an [`UpdatePullRequestBranch`](WriteOp::UpdatePullRequestBranch)
 * brings the head up to date with its base. Serialized as the lowercase
 * word, which is also what the view's *Update branch…* offers as its options
 * and what Gitea's `?style=` takes.
 *
 * - `merge`: the base merged into the head: a merge commit on the head.
 * - `rebase`: the head's commits replayed onto the base: the head rewritten.
 *
 * The styles are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type UpdateStyle = "merge" | "rebase";

/**
 * One more form an item's page is served under, beside the `web_url` the
 * adapter reports.
 *
 * `template` is the address **after the source's base URL**, starting with
 * `/`: literal text, with `{}` for one value and at most one `{*}`, last, for
 * the rest of the address. `{*}` alone is the whole address after the base,
 * its `/` included: a payload that keeps a relative address (Confluence's
 * tiny link, `/x/CoAF`) is matched by it. `fields[i]` is where the item's payload holds the
 * value of the template's `i`-th placeholder.
 *
 * * `{}` ends at the next `/`, `?`, `&` or `#`, and its text is
 *   form-decoded before it is compared (`%2B` is `+`, `+` is a space): a
 *   Confluence title in a `/display/` link is spelled that way.
 * * `{*}` is the rest of the address, fragment aside, compared as written:
 *   it stands for a stored *address* (Confluence's `_links.tinyui`), and an
 *   address compared to an address is not decoded.
 *
 * After the template the pasted address may go on only with a query
 * (`?…`, `&…`) or a trailing `/`: `…?pageId=360458&focusedCommentId=7` is
 * still the page.
 *
 * The contract battery holds a declaration to [`check`].
 */
export type UrlForm = { 
/**
 * The entity kind of the items this form names; one of the descriptor's
 * `entity_kinds`.
 */
kind: string, 
/**
 * The address after the base URL, e.g. `/pages/viewpage.action?pageId={}`.
 */
template: string, 
/**
 * Where the payload holds each placeholder's value, in template order.
 */
fields: Array<PayloadPath>, };

/**
 * Why a pending write has not gone yet.
 *
 * - `unreachable`: the server did not answer, or the keychain would not
 *   open. The two climb the same ladder, and a keychain fault's
 *   `detail` starts `keychain:` (#1741).
 * - `unauthorized`: the credential was refused.
 *
 * Only the two *retryable* faults appear here, the ones that pass on
 * their own or with a human's help. A refusal is not a reason to wait,
 * it is a reason to stop, and it has its own [`WriteState::Refused`];
 * `write_queue_reason_state_chk` in migration 0005 is what keeps a held
 * or refused row from also claiming to be waiting on a network. So a
 * row's `wait_reason` is `null` on every row that is not `pending`, on a
 * pending one that has not been tried yet, which `attempted_at` is what
 * distinguishes, and on one a person acting on the source released
 * ([`release_credential_waits`]).
 *
 * The two are waited out differently (#1628): an `unreachable` write is
 * tried again on the sync side's backoff ladder, an `unauthorized` one
 * only after a person acts on the source or presses *Flush now*. A
 * person acting on the source also ends a keychain fault's wait at once
 * (#1944).
 *
 * The reasons are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type WaitReason = "unreachable" | "unauthorized";

/**
 * A write knobas asks an adapter to perform on the remote system.
 *
 * Each variant has a stable snake_case identifier that adapters list in
 * [`SourceDescriptor::write_ops`] and the UI renders its action bar from:
 *
 * | variant | identifier | ratified for |
 * | --- | --- | --- |
 * | [`Comment`](Self::Comment) | `"comment"` | M1 (Jira, Gitea) |
 * | [`Transition`](Self::Transition) | `"transition"` | M2 (Jira) |
 * | [`CreateTicket`](Self::CreateTicket) | `"create_ticket"` | M2 (Jira) |
 * | [`CreateBranch`](Self::CreateBranch) | `"create_branch"` | M2 (Gitea) |
 * | [`CreatePullRequest`](Self::CreatePullRequest) | `"create_pull_request"` | M2 (Gitea) |
 * | [`Approve`](Self::Approve) | `"approve"` | M2 (Gitea) |
 * | [`TriggerBuild`](Self::TriggerBuild) | `"trigger_build"` | M2 (TeamCity) |
 * | [`RerunBuild`](Self::RerunBuild) | `"rerun_build"` | M2 (TeamCity) |
 * | [`LogWork`](Self::LogWork) | `"log_work"` | M3 (Jira) |
 * | [`MergePullRequest`](Self::MergePullRequest) | `"merge_pull_request"` | source views (Gitea, #667) |
 * | [`SetParameter`](Self::SetParameter) | `"set_parameter"` | source views (TeamCity, #668) |
 * | [`RunBuild`](Self::RunBuild) | `"run_build"` | source views (TeamCity, #669) |
 * | [`SubmitReview`](Self::SubmitReview) | `"submit_review"` | git workflows, spec #701 (Gitea, #705) |
 * | [`DeleteBranch`](Self::DeleteBranch) | `"delete_branch"` | source views (Gitea, #707) |
 * | [`EditPullRequest`](Self::EditPullRequest) | `"edit_pull_request"` | git workflows, spec #701 (Gitea, #706) |
 * | [`UpdatePullRequestBranch`](Self::UpdatePullRequestBranch) | `"update_pull_request_branch"` | git workflows, spec #701 (Gitea, #708) |
 * | [`CommitFile`](Self::CommitFile) | `"commit_file"` | ticket-first, spec #808 (Gitea, #822) |
 * | [`SourceAction`](Self::SourceAction) | `"source_action"` | roles, spec #1192 (Gitea, #1194) |
 *
 * The enum grows per milestone and **each growth is a §10.8 ratified
 * exception** (ADR-0006); an adapter must reject every op it does not declare
 * with [`SourceError::Protocol`].
 *
 * On the wire it is externally tagged, so the variant name is the object's
 * one key, and a frontend written today will meet a payload it does not
 * recognise. It handles that by narrowing on the key rather than assuming
 * `Comment`: an unrecognised op can still be applied and discarded, and only
 * *editing* needs to know where the words are. What each variant means is
 * in the table above and on the variant; what the shell needs of its payload
 * is on the payload's fields, because the generated TypeScript
 * (`app/src/lib/ipc/generated/sources.ts`) carries a field's doc but not a
 * variant's (#1325).
 *
 * **One variant per operation, never per adapter** (ADR-0006). Two sources
 * that do the same conceptual thing share a variant, and the adapter-specific
 * spelling lives behind the adapter's own `write`. That is why `Comment` says
 * nothing about issues or pull requests, and why `CreateTicket` is spelled in
 * knobas' vocabulary (`CONTEXT.md`: a ticket) rather than in Jira's.
 *
 * ## Every variant names its target, almost always as `entity`
 *
 * The target is an [`EntityRef`] in string form, and it is three things at
 * once: what the adapter resolves to an API path, the **ordering key** the
 * write queue keeps per-entity order within, and the thing hold detection
 * snapshots. [`WriteOp::delivery`] says which field carries it and
 * [`WriteOp::target`] reads that field; `CreatePage`'s `parent` and
 * `CommitFile`'s `repo` are the two that are not `entity`.
 *
 * For an op that *creates* something, the entity is the **container** the new
 * thing goes into -- the Jira project, the Gitea repository -- addressed in
 * the source's own namespace (`jira:PAY`, `gitea:tidewater/payout-service`).
 * A container knobas does not mirror is still a legal target: the queue has no
 * foreign key on it, and hold detection reads "not in the mirror" as a fact
 * rather than an error.
 *
 * [`EntityRef`]: knobas_spi_types::entity::EntityRef
 */
export type WriteOpPayload = { "Comment": { entity: string, body: string, } } | { "Transition": { entity: string, status: string, } } | { "CreateTicket": { entity: string, title: string, body: string, 
/**
 * The source's own name for the kind of ticket (`"Task"`, `"Bug"`).
 * Required: Jira refuses a create without one and there is no
 * defensible default -- a project's issue types are configured.
 */
ticket_type: string, } } | { "CreateBranch": { entity: string, name: string, 
/**
 * What the branch starts from -- a branch name, a tag or a commit.
 */
from_ref: string, } } | { "CreatePullRequest": { entity: string, title: string, body: string, 
/**
 * The branch carrying the change.
 */
head: string, 
/**
 * The branch it is proposed into.
 */
base: string, } } | { "Approve": { entity: string, body: string, } } | { "TriggerBuild": { entity: string, } } | { "RerunBuild": { entity: string, } } | { "LogWork": { entity: string, 
/**
 * RFC 3339, UTC -- when the logged span began.
 */
started: string, 
/**
 * How long was **worked**, which is not the span's length.
 */
seconds: number, comment: string, } } | { "CreatePage": { 
/**
 * The **parent page**, as an entity id -- this op's queue target.
 */
parent: string, 
/**
 * The space key in Confluence's own spelling (`ENG`), not an id.
 */
space: string, title: string, 
/**
 * Storage format, not text: what a later read must return unchanged.
 */
body: string, } } | { "UpdatePage": { entity: string, 
/**
 * The version the edit was made **against**, not the one to write:
 * the adapter sends `base_version + 1` and Confluence aborts if
 * somebody else got there first (#286).
 */
base_version: number, 
/**
 * The page's **whole** storage format. Confluence's content `PUT`
 * replaces the record, so a body carrying only the edited section
 * would delete the rest of the page -- `DocWrites::edit_section`
 * is what composes this.
 */
body: string, } } | { "PauseMonitor": { 
/**
 * The **monitor** (`kuma:8`). No second field: Uptime Kuma's
 * `pauseMonitor` takes a monitor id and nothing else, so a duration
 * or a reason here would be something knobas asked a reader for and
 * threw away.
 */
entity: string, } } | { "ResumeMonitor": { 
/**
 * The paused **monitor**, to start checking again.
 */
entity: string, } } | { "CreateMonitor": { 
/**
 * What the create targets: the source that will make it, as
 * `assets::MonitorTarget.entity` gives it. Never composed by a
 * caller (#453).
 */
entity: string, 
/**
 * What the monitor is called, and therefore what attaches it: the
 * next poll mirrors it, and `knobas_sync::attach` draws the
 * `monitored-by` link to every asset carrying this name.
 */
name: string, 
/**
 * What it checks, spelled as `kind` reads it: an http(s) URL,
 * `host:port`, a host, or a container's name (#864).
 */
url: string, 
/**
 * The kind of check; absent is `http`, what every create meant
 * before #864.
 */
kind?: MonitorKind, } } | { "MergePullRequest": { 
/**
 * The **pull request** (#667).
 */
entity: string, 
/**
 * How the merge lands; a word outside the three does not decode.
 */
style: MergeStyle, 
/**
 * Delete the head branch once merged (#708); absent reads as
 * `false`.
 */
delete_branch: boolean, 
/**
 * The commit message the reader wrote (#843): first line the title;
 * absent or `null` keeps the source's default.
 */
message?: string | null, } } | { "SetParameter": { 
/**
 * The **build configuration** (#668).
 */
entity: string, 
/**
 * The parameter's full name. On an inherited parameter the source
 * creates an own value that overrides it.
 */
name: string, value: string, 
/**
 * The value the edit was made against; the source refuses the
 * write when the parameter no longer holds it. `null` for a secret,
 * whose value is never drawn and whose write is last-write-wins, and
 * for a parameter that did not exist.
 */
base: string | null, } } | { "RunBuild": { 
/**
 * The **build configuration** (#669).
 */
entity: string, 
/**
 * The source's name for the branch; `null` for the default branch.
 */
branch: string | null, 
/**
 * Only the values the reader changed, for this build alone.
 */
parameters: Array<BuildParameter>, 
/**
 * Check the sources out anew (#852); absent reads as `false`.
 */
clean_checkout?: boolean, 
/**
 * Put the run at the top of the source's queue (#852); absent reads
 * as `false`.
 */
first_in_queue?: boolean, } } | { "SubmitReview": { 
/**
 * The **pull request** (#705).
 */
entity: string, verdict: ReviewVerdict, body: string, 
/**
 * The line comments that go with the review; empty for a review of
 * the whole change.
 */
comments: Array<ReviewComment>, } } | { "DeleteBranch": { 
/**
 * The **branch** (#707); the source refuses the default branch and a
 * protected one.
 */
entity: string, } } | { "EditPullRequest": { 
/**
 * The **pull request** (#706). A field below left `null` is not
 * written, so only what the reader changed reaches the source.
 */
entity: string, title: string | null, body: string | null, state: PullRequestState | null, } } | { "UpdatePullRequestBranch": { 
/**
 * The **pull request** (#708).
 */
entity: string, style: UpdateStyle, } } | { "CommitFile": { 
/**
 * The **repository** (#822), this op's queue target.
 */
repo: string, branch: string, 
/**
 * Makes this branch from `branch` first; `null` commits onto
 * `branch`.
 */
new_branch: string | null, path: string, 
/**
 * The whole new text, never a patch.
 */
content: string, message: string, 
/**
 * The blob the edit was made against. A file that changed since
 * comes back held, theirs in the held snapshot (`text`, `base`) and
 * this payload as the reader's. *Apply anyway* re-sends against
 * `base` by itself; an *Edit* (amend) of a moved commit sends
 * `base_sha` set to the held side's `base`, or it is held again.
 */
base_sha: string, } } | { "SourceAction": { entity: string, 
/**
 * The adapter's own name for the action (#1194, ADR-0029).
 */
id: string, 
/**
 * The answers of the dialog the adapter declared, by field name. The
 * shell copies `id` and these from the action and never reads them.
 */
fields: { [key in string]: string }, } };

/**
 * What the queue will do about a write next.
 *
 * The queue panel branches on this and so does the shell badge, so the
 * three open states are three different sentences, not three shades of
 * "waiting":
 *
 * - `pending`: needs **patience**. Retried automatically, and it goes on
 *   its own when the source can take it.
 * - `held`: needs a **decision**. The target changed after the write was
 *   queued. Waits for the user and for nothing else: no timeout, no
 *   auto-apply, no auto-discard. Since migration `0012` a target also
 *   leaves the view when the user turns its source off, and that write
 *   holds by the same mechanism. [`QueuedWrite::source_enabled`] is what
 *   tells the two apart -- the state stays one word because the queue's
 *   answer is the same (nothing sends until someone acts); only the
 *   explanation differs.
 * - `refused`: needs a **decision**. The source rejected the write
 *   outright. Deliberately never retried -- a refusal is a decision, not
 *   a blip (ADR-0004 is what tells the two apart).
 * - `sent`: delivered. Terminal.
 * - `discarded`: withdrawn by the user. Terminal.
 *
 * `sent` and `discarded` are settled and never appear in
 * `pending_writes`.
 *
 * Stored as lowercase text in `knobas.write_queue.state`, whose
 * `write_queue_state_chk` (migration 0005) allows exactly these
 * spellings -- and `ALL` is what the test that pins the two together
 * walks.
 *
 * The states are documented here and not on each variant because the
 * generated TypeScript (`app/src/lib/ipc/generated/sources.ts`) carries a
 * type's and a field's doc but not a variant's (#1325).
 */
export type WriteState = "pending" | "held" | "refused" | "sent" | "discarded";
