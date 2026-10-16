/** Entity and room reads — `crates/knobas-app/src/commands/entity.rs`. */
import { invoke } from "@tauri-apps/api/core";

// The write queue's own types live with the queue's other commands. A
// write-back is an operation on an entity, which is why `submitWrite` is here;
// what it hands over and what comes back are the queue's, and a second
// declaration of either would be two shapes for one wire format.
import type { BuildParameter, QueuedWrite, ReviewSide, Trigger, WriteOpPayload, WriteState } from "./sources";
// A day is a day wherever it is asked for: the digest takes the same
// `DayWindow` the timesheet does rather than declaring a second shape for one
// wire format, which is the rule the import above follows for the queue's.
import type { DayWindow } from "./time";

/**
 * One line in a room — `knobas_app::commands::entity::EntityRow`.
 *
 * Deliberately not the whole mirror row: `body_text` and `payload` belong to
 * the detail view, and a tile that fetched them would pull the corpus into the
 * webview to draw five columns of it.
 */
export interface EntityRow {
  /** `"<namespace>:<key>"` — `mock:PAY-231`. The kind is not part of it. */
  entity_id: string;
  kind: string;
  source_id: string;
  /** Raw source text. Render as text, never as markup (gotcha 7). */
  title: string;
  /**
   * RFC 3339, or `null`.
   *
   * The source's own timestamp — a source that reports none leaves it null;
   * never `now()` (interfaces §4.1 normalization). A row that has never been
   * dated is therefore distinguishable from one changed this second, and the
   * room sorts it last rather than first.
   */
  updated_at: string | null;
  /** RFC 3339. When knobas last saw it — knobas' own clock, always known. */
  synced_at: string;
  /**
   * Where the row sits **inside its source**, as one line — a Confluence
   * page's ancestor path, `Engineering › Payments` (#284) — and, since #596,
   * the parent chain any kind declares (ADR-0018): `owner/repo` for a Gitea
   * branch, pull request or commit.
   *
   * `null` for every row whose kind declares no parent and whose record
   * carries no readable `ancestors`: the ADR-0007 **miss**, so a row shows no
   * path rather than a wrong one and no caller has to ask what kind it holds.
   * Joined by one SQL expression, `knobas_core::path_line_read!`, so the
   * launcher row and the detail panel cannot disagree about what a path is.
   */
  path: string | null;
  /**
   * The item's state chip — a pull request's `open` / `merged` / `closed`, a
   * build's `running` or `FAILURE` (#666) — or `null` for a kind whose adapter
   * declares no state, and for a value its declaration does not name (the
   * ADR-0007 **miss**). Resolved in SQL by `knobas_core::declared_state!`
   * beside the path line. `text` is source text: render it as text.
   */
  state: StateChip | null;
}

/**
 * How loud a value is drawn — `knobas_core::tone::Tone`. The adapter says
 * which; the shell owns the colours.
 */
export type Tone = "ok" | "warn" | "err" | "info" | "muted";

/** A row's state chip — `knobas_core::payload::StateChip`. */
export interface StateChip {
  text: string;
  tone: Tone;
}

/**
 * Which ordering a room reads in — `EntityOrder`.
 *
 * The last two are the room tiles' (#680, `CONTEXT.md` **Tile**), driven by
 * each row's declared state and its tone: `attention` drops settled rows (tone
 * `ok` or `muted`) and draws the rest loudest first; `latest_per_parent`
 * keeps the newest row of each declared parent, for kinds that declare a
 * state, and draws every tone loudest first. A row with no state sorts by
 * recency under both. Both read the live corpus only.
 */
export type EntityOrder = "updated_desc" | "title_asc" | "attention" | "latest_per_parent";

/**
 * Which slice of the corpus a room wants — `EntityFilter`.
 *
 * Both lists are *unfiltered when empty*. The backend binds an empty list as
 * SQL `NULL` for exactly that reason; do not "helpfully" send every known id
 * instead.
 */
export interface EntityFilter {
  /** Source ids to include; `[]` means every source. */
  sources: string[];
  /** Kinds to include; `[]` means every kind. */
  kinds: string[];
  /**
   * Only items the source dated within this many days, or `null` for no
   * window. Items the source never dated fall outside every window.
   */
  updated_within_days: number | null;
  order: EntityOrder;
  /**
   * Only members of this stored context (`ctx:<id>`), by the fixed rule
   * (§16.11, ADR-0008: seed, direct links, one hop, and what the assets
   * among them hold) — or `null` for no scoping. Membership is
   * resolved server-side per read, so a page and its `total` always describe
   * the same instant.
   */
  context: string | null;
  /**
   * Only items carrying this project key, in the source's own word (ADR-0010,
   * #208) — or `null` for no scoping.
   *
   * It narrows **within** `sources`, never instead of them: a project key is
   * unique only inside its own source, so a project room sets both. An item
   * whose mirrored record names no project knobas can read matches no value
   * here — it is in no project room, and still in *All work* and its source's
   * room.
   */
  project: string | null;
  /** Reach past the live-item view for entities withdrawn upstream (§5a). */
  include_deleted: boolean;
  /**
   * Only what this ticket (an entity id) reaches — the job-reach read, #817.
   * Absent or `null` is unscoped; with {@link context} too, a row must be in
   * both.
   */
  reach?: string | null;
}

/** One page of a room — `EntityPage`. */
export interface EntityPage {
  rows: EntityRow[];
  /** The whole filtered set, before `limit`/`offset`. `0` for an empty page. */
  total: number;
  /**
   * Under a tile order (`attention`, `latest_per_parent`), the whole set
   * counted by kind and tone, which a tile's header phrases; `null` under the
   * plain orders (#680).
   */
  tally: KindTally[] | null;
  /**
   * Under `attention`, in a room where some row's source has a configured
   * identity, the Code tile's non-empty groups in draw order with their counts
   * (#712); the rows arrive group by group. `null` otherwise, and the tile
   * then draws one list as before.
   */
  groups: GroupTally[] | null;
}

/**
 * Which Code-tile group a row is drawn under — `AttentionGroup` (#712): what
 * needs me first, in this order.
 */
export type AttentionGroup =
  | "review_requested"
  | "my_pull_requests"
  | "open_pull_requests"
  | "my_branches"
  | "branches"
  | "other_branches";

/** How many rows one group holds — `GroupTally`. */
export interface GroupTally {
  group: AttentionGroup;
  count: number;
}

/** How many rows of one kind, in one tone, a tile order drew — `KindTally`. */
export interface KindTally {
  kind: string;
  /** The tone of the rows' state chip; `null` for rows that have none. */
  tone: Tone | null;
  count: number;
}

/** One page of the room `filter` addresses, newest (or first) `limit` rows. */
export function listEntities(
  filter: EntityFilter,
  limit: number,
  offset: number,
): Promise<EntityPage> {
  return invoke<EntityPage>("list_entities", { filter, limit, offset });
}

/** One line of the activity log — `knobas_core::activity::ActivityRow`. */
export interface ActivityRow {
  id: number;
  /** RFC 3339 timestamp. */
  at: string;
  /** `"user"`, or `"sync:<source_id>"`. */
  actor: string;
  verb: string;
  entity_id: string | null;
  /**
   * Free-form jsonb — `serde_json::Value`, so §2.6's mapping is `unknown`.
   *
   * Not `Record<string, unknown>`: the Rust type is any JSON value, and only
   * `activity::insert` coerces a JSON `null` to `{}`. A row written with a
   * string, a number or an array is well-typed on the Rust side and would make
   * the narrower declaration a lie — the kind that type-checks in the frontend
   * and throws at runtime. Narrow it with a check where you read it.
   */
  detail: unknown;
}

/**
 * The `limit` most recent activity lines, newest first.
 *
 * `entityId` scopes the read to one entity's history (the detail view's
 * History panel); omitting it is the global stream the status bar reads. A
 * malformed id rejects with `invalid`, not `internal`.
 */
export function recentActivity(limit: number, entityId?: string): Promise<ActivityRow[]> {
  return invoke<ActivityRow[]>("recent_activity", { limit, entityId });
}

/** Which source an entity came from — `SourceRef`. */
export interface SourceRef {
  id: string;
  /**
   * The configured display name, falling back to the id. `run_once` syncs
   * sources that were never configured, so there is not always a name.
   */
  display_name: string;
  /** The *adapter* kind (`jira`, `mock`), not the instance id. */
  adapter_kind: string;
  /**
   * Whether the user has this source turned on, as of this read — derived by
   * the backend, never stored (issue #204).
   *
   * The detail opens entities every other reader hides, and there are two
   * reasons a reader hides one: `deleted_at` marks "withdrawn upstream",
   * `enabled: false` marks "you turned the source off". Draw a banner for
   * each — they are different facts with different remedies (nothing undoes
   * a withdrawal; re-enabling the source undoes a disable, and this flag
   * flips on the next read with no re-sync). A source that was never
   * configured reads `true`: absence of configuration is not a decision.
   */
  enabled: boolean;
}

/**
 * An adapter's metadata for one kind — `knobas_source::KindInfo`: how to
 * display it, and whether a full sync of it is exhaustive.
 */
export interface KindInfo {
  id: string;
  label: string;
  plural: string;
  /** Two characters, e.g. `"PR"`. */
  monogram: string;
  /**
   * Whether a full sync returns every item **of this kind** — the sync
   * engine's tombstone sweep runs only for kinds that say so (ADR-0003).
   * Declared per kind, not per source: one adapter can enumerate its
   * repositories exhaustively while budgeting its commits.
   */
  full_sync_exhaustive: boolean;
}

/** One link — `knobas_core::link::LinkRow`. */
export interface LinkRow {
  id: string;
  from_id: string;
  to_id: string;
  /**
   * What kind of link this is — `"related"` unless one was named.
   *
   * Always **lower case**: the backend folds it on write, so `Blocks` and
   * `blocks` are one relation and one panel group rather than two. Key the
   * curated list and the inverse-label lookup on this spelling; capitalize
   * for display from the whole string.
   */
  relation: string;
  origin: "manual" | "suggested" | "imported" | "source" | "implied";
  /**
   * Why the link exists, in the user's own words, or `null`.
   *
   * `null` and not `""`: “no reason recorded” and “a reason recorded as
   * nothing” are different facts, and only one of them is worth a line.
   */
  note: string | null;
  created_by: string;
  /** RFC 3339. */
  created_at: string;
  /**
   * RFC 3339 when the user accepted this link, else `null` — and `null` is the
   * whole difference between a link and a **suggestion**.
   *
   * A row with `confirmed_at: null` is a proposal knobas made and nobody has
   * accepted. It is never in `EntityDetail.links` (that read is the confirmed
   * view) and it is the only thing `roomSuggestions` returns (that read is the
   * proposed view). The two cannot overlap: their predicates are each other's
   * negation over the same rows.
   *
   * Every link drawn by hand is confirmed the moment it is written.
   */
  confirmed_at: string | null;
  /**
   * The named detection rule that proposed this link, or `null` for one a
   * person drew — `"branch_name_key"`, `"similar_text"`, …
   *
   * Deliberately not a union: rules grow, and a closed list here would be a
   * frontend that has to ship before a new detector can. Branch on
   * `rule_class`, and show `rule` only as provenance.
   */
  rule: string | null;
  /**
   * Which class of evidence `rule` is, or `null` for a link a person drew.
   *
   * This is the axis a reader calibrates trust on and the one a badge should
   * show: `exact_key` is "the two ends name each other", `similarity` is a
   * guess from overlapping text, `source_relation` is a relation the source
   * system already states.
   */
  rule_class: "exact_key" | "similarity" | "source_relation" | null;
  /**
   * Why knobas proposed this link, in the detector's own words — "the branch
   * name contains PAY-231" — or `null` for a link a person drew.
   *
   * Stored, never re-rendered from `rule`: a suggestion whose reason cannot be
   * shown is not shippable, and the database refuses an unconfirmed row that
   * has no reason. Raw text; render as text.
   */
  reason: string | null;
}

/**
 * The end of a link the reader is **not** on — `knobas_core::link::LinkEnd`.
 *
 * An id is not something a person recognises, so the panel draws these instead
 * of `to_id`: the kind, the title, and whether the entity was withdrawn
 * upstream.
 */
export interface LinkEnd {
  /** `"<namespace>:<key>"` — the address a row navigates to. */
  entity_id: string;
  kind: string;
  /** Raw source text. Render as text, never as markup (gotcha 7). */
  title: string;
  /**
   * RFC 3339 when the source withdrew this entity, else `null`.
   *
   * Not a reason to hide the row — the opposite. §5a keeps the entity so a
   * link never dangles, and the panel marks it instead.
   */
  deleted_at: string | null;
}

/**
 * One link as the panel draws it — `knobas_core::link::LinkEntry`.
 *
 * Which end `other` holds depends on whose detail was read: `A → B` hydrates
 * `B` on A's panel and `A` on B's. Direction and the inverse-label wording are
 * the frontend's (`detail/relations.ts`), computed from `link.from_id` against
 * the entity being viewed.
 */
export interface LinkEntry {
  link: LinkRow;
  other: LinkEnd;
}

/** Everything the detail slide-over draws — `EntityDetail` (interfaces §2.5). */
export interface EntityDetail {
  row: EntityRow;
  source: SourceRef;
  /**
   * The adapter's label and monogram for this kind, or `null`.
   *
   * Resolved from the adapter registry by the source's `adapter_kind` (it was
   * `null` throughout M1 phase 1, before the registry existed). `null` for a
   * kind the adapter does not declare, and for a source with no configuration
   * row at all — which `run_once` produces. The header falls back to the
   * title-cased kind, which is what §3a asks for when no adapter declares one
   * anyway.
   */
  kind_info: KindInfo | null;
  /** Untrusted source text — render as text, never as markup (gotcha 7). */
  body_text: string;
  author: string | null;
  /**
   * The source record verbatim (§3a).
   *
   * `unknown`, per §2.6's `serde_json::Value` mapping — and untrusted source
   * text all the way down: project it as text, never as markup (interfaces
   * §2.5, gotcha 7).
   */
  payload: unknown;
  /**
   * Where the item lives in its own system, or `null` when the adapter
   * reported no page — in which case *Open in browser* is absent (P5).
   */
  web_url: string | null;
  /**
   * RFC 3339 when the source withdrew the entity, else `null`. The mirror row
   * survives so links and notes still resolve (§5a).
   */
  deleted_at: string | null;
  /**
   * The entity's confirmed links, newest first — undirected, so a link drawn
   * from either end appears on both, each with the other end already resolved.
   * `createLink`/`unlink` are what move it.
   */
  links: LinkEntry[];
  /** This entity's own history, newest first (spec §12.1). */
  activity: ActivityRow[];
  /**
   * The mirrored builds that ran on the commit this entity stands at (#710):
   * a pull request's head, a branch's tip, a commit itself. Failing first,
   * then running, then the rest by tone, newest first within one, at most 20;
   * empty for anything that is not code and for a commit nothing mirrored ran
   * on.
   */
  builds: EntityRow[];
  /**
   * A ticket's status as its source declares it (#760), or `null` — for
   * anything that is not a ticket, for a ticket whose record carries none (the
   * board's "No status" group), and for a withdrawn ticket. The status select
   * shows it as the ticket's current one; it is the board's own rule, read
   * once in `get_entity`, so the panel never reads a card list to find it.
   */
  status: string | null;
  /**
   * A ticket's type in its source's own word (#816, `Story`), through the
   * declared `issue_type` path, or `null` on a miss. The job card draws it.
   */
  issue_type: string | null;
  /** A ticket's priority in its source's own word (#816), or `null` on a miss. */
  priority: string | null;
  /**
   * A ticket's original estimate in seconds (#816), or `null`: nothing
   * declared, nothing recorded, or not a whole number. Never `0` for a miss.
   */
  estimate_seconds: number | null;
}

