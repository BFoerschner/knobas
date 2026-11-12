/**
 * The Tickets page's reads (#1319, spec #1304): the board, its total, its
 * failure, the expanded columns, *Show N more* on the list, the lanes shown,
 * the typing pause and the generations that drop an older answer, and the
 * selected ticket's preview with its own generation.
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
import type {
  EntityDetail,
  LanesAsked,
  MiniBoard,
  MiniBoardCard,
  MiniBoardColumn,
} from "../../ipc/entity";
import type { ViewColumn } from "../../ipc/search";
import type { Prose } from "../../render/prose";
import { getAsset, type AssetDetail } from "../../ipc/assets";
import { entityCheckout, type CheckoutView } from "../../ipc/entity";
import { cardSections, readCardReads, type CardReadPorts, type CardTable } from "../job/card-sections";
import { bridgeTicketBoard } from "../job/ports";
import type { ReachPorts } from "../job/sections";
import { TICKET_SYSTEM } from "../roles.svelte";
import {
  boardLanes,
  COLUMN_CAP,
  columnDone,
  drawnCards,
  shownCount,
  withoutTicket,
  LANES_AT_FIRST,
  TYPED_CAP,
  type Lane,
  type LaneField,
  type ListSort,
} from "./tickets";
import { viewColumns } from "./views";

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
  ) => Promise<MiniBoard>;
  /** The selected ticket's description and its markup (#2308). */
  detail: (entityId: string) => Promise<Pick<EntityDetail, "description" | "markup">>;
  /** The job's section reads, scoped by what a ticket reaches: *Around this ticket* (#1030). */
  listEntities: ReachPorts["listEntities"];
  /** What a ticket reaches (`job_reach`, #817), for the Assets row. */
  jobReach: ReachPorts["jobReach"];
  /** One reached asset, for the Assets row's state; the bridge's when left out. */
  getAsset?: (assetId: string) => Promise<AssetDetail>;
  /** The board under `under:<key>`, for the Sub-tasks row; the bridge's when left out. */
  ticketBoard?: (query: string) => Promise<MiniBoard>;
  /** A branch's checkout on this machine, for Code's *not pushed*; the bridge's when left out. */
  entityCheckout?: (entityId: string) => Promise<CheckoutView>;
  /** The card's sections in one read over one walk of the reach (`card_reads`); where left out, a read a section. */
  cardReads?: CardReadPorts["cardReads"];
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
  readonly bands: Band[];
  /** The previewed ticket's description, or `null` until it answers or where it has none. */
  readonly description: Prose | null;
  /** What the previewed ticket reaches, by section, or `null` until it answers. */
  readonly around: CardTable | null;
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
  /** *Show N more lanes* (#939): ten more of them. */
  moreLanes(): void;
  chooseLayout(layout: "board" | "list"): void;
  chooseLanes(lanes: LaneField): void;
  /** Reads now, with no pause: a sync's re-read, which keeps the drawn cards where they are (`asked` stays). */
  refresh(): void;
  /** Reads `id`'s preview once, or drops the one out for the last when `null`. */
  preview(id: string | null): void;
  /** Stops the pause and drops every answer still out. */
  close(): void;
}

/** A board with nothing on it, new each time, as `$state` may take it: what no answer yet draws. */
function emptyBoard(): MiniBoard {
  return { columns: [], sources: [], lanes: [], more_lanes: 0 };
}

const sum = (board: MiniBoard) => board.columns.reduce((all, column) => all + column.total, 0);

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
  /**
   * How long, in ms, a moved selection waits for the next move before its preview is read: {@link PREVIEW_PAUSE_MS} on
   * the page, none when left out. The first selection, and one that comes back from none, is read at once.
   */
  previewPause?: number;
}

/**
 * How long a moved selection settles before its preview is read: the launcher's preview's 120 ms (the query audit,
 * 2026-11-12: holding j sent seven requests a row, and they queued behind each other for the pool's five connections).
 */
export const PREVIEW_PAUSE_MS = 120;

/** How many previews the reader holds, so a ticket selected again draws what it showed at once. */
const PREVIEWS_HELD = 40;

/** How long, in ms, typed words wait for a board read still out before they are read anyway. */
export const TYPED_WAIT_MOST = 400;

/** How long, in ms, the keys pause before a list showing every ticket reads all of them for its words. */
export const LIST_SETTLE_MS = 600;

