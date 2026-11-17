/**
 * The Tickets page's reads, with no mount (#1319, spec #1304): the typing
 * pause on a fake clock, the generations that drop an older answer, the cap
 * an expanded column raises, the total counted once per base, lanes paged by
 * ten, a scope change that starts fresh, and the preview's own generation.
 * Every port answers when the test says so.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { BoardPage, LanesAsked, MiniBoard, MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import { createBoardReader, LIST_SETTLE_MS, PAGE_CARDS, TYPED_WAIT_MOST, type BoardReaderPorts, type BoardView, type ViewSessions } from "./board-reader.svelte";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/** Lets every settled promise's callbacks run. */
const flush = () => vi.advanceTimersByTimeAsync(0);

interface Read {
  perColumn: number | null;
  query: string | null;
  savedView: string | null | undefined;
  lanes: LanesAsked | null | undefined;
  role: string | undefined;
  page: BoardPage | null | undefined;
  answer: (board: MiniBoard) => void;
  fail: (error: Error) => void;
}

function card(key: string, assignee: string | null = null, status: string | null = null): MiniBoardCard {
  return {
    entity_id: `jira:${key}`,
    source_id: "jira",
    key,
    title: key,
    status,
    priority: null,
    issue_type: null,
    assignee,
    updated_at: null,
    path: null,
    parent_title: null,
    mine: false,
    labels: [],
    comment_count: null,
    signals: [],
  };
}

/** A column named `status`; its cards stand in `own`, the name's status unless a view's column gathers another. */
function column(status: string, total: number, drawn = total, own: string = status): MiniBoardColumn {
  return {
    status,
    cards: Array.from({ length: Math.min(total, drawn) }, (_, i) => card(`${status[0]}-${i + 1}`, null, own)),
    total,
  };
}

function board(...columns: MiniBoardColumn[]): MiniBoard {
  return { columns, sources: [], lanes: [], more_lanes: 0 };
}

/** Twelve To Do (eight sent) and five Done (eight sent): the board an uncapped first read answers. */
const FIRST = board(column("To Do", 12, 8), column("Done", 5));

function fakes() {
  const reads: Read[] = [];
  const ports: BoardReaderPorts = {
    board: (perColumn, query, savedView, lanes, role, page) =>
      new Promise((answer, fail) => reads.push({ perColumn, query, savedView, lanes, role, page, answer, fail })),
  };
  const sessions: ViewSessions = $state({ views: {} });
  const view: BoardView = $state({ columns: [], layout: "board", lanes: "none" });
  const reader = createBoardReader(ports, { typingPause: 40, sessions, view: () => view });
  /** What each read asked for, without its answer. */
  const asked = () => reads.map(({ perColumn, query, lanes }) => (lanes ? { perColumn, query, lanes } : { perColumn, query }));
  return { reader, reads, sessions, view, asked };
}

const ALL = { key: "", base: "", savedView: null };
const NO_WORDS = { query: "", words: false };

test("the keys typed inside the typing pause are one read of the whole word, fifty a column", async () => {
  const { reader, reads, asked } = fakes();
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(FIRST);
  await flush();
  for (const query of ["s", "se", "sep", "sepa"]) {
    reader.setFilter({ query, words: true });
    await vi.advanceTimersByTimeAsync(20);
  }
  await vi.advanceTimersByTimeAsync(19);
  expect(reads).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(1);
  expect(asked().slice(1)).toEqual([{ perColumn: 50, query: "sepa" }]);
  expect(reads[1]!.role).toBe("ticket-system");
});

