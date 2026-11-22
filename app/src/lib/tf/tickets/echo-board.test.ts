/** A ticket's move laid over the board's read (ticket-edit decisions, section 11). */
import { expect, test } from "vitest";

import type { MiniBoard, MiniBoardCard } from "../../ipc/entity";
import { withFieldEchoes, withFieldRows, withStatusEchoes, withStatusRows, type FieldEchoed } from "./echo-board";
import { viewColumns } from "./views";

const card = (key: string, status: string): MiniBoardCard => ({
  entity_id: `jira:${key}`,
  source_id: "jira",
  key,
  title: key,
  status,
  priority: null,
  issue_type: "Story",
  assignee: null,
  updated_at: "2026-11-20T10:00:00Z",
  path: null,
  parent_title: null,
  mine: false,
  labels: [],
  comment_count: null,
  signals: [],
});

function board(): MiniBoard {
  return {
    columns: [
      { status: "To Do", cards: [card("PAY-1", "To Do"), card("PAY-2", "To Do")], total: 5 },
      { status: "In Progress", cards: [card("PAY-3", "In Progress")], total: 1 },
      { status: "In Review", cards: [], total: 0 },
    ],
    sources: [{ source_id: "jira", statuses: ["To Do", "In Progress", "In Review", "Done"] }],
    lanes: [],
    more_lanes: 0,
  };
}

const moved = (to: Record<string, string>) => (id: string) => to[id] ?? null;

test("a card moves between status columns, at the top, and both totals follow", () => {
  const drawn = withStatusEchoes(board(), moved({ "jira:PAY-2": "in progress" }));
  expect(drawn.columns[0]!.cards.map((one) => one.key)).toEqual(["PAY-1"]);
  expect(drawn.columns[0]!.total).toBe(4);
  expect(drawn.columns[1]!.cards.map((one) => one.key)).toEqual(["PAY-2", "PAY-3"]);
  expect(drawn.columns[1]!.cards[0]!.status).toBe("In Progress");
  expect(drawn.columns[1]!.total).toBe(2);
});

test("no echo, or one that says the status the read has, answers the same board", () => {
  const read = board();
  expect(withStatusEchoes(read, moved({}))).toBe(read);
  expect(withStatusEchoes(read, moved({ "jira:PAY-3": "In Progress" }))).toBe(read);
});

test("with a view's columns it is gathered under the column holding its new status", () => {
  const drawn = withStatusEchoes(board(), moved({ "jira:PAY-1": "In Review" }));
  const gathered = viewColumns(drawn.columns, [
    { name: "Open", statuses: ["To Do"], limit: null },
    { name: "Doing", statuses: ["In Progress", "In Review"], limit: null },
  ] as never);
  expect(gathered[0]!.cards.map((one) => one.key)).toEqual(["PAY-2"]);
  expect(gathered[1]!.cards.map((one) => one.key).sort()).toEqual(["PAY-1", "PAY-3"]);
  expect(gathered[1]!.total).toBe(2);
});

test("a status no column holds is a column of its own at the board's end", () => {
  const drawn = withStatusEchoes(board(), moved({ "jira:PAY-3": "Done" }));
  expect(drawn.columns.map((one) => one.status)).toEqual(["To Do", "In Progress", "In Review", "Done"]);
  expect(drawn.columns[3]!.cards.map((one) => one.key)).toEqual(["PAY-3"]);
});

test("with lanes the cells' ids and totals move too", () => {
  const read: MiniBoard = {
    ...board(),
    lanes: [
      {
        key: "mara",
        count: 3,
        columns: [
          { status: "To Do", ids: ["jira:PAY-1", "jira:PAY-2"], total: 2 },
          { status: "In Progress", ids: ["jira:PAY-3"], total: 1 },
        ],
      },
    ],
  };
  const drawn = withStatusEchoes(read, moved({ "jira:PAY-1": "In Review" }));
  const cells = drawn.lanes[0]!.columns;
  expect(cells.find((cell) => cell.status === "To Do")).toEqual({ status: "To Do", ids: ["jira:PAY-2"], total: 1 });
  expect(cells.find((cell) => cell.status === "In Review")).toEqual({ status: "In Review", ids: ["jira:PAY-1"], total: 1 });
  expect(drawn.lanes[0]!.count).toBe(3);
});

