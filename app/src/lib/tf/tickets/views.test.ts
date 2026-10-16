/** Ticket views' pure rules (#812): grouping, finding, own columns and the drafts. */
import { expect, test } from "vitest";

import type { MiniBoardCard } from "../../ipc/entity";
import type { TicketViewRow } from "../../ipc/search";
import { columnLimit, draftCopy, draftFromFilter, draftRestore, groupViews, savedShowing, viewColumns } from "./views";

function row(id: string, group: string, position: number, extra: Partial<TicketViewRow> = {}): TicketViewRow {
  return {
    id,
    name: id,
    group,
    position,
    query: "",
    layout: "board",
    columns: [],
    lanes: "none",
    card_fields: ["signals"],
    list_columns: ["key"],
    list_sort: null,
    jira_ref: null,
    count: 1,
    unreadable: null,
    ...extra,
  };
}

function card(key: string, updated: string): MiniBoardCard {
  return {
    entity_id: `jira:${key}`,
    source_id: "jira",
    key,
    title: key,
    priority: null,
    issue_type: null,
    assignee: null,
    updated_at: updated,
    path: null,
    mine: false,
    labels: [],
    signals: [],
  };
}

test("own groups first in their order, typed groups after in the order their first view sits, views by position", () => {
  const groups = groupViews(
    [row("z", "Zeta", 0), row("p", "Projects", 1), row("a", "Alpha", 2), row("m2", "Mine", 5), row("m1", "Mine", 3)],
    "",
  );
  expect(groups.map((g) => [g.name, g.views.map((v) => v.id)])).toEqual([
    ["Mine", ["m1", "m2"]],
    ["Projects", ["p"]],
    ["Zeta", ["z"]],
    ["Alpha", ["a"]],
  ]);
});

test("finding needs every word somewhere in the name or the text, and drops groups left empty", () => {
  const views = [row("bugs", "Mine", 0, { query: "issuetype:Bug" }), row("ops", "Team", 1, { name: "OPS board" })];
  expect(groupViews(views, "BUG").map((g) => g.name)).toEqual(["Mine"]);
  expect(groupViews(views, "ops board").map((g) => g.name)).toEqual(["Team"]);
  expect(groupViews(views, "ops bug")).toEqual([]);
});

test("own columns gather their statuses newest first and leave a status on no column out", () => {
  const board = [
    { status: "To Do", cards: [card("A", "2026-08-01")], total: 4 },
    { status: "In Progress", cards: [card("B", "2026-08-03")], total: 1 },
    { status: "Blocked", cards: [card("C", "2026-08-02")], total: 1 },
  ];
  expect(viewColumns(board, [])).toBe(board);
  const own = [{ name: "Open", statuses: ["To Do", "In Progress"], limit: 3 }];
  expect(viewColumns(board, own)).toEqual([{ status: "Open", cards: [card("B", "2026-08-03"), card("A", "2026-08-01")], total: 5 }]);
  expect(columnLimit(own, "Open")).toBe(3);
  expect(columnLimit(own, "Done")).toBeNull();
});

test("Save as a view takes the text and how it shows now; Duplicate and Undo keep the setup", () => {
  const saved = row("pay", "Projects", 7, {
    query: "project:PAY",
    columns: [{ name: "Open", statuses: ["To Do"], limit: null }],
    jira_ref: "board:3",
  });
  const fromFilter = draftFromFilter(saved, "project:PAY retry", { layout: "list", lanes: "epic", sort: savedShowing(null).sort });
  expect(fromFilter).toEqual(
    expect.objectContaining({ id: null, name: "", group: "Projects", query: "project:PAY retry", layout: "list", lanes: "epic", list_sort: null, jira_ref: null }),
  );
  expect(fromFilter.columns).toEqual(saved.columns);
  expect(fromFilter.columns).not.toBe(saved.columns);
  expect(draftFromFilter(null, "", savedShowing(null))).toEqual(expect.objectContaining({ group: "Mine", query: "", layout: "board" }));
  expect(draftCopy(saved)).toEqual(expect.objectContaining({ id: null, position: null, name: "Copy of pay", jira_ref: null }));
  expect(draftRestore(saved)).toEqual(expect.objectContaining({ id: "pay", position: 7, name: "pay", jira_ref: "board:3" }));
});
