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

/** The `limit` most recent activity lines, newest first. */
export function recentActivity(limit: number): Promise<ActivityRow[]> {
  return invoke<ActivityRow[]>("recent_activity", { limit });
}