/**
 * One entity, deleted or not.
 *
 * Rejects with `invalid` for something that is not an entity id and
 * `not_found` for one nothing carries — a deep link into a corpus that has not
 * synced yet is a normal event, not a bug.
 */
export function getEntity(entityId: string): Promise<EntityDetail> {
  return invoke<EntityDetail>("get_entity", { entityId });
}

/**
 * One run of a pipeline — `knobas_app::commands::entity::RunRow` (#848). Every
 * field is read through the run kind's declared paths; `started` and
 * `finished` are RFC 3339, `null` while queued or in flight.
 */
export interface RunRow {
  entity_id: string;
  /** In the source's own spelling; a queued run may have none yet. */
  number: string | null;
  state: StateChip | null;
  branch: string | null;
  started: string | null;
  finished: string | null;
  /** Whole seconds, only where both instants were read. */
  duration_seconds: number | null;
  /** The source's own timestamp, what runs are ordered by. */
  updated_at: string | null;
  web_url: string | null;
  /** How it was started (#961), or `null` where the declaration misses. */
  trigger: Trigger | null;
  /** Who started it, as the source spells the account; none for a change or a schedule. */
  started_by: string | null;
  /** Whether `started_by` is one of the configured usernames (*Started by me*, *you*). */
  started_by_me: boolean;
  /** The commit it built, a full sha: the first declared revision. */
  revision: string | null;
}

/**
 * One pipeline of the Builds list — `knobas_app::commands::entity::PipelineRow`
 * (#848). `source_name` is the one place the source's word appears (ADR-0022);
 * `recent` is the last twelve runs, newest first.
 */
export interface PipelineRow {
  entity_id: string;
  source_id: string;
  source_name: string;
  name: string;
  group: string | null;
  /** The chain of groups it sits in, outermost first (#849); empty when none is declared. */
  path: string[];
  /** Whether the reader pinned it (#849), knobas-local. */
  pinned: boolean;
  web_url: string | null;
  recent: RunRow[];
}

/** Every pipeline the configured sources declare, by name (#848). */
export function pipelines(): Promise<PipelineRow[]> {
  return invoke<PipelineRow[]>("pipelines");
}

/**
 * One pipeline's runs, newest first, at most `limit` (100 when omitted).
 * Rejects with `invalid` for a non-id and `not_found` for no pipeline.
 */
export function pipelineRuns(pipelineId: string, limit?: number): Promise<RunRow[]> {
  return invoke<RunRow[]>("pipeline_runs", { pipelineId, limit: limit ?? null });
}

/**
 * The files of pull request `prId` the reader marked *Viewed* on its current
 * head (#842, R-8.17), in path order — `pr_viewed_files`. Knobas-local, kept
 * by pull request and the head commit its source declares: once the mirror
 * reads another head they read as none.
 */
export function prViewedFiles(prId: string): Promise<string[]> {
  return invoke<string[]>("pr_viewed_files", { prId });
}

/** Mark one file of a pull request *Viewed* or not on its current head (#842), answering the files now viewed. */
export function setPrFileViewed(prId: string, path: string, viewed: boolean): Promise<string[]> {
  return invoke<string[]>("set_pr_file_viewed", { prId, path, viewed });
}

/** Pin or unpin a pipeline (#849); answers whether it is now pinned. */
export function setPipelinePinned(pipelineId: string, pinned: boolean): Promise<boolean> {
  return invoke<boolean>("set_pipeline_pinned", { pipelineId, pinned });
}

/**
 * Why a pipeline is on a job — `knobas_app::commands::entity::PipelineReason`
 * (#850): a run on one of its branches, a run checking one of its pull
 * requests, or a link the reach walked. Tagged on `kind`.
 */
export type PipelineReason =
  | { kind: "branch"; branch: string }
  | { kind: "pr_check"; pr: string }
  | { kind: "linked"; step: string; origin: ReachOrigin };

/** One pipeline on a job and every reason it is there (#850). */
export interface JobPipeline {
  pipeline_id: string;
  reasons: PipelineReason[];
}

/** The pipelines on the job of a ticket, by entity id (#850). */
export function jobPipelines(entityId: string): Promise<JobPipeline[]> {
  return invoke<JobPipeline[]>("job_pipelines", { entityId });
}

/** What Code draws beside one mirrored row — `knobas_app::commands::entity::CodeFacts` (#966). */
export interface CodeFacts {
  entity_id: string;
  /** Its author as the source spells the account (a pull request's opener, a commit's or branch tip's author), or `null`. */
  author: string | null;
  /** Whether `author` is its source's configured username, as the attention read compares it (#712). */
  mine: boolean;
  /** A repository's declared language, or `null` on a miss. */
  language: string | null;
  /** A repository's declared teams, in the record's order; empty on a miss. */
  teams: string[];
}

/** {@link CodeFacts} for each id the mirror holds live; the others are left out (#966). */
export function codeFacts(entityIds: string[]): Promise<CodeFacts[]> {
  return invoke<CodeFacts[]>("code_facts", { entityIds });
}

/**
 * Runs of a pipeline older than the run `before`, asked of the source: the
 * history beyond the mirror's window (#851), newest first, at most `limit`
 * (50 when omitted). Empty is the start of the history. Rejects as
 * {@link pipelineRuns} does, and with the source's own failure.
 */
export function olderPipelineRuns(pipelineId: string, before: string, limit?: number): Promise<RunRow[]> {
  return invoke<RunRow[]>("older_pipeline_runs", { pipelineId, before, limit: limit ?? null });
}

/** One repository a pipeline builds — `knobas_source::pipeline::PipelineRepository` (#851). */
export interface PipelineRepository {
  name: string;
  /** Where it fetches from, never with a credential. */
  url: string | null;
  default_branch: string | null;
  /** The mirrored repository, where the mirror holds it. */
  entity_id: string | null;
}

/** One trigger of a pipeline — `knobas_source::pipeline::PipelineTrigger` (#851). */
export interface PipelineTrigger {
  /** Its kind in the source's word (`VCS`, `Schedule`). */
  name: string;
  facts: Fact[];
  disabled: boolean;
}

/**
 * A pipeline's settings, read live — `knobas_source::pipeline::PipelineSettings`
 * (#851). A secret parameter's value is never sent.
 */
export interface PipelineSettings {
  repositories: PipelineRepository[];
  triggers: PipelineTrigger[];
  parameters: Setting[];
  /** The write op a parameter's default changes through, or `null`. */
  parameter_write_op: string | null;
  /** How each parameter is asked for in Run…, one per parameter in order (#852). */
  parameter_specs: ParameterSpec[];
  /** The chain the pipeline is a step of, itself included, in running order; empty for none (#857). */
  chain: ChainStep[];
}

/** One pipeline of a chain — `knobas_source::pipeline::ChainStep` (#857). */
export interface ChainStep {
  /** The pipeline's entity id. */
  pipeline: string;
  /** What the source calls it. */
  name: string;
}

/**
 * One step of the chain a run is in — `knobas_app::commands::entity::ChainStepRow`
 * (#857): its run on the run's branch, or `null` where it has not run there.
 */
export interface ChainStepRow {
  pipeline_id: string;
  name: string;
  run: RunRow | null;
}

/** The chain a run is in, read live, in running order; empty for none (#857). */
export function runChain(runId: string): Promise<ChainStepRow[]> {
  return invoke<ChainStepRow[]>("run_chain", { runId });
}

/** How one parameter is asked for — `knobas_source::pipeline::ParameterSpec` (#852). */
export interface ParameterSpec {
  name: string;
  /** What the source shows instead of the name. */
  label: string | null;
  /** What the source says about the value, such as what empty means. */
  hint: string | null;
  kind: ParameterKind;
}

/** The field a parameter is asked with — `knobas_source::pipeline::ParameterKind`. */
export type ParameterKind =
  | { type: "text" }
  | { type: "select"; options: ParameterOption[] }
  | { type: "checkbox"; checked: string; unchecked: string };

/** One value a select offers — `knobas_source::pipeline::ParameterOption`. */
export interface ParameterOption {
  value: string;
  label: string | null;
}

/**
 * One Run… preset — `knobas_app::run_presets::RunPreset` (#852): what a run
 * of one pipeline starts from, saved under a name. Knobas-local.
 */
export interface RunPreset {
  pipeline_id: string;
  name: string;
  /** `null` is the default branch. */
  branch: string | null;
  /** The overrides only, in order. */
  parameters: BuildParameter[];
  clean_checkout: boolean;
  first_in_queue: boolean;
}

/** A pipeline's Run… presets, in the order they were first saved (#852). */
export function runPresets(pipelineId: string): Promise<RunPreset[]> {
  return invoke<RunPreset[]>("run_presets", { pipelineId });
}

/** Save a Run… preset, replacing the pipeline's preset of that name (#852). */
export function saveRunPreset(preset: RunPreset): Promise<RunPreset> {
  return invoke<RunPreset>("save_run_preset", { preset });
}

/** One pipeline's settings, read live; `null` for a source with no pipelines (#851). */
export function pipelineSettings(pipelineId: string): Promise<PipelineSettings | null> {
  return invoke<PipelineSettings | null>("pipeline_settings", { pipelineId });
}

/** A range of a run's log — `knobas_source::run::RunLog` (#854). */
export interface RunLog {
  lines: LogLine[];
  /** The run's steps over the whole log, whether or not their lines are here. */
  steps: LogStep[];
  /** How many lines the whole log has. */
  total: number;
}

/** One log line — `knobas_source::run::LogLine`. */
export interface LogLine {
  /** Its place in the whole log, from 1. */
  number: number;
  /** The source's stamp, as written (`10:06:12`). */
  time: string | null;
  text: string;
  /** `err` or `warn` where the source marked it; `null` otherwise. */
  tone: Tone | null;
  /** Index into `RunLog.steps`; `null` outside every step. */
  step: number | null;
}

/** One step, as a range of the log's lines — `knobas_source::run::LogStep`. */
export interface LogStep {
  name: string;
  first: number;
  last: number;
  seconds: number | null;
  tone: Tone | null;
}

/** A run's tests — `knobas_source::run::RunTests` (#855). */
export interface RunTests {
  /** Failed first. */
  tests: RunTest[];
  /** The run has more tests than these. */
  truncated: boolean;
}

/** How a test came out — `knobas_source::run::TestStatus`. */
export type TestStatus = "failed" | "passed" | "ignored";

/** One test of a run — `knobas_source::run::RunTest`. */
export interface RunTest {
  /** The source's id for the test across runs, for its history. */
  id: string | null;
  name: string;
  module: string | null;
  status: TestStatus;
  muted: boolean;
  new_failure: boolean;
  duration_ms: number | null;
  /** The assertion and stack, as the source wrote them. */
  details: string | null;
}

/** One test's outcome in one run — `knobas_source::run::TestOutcome` (#855). */
export interface TestOutcome {
  /** The run's entity id. */
  run: string;
  number: string | null;
  branch: string | null;
  status: TestStatus;
}

/** One entry of an artifact folder — `knobas_source::run::ArtifactEntry` (#856). */
export interface ArtifactEntry {
  /** From the artifacts' root, `/`-separated. */
  path: string;
  name: string;
  /** Bytes; `null` for a folder. */
  size: number | null;
  folder: boolean;
  /** How many entries a folder holds, where the source says. */
  children: number | null;
}

/** The head of a text artifact — `knobas_source::run::ArtifactPreview` (#856). */
export interface ArtifactPreview {
  /** `null` for a file that is not text. */
  text: string | null;
  truncated: boolean;
}

/** What one download streams — `knobas_source::run::DownloadTarget`, tagged on `kind`. */
export type DownloadTarget =
  | { kind: "log" }
  | { kind: "file"; path: string }
  | { kind: "folder"; path: string }
  | { kind: "all" };

/**
 * A range of a run's whole log and its steps (#854): `count` lines (the cap
 * when omitted) from line `from`, or the last `count` when `from` is `null`.
 * `null` for a source that keeps no runs.
 */
export function runLog(runId: string, from: number | null = null, count: number | null = null): Promise<RunLog | null> {
  return invoke<RunLog | null>("run_log", { runId, from, count });
}

/** A run's tests, failed first (#855). */
export function runTests(runId: string): Promise<RunTests | null> {
  return invoke<RunTests | null>("run_tests", { runId });
}

/** How one test came out in the last ten runs of the run's pipeline, newest first (#855). */
export function testHistory(runId: string, testId: string): Promise<TestOutcome[] | null> {
  return invoke<TestOutcome[] | null>("test_history", { runId, testId });
}

/** One folder of a run's artifacts; `null` path is the root (#856). */
export function runArtifacts(runId: string, path: string | null = null): Promise<ArtifactEntry[] | null> {
  return invoke<ArtifactEntry[] | null>("run_artifacts", { runId, path });
}

/** The head of one text artifact, for *Preview* (#856). */
export function artifactPreview(runId: string, path: string): Promise<ArtifactPreview | null> {
  return invoke<ArtifactPreview | null>("artifact_preview", { runId, path });
}

/**
 * Stream a log, file, folder or every artifact into the downloads folder
 * with the stored credential (D8, #856); the saved file's path, or `null`
 * for a source that keeps no runs.
 */
export function downloadRunFile(runId: string, target: DownloadTarget): Promise<string | null> {
  return invoke<string | null>("download_run_file", { runId, target });
}

/**
 * What a pasted URL named — `knobas_app::commands::entity::UrlMatch` (#496).
 *
 * `kind` rides along because an entity id names a namespace and a key, not a
 * kind, and the address an entity opens at is built from both.
 */
export interface UrlMatch {
  entity_id: string;
  kind: string;
}

