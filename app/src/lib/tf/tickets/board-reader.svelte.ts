/**
 * The Tickets page's reads (#1319, spec #1304): the board, its total, its
 * failure, the expanded columns, *Show N more* on the list, the lanes shown,
 * a view that shows every ticket (ux18), the typing pause and the generations
 * that drop an older answer. The selected ticket's preview and its reads went
 * with the preview (ux18).
 *
 * `TicketsPage.svelte` binds to it and keeps the drawing, the keyboard step,
 * the view CRUD and the editor. The reader touches no DOM, so its races are
 * tested on a fake clock with fake ports (`board-reader.test.svelte.ts`).
 *
 * The board is `mini_board`'s, read for the ticket system (#1200, #1299,
 * ADR-0029 as amended): the server reads it over the sources that fill the
 * role, whatever their adapter kind, so every count is the fillers' own and
 * nothing is narrowed here.
 */
import type { BoardPage, BoardPageMark, LanesAsked, MiniBoard, MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import type { ViewColumn } from "../../ipc/search";
import { TICKET_SYSTEM } from "../roles.svelte";
import {
  boardLanes,
  COLUMN_CAP,
  columnDone,
  DEFAULT_SORT,
  PRIORITY_ORDER,
  drawnCards,
  shownCount,
  withoutTicket,
  LANES_AT_FIRST,
  TYPED_CAP,
  type Lane,
  type LaneField,
  type ListSort,
} from "./tickets";
import { newerFirst, readPrefix, viewColumnParts, viewColumns } from "./views";

/** The reads the reader makes, injectable so a test needs no bridge. */
export interface BoardReaderPorts {
  /**
   * `mini_board` over the sources filling `role`, every source without one
   * (#1299): capped per column or not, narrowed by a query or not; or a
   * Jira view's board, when `savedView` names one (#877); in lanes or not
   * (#939).
   */
  board: (
    perColumn: number | null,
    query: string | null,
    savedView?: string | null,
    lanes?: LanesAsked | null,
    role?: string,
    page?: BoardPage | null,
  ) => Promise<MiniBoard>;
}

/** One view's session: the switches over what it was saved with, and the expanded columns. */
export interface ViewSession {
  /** The *Board / List* switch, or `undefined` for the view's own. */
  layout?: "board" | "list";
  /** The *Lanes* switch (#813), or `undefined` for the view's own. */
  lanes?: LaneField;
  /** The list's sort (#813), or `undefined` for the view's own. */
  sort?: ListSort;
  /** Expanded board columns by name (`""` for *No status*); in a lane, `<lane>|<name>`. */
  expanded: string[];
  /** Whether the list's *Show N more* was pressed: the list reads every ticket, the board keeps its caps. */
  listAll: boolean;
}

/** Every view's session, by view id (`""` for *All tickets*): the page's store, held for the session. */
export interface ViewSessions {
  views: Record<string, ViewSession>;
}

/** `key`'s session in `sessions`, made when first switched. */
export function viewSession(sessions: ViewSessions, key: string): ViewSession {
  // Read back through the store: `??=` answers the plain object, not its proxy.
  sessions.views[key] ??= { expanded: [], listAll: false };
  return sessions.views[key]!;
}

/** What the reader reads: the view's base query, its saved view, and the session it keeps. */
export interface BoardScope {
  /** The view's id, `""` for *All tickets*. */
  key: string;
  /** The base query, `null` when there is nothing to read (the views unread, or a text the grammar cannot read). */
  base: string | null;
  /** The saved view the board reads (#877), or `null`. */
  savedView: string | null;
  /**
   * The ticket whose sub-tasks the board is narrowed to, by key, or `null`: its base is `under:<key>`, which answers
   * the ticket itself too, so its cards are dropped ({@link withoutTicket}) and every card is read to drop them all.
   */
  without?: string | null;
}

/** The filter bar as a read sends it. */
export interface BoardFilter {
  /** The base, chips, dropdowns and words as one query (`toQuery`). */
  query: string;
  /** Whether words are typed: fifty a column then, not eight (R-3.4, #1024). */
  words: boolean;
}

/** What the view on screen says about drawing its board: read when drawn, not when opened. */
export interface BoardView {
  /** The view's own columns, which gather the board's statuses (R-3.15). */
  columns: ViewColumn[];
  /**
   * The view's columns as it stores them, each listing the statuses it
   * gathers: what makes a column done (#1423). A Jira view's `columns` hold
   * only their names, so it passes these too; `columns` otherwise.
   */
  listed?: ViewColumn[];
  /** The layout and lanes it was saved with, under the session's switches. */
  layout: "board" | "list";
  lanes: LaneField;
  /**
   * *Show every ticket when the view opens* (ux18): no column, lane or list offers more. Each column reads fifty and
   * then its next {@link PAGE_CARDS} as its window nears its last card, a laned board eight a lane's column and ten
   * lanes more as the board's foot nears, and the list pages sorted on the server (Fable's ruling, ux18).
   */
  showAll?: boolean;
  /** The list's sort as the page shows it, which a view that shows every ticket sorts on the server. */
  sort?: ListSort;
}

/** A band of the board (one, untitled, without lanes): each column with the cards it draws. */
export interface Band {
  lane: Lane;
  /**
   * `done`: whether the column is a done one ({@link columnDone}), so it draws three; `listed`: the statuses the
   * view's column gathers, none for the board's own.
   */
  drawn: { column: MiniBoardColumn; cards: MiniBoardCard[]; done: boolean; listed: readonly string[] }[];
}

export interface BoardReader {
  /** The board's answer, or `null` before one. */
  readonly board: MiniBoard | null;
  /** How many tickets there are unfiltered: the count's "of 75". */
  readonly total: number | null;
  /** Why the board could not be read, or `null`. */
  readonly failed: string | null;
  /** How many tickets the board holds, or `null` before an answer. */
  readonly shown: number | null;
  readonly expanded: readonly string[];
  readonly listAll: boolean;
  /** How many lanes a laned board reads: the first ten, ten more a *Show N more lanes*. */
  readonly lanesShown: number;
  readonly layout: "board" | "list";
  readonly lanes: LaneField;
  /** Whether the board is split into lanes: read in lanes on the server (#939). */
  readonly laned: boolean;
  /** Whether the filter has words. */
  readonly typing: boolean;
  /** Whether the view shows every ticket ({@link BoardView.showAll}): no column, lane or list offers more. */
  readonly all: boolean;
  /**
   * The list of a view that shows every ticket, in the server's order of its sort, the pages read so far; `null`
   * on every other list and board, which draw the board's cards.
   */
  readonly rows: MiniBoardCard[] | null;
  /** Whether that list's first page is still out: it draws nothing of its counts' read meanwhile. */
  readonly rowsWaiting: boolean;
  readonly bands: Band[];
  /** Moves each time a read the person caused lands: what the drawn cards sort again on. A sync's refresh leaves it. */
  readonly asked: number;
  /** Opens a scope: the board starts fresh, and an answer for the last one is dropped. */
  open(scope: BoardScope, filter: BoardFilter): void;
  /** A change in the filter bar: read once the keys pause. */
  setFilter(filter: BoardFilter): void;
  /** A column's *Show more*: {@link MORE_STEP} more of its cards, read and drawn. */
  expand(lane: Lane, column: MiniBoardColumn): void;
  /** The list's *Show N more*: every ticket read. */
  expandAll(): void;
  /** *Show N more lanes* (#939): ten more of them; on a view that shows every ticket, as its foot nears. */
  moreLanes(): void;
  /**
   * A view that shows every ticket: `column`'s next {@link PAGE_CARDS} cards (in `lane` on a laned board), as its
   * window nears its last card. `listed`, the statuses its column gathers. Nothing while one is out for it.
   */
  more(lane: Lane, column: MiniBoardColumn, listed: readonly string[]): void;
  /** A view that shows every ticket: the list's next page, as its window nears its last row. */
  moreRows(): void;
  /** The list's sort changed: a view that shows every ticket reads its first page again, sorted. */
  resort(): void;
  chooseLayout(layout: "board" | "list"): void;
  chooseLanes(lanes: LaneField): void;
  /** Reads now, with no pause: a sync's re-read, which keeps the drawn cards where they are (`asked` stays). */
  refresh(): void;
  /** Stops the pause and drops every answer still out. */
  close(): void;
}

/** A board with nothing on it, new each time, as `$state` may take it: what no answer yet draws. */
function emptyBoard(): MiniBoard {
  return { columns: [], sources: [], lanes: [], more_lanes: 0 };
}

const sum = (board: MiniBoard) => board.columns.reduce((all, column) => all + column.total, 0);

/** How many lanes a page in one lane asks for, so the server cuts none: more than any board holds (it takes a `u32`). */
const EVERY_LANE = 1_000_000;

/**
 * How many cards a column's page reads, and rows a page of the list (Fable's ruling, ux18): at two screens from
 * its last card a column reads its next 200, about 120 KB and 30 to 48 ms at 20,000 tickets, where a read of every
 * ticket was 9.5 MB and 300 ms before its parse.
 */
export const PAGE_CARDS = 200;

/** `answer`'s page of cards added below `held`'s, each in its column and lane: none drawn moves (ux18). */
function withPage(held: MiniBoard, answer: MiniBoard, lane: string | null): MiniBoard {
  const known = new Set(held.columns.flatMap((column) => column.cards.map((card) => card.entity_id)));
  const columns = held.columns.map((column) => {
    const page = answer.columns.find((other) => other.status === column.status);
    const fresh = page?.cards.filter((card) => !known.has(card.entity_id)) ?? [];
    return fresh.length === 0 ? column : { ...column, cards: [...column.cards, ...fresh] };
  });
  for (const page of answer.columns) {
    if (!columns.some((column) => column.status === page.status)) columns.push({ ...page, cards: page.cards.filter((card) => !known.has(card.entity_id)) });
  }
  if (lane === null) return { ...held, columns };
  const cells = answer.lanes.find((one) => one.key === lane)?.columns ?? [];
  const lanes = held.lanes.map((one) => {
    if (one.key !== lane) return one;
    const had = new Set(one.columns.flatMap((cell) => cell.ids));
    const merged = one.columns.map((cell) => {
      const page = cells.find((other) => other.status === cell.status);
      const ids = page?.ids.filter((id) => !had.has(id)) ?? [];
      return ids.length === 0 ? cell : { ...cell, ids: [...cell.ids, ...ids] };
    });
    for (const page of cells) if (!merged.some((cell) => cell.status === page.status)) merged.push(page);
    return { ...one, columns: merged };
  });
  return { ...held, columns, lanes };
}

/** A sorted page's cards in the server's order (`MiniBoard.order`). */
function inOrder(answer: MiniBoard): MiniBoardCard[] {
  const byId = new Map(answer.columns.flatMap((column) => column.cards.map((card) => [card.entity_id, card] as const)));
  return (answer.order ?? []).flatMap((id) => byId.get(id) ?? []);
}

/** A column's key among the expanded ones: its name, `""` for *No status*. */
export const columnKey = (column: Pick<MiniBoardColumn, "status">) => column.status ?? "";
const expandKey = (lane: Lane, column: MiniBoardColumn) =>
  lane.key === null ? columnKey(column) : `${lane.key}|${columnKey(column)}`;

/** What a failed read says. */
export function messageOf(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) return String(error.message);
  return String(error);
}

