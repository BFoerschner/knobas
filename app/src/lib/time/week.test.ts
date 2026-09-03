/**
 * The week timesheet's arithmetic (issue #283).
 *
 * Pure, and in a module of its own for the reason `day.test.ts` gives: the two
 * rules with real content — **which seven intervals a week is** and **which of
 * its columns are drawn** — are worth driving without a mounted component.
 *
 * Fixtures build instants with the local `Date` constructor, the discipline
 * `day.test.ts` records: a week is a local thing and these tests run in
 * whatever timezone the machine is set to.
 */
import { expect, test } from "vitest";

import type { Week, WeekCell, WeekRow } from "../ipc/time";
import {
  columnHeading,
  dayIsEmpty,
  daysPastHorizon,
  minutesOf,
  mondayOf,
  visibleColumns,
  weekLabel,
  weekWindows,
} from "./week";

function cell(over: Partial<WeekCell> = {}): WeekCell {
  return {
    tracked_seconds: 0,
    offered_seconds: 0,
    logged_seconds: 0,
    held_seconds: 0,
    unlogged_seconds: 0,
    ...over,
  };
}

/** A week whose only content is `cells`, keyed by column index. */
function weekWith(cells: Record<number, WeekCell>): Week {
  const row: WeekRow = {
    target: { kind: "entity", entity_id: "jira:PAY-231" },
    title: "Retry failed SEPA payouts",
    cells: Array.from({ length: 7 }, (_, index) => cells[index] ?? cell()),
  };
  return {
    days: [
      "2026-08-24",
      "2026-08-25",
      "2026-08-26",
      "2026-08-27",
      "2026-08-28",
      "2026-08-29",
      "2026-08-30",
    ],
    rows: [row],
    past_horizon: Array.from({ length: 7 }, () => false),
  };
}

test("a week begins on the Monday of the day it is asked about", () => {
  // Every day of one week resolves to the same Monday, Sunday included --
  // which is the one `getDay() - 1` gets wrong, by seven days.
  for (const day of ["2026-08-24", "2026-08-25", "2026-08-28", "2026-08-29", "2026-08-30"]) {
    expect(mondayOf(day)).toBe("2026-08-24");
  }
  // ...and the Monday after it is its own week, not the previous one's.
  expect(mondayOf("2026-08-31")).toBe("2026-08-31");
  // Across a month boundary, which `Date` rolls and arithmetic on the day
  // number does not.
  expect(mondayOf("2026-09-01")).toBe("2026-08-31");
});

test("a week is seven consecutive local midnights, not seven equal spans", () => {
  const windows = weekWindows("2026-08-27");

  expect(windows.map((window) => window.day)).toEqual([
    "2026-08-24",
    "2026-08-25",
    "2026-08-26",
    "2026-08-27",
    "2026-08-28",
    "2026-08-29",
    "2026-08-30",
  ]);
  // Each window ends exactly where the next begins: a gap would be time no
  // column claims, and an overlap would be a block counted into two columns —
  // which the backend refuses outright.
  for (let index = 0; index < 6; index += 1) {
    expect(windows[index]!.to).toBe(windows[index + 1]!.from);
  }
  // Each is that day's own midnight rather than "the first plus 24n hours",
  // which is what makes the week containing a daylight-saving change right.
  expect(new Date(windows[0]!.from)).toEqual(new Date(2026, 7, 24, 0, 0, 0, 0));
  expect(new Date(windows[6]!.to)).toEqual(new Date(2026, 7, 31, 0, 0, 0, 0));
});

/**
 * **A weekend with something on it is shown; an empty one is not** — the
 * acceptance criterion, both directions, because either alone is satisfied by
 * a view that always collapses or never does.
 */
test("an empty weekend collapses and a worked one does not", () => {
  const quiet = weekWith({ 0: cell({ tracked_seconds: 3600 }) });
  expect(visibleColumns(quiet)).toEqual([0, 1, 2, 3, 4]);

  const worked = weekWith({
    0: cell({ tracked_seconds: 3600 }),
    5: cell({ tracked_seconds: 1800, unlogged_seconds: 1800 }),
  });
  expect(visibleColumns(worked)).toEqual(
    [0, 1, 2, 3, 4, 5],
    // Saturday shows and Sunday still does not: the two weekend days collapse
    // on their own evidence, so a Saturday call-out does not drag an empty
    // Sunday onto the screen with it.
  );

  const both = weekWith({
    5: cell({ tracked_seconds: 1800 }),
    6: cell({ tracked_seconds: 900 }),
  });
  expect(visibleColumns(both)).toEqual([0, 1, 2, 3, 4, 5, 6]);
});