/**
 * The entity a pasted URL names, or `null` for a link the mirror does not
 * hold — `resolve_url` (#496, spec #491 stories 9-11 and 15-17).
 *
 * One read on the local mirror and **never a fetch from a source**: a paste
 * while the tunnel is down is a miss and not a hang (story 16). Matching is
 * exact on the stored `web_url` after both sides are normalised — fragment
 * out, the path's trailing slash out, scheme and host down-cased, query kept
 * verbatim — so a Confluence `viewpage.action?pageId=` link resolves and a
 * different `pageId` does not.
 *
 * Rejects with `invalid` for anything that is not an absolute `http`/`https`
 * URL. That is not a miss: a miss is *the mirror does not hold this link*, and
 * it is the only outcome that earns *Open in browser*, which refuses every
 * other scheme anyway (`lib/shell/open-external.ts`).
 */
export function resolveUrl(url: string): Promise<UrlMatch | null> {
  return invoke<UrlMatch | null>("resolve_url", { url });
}

/**
 * Draw a link between two entities — `knobas_app::commands::entity::create_link`.
 *
 * `relation` defaults to `"related"` and is folded to lower case; `note` is
 * optional and kept as typed — it is prose, not a group key. Both are
 * normalized backend-side, so a field the user left alone may be sent as `""`.
 * The origin is always `"manual"` and is deliberately not suppliable: the
 * other origins belong to the suggestion engine and to import.
 *
 * There is no matching read: an entity's links arrive with `getEntity`, and
 * they are undirected, so a link drawn from either end is on both.
 *
 * Rejects with `invalid` for an id that is not an entity id or for an entity
 * linked to itself, `not_found` when an endpoint is not in the local mirror —
 * it has not synced yet — and `conflict` when that pair is already linked
 * under that relation ("already linked"). Emits `EVENTS.activityNew`.
 *
 * The `conflict` is **directed**: the same pair linked the other way round is
 * not refused, and shows as a second row on both ends. Known — see #70.
 */
export function createLink(
  fromId: string,
  toId: string,
  relation?: string,
  note?: string,
): Promise<LinkRow> {
  return invoke<LinkRow>("create_link", { fromId, toId, relation, note });
}

/**
 * Withdraw a link — `knobas_app::commands::entity::unlink`.
 *
 * The row is kept, tombstoned, so the removal is remembered and the same pair
 * can be linked again afterwards. Idempotent: withdrawing an already-withdrawn
 * link resolves, writes no second activity line and emits nothing.
 *
 * Rejects with `invalid` for something that is not a link id and `not_found`
 * for one nothing carries. Emits `EVENTS.activityNew`.
 */
export function unlink(linkId: string): Promise<void> {
  return invoke<void>("unlink", { linkId });
}

/**
 * One note — `knobas_core::note::NoteRow`.
 *
 * Notes are the first kind knobas *owns* rather than mirrors: nothing synced
 * this and nothing can re-fetch it, which is why it is in the backup and never
 * written to a source.
 */
export interface NoteRow {
  /** `"note:<uuid>"` — also this note's entity id, so it links like anything else. */
  id: string;
  /** Never blank: a note the user has not named is "Untitled note". */
  title: string;
  /**
   * The markdown the user typed.
   *
   * Untrusted text — render as text, never as markup (gotcha 7). It is the
   * user's own rather than a source's, which changes who is at fault and
   * nothing else. There is no `{@html}` anywhere in this app; a `[[ref]]`
   * becomes a chip by parsing the body into tokens and drawing them, not by
   * turning the string into HTML.
   */
  body_md: string;
  /** RFC 3339. */
  created_at: string;
  /** RFC 3339. Moves on every save. */
  updated_at: string;
}

/**
 * One `[[ref]]` a note's body names — `knobas_core::note::NoteRef`.
 *
 * `target` is `null` for an **unresolved** ref: nothing carries that id, so it
 * creates no link and is drawn as unresolved rather than as plain text — that
 * is how a typo is discoverable. A `target` whose `deleted_at` is set is the
 * other case entirely: the ref resolves, the link exists, and the chip is
 * marked withdrawn.
 */
export interface NoteRef {
  /** The text between the brackets, exactly as the body wrote it. */
  target_id: string;
  target: LinkEnd | null;
}

/**
 * Everything the note view draws — `knobas_app::commands::entity::NoteDetail`.
 *
 * `refs` and `links` overlap on purpose and answer different questions.
 * `refs` is the body's own list, in body order, including the ones that
 * resolve to nothing. `links` is the same panel `getEntity` fills: every link
 * the note takes part in, in either direction, so a note shows what points at
 * it as well as what it points at.
 */
export interface NoteDetail {
  note: NoteRow;
  refs: NoteRef[];
  links: LinkEntry[];
}

/**
 * One note — `knobas_app::commands::entity::get_note`.
 *
 * Rejects with `invalid` for something that is not an entity id and
 * `not_found` for a note that does not exist — including one deleted in
 * another window.
 */
export function getNote(noteId: string): Promise<NoteDetail> {
  return invoke<NoteDetail>("get_note", { noteId });
}

/**
 * One link a new note is born with —
 * `knobas_app::commands::entity::NoteLinkInput`.
 *
 * The note is always the **from** end, so `captured-in` reads *captured in*
 * from the note and *captured here* from the context.
 */
export interface NoteLinkInput {
  /** The other end's entity id. */
  target_id: string;
  /**
   * The stored relation. Folded to lower case by the backend like every other
   * relation, and **blank is refused** with `invalid` rather than defaulting to
   * `related`: `create_link`'s relation is optional and its absence means
   * *nobody named one*, but a link drawn beside a note is one the reader saw no
   * dialog for, so `""` is a caller bug.
   */
  relation: string;
}

/**
 * Write a new note — `knobas_app::commands::entity::create_note`.
 *
 * Every argument is optional: *New note* creates the row before the first
 * keystroke, so nothing typed into it can be lost to a closed window.
 *
 * `links` is what a **capture** attaches (`CONTEXT.md`, **Capture**):
 * `captured-in` to the context of the stored room the reader stands in, which
 * makes the note a member of it by ADR-0008, and `captured-from` to the
 * **foreground** — the open detail, else the room's anchor, else nothing, the
 * word as `CONTEXT.md`'s **Passive attribution** defines it and as the
 * heartbeat computes it (the deputy's ruling of 2026-09-08 on #502). A derived
 * room has no context and a room with nothing in front of the reader has
 * nothing to point at, so a caller with neither omits the argument. Drawn in
 * the note's own transaction — a note is never briefly in no context.
 *
 * Rejects with `invalid` for a link whose target is not an entity id, and for a
 * blank relation. An id no row carries is not an error: no link is drawn and
 * the note is written, the same answer a `[[ref]]` naming nothing gets.
 */
export function createNote(
  title?: string,
  bodyMd?: string,
  links?: NoteLinkInput[],
): Promise<NoteDetail> {
  return invoke<NoteDetail>("create_note", { title, bodyMd, links });
}

/**
 * Save a note — `knobas_app::commands::entity::save_note`.
 *
 * What the editor's autosave calls. The body is the source of truth for the
 * note's `[[ref]]` links: this reconciles them, so adding a ref draws the link
 * and removing it withdraws the link, and the answer carries the refs the
 * editor redraws its chips from.
 *
 * Rejects with `invalid` for something that is not an entity id and
 * `not_found` for a note that no longer exists — an editor open on a deleted
 * note does not resurrect it.
 */
export function saveNote(noteId: string, title: string, bodyMd: string): Promise<NoteDetail> {
  return invoke<NoteDetail>("save_note", { noteId, title, bodyMd });
}

/**
 * Delete a note — `knobas_app::commands::entity::delete_note`.
 *
 * The body goes; the address stays, tombstoned, so a link somebody drew *to*
 * the note stays visible and marked instead of dangling. Resolves `false` when
 * there was nothing left to delete.
 *
 * Rejects with `invalid` for something that is not an entity id.
 */
export function deleteNote(noteId: string): Promise<boolean> {
  return invoke<boolean>("delete_note", { noteId });
}

/**
 * One proposal as the room tray draws it —
 * `knobas_core::suggest::SuggestionEntry`.
 *
 * **Both** ends, unlike `LinkEntry`, which resolves only the end the reader is
 * not on: the tray is read from a room rather than from an entity, so there is
 * no "here" for it to leave out, and both ends have to be openable before the
 * reader decides.
 */
export interface SuggestionEntry {
  link: LinkRow;
  from: LinkEnd;
  to: LinkEnd;
}

/** One page of a room's tray — `SuggestionPage`. */
export interface SuggestionPage {
  rows: SuggestionEntry[];
  /**
   * Every proposal in the room, before `limit` — the number the tray's heading
   * shows. Not `rows.length`: "how many are waiting" is the question that
   * decides whether to look at all.
   */
  total: number;
  /**
   * Items holding more proposals than the tray draws for one item (three),
   * most hidden first — `SuggestionMore` (#774). The bound is applied in the
   * backend read, never after the fetch.
   */
  more: SuggestionMore[];
}

/** An item with proposals the tray did not draw, and how many. */
export interface SuggestionMore {
  item: LinkEnd;
  /** This item's proposals in the room that the tray did not draw. */
  more: number;
}

/**
 * Run a detection pass over the mirror, resolving to how many proposals it
 * wrote — `detect_suggestions`.
 *
 * Idempotent and cheap to repeat: a pass over an unchanged mirror writes
 * nothing, and a pass after a sync writes only what the new items justify and
 * resurrects nothing that was dismissed. This is what makes detection something
 * a surface calls rather than something the user asks for.
 *
 * One pass at a time, on a connection of its own (#745): a call made while a
 * pass runs resolves when the *next* pass does, and that one pass answers
 * every call that waited for it. It can therefore take a while; it never holds
 * a connection another command needs.
 *
 * It never creates a confirmed link, and nothing it writes reaches a source.
 */
export function detectSuggestions(): Promise<number> {
  return invoke<number>("detect_suggestions");
}

/**
 * The proposals a room holds, newest first — `room_suggestions`.
 *
 * `sources` is the room's membership, the same convention `EntityFilter.sources`
 * uses: `[]` means every source, and an enumerated list would hide anything
 * synced by a source with no configuration row. A proposal belongs to a room
 * when *either* of its ends does.
 *
 * `ctx` is a stored context's room (#47): proposals are scoped to its
 * membership — the fixed one-hop rule, resolved server-side — plus the
 * context's own entity, so a proposed *add to this context* surfaces in the
 * room it would add to. `null` leaves the derived rooms' source scoping as
 * the whole rule.
 *
 * The tray holds no state: this call is the whole of it.
 */
export function roomSuggestions(
  sources: string[],
  ctx: string | null,
  limit: number,
): Promise<SuggestionPage> {
  return invoke<SuggestionPage>("room_suggestions", { sources, ctx, limit });
}

/**
 * Accept a suggestion — `accept_suggestion`. It becomes an ordinary link and
 * appears in both ends' links panels.
 *
 * Idempotent: accepting an already-accepted or already-dismissed suggestion
 * resolves, writes no activity line and emits nothing. Rejects with `invalid`
 * for something that is not a link id and `not_found` for one nothing carries.
 * Emits `EVENTS.activityNew`.
 */
export function acceptSuggestion(linkId: string): Promise<void> {
  return invoke<void>("accept_suggestion", { linkId });
}

/**
 * Dismiss a suggestion — `dismiss_suggestion`. It is remembered, so the same
 * suggestion is never proposed again, and neither is the link it would have
 * become.
 *
 * The same tombstone `unlink` leaves, deliberately: a dismissed suggestion and
 * an unlinked link are one fact to the detector.
 *
 * Idempotent, and refuses nothing when the id names a link that has already
 * been accepted — that is a link, and `unlink` is what withdraws it. Emits
 * `EVENTS.activityNew`.
 */
export function dismissSuggestion(linkId: string): Promise<void> {
  return invoke<void>("dismiss_suggestion", { linkId });
}

/** One run of an excerpt's text — `knobas_core::evidence::Part`. */
export interface EvidencePart {
  text: string;
  /** The match: the key, the host or a shared word. */
  marked: boolean;
}

/** A stretch of one end's text — `knobas_core::evidence::Excerpt`. */
export interface EvidenceExcerpt {
  /** The end the text is from. */
  entity_id: string;
  /** The text in order; the marked parts are the match. */
  parts: EvidencePart[];
}

/**
 * What was said for and against one proposal —
 * `knobas_core::evidence::ProposalEvidence` (#836).
 */
export interface ProposalEvidence {
  link_id: string;
  /** The reason the rule stored, unchanged. */
  reason: string | null;
  /** The text the evidence was found in; empty for a relation the source records. */
  excerpts: EvidenceExcerpt[];
  /** *Also found*: what supports it. May be empty. */
  support: string[];
  /** *Speaks against it*: never empty ("Nothing found against it."). */
  against: string[];
}

/**
 * Why one proposal was made, beyond its one-line reason —
 * `proposal_evidence` (#836): the text it was found in with the match marked,
 * what supports it and what speaks against it. A read beside
 * `roomSuggestions`; it changes no ranking. Rejects with `invalid` for
 * something that is not a link id and `not_found` for one nothing carries.
 */
export function proposalEvidence(linkId: string): Promise<ProposalEvidence> {
  return invoke<ProposalEvidence>("proposal_evidence", { linkId });
}

/**
 * Ask a source to change something — `knobas_app::commands::entity::submit_write`.
 *
 * The one way the UI starts a write-back (issue #43). `payload` is a
 * serialized `knobas_source::WriteOp`, the same {@link WriteOpPayload} shape
 * {@link pendingWrites} hands back and {@link amendWrite} takes — untyped on
 * the wire because `WriteOp` grows per milestone (ADR-0006) and a typed
 * argument would make every growth an IPC change.
 *
 * **The write is queued, not sent**, and the row that comes back is the write
 * *as queued*, before the attempt: the queue is what decides send, pend or
 * hold, and there is no path around it. Read the outcome back with
 * {@link pendingWrites}, or re-read on `EVENTS.activityNew` — every queue
 * transition writes an activity line.
 *
 * **There is no source argument.** The target's namespace *is* the source id
 * (interfaces §4.1), so a second argument could only agree or contradict.
 *
 * Rejects with `invalid` for a payload that is not a write op, a target that
 * is not an entity id, or an op the source does not offer — which the action
 * bar should never have rendered, since it is drawn from that source's
 * `write_ops` — and `not_found` when the target names a source that is not
 * configured.
 */
export function submitWrite(payload: WriteOpPayload): Promise<QueuedWrite> {
  return invoke<QueuedWrite>("submit_write", { payload });
}

