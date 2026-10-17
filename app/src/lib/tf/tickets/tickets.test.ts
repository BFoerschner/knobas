/** The Tickets view's rules (#810). */
import { expect, test } from "vitest";

import type { MiniBoard, MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import {
  aroundLines,
  boardLanes,
  drawnCards,
  moreLanesLine,
  initials,
  laneTitle,
  moreLine,
  nextSort,
  previewAction,
  sortRows,
  splitLanes,
  when,
} from "./tickets";

function card(n: number): MiniBoardCard {
  return {
    entity_id: `jira:PAY-${n}`,
    source_id: "jira",
    key: `PAY-${n}`,
    title: `Ticket ${n}`,
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

function column(status: string | null, n: number, total = n): MiniBoardColumn {
  return { status, cards: Array.from({ length: n }, (_, i) => card(i)), total };
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

test("Show N more counts what the column holds past what it draws, Done saying so", () => {
  expect(moreLine(column("To Do", 8, 30), 8)).toBe("Show 22 more");
  expect(moreLine(column("Done", 8, 40), 3)).toBe("Show 37 more done");
  expect(moreLine(column("To Do", 5), 5)).toBeNull();
});

test("initials are the account's two parts, or its first two letters, or a dash for nobody", () => {
  expect(initials("mara.lindqvist")).toBe("ML");
  expect(initials("jonas")).toBe("JO");
  expect(initials(null)).toBe("–");
});

test("a ticket's update reads today, yesterday or the date, as the prototype writes it", () => {
  const now = new Date("2026-08-22T15:00:00+02:00");
  expect(when("2026-08-22T11:48:00+02:00", now)).toBe("today 11:48");
  expect(when("2026-08-21T16:05:00+02:00", now)).toBe("yesterday 16:05");
  expect(when("2026-08-18T10:00:00+02:00", now)).toBe("2026-08-18");
  expect(when(null, now)).toBe("never");
});

test("the preview's button: Start work, Resume work or Open job by the ticket's state", () => {
  const mine = { assignee: "mara", mine: true };
  const theirs = { assignee: "jonas.becker", mine: false };
  expect(previewAction(mine, "To Do", false).label).toBe("Start work");
  expect(previewAction(mine, "In Progress", false)).toEqual({
    label: "Resume work",
    note: "Starts the timer on this ticket.",
  });
  expect(previewAction(mine, "In Progress", true).note).toBe("Your timer is running on this ticket.");
  expect(previewAction(mine, "Done", false).label).toBe("Open job");
  expect(previewAction(theirs, "To Do", false)).toEqual({ label: "Open job", note: "Assigned to jonas.becker." });
  expect(previewAction({ assignee: null, mine: false }, "To Do", false).note).toBe("Unassigned.");
});

test("Around this ticket groups the context's kinds as the room's tiles do, and counts what fails", () => {
  expect(
    aroundLines([
      { kind: "ticket", tone: null, count: 3 },
      { kind: "pr", tone: "info", count: 2 },
      { kind: "commit", tone: null, count: 3 },
      { kind: "build", tone: "err", count: 1 },
      { kind: "build", tone: "ok", count: 6 },
      { kind: "page", tone: null, count: 3 },
      { kind: "monitor", tone: "err", count: 1 },
    ]),
  ).toEqual([
    { section: "Code", count: "5 items", failing: null },
    { section: "Builds", count: "7 builds", failing: "1 failing or down" },
    { section: "Docs", count: "3 pages", failing: null },
    { section: "Monitors", count: "1 monitor", failing: "1 failing or down" },
  ]);
  expect(aroundLines(null)).toEqual([]);
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
    { card: ticket("PAY-10", { title: "b", priority: "Low", assignee: "mara", updated_at: "2026-08-20T10:00:00Z" }), status: "Done" },
    { card: ticket("PAY-9", { title: "a", priority: "Highest", assignee: null, updated_at: null }), status: "To Do" },
    { card: ticket("PAY-11", { title: "c", priority: null, assignee: "jonas", updated_at: "2026-08-22T10:00:00Z" }), status: "In Progress" },
  ];
  const order = ["To Do", "In Progress", "Done"];
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
