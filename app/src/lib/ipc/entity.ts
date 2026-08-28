/** Entity and room reads — `crates/knobas-app/src/commands/entity.rs`. */
import { invoke } from "@tauri-apps/api/core";

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

/** One link — `knobas_core::link::LinkRow`. Always empty in M1. */
export interface LinkRow {
  id: string;
  from_id: string;
  to_id: string;
  relation: string;
  origin: "manual" | "suggested" | "imported" | "source" | "implied";
  created_by: string;
  /** RFC 3339. */
  created_at: string;
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
  /** Always `[]` in M1: nothing writes `knobas.link` until M2. */
  links: LinkRow[];
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
