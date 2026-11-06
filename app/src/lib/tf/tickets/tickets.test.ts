/** The Tickets view's rules (#810). */
import { expect, test } from "vitest";

import type { MiniBoard, MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import {
  columnHead,
  sentenceCase,
  shortKey,
  boardLanes,
  columnDone,
  drawnCards,
  moreLanesLine,
  assigneeName,
  firstName,
  initials,
  laneTitle,
  moreLine,
  nextSort,
  previewAction,
  sharedKeys,
  sortRows,
  splitLanes,
  statusOrder,
  ticketAddress,
} from "./tickets";

function card(n: number, status: string | null = null): MiniBoardCard {
  return {
    entity_id: `jira:PAY-${n}`,
    source_id: "jira",
    key: `PAY-${n}`,
    title: `Ticket ${n}`,
    status,
    priority: null,
    issue_type: null,
    assignee: null,
    updated_at: null,
    path: null,
    parent_title: null,
    comment_count: null,
    mine: false,
    labels: [],
    signals: [],
  };
}

/** A board column of one status: its cards stand in it. */
function column(status: string | null, n: number, total = n): MiniBoardColumn {
  return { status, cards: Array.from({ length: n }, (_, i) => card(i, status)), total };
}

/** A view's column (#1423): its name, and its cards' own statuses. */
function named(name: string, statuses: (string | null)[]): MiniBoardColumn {
  return { status: name, cards: statuses.map((status, i) => card(i, status)), total: statuses.length };
}

test("a column draws eight, Done three, fifty of either while words filter, and every card once expanded", () => {
  expect(drawnCards(column("To Do", 12), false, false)).toHaveLength(8);
  expect(drawnCards(column("done", 12), false, false)).toHaveLength(3);
  expect(drawnCards(column("To Do", 12), true, false)).toHaveLength(12);
  expect(drawnCards(column("Done", 12), false, true)).toHaveLength(12);
  expect(drawnCards(column("To Do", 60), false, true)).toHaveLength(50);
  expect(drawnCards(column("Done", 60), false, true)).toHaveLength(50);
  expect(drawnCards(column("Done", 60), true, true)).toHaveLength(60);
});

test("a column is done by its tickets' statuses or the statuses it lists, never by its name (#1423)", () => {
  const shipped = named("Shipped", Array(12).fill("Done"));
  expect(columnDone(shipped)).toBe(true);
  expect(drawnCards(shipped, false, false)).toHaveLength(3);
  expect(moreLine({ ...shipped, total: 20 }, 3)).toBe("Show 17 more done");
  const done = named("Done", [...Array(11).fill("Done"), "In Progress"]);
  expect(columnDone(done)).toBe(false);
  expect(drawnCards(done, false, false)).toHaveLength(8);
  // The view's own list decides where it has one: every status it gathers done.
  const closed = named("Closed", Array(12).fill("Done"));
  expect(drawnCards(closed, false, false, columnDone(closed, ["Done"]))).toHaveLength(3);
  expect(columnDone(named("Done", Array(12).fill("Done")), ["In Review", "Done"])).toBe(false);
  expect(columnDone(named("Done", []))).toBe(false);
});

test("Show N more counts what the column holds past what it draws, Done saying so", () => {
  expect(moreLine(column("To Do", 8, 30), 8)).toBe("Show 22 more");
  expect(moreLine(column("Done", 8, 40), 3)).toBe("Show 37 more done");
  expect(moreLine(column("To Do", 5), 5)).toBeNull();
});

test("initials are a person's first and last names' letters, an account's first two letters, or a dash for nobody", () => {
  expect(initials("mara.lindqvist")).toBe("MA");
  expect(initials("mara")).toBe("MA");
  expect(initials("Lena Hoffmann")).toBe("LH");
  expect(initials("jonas")).toBe("JO");
  expect(initials(null)).toBe("–");
});

test("the preview's button: Start work, Resume work or Open ticket by the ticket's state", () => {
  const mine = (status: string) => ({ assignee: "mara", mine: true, status });
  const theirs = { assignee: "jonas.becker", mine: false, status: "To Do" };
  // *Start work* shows the steps and chooses nothing until they run (F-130).
  expect(previewAction(mine("To Do"), false)).toMatchObject({ label: "Start work", chooses: false, asks: "steps" });
  expect(previewAction(mine("In Progress"), false)).toEqual({
    label: "Resume work",
    note: "Starts the timer on this ticket.",
    chooses: true,
    asks: "timer",
  });
  expect(previewAction(mine("In Progress"), true)).toEqual({ label: "Resume work", note: "Your timer is running on this ticket.", chooses: true });
  // *Open job* only reads (spec #2396): it goes to the ticket's page and chooses nothing.
  expect(previewAction(mine("Done"), false)).toMatchObject({ label: "Open ticket", chooses: false });
  expect(previewAction(theirs, false)).toEqual({ label: "Open ticket", note: "Assigned to jonas.becker.", chooses: false });
  expect(previewAction({ assignee: null, mine: false, status: "To Do" }, false).note).toBe("Unassigned.");
});

test("the preview's button reads the card's status, not the column it stands in (#1423)", () => {
  // PAY-233 is To Do and mine, under a Jira view's *Ready*; a done one under *Shipped*.
  const [ready] = named("Ready", ["To Do"]).cards;
  expect(previewAction({ ...ready!, assignee: "mara", mine: true }, false).label).toBe("Start work");
  const [shipped] = named("Shipped", ["Done"]).cards;
  expect(previewAction({ ...shipped!, assignee: "mara", mine: true }, false).label).toBe("Open ticket");
});

function ticket(key: string, extra: Partial<MiniBoardCard>): MiniBoardCard {
  return { ...card(0), entity_id: `jira:${key}`, key, title: key, ...extra };
}

test("lanes split the board into bands by a field, each with every column and its own count, biggest first", () => {
  const columns: MiniBoardColumn[] = [
    {
      status: "To Do",
      cards: [
        ticket("PAY-1", { assignee: "jonas" }),
        ticket("PAY-2", { assignee: "mara" }),
        ticket("PAY-3", { assignee: "mara" }),
      ],
      total: 3,
    },
    { status: "Done", cards: [ticket("PAY-4", { assignee: null }), ticket("PAY-5", { assignee: "mara" })], total: 2 },
  ];
  const lanes = splitLanes(columns, "assignee");
  expect(lanes.map((lane) => [lane.title, lane.count])).toEqual([
    ["mara", 3],
    ["jonas", 1],
    ["Unassigned", 1],
  ]);
  expect(lanes[0]!.columns.map((column) => [column.status, column.cards.map((c) => c.key), column.total])).toEqual([
    ["To Do", ["PAY-2", "PAY-3"], 2],
    ["Done", ["PAY-5"], 1],
  ]);
  // Every lane keeps every column, an empty one included.
  expect(lanes[1]!.columns.map((column) => [column.status, column.total])).toEqual([
    ["To Do", 1],
    ["Done", 0],
  ]);
  // None is one band, untitled, that is the board.
  expect(splitLanes(columns, "none")).toEqual([{ key: null, title: "", count: 5, columns }]);
});

test("a label lane is the ticket's first label; epic, priority and type lanes read their fields", () => {
  const columns: MiniBoardColumn[] = [
    {
      status: "To Do",
      cards: [
        ticket("PAY-1", { labels: ["sepa", "api"], path: "PAY-200", priority: "High", issue_type: "Bug" }),
        ticket("PAY-2", { labels: ["api"] }),
        ticket("PAY-3", { labels: [] }),
      ],
      total: 3,
    },
  ];
  expect(splitLanes(columns, "label").map((lane) => lane.title)).toEqual(["sepa", "api", "No label"]);
  expect(splitLanes(columns, "epic").map((lane) => lane.title)).toEqual(["No epic", "PAY-200"]);
  expect(splitLanes(columns, "priority").map((lane) => lane.title)).toEqual(["No priority", "High"]);
  expect(splitLanes(columns, "issuetype").map((lane) => lane.title)).toEqual(["No type", "Bug"]);
  expect(laneTitle("epic", "PAY-300")).toBe("PAY-300");
});

test("list headers sort by their column, a second press turning the direction, missing values last", () => {
  const rows = [
    { card: ticket("PAY-10", { title: "b", priority: "Low", assignee: "mara", updated_at: "2026-08-20T10:00:00Z", status: "Done" }) },
    { card: ticket("PAY-9", { title: "a", priority: "Highest", assignee: null, updated_at: null, status: "To Do" }) },
    { card: ticket("PAY-11", { title: "c", priority: null, assignee: "jonas", updated_at: "2026-08-22T10:00:00Z", status: "In Progress" }) },
  ];
  // A view's columns: *Ready* holds To Do, *In flight* In Progress; the order is the cards' own statuses (#1423).
  const order = statusOrder([named("Ready", ["To Do"]), named("In flight", ["In Progress", "To Do"]), named("Shipped", ["Done"])]);
  expect(order).toEqual(["To Do", "In Progress", "Done"]);
  const keys = (column: Parameters<typeof nextSort>[1], direction: "ascending" | "descending") =>
    sortRows(rows, { column, direction }, order).map((row) => row.card.key);
  expect(keys("key", "ascending")).toEqual(["PAY-9", "PAY-10", "PAY-11"]);
  expect(keys("key", "descending")).toEqual(["PAY-11", "PAY-10", "PAY-9"]);
  expect(keys("summary", "ascending")).toEqual(["PAY-9", "PAY-10", "PAY-11"]);
  expect(keys("status", "ascending")).toEqual(["PAY-9", "PAY-11", "PAY-10"]);
  expect(keys("priority", "ascending")).toEqual(["PAY-9", "PAY-10", "PAY-11"]);
  expect(keys("priority", "descending")).toEqual(["PAY-10", "PAY-9", "PAY-11"]);
  expect(keys("assignee", "ascending")).toEqual(["PAY-11", "PAY-10", "PAY-9"]);
  expect(keys("updated", "descending")).toEqual(["PAY-11", "PAY-10", "PAY-9"]);
  expect(keys("updated", "ascending")).toEqual(["PAY-10", "PAY-11", "PAY-9"]);

  expect(nextSort({ column: "updated", direction: "descending" }, "updated")).toEqual({
    column: "updated",
    direction: "ascending",
  });
  expect(nextSort({ column: "updated", direction: "descending" }, "key")).toEqual({
    column: "key",
    direction: "ascending",
  });
});

test("a board read in lanes draws the server's lanes in its order, each with every column and its own counts (#939)", () => {
  const board: MiniBoard = {
    columns: [
      { status: "To Do", cards: [ticket("PAY-1", {}), ticket("PAY-2", {})], total: 9 },
      { status: "In Review", cards: [ticket("PAY-3", {})], total: 4 },
    ],
    sources: [],
    lanes: [
      {
        key: "mara",
        count: 7,
        columns: [
          { status: "To Do", ids: ["jira:PAY-2", "jira:PAY-1"], total: 5 },
          { status: "In Review", ids: ["jira:PAY-3"], total: 2 },
        ],
      },
      { key: "", count: 6, columns: [{ status: "To Do", ids: [], total: 4 }] },
    ],
    more_lanes: 3,
  };
  const lanes = boardLanes(board, "assignee", (columns) => columns);
  expect(lanes.map((lane) => [lane.key, lane.title, lane.count])).toEqual([
    ["mara", "mara", 7],
    ["", "Unassigned", 6],
  ]);
  expect(lanes[0]!.columns.map((c) => [c.status, c.cards.map((card) => card.key), c.total])).toEqual([
    ["To Do", ["PAY-2", "PAY-1"], 5],
    ["In Review", ["PAY-3"], 2],
  ]);
  expect(lanes[1]!.columns.map((c) => [c.status, c.cards.length, c.total])).toEqual([
    ["To Do", 0, 4],
    ["In Review", 0, 0],
  ]);
  // Without lanes, one untitled band that is the board, counted whole.
  expect(boardLanes(board, "none", (columns) => columns)).toEqual([
    { key: null, title: "", count: 13, columns: board.columns },
  ]);
  expect(moreLanesLine(3)).toBe("Show 3 more lanes");
  expect(moreLanesLine(1)).toBe("Show 1 more lane");
  expect(moreLanesLine(25)).toBe("Show 10 more lanes");
  expect(moreLanesLine(0)).toBeNull();
});

test("a key two sources hold is shared, and every ticket opens at its source's address (#1476, #1977)", () => {
  const jira = card(231, "To Do");
  const mock = { ...card(231, "Done"), entity_id: "mock:PAY-231", source_id: "mock" };
  const columns = [named("To Do", ["To Do"]), { status: "Done", cards: [mock, card(7, "Done")], total: 2 }];
  columns[0]!.cards = [jira, card(8, "To Do")];
  const shared = sharedKeys(columns);
  expect([...shared]).toEqual(["PAY-231"]);
  expect(ticketAddress(jira)).toBe("#/ticket/jira%3APAY-231");
  expect(ticketAddress(mock)).toBe("#/ticket/mock%3APAY-231");
  // A key one source holds names its source too.
  expect(ticketAddress(card(7))).toBe("#/ticket/jira%3APAY-7");
});

test("a forge's long key keeps its repository and number; other keys stay whole", () => {
  expect(shortKey("tidewater/payout-service/issues/143")).toBe("payout-service#143");
  expect(shortKey("tidewater/ledger#12")).toBe("ledger#12");
  expect(shortKey("PAY-231")).toBe("PAY-231");
});

test("a source's lower-case word reads as a label, and a written one stays as it is", () => {
  expect(sentenceCase("closed")).toBe("Closed");
  expect(sentenceCase("In Progress")).toBe("In Progress");
  expect(sentenceCase(null)).toBeNull();
});

test("a column's head reads as a state only when it is called after one it holds", () => {
  const columns = [
    { name: "In Progress", statuses: ["In Progress"] },
    { name: "Waiting on Björn", statuses: ["Blocked"] },
    { name: "Doing", statuses: ["In Progress", "In Review"] },
  ];
  expect(columnHead("In Progress", columns)).toBe("In progress");
  expect(columnHead("Waiting on Björn", columns)).toBe("Waiting on Björn");
  expect(columnHead("Doing", columns)).toBe("Doing");
  expect(columnHead("To Do", [])).toBe("To do");
  expect(columnHead(null, [])).toBe("No status");
});

test("a card calls its assignee by name where the source keeps one, else by account (F-107)", () => {
  expect(assigneeName({ assignee: "mara", assignee_name: "Mara Lindqvist" })).toBe("Mara Lindqvist");
  expect(assigneeName({ assignee: "mara" })).toBe("mara");
  expect(assigneeName({ assignee: null })).toBeNull();
  expect(firstName("Mara Lindqvist")).toBe("Mara");
  expect(firstName("mara")).toBe("mara");
  expect(firstName(null)).toBeNull();
  expect(initials(assigneeName({ assignee: "mara", assignee_name: "Mara Lindqvist" }))).toBe(initials("Mara Lindqvist"));
});
