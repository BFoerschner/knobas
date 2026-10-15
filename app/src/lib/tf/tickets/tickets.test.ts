/** The Tickets view's rules (#810). */
import { expect, test } from "vitest";

import type { MiniBoardCard, MiniBoardColumn } from "../../ipc/entity";
import { aroundLines, drawnCards, initials, moreLine, previewAction, when } from "./tickets";

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
    mine: false,
  };
}

function column(status: string | null, n: number, total = n): MiniBoardColumn {
  return { status, cards: Array.from({ length: n }, (_, i) => card(i)), total };
}

test("a column draws eight, Done three, and every card once expanded or while words filter", () => {
  expect(drawnCards(column("To Do", 12), false, false)).toHaveLength(8);
  expect(drawnCards(column("done", 12), false, false)).toHaveLength(3);
  expect(drawnCards(column("To Do", 12), true, false)).toHaveLength(12);
  expect(drawnCards(column("Done", 12), false, true)).toHaveLength(12);
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
