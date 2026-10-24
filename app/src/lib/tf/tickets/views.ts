/**
 * Ticket views' rules (#812, R-3.2, R-3.3, R-3.12), pure: how the views pane
 * groups and finds them, how a view's own columns gather the board's, what a
 * new view starts from, and what *Save as a view* and *Duplicate* save.
 */
import type { MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import type {
  CardField,
  LeftOut,
  ListColumn,
  QueryTranslation,
  SavedViewSource,
  TicketView,
  TicketViewDraft,
  TicketViewRow,
  ViewColumn,
} from "../../ipc/search";
import { matchesAll } from "../filter/filter";
import { CONDITION_FIELDS, conditionsText } from "./editor";
import { DEFAULT_SORT, type LaneField, type ListSort } from "./tickets";

/** The groups that come first, in this order (R-3.2); typed groups follow. */
export const OWN_GROUPS = ["Mine", "Team", "Epics", "Projects"] as const;
/**
 * A saved view's own columns as the board read draws them (#877): the read
 * has already gathered its source's columns, each under its own name, so each
 * holds just that name, keeping its limit.
 */
export function drawnSavedViewColumns(own: ViewColumn[]): ViewColumn[] {
  return own.map((column) => ({ ...column, statuses: [column.name] }));
}

/** The source a saved view comes from: its `saved_view`'s source id (`jira:board-21` → `jira`). */
export function savedViewSourceId(ref: string): string {
  return ref.split(":")[0]!;
}

/** What a saved view is read from, in prototype-6's words (#879): *Kanban board*, *saved filter*. */
export function savedViewKindWords({ kind, board_type }: Pick<SavedViewSource, "kind" | "board_type">): string {
  if (kind !== "board") return "saved filter";
  const type = board_type ? board_type[0]!.toUpperCase() + board_type.slice(1) : "";
  return `${type} board`.trim();
}

/** A sprint's end as the header says it (#879): *22 Oct*, in the reader's zone; `""` for a date that is not one. */
export function sprintEnds(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? "" : at.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}
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

/** One group of the views pane: its name, its views in list order, and the ticket system a synced group's views come from. */
export interface ViewGroup {
  name: string;
  views: TicketViewRow[];
  /** The ticket system a group of saved views comes from (*Jira*, #2032); `null` for a group of your own. */
  system: string | null;
}

/**
 * The views pane's groups (R-3.2): *Mine*, *Team*, *Epics* and *Projects*
 * first, then every group somebody typed in the order its first view sits,
 * each holding its views in list order, then the groups of saved views their
 * sources bring in at every sync (#877, R-3.20): per ticket system in the
 * order its first view sits, boards before filters. A group is a synced one
 * when a view in it has a `saved_view_source`, whatever it is called (#2032).
 * Only views `find` admits (by name and by query, every word somewhere); a
 * group left empty is not drawn.
 */
export function groupViews(views: TicketViewRow[], find: string): ViewGroup[] {
  const found = views
    .filter((view) => view.saved_view_source?.hidden !== true)
    .filter((view) => find.trim() === "" || matchesAll(`${view.name} ${view.saved_view_source?.source_query ?? view.query}`, find))
    .sort(listOrder);
  const groups = [...new Set([...OWN_GROUPS, ...found.map((view) => view.group)])].map((name) => {
    const held = found.filter((view) => view.group === name);
    return { name, views: held, system: held.find((view) => view.saved_view_source !== null)?.saved_view_source?.system ?? null };
  });
  const systems = savedViewSystems(found);
  const synced = (group: ViewGroup) => (group.system === null ? -1 : systems.indexOf(group.system));
  const filters = (group: ViewGroup) => Number(group.views.some((view) => view.saved_view_source?.kind === "filter"));
  return groups
    .filter((group) => group.views.length > 0)
    .sort((a, b) => synced(a) - synced(b) || (a.system === null ? 0 : filters(a) - filters(b)));
}

function listOrder(a: TicketViewRow, b: TicketViewRow): number {
  return a.position - b.position || a.id.localeCompare(b.id);
}

/** The ticket systems `views`' saved views come from (#2032), each once, in the order its first view sits. */
export function savedViewSystems(views: TicketViewRow[]): string[] {
  return [
    ...new Set(
      [...views]
        .sort(listOrder)
        .flatMap((view) => (view.saved_view_source ? [view.saved_view_source.system] : [])),
    ),
  ];
}

/** Ticket systems as a sentence names them (#2032): *Jira*, *Jira and Kestrel*, *Jira, Kestrel and Plover*. */
export function systemsWords(systems: readonly string[]): string {
  return systems.length < 2 ? (systems[0] ?? "") : `${systems.slice(0, -1).join(", ")} and ${systems.at(-1)!}`;
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

/**
 * The Jira views hidden from the list (#878, R-3.25), in list order: one line
 * at the bottom of the views pane, each with *Show*. They keep syncing.
 */
export function hiddenViews(views: TicketViewRow[]): TicketViewRow[] {
  return views
    .filter((view) => view.saved_view_source?.hidden === true)
    .sort((a, b) => a.position - b.position || a.id.localeCompare(b.id));
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

/** Where a copy of a saved view came from, for the editor's line about it (#880). */
export interface CopiedFrom {
  kind: SavedViewSource["kind"];
  /** The ticket system it came from (#2032). */
  system: string;
  name: string;
  /** The parts of the query it left out, and why. */
  left: LeftOut[];
}

/** A source's translation of its query as the search text a view stores (#1208): its conditions, in order. */
export function translationText(copy: QueryTranslation): string {
  const conditions = copy.conditions.map((condition) => ({
    field: CONDITION_FIELDS.find((entry) => entry.key === condition.key)!.field,
    negated: condition.negated,
    values: [...condition.values],
  }));
  return conditionsText({ conditions, words: "" });
}

/** *A copy keeps 1 of 2 parts of the query.*, in View settings on a saved view (#880). */
export function copyKeeps(copy: QueryTranslation): string {
  return copy.left_out.length === 0
    ? "A copy keeps every part of the query."
    : `A copy keeps ${copy.kept} of ${copy.kept + copy.left_out.length} parts of the query.`;
}

/**
 * What *Make an editable copy* starts from (#880, R-3.25): a view of your
 * own under *Mine*, the parts of the query knobas can test as its conditions
 * (`copy`, the source's own reading of its query, #1208), Jira's columns as
 * its own, and the rest of the Jira view's setup. With `showing`, *Save as a
 * view* on a Jira view: no name yet, `extra` (what the filter bar adds now)
 * after the conditions, and how the tickets show now.
 */
export function draftSavedViewCopy(
  view: TicketView,
  savedViewSource: SavedViewSource,
  copy: QueryTranslation,
  extra = "",
  showing?: Showing,
): { draft: TicketViewDraft; from: CopiedFrom } {
  const query = [translationText(copy), extra.trim()].filter((part) => part !== "").join(" ");
  const draft: TicketViewDraft = { ...draftOf(view), group: "Mine", query };
  if (showing !== undefined) {
    Object.assign(draft, {
      name: "",
      layout: showing.layout,
      lanes: showing.lanes,
      list_sort: sameSort(showing.sort, DEFAULT_SORT) ? null : showing.sort,
    });
  }
  return { draft, from: { kind: savedViewSource.kind, system: savedViewSource.system, name: view.name, left: copy.left_out } };
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
  return { ...draftOf(view), id: view.id, position: view.position, saved_view: view.saved_view };
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
    // A copy is your own view: its source decides nothing about it.
    saved_view: null,
  };
}

function sameSort(a: ListSort, b: ListSort): boolean {
  return a.column === b.column && a.direction === b.direction;
}
