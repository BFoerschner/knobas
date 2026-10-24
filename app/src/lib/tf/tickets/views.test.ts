/** Ticket views' pure rules (#812): grouping, finding, own columns and the drafts. */
import { expect, test } from "vitest";

import type { MiniBoardCard } from "../../ipc/entity";
import type { QueryTranslation, SavedViewSource, TicketViewRow } from "../../ipc/search";
import {
  columnLimit,
  copyKeeps,
  draftCopy,
  draftFromFilter,
  draftSavedViewCopy,
  draftRestore,
  drawnSavedViewColumns,
  groupViews,
  hiddenViews,
  savedViewKindWords,
  savedViewSystems,
  systemsWords,
  savedShowing,
  sprintEnds,
  translationText,
  viewColumns,
} from "./views";

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
    saved_view: null,
    count: 1,
    unreadable: null,
    saved_view_source: null,
    ...extra,
  };
}

function card(key: string, updated: string): MiniBoardCard {
  return {
    entity_id: `jira:${key}`,
    source_id: "jira",
    key,
    title: key,
    status: null,
    priority: null,
    issue_type: null,
    assignee: null,
    updated_at: updated,
    path: null,
    parent_title: null,
    comment_count: null,
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
    saved_view: "board:3",
  });
  const fromFilter = draftFromFilter(saved, "project:PAY retry", { layout: "list", lanes: "epic", sort: savedShowing(null).sort });
  expect(fromFilter).toEqual(
    expect.objectContaining({ id: null, name: "", group: "Projects", query: "project:PAY retry", layout: "list", lanes: "epic", list_sort: null, saved_view: null }),
  );
  expect(fromFilter.columns).toEqual(saved.columns);
  expect(fromFilter.columns).not.toBe(saved.columns);
  expect(draftFromFilter(null, "", savedShowing(null))).toEqual(expect.objectContaining({ group: "Mine", query: "", layout: "board" }));
  expect(draftCopy(saved)).toEqual(expect.objectContaining({ id: null, position: null, name: "Copy of pay", saved_view: null }));
  expect(draftRestore(saved)).toEqual(expect.objectContaining({ id: "pay", position: 7, name: "pay", saved_view: "board:3" }));
});

test("a Jira view is read from a board of its type or a saved filter, and a sprint ends on a day (#879)", () => {
  expect(savedViewKindWords({ kind: "board", board_type: "scrum" })).toBe("Scrum board");
  expect(savedViewKindWords({ kind: "board", board_type: null })).toBe("board");
  expect(savedViewKindWords({ kind: "filter", board_type: null })).toBe("saved filter");
  expect(sprintEnds("2026-10-22T16:00:00Z")).toBe("22 Oct");
  expect(sprintEnds("soon")).toBe("");
});

test("Jira boards and Jira filters come after every group of your own, and a Jira view is found by its JQL", () => {
  const views = [
    row("cus", "Jira boards", 0, { saved_view_source: savedViewOf("project = CUS"), saved_view: "jira:board-31" }),
    row("ops", "Ops", 1),
    row("mine", "Mine", 2),
  ];
  expect(groupViews(views, "").map((group) => group.name)).toEqual(["Mine", "Ops", "Jira boards"]);
  expect(groupViews(views, "cus").map((group) => group.views.map((view) => view.id))).toEqual([["cus"]]);
  // "project" is only in the source's query, so this finds it by that and not by its name (#1273).
  expect(groupViews(views, "project = cus").map((group) => group.views.map((view) => view.id))).toEqual([["cus"]]);
});

function savedViewOf(sourceQuery: string, extra: Partial<SavedViewSource> = {}): SavedViewSource {
  return {
    kind: "board",
    system: "Jira",
    board_type: "scrum",
    source_query: sourceQuery,
    sub_query: null,
    sprint: null,
    last_read: "",
    web_url: null,
    hidden: false,
    new: false,
    unsynced: [],
    ...extra,
  };
}

test("a second ticket system's views come last by their source, and a group of your own is yours whatever it is called (#2032)", () => {
  const views = [
    row("triage", "Kestrel filters", 0, {
      saved_view_source: savedViewOf("assignee: me", { kind: "filter", system: "Kestrel" }),
      saved_view: "kestrel:filter-7",
    }),
    row("fix", "Jira filters", 1, { saved_view_source: savedViewOf("fixVersion is EMPTY", { kind: "filter" }), saved_view: "jira:filter-1" }),
    row("cus", "Jira boards", 2, { saved_view_source: savedViewOf("project = CUS"), saved_view: "jira:board-31" }),
    // A group somebody typed that reads like a synced one is still theirs.
    row("lookalike", "Jira boards of mine", 3),
    row("named", "Kestrel boards", 4),
    row("mine", "Mine", 5),
  ];
  expect(groupViews(views, "").map((group) => [group.name, group.system])).toEqual([
    ["Mine", null],
    ["Jira boards of mine", null],
    ["Kestrel boards", null],
    // Each system's boards before its filters, the systems in the order their first view sits.
    ["Kestrel filters", "Kestrel"],
    ["Jira boards", "Jira"],
    ["Jira filters", "Jira"],
  ]);
  expect(savedViewSystems(views)).toEqual(["Kestrel", "Jira"]);
  expect(savedViewSystems(views.filter((view) => view.saved_view_source === null))).toEqual([]);
});