/**
 * What this ticket's workflow offers from where it stands **right now** —
 * `knobas_app::commands::entity::reachable_transitions` (#498).
 *
 * The statuses in the source's own spelling, straight from the source: nothing
 * is stored, so this is an answer about now and is good only for as long as the
 * detail is open. The status select offers these and only these, and the write
 * is unchanged — {@link submitWrite}'s `Transition` still names the status and
 * the adapter still resolves and refuses it at write time.
 *
 * **Only ask it of a ticket whose source declares `transition`.** A source with
 * no workflow — Gitea, TeamCity, Confluence, Uptime Kuma, the mock — rejects
 * this by name rather than answering an empty list, because the two mean
 * different things: an empty answer is a real workflow with nowhere left to go.
 * A rejection of any kind is the caller's cue to fall back to the
 * corpus-observed offer ({@link MiniBoard.sources}) and say the offer is
 * unverified, never to show an empty select.
 *
 * **There is no source argument**, for {@link submitWrite}'s reason: the
 * entity id already names the source.
 *
 * Rejects with `invalid` for something that is not an entity id, `not_found`
 * when its namespace is not a configured source, and `unauthorized` /
 * `unreachable` / `internal` as the source's own failure maps.
 */
export function reachableTransitions(entityId: string): Promise<string[]> {
  return invoke<string[]>("reachable_transitions", { entityId });
}

// -- the source view (#661, ADR-0020) ----------------------------------------
//
// The mirror of `knobas_source::view`, field for field. Frozen with it: a new
// block, field or tone is a §10.8 entry on both sides.

export interface Fact {
  label: string;
  value: string;
  tone: Tone | null;
}

/**
 * One row of an entity list. `entity_id` is set only where the mirror holds it.
 *
 * Rust's `ViewRow` has one more field, `commit` (#665): a commit named by its
 * repository and sha, which the app resolves to `entity_id` and clears. It is
 * absent on the wire to the shell, so it has no mirror here.
 */
export interface ViewRow {
  entity_id: string | null;
  title: string;
  meta: string | null;
  tone: Tone | null;
  chip: string | null;
  url: string | null;
}

/**
 * One write a view offers (#667, #702) — `knobas_source::view::ViewAction`: a
 * toolbar button (`SourceView.actions`) or one in an actions block.
 *
 * The source declares its dialog: the fields, the sentence, the tone. The
 * shell draws it only where the source's descriptor declares `write_op` and
 * `detail/view/actions.ts` can build its payload, and queues it against the
 * entity the view is of.
 */
export interface ViewAction {
  /** The `WriteOp` identifier it queues. */
  write_op: string;
  /** The button's words (`Merge…`). */
  label: string;
  /** What the dialog asks for, in order. */
  fields: ActionField[];
  /** The dialog's sentence. */
  confirm: string | null;
  /** The entity's main write: drawn filled. */
  primary: boolean;
  /** A write that destroys something: the error tone, always through its dialog. */
  danger: boolean;
  /** Why it cannot be taken now: drawn disabled, this as tooltip and description. */
  disabled: string | null;
  /** The dialog also offers this entity's own settings to change for this one write (#669). */
  with_settings: boolean;
  /**
   * Answers the source fixes, by name: never drawn, handed to the payload
   * table under the dialog's values (#705). One op as two actions
   * (`submit_review`'s *Approve…* and *Request changes…*, by `verdict`).
   */
  values: Record<string, string>;
  /**
   * The entity the write targets where it is not the view's own (#707): a
   * branch's *New branch…* names its repository. `null` targets the view's.
   */
  entity: string | null;
  /**
   * Whether it sends the reader's pending line comments (#709): `sends` when
   * there are some, `requires` only then (drawn only while there are some).
   */
  line_comments: LineComments;
}

/** What an action does with the pending line comments — `knobas_source::view::LineComments`. */
export type LineComments = "none" | "sends" | "requires";

/** How a reader comments on a diff's lines (#709) — `knobas_source::view::Commenting`. */
export interface Commenting {
  /** The op the comments are sent through (`submit_review`). */
  write_op: string;
}

/** One input of an action's dialog — `knobas_source::view::ActionField`. */
export interface ActionField {
  /** The key its value is handed on under. */
  name: string;
  label: string;
  kind: FieldKind;
  /** A text's prefill, a choice's preselected value (`null`: the first), a flag's `"true"`. */
  value: string | null;
  /** Not sent without: text other than blanks, a choice picked, a flag checked. */
  required: boolean;
  placeholder: string | null;
}

/** How a field asks — `knobas_source::view::FieldKind`, a union on `type`. */
export type FieldKind =
  | { type: "text"; suggestions: string[] }
  | { type: "long_text" }
  | { type: "choice"; options: ChoiceOption[] }
  | { type: "flag" };

export interface ChoiceOption {
  /** What the op takes. */
  value: string;
  /** What the reader sees. */
  label: string;
}

export interface Setting {
  name: string;
  /** Empty for a `secret` value: the adapter never sends one. */
  value: string;
  inherited: boolean;
  secret: boolean;
  editable: boolean;
  description: string | null;
}

export type DiffChange = "added" | "modified" | "deleted" | "renamed" | "binary";

export interface DiffLine {
  kind: "context" | "add" | "del";
  /** The line's number before the change; `null` for an added line. */
  old: number | null;
  /** The line's number after the change; `null` for a removed line. */
  new: number | null;
  text: string;
}

export interface Hunk {
  /** The `@@ -a,b +c,d @@ context` line as the server sent it. */
  header: string;
  lines: DiffLine[];
}

export interface DiffFile {
  path: string;
  /** Where it was before, for a rename. */
  old_path: string | null;
  change: DiffChange;
  /** Counted over the whole file, even where its lines were cut. */
  additions: number;
  deletions: number;
  hunks: Hunk[];
  /** Some of its lines were cut by a cap. */
  truncated: boolean;
  /** The review conversations on its lines (#709), in the file's order, the outdated last. */
  threads: LineThread[];
}

/**
 * One conversation on one line (#709) — `knobas_source::view::LineThread`.
 * `line` counts on `side`; an `outdated` one is no longer on a line of this
 * diff as written (the source's answer) and is drawn apart.
 */
export interface LineThread {
  side: ReviewSide;
  line: number;
  outdated: boolean;
  comments: LineComment[];
}

/** A thread on a file the diff no longer holds (#709) — `knobas_source::view::FileThread`. Always outdated. */
export interface FileThread {
  path: string;
  thread: LineThread;
}

/** One comment of a thread. `body` is the source's Markdown, never rendered as markup. */
export interface LineComment {
  author: string;
  body: string;
  /** RFC 3339, as the source sent it. */
  at: string | null;
}

/**
 * One typed piece of a section: the vocabulary the shell draws, and nothing else.
 * `Block` in Rust; `ViewBlock` here, because the barrel already exports a
 * time-tracking `Block`.
 */
export type ViewBlock =
  | { type: "facts"; facts: Fact[] }
  | { type: "text"; text: string; markdown: boolean }
  | { type: "entities"; title: string | null; rows: ViewRow[] }
  | { type: "diff"; files: DiffFile[]; truncated: boolean; commentable: Commenting | null; gone: FileThread[] }
  | { type: "log"; text: string; truncated: boolean }
  | {
      type: "settings";
      title: string | null;
      entity: string;
      write_op: string;
      values: Setting[];
    }
  | { type: "actions"; actions: ViewAction[] }
  | {
      type: "tree";
      trail: Crumb[];
      entries: TreeEntry[];
      /** The source's cap cut the listing; the rest is at `url`. */
      truncated: boolean;
      /** The directory's page at its source. */
      url: string | null;
    }
  | {
      type: "file";
      trail: Crumb[];
      path: string;
      /** Empty for a binary file. */
      text: string;
      /** A hint from the file's name (`"rust"`, `"markdown"`); nothing is highlighted. */
      language: string | null;
      /** The whole file's size in bytes. */
      size: number;
      /** A cap cut the text; the rest is at `url`. */
      truncated: boolean;
      binary: boolean;
      /** The file's page at its source. */
      url: string | null;
      /**
       * The file's blob sha at the ref drawn (#883): what an edit of it sends
       * as `CommitFile`'s `base_sha`. `null` where the source has none.
       */
      sha: string | null;
    };

/**
 * One step of a tree's or a file's trail (#711): a name and the section that
 * opens it. A section a block names opens **in the tab it is drawn in**, read
 * with `entityView(entity, section)`; the id is the source's and never parsed.
 */
export interface Crumb {
  name: string;
  section: string;
}

/** What a tree entry is, as git records it. */
export type EntryKind = "dir" | "file" | "symlink" | "submodule";

/** One entry of a directory's listing. */
export interface TreeEntry {
  name: string;
  kind: EntryKind;
  /** A file's size in bytes; `null` for a directory or a submodule. */
  size: number | null;
  /** The section that opens it in the same tab; `null` where it cannot be opened. */
  section: string | null;
}

/** One tab. A `deferred` one carries no blocks until its own read. */
export interface ViewSection {
  id: string;
  title: string;
  count: number | null;
  deferred: boolean;
  blocks: ViewBlock[];
}

export interface SourceView {
  sections: ViewSection[];
  /**
   * The entity's actions, drawn as a toolbar above the tabs on every tab
   * (#702). Empty draws none.
   */
  actions: ViewAction[];
}

/**
 * The source view of one entity — `knobas_app::commands::entity::entity_view`
 * (#661, ADR-0020).
 *
 * Read live from the entity's own source and never stored. `section` omitted
 * reads the whole view, deferred sections as stubs; a section id reads that
 * one section's blocks, and the answer holds exactly that section.
 *
 * Three answers, each drawn differently: a view (tabs, the generic projection
 * behind *Raw*); `null`, a source with nothing to draw for this kind (the panel
 * as it always was); and a rejection, which draws the same panel under one line
 * naming it. Rejects with `invalid`, `not_found`, `unauthorized`,
 * `unreachable` or `internal` as {@link reachableTransitions} does.
 */
export function entityView(entityId: string, section: string | null = null): Promise<SourceView | null> {
  return invoke<SourceView | null>("entity_view", { entityId, section });
}

// ---------------------------------------------------------------------------
// The start-work flow — `knobas_app::commands::entity`'s start-work block and
// `knobas_core::start_work` (issue #44).
// ---------------------------------------------------------------------------

/**
 * Which step of a start-work flow this is —
 * `knobas_core::start_work::Step`.
 *
 * Three of the four are `WriteOp` identifiers; `link_pull_request` is not, and
 * that is the point — the link is knobas-owned and local, written to neither
 * source, which is why the relationship survives whatever Jira and Gitea
 * record.
 */
export type StartWorkStepKind =
  | "create_branch"
  | "create_pull_request"
  | "link_pull_request"
  | "transition";

/**
 * What happened to one step — `knobas_core::start_work::StepOutcome`.
 *
 * `queued` is neither a success nor a failure: the source could not take the
 * write, so it is a pending write and will go when the source can. The flow's
 * completion and the write's delivery are different events, and a stepper that
 * drew them the same way would report work that has not happened.
 */
export type StartWorkOutcome =
  | "pending"
  | "running"
  | "succeeded"
  | "queued"
  | "failed"
  | "skipped";

/**
 * One step of a flow — `knobas_core::start_work::FlowStep`.
 *
 * `payload` is **the proposal**, as the user last left it: the serialized
 * `WriteOp` this step will submit, or the link's relation. Untyped on the wire
 * for {@link submitWrite}'s reason — `WriteOp` grows per milestone.
 *
 * `write_id` names the `knobas.write_queue` row this step dispatched, once it
 * has one. It is what makes a retry safe: a step whose write is still open is
 * retried by acting on that write rather than by queueing a second one.
 */
export interface StartWorkStep {
  id: number;
  /** The ticket the flow is about — and the flow's identity. */
  ticket_id: string;
  step: StartWorkStepKind;
  /** Where the step sits in the sequence; the stepper renders in this order. */
  position: number;
  outcome: StartWorkOutcome;
  payload: unknown;
  write_id: number | null;
  /** What happened, in whoever's words. Untrusted source text. */
  detail: string | null;
  updated_at: string;
}

/**
 * The flow for a ticket, proposing one if there is none and `repoId` says where
 * it would go — `knobas_app::commands::entity::start_work_flow`.
 *
 * An empty array means there is no flow and no repository was named, which is
 * the state where the view asks for one. **Proposing dispatches nothing**: the
 * sequence is composed and stored so it can be shown before anything happens.
 *
 * Rejects with `invalid` for an id that is not an entity id, and `not_found` if
 * the ticket is not in the mirror.
 */
export function startWorkFlow(entityId: string, repoId: string | null): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_flow", { entityId, repoId });
}

/**
 * Run the flow as far as it will go, and answer where it stopped —
 * `knobas_app::commands::entity::start_work_run`.
 *
 * A step that fails stops the sequence; a step whose source cannot take the
 * write leaves it queued and waiting. Neither is a rejection here — both are
 * outcomes in the rows that come back, which is what the stepper draws. Emits
 * `EVENTS.activityNew` for every queue transition the run causes.
 */
export function startWorkRun(entityId: string): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_run", { entityId });
}

/**
 * Retry one step, without redoing the ones that succeeded —
 * `knobas_app::commands::entity::start_work_retry`.
 *
 * A step whose write is still open is retried by reading that write, not by
 * queueing a second one; otherwise knobas looks for the effect at the source
 * before sending anything again.
 *
 * Rejects with `conflict` if an earlier step is where the flow stopped —
 * running a later step over a failed one is what the sequence exists to
 * prevent.
 */
export function startWorkRetry(stepId: number): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_retry", { stepId });
}

/**
 * Skip one step — `knobas_app::commands::entity::start_work_skip`.
 *
 * A skipped step is settled, so the sequence carries on past it: a ticket that
 * needs no branch still gets its status moved.
 */
export function startWorkSkip(stepId: number): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_skip", { stepId });
}

/**
 * Replace a step's proposal with the one the reader edited —
 * `knobas_app::commands::entity::start_work_amend`.
 *
 * `payload` is the step's own stored value, edited. Rejects with `conflict` if
 * the step has already happened: editing it then would change nothing at the
 * source and everything on screen.
 */
export function startWorkAmend(stepId: number, payload: unknown): Promise<StartWorkStep[]> {
  return invoke<StartWorkStep[]>("start_work_amend", { stepId, payload });
}

/**
 * The reverse direction: move every ticket whose linked pull request has been
 * merged — `knobas_app::commands::entity::follow_merges`.
 *
 * Answers how many moved, so an automatic change can be announced rather than
 * happening silently. It acts **only on links knobas holds**: a merged pull
 * request nobody connected to a ticket does nothing at all.
 */
export function followMerges(): Promise<number> {
  return invoke<number>("follow_merges");
}

// -- the inbox (issue #45) --------------------------------------------------
//
// Appended, never rewritten: this file is orchestrator-owned and append-only.
// No new module on either side — inbox items are derived from entities and its
// two write commands act on them, so they live in the existing `entity`
// module and its mirror.

/**
 * Which kind of demand an item is — `knobas_core::inbox::Category`.
 *
 * A string union rather than a lookup table, and total on purpose: a category
 * added on the Rust side has to fail `svelte-check` here rather than fall
 * through to a label nobody notices.
 */
export type InboxCategory =
  | "review_request"
  | "mention"
  | "failed_build"
  | "new_assignment"
  | "credential_expiry"
  | "alert";

