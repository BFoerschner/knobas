/**
 * Sources › Jira › *Boards and filters* (#878, R-3.20, R-13a.2, D17): which
 * Jira boards and whether the favourite filters come in as Jira views, and
 * every Jira view the source keeps, with its tickets and *Hide* / *Show*.
 *
 * The setting is two keys of the adapter's own config, `boards` and
 * `favourite_filters`, which its schema marks for this section
 * (`x-knobas-section`), so the generated form leaves them out. The keys, the
 * four choices and the Jira view words are Jira's, which is the only adapter
 * that declares the section; the page draws it for whichever adapter does.
 */
import type { SavedViewSource, TicketViewRow, UnsyncedTickets } from "../../ipc/search";
import type { SourceDescriptor } from "../../ipc/sources";

/** The section's name in an adapter's schema. */
export const BOARDS_SECTION = "boards-and-filters";

export type BoardScope = "synced" | "favourites" | "all" | "none";

/**
 * The choices, in prototype-6's order. The second reads a favourite *filter*, not a board star
 * (Jira DC has no favourite-board read we can witness, #878), so its label says that rather
 * than prototype-6's "boards I marked as favourites" (#1096).
 */
export const BOARD_SCOPES: readonly (readonly [BoardScope, string])[] = [
  ["synced", "Boards on the projects this source syncs"],
  ["favourites", "Only boards built on my favourite filters"],
  ["all", "Every board I can see"],
  ["none", "No boards"],
];

function field(config: unknown, key: string): unknown {
  return typeof config === "object" && config !== null ? (config as Record<string, unknown>)[key] : undefined;
}

/**
 * The ticket system an adapter's saved views come from (#2032): the declared
 * `system_name` of the first kind it emits that declares the saved-view frame,
 * else the adapter's name. A restatement of `knobas_sync::saved_views::declared`,
 * the authority, which names the views' groups and their rows' `system` the same way.
 */
export function savedViewSystem(adapter: Pick<SourceDescriptor, "name" | "entity_kinds" | "payload_paths">): string {
  const first = adapter.payload_paths.find(
    (paths) => paths.frame === "saved_view" && adapter.entity_kinds.some((kind) => kind.id === paths.kind),
  );
  return first?.system_name ?? adapter.name;
}

/** The stored choice; one the config leaves out, or does not know, is the default (D17). */
export function boardScope(config: unknown): BoardScope {
  const value = field(config, "boards");
  return BOARD_SCOPES.some(([scope]) => scope === value) ? (value as BoardScope) : "synced";
}

/** Whether the favourite filters come in; on unless the config says `false`. */
export function favouriteFilters(config: unknown): boolean {
  return field(config, "favourite_filters") !== false;
}

/** A saved view of the source `sourceId`, with what the source says about it. */
export type SavedViewRow = TicketViewRow & { saved_view_source: SavedViewSource };

/** The source's saved views: the boards, then the filters, each in list order. */
export function savedViewsOf(views: TicketViewRow[], sourceId: string): SavedViewRow[] {
  return views
    .filter((view): view is SavedViewRow => view.saved_view_source !== null && (view.saved_view ?? "").startsWith(`${sourceId}:`))
    .sort(
      (a, b) =>
        Number(a.saved_view_source.kind === "filter") - Number(b.saved_view_source.kind === "filter") ||
        a.position - b.position ||
        a.id.localeCompare(b.id),
    );
}

/** *Scrum board*, *Kanban board*, *Board* or *Filter*. */
export function viewType({ kind, board_type }: Pick<SavedViewSource, "kind" | "board_type">): string {
  if (kind === "filter") return "Filter";
  const type = board_type ?? "";
  return type === "" || type === "simple" ? "Board" : `${type[0]!.toUpperCase()}${type.slice(1)} board`;
}

/** *and 3 in WEB, not synced*, or `null` when knobas mirrors every ticket on it. */
export function unsyncedWords(unsynced: UnsyncedTickets[]): string | null {
  if (unsynced.length === 0) return null;
  return `and ${unsynced.map((one) => `${one.count} in ${one.project}`).join(", ")}, not synced`;
}

export { counted } from "../counted";