test("ticket systems are named in a list a sentence can hold (#2032)", () => {
  expect(systemsWords(["Jira"])).toBe("Jira");
  expect(systemsWords(["Jira", "Kestrel"])).toBe("Jira and Kestrel");
  expect(systemsWords(["Jira", "Kestrel", "Plover"])).toBe("Jira, Kestrel and Plover");
});

test("a hidden Jira view leaves its group and is listed apart, in list order (#878, R-3.25)", () => {
  const views = [
    row("flow", "Jira boards", 4, { saved_view_source: savedViewOf("project = PAY", { hidden: true }), saved_view: "jira:board-22" }),
    row("cus", "Jira boards", 2, { saved_view_source: savedViewOf("project = CUS"), saved_view: "jira:board-31" }),
    row("fix", "Jira filters", 3, { saved_view_source: savedViewOf("fixVersion is EMPTY", { kind: "filter", hidden: true }), saved_view: "jira:filter-1" }),
  ];
  expect(groupViews(views, "").map((group) => [group.name, group.views.map((view) => view.id)])).toEqual([
    ["Jira boards", ["cus"]],
  ]);
  expect(hiddenViews(views).map((view) => view.id)).toEqual(["fix", "flow"]);
});

test("a Jira view's columns are drawn by their own names, keeping Jira's order and limits", () => {
  const own = [
    { name: "Doing", statuses: ["In Progress", "In Review"], limit: 3 },
    { name: "Done", statuses: ["Done"], limit: null },
  ];
  expect(drawnSavedViewColumns(own)).toEqual([
    { name: "Doing", statuses: ["Doing"], limit: 3 },
    { name: "Done", statuses: ["Done"], limit: null },
  ]);
  const board = [
    { status: "Doing", cards: [card("PAY-2", "2026-08-01T00:00:00Z"), card("PAY-1", "2026-08-09T00:00:00Z")], total: 2 },
  ];
  expect(viewColumns(board, drawnSavedViewColumns(own))[0]!.cards.map((c) => c.key)).toEqual(["PAY-2", "PAY-1"]);
});

test("Make an editable copy starts a view of your own from the Jira view: its conditions, Jira's columns, what it left out", () => {
  const savedViewSource = {
    kind: "board" as const,
    system: "Jira",
    board_type: "kanban",
    source_query: "project in (OPS, WEB) AND component = on-call ORDER BY priority DESC",
    sub_query: null,
    sprint: null,
    last_read: "",
    web_url: null,
    hidden: false,
    new: false,
    unsynced: [],
  };
  const columns = [{ name: "Working", statuses: ["In Progress", "In Review"], limit: 3 }];
  const board = row("mock-board-40", "Jira boards", 4, { name: "Platform on-call", saved_view_source: savedViewSource, saved_view: "mock:board-40", columns, lanes: "priority" });
  // As the Jira adapter reads the query (#1208): the source's answer, not the shell's.
  const copy: QueryTranslation = {
    conditions: [{ key: "project", negated: false, values: ["OPS", "WEB"] }],
    kept: 1,
    left_out: [{ clause: "component = on-call", reason: "knobas does not sync components" }],
  };
  const { draft, from } = draftSavedViewCopy(board, savedViewSource, copy);
  expect(draft).toEqual(
    expect.objectContaining({ id: null, position: null, name: "Platform on-call", group: "Mine", query: "project:OPS project:WEB", lanes: "priority", saved_view: null }),
  );
  expect(draft.columns).toEqual(columns);
  expect(draft.columns).not.toBe(columns);
  expect(from).toEqual({
    kind: "board",
    system: "Jira",
    name: "Platform on-call",
    left: [{ clause: "component = on-call", reason: "knobas does not sync components" }],
  });

  // *Save as a view* on a Jira view: the copy's conditions, then what the bar adds, and how the tickets show now.
  const saved = draftSavedViewCopy(board, savedViewSource, copy, "priority:High retry", { layout: "list", lanes: "epic", sort: savedShowing(null).sort });
  expect(saved.draft).toEqual(
    expect.objectContaining({ name: "", group: "Mine", query: "project:OPS project:WEB priority:High retry", layout: "list", lanes: "epic", saved_view: null }),
  );
  expect(saved.from.left).toHaveLength(1);
});

test("a source's translation is search text in the grammar's keys, quoted where a value has a space (#1208)", () => {
  const copy: QueryTranslation = {
    conditions: [
      { key: "status", negated: false, values: ["In Review"] },
      { key: "under", negated: false, values: ["PAY-12"] },
      { key: "label", negated: true, values: ["a", "b"] },
      { key: "assignee", negated: false, values: ["me"] },
    ],
    kept: 4,
    left_out: [],
  };
  expect(translationText(copy)).toBe('status:"In Review" under:PAY-12 -label:a -label:b assignee:me');
  expect(copyKeeps(copy)).toBe("A copy keeps every part of the query.");
  expect(copyKeeps({ ...copy, kept: 1, left_out: [{ clause: "x", reason: "y" }] })).toBe("A copy keeps 1 of 2 parts of the query.");
});