test("the list's rows draw the echoed status; none touched answers the same array", () => {
  const rows = [card("PAY-1", "To Do"), card("PAY-2", "To Do")];
  expect(withStatusRows(rows, moved({}))).toBe(rows);
  expect(withStatusRows(rows, moved({ "jira:PAY-2": "Done" })).map((one) => one.status)).toEqual(["To Do", "Done"]);
});

// -- a field edit (section 26) --

const echoed = (by: Record<string, FieldEchoed>) => (id: string) => by[id] ?? null;

test("a card draws its echoed assignee, priority, summary and labels; no echo answers the same board", () => {
  const read = board();
  expect(withFieldEchoes(read, echoed({}))).toBe(read);
  const drawn = withFieldEchoes(read, echoed({ "jira:PAY-3": { assignee: { account: "mara", name: "Mara Lindqvist" }, priority: "High", title: "Retitled", labels: ["+sepa", "+triage", "-triage"] } }));
  const card = drawn.columns[1]!.cards[0]!;
  expect([card.assignee, card.assignee_name, card.priority, card.title, card.labels]).toEqual(["mara", "Mara Lindqvist", "High", "Retitled", ["sepa"]]);
  expect(drawn.columns[0]).toBe(read.columns[0]);
});

test("an echo that says what the read has answers the same board, and unassigning draws nobody", () => {
  const read = board();
  read.columns[1]!.cards[0] = { ...read.columns[1]!.cards[0]!, assignee: "mara", assignee_name: "Mara Lindqvist", priority: "High" };
  expect(withFieldEchoes(read, echoed({ "jira:PAY-3": { assignee: { account: "mara", name: "Mara Lindqvist" }, priority: "high" } }))).toBe(read);
  const drawn = withFieldEchoes(read, echoed({ "jira:PAY-3": { assignee: { account: "", name: "Unassigned" } } }));
  expect(drawn.columns[1]!.cards[0]!.assignee).toBeNull();
  expect(drawn.columns[1]!.cards[0]!.assignee_name).toBeUndefined();
});

test("laned by assignee, a reassigned card leaves its lane for its new one, in the same status, and the counts follow", () => {
  const read: MiniBoard = {
    ...board(),
    lanes: [
      { key: "mara", count: 2, columns: [{ status: "To Do", ids: ["jira:PAY-1", "jira:PAY-2"], total: 2 }] },
      { key: "", count: 1, columns: [{ status: "In Progress", ids: ["jira:PAY-3"], total: 1 }] },
    ],
  };
  read.columns[0]!.cards = read.columns[0]!.cards.map((one) => ({ ...one, assignee: "mara" }));
  const drawn = withFieldEchoes(read, echoed({ "jira:PAY-2": { assignee: { account: "", name: "Unassigned" } } }), "assignee");
  expect(drawn.lanes.map((lane) => [lane.key, lane.count])).toEqual([["mara", 1], ["", 2]]);
  expect(drawn.lanes[1]!.columns.map((cell) => [cell.status, cell.ids])).toEqual([["To Do", ["jira:PAY-2"]], ["In Progress", ["jira:PAY-3"]]]);
  // A lane no read drew yet stands at the board's end.
  const to = withFieldEchoes(read, echoed({ "jira:PAY-1": { assignee: { account: "jonas", name: "Jonas Becker" } } }), "assignee");
  expect(to.lanes.at(-1)).toEqual({ key: "jonas", count: 1, columns: [{ status: "To Do", ids: ["jira:PAY-1"], total: 1 }] });
});

test("laned by something else, a field edit moves no card between lanes", () => {
  const read: MiniBoard = { ...board(), lanes: [{ key: "Story", count: 1, columns: [{ status: "In Progress", ids: ["jira:PAY-3"], total: 1 }] }] };
  const drawn = withFieldEchoes(read, echoed({ "jira:PAY-3": { priority: "High" } }), "issuetype");
  expect(drawn.lanes).toBe(read.lanes);
});

test("the list draws the echoed fields in its rows", () => {
  const rows = board().columns[0]!.cards;
  expect(withFieldRows(rows, echoed({}))).toBe(rows);
  expect(withFieldRows(rows, echoed({ "jira:PAY-1": { title: "New" } }))[0]!.title).toBe("New");
});
