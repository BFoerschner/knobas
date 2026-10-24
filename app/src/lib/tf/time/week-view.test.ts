/**
 * The Week tab as arithmetic (#828, R-13.4): what each cell says, the four
 * figures, the running timer's share of today, and the held worklogs' notes.
 *
 * Instants are local, for `day.test.ts`'s reason.
 */
import { expect, test } from "vitest";

import type { QueuedWrite } from "../../ipc/sources";
import type { RunningTimer, Week, WeekCell } from "../../ipc/time";
import { heldNotes, weekView } from "./week-view";

const DAYS = ["2026-08-17", "2026-08-18", "2026-08-19", "2026-08-20", "2026-08-21", "2026-08-22", "2026-08-23"];

/** A cell from minutes: tracked, logged, held. */
function cell(tracked: number, logged = 0, held = 0): WeekCell {
  return {
    tracked_seconds: tracked * 60,
    offered_seconds: 0,
    logged_seconds: logged * 60,
    held_seconds: held * 60,
    unlogged_seconds: Math.max(0, tracked - logged - held) * 60,
  };
}

const EMPTY = Array.from({ length: 7 }, () => cell(0));

function week(): Week {
  return {
    days: DAYS,
    rows: [
      {
        target: { kind: "entity", entity_id: "jira:PAY-228" },
        title: "Fix ledger drift on partial refunds",
        cells: [cell(120, 120), cell(60, 60), cell(90, 90), cell(160, 100, 60), cell(35, 35), cell(0), cell(0)],
      },
      {
        target: { kind: "entity", entity_id: "jira:PAY-231" },
        title: "Retry failed SEPA payouts",
        cells: [cell(0), cell(90, 90), cell(180, 180), cell(270, 180), cell(164), cell(0), cell(0)],
      },
      {
        target: null,
        title: null,
        cells: [cell(18), cell(12), cell(36), cell(6), cell(35), cell(0), cell(0)],
      },
    ],
    past_horizon: DAYS.map(() => false),
  };
}

const FRIDAY_NOON = new Date(2026, 7, 21, 12, 0);

test("each cell says what is left to log, else what is held, else that it is logged", () => {
  const view = weekView(week(), null, FRIDAY_NOON);
  const pay228 = view.rows[0]!;
  expect(pay228.cells.map((c) => c.note)).toEqual(["logged", "logged", "logged", "1h held", "logged", null, null]);
  const pay231 = view.rows[1]!;
  expect(pay231.cells.map((c) => c.note)).toEqual([null, "logged", "logged", "1h 30m to log", "2h 44m to log", null, null]);
  expect(pay231.cells[4]).toMatchObject({ tracked: 164, logged: 0, held: 0, toLog: 164 });
});

test("the no-target row draws its minutes and no bar or note: nothing on it can be logged", () => {
  const open = weekView(week(), null, FRIDAY_NOON).rows[2]!;
  expect(open.target).toBeNull();
  expect(open.cells[2]).toMatchObject({ tracked: 36, note: null, bar: false });
});

test("a ticket's cell with time draws its bar; an empty cell does not", () => {
  const pay231 = weekView(week(), null, FRIDAY_NOON).rows[1]!;
  expect(pay231.cells.map((c) => c.bar)).toEqual([false, true, true, true, true, false, false]);
});

test("Saturday and Sunday are the weekend columns, Monday first", () => {
  const view = weekView(week(), null, FRIDAY_NOON);
  expect(view.rows[0]!.cells.map((c) => c.weekend)).toEqual([false, false, false, false, false, true, true]);
});

test("the figures: tracked counts the no-target row, to log does not", () => {
  const view = weekView(week(), null, FRIDAY_NOON);
  // 465 + 704 + 107 tracked; 405 + 450 logged; 60 held; 0 + 254 to log.
  expect(view.figures).toEqual({ tracked: 1276, logged: 855, held: 60, toLog: 254 });
  expect(view.rows.map((row) => row.total)).toEqual([465, 704, 107]);
  expect(view.dayTotals).toEqual([138, 162, 306, 436, 234, 0, 0]);
});

test("minutes are floored per cell, never rounded up, and the totals add what is drawn", () => {
  const odd: Week = {
    days: DAYS,
    rows: [
      {
        target: { kind: "label", label: "Standup and planning" },
        title: null,
        cells: [
          { tracked_seconds: 119, offered_seconds: 0, logged_seconds: 0, held_seconds: 0, unlogged_seconds: 119 },
          { tracked_seconds: 119, offered_seconds: 0, logged_seconds: 0, held_seconds: 0, unlogged_seconds: 119 },
          ...EMPTY.slice(2),
        ],
      },
    ],
    past_horizon: DAYS.map(() => false),
  };
  const view = weekView(odd, null, FRIDAY_NOON);
  expect(view.rows[0]!.cells[0]!.tracked).toBe(1);
  expect(view.rows[0]!.total).toBe(2);
  expect(view.figures.tracked).toBe(2);
});