/**
 * Which shelf a read wants — `knobas_core::inbox::Shelf`.
 *
 * `"stream"` is what needs you now; `"snoozed"` is what you deferred. One
 * derivation with one predicate, asked for one side or the other, so the two
 * can never overlap or leave a gap.
 */
export type InboxShelf = "stream" | "snoozed";

/**
 * One line of the stream — `knobas_core::inbox::InboxItem`.
 *
 * **Derived, not stored.** There is no inbox table: every field here is
 * computed from the mirror when the stream is read, which is why an item
 * leaves on its own when the thing it was about is resolved at the source.
 */
export interface InboxItem {
  /**
   * `"<category>:<subject>"` — the durable name of this demand, and what
   * {@link snoozeInboxItem} and {@link completeInboxItem} take.
   *
   * Stable across syncs: the category is a fixed word and the subject is an
   * entity id (or a source id, for a credential expiry). Do not compose one
   * here — hand back the string the backend gave you.
   */
  key: string;
  category: InboxCategory;
  /** The source that owes it. Also the entity namespace. */
  source_id: string;
  /**
   * The entity to open, or `null` for a credential expiry — whose subject is
   * a source and not an entity. An item with no entity has no *Open*, and
   * that is honest rather than incomplete.
   */
  entity_id: string | null;
  /** The entity's kind, for the monogram. `null` with `entity_id`. */
  kind: string | null;
  /** Raw source text. Render as text, never as markup (gotcha 7). */
  title: string;
  /** Why this is here, as a sentence the backend produced. Also raw text. */
  reason: string;
  /** RFC 3339. When the item last moved — the stream's ordering. */
  occurred_at: string;
  /** Where a human reads this in the source's own UI, when the mirror knows. */
  web_url: string | null;
  /** RFC 3339, on the snoozed shelf only: when it comes back. */
  snoozed_until: string | null;
}

/**
 * One entry of the stream — `knobas_app::inbox::InboxEntry`.
 *
 * **Nested, not flattened** (`{item, actions}`), which is #53's ratified shape
 * for a record paired with an answer about it: the item is the derivation's
 * and the actions are the app's, and a flattened bag would make a reader guess
 * which half a field came from. A later tidy-up that flattens it is a bug, not
 * a simplification.
 */
export interface InboxEntry {
  item: InboxItem;
  /**
   * `knobas_source::WriteOp` identifiers this item's source really declares,
   * best first — dispatch one with {@link submitWrite}.
   *
   * **Render exactly these and no others.** An op absent from this list is one
   * the source cannot perform, and a button for it would fail on press. Empty
   * is a real answer: a credential expiry has nothing to ask a source for.
   * *Open*, *Snooze* and *Done* are knobas' own and are not in here.
   */
  actions: string[];
}

/**
 * The inbox — `inbox_items`. One actionable stream, newest first.
 *
 * Both shelves are the same derivation with the same predicate: ask for
 * `"snoozed"` to see what is deferred and when it returns.
 *
 * Rejects with `not_ready` while the database or the sync engine is still
 * coming up.
 */
export function inboxItems(shelf: InboxShelf): Promise<InboxEntry[]> {
  return invoke<InboxEntry[]>("inbox_items", { shelf });
}

/**
 * How many items need you now — `inbox_count`. The number the top strip shows.
 *
 * **Snoozed items are not in it**, because the number means "needs me now";
 * neither are items marked done. It is the stream's own statement counted, so
 * the badge cannot disagree with the view it opens — do not recompute it from
 * a filtered {@link inboxItems} result.
 */
export function inboxCount(): Promise<number> {
  return invoke<number>("inbox_count");
}

/**
 * Not now — `snooze_inbox_item`. The item leaves the stream and returns on
 * `until`, without anything having to be scheduled.
 *
 * `until` is an absolute RFC 3339 moment: the presets (*tomorrow*, *next
 * Monday*, *after the credential expires*) are this side's arithmetic.
 * Snoozing something already snoozed moves its date.
 *
 * Rejects with `not_found` for an item on neither shelf — which is what a
 * stale list in this webview looks like. Writes one activity line and emits
 * `EVENTS.activityNew`.
 */
export function snoozeInboxItem(itemKey: string, until: string): Promise<void> {
  return invoke<void>("snooze_inbox_item", { itemKey, until });
}

/**
 * I handled this — `complete_inbox_item`.
 *
 * The item leaves, and **comes back if its subject moves again**: done is
 * recorded as the moment it was answered, so marking a mention done hides it
 * until somebody says something new rather than muting the ticket for ever.
 *
 * Rejects like {@link snoozeInboxItem}. Writes one activity line and emits
 * `EVENTS.activityNew`.
 */
export function completeInboxItem(itemKey: string): Promise<void> {
  return invoke<void>("complete_inbox_item", { itemKey });
}

/** What kind of working set a context is — `ContextKind`. */
export type ContextKind = "epic" | "ticket" | "adhoc";

/** One stored context — `ContextRow` (#47). */
export interface ContextRow {
  /** `ctx:<uuid>` — a local id, and the id of the context's own entity. */
  id: string;
  kind: ContextKind;
  title: string;
  /** The promoted entity this context is about; `null` for an ad-hoc label. */
  anchor_id: string | null;
  /** RFC 3339. */
  created_at: string;
  /** RFC 3339, or `null` while the context is live. */
  archived_at: string | null;
}

/**
 * The switcher's list — `list_contexts`. Every unarchived context, newest
 * first.
 */
export function listContexts(): Promise<ContextRow[]> {
  return invoke<ContextRow[]>("list_contexts");
}

/**
 * Mint an ad-hoc context from a label — `create_context`.
 *
 * Rejects with `invalid` for a blank label. Writes one activity line and
 * emits `EVENTS.contextsChanged` with the new row.
 */
export function createContext(title: string): Promise<ContextRow> {
  return invoke<ContextRow>("create_context", { title });
}

/**
 * Promote an entity to a context of its own — `promote_context` (spec §7:
 * "any ticket can be promoted").
 *
 * Idempotent: promoting twice answers with the one context. Rejects with
 * `not_found` for an entity that never synced, and with `invalid` for a
 * context — a context about a context would union two working sets
 * (ADR-0008). Writes one activity line and emits `EVENTS.contextsChanged` —
 * on the first promotion only, because the second mutated nothing.
 */
export function promoteContext(entityId: string): Promise<ContextRow> {
  return invoke<ContextRow>("promote_context", { entityId });
}

/**
 * Who is in a context — `context_members`, by the fixed rule (§16.11,
 * ADR-0008): explicit adds and the anchor (an epic's tickets count through
 * their source-recorded parent), their direct links, one hop out, and — since
 * #434 — every asset held by one of those, at any depth.
 *
 * An unknown context has no members rather than an error. The per-context
 * inbox filter ("3 here") intersects the inbox stream with this set, so the
 * badge and the rows it stands for come from the same stream by construction.
 */
export function contextMembers(ctxId: string): Promise<string[]> {
  return invoke<string[]>("context_members", { ctxId });
}

/**
 * Where a reached entity's path came from — `knobas_core::reach::Origin`.
 * Tagged on `kind`: the ticket's own links, the parent's (R-7.3), or a link
 * set whose rule matches the ticket (#866), named as the reader named it.
 */
export type ReachOrigin =
  | { kind: "own" }
  | { kind: "parent"; ticket: string }
  | { kind: "link_set"; set: string; name: string };

/** One entity a ticket reaches — `knobas_core::reach::Reached` (#817). */
export interface Reached {
  entity_id: string;
  /** What it is: `pr`, `build`, `asset`, `monitor`. */
  kind: string;
  /**
   * The first step of the path that brought it in, an entity id: what the
   * row's reason names. Equal to `entity_id` when it is itself the first step.
   */
  step: string;
  origin: ReachOrigin;
}

/**
 * What a ticket reaches — `job_reach` (#817, R-2.8): its context's walk,
 * down only from a VM, proxy or host, plus its parent's own links behind
 * {@link parentLinks}. An id nothing answers to reaches nothing.
 */
export function jobReach(entityId: string): Promise<Reached[]> {
  return invoke<Reached[]>("job_reach", { entityId });
}

/** Whether *Sub-tasks show their parent ticket's connections* is on (R-7.3). */
export function parentLinks(): Promise<boolean> {
  return invoke<boolean>("parent_links");
}

/** Switch R-7.3's switch; answers what is now stored. */
export function setParentLinks(enabled: boolean): Promise<boolean> {
  return invoke<boolean>("set_parent_links", { enabled });
}

/** One ticket on the mini board — `MiniBoardCard` (#177). */
export interface MiniBoardCard {
  /** The ticket's stable in-app address; clicking a card opens it. */
  entity_id: string;
  /** Which source the ticket came from — the key into `MiniBoard.sources`. */
  source_id: string;
  /** The source's own key for it, e.g. `PAY-231`. */
  key: string;
  title: string;
  /** `null` where the mirrored record carries no priority knobas can read —
   * the card simply omits it rather than guessing (ADR-0007). */
  priority: string | null;
  /** The ticket's type in its source's words (`Story`, `Bug`), or `null` on a miss (#810). */
  issue_type: string | null;
  /** The account it is assigned to, as the source spells an account; `null` when unassigned. */
  assignee: string | null;
  /** RFC 3339, when the source last changed it, or `null` where it never said. */
  updated_at: string | null;
  /** Its declared parent chain as one line (its epic, for Jira), or `null`. */
  path: string | null;
  /** The title of the mirrored item its direct parent names (the epic's name, #894), or `null`. */
  parent_title: string | null;
  /** Whether `assignee` is one of the configured usernames, ignoring case. */
  mine: boolean;
  /** Its labels at the declared path, in the record's order; empty on a miss (#813). */
  labels: string[];
  /** How many comments it has at the declared path (#894); `0` is an answer, `null` a miss. */
  comment_count: number | null;
  /** The signals around it, in the order a card draws them (#815). */
  signals: Signal[];
}

/** Which fact a signal states: the values `has:` takes — `knobas_core::signals::SignalKind` (#815). */
export type SignalKind = "failing-build" | "blocker" | "open-pr" | "review-for-me";

/** One signal on a ticket — `knobas_core::signals::Signal` (#815). */
export interface Signal {
  kind: SignalKind;
  /** The words beside the dot: *build failed*, *PR #142 open*, *blocked by OPS-77*. */
  text: string;
  tone: Tone;
}

/** One status column — `MiniBoardColumn` (#177). */
export interface MiniBoardColumn {
  /**
   * The status in the source's own words, or `null` for the terminal group:
   * tickets whose record carries no status the read recognizes. The words on
   * screen for that group ("No status") are the shell's — what crosses the
   * bridge is the absence.
   */
  status: string | null;
  /** Newest first: all of them, or the newest `perColumn` of a capped read. */
  cards: MiniBoardCard[];
  /**
   * How many tickets stand in this column, drawn or not (#753) — the column
   * header's count. `cards.length` on an uncapped read.
   */
  total: number;
}

/** What one source's corpus shows — `SourceStatuses` (#177). */
export interface SourceStatuses {
  source_id: string;
  /**
   * Every status this source's live tickets carry, in the board's order.
   * Never carries the terminal group: "no status" is somewhere a ticket can
   * be, not somewhere it can be moved to.
   */
  statuses: string[];
}

/** A context's tickets as the mini board draws them — `MiniBoard` (#177). */
export interface MiniBoard {
  /**
   * The observed status columns in display order: To Do, In Progress, In
   * Review, Done first where the corpus shows them, every other observed
   * status after them alphabetically, and the terminal group last.
   */
  columns: MiniBoardColumn[];
  /**
   * Per source that put a card on this board, the statuses its own corpus
   * shows — what the ticket detail's status select falls back to when the
   * workflow read fails (#179, and {@link reachableTransitions} for why it is
   * the fallback rather than the offer).
   */
  sources: SourceStatuses[];
}

/**
 * The Tickets tile's mini board — `mini_board` (ADR-0009, #177).
 *
 * Takes the room's own filter, so the board is scoped exactly as every other
 * tile in that room is: a stored room narrows by `context` — the fixed one-hop
 * membership `contextMembers` answers with (§16.11, ADR-0008) — and a derived
 * room by `sources`, which is empty in *All work*. Of what the scope admits the
 * board draws the live tickets, so a ticket the source deleted is off it.
 * Grouping and column order happen in the command, not here.
 *
 * `perColumn` cuts each column to its newest cards (#753): what the tile asks
 * for, so a room of thousands of tickets is not thousands of cards. Omitted,
 * the board is whole.
 *
 * `query` (#810) is search grammar text — the Tickets view's base query and
 * its filter bar's words and chips. A non-blank one narrows the board to
 * every ticket the grammar admits; the filtering is the server's, never the
 * webview's (R-2.7).
 *
 * An unknown context is an empty board rather than a rejection — the tile has
 * to be able to say "nothing here" without saying "this broke".
 */
export function miniBoard(
  filter: Pick<EntityFilter, "sources" | "context" | "project">,
  perColumn?: number | null,
  query?: string | null,
): Promise<MiniBoard> {
  return invoke<MiniBoard>("mini_board", {
    ctxId: filter.context,
    sources: filter.sources,
    project: filter.project,
    perColumn: perColumn ?? null,
    query: query ?? null,
  });
}

/**
 * The statuses one source's live ticket corpus shows, in the board's order —
 * `source_statuses` (#760). The ticket detail's fallback offer when the
 * workflow cannot be read ({@link reachableTransitions}); asked only then.
 */
export function sourceStatuses(sourceId: string): Promise<string[]> {
  return invoke<string[]>("source_statuses", { sourceId });
}

/** One project a source's live corpus shows — `Project` (ADR-0010, #208). */
export interface Project {
  /**
   * Which source shows it. Half of the identity, not decoration: a project
   * key is unique only inside its own source, so two sources using one key
   * are two projects.
   */
  source_id: string;
  /** The source's own key for it, e.g. `PAY`. */
  key: string;
  /**
   * The source's own name for it, or `null` where the record carries none
   * that knobas can read. A project with a key and no name is still a
   * project; what to label it with is the shell's decision, and a name the
   * backend invented would be indistinguishable from one the source said.
   */
  name: string | null;
}

/**
 * Every project the live corpus shows — `list_projects`, ordered by source
 * then key.
 *
 * What the switcher builds a room per project from. It is a **census** and
 * deliberately not derived from a room's own read, which is a window over the
 * newest items: a quiet project would otherwise silently have no room. A
 * source the user turned off shows no projects, the same way its items leave
 * every other reader.
 */
export function listProjects(): Promise<Project[]> {
  return invoke<Project[]>("list_projects");
}

/**
 * Which inbox categories may raise a desktop notification —
 * `notification_kinds` (#290, spec #272 "Notifications").
 *
 * Empty for a profile nobody has switched one on in: every kind is off by
 * default, so a noisy Jira cannot make the feature unusable on day one.
 *
 * One `knobas.setting` row and no migration — the rule
 * `set_backup_schedule` and `set_passive_attribution` follow. It is a
 * *preference* and says nothing about the OS permission, which is asked for on
 * the click that switches the first kind on and is not knobas' to remember.
 */
