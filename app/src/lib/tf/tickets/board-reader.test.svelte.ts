/**
 * The Tickets page's reads, with no mount (#1319, spec #1304): the typing
 * pause on a fake clock, the generations that drop an older answer, the cap
 * an expanded column raises, the total counted once per base, lanes paged by
 * ten, a scope change that starts fresh, and the preview's own generation.
 * Every port answers when the test says so.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LanesAsked, MiniBoard, MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import { createBoardReader, type BoardReaderPorts, type BoardView, type ViewSessions } from "./board-reader.svelte";

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
  const details: { id: string; answer: (text: string) => void }[] = [];
  const reaches: string[] = [];
  const ports: BoardReaderPorts = {
    board: (perColumn, query, savedView, lanes, role) =>
      new Promise((answer, fail) => reads.push({ perColumn, query, savedView, lanes, role, answer, fail })),
    detail: (id) => new Promise((answer) => details.push({ id, answer: (text) => answer({ body_text: text }) })),
    listEntities: () => Promise.resolve({ rows: [], total: 0, tally: null, groups: null }),
    jobReach: (id) => {
      reaches.push(id);
      return Promise.resolve([]);
    },
  };
  const sessions: ViewSessions = $state({ views: {} });
  const view: BoardView = $state({ columns: [], layout: "board", lanes: "none" });
  const reader = createBoardReader(ports, { typingPause: 40, sessions, view: () => view });
  /** What each read asked for, without its answer. */
  const asked = () => reads.map(({ perColumn, query, lanes }) => (lanes ? { perColumn, query, lanes } : { perColumn, query }));
  return { reader, reads, details, reaches, sessions, view, asked };
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
  // A read of the base itself counts the whole as it draws it.
  reader.setFilter({ query: "assignee:@me", words: false });
  await vi.advanceTimersByTimeAsync(40);
  reads.at(-1)!.answer(board(column("To Do", 31, 8), column("Done", 4)));
  await flush();
  expect(reads).toHaveLength(4);
  expect(reader.total).toBe(35);
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

test("the preview reads once per ticket and drops the answer of one no longer previewed", async () => {
  const { reader, details, reaches } = fakes();
  reader.preview("jira:PAY-1");
  reader.preview("jira:PAY-1");
  expect(details.map(({ id }) => id)).toEqual(["jira:PAY-1"]);
  details[0]!.answer("One.");
  await flush();
  expect(reader.description).toBe("One.");
  expect(reader.around).toEqual([]);

  reader.preview("jira:PAY-2");
  expect(reader.description).toBeNull();
  reader.preview("jira:PAY-3");
  details[1]!.answer("Two, late.");
  await flush();
  expect(reader.description).toBeNull();
  details[2]!.answer("Three.");
  await flush();
  expect(reader.description).toBe("Three.");
  expect(reaches).toEqual(["jira:PAY-1", "jira:PAY-2", "jira:PAY-3"]);

  // Nothing selected, then the same ticket again: it is read again.
  reader.preview(null);
  reader.preview("jira:PAY-3");
  expect(details).toHaveLength(4);
});

test("close stops the pause and drops every answer still out", async () => {
  const { reader, reads, details } = fakes();
  reader.open(ALL, NO_WORDS);
  reader.preview("jira:PAY-1");
  reader.setFilter({ query: "x", words: true });
  reader.close();
  await vi.advanceTimersByTimeAsync(100);
  expect(reads).toHaveLength(1);
  reads[0]!.answer(FIRST);
  details[0]!.answer("Gone.");
  await flush();
  expect(reader.board).toBeNull();
  expect(reader.description).toBeNull();
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