/**
 * **The day the strip is standing on is always drawn**, even when it is an
 * empty Saturday — otherwise the highlighted column is the one column the week
 * does not have, and a reader stepping onto a quiet weekend loses the day they
 * are on.
 */
test("the day the strip is on is drawn even when it is an empty weekend", () => {
  const quiet = weekWith({ 0: cell({ tracked_seconds: 3600 }) });

  expect(visibleColumns(quiet)).toEqual([0, 1, 2, 3, 4]);
  expect(visibleColumns(quiet, "2026-08-29")).toEqual([0, 1, 2, 3, 4, 5]);
  expect(visibleColumns(quiet, "2026-08-30")).toEqual([0, 1, 2, 3, 4, 6]);
  // ...and standing on a weekday changes nothing: that column was never at
  // risk, and the exception must not quietly expand the weekend as well.
  expect(visibleColumns(quiet, "2026-08-26")).toEqual([0, 1, 2, 3, 4]);
});

/**
 * A weekend whose only content is knobas' own guess is still a weekend a
 * person needs to see: the day strip above draws those blocks and offers
 * *Assign…* on them, and a week that hid the column would hide the offer.
 */
test("a weekend carrying only offered time is not empty", () => {
  const week = weekWith({ 5: cell({ offered_seconds: 3600 }) });
  expect(dayIsEmpty(week, 5)).toBe(false);
  expect(visibleColumns(week)).toContain(5);
});

test("a weekday is never collapsed, empty or not", () => {
  // An empty Wednesday is the gap a person opened the timesheet to find.
  expect(visibleColumns(weekWith({}))).toEqual([0, 1, 2, 3, 4]);
});

/**
 * A weekend whose only content is a **held** worklog is a weekend that needs
 * looking at, so *empty* is every number rather than only *tracked*.
 */
test("a weekend carrying only a held worklog is not empty", () => {
  const week = weekWith({ 6: cell({ held_seconds: 3600 }) });
  expect(dayIsEmpty(week, 6)).toBe(false);
  expect(visibleColumns(week)).toContain(6);
});

test("minutes are floored, because a timesheet may never invent time", () => {
  expect(minutesOf(0)).toBe(0);
  expect(minutesOf(59)).toBe(0);
  expect(minutesOf(60)).toBe(1);
  // 90 minutes and 59 seconds is 90 minutes, not 91: rounding up would be a
  // policy the reader's Jira may not have.
  expect(minutesOf(90 * 60 + 59)).toBe(90);
});

test("a column heading names a weekday and a date a calendar can be checked against", () => {
  expect(columnHeading("2026-08-24", 0)).toBe("Mon 24");
  expect(columnHeading("2026-08-30", 6)).toBe("Sun 30");
});

test("the week's heading reads the same on every machine", () => {
  expect(weekLabel(["2026-08-24", "2026-08-30"])).toBe("24–30 August 2026");
  expect(weekLabel(["2026-08-31", "2026-09-06"])).toBe("31 August – 6 September 2026");
  expect(weekLabel(["2026-12-28", "2027-01-03"])).toBe("28 December 2026 – 3 January 2027");
});

/**
 * The days a note has to name (#337), in the table's own words.
 *
 * The fixture is a **straddling** week, which is the ordinary one: `vet`
 * bounds a timesheet's column count and says nothing about where its windows
 * sit, so any run of days can be behind the horizon. A helper that answered
 * "the week is past it" or "the week is not" could not draw this at all.
 */
test("the days past the horizon are named, and only those", () => {
  const week = weekWith({});
  week.past_horizon = [true, true, true, false, false, false, false];
  expect(daysPastHorizon(week)).toEqual(["Mon 24", "Tue 25", "Wed 26"]);
});

test("a week knobas still has every beat for names no days at all", () => {
  expect(daysPastHorizon(weekWith({}))).toEqual([]);
});
