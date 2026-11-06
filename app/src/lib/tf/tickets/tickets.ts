/**
 * The Tickets view's rules (#810, requirements §3), pure: what a column
 * draws, what a card's corner says, what the preview's button reads, and
 * what *Around this ticket* counts.
 */
import type { EntityDetail, MiniBoard, MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import type { ListSort as StoredSort, ViewLanes, ViewSortColumn } from "../../ipc/search";
import { keyOf } from "../../time/format";
import { entityAddress } from "../address";
import { stateWord } from "../state-word";

/** How many cards a column draws before *Show N more* (R-3.4). */
export const COLUMN_CAP = 8;
/** How many a *Done* column draws (R-3.4). */
export const DONE_CAP = 3;
/**
 * How many a column reads and draws while the filter has words (R-3.4,
 * #1024): the server still filters the whole mirror and counts every match,
 * but sends at most this many a column, *Done* too.
 */
export const TYPED_CAP = 50;
/**
 * How many lanes a laned board reads at first, and how many more each
 * *Show N more lanes* reads (#939): biggest first, the rest counted.
 */
export const LANES_AT_FIRST = 10;

/** Whether a status is *Done*, however its source cases it. */
export function isDone(status: string | null): boolean {
  return status !== null && status.toLowerCase() === "done";
}

/**
 * What a column's header says: its status in the source's words, or *No
 * status*. A view's column (a Jira view's, or one a view gathers) says its
 * name; a ticket's status is its card's (#1423).
 */
export function columnName(status: string | null): string {
  return status === null || status === "" ? "No status" : stateWord(status);
}

/**
 * A column's head. A view's column called after a status it holds reads as that status does; one named anything else keeps
 * the words it was given, by the reader or by a board in the source, because `stateWord` takes states and *Waiting on Björn*
 * is a name.
 */
export function columnHead(name: string | null, columns: readonly { name: string; statuses: readonly (string | null)[] }[]): string {
  const own = columns.find((column) => column.name === name);
  return name !== null && name !== "" && own !== undefined && !own.statuses.includes(name) ? name : columnName(name);
}

/**
 * A key as a card can hold it: a forge's `owner/repo/issues/143` or `owner/repo#143` is told apart by its repository
 * and number, which an end-of-line ellipsis would cut. Every other key is short already.
 */
export function shortKey(key: string): string {
  const issue = /^[^/\s]+\/([^/\s]+)\/issues\/(\d+)$/.exec(key) ?? /^[^/\s]+\/([^/\s#]+)#(\d+)$/.exec(key);
  return issue === null ? key : `${issue[1]}#${issue[2]}`;
}

/**
 * A source's word as a label: Gitea says *closed* and *high* where Jira says *Done* and *High*, and the shell's
 * labels are sentence case. Only the first letter moves, for a word that is not a state, such as a priority: a status goes
 * through {@link columnName}, and so `stateWord`.
 */
export function sentenceCase<Word extends string | null>(word: Word): Word | string {
  return word === null || word === "" ? word : word[0]!.toLocaleUpperCase() + word.slice(1);
}

/**
 * Whether a column is a *Done* one (R-3.4, #1423), never by its name: every
 * status the view's column lists is done, or, for a column that lists none
 * (the board's own, one a status), every card on it is.
 */
export function columnDone(column: MiniBoardColumn, listed: readonly string[] = []): boolean {
  if (listed.length > 0) return listed.every(isDone);
  return column.cards.length > 0 && column.cards.every((card) => isDone(card.status));
}

/**
 * The cards a column draws (R-3.4): up to eight, three for a done one
 * ({@link columnDone}), fifty of either while the filter has words, and all
 * of them once expanded.
 */
export function drawnCards(
  column: MiniBoardColumn,
  expanded: boolean,
  typing: boolean,
  done: boolean = columnDone(column),
): MiniBoardCard[] {
  if (expanded) return column.cards;
  return column.cards.slice(0, typing ? TYPED_CAP : done ? DONE_CAP : COLUMN_CAP);
}

/** What the column's *Show N more* button says, or `null` when it draws every card. */
export function moreLine(column: MiniBoardColumn, drawn: number, done: boolean = columnDone(column)): string | null {
  const rest = column.total - drawn;
  if (rest <= 0) return null;
  return done ? `Show ${rest} more done` : `Show ${rest} more`;
}

/**
 * A ticket's epic as the card and the preview draw it (#894): the path line
 * and the direct parent's name, `PAY-200 Payout reliability`. A path of one
 * segment is the parent's key (an epic's); a deeper chain is followed by its
 * last parent's name. The path is the ticket system's declared `parent`
 * (#1200, ADR-0029), whatever the source. The path alone when the mirror
 * holds no parent of that name, and `null` with no path.
 */
export function epicLine(card: Pick<MiniBoardCard, "path" | "parent_title">): string | null {
  if (card.path === null) return null;
  return card.parent_title ? `${card.path} ${card.parent_title}` : card.path;
}

/**
 * The comment count a card draws (#894): "N comments" above zero only, so a
 * count of none and a count that never answered both draw nothing.
 */
export function commentsLine(count: number | null): string | null {
  if (count === null || count <= 0) return null;
  return count === 1 ? "1 comment" : `${count} comments`;
}

/**
 * The initials on an avatar: a display name's first and last letters (*Lena Hoffmann* is *LH*), else the first two
 * letters of an account's first part, or *–* for nobody. One account has one avatar, whichever form a read answered:
 * `mara` and `mara.lindqvist` are both *MA*.
 */
export function initials(assignee: string | null): string {
  if (assignee === null) return "–";
  const names = assignee.trim().split(/\s+/);
  if (names.length >= 2) return (names[0]![0]! + names.at(-1)![0]!).toUpperCase();
  const parts = assignee.split(/[.\s_@-]+/).filter(Boolean);
  return (parts[0] ?? assignee).slice(0, 2).toUpperCase();
}

/**
 * What a card calls its assignee (F-107): the name the source keeps beside the
 * account, which is what a ticket's comments call the same person, else the
 * account. Filters and lanes stay on the account, the stored value.
 */
export function assigneeName(card: Pick<MiniBoardCard, "assignee" | "assignee_name">): string | null {
  return card.assignee_name ?? card.assignee;
}

/** What a list's narrow column calls someone: the first word of the name the source gave, on one line. The whole name is the cell's title. */
export function firstName(name: string | null): string | null {
  return name?.trim().split(/\s+/)[0] || null;
}

/**
 * What the Tickets preview and the ticket page draw of a ticket (spec #2227):
 * a board card's fields, or the same fields read off the ticket's detail
 * ({@link ticketFacts}), so both draw from one component.
 */
export type TicketFacts = Pick<
  MiniBoardCard,
  "entity_id" | "key" | "title" | "status" | "assignee" | "assignee_name" | "priority" | "path" | "parent_title" | "updated_at" | "mine"
>;

/**
 * A ticket's {@link TicketFacts} off its detail, for the ticket page: the
 * assignee is the detail's `author`, as the Jira adapter maps it, and `mine`
 * is `isMine`'s answer, the rule `mini_board`'s `mine` reads. The detail holds
 * no epic name: `parentTitle` is the title of the nearest parent's own row,
 * which the page reads from the mirror as the board's `parent_title` is, so
 * the epic line is the preview's (`PAY-200 Payout reliability`). `null` while
 * it is read, or when it cannot be, draws the path line alone.
 */
export function ticketFacts(
  detail: Pick<EntityDetail, "row" | "author" | "author_name" | "status" | "priority">,
  mine: boolean,
  parentTitle: string | null,
): TicketFacts {
  return {
    entity_id: detail.row.entity_id,
    key: keyOf(detail.row.entity_id),
    title: detail.row.title,
    status: detail.status,
    assignee: detail.author,
    ...(detail.author_name ? { assignee_name: detail.author_name } : {}),
    priority: detail.priority,
    path: detail.row.path,
    parent_title: parentTitle,
    updated_at: detail.row.updated_at,
    mine,
  };
}

/**
 * The one primary button of the Tickets preview and the ticket page, and the
 * line under it (R-3.9, spec #2227), by the ticket's state as prototype-6
 * decides it: *Start work* on a To Do ticket of mine, *Open job* on a done one
 * or somebody else's, *Resume work* on the rest of mine. The state is the
 * ticket's own status, whatever column it stands in (#1423). `chooses` says
 * where it goes: *Resume work* to the job's address (`jobHash`), which makes it
 * the chosen job; *Open ticket* only reads, so to the ticket's page
 * (`ticketHash`), which chooses nothing (spec #2396); and *Start work* to the
 * ticket's page too, because it chooses nothing until its steps run (F-130).
 * `asks` is what the ticket's page does on arriving (`job/chosen.svelte.ts`).
 */
export function previewAction(
  card: Pick<MiniBoardCard, "assignee" | "mine" | "status">,
  timerOnIt: boolean,
  chosen = false,
): { label: string; note: string; chooses: boolean; asks?: "steps" | "timer" } {
  const lower = card.status?.toLowerCase() ?? "";
  // Already the job: nothing to start from here, and its page has the timer.
  if (chosen) return { label: "Open ticket", note: timerOnIt ? "Your timer is running on this ticket." : "This is the ticket you are on.", chooses: false };
  if (lower === "to do" && card.mine) {
    return {
      label: "Start work",
      note: "Shows the steps first. Nothing happens until you run them.",
      chooses: false,
      asks: "steps",
    };
  }
  if (lower === "done") return { label: "Open ticket", note: "Done. Opening shows what was attached.", chooses: false };
  if (!card.mine) {
    return { label: "Open ticket", note: card.assignee ? `Assigned to ${assigneeName(card)}.` : "Unassigned.", chooses: false };
  }
  return {
    label: "Resume work",
    note: timerOnIt ? "Your timer is running on this ticket." : "Starts the timer on this ticket.",
    chooses: true,
    ...(timerOnIt ? {} : { asks: "timer" as const }),
  };
}

/** What a ticket board's lanes split by (R-3.5): nothing, or one field. */
export type LaneField = ViewLanes;

/** The *Lanes* switch's choices, in prototype-6's order and words. */
export const LANE_FIELDS: readonly (readonly [LaneField, string])[] = [
  ["none", "None"],
  ["epic", "Epic"],
  ["assignee", "Assignee"],
  ["priority", "Priority"],
  ["issuetype", "Issue type"],
  ["label", "Label"],
];

/** One band of the ticket board: its own columns and its own count. */
export interface Lane {
  /** What the lane's tickets share (`""` for none of it); `null` for the one band of no lanes. */
  key: string | null;
  /** The lane's heading; empty for the one band of no lanes. */
  title: string;
  count: number;
  columns: MiniBoardColumn[];
}

/**
 * The value a card is laned by: its epic line, assignee, priority, type or
 * first label; `""` for none. `MiniBoardCard::lane` is the authority.
 */
export function laneOf(
  card: Pick<MiniBoardCard, "path" | "assignee" | "priority" | "issue_type" | "labels">,
  field: LaneField,
): string {
  switch (field) {
    case "epic":
      return card.path ?? "";
    case "assignee":
      return card.assignee ?? "";
    case "priority":
      return card.priority ?? "";
    case "issuetype":
      return card.issue_type ?? "";
    case "label":
      return card.labels[0] ?? "";
    case "none":
      return "";
  }
}

/** A lane's heading: the value, or what the lane of tickets without one is called. */
export function laneTitle(field: LaneField, key: string): string {
  if (key !== "") return key;
  switch (field) {
    case "epic":
      return "No epic";
    case "assignee":
      return "Unassigned";
    case "priority":
      return "No priority";
    case "issuetype":
      return "No type";
    case "label":
      return "No label";
    case "none":
      return "";
  }
}

/**
 * A board read in lanes (#939) as the board draws it: one band per lane the
 * server sent, in its order (biggest first), each with every column of the
 * board, its cards looked up by id and its counts the lane's own, then
 * gathered into the view's columns by `gather`. A board with no lanes is
 * one untitled band holding the board as it is.
 */
export function boardLanes(
  board: MiniBoard,
  field: LaneField,
  gather: (columns: MiniBoardColumn[]) => MiniBoardColumn[],
): Lane[] {
  if (field === "none") {
    return [{ key: null, title: "", count: board.columns.reduce((all, c) => all + c.total, 0), columns: gather(board.columns) }];
  }
  const byId = new Map(board.columns.flatMap((column) => column.cards.map((card) => [card.entity_id, card] as const)));
  return board.lanes.map((lane) => ({
    key: lane.key,
    title: laneTitle(field, lane.key),
    count: lane.count,
    columns: gather(
      board.columns.map((column) => {
        const cell = lane.columns.find((candidate) => candidate.status === column.status);
        return {
          status: column.status,
          cards: (cell?.ids ?? []).flatMap((id) => byId.get(id) ?? []),
          total: cell?.total ?? 0,
        };
      }),
    ),
  }));
}

/** What the line under a laned board says when lanes were not read, or `null`. */
export function moreLanesLine(more: number): string | null {
  if (more <= 0) return null;
  return `Show ${Math.min(more, LANES_AT_FIRST)} more ${more === 1 ? "lane" : "lanes"}`;
}

/**
 * The board split into lanes (R-3.5) over the cards it holds: one per value
 * of `field` the cards show, each with every column of the board narrowed
 * to its tickets and counted, sorted by size, biggest first (ties in the
 * order first met). What the view editor's preview counts its lanes by; the
 * Tickets board's lanes are the server's ({@link boardLanes}).
 */
export function splitLanes(columns: MiniBoardColumn[], field: LaneField): Lane[] {
  if (field === "none") {
    return [{ key: null, title: "", count: columns.reduce((all, column) => all + column.total, 0), columns }];
  }
  const keys: string[] = [];
  for (const column of columns) {
    for (const card of column.cards) {
      const key = laneOf(card, field);
      if (!keys.includes(key)) keys.push(key);
    }
  }
  return keys
    .map((key) => {
      const laned = columns.map((column) => {
        const cards = column.cards.filter((card) => laneOf(card, field) === key);
        return { status: column.status, cards, total: cards.length };
      });
      return {
        key,
        title: laneTitle(field, key),
        count: laned.reduce((all, column) => all + column.total, 0),
        columns: laned,
      };
    })
    .sort((a, b) => b.count - a.count);
}

/** What a list header sorts by (R-3.6). Signals do not sort. */
export type SortColumn = ViewSortColumn;

/** The list's sort: the one a view stores (`ticket_views::ListSort`). */
export type ListSort = StoredSort;

/** How the list opens: newest first. */
export const DEFAULT_SORT: ListSort = { column: "updated", direction: "descending" };

/** A header pressed: the same column turns its direction, another starts ascending. */
export function nextSort(current: ListSort, column: SortColumn): ListSort {
  if (current.column === column) {
    return { column, direction: current.direction === "ascending" ? "descending" : "ascending" };
  }
  return { column, direction: "ascending" };
}

/**
 * Priorities in the order ticket systems' default schemes rank them, highest
 * first, matched ignoring case. Order only, never wording: a priority not here sorts after
 * these, ties in key order.
 */
const PRIORITY_ORDER = ["highest", "blocker", "critical", "high", "major", "medium", "normal", "minor", "low", "trivial", "lowest"];

const byText = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** One row of the list: a card, whose status is its own (#1423). */
export interface TicketRow {
  card: MiniBoardCard;
}

/** One row's value for a column, comparable; `null` for none, which sorts last either way. */
function sortValue(
  row: TicketRow,
  column: SortColumn,
  statuses: (string | null)[],
): string | number | null {
  const { card } = row;
  switch (column) {
    case "key":
      return card.key;
    case "summary":
      return card.title;
    case "status":
      return statuses.indexOf(card.status);
    case "priority": {
      if (card.priority === null) return null;
      const at = PRIORITY_ORDER.indexOf(card.priority.toLowerCase());
      return at === -1 ? PRIORITY_ORDER.length : at;
    }
    case "assignee":
      return card.assignee;
    case "updated":
      return card.updated_at;
  }
}

/**
 * The list's rows sorted by a header (R-3.6). Status sorts in the order the
 * board first draws each (`statuses`, {@link statusOrder}), priority by {@link PRIORITY_ORDER}, keys
 * numerically (`PAY-9` before `PAY-10`), the rest by text. A row with no
 * value sorts last in either direction; ties keep key order.
 */
export function sortRows<Row extends TicketRow>(
  rows: Row[],
  sort: ListSort,
  statuses: (string | null)[],
): Row[] {
  const sign = sort.direction === "ascending" ? 1 : -1;
  const compare = (x: string | number, y: string | number) =>
    typeof x === "number" && typeof y === "number" ? x - y : byText.compare(String(x), String(y));
  return [...rows].sort((a, b) => {
    const x = sortValue(a, sort.column, statuses);
    const y = sortValue(b, sort.column, statuses);
    if (x === null || y === null) {
      if (x !== y) return x === null ? 1 : -1;
    } else {
      const order = compare(x, y);
      if (order !== 0) return order * sign;
    }
    return byText.compare(a.card.key, b.card.key);
  });
}

/**
 * The statuses in the order the board draws them (#1423): each card's own,
 * first met column by column, so a view whose column gathers several keeps
 * them together. What the list's *Status* sorts by.
 */
export function statusOrder(columns: readonly MiniBoardColumn[]): (string | null)[] {
  return [...new Set(columns.flatMap((column) => column.cards.map((card) => card.status)))];
}

/**
 * The keys more than one card on the board holds (#1476): two sources'
 * PAY-231. Their cards and rows name their source. Counted over the cards
 * read.
 */
export function sharedKeys(columns: readonly MiniBoardColumn[]): Set<string> {
  const seen = new Set<string>();
  const shared = new Set<string>();
  for (const card of columns.flatMap((column) => column.cards)) {
    if (seen.has(card.key)) shared.add(card.key);
    seen.add(card.key);
  }
  return shared;
}

/**
 * Where a card opens (Enter, a double click): its ticket page by its entity
 * id (`#/ticket/jira:PAY-231`, spec #2227), the address that names the source
 * ({@link entityAddress}'s rule, #1476, #1977). Never the job: only the
 * primary button takes the ticket on, at the job's address (`jobHash`).
 */
export function ticketAddress(card: Pick<MiniBoardCard, "entity_id">): string {
  return entityAddress("ticket", card.entity_id);
}
