/** Entity and room reads — `crates/knobas-app/src/commands/entity.rs`. */
import { invoke } from "@tauri-apps/api/core";

// The write queue's own types live with the queue's other commands. A
// write-back is an operation on an entity, which is why `submitWrite` is here;
// what it hands over and what comes back are the queue's, and a second
// declaration of either would be two shapes for one wire format.
import type { QueuedWrite, WriteOpPayload, WriteState } from "./sources";
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
   * page's ancestor path, `Engineering › Payments` (#284).
   *
   * `null` for every row whose record carries no readable `ancestors`, which
   * is every kind but a page today: the ADR-0007 **miss**, so a row shows no
   * path rather than a wrong one and no caller has to ask what kind it holds.
   * Joined by one SQL statement, `knobas_core::ancestor_path_read!`, so the
   * launcher row and the detail panel cannot disagree about what a path is.
   */
  path: string | null;
}

/** Which ordering a room reads in — `EntityOrder`. */
export type EntityOrder = "updated_desc" | "title_asc";

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
}

/** One page of a room — `EntityPage`. */
export interface EntityPage {
  rows: EntityRow[];
  /** The whole filtered set, before `limit`/`offset`. `0` for an empty page. */
  total: number;
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
 * Write a new note — `knobas_app::commands::entity::create_note`.
 *
 * Both arguments are optional: *New note* creates the row before the first
 * keystroke, so nothing typed into it can be lost to a closed window.
 */
export function createNote(title?: string, bodyMd?: string): Promise<NoteDetail> {
  return invoke<NoteDetail>("create_note", { title, bodyMd });
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
  /** Newest first. The column header's count is `cards.length`. */
  cards: MiniBoardCard[];
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
   * shows — what the ticket detail's status select offers (#179).
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
 * An unknown context is an empty board rather than a rejection — the tile has
 * to be able to say "nothing here" without saying "this broke".
 */
export function miniBoard(
  filter: Pick<EntityFilter, "sources" | "context" | "project">,
): Promise<MiniBoard> {
  return invoke<MiniBoard>("mini_board", {
    ctxId: filter.context,
    sources: filter.sources,
    project: filter.project,
  });
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
 * The standup's three lists — `knobas_app::standup::StandupDigest` (#288).
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