export function createBoardReader(ports: BoardReaderPorts, options: BoardReaderOptions): BoardReader {
  const { typingPause, sessions } = options;
  const previewPause = options.previewPause ?? 0;
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
    description: null as Prose | null,
    around: null as CardTable | null,
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
  /** When the last board read was sent (`Date.now()`). */
  let sentAt = 0;
  /** The ticket previewed, and the generation its answers carry. */
  let previewing: string | null = null;
  let previewed = 0;
  /** A moved selection's pause before its preview is read. */
  let settlingPreview: ReturnType<typeof setTimeout> | undefined;
  /** The previews read, newest last: a ticket selected again draws its own at once while it is read again. */
  const heldPreviews = new Map<string, { description: Prose | null; around: CardTable | null }>();
  const holdPreview = (id: string, part: Partial<{ description: Prose | null; around: CardTable | null }>) => {
    const held = { description: null, around: null, ...heldPreviews.get(id), ...part };
    heldPreviews.delete(id);
    heldPreviews.set(id, held);
    if (heldPreviews.size > PREVIEWS_HELD) heldPreviews.delete(heldPreviews.keys().next().value!);
  };

  const view = $derived(options.view());
  const own = $derived(sessions.views[state.scope.key]);
  const expanded = $derived(own?.expanded ?? []);
  const listAll = $derived(own?.listAll ?? false);
  const layout = $derived(own?.layout ?? view.layout);
  const lanes = $derived(own?.lanes ?? view.lanes);
  const laned = $derived(layout === "board" && lanes !== "none");
  const typing = $derived(state.filter.words);
  const bands = $derived(
    boardLanes(board ?? emptyBoard(), laned ? lanes : "none", (columns) => viewColumns(columns, view.columns)).map(
      (lane) => ({
        lane,
        drawn: lane.columns.map((column) => {
          const listed = (view.listed ?? view.columns).find((own) => own.name === column.status)?.statuses ?? [];
          const done = columnDone(column, listed);
          return { column, cards: drawnCards(column, pagesOf(expandKey(lane, column)), typing, done), done, listed };
        }),
      }),
    ),
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
    if (quick) return cap;
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

  /**
   * A read with words still filters and counts the whole mirror, so on a
   * large one a short prefix is the broadest of them; it sends fifty cards a
   * column at most (R-3.4, #1024).
   */
  function read(quick = false, byPerson = true) {
    clearTimeout(pausing);
    const { base, savedView, without } = state.scope;
    if (base === null) return;
    const kept = (answer: MiniBoard) => (without ? withoutTicket(answer, without) : answer);
    const query = state.filter.query;
    const mine = ++reading;
    out += 1;
    sentAt = Date.now();
    ports
      .board(capFor(quick), query === "" ? null : query, savedView, lanesAsked(), TICKET_SYSTEM)
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
        state.failed = null;
        // A read the person caused (a filter, *Show more*, lanes) sorts the cards again; a sync's does not (`held-order`).
        if (byPerson) state.asked += 1;
        if (query === base) state.total = sum(drawn);
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
    get bands() {
      return bands;
    },
    get description() {
      return state.description;
    },
    get around() {
      return state.around;
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
      if (scope.base !== null) read();
    },
    /**
     * The read waits until the keys pause for `typingPause`, so a word typed
     * at speed is one read and not one a letter, and an answer still out for
     * the text before it is never drawn (#897). While a read is out the next
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
      const quick = listAll && !laned && layout === "list";
      const go = () => {
        read(quick);
        if (quick) settling = setTimeout(() => read(), LIST_SETTLE_MS);
      };
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
      read();
    },
    chooseLayout(next) {
      if (layout === next) return;
      const wasLaned = laned;
      here().layout = next;
      if (wasLaned !== laned) read();
    },
    chooseLanes(next) {
      if (lanes === next) return;
      const wasLaned = laned;
      here().lanes = next;
      state.lanesShown = LANES_AT_FIRST;
      // Each field is its own read of lanes (#939).
      if (wasLaned || laned) read();
    },
    refresh: () => read(false, false),
    preview(id) {
      if (id === previewing) return;
      const moved = previewing !== null;
      previewing = id;
      const mine = ++previewed;
      clearTimeout(settlingPreview);
      if (id === null) return;
      // The selection draws at once, with what is held of its preview; the reads wait until it stops moving.
      const held = heldPreviews.get(id);
      state.description = held?.description ?? null;
      state.around = held?.around ?? null;
      const read = () => {
        if (mine !== previewed) return;
        ports
          .detail(id)
          .then((detail) => {
            if (mine !== previewed) return;
            const description = detail.description === null ? null : { text: detail.description, markup: detail.markup };
            holdPreview(id, { description });
            if (description !== null) state.description = description;
          })
          .catch(() => {});
        // The card's own rows, through the job-reach read: no context is needed, and
        // none is promoted to draw this (#1030). A run counts under Builds because
        // the Builds read answered it, as on the card (#1300).
        void readCardReads({ ...ports, getAsset: ports.getAsset ?? getAsset, ticketBoard: ports.ticketBoard ?? bridgeTicketBoard, entityCheckout: ports.entityCheckout ?? entityCheckout }, id).then((reads) => {
          if (mine !== previewed) return;
          const around = cardSections(reads);
          holdPreview(id, { around });
          state.around = around;
        });
      };
      if (moved && previewPause > 0) settlingPreview = setTimeout(read, previewPause);
      else read();
    },
    close() {
      clearTimeout(settlingPreview);
      clearTimeout(pausing);
      clearTimeout(settling);
      queued = null;
      reading += 1;
      epoch += 1;
      previewing = null;
      previewed += 1;
    },
  };
}