test("the running timer counts into today's cell on its target, as tracked and to log", () => {
  const running: RunningTimer = {
    target: { kind: "entity", entity_id: "jira:PAY-231" },
    started_at: new Date(2026, 7, 21, 11, 26).toISOString(),
    last_heartbeat: FRIDAY_NOON.toISOString(),
  };
  const view = weekView(week(), running, FRIDAY_NOON);
  expect(view.rows[1]!.cells[4]).toMatchObject({ tracked: 198, toLog: 198, note: "3h 18m to log" });
  expect(view.figures.toLog).toBe(288);
});

test("a timer that started yesterday counts from today's midnight only", () => {
  const running: RunningTimer = {
    target: { kind: "entity", entity_id: "jira:PAY-231" },
    started_at: new Date(2026, 7, 20, 23, 0).toISOString(),
    last_heartbeat: FRIDAY_NOON.toISOString(),
  };
  const view = weekView(week(), running, FRIDAY_NOON);
  expect(view.rows[1]!.cells[3]!.tracked).toBe(270);
  expect(view.rows[1]!.cells[4]!.tracked).toBe(164 + 12 * 60);
});

test("a timer on a target the week has no row for gets one, before the no-target row", () => {
  const running: RunningTimer = {
    target: { kind: "label", label: "Incident review" },
    started_at: new Date(2026, 7, 21, 11, 0).toISOString(),
    last_heartbeat: FRIDAY_NOON.toISOString(),
  };
  const view = weekView(week(), running, FRIDAY_NOON);
  expect(view.rows.map((row) => row.target?.kind === "label" ? row.target.label : row.target?.kind ?? null)).toEqual([
    "entity",
    "entity",
    "Incident review",
    null,
  ]);
  expect(view.rows[2]!.cells[4]!.tracked).toBe(60);
});

test("a timer in another week is not drawn", () => {
  const running: RunningTimer = {
    target: { kind: "entity", entity_id: "jira:PAY-231" },
    started_at: new Date(2026, 7, 28, 9, 0).toISOString(),
    last_heartbeat: new Date(2026, 7, 28, 10, 0).toISOString(),
  };
  const view = weekView(week(), running, new Date(2026, 7, 28, 10, 0));
  expect(view.figures.tracked).toBe(1276);
});

function queued(over: Partial<QueuedWrite> & { started: string; seconds: number; entity?: string }): QueuedWrite {
  const entity = over.entity ?? "jira:PAY-228";
  return {
    id: 40,
    source_id: "jira",
    entity_id: entity,
    op: "log_work",
    payload: { LogWork: { entity, started: over.started, seconds: over.seconds, comment: "" } },
    target_snapshot: null,
    state: "held",
    wait_reason: null,
    detail: null,
    queued_at: over.started,
    attempted_at: null,
    attempts: 1,
    held_snapshot: null,
    settled_at: null,
    source_enabled: true,
    ...over,
  } as QueuedWrite;
}

test("a held worklog in the week says how much, of which ticket, on which day, and why", () => {
  const notes = heldNotes(
    [
      queued({
        state: "refused",
        detail: "Jira refused the worklog because the ticket moved to In Review with time tracking closed",
        started: new Date(2026, 7, 20, 14, 0).toISOString(),
        seconds: 3600,
      }),
    ],
    DAYS,
  );
  expect(notes).toEqual([
    "1h of PAY-228 on Thursday is held: Jira refused the worklog because the ticket moved to In Review with time tracking closed.",
  ]);
});

test("without the source's words, the reason is the queue's state", () => {
  const thursday = new Date(2026, 7, 20, 14, 0).toISOString();
  expect(heldNotes([queued({ started: thursday, seconds: 1800 })], DAYS)).toEqual([
    "30m of PAY-228 on Thursday is held: the ticket changed after you logged it.",
  ]);
  expect(heldNotes([queued({ started: thursday, seconds: 1800, source_enabled: false })], DAYS)).toEqual([
    "30m of PAY-228 on Thursday is held: its source is turned off.",
  ]);
  expect(heldNotes([queued({ started: thursday, seconds: 1800, state: "refused" })], DAYS)).toEqual([
    "30m of PAY-228 on Thursday is held: the source refused it.",
  ]);
  // Sent and never answered (#1125): not a ticket that changed, and it may be in Jira already (#1966).
  const unanswered = { op: "log_work", unconfirmed: { source: "Jira", detail: "the connection closed" } };
  expect(heldNotes([queued({ started: thursday, seconds: 1800, held_snapshot: unanswered })], DAYS)).toEqual([
    "30m of PAY-228 on Thursday is held: Jira did not answer, so it may have arrived. Check Jira, then send it again or discard it.",
  ]);
});

test("a pending worklog, another op, or another week's worklog says nothing here", () => {
  const thursday = new Date(2026, 7, 20, 14, 0).toISOString();
  const comment = queued({ started: thursday, seconds: 60 });
  expect(
    heldNotes(
      [
        queued({ started: thursday, seconds: 60, state: "pending" }),
        { ...comment, op: "comment", payload: { Comment: { entity: "jira:PAY-228", body: "x" } } },
        queued({ started: new Date(2026, 7, 26, 9, 0).toISOString(), seconds: 60 }),
      ],
      DAYS,
    ),
  ).toEqual([]);
});