export function notificationKinds(): Promise<InboxCategory[]> {
  return invoke<InboxCategory[]>("notification_kinds");
}

/**
 * Set which inbox categories may notify — `set_notification_kinds`. Answers
 * with what is now **stored**, deduplicated and in the backend's own order, so
 * the toggles draw the database rather than the click.
 *
 * Rejects with `invalid` for a word that is not a category — the write side is
 * strict where the read side is forgiving, because the only caller is a
 * surface sending back words it was given.
 */
export function setNotificationKinds(kinds: InboxCategory[]): Promise<InboxCategory[]> {
  return invoke<InboxCategory[]>("set_notification_kinds", { kinds });
}

/**
 * One line of the standup digest — `knobas_app::standup::DigestLine` (#288).
 *
 * Story 59: *every digest line links to the commit, PR, worklog, comment or
 * transition it came from, so that a line is something I can check.* The first
 * three fields are what the line is about and the last three are why it is
 * here — produced by the rule that made the line, never re-rendered from
 * {@link verb} at display time.
 */
export interface DigestLine {
  /**
   * The item to open, as an entity id.
   *
   * `null` for exactly one line: a running timer on an ad-hoc label, which has
   * no item behind it. The view draws that line as text rather than as a link;
   * "DB config for the migration" is a legal thing to be working on and a
   * digest that dropped it would leave the reader's own afternoon out of their
   * standup.
   */
  entity_id: string | null;
  /** The entity's kind, for the monogram. `null` with `entity_id`. */
  kind: string | null;
  /**
   * What the line is about, in the source's own words — or the ad-hoc label.
   * Raw source text: render as text, never as markup (gotcha 7).
   */
  title: string;
  /**
   * Which source said so — a `source_config.id`, or `knobas` for a line that
   * is knobas' own record of an act (a worklog copy, the running timer).
   */
  source: string;
  /**
   * Which verb, as one word: `attributed`, `log_work`, `timer`,
   * `blocked_status`, `blocked_by` — or, for a write, **the write queue's own
   * `op`** (`comment`, `transition`, …).
   *
   * A `string` and deliberately not a union, which is where this differs from
   * `InboxItem.category`: a `WriteOp` identifier is the *adapter's*, so a
   * closed union here would go stale the day an adapter declares a new op. A
   * view that branches on it must have a default arm.
   *
   * `attributed`, not `authored`: interfaces §4.1's `author` is whoever the
   * **adapter** says a record belongs to, and Jira's says the assignee — so a
   * mirror line means "the source attributes this to you", never "you wrote
   * it".
   */
  verb: string;
  /** Why this line is here, as a sentence naming the source and the verb. */
  reason: string;
  /** RFC 3339 — when it happened, and what each list is ordered by. */
  at: string;
}

/**
 * The standup's four lists — `knobas_app::standup::StandupDigest` (#288, #932).
 *
 * `CONTEXT.md`'s **digest**, drawn from the mirror and the activity stream for
 * the configured usernames. Mine only: it describes the person the sources
 * were configured as, never a colleague.
 */
export interface StandupDigest {
  /**
   * `"2026-08-28"` — the day {@link yesterday} is really about, in the
   * reader's own reckoning. Friday, on a Monday.
   *
   * `null` when nothing was found inside the seven-day cap, which is a week
   * off and not an error. The view says so; a date filled in anyway would put
   * a heading over an empty list claiming that day was quiet.
   */
  yesterday_day: string | null;
  /** The newest earlier day with any of the user's activity, newest first. */
  yesterday: DigestLine[];
  /**
   * What has been touched since the day's own midnight, plus the running
   * timer's target, newest first.
   */
  today: DigestLine[];
  /**
   * The user's items a source calls stuck or a link calls blocked. An item can
   * be on this list twice, once for each reason — they are two different facts
   * about it and collapsing them would throw one of them away.
   */
  blockers: DigestLine[];
  /**
   * The monitor alerts since the last working day (#932), newest first: one
   * line per alert open at any point of the window, `kind` `"monitor"`, `verb`
   * `"down"` or `"warn"`, `reason` the duration (`was down 12 min`, `is still
   * down`) and `at` when it began. Not mine-only: a monitor has no owner.
   */
  infrastructure: DigestLine[];
}

/**
 * The standup digest for one day — `standup_digest` (#288, spec #272).
 *
 * **The webview computes the days**, each as a date and the two instants it
 * spans, because the machine's timezone is a fact only this side holds — the
 * rule `lib/time/day.ts` records in full. `today` is the day being asked about
 * and `earlier` the days before it, oldest first; `lib/standup/standup.ts`'s
 * `digestWindows` is what builds both.
 *
 * How far back *yesterday* may reach is **not** this side's to decide: the
 * backend consults at most its own seven, however many windows are sent.
 */
export function standupDigest(
  today: DayWindow,
  earlier: DayWindow[],
): Promise<StandupDigest> {
  return invoke<StandupDigest>("standup_digest", { today, earlier });
}

/**
 * Where standup protocols are published —
 * `knobas_app::protocol::PublishTarget` (#289, spec #272 stories 67-68).
 *
 * Both halves, because neither is enough on its own: with two Confluence
 * sources configured a parent id alone names a page in an instance nobody
 * chose. The space key is deliberately **not** here — it is read off the
 * parent's own record when the op is composed, so the two cannot disagree
 * about which space the parent is in.
 */
export interface PublishTarget {
  /** The Confluence source's instance id, which is also the id namespace. */
  source_id: string;
  /** The parent page, as an entity id (`confluence:98400`). */
  parent: string;
}

/**
 * What became of publishing a protocol —
 * `knobas_app::protocol::Publication`.
 */
export interface Publication {
  /** The write queue row carrying it. */
  write_id: number;
  /** Where the write stands, in the queue's own vocabulary. */
  state: WriteState;
  /** The source's sentence, when it had one. Untrusted source text. */
  detail: string | null;
  /**
   * The page, as an entity id, once the source has named it.
   *
   * `null` while the write is in flight — and not the same fact as
   * {@link linked} being false, which additionally means the mirror has not
   * caught up with the page yet.
   */
  page_entity_id: string | null;
  /** Whether note and page are linked. */
  linked: boolean;
}

/**
 * A date's standup protocol — `knobas_app::protocol::Protocol`.
 *
 * `CONTEXT.md`'s **standup protocol**: a note, one per date, holding
 * attendees, per-person notes and action items. Not a kind of its own, which
 * is why this carries a `note_id` and everything else about it is read through
 * {@link getNote} like any other note.
 */
export interface Protocol {
  /** The date, `YYYY-MM-DD`. */
  day: string;
  /** The note holding it. */
  note_id: string;
  /** What the published page is titled — what a second publish would collide with. */
  page_title: string;
  /** The publication, when there has been one. */
  publication: Publication | null;
}

/**
 * The protocol for a date, made from the template if it is not there yet —
 * `standup_protocol` (#289, story 64).
 *
 * **Get-or-create**: opening a date twice lands in the same note. It is also
 * where a publication that settled while nobody was looking catches up — the
 * page id comes off the write queue row, which outlives the process that first
 * received it.
 */
export function standupProtocol(day: string): Promise<Protocol> {
  return invoke<Protocol>("standup_protocol", { day });
}

/**
 * Publish a date's protocol to Confluence — `publish_standup_protocol`
 * (stories 65-68).
 *
 * `target` is the first publish's dialog answer; omit it to use the stored
 * one, and a missing stored one is a refusal rather than a guess. **A second
 * publish for one date queues nothing** and answers with the publication
 * already on the queue.
 */
export function publishStandupProtocol(
  day: string,
  target?: PublishTarget,
): Promise<Protocol> {
  // Answers once the page's write is queued (#779): the publication is that
  // write, with no page yet; `standupProtocol` names it once it lands.
  return invoke<Protocol>("publish_standup_protocol", { day, target: target ?? null });
}

/** Where protocols are published, or `null` until somebody has said. */
export function standupPublishTarget(): Promise<PublishTarget | null> {
  return invoke<PublishTarget | null>("standup_publish_target");
}

/** Change where protocols are published — settings' half of story 67. */
export function setStandupPublishTarget(target: PublishTarget): Promise<PublishTarget> {
  return invoke<PublishTarget>("set_standup_publish_target", { target });
}

/**
 * What became of filing a ticket from an action item —
 * `knobas_app::commands::entity::ActionItemTicket`.
 */
export interface ActionItemTicket {
  /** The write queue row carrying the create. */
  write_id: number;
  /** Where that write stands, in the queue's own vocabulary (#779). */
  state: WriteState;
  /** The queue's or the source's sentence about it. Untrusted text. */
  detail: string | null;
  /**
   * The ticket, as an entity id, once the mirror has it.
   *
   * `null` for a create still on the queue, and for one that landed at a
   * source whose re-read has not brought the ticket back. Both are "not yet",
   * and the note is unlinked in both — there is nothing to link to.
   */
  ticket_entity_id: string | null;
  /** Whether the protocol and the ticket are linked. */
  linked: boolean;
}

/**
 * File a ticket from an action item and link it to the protocol —
 * `create_action_item_ticket` (story 69).
 *
 * Answers once the create is queued (#779): `ticket_entity_id` is `null`, and
 * {@link actionItemTicket} names the ticket once it is filed and mirrored.
 *
 * The **existing** create-ticket op, carried by the write queue like any other
 * write; `project` is the project's entity id, which is what {@link
 * listProjects} hands the picker.
 */
export function createActionItemTicket(
  noteId: string,
  project: string,
  ticketType: string,
  title: string,
  body: string,
): Promise<ActionItemTicket> {
  return invoke<ActionItemTicket>("create_action_item_ticket", {
    noteId,
    project,
    ticketType,
    title,
    body,
  });
}

/**
 * Where an action item's ticket stands — `action_item_ticket` (#779): its
 * write, and the ticket, linked to the protocol, once it is filed and
 * mirrored. Asked as the queue moves, to fill in the address of a ticket
 * drawn as on its way.
 */
export function actionItemTicket(noteId: string, writeId: number): Promise<ActionItemTicket> {
  return invoke<ActionItemTicket>("action_item_ticket", { noteId, writeId });
}

/**
 * What `notify` shows — `knobas_app::notify::NotificationDraft` (#339).
 *
 * `address` is where the click goes, the inbox's own `addressOf` hash
 * (`lib/inbox/address.ts`, shared with the inbox row's *Open* since #447);
 * the backend carries it unread and hands it back on
 * `EVENTS.notificationClicked`, so both ends of the click path stay the
 * frontend's.
 */
export interface NotificationDraft {
  title: string;
  body: string;
  address: string;
}

/**
 * The `notification:clicked` payload — `knobas_app::notify::NotificationClicked`
 * (#339): the `address` the notification was sent with.
 */
export interface NotificationClicked {
  address: string;
}

/**
 * Show a desktop notification whose click comes back as
 * `EVENTS.notificationClicked` — `notify` (#339).
 *
 * knobas's own Rust sends it through `notify-rust` and waits on the handle,
 * because `@tauri-apps/plugin-notification`'s desktop `notify` drops that
 * handle and its `onAction` exists on mobile only. Resolves once the OS has
 * the notification, not when it is clicked. Rejects with `internal` and the
 * platform's sentence when the OS refused — on macOS a bare `tauri dev`
 * binary is refused by `UNUserNotificationCenter` (no bundle identifier).
 */
export function notify(draft: NotificationDraft): Promise<void> {
  return invoke<void>("notify", { draft });
}

/**
 * How a checkout was arrived at, or that it was not —
 * `knobas_app::checkout::FoundBy` (#499).
 *
 * `"override"` is a path a person set by hand and it wins; `"scan"` is a clone
 * matched under the clones root; `"nothing"` is *no checkout*, which is a
 * state and not a failure.
 */
export type CheckoutFoundBy = "override" | "scan" | "nothing";

/**
 * What a repo or branch detail draws for its checkout —
 * `knobas_app::checkout::CheckoutView` (#499).
 *
 * A **checkout** (`CONTEXT.md`) is knobas-owned data about the local disk,
 * never a field of the mirrored repo. `clone_command` is text to copy and is
 * never run: knobas issues no clone, no checkout and no fetch (ADR-0016).
 */
export interface CheckoutView {
  /**
   * The repo the answer belongs to — the entity itself for a repo, its
   * repository for a branch, `null` for a branch whose repo is not mirrored.
   * What {@link setCheckoutOverride} keys on.
   */
  repo_entity_id: string | null;
  /** The repo's own stored URL, or `null` where the adapter reported no page. */
  repo_url: string | null;
  /** Where the clone is, if there is one. */
  path: string | null;
  found_by: CheckoutFoundBy;
  /** The configured clones root, so *not set* and *not found* stay apart. */
  clones_root: string | null;
  /** `git clone <the repo's URL>` — `null` exactly when `repo_url` is. */
  clone_command: string | null;
  /**
   * Where the working tree at `path` stands, read by read-only git when the
   * view was asked for (ADR-0021, #704). `null` exactly when `path` is.
   */
  tree: TreeRead | null;
}

/**
 * What a checkout's working tree is doing —
 * `knobas_app::checkout::working_tree::WorkingTreeState` (#704).
 */
export interface WorkingTreeState {
  /** The branch checked out; `null` for a detached head. */
  branch: string | null;
  /** The commit checked out; `null` in a repository with no commit yet. */
  head: string | null;
  /** The upstream the branch tracks, as git names it (`origin/main`). */
  upstream: string | null;
  /** Commits the upstream does not have: *to push*. */
  ahead: number;
  /** Commits on the upstream the branch does not have: *to pull*. */
  behind: number;
  /** Tracked paths that differ from `HEAD`, staged or not. */
  changed: number;
  /** Paths git neither tracks nor ignores. */
  untracked: number;
  /** Paths with an unresolved merge conflict. */
  conflicted: number;
}

/**
 * The working tree read, or why there is none —
 * `knobas_app::checkout::working_tree::TreeRead` (#704). None is an error:
 * each arm is a sentence the checkout panel says.
 */
export type TreeRead =
  | ({ status: "read" } & WorkingTreeState)
  | { status: "no_git" }
  | { status: "not_a_repo" }
  | { status: "failed"; reason: string };

/** Where knobas looks for clones — `clones_root` (#499). */
export function clonesRoot(): Promise<string | null> {
  return invoke<string | null>("clones_root");
}

/**
 * Set the clones root, or clear it — `set_clones_root` (#499).
 *
 * A blank path clears it: an empty root would make the scan walk whatever the
 * process's working directory happens to be.
 */
export function setClonesRoot(path: string | null): Promise<void> {
  return invoke<void>("set_clones_root", { path });
}