export interface BoardReaderOptions {
  /** How long, in ms, the filter waits for the next key before it reads: the launcher's 90. */
  typingPause: number;
  /** The page's store of each view's session. */
  sessions: ViewSessions;
  /** What the view on screen says about drawing it. */
  view: () => BoardView;
}

/** How long, in ms, typed words wait for a board read still out before they are read anyway. */
export const TYPED_WAIT_MOST = 400;

/** How long, in ms, the keys pause before a list showing every ticket reads all of them for its words. */
export const LIST_SETTLE_MS = 600;

export function createBoardReader(ports: BoardReaderPorts, options: BoardReaderOptions): BoardReader {
  const { typingPause, sessions } = options;
  /**
   * The last answer, replaced whole and never changed in place: `$state.raw`,
   * since the list's *Show N more* reads every ticket and a deep proxy over
   * thousands of cards made each draw subscribe to every field it read (#2200's cause).
   */
  let board = $state.raw<MiniBoard | null>(null);
  const state = $state({
    total: null as number | null,
    failed: null as string | null,
    lanesShown: LANES_AT_FIRST,
    scope: { key: "", base: null, savedView: null, without: null } as BoardScope,
    filter: { query: "", words: false } as BoardFilter,
    asked: 0,
  });
  /** Bumped by every read and every key, so only the last read's answer lands. */
  let reading = 0;
  /** Bumped when another scope opens, so a count read for the last one is dropped. */
  let epoch = 0;
  /** The read a filter change waits to make until the keys pause. */
  let pausing: ReturnType<typeof setTimeout> | undefined;
  /** Board reads sent and not yet answered. */
  let out = 0;
  /** The read words typed while a read was out wait to make: made once it answers, with the words as they are then. */
  let queued: (() => void) | null = null;
  /** The list's whole read after its quick one, once the keys have paused ({@link LIST_SETTLE_MS}). */
  let settling: ReturnType<typeof setTimeout> | undefined;
  /** When the last board read was sent, and the last filter change made (`Date.now()`). */
  let sentAt = 0;
  let keyAt = 0;
  /** A view that shows every ticket: the list's pages, its generation, and whether a page is out. */
  let rows = $state.raw<MiniBoardCard[] | null>(null);
  let rowsReading = 0;
  let rowsOut = false;
  /** The columns (by lane and name) with a page out. */
  const paging = new Set<string>();
  /**
   * A view that shows every ticket: each column's (by lane and name) last card of its last full page, which holds every
   * card before it ({@link readPrefix}). Set before the page's board lands, so the bands drawn from it read it.
   */
  const readThrough = new Map<string, string>();

  const view = $derived(options.view());
  const own = $derived(sessions.views[state.scope.key]);
  const expanded = $derived(own?.expanded ?? []);
  const listAll = $derived(own?.listAll ?? false);
  const layout = $derived(own?.layout ?? view.layout);
  const lanes = $derived(own?.lanes ?? view.lanes);
  const laned = $derived(layout === "board" && lanes !== "none");
  const typing = $derived(state.filter.words);
  const all = $derived(view.showAll === true);
  const bands = $derived(
    boardLanes(board ?? emptyBoard(), laned ? lanes : "none", (columns) => columns).map((byStatus) => {
      const lane: Lane = { ...byStatus, columns: viewColumns(byStatus.columns, view.columns) };
      return {
        lane,
        drawn: lane.columns.map((column, at) => {
          const listed = (view.listed ?? view.columns).find((own) => own.name === column.status)?.statuses ?? [];
          const done = columnDone(column, listed);
          // A view that shows every ticket draws every card read up to the first it does not hold: the first read's fifty
          // a column, then each page below them (the ux18 Windows pass). A Jira view's columns come in its source's order,
          // each paged in it, so they draw every card read.
          const key = expandKey(lane, column);
          const cards = !all
            ? drawnCards(column, pagesOf(key), typing, done)
            : state.scope.savedView !== null
              ? column.cards
              : (readPrefix(column.cards, viewColumnParts(byStatus.columns, view.columns, at), readThrough.get(key) ?? null) as MiniBoardCard[]);
          return { column, cards, done, listed };
        }),
      };
    }),
  );

  const here = () => viewSession(sessions, state.scope.key);

  /**
   * The per-column cap a read asks for: the eight a column draws, fifty while
   * words filter (R-3.4, #1024), and the largest expanded column's count once
   * one is expanded (every column's once the list is). In lanes the same,
   * each lane's column counted on its own (#939).
   */
  function capFor(quick = false): number | null {
    if (state.scope.without) return null;
    const cap = typing ? TYPED_CAP : COLUMN_CAP;
    // A quick read (a key on the whole list or an expanded board) sends fifty a column whatever the words: at eight, a
    // filter cleared would cut the columns to eight until the whole read (Fable's ruling, the ux13-overs pass).
    if (quick) return TYPED_CAP;
    const answered = board;
    if ((expanded.length === 0 && !listAll) || answered === null) return cap;
    if (listAll && !laned) return Math.max(cap, ...answered.columns.map((column) => column.total));
    // An expanded column asks for what it draws, not for every card it holds (MORE_STEP).
    const wanted = bands.flatMap(({ lane, drawn }) =>
      drawn.map(({ column, done }) => shownCount(column.total, pagesOf(expandKey(lane, column)), typing, done)),
    );
    return Math.max(cap, ...wanted);
  }

  /** How many times the reader pressed a column's *Show more*: its key, once a press. */
  function pagesOf(key: string): number {
    let pages = 0;
    for (const held of expanded) if (held === key) pages++;
    return pages;
  }

  /** What a laned read asks for (#939), or `null` without lanes. */
  function lanesAsked(): LanesAsked | null {
    if (!laned || lanes === "none") return null;
    return { by: lanes, first: state.lanesShown };
  }

  /** Whether the list of a view that shows every ticket is read in pages sorted on the server. Sub-tasks read whole. */
  const paged = () => all && layout === "list" && !state.scope.without;

  /**
   * A view that shows every ticket (ux18): fifty a column (eight a lane's column), its pages read as it scrolls; a
   * sync's re-read reads each column to the depth it holds, so no card drawn goes. Its list reads one card a column
   * here, for the counts: the rows are its pages.
   */
  function capAll(byPerson: boolean): number | null {
    if (state.scope.without) return null;
    if (paged()) return 1;
    const first = laned ? COLUMN_CAP : TYPED_CAP;
    if (byPerson || board === null) return first;
    const held = laned ? board.lanes.flatMap((one) => one.columns.map((cell) => cell.ids.length)) : board.columns.map((column) => column.cards.length);
    return Math.max(first, ...held);
  }

  /** The statuses in the order the list's *Status* sorts them: the view's columns', else the board's. */
  function statusOrder(): (string | null)[] {
    const own = (view.listed ?? view.columns).flatMap((column) => column.statuses);
    if (own.length > 0) return own;
    return [...new Set((board?.columns ?? []).flatMap((column) => [column.status, ...column.cards.map((card) => card.status)]))];
  }

  /**
   * Each of `statuses`' last card the board holds, in `lane` (its key, `null` without lanes), in the board's order: a
   * page's marks ({@link BoardPage.frontier}), past which it reads.
   */
  function marksOf(lane: string | null, statuses: readonly (string | null)[]): BoardPageMark[] {
    if (board === null) return [];
    const cells =
      lane === null
        ? board.columns.map((column) => ({ status: column.status, last: column.cards.at(-1)?.entity_id }))
        : (board.lanes.find((one) => one.key === lane)?.columns ?? []).map((cell) => ({ status: cell.status, last: cell.ids.at(-1) }));
    return cells.flatMap(({ status, last }) => (last !== undefined && statuses.includes(status) ? [{ status, id: last }] : []));
  }

  /** The list's first page, sorted on the server (or as deep as it reads now, for a sync's re-read). */
  function readRows(byPerson: boolean) {
    const { base, savedView } = state.scope;
    if (base === null || !paged()) return;
    const sort = view.sort ?? DEFAULT_SORT;
    const page: BoardPage = {
      after: 0,
      sort: sort.column,
      direction: sort.direction,
      status_order: statusOrder(),
      priority_order: [...PRIORITY_ORDER],
    };
    const depth = byPerson ? PAGE_CARDS : Math.max(PAGE_CARDS, rows?.length ?? 0);
    const query = state.filter.query;
    const mine = ++rowsReading;
    rowsOut = false;
    ports
      .board(depth, query === "" ? null : query, savedView, null, TICKET_SYSTEM, page)
      .then((answer) => {
        if (mine !== rowsReading) return;
        rows = inOrder(answer);
        if (byPerson) state.asked += 1;
      })
      .catch((error: unknown) => {
        if (mine === rowsReading) state.failed = messageOf(error);
      });
  }

  /** A person's read of a view that shows every ticket: its board (or the list's counts), and the list's first page. */
  function readAll() {
    if (paged() && (view.sort ?? DEFAULT_SORT).column === "status" && (view.listed ?? view.columns).length === 0 && board === null) {
      // The statuses' order comes with the board: the list's page waits for it.
      read(false, true, () => readRows(true));
      return;
    }
    read();
    readRows(true);
  }

  /**
   * A read with words still filters and counts the whole mirror, so on a
   * large one a short prefix is the broadest of them; it sends fifty cards a
   * column at most (R-3.4, #1024).
   */
  function read(quick = false, byPerson = true, then?: () => void) {
    const asked = all ? capAll(byPerson) : capFor(quick);
    const lanesNow = lanesAsked();
    clearTimeout(pausing);
    const { base, savedView, without } = state.scope;
    if (base === null) return;
    const kept = (answer: MiniBoard) => (without ? withoutTicket(answer, without) : answer);
    const query = state.filter.query;
    const mine = ++reading;
    out += 1;
    sentAt = Date.now();
    ports
      .board(asked, query === "" ? null : query, savedView, lanesNow, TICKET_SYSTEM)
      .finally(() => {
        out -= 1;
        if (out === 0 && queued) {
          const run = queued;
          queued = null;
          run();
        }
      })
      .then((answer) => {
        if (mine !== reading) return;
        const drawn = kept(answer);
        board = drawn;
        // A quick answer that holds every card of every column is the whole read already: none follows it.
        if (quick && drawn.columns.every((column) => column.cards.length >= column.total)) clearTimeout(settling);
        state.failed = null;
        // A read the person caused (a filter, *Show more*, lanes) sorts the cards again; a sync's does not (`held-order`).
        if (byPerson) state.asked += 1;
        if (query === base) state.total = sum(drawn);
        then?.();
      })
      .catch((error: unknown) => {
        if (mine === reading) state.failed = messageOf(error);
      });
    // A session that comes back to a filter has not counted the whole yet.
    if (query !== base && state.total === null) {
      const at = epoch;
      ports
        .board(without ? null : 1, base === "" ? null : base, savedView, undefined, TICKET_SYSTEM)
        .then((answer) => {
          if (at === epoch) state.total ??= sum(kept(answer));
        })
        .catch(() => {});
    }
  }

  return {
    get board() {
      return board;
    },
    get total() {
      return state.total;
    },
    get failed() {
      return state.failed;
    },
    get shown() {
      return board === null ? null : sum(board);
    },
    get expanded() {
      return expanded;
    },
    get listAll() {
      return listAll;
    },
    get lanesShown() {
      return state.lanesShown;
    },
    get layout() {
      return layout;
    },
    get lanes() {
      return lanes;
    },
    get laned() {
      return laned;
    },
    get typing() {
      return typing;
    },
    get all() {
      return all;
    },
    get rows() {
      return paged() ? rows : null;
    },
    get rowsWaiting() {
      return paged() && rows === null;
    },
    get bands() {
      return bands;
    },
    get asked() {
      return state.asked;
    },
    open(scope, filter) {
      board = null;
      state.total = null;
      state.failed = null;
      reading += 1;
      epoch += 1;
      queued = null;
      clearTimeout(settling);
      state.scope = { ...scope };
      state.filter = { ...filter };
      rows = null;
      rowsReading += 1;
      paging.clear();
      readThrough.clear();
      if (scope.base === null) return;
      if (all) readAll();
      else read();
    },
    /**
     * A key reads at once when no read is out and the last was sent at least
     * `typingPause` ago: the first key typed into a settled board (the
     * broad-filters pass, 2026-11-13). Any other read waits until the keys
     * pause for `typingPause`, so the rest of a word typed at speed is one
     * read and not one a letter, and an answer still out for the text before
     * it is never drawn (#897). While a read is out the next
     * waits for its answer and then reads the words as they are by then: at
     * 20,000 tickets a typed read takes 160 to 240 ms, so a key every 150 ms
     * had two or three running against each other, each slower for it
     * (2026-11-10).
     */
    setFilter(filter) {
      state.filter = { ...filter };
      clearTimeout(pausing);
      clearTimeout(settling);
      reading += 1;
      // The list showing every ticket reads the capped board first, then all of it once the keys pause: every match
      // read on every key was 5.8 MB and 1.2 s a key at 20,000 tickets (2026-11-10).
      // So does a board whose *Show more* drew past fifty a column (the ux13-overs pass, 2026-11-15, Fable's ruling): a
      // key on 1,243 cards read 610 KB in 54 to 62 ms, 109 to 117 ms to its rows at CPU 4x on Björn's PC; the screen
      // shows the first fifty of a column, and the rest come once the keys pause. Lanes keep their one read.
      // A view that shows every ticket reads fifty a column, its list's first page, and the rest as it scrolls.
      const quick = !all && !laned && ((listAll && layout === "list") || (layout === "board" && expanded.length > 0 && (capFor() ?? 0) > TYPED_CAP));
      const go = () => {
        if (all) {
          paging.clear();
          readThrough.clear();
          return readAll();
        }
        read(quick);
        // The whole read finishes the key's own: it sorts nothing again (`held-order`), so the cards it adds join below
        // those drawn and none drawn moves.
        if (quick) settling = setTimeout(() => read(false, false), LIST_SETTLE_MS);
      };
      // The first key after the board settled reads at once: the pause made every first key wait 90 ms for keys that
      // were not coming (the broad-filters pass, 2026-11-13). A key typed within the pause of the last key or read
      // waits, so a word typed at speed is still one read after its first letter.
      const at = Date.now();
      const settled = out === 0 && at - Math.max(sentAt, keyAt) >= typingPause;
      keyAt = at;
      if (settled) {
        queued = null;
        return go();
      }
      pausing = setTimeout(() => {
        const waited = Date.now() - sentAt;
        if (out === 0 || waited >= TYPED_WAIT_MOST) return go();
        // A read that hangs holds nothing up for long: past the most, the words go anyway.
        queued = go;
        pausing = setTimeout(() => {
          queued = null;
          go();
        }, TYPED_WAIT_MOST - waited);
      }, typingPause);
    },
    expand(lane, column) {
      here().expanded = [...expanded, expandKey(lane, column)];
      // A lane's column is capped as the board's own is (#939).
      read();
    },
    expandAll() {
      here().listAll = true;
      read();
    },
    moreLanes() {
      state.lanesShown += LANES_AT_FIRST;
      // A view that shows every ticket keeps each lane's column as deep as it reads now.
      read(false, !all);
    },
    more(lane, column, listed) {
      const { base, savedView } = state.scope;
      const key = expandKey(lane, column);
      if (!all || base === null || board === null || paging.has(key) || column.cards.length >= column.total) return;
      paging.add(key);
      // A Jira view's column is its source's: the server cuts it by its name, in its order, past the cards it holds. A
      // view's own column reads past each of its statuses' last card held, ranked as one: `after` the cards it held, over
      // its statuses ranked as one, passed over cards a status's first fifty did not reach (the ux18 Windows pass).
      const statuses = savedView !== null || listed.length === 0 ? [column.status] : [...listed];
      const page: BoardPage =
        savedView !== null
          ? { after: column.cards.length, statuses, ...(lane.key !== null ? { lane: lane.key } : {}) }
          : { after: 0, statuses, frontier: marksOf(laned ? lane.key : null, statuses), ...(lane.key !== null ? { lane: lane.key } : {}) };
      const at = reading;
      const query = state.filter.query;
      const inLanes: LanesAsked | null = laned && lanes !== "none" && lane.key !== null ? { by: lanes, first: EVERY_LANE } : null;
      ports
        .board(PAGE_CARDS, query === "" ? null : query, savedView, inLanes, TICKET_SYSTEM, page)
        .then((answer) => {
          if (at !== reading || board === null) return;
          if (savedView === null) {
            // A full page holds every card up to its last: the column draws that far, whatever a status's marks say.
            const read = answer.columns.flatMap((one) => one.cards);
            if (read.length >= PAGE_CARDS) readThrough.set(key, read.reduce((last, card) => (newerFirst(last, card) < 0 ? card : last)).entity_id);
          }
          board = withPage(board, answer, laned ? lane.key : null);
        })
        .catch(() => {})
        .finally(() => {
          if (at === reading) paging.delete(key);
        });
    },
    moreRows() {
      const { base, savedView } = state.scope;
      const shown = board === null ? null : sum(board);
      if (!paged() || base === null || rows === null || rowsOut || shown === null || rows.length >= shown) return;
      rowsOut = true;
      const sort = view.sort ?? DEFAULT_SORT;
      const page: BoardPage = { after: rows.length, sort: sort.column, direction: sort.direction, status_order: statusOrder(), priority_order: [...PRIORITY_ORDER] };
      const mine = rowsReading;
      const query = state.filter.query;
      ports
        .board(PAGE_CARDS, query === "" ? null : query, savedView, null, TICKET_SYSTEM, page)
        .then((answer) => {
          if (mine !== rowsReading || rows === null) return;
          const had = new Set(rows.map((card) => card.entity_id));
          rows = [...rows, ...inOrder(answer).filter((card) => !had.has(card.entity_id))];
        })
        .catch(() => {})
        .finally(() => {
          if (mine === rowsReading) rowsOut = false;
        });
    },
    resort() {
      if (paged()) readRows(true);
    },
    chooseLayout(next) {
      if (layout === next) return;
      const wasLaned = laned;
      here().layout = next;
      // A view that shows every ticket reads its list in pages and its board by the column: the other layout reads.
      if (all) return readAll();
      if (wasLaned !== laned) read();
    },
    chooseLanes(next) {
      if (lanes === next) return;
      const wasLaned = laned;
      here().lanes = next;
      state.lanesShown = LANES_AT_FIRST;
      // Each field is its own read of lanes (#939).
      if (!wasLaned && !laned) return;
      paging.clear();
      readThrough.clear();
      read();
    },
    refresh: () => {
      read(false, false);
      readRows(false);
    },
    close() {
      clearTimeout(pausing);
      clearTimeout(settling);
      queued = null;
      reading += 1;
      epoch += 1;
    },
  };
}
