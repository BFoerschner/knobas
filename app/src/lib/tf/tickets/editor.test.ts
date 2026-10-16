/** The view editor's pure rules (#814): text to conditions and back, the board's columns, the preview's counts. */
import { describe, expect, test } from "vitest";

import type { MiniBoardColumn } from "../../ipc/entity";
import {
  conditionsText,
  editColumns,
  looseStatuses,
  moveColumn,
  onePerStatus,
  parseConditions,
  previewBoard,
  searchParts,
  storedColumns,
  toggleStatus,
  type EditColumn,
} from "./editor";

describe("text to conditions to text", () => {
  // One text per grammar key of #811 and the two built keys the editor
  // offers (`under:` for Epic, `updated:`), each as the editor writes it.
  test.each([
    ["project:PAY"],
    ["project:PAY project:OPS"],
    ["-project:CUS"],
    ['status:"In Review"'],
    ['-status:Done status:"To Do"'],
    ["issuetype:Bug"],
    ["priority:High priority:Medium"],
    ["assignee:@me"],
    ["assignee:@me assignee:jonas.becker"],
    ["-assignee:jonas.becker"],
    ["label:sepa"],
    ["sprint:current"],
    ["assigned:today"],
    ["assigned:week"],
    ["has:failing-build has:open-pr"],
    ["-has:blocker"],
    ["under:PAY-200"],
    ["updated:7d"],
    ['project:PAY issuetype:Bug -status:Done label:"tech debt" sepa retry'],
    ["sepa"],
    [""],
  ])("%s", (text) => {
    expect(conditionsText(parseConditions(text))).toBe(text);
  });

  test("reads each key into its field, sign and values", () => {
    expect(parseConditions('project:PAY -status:Done status:"In Review" -status:"To Do" refund').conditions).toEqual([
      { field: "project", negated: false, values: ["PAY"] },
      { field: "status", negated: true, values: ["Done", "To Do"] },
      { field: "status", negated: false, values: ["In Review"] },
    ]);
    expect(parseConditions("has:blocker under:PAY-200 updated:30d assigned:week sprint:current").conditions).toEqual([
      { field: "signal", negated: false, values: ["blocker"] },
      { field: "epic", negated: false, values: ["PAY-200"] },
      { field: "updated", negated: false, values: ["30d"] },
      { field: "assigned", negated: false, values: ["week"] },
      { field: "sprint", negated: false, values: ["current"] },
    ]);
  });

  test("anything that is not a condition stays as typed in the words, in its order", () => {
    const read = parseConditions('is:mine sepa project:PAY "partial refund" /jira');
    expect(read.words).toBe('is:mine sepa "partial refund" /jira');
    expect(conditionsText(read)).toBe('project:PAY is:mine sepa "partial refund" /jira');
    // ... and the conditions round-trip from there.
    expect(parseConditions(conditionsText(read))).toEqual(read);
  });

  test("a condition with no value writes nothing; a key with an empty value is a word", () => {
    expect(conditionsText({ conditions: [{ field: "status", negated: false, values: [] }], words: "" })).toBe("");
    expect(parseConditions("status:").words).toBe("status:");
  });
});

describe("the search line", () => {
  test("underlines the tokens the grammar does not parse", () => {
    const parts = searchParts(
      parseConditions("sprint:next sprint:current has:nonsense has:blocker -under:PAY-200 -updated:7d assigned:soon status:Done"),
    );
    expect(parts.filter((part) => !part.parses).map((part) => part.text)).toEqual([
      "sprint:next",
      "has:nonsense",
      "-under:PAY-200",
      "-updated:7d",
      "assigned:soon",
    ]);
    expect(parts.filter((part) => part.parses).map((part) => part.text)).toEqual([
      "sprint:current",
      "has:blocker",
      "status:Done",
    ]);
  });
});

describe("the board's columns", () => {
  const statuses = ["To Do", "In Progress", "In Review", "Done"];

  test("one column per status, in the board's order", () => {
    expect(onePerStatus(statuses).map((column) => [column.name, column.statuses, column.limit])).toEqual([
      ["To Do", ["To Do"], null],
      ["In Progress", ["In Progress"], null],
      ["In Review", ["In Review"], null],
      ["Done", ["Done"], null],
    ]);
  });

  test("a view with no columns of its own edits one per status; a view with its own edits those", () => {
    expect(editColumns([], statuses)).toEqual(onePerStatus(statuses));
    expect(editColumns([{ name: "Open", statuses: ["To Do"], limit: 3 }], statuses)).toEqual([
      { name: "Open", statuses: ["To Do"], limit: 3 },
    ]);
  });

  test("each status sits on at most one column: putting it on one takes it off the other", () => {
    let columns = onePerStatus(statuses);
    columns = toggleStatus(columns, 1, "In Review");
    expect(columns.map((column) => column.statuses)).toEqual([["To Do"], ["In Progress", "In Review"], [], ["Done"]]);
    // Pressed again, it leaves its column and sits on none.
    columns = toggleStatus(columns, 1, "In Review");
    expect(columns.map((column) => column.statuses)).toEqual([["To Do"], ["In Progress"], [], ["Done"]]);
    expect(looseStatuses(columns, statuses)).toEqual(["In Review"]);
  });

  test("columns move earlier and later", () => {
    const columns = onePerStatus(statuses);
    expect(moveColumn(columns, 0, 1).map((column) => column.name)).toEqual(["In Progress", "To Do", "In Review", "Done"]);
    expect(moveColumn(columns, 3, -1).map((column) => column.name)).toEqual(["To Do", "In Progress", "Done", "In Review"]);
  });

  test("one column per status is stored as none of its own; anything else as typed, a blank limit as none", () => {
    expect(storedColumns(onePerStatus(statuses), statuses)).toEqual([]);
    const own: EditColumn[] = [
      { name: "Open", statuses: ["To Do"], limit: null },
      { name: "Working", statuses: ["In Progress", "In Review"], limit: 0 },
      { name: "Shipped", statuses: ["Done"], limit: 4 },
    ];
    expect(storedColumns(own, statuses)).toEqual([
      { name: "Open", statuses: ["To Do"], limit: null },
      { name: "Working", statuses: ["In Progress", "In Review"], limit: null },
      { name: "Shipped", statuses: ["Done"], limit: 4 },
    ]);
    // Reordered, it is the reader's own.
    expect(storedColumns(moveColumn(onePerStatus(statuses), 0, 1), statuses)).toHaveLength(4);
  });
});

describe("the preview", () => {
  const column = (status: string | null, total: number): MiniBoardColumn => ({ status, cards: [], total });
  const board = [column("To Do", 5), column("In Progress", 3), column("In Review", 2), column("Done", 7), column(null, 1)];

  test("counts every ticket the view holds, gathers the columns, and calls out a column over its limit", () => {
    const shown = previewBoard(board, [
      { name: "Open", statuses: ["To Do"], limit: 4 },
      { name: "Working", statuses: ["In Progress", "In Review"], limit: 6 },
    ]);
    expect(shown.count).toBe(18);
    expect(shown.columns.map((c) => [c.name, c.total, c.over])).toEqual([
      ["Open", 5, true],
      ["Working", 5, false],
    ]);
    // Done (7) and the statusless one (1) are on no column.
    expect(shown.hidden).toBe(8);
  });
});