/**
 * The checkout for a repo or a branch — `entity_checkout` (#499).
 *
 * Read on every open rather than stored, so a clone that moved is a *no
 * checkout* and not a path pointing at nothing. Rejects with `invalid` for an
 * entity that is neither a repo nor a branch.
 */
export function entityCheckout(entityId: string): Promise<CheckoutView> {
  return invoke<CheckoutView>("entity_checkout", { entityId });
}

/**
 * Set this repository's checkout path by hand, or clear it back to the scan —
 * `set_checkout_override` (#499).
 *
 * Answers the fresh view, so clearing an override shows the scan taking over.
 * Keyed on the repository even when a branch was the address.
 */
export function setCheckoutOverride(
  entityId: string,
  path: string | null,
): Promise<CheckoutView> {
  return invoke<CheckoutView>("set_checkout_override", { entityId, path });
}

/**
 * One application as #501's buttons draw it —
 * `knobas_app::checkout::open::OpenCommandView`. Since #844 a view over the
 * *Open in…* list (`checkoutSettings`): one row per application.
 *
 * `is_default` says whether the command is the one this platform ships for
 * that id, so the rooms settings field can offer to go back to it.
 */
export interface CheckoutCommand {
  /** The application's id — what {@link openCheckout} takes. */
  action: string;
  /** What the button says: "Open in VS Code". */
  label: string;
  template: string | null;
  is_default: boolean;
}

/** Every application as a button — `checkout_commands` (#501, #844). */
export function checkoutCommands(): Promise<CheckoutCommand[]> {
  return invoke<CheckoutCommand[]>("checkout_commands");
}

/**
 * Set one application's command, or put back the one this platform ships —
 * `set_checkout_command` (#501, #844). Answers the fresh list. Rejects with
 * `invalid` for a command knobas could not run.
 */
export function setCheckoutCommand(
  action: string,
  template: string | null,
): Promise<CheckoutCommand[]> {
  return invoke<CheckoutCommand[]>("set_checkout_command", { action, template });
}

/**
 * One *Open in…* application — `knobas_core::checkout::open::Application`
 * (#844, R-8.21). Its commands take `{path}`, `{file}`, `{line}` and
 * `{name}`, all filled from this disk.
 */
export interface CheckoutApplication {
  /** What {@link openCheckout} takes. */
  id: string;
  name: string;
  /** Opens the checkout. */
  command: string;
  /** Opens a file in it, if the application has one. */
  file_command: string | null;
}

/** An application as the dialog saves it: a new one has no id. */
export interface CheckoutApplicationDraft {
  id: string | null;
  name: string;
  command: string;
  file_command: string | null;
}

/**
 * The `git` knobas runs at a checkout — `knobas_app::checkout::open::GitFound`
 * (#844). Where it is, not its version: knobas runs git only for `status`,
 * `diff` and `clone`.
 */
export type GitFound =
  | { status: "found"; path: string }
  | { status: "no_git" }
  | { status: "failed"; reason: string };

/**
 * Settings › Checkouts, and what every *Open in…* menu reads —
 * `knobas_app::checkout::open::CheckoutSettings` (#844, R-13a.3).
 */
export interface CheckoutSettings {
  clones_root: string | null;
  git: GitFound;
  /** The default's id; `null` only when there are no applications. */
  default_application: string | null;
  /** In the order the person put them. */
  applications: CheckoutApplication[];
}

/** The applications, the default, the clones root and the git — `checkout_settings` (#844). */
export function checkoutSettings(): Promise<CheckoutSettings> {
  return invoke<CheckoutSettings>("checkout_settings");
}

/**
 * Save the applications in order, and which is the default —
 * `set_checkout_applications` (#844). Answers the fresh settings. Rejects
 * with `invalid` for an entry with no name or a command knobas could not run.
 */
export function setCheckoutApplications(
  applications: CheckoutApplicationDraft[],
  defaultApplication: string | null,
): Promise<CheckoutSettings> {
  return invoke<CheckoutSettings>("set_checkout_applications", { applications, defaultApplication });
}

/** What {@link openCheckout} runs, and where. */
export interface OpenAt {
  /** An application's id; the default when absent. */
  application?: string | null;
  /** A file inside the checkout, relative to it: the file command runs. */
  file?: string | null;
  /** Fills `{line}`. */
  line?: number | null;
  /** A command not saved yet, run instead of an application (*Try it*). */
  command?: string | null;
}

/**
 * Run an application at this entity's checkout — `open_checkout` (#501,
 * #844). Answers the argument vector it started.
 *
 * The one call that starts a program a person chose. Every value filled into
 * it is from this disk: the checkout path and a file inside it, never a field
 * of the mirrored repo (ADR-0016). Resolves once the program has started.
 */
export function openCheckout(entityId: string, at: OpenAt = {}): Promise<string[]> {
  return invoke<string[]>("open_checkout", {
    entityId,
    application: at.application ?? null,
    file: at.file ?? null,
    line: at.line ?? null,
    command: at.command ?? null,
  });
}

/** Why a clone failed — `knobas_app::checkout::clone::FailureCause` (#823). */
export type CloneFailureCause = "auth" | "disk_full" | "other";

/**
 * Where one clone is — `knobas_app::checkout::clone::CloneState` (#823).
 * `percent` is the whole clone, 0 to 100. `failed` and `stopped` say whether
 * the half-made folder was removed.
 */
export type CloneState =
  | { state: "progress"; phase: string; percent: number }
  | { state: "done" }
  | { state: "failed"; reason: string; cause: CloneFailureCause; removed: boolean }
  | { state: "stopped"; removed: boolean };

/**
 * The `clone:progress` payload — `knobas_app::checkout::clone::CloneEvent`
 * (#823): the repo entity being cloned, the folder, and its state.
 */
export type CloneEvent = {
  entity_id: string;
  path: string;
} & CloneState;

/** What {@link cloneRepo} answers — `knobas_app::checkout::clone::CloneAnswer`. */
export type CloneAnswer =
  | { status: "cloning"; path: string }
  | { status: "adopted"; path: string }
  | { status: "stopped"; path: string; removed: boolean }
  | { status: "not_cloning" };

/**
 * Clone a repository into `<clones root>/<name>`, or stop the clone that is
 * running — `clone_repo` (#823, ADR-0023).
 *
 * Starting resolves at once with `cloning` (progress follows on
 * `EVENTS.cloneProgress`) or `adopted` (the folder is already a clone of it).
 * Stopping resolves once the half-made folder is removed. A folder that
 * exists and is not a clone of this repository, or that another repository
 * of the same name is cloning into, rejects with `conflict`.
 */
export function cloneRepo(entityId: string, stop?: boolean): Promise<CloneAnswer> {
  return invoke<CloneAnswer>("clone_repo", { entityId, stop: stop ?? null });
}

/**
 * One half of a checkout's uncommitted diff —
 * `knobas_app::checkout::uncommitted::CappedDiff` (#824).
 */
export interface CappedDiff {
  files: DiffFile[];
  /** A cap cut this half: the per-view line cap, or the read cap that stopped git. */
  truncated: boolean;
}

/**
 * What is uncommitted at a checkout —
 * `knobas_app::checkout::uncommitted::UncommittedDiff` (#824, ADR-0026).
 */
export interface UncommittedDiff {
  /** Changed in the working tree and not staged (`git diff`). */
  unstaged: CappedDiff;
  /** Staged and not committed (`git diff --cached`). */
  staged: CappedDiff;
  /** Paths git neither tracks nor ignores; a new directory once, with its `/`. */
  untracked: string[];
  /** More untracked paths than the list holds. */
  untracked_truncated: boolean;
}

/**
 * The uncommitted diff, or why there is none —
 * `knobas_app::checkout::uncommitted::DiffRead` (#824). The arms are
 * {@link TreeRead}'s; none is an error.
 */
export type DiffRead =
  | ({ status: "read" } & UncommittedDiff)
  | { status: "no_git" }
  | { status: "not_a_repo" }
  | { status: "failed"; reason: string };

/**
 * The uncommitted changes at a repo's or a branch's checkout —
 * `entity_checkout_diff` (#824, ADR-0026). Read when the *Local changes* tab
 * opens. Rejects with `not_found` for an entity with no checkout on this
 * machine and `invalid` for one that is neither a repo nor a branch.
 */
export function entityCheckoutDiff(entityId: string): Promise<DiffRead> {
  return invoke<DiffRead>("entity_checkout_diff", { entityId });
}

/**
 * The capture shortcut as the settings pane draws it —
 * `knobas_app::capture::ShortcutView` (#503).
 *
 * Three states in two fields. `accelerator: null` is the default and means
 * knobas has registered nothing: no key is taken from any other application
 * until somebody chooses one. An accelerator with `refusal: null` is holding.
 * An accelerator with a `refusal` is stored and **not** holding — the plugin
 * could not parse it, or the operating system would not give the combination
 * up — and the sentence is the one to show, because *which* refusal it was is
 * the whole of what the reader can act on.
 */
export interface CaptureShortcut {
  accelerator: string | null;
  refusal: string | null;
  /**
   * *Show notes and quick capture* (#832, D10). Off, the accelerator is kept
   * and not registered, and `refusal` is `null`.
   */
  shown: boolean;
  /** How many notes there are (#832). */
  note_count: number;
  /** RFC 3339: when the newest note was last changed; `null` with no notes. */
  last_note_at: string | null;
}

/** What the capture shortcut is set to, and whether it holds — `capture_shortcut` (#503). */
export function captureShortcut(): Promise<CaptureShortcut> {
  return invoke<CaptureShortcut>("capture_shortcut");
}

/**
 * Set the capture shortcut, or clear it with a blank one —
 * `set_capture_shortcut` (#503).
 *
 * Answers the fresh view, so the field draws what is now stored. A combination
 * the plugin or the operating system refuses does **not** reject: it is stored,
 * and the answer carries the refusal. See {@link CaptureShortcut}.
 */
export function setCaptureShortcut(accelerator: string | null): Promise<CaptureShortcut> {
  return invoke<CaptureShortcut>("set_capture_shortcut", { accelerator });
}

/**
 * Where the reader is standing, as the main window last recorded it —
 * `knobas_app::capture::Recorded` (#503).
 *
 * The two ids a capture attaches, and nothing else. `context` is the **stored**
 * room's `ctx:` entity, `null` for every derived room because a derived room
 * has none; `foreground` is the **foreground** as `CONTEXT.md`'s *Passive
 * attribution* defines it, which is `shell/timer.ts`'s `roomForeground` and no
 * second spelling of it.
 */
export interface CaptureContext {
  context: string | null;
  foreground: string | null;
}

/**
 * Record where the reader is standing — `record_capture_context` (#503).
 *
 * The **main** window's to call, whenever either half changes. The capture
 * window is a webview with no shell in it and cannot work either out; this is
 * how it is told.
 */
export function recordCaptureContext(
  context: string | null,
  foreground: string | null,
): Promise<void> {
  return invoke<void>("record_capture_context", { context, foreground });
}

/** What the main window last recorded — `capture_context` (#503). */
export function captureContext(): Promise<CaptureContext> {
  return invoke<CaptureContext>("capture_context");
}

/**
 * Bring the main window forward and open one note in it — `reveal_note` (#503).
 *
 * The capture window's *Open in knobas* button. It does not close the capture
 * window; the capture window closes itself, so that a main window which would
 * not come forward leaves the reader holding the words they typed.
 */
export function revealNote(noteId: string): Promise<void> {
  return invoke<void>("reveal_note", { noteId });
}

/** One comment of a ticket's thread — `knobas_app::conversation::ThreadComment` (#819). */
export interface ThreadComment {
  /** The name a person reads (Jira's display name), else the account; `null` for nobody. */
  author: string | null;
  /** RFC 3339; `null` when the source's stamp is absent or unreadable. */
  at: string | null;
  /** The source's own text (Jira's wiki markup), drawn as text. */
  body: string;
  /** The reader wrote it. */
  mine: boolean;
  /** It names the reader: `@name` or Jira's `[~name]`, for an account `@me` resolves to. */
  mentions_me: boolean;
}

/** A status move on the thread — `knobas_app::conversation::ThreadChange` (#819). */
export interface ThreadChange {
  at: string;
  /** The status the ticket was moved to. */
  status: string;
}

/** A ticket's thread — `knobas_app::conversation::Thread` (#819). */
export interface Thread {
  /** In the source's order, oldest first. */
  comments: ThreadComment[];
  /** The moves knobas delivered: status changes, which are never comments. */
  changes: ThreadChange[];
  /** The accounts `@me` resolves to, to mark `@name` in a body by. */
  me: string[];
}

/**
 * Record that the reader opened this ticket's job, and answer when they last
 * did before now — `visit_entity` (#819, R-5.5). `null` on a first visit.
 * Rejects with `invalid` for a non-id and `not_found` for an unknown entity.
 */
export function visitEntity(entityId: string): Promise<string | null> {
  return invoke<string | null>("visit_entity", { entityId });
}

/**
 * A ticket's thread — `entity_thread` (#819, R-5.2): its comments, each marked
 * when it is the reader's or mentions them, and the status moves knobas sent.
 * Rejects with `invalid` for a non-id and `not_found` for one the mirror does
 * not hold.
 */
export function entityThread(entityId: string): Promise<Thread> {
  return invoke<Thread>("entity_thread", { entityId });
}

/**
 * Turn *Show notes and quick capture* on or off — `set_notes_shown` (#832,
 * D10). Registers or releases the stored capture shortcut to match, and
 * answers the capture shortcut's view, which carries the setting.
 */
export function setNotesShown(shown: boolean): Promise<CaptureShortcut> {
  return invoke<CaptureShortcut>("set_notes_shown", { shown });
}

/** What *Export notes as Markdown* wrote — `knobas_app::notes_export::NotesExport` (#832). */
export interface NotesExport {
  /** Notes written, one `.md` file each. */
  count: number;
  /** The folder they are in, absolute. */
  folder: string;
}

/**
 * *Export notes as Markdown* — `export_notes_markdown` (#832, D10): one `.md`
 * per note, `[[refs]]` kept as text, in `~/Downloads/knobas-notes`.
 */
export function exportNotesMarkdown(): Promise<NotesExport> {
  return invoke<NotesExport>("export_notes_markdown");
}

// -- the Docs reader (#859, ADR-0022) ----------------------------------------

/** Every space the mirror holds pages of, and the page read last — `DocsTree`. */
export interface DocsTree {
  /** Ordered by source, then key. */
  spaces: DocSpace[];
  /** The page opened most recently (`visit_entity`) while it is still live; `#/docs` opens it. */
  last_read: string | null;
  /** *For you* (#860): the pages whose mention of the reader is in the inbox now, newest first. */
  for_you: DocForYou[];
}

/** A page that needs the reader, and who asked — `DocForYou`. */
export interface DocForYou {
  entity_id: string;
  title: string;
  space_key: string | null;
  /** Who mentioned the reader; `null` where the record does not say. */
  by_name: string | null;
}