test("the first key typed into a settled board reads at once; the keys typed within the pause after it wait (the broad-filters pass, 2026-11-13)", async () => {
  const { reader, reads, asked } = fakes();
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(FIRST);
  await flush();
  await vi.advanceTimersByTimeAsync(100);
  reader.setFilter({ query: "p", words: true });
  expect(asked().slice(1)).toEqual([{ perColumn: 50, query: "p" }]);
  reads[1]!.answer(board(column("To Do", 3)));
  await flush();
  expect(reader.board?.columns[0]!.total).toBe(3);
  // Typed 20 ms later, after "p" answered: within the pause, so it waits for the keys to stop.
  await vi.advanceTimersByTimeAsync(20);
  reader.setFilter({ query: "pa", words: true });
  await vi.advanceTimersByTimeAsync(20);
  reader.setFilter({ query: "pay", words: true });
  expect(reads).toHaveLength(2);
  await vi.advanceTimersByTimeAsync(40);
  expect(asked().slice(2)).toEqual([{ perColumn: 50, query: "pay" }]);
  // A read still out holds the next key, however long ago the last was typed.
  await vi.advanceTimersByTimeAsync(100);
  reader.setFilter({ query: "payo", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(reads).toHaveLength(3);
  reads[2]!.answer(board(column("To Do", 2)));
  await flush();
  expect(asked().slice(3)).toEqual([{ perColumn: 50, query: "payo" }]);
});

test("an older answer never lands: not one a later read overtook, nor one out when the next key was typed", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  reader.refresh();
  // The second read answers first; the first answers late and is dropped.
  reads[1]!.answer(board(column("Done", 1)));
  reads[0]!.answer(FIRST);
  await flush();
  expect(reader.board?.columns.map((c) => c.status)).toEqual(["Done"]);

  reader.setFilter({ query: "x", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(reads).toHaveLength(3);
  // A key typed while the read for "x" is out: that answer never draws.
  reader.setFilter({ query: "xy", words: true });
  reads[2]!.answer(board(column("To Do", 7)));
  await flush();
  expect(reader.board?.columns.map((c) => c.status)).toEqual(["Done"]);
  reads[2]!.fail(new Error("too late"));
  await flush();
  expect(reader.failed).toBeNull();
});

test("a failed read says why, and the next answer clears it", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  reads[0]!.fail(new Error("database is locked"));
  await flush();
  expect(reader.failed).toBe("database is locked");
  reader.refresh();
  reads[1]!.answer(FIRST);
  await flush();
  expect(reader.failed).toBeNull();
  expect(reader.shown).toBe(17);
});

test("a ticket's sub-tasks read every card and leave the ticket out, so its count is theirs (the live run, 2026-11-09)", async () => {
  const { reader, reads, asked } = fakes();
  reader.open({ key: "", base: "under:PAY-219", savedView: null, without: "PAY-219" }, { query: "under:PAY-219", words: false });
  expect(asked()).toEqual([{ perColumn: null, query: "under:PAY-219" }]);
  reads[0]!.answer(board({ status: "To Do", cards: [card("PAY-266"), card("PAY-254")], total: 2 }, { status: "Done", cards: [card("PAY-219")], total: 1 }));
  await flush();
  expect(reader.board?.columns.map((c) => [c.status, c.total])).toEqual([["To Do", 2]]);
  expect(reader.total).toBe(2);
});

test("expand raises the cap to that column's count and draws it whole; the others keep theirs", async () => {
  const { reader, reads, asked } = fakes();
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(FIRST);
  await flush();
  expect(asked()).toEqual([{ perColumn: 8, query: null }]);
  const [band] = reader.bands;
  reader.expand(band!.lane, band!.drawn[0]!.column);
  expect(asked().at(-1)).toEqual({ perColumn: 12, query: null });
  reads[1]!.answer(board(column("To Do", 12), column("Done", 5)));
  await flush();
  expect(reader.expanded).toEqual(["To Do"]);
  expect(reader.bands[0]!.drawn.map(({ cards }) => cards.length)).toEqual([12, 3]);
});

test("a long column's Show more reads and draws fifty more a press, never the whole board (2026-11-10)", async () => {
  const { reader, reads, asked } = fakes();
  reader.open(ALL, NO_WORDS);
  const long = { ...column("To Do", 8), total: 8016 };
  reads[0]!.answer(board(long, column("Done", 5)));
  await flush();
  reader.expand(reader.bands[0]!.lane, reader.bands[0]!.drawn[0]!.column);
  expect(asked().at(-1)).toEqual({ perColumn: 58, query: null });
  reads[1]!.answer(board({ ...column("To Do", 58), total: 8016 }, column("Done", 5)));
  await flush();
  expect(reader.bands[0]!.drawn.map(({ cards }) => cards.length)).toEqual([58, 3]);
  reader.expand(reader.bands[0]!.lane, reader.bands[0]!.drawn[0]!.column);
  expect(asked().at(-1)).toEqual({ perColumn: 108, query: null });
  expect(reader.expanded).toEqual(["To Do", "To Do"]);
});

test("in lanes, expand raises the cap for that lane's column only", async () => {
  const { reader, reads, view } = fakes();
  view.lanes = "assignee";
  reader.open(ALL, NO_WORDS);
  expect(reads[0]!.lanes).toEqual({ by: "assignee", first: 10 });
  const todo = column("To Do", 0);
  todo.cards = [...Array.from({ length: 11 }, (_, i) => card(`A-${i}`, "ana")), ...Array.from({ length: 9 }, (_, i) => card(`B-${i}`, "bo"))];
  todo.total = 20;
  reads[0]!.answer({
    columns: [todo],
    sources: [],
    lanes: [
      { key: "ana", count: 11, columns: [{ status: "To Do", ids: todo.cards.slice(0, 8).map((c) => c.entity_id), total: 11 }] },
      { key: "bo", count: 9, columns: [{ status: "To Do", ids: todo.cards.slice(11, 19).map((c) => c.entity_id), total: 9 }] },
    ],
    more_lanes: 0,
  });
  await flush();
  const ana = reader.bands[0]!;
  reader.expand(ana.lane, ana.drawn[0]!.column);
  expect(reads[1]!.perColumn).toBe(11);
  expect(reader.expanded).toEqual(["ana|To Do"]);
});

test("the list's Show N more reads every column whole and the board keeps its caps", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(FIRST);
  await flush();
  reader.chooseLayout("list");
  // Out of lanes into the list is no new read.
  expect(reads).toHaveLength(1);
  reader.expandAll();
  expect(reads[1]!.perColumn).toBe(12);
  reads[1]!.answer(board(column("To Do", 12), column("Done", 5)));
  await flush();
  reader.chooseLayout("board");
  expect(reader.bands[0]!.drawn.map(({ cards }) => cards.length)).toEqual([8, 3]);
});

test("a list showing every ticket reads fifty a column as words change, and all of them once the keys pause (2026-11-10)", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(FIRST);
  await flush();
  reader.chooseLayout("list");
  reader.expandAll();
  reads[1]!.answer(board(column("To Do", 120), column("Done", 900)));
  await flush();
  reader.setFilter({ query: "pay", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(reads[2]!.perColumn).toBe(50);
  reads[2]!.answer(board({ ...column("To Do", 50), total: 70 }, { ...column("Done", 50), total: 400 }));
  await flush();
  // Another key inside the pause starts it over: no whole read yet.
  reader.setFilter({ query: "payo", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(reads[3]!.perColumn).toBe(50);
  reads[3]!.answer(board({ ...column("To Do", 50), total: 60 }, { ...column("Done", 50), total: 300 }));
  await flush();
  await vi.advanceTimersByTimeAsync(LIST_SETTLE_MS - 1);
  expect(reads).toHaveLength(4);
  await vi.advanceTimersByTimeAsync(1);
  expect(reads[4]!.query).toBe("payo");
  expect(reads[4]!.perColumn).toBe(300);
});

test("a board whose Show more drew past fifty a column reads fifty a column on a key, and the rest once the keys pause (the ux13-overs pass, Fable's ruling)", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(board({ ...column("To Do", 8), total: 900 }, column("Done", 5)));
  await flush();
  for (let press = 0; press < 4; press += 1) {
    reader.expand(reader.bands[0]!.lane, reader.bands[0]!.drawn[0]!.column);
    reads.at(-1)!.answer(board({ ...column("To Do", 8 + 50 * (press + 1)), total: 900 }, column("Done", 5)));
    await flush();
  }
  expect(reads.at(-1)!.perColumn).toBe(208);
  const before = reads.length;
  reader.setFilter({ query: "pay", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(reads[before]!.perColumn, "the key: what the screen shows").toBe(50);
  reads[before]!.answer(board({ ...column("To Do", 50), total: 400 }, column("Done", 5)));
  await flush();
  expect(reader.bands[0]!.drawn[0]!.cards).toHaveLength(50);
  await vi.advanceTimersByTimeAsync(LIST_SETTLE_MS);
  expect(reads[before + 1]!.query).toBe("pay");
  expect(reads[before + 1]!.perColumn, "the pause: as many as its four presses of Show more draw under words").toBe(250);
  // The words cleared: still fifty a column on the key, never the eight a column of a board not expanded.
  reads[before + 1]!.answer(board({ ...column("To Do", 208), total: 400 }, column("Done", 5)));
  await flush();
  reader.setFilter({ query: "", words: false });
  await vi.advanceTimersByTimeAsync(40);
  expect(reads[before + 2]!.perColumn).toBe(50);
});

test("a quick read whose answer holds every card of every column is the whole read: no read follows it once the keys pause", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(FIRST);
  await flush();
  reader.chooseLayout("list");
  reader.expandAll();
  reads[1]!.answer(board(column("To Do", 120), column("Done", 900)));
  await flush();
  reader.setFilter({ query: "payout-17", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(reads[2]!.perColumn).toBe(50);
  reads[2]!.answer(board(column("To Do", 3), column("Done", 1)));
  await flush();
  await vi.advanceTimersByTimeAsync(LIST_SETTLE_MS * 2);
  expect(reads).toHaveLength(3);
});

// #2200's class: the list's Show N more reads every ticket, and a deep proxy
// over thousands of cards made each draw subscribe to every field it read.
test("the board drawn is the answer as read, not a reactive copy of each card", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  const answer = board(column("To Do", 12), column("Done", 5));
  reads[0]!.answer(answer);
  await flush();
  expect(reader.board).toBe(answer);
  expect(reader.bands[0]!.drawn[0]!.cards[0]).toBe(answer.columns[0]!.cards[0]);
});

test("the total is counted once per base: from a base answer, or one count read for a filter", async () => {
  const { reader, reads, asked } = fakes();
  // Coming back to a view with words typed: the whole is counted once, apart.
  reader.open({ key: "mine", base: "assignee:@me", savedView: null }, { query: "assignee:@me sepa", words: true });
  expect(asked()).toEqual([
    { perColumn: 50, query: "assignee:@me sepa" },
    { perColumn: 1, query: "assignee:@me" },
  ]);
  reads[1]!.answer(board(column("To Do", 30, 1), column("Done", 4, 1)));
  reads[0]!.answer(board(column("To Do", 2)));
  await flush();
  expect(reader.total).toBe(34);
  reader.setFilter({ query: "assignee:@me sep", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(asked().slice(2)).toEqual([{ perColumn: 50, query: "assignee:@me sep" }]);
  // A read of the base itself counts the whole as it draws it, once the read still out has answered.
  reader.setFilter({ query: "assignee:@me", words: false });
  await vi.advanceTimersByTimeAsync(40);
  expect(reads).toHaveLength(3);
  reads[2]!.answer(board(column("To Do", 9)));
  await flush();
  expect(reads).toHaveLength(4);
  reads[3]!.answer(board(column("To Do", 31, 8), column("Done", 4)));
  await flush();
  expect(reader.total).toBe(35);
});

test("words typed while a read is out wait for it, then read once as they are by then; a hung read holds them 400 ms at most", async () => {
  const { reader, reads, asked } = fakes();
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(FIRST);
  await flush();
  reader.setFilter({ query: "p", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(asked().map((read) => read.query)).toEqual([null, "p"]);
  // Two more keys while "p" is out: no read of "pa", one of "pay" when "p" answers, and its answer is not drawn.
  reader.setFilter({ query: "pa", words: true });
  await vi.advanceTimersByTimeAsync(40);
  reader.setFilter({ query: "pay", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(asked().map((read) => read.query)).toEqual([null, "p"]);
  reads[1]!.answer(board(column("To Do", 50)));
  await flush();
  expect(asked().map((read) => read.query)).toEqual([null, "p", "pay"]);
  expect(reader.board).toBe(FIRST);
  // "pay" never answers: "payo" waits for it, and goes 400 ms after "pay" was sent.
  reader.setFilter({ query: "payo", words: true });
  await vi.advanceTimersByTimeAsync(40);
  expect(asked()).toHaveLength(3);
  await vi.advanceTimersByTimeAsync(TYPED_WAIT_MOST);
  expect(asked().map((read) => read.query)).toEqual([null, "p", "pay", "payo"]);
});

test("a laned board reads its first ten lanes, ten more each time, and ten again for another field", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  reader.chooseLanes("label");
  expect(reads.at(-1)!.lanes).toEqual({ by: "label", first: 10 });
  reader.moreLanes();
  expect(reads.at(-1)!.lanes).toEqual({ by: "label", first: 20 });
  reader.moreLanes();
  expect(reads.at(-1)!.lanes).toEqual({ by: "label", first: 30 });
  reader.chooseLanes("assignee");
  expect(reads.at(-1)!.lanes).toEqual({ by: "assignee", first: 10 });
  reader.chooseLanes("none");
  expect(reads.at(-1)!.lanes).toBeNull();
  expect(reads).toHaveLength(6);
});

test("a scope change starts fresh and drops what the last scope still had out", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, { query: "sepa", words: true });
  expect(reads).toHaveLength(2);
  reader.open({ key: "jira-board", base: "", savedView: "jira:board:7" }, NO_WORDS);
  expect(reader.board).toBeNull();
  expect(reads[2]!.savedView).toBe("jira:board:7");
  // The last scope's board and count answer now, and neither lands.
  reads[0]!.answer(FIRST);
  reads[1]!.answer(FIRST);
  await flush();
  expect(reader.board).toBeNull();
  expect(reader.total).toBeNull();
  reads[2]!.answer(board(column("Backlog", 2)));
  await flush();
  expect(reader.board?.columns.map((c) => c.status)).toEqual(["Backlog"]);
  expect(reader.total).toBe(2);

  // A view with nothing to read reads nothing, and draws nothing.
  reader.open({ key: "broken", base: null, savedView: null }, NO_WORDS);
  expect(reads).toHaveLength(3);
  expect(reader.board).toBeNull();
});

test("close stops the pause and drops every answer still out", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  reader.setFilter({ query: "x", words: true });
  reader.close();
  await vi.advanceTimersByTimeAsync(100);
  expect(reads).toHaveLength(1);
  reads[0]!.answer(FIRST);
  await flush();
  expect(reader.board).toBeNull();
});

test("a Jira view's column draws three when every status it lists is done, whatever its name (#1423)", async () => {
  const { reader, reads, view } = fakes();
  // The read gathered Jira's columns under their names; the view lists their statuses.
  view.columns = [
    { name: "Ready", statuses: ["Ready"], limit: null },
    { name: "Shipped", statuses: ["Shipped"], limit: null },
    { name: "Done", statuses: ["Done"], limit: null },
  ];
  view.listed = [
    { name: "Ready", statuses: ["To Do"], limit: null },
    { name: "Shipped", statuses: ["Done"], limit: null },
    { name: "Done", statuses: ["In Review", "Done"], limit: null },
  ];
  reader.open({ key: "jira-board-21", base: "", savedView: "jira:board-21" }, NO_WORDS);
  reads[0]!.answer(board(column("Ready", 12, 12, "To Do"), column("Shipped", 12, 12, "Done"), column("Done", 12, 12, "Done")));
  await flush();
  expect(reader.bands[0]!.drawn.map(({ cards, done }) => [cards.length, done])).toEqual([
    [8, false],
    [3, true],
    [8, false],
  ]);
});


// -- a view that shows every ticket (ux18, Björn: "always show all the tickets on open"; Fable's ruling on how) --

/** A column of `total` cards of which the first `drawn` are read, keyed from `from`. */
function cards(status: string, from: number, count: number): MiniBoardCard[] {
  return Array.from({ length: count }, (_, i) => card(`${status[0]}-${from + i}`, null, status));
}

test("off, a view opens as it always did: eight a column, three for Done, and Show more to press", async () => {
  const { reader, reads } = fakes();
  reader.open(ALL, NO_WORDS);
  expect(reads[0]!.perColumn).toBe(8);
  expect(reads[0]!.page ?? null).toBeNull();
  reads[0]!.answer(board(column("To Do", 300, 8), { status: "Done", cards: cards("Done", 1, 8), total: 900 }));
  await flush();
  expect(reader.all).toBe(false);
  expect(reader.bands[0]!.drawn.map(({ cards }) => cards.length)).toEqual([8, 3]);
});

test("on, a board opens on fifty a column and draws every card it reads, Done too: no column is cut", async () => {
  const { reader, reads, view } = fakes();
  view.showAll = true;
  reader.open(ALL, NO_WORDS);
  expect(reads).toHaveLength(1);
  expect(reads[0]!.perColumn).toBe(50);
  reads[0]!.answer(board({ status: "To Do", cards: cards("To Do", 1, 50), total: 300 }, { status: "Done", cards: cards("Done", 1, 50), total: 900 }));
  await flush();
  expect(reader.all).toBe(true);
  // No eight, no three for Done: the fifty read are drawn, and nothing else is read until a column's foot nears.
  expect(reader.bands[0]!.drawn.map(({ cards }) => cards.length)).toEqual([50, 50]);
  expect(reads).toHaveLength(1);
});

test("on, a column's foot nearing reads its next 200 past those it holds, joined below them; once a column, and none for a whole one", async () => {
  const { reader, reads, view } = fakes();
  view.showAll = true;
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(board({ status: "To Do", cards: cards("To Do", 1, 50), total: 300 }, { status: "Done", cards: cards("Done", 1, 3), total: 3 }));
  await flush();
  const [todo, done] = reader.bands[0]!.drawn;
  reader.more(reader.bands[0]!.lane, todo!.column, todo!.listed);
  reader.more(reader.bands[0]!.lane, todo!.column, todo!.listed);
  reader.more(reader.bands[0]!.lane, done!.column, done!.listed);
  expect(reads).toHaveLength(2);
  expect(reads[1]!.perColumn).toBe(PAGE_CARDS);
  expect(reads[1]!.page).toEqual({ after: 50, statuses: ["To Do"] });
  reads[1]!.answer(board({ status: "To Do", cards: cards("To Do", 51, 200), total: 300 }));
  await flush();
  const drawn = reader.bands[0]!.drawn[0]!.cards;
  expect(drawn).toHaveLength(250);
  expect(drawn[49]!.key).toBe("T-50");
  expect(drawn[50]!.key).toBe("T-51");
  // The next page goes past the 250 held.
  reader.more(reader.bands[0]!.lane, reader.bands[0]!.drawn[0]!.column, []);
  expect(reads[2]!.page).toEqual({ after: 250, statuses: ["To Do"] });
});

test("on, a view's column that gathers statuses pages them as one, and a Jira view's column by its name", async () => {
  const { reader, reads, view } = fakes();
  view.showAll = true;
  view.columns = [{ name: "Doing", statuses: ["In Progress", "In Review"], limit: null }];
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(board({ status: "In Progress", cards: cards("In Progress", 1, 50), total: 120 }, { status: "In Review", cards: cards("In Review", 1, 50), total: 80 }));
  await flush();
  const [doing] = reader.bands[0]!.drawn;
  expect(doing!.column.status).toBe("Doing");
  reader.more(reader.bands[0]!.lane, doing!.column, doing!.listed);
  expect(reads[1]!.page).toEqual({ after: 100, statuses: ["In Progress", "In Review"] });

  const jira = fakes();
  jira.view.showAll = true;
  jira.view.columns = [{ name: "Doing", statuses: ["Doing"], limit: null }];
  jira.view.listed = [{ name: "Doing", statuses: ["In Progress", "In Review"], limit: null }];
  jira.reader.open({ key: "j", base: "", savedView: "jira:board-21" }, NO_WORDS);
  jira.reads[0]!.answer(board({ status: "Doing", cards: cards("Doing", 1, 50), total: 400 }));
  await flush();
  const [column] = jira.reader.bands[0]!.drawn;
  jira.reader.more(jira.reader.bands[0]!.lane, column!.column, column!.listed);
  expect(jira.reads[1]!.savedView).toBe("jira:board-21");
  expect(jira.reads[1]!.page).toEqual({ after: 50, statuses: ["Doing"] });
});

test("on, a laned board reads eight a lane's column and ten lanes, pages a lane's column in its lane, and keeps the depth for ten more lanes", async () => {
  const { reader, reads, view } = fakes();
  view.showAll = true;
  view.lanes = "assignee";
  reader.open(ALL, NO_WORDS);
  expect(reads[0]!.perColumn).toBe(8);
  expect(reads[0]!.lanes).toEqual({ by: "assignee", first: 10 });
  const first = cards("To Do", 1, 8);
  reads[0]!.answer({
    columns: [{ status: "To Do", cards: first, total: 40 }],
    sources: [],
    lanes: [{ key: "mara", count: 40, columns: [{ status: "To Do", ids: first.map((one) => one.entity_id), total: 40 }] }],
    more_lanes: 12,
  });
  await flush();
  const band = reader.bands[0]!;
  reader.more(band.lane, band.drawn[0]!.column, []);
  expect(reads[1]!.lanes).toEqual({ by: "assignee", first: 1_000_000 });
  expect(reads[1]!.page).toEqual({ after: 8, statuses: ["To Do"], lane: "mara" });
  const next = cards("To Do", 9, 32);
  reads[1]!.answer({
    columns: [{ status: "To Do", cards: next, total: 40 }],
    sources: [],
    lanes: [{ key: "mara", count: 40, columns: [{ status: "To Do", ids: next.map((one) => one.entity_id), total: 40 }] }],
    more_lanes: 0,
  });
  await flush();
  expect(reader.bands[0]!.drawn[0]!.cards).toHaveLength(40);
  // Ten lanes more as the foot nears, each lane's column as deep as it reads now.
  reader.moreLanes();
  expect(reads[2]!.lanes).toEqual({ by: "assignee", first: 20 });
  expect(reads[2]!.perColumn).toBe(40);
});

test("on, the list reads its counts and its first 200 rows sorted on the server, the next 200 as its foot nears, and its first page again for another sort", async () => {
  const { reader, reads, view } = fakes();
  view.showAll = true;
  view.layout = "list";
  view.sort = { column: "priority", direction: "ascending" };
  reader.open(ALL, NO_WORDS);
  expect(reads.map(({ perColumn, page }) => [perColumn, page?.sort ?? null])).toEqual([[1, null], [PAGE_CARDS, "priority"]]);
  expect(reads[1]!.page).toMatchObject({ after: 0, direction: "ascending", priority_order: expect.arrayContaining(["high", "low"]) });
  reads[0]!.answer(board({ status: "To Do", cards: cards("To Do", 1, 1), total: 300 }, { status: "Done", cards: cards("Done", 1, 1), total: 100 }));
  const page = [...cards("Done", 1, 100), ...cards("To Do", 1, 100)];
  reads[1]!.answer({ ...board({ status: "To Do", cards: page.slice(100), total: 300 }, { status: "Done", cards: page.slice(0, 100), total: 100 }), order: page.map((one) => one.entity_id) });
  await flush();
  // In the server's order, not grouped by column.
  expect(reader.rows!.slice(0, 2).map((one) => one.key)).toEqual(["D-1", "D-2"]);
  expect(reader.rows!.at(-1)!.key).toBe("T-100");
  reader.moreRows();
  reader.moreRows();
  expect(reads).toHaveLength(3);
  expect(reads[2]!.page).toMatchObject({ after: 200, sort: "priority" });
  const more = cards("To Do", 101, 200);
  reads[2]!.answer({ ...board({ status: "To Do", cards: more, total: 300 }), order: more.map((one) => one.entity_id) });
  await flush();
  expect(reader.rows).toHaveLength(400);
  // Every row read: no page more.
  reader.moreRows();
  expect(reads).toHaveLength(3);
  view.sort = { column: "key", direction: "descending" };
  reader.resort();
  expect(reads[3]!.page).toMatchObject({ after: 0, sort: "key", direction: "descending" });
});

test("on, a key reads fifty a column once and no whole read follows; a sync's re-read reads each column as deep as it holds", async () => {
  const { reader, reads, view } = fakes();
  view.showAll = true;
  reader.open(ALL, NO_WORDS);
  reads[0]!.answer(board({ status: "To Do", cards: cards("To Do", 1, 50), total: 300 }));
  await flush();
  reader.more(reader.bands[0]!.lane, reader.bands[0]!.drawn[0]!.column, []);
  reads[1]!.answer(board({ status: "To Do", cards: cards("To Do", 51, 200), total: 300 }));
  await flush();
  reader.refresh();
  expect(reads[2]!.perColumn).toBe(250);
  reads[2]!.answer(board({ status: "To Do", cards: cards("To Do", 1, 250), total: 300 }));
  await flush();
  reader.setFilter({ query: "pay", words: true });
  await vi.advanceTimersByTimeAsync(LIST_SETTLE_MS * 2);
  expect(reads.slice(3).map(({ perColumn, query }) => [perColumn, query])).toEqual([[50, "pay"]]);
});
