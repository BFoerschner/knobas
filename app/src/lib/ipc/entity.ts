/** Entity and room reads — `crates/knobas-app/src/commands/entity.rs`. */
import { invoke } from "@tauri-apps/api/core";

// The write queue's own types live with the queue's other commands. A
// write-back is an operation on an entity, which is why `submitWrite` is here;
// what it hands over and what comes back are the queue's, and a second
// declaration of either would be two shapes for one wire format.
import type { QueuedWrite, WriteOpPayload } from "./sources";

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
   * `null` throughout M1 phase 1 — resolving it needs the adapter registry
   * (task 21). The header falls back to the title-cased kind, which is what
   * §3a asks for when no adapter declares one anyway.
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
 * The tray holds no state: this call is the whole of it.
 */
export function roomSuggestions(sources: string[], limit: number): Promise<SuggestionPage> {
  return invoke<SuggestionPage>("room_suggestions", { sources, limit });
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