/** One space and its pages — `DocSpace`. */
export interface DocSpace {
  source_id: string;
  key: string;
  /** `null` where no page of it carries a name knobas can read. */
  name: string | null;
  /** Every live page of the space, ordered by title. */
  pages: DocNode[];
}

/** One page in a space's tree — `DocNode`. */
export interface DocNode {
  entity_id: string;
  title: string;
  /** The mirrored page above it in the same space, `null` at the top. */
  parent: string | null;
}

/** One page as the reader draws it — `DocPage`. */
export interface DocPage {
  entity_id: string;
  source_id: string;
  /** The source's configured name: *Open in …* and the placeholder's words. */
  source_name: string;
  title: string;
  space: DocSpaceRef | null;
  /** The parent chain, outermost first. */
  ancestors: DocAncestor[];
  /** Who made this version, in the source's account spelling. */
  editor: string | null;
  /** The editor's display name, where the source's record carries one. */
  editor_name: string | null;
  updated_at: string | null;
  /** The page in the source. */
  web_url: string | null;
  /** Whether the source's adapter projects its pages; `false` leaves the fields below empty. */
  readable: boolean;
  version: number | null;
  labels: string[];
  blocks: DocBlock[];
  comments: DocComment[];
  /** The identity `@me` resolves to. */
  me: string[];
  /** The sections, numbered as the anchors are, each with its text or why knobas does not edit it (#862). */
  sections: DocSection[];
  /** The versions the record names, newest first (#861). */
  history: DocVersion[];
  /** The pages of the same source whose body links here (#861). */
  backlinks: DocBacklink[];
  /** The tickets the page references or is linked to, with their status now (#861). */
  work: DocWork[];
  /** Which Docs writes the page's source takes (#862). */
  writes: DocWrite[];
}

/** A Docs write a page's source takes — `DocPage.writes`. */
export type DocWrite = "comment" | "update_page" | "create_page";

/** Why knobas does not edit a section — `SectionRefusal`. */
export type SectionRefusal = "table" | "code" | "macro" | "formatting";

/** One section of a page — `knobas_source::docs::DocSection`. */
export interface DocSection {
  /** 1 for the first top-level heading. */
  number: number;
  /** The plain text the editor opens with; `null` when refused. */
  text: string | null;
  refusal: SectionRefusal | null;
}

/** One version of a page — `knobas_source::docs::DocVersion`. */
export interface DocVersion {
  number: number | null;
  by: string | null;
  by_name: string | null;
  /** RFC 3339, as the source wrote it. */
  at: string | null;
  message: string | null;
}

/** A page that links here — `DocBacklink`. */
export interface DocBacklink {
  entity_id: string;
  title: string;
  space_key: string | null;
}

/** A ticket a page belongs to — `DocWork`. */
export interface DocWork {
  entity_id: string;
  /** The job's address. */
  key: string;
  title: string;
  status: string | null;
  /** Whether a knobas link draws it (else the body references it). */
  linked: boolean;
}

/** The Docs search's answer — `DocsHits` (#860). */
export interface DocsHits {
  /** Every page the query admits, before the limit. */
  total: number;
  took_ms: number;
  /** The words and phrases to mark, lowercased. */
  needles: string[];
  unknown_tokens: string[];
  hits: DocHit[];
}

/** One page the Docs search found — `DocHit`. */
export interface DocHit {
  entity_id: string;
  title: string;
  space: DocSpaceRef | null;
  /** The parent chain as one line, `null` at the top of a space. */
  path: string | null;
  snippet: DocSnippet;
  matches: number;
  editor: string | null;
  editor_name: string | null;
  updated_at: string | null;
}

/** A hit's best sentence and where it is — `DocSnippet`. Source text, never markup. */
export interface DocSnippet {
  /** The section heading it sits under. */
  section: string | null;
  /** The comment's author when it is a comment's (`""` for a nameless one). */
  comment_by: string | null;
  segments: { text: string; hit: boolean }[];
}

/** What *Export as Markdown* wrote — `DocExport` (#861). */
export interface DocExport {
  file: string;
  folder: string;
}

/** What *New page* asks for — `NewPage` (#862). */
export interface NewPage {
  /** The mirrored page it goes under. */
  parent: string;
  title: string;
  /** Plain text: `## ` a section, `- ` a list item. */
  text: string;
}

/** A space, named — `DocSpaceRef`. */
export interface DocSpaceRef {
  key: string;
  name: string | null;
}

/** One step of a page's parent chain — `DocAncestor`. */
export interface DocAncestor {
  title: string;
  entity_id: string | null;
}

/** One comment on a page — `knobas_source::docs::DocComment`. */
export interface DocComment {
  author: string | null;
  author_name: string | null;
  /** RFC 3339, as the source wrote it. */
  at: string | null;
  blocks: DocBlock[];
}

/** Which panel a panel block is — `PanelKind`. */
export type PanelKind = "info" | "note" | "warning" | "tip";

/** One cell of a table block — `TableCell`. */
export interface TableCell {
  header: boolean;
  text: DocInline[];
}

/** One block of a page body — `knobas_source::docs::DocBlock`. Source text, never markup. */
export type DocBlock =
  | { type: "heading"; level: number; text: DocInline[] }
  | { type: "paragraph"; text: DocInline[] }
  | { type: "list"; ordered: boolean; items: DocBlock[][] }
  | { type: "code"; code: string; language: string | null }
  | { type: "table"; rows: TableCell[][] }
  | { type: "panel"; kind: PanelKind; title: string | null; blocks: DocBlock[] }
  /** A part the reader does not draw, by the source's own name for it. */
  | { type: "placeholder"; name: string };

/** One run inside a block — `knobas_source::docs::Inline`. */
export type DocInline =
  | { type: "text"; text: string; strong: boolean; emphasis: boolean; code: boolean }
  | { type: "link"; text: string; href: string }
  | { type: "page_link"; text: string; title: string; space: string | null; entity_id: string | null }
  | { type: "issue"; key: string; entity_id: string | null; title: string | null; status: string | null }
  | { type: "mention"; user: string | null; name: string | null; me: boolean }
  | { type: "break" };

/** The Docs reader's left pane — `docs_tree` (#859). */
export function docsTree(): Promise<DocsTree> {
  return invoke<DocsTree>("docs_tree");
}

/**
 * One page drawn as blocks — `docs_page` (#859). Rejects with `invalid` for a
 * non-id and `not_found` for an id that is not a live page.
 */
export function docsPage(entityId: string): Promise<DocPage> {
  return invoke<DocPage>("docs_page", { entityId });
}

/**
 * The Docs search — `docs_search` (#860): the pages `query` admits in `space`
 * (the picker's; a typed `space:` wins), or `null` for a query with neither
 * words nor a filter.
 */
export function docsSearch(
  query: string,
  space: { source_id: string; key: string } | null,
): Promise<DocsHits | null> {
  return invoke<DocsHits | null>("docs_search", { query, space });
}

/** *Export as Markdown* — `docs_export_markdown` (#861): the page as `.md` in Downloads. */
export function docsExportMarkdown(entityId: string): Promise<DocExport> {
  return invoke<DocExport>("docs_export_markdown", { entityId });
}

/**
 * *Edit section* — `docs_edit_section` (#862): queue section `section`'s new
 * text against the version read. Rejects with `invalid` for a section knobas
 * does not edit or a page that moved on.
 */
export function docsEditSection(
  entityId: string,
  section: number,
  version: number | null,
  text: string,
): Promise<QueuedWrite> {
  return invoke<QueuedWrite>("docs_edit_section", { entityId, section, version, text });
}

/** *New page* — `docs_create_page` (#862), linked to `linkTo` (a ticket id) once it lands. */
export function docsCreatePage(page: NewPage, linkTo: string | null): Promise<QueuedWrite> {
  return invoke<QueuedWrite>("docs_create_page", { page, linkTo });
}

// ---- Link sets (#866, ADR-0025) ----

/** What a link-set member can be (R-7.7's five panels) — `link_sets::MEMBER_KINDS`. */
export type LinkSetMemberKind = "repo" | "pipeline" | "page" | "asset" | "monitor";

/** What a rule matches on (R-7.1) — `link_sets::RULE_KINDS`. */
export type LinkSetRuleKind = "epic" | "parent" | "project" | "manual";

/** One fixed member — `knobas_app::link_sets::LinkSetMember`. */
export interface LinkSetMember {
  kind: LinkSetMemberKind;
  entity_id: string;
}

/**
 * One rule — `knobas_app::link_sets::LinkSetRule`: a ticket entity id for
 * `epic`, `parent` and `manual`, `<source id>:<project key>` for `project`.
 */
export interface LinkSetRule {
  kind: LinkSetRuleKind;
  value: string;
}

/**
 * One pattern member — `knobas_app::link_sets::LinkSetPattern` (#868, R-7.2):
 * a query in the search grammar (`project:Payouts name:Payout_*`), for
 * pipelines and pages.
 */
export interface LinkSetPattern {
  kind: LinkSetMemberKind;
  query: string;
}

/** What `save_link_set` takes — `LinkSetDraft`: the whole set, in order. */
export interface LinkSetDraft {
  /** `null` makes a new set with an id from its name. */
  id: string | null;
  name: string;
  description: string;
  members: LinkSetMember[];
  /** The patterns, after the fixed members (#868). */
  patterns: LinkSetPattern[];
  rules: LinkSetRule[];
}

/** A member as the views draw it — `MemberRow`: a fixed member, or one item a pattern matches. */
export interface LinkSetMemberRow {
  kind: LinkSetMemberKind;
  entity_id: string;
  /** `null` when knobas has no entity of that id. */
  title: string | null;
  /**
   * The line beside the name (#1008): an asset's type and environment, a
   * pipeline's project path, a page's space and path. `null` where nothing is declared.
   */
  line: string | null;
  /** The dot (#1008): a pipeline's newest run, a mirrored item's state, an asset's health. */
  state: StateChip | null;
}

/** A pattern member as the editor draws it — `PatternRow`: "(N match now)". */
export interface LinkSetPatternRow {
  kind: LinkSetMemberKind;
  query: string;
  /** How many live items it matches now. */
  count: number;
  /** The first 200 of them, by title. */
  matches: LinkSetMemberRow[];
}

/** A project or space a pattern's kind sits in — `PatternProject`. */
export interface LinkSetPatternProject {
  source_id: string;
  key: string;
  name: string | null;
}

/** The pattern dialog's "Matches now: N" — `PatternPreview`. */
export interface LinkSetPatternPreview {
  count: number;
  matches: LinkSetMemberRow[];
  projects: LinkSetPatternProject[];
  /** Why the query is not a pattern; `null` when *Add pattern* can save it. */
  refusal: string | null;
}

/** A rule as the views draw it — `RuleRow`: its ticket's title or project's name. */
export interface LinkSetRuleRow {
  kind: LinkSetRuleKind;
  value: string;
  title: string | null;
}

/** One set as the Link sets view lists it — `LinkSetRow` (R-7.6). */
export interface LinkSetRow {
  /** A slug: `#/sets/<id>`. */
  id: string;
  name: string;
  description: string;
  members: LinkSetMemberRow[];
  /** Its patterns, in order (#868). */
  patterns: LinkSetPatternRow[];
  rules: LinkSetRuleRow[];
  /** Live tickets its rules match now. */
  tickets: number;
  /** How many of them are yours. */
  mine: number;
}

/** A ticket a rule or set applies to — `RuleTicket`. */
export interface RuleTicket {
  entity_id: string;
  title: string;
  mine: boolean;
}

/** One set as its editor draws it — `LinkSetDetail` (R-7.7). */
export interface LinkSetDetail extends LinkSetRow {
  /** The tickets it shows on now, in key order. */
  shows_on: RuleTicket[];
}

/** Every link set, by name (R-7.6). */
export function linkSets(): Promise<LinkSetRow[]> {
  return invoke<LinkSetRow[]>("link_sets");
}

/** One link set and the tickets it shows on; `not_found` for an id nobody holds. */
export function linkSet(id: string): Promise<LinkSetDetail> {
  return invoke<LinkSetDetail>("link_set", { id });
}

/** Save a whole set, members and rules replaced in order; answers it as stored. */
export function saveLinkSet(draft: LinkSetDraft): Promise<LinkSetDetail> {
  return invoke<LinkSetDetail>("save_link_set", { draft });
}

/** Delete a set; links drawn on tickets by hand stay. */
export function deleteLinkSet(id: string): Promise<void> {
  return invoke<void>("delete_link_set", { id });
}

/** The live tickets one rule applies to now, yours marked (R-7.7's live counts). */
export function previewLinkSetRule(rule: LinkSetRule): Promise<RuleTicket[]> {
  return invoke<RuleTicket[]>("preview_link_set_rule", { rule });
}

/** What a pattern would match now, the projects to pick from, and why it is not a pattern (#868, R-7.7). */
export function previewLinkSetPattern(pattern: LinkSetPattern): Promise<LinkSetPatternPreview> {
  return invoke<LinkSetPatternPreview>("preview_link_set_pattern", { pattern });
}

/** *Hide here* (`hidden`) or *Show again* one set member on one ticket (R-7.5); answers the new state. */
export function hideLinkSetMember(ticketId: string, memberId: string, hidden: boolean): Promise<boolean> {
  return invoke<boolean>("hide_link_set_member", { ticketId, memberId, hidden });
}

/** One member of a set that applies to a job — `JobMember` (#867). */
export interface JobMember {
  /** The member's kind: the row's section. */
  kind: LinkSetMemberKind;
  entity_id: string;
  /** The entity's own kind, what the row links by; `null` when knobas has none. */
  entity_kind: string | null;
  title: string | null;
  /** Hidden on this ticket by *Hide here* (R-7.5). */
  hidden: boolean;
}

/** One set that applies to a job — `JobSet` (R-4.4, R-6.2). */
export interface JobSet {
  id: string;
  name: string;
  /** The first of its rules that matches the ticket, in the set's order: the reason. */
  via: LinkSetRuleRow;
  members: JobMember[];
}

/** The far end of one of the parent's own links — `JobLink`. */
export interface JobLink {
  entity_id: string;
  kind: string;
  title: string | null;
}

/** The parent whose own links a sub-task shows (R-7.3) — `JobParent`. */
export interface JobParent {
  ticket: string;
  title: string | null;
  links: JobLink[];
}

/** What a job shows beside its own links — `JobLinkSets` (#867). */
export interface JobLinkSets {
  /** The sets that apply, by name. */
  sets: JobSet[];
  /** `null` with no parent, an epic for one, or R-7.3's switch off. */
  parent: JobParent | null;
}

/** The link sets and the parent a ticket's job shows; an unknown id has neither. */
export function jobLinkSets(entityId: string): Promise<JobLinkSets> {
  return invoke<JobLinkSets>("job_link_sets", { entityId });
}
