/**
 * Ticket views' rules (#812, R-3.2, R-3.3, R-3.12), pure: how the views pane
 * groups and finds them, how a view's own columns gather the board's, what a
 * new view starts from, and what *Save as a view* and *Duplicate* save.
 */
import type { MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import type { CardField, ListColumn, TicketView, TicketViewDraft, TicketViewRow, ViewColumn } from "../../ipc/search";
import { matchesAll } from "../filter/filter";
import { DEFAULT_SORT, type LaneField, type ListSort } from "./tickets";

/** The groups that come first, in this order (R-3.2); typed groups follow. */
export const OWN_GROUPS = ["Mine", "Team", "Epics", "Projects"] as const;

/** What a card shows unless a view says otherwise (prototype-6's `VCARD`). */
export const DEFAULT_CARD_FIELDS: CardField[] = ["type", "signals", "comments", "assignee"];
/** The list's columns unless a view says otherwise (prototype-6's new view). */
export const DEFAULT_LIST_COLUMNS: ListColumn[] = ["key", "summary", "status", "priority", "signals", "assignee", "updated"];
/** Every list column, in the order the list draws them (R-3.16). */
export const LIST_COLUMN_ORDER: readonly (readonly [ListColumn, string])[] = [
  ["key", "Key"],
  ["summary", "Summary"],
  ["status", "Status"],
  ["priority", "Priority"],
  ["signals", "Signals"],
  ["assignee", "Assignee"],
  ["updated", "Updated"],
  ["epic", "Epic"],
  ["labels", "Labels"],
];

/** One group of the views pane: its name and its views in list order. */
export interface ViewGroup {
  name: string;
  views: TicketViewRow[];
}

/**
 * The views pane's groups (R-3.2): *Mine*, *Team*, *Epics* and *Projects*
 * first, then every group somebody typed in the order its first view sits,
 * each holding its views in list order. Only views `find` admits (by name
 * and by query, every word somewhere); a group left empty is not drawn.
 */
export function groupViews(views: TicketViewRow[], find: string): ViewGroup[] {
  const found = views
    .filter((view) => find.trim() === "" || matchesAll(`${view.name} ${view.query}`, find))
    .sort((a, b) => a.position - b.position || a.id.localeCompare(b.id));
  const typed = [...new Set(found.map((view) => view.group))].filter(
    (group) => !(OWN_GROUPS as readonly string[]).includes(group),
  );
  return [...OWN_GROUPS, ...typed]
    .map((name) => ({ name, views: found.filter((view) => view.group === name) }))
    .filter((group) => group.views.length > 0);
}

/**
 * The board's columns as a view's own columns gather them (R-3.15): each
 * named column holds the cards of its statuses, newest first, and counts
 * them; a status on no column does not show, as on a Jira board. No columns
 * of its own is the board's: one per status.
 */
export function viewColumns(board: MiniBoardColumn[], own: ViewColumn[]): MiniBoardColumn[] {
  if (own.length === 0) return board;
  return own.map((column) => {
    const held = board.filter((status) => status.status !== null && column.statuses.includes(status.status));
    const cards: MiniBoardCard[] = held.flatMap((status) => status.cards);
    if (held.length > 1) cards.sort((a, b) => (b.updated_at ?? "").localeCompare(a.updated_at ?? ""));
    return { status: column.name, cards, total: held.reduce((all, status) => all + status.total, 0) };
  });
}

/** A view's *At most* for a column it named, or `null`. */
export function columnLimit(own: ViewColumn[], name: string | null): number | null {
  return own.find((column) => column.name === name)?.limit ?? null;
}

/** How a view shows, with the session's switches over it (R-3.3). */
export interface Showing {
  layout: "board" | "list";
  lanes: LaneField;
  sort: ListSort;
}

/** How a view shows as saved; *All tickets* (no view) as the screen opens. */
export function savedShowing(view: TicketView | null): Showing {
  if (view === null) return { layout: "board", lanes: "none", sort: DEFAULT_SORT };
  return { layout: view.layout, lanes: view.lanes, sort: view.list_sort ?? DEFAULT_SORT };
}

/**
 * What *Save as a view* starts from (R-3.12): the text the filter bar sends
 * now, which is the current view's conditions plus the chips, dropdowns and
 * words, and how the tickets show now, layout, lanes and sort included. The
 * rest of the current view's setup comes along; *All tickets* brings the
 * defaults. The name is the reader's to give.
 */
export function draftFromFilter(view: TicketView | null, query: string, showing: Showing): TicketViewDraft {
  return {
    ...draftOf(view),
    name: "",
    query,
    layout: showing.layout,
    lanes: showing.lanes,
    list_sort: sameSort(showing.sort, DEFAULT_SORT) ? null : showing.sort,
  };
}

/** What *Duplicate* starts from (R-3.12): the view as saved, called *Copy of …*. */
export function draftCopy(view: TicketView): TicketViewDraft {
  return { ...draftOf(view), name: `Copy of ${view.name}` };
}

/** What *+ New view* starts from (R-3.12): no conditions, one column per status, the defaults. */
export function draftNew(): TicketViewDraft {
  return draftOf(null);
}

/** What *Edit view* starts from: the view as saved, under its id and in its place. */
export function draftEdit(view: TicketView): TicketViewDraft {
  return draftRestore(view);
}

/** What *Undo* on a delete saves: the view back, under its id and in its place. */
export function draftRestore(view: TicketView): TicketViewDraft {
  return { ...draftOf(view), id: view.id, position: view.position, jira_ref: view.jira_ref };
}

function draftOf(view: TicketView | null): TicketViewDraft {
  return {
    id: null,
    name: view?.name ?? "",
    group: view?.group ?? "Mine",
    position: null,
    query: view?.query ?? "",
    layout: view?.layout ?? "board",
    columns: view?.columns.map((column) => ({ ...column, statuses: [...column.statuses] })) ?? [],
    lanes: view?.lanes ?? "none",
    card_fields: [...(view?.card_fields ?? DEFAULT_CARD_FIELDS)],
    list_columns: [...(view?.list_columns ?? DEFAULT_LIST_COLUMNS)],
    list_sort: view?.list_sort ?? null,
    // A copy is your own view: Jira decides nothing about it.
    jira_ref: null,
  };
}

function sameSort(a: ListSort, b: ListSort): boolean {
  return a.column === b.column && a.direction === b.direction;
}
