/**
 * The week timesheet: which columns are drawn, which one is highlighted, and
 * what *Log all* does before it does anything (issue #283).
 *
 * The seam is spec #272's — **a rendered view in, user-visible text and
 * addresses out**. The bridge is injected, so no Tauri and no database: what
 * the backend makes of these calls is `crates/knobas-app/tests/week_ipc.rs`'s.
 *
 * Fixtures use the **local** `Date` constructor, the discipline
 * `DayReview.test.svelte.ts` records: a week is a local thing and these tests
 * run in whatever timezone the machine is set to.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DayWindow, PlannedWorklog, Week, WeekCell, WeekRow, Worklog } from "../ipc/time";
import WeekTimesheet from "./WeekTimesheet.svelte";

/** The fixture week: Monday 24 August 2026 to Sunday the 30th. */
const DAYS = [
  "2026-08-24",
  "2026-08-25",
  "2026-08-26",
  "2026-08-27",
  "2026-08-28",
  "2026-08-29",
  "2026-08-30",
];

function cell(over: Partial<WeekCell> = {}): WeekCell {
  return {
    tracked_seconds: 0,
    logged_seconds: 0,
    held_seconds: 0,
    unlogged_seconds: 0,
    ...over,
  };
}

function row(over: Partial<WeekRow> = {}): WeekRow {
  return {
    target: { kind: "entity", entity_id: "jira:PAY-231" },
    title: "Retry failed SEPA payouts",
    cells: DAYS.map(() => cell()),
    ...over,
  };
}

function weekOf(rows: WeekRow[]): Week {
  return { days: DAYS, rows };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(week: Week, over: { planned?: PlannedWorklog[]; day?: string } = {}) {
  const asked: DayWindow[][] = [];
  const previews: DayWindow[][] = [];
  const logged: DayWindow[][] = [];

  app = mount(WeekTimesheet, {
    target,
    props: {
      day: over.day ?? "2026-08-26",
      ports: {
        weekTimesheet: (days: DayWindow[]) => {
          asked.push(days);
          return Promise.resolve(week);
        },
        logAllPreview: (days: DayWindow[]) => {
          previews.push(days);
          return Promise.resolve(over.planned ?? []);
        },
        logAll: (days: DayWindow[]) => {
          logged.push(days);
          return Promise.resolve([] as Worklog[]);
        },
      },
    },
  });
  flushSync();
  return { asked, previews, logged };
}

/** The column headings actually drawn, in order. */
function headings(): string[] {
  return [...target.querySelectorAll("thead th")]
    .map((th) => (th.textContent ?? "").trim())
    .slice(1);
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

function button(label: string): HTMLButtonElement | undefined {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find(
    (candidate) => candidate.textContent?.trim() === label,
  );
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("the week read is the seven local midnights of the strip's own week", async () => {
  const { asked } = render(weekOf([]));
  await vi.waitFor(() => expect(asked).toHaveLength(1));

  const days = asked[0]!;
  expect(days.map((window) => window.day)).toEqual(DAYS);
  expect(new Date(days[0]!.from)).toEqual(new Date(2026, 7, 24, 0, 0, 0, 0));
  expect(new Date(days[6]!.to)).toEqual(new Date(2026, 7, 31, 0, 0, 0, 0));
});

/**
 * **An empty weekend collapses; a worked one does not** — the acceptance
 * criterion, and both halves are here because either alone is satisfied by a
 * view that always collapses or never does.
 */
test("an empty weekend is not drawn and a worked Saturday is", async () => {
  render(
    weekOf([
      row({
        cells: DAYS.map((_, index) => (index === 0 ? cell({ tracked_seconds: 3600 }) : cell())),
      }),
    ]),
  );
  await vi.waitFor(() => expect(headings()).toHaveLength(5));
  expect(headings()).toEqual(["Mon 24", "Tue 25", "Wed 26", "Thu 27", "Fri 28"]);

  unmount(app!);
  app = undefined;
  target.remove();
  target = document.createElement("div");
  document.body.append(target);

  render(
    weekOf([
      row({
        cells: DAYS.map((_, index) => (index === 5 ? cell({ tracked_seconds: 1800 }) : cell())),
      }),
    ]),
  );
  await vi.waitFor(() => expect(headings()).toHaveLength(6));
  expect(headings()).toEqual(["Mon 24", "Tue 25", "Wed 26", "Thu 27", "Fri 28", "Sat 29"]);
});

/** The strip's date is what the highlighted column is. */
test("the strip's day is the highlighted column", async () => {
  render(weekOf([row({ cells: DAYS.map(() => cell({ tracked_seconds: 3600 })) })]), {
    day: "2026-08-27",
  });
  await vi.waitFor(() => expect(headings()).toHaveLength(7));

  const marked = [...target.querySelectorAll("thead th.on")].map((th) =>
    (th.textContent ?? "").trim(),
  );
  expect(marked).toEqual(["Thu 27"]);
});

/**
 * The three readings a cell can carry, in the words the criterion names them
 * with — and *held* said as held rather than folded into either neighbour
 * (spec story 39).
 */
test("a cell reads tracked, logged, held and unlogged in minutes", async () => {
  render(
    weekOf([
      row({
        cells: DAYS.map((_, index) =>
          index === 0
            ? cell({
                tracked_seconds: 5_400,
                logged_seconds: 1_800,
                held_seconds: 1_800,
                unlogged_seconds: 1_800,
              })
            : cell(),
        ),
      }),
    ]),
  );
  await vi.waitFor(() => expect(text()).toContain("1 h 30 min"));

  expect(text()).toContain("30 min logged");
  expect(text()).toContain("30 min held");
  expect(text()).toContain("30 min unlogged");
});

/**
 * Minutes, floored. 89 minutes and 59 seconds is `1 h 29 min` and never
 * `1 h 30 min`: `CONTEXT.md`'s timesheet has no rounding, because knobas must
 * not invent a policy a person's Jira may not have.
 */
test("a cell never rounds a second up into a minute", async () => {
  render(
    weekOf([
      row({
        cells: DAYS.map((_, index) =>
          index === 0 ? cell({ tracked_seconds: 89 * 60 + 59 }) : cell(),
        ),
      }),
    ]),
  );
  await vi.waitFor(() => expect(text()).toContain("1 h 29 min"));
  expect(text()).not.toContain("1 h 30 min");
});

/** The "no target, app open" row says what it is and offers nothing. */
test("the no-target row names itself as focused time no block covers", async () => {
  render(
    weekOf([
      row({
        target: null,
        title: null,
        cells: DAYS.map((_, index) =>
          index === 0 ? cell({ tracked_seconds: 600, unlogged_seconds: 600 }) : cell(),
        ),
      }),
    ]),
  );
  await vi.waitFor(() => expect(text()).toContain("No target, app open"));
  expect(text()).toContain("focused time no block covers");
});

/**
 * **The confirmation lists what it will send, and nothing goes until it is
 * confirmed** — the acceptance criterion's own wording.
 *
 * Two days and one ticket in the plan, so the list a reader reads is the thing
 * story 43 promises: a week logs per day rather than as one lump.
 */
test("Log all lists what it will send and sends nothing until it is confirmed", async () => {
  const planned: PlannedWorklog[] = [
    {
      day: "2026-08-24",
      entity_id: "jira:PAY-231",
      title: "Retry failed SEPA payouts",
      started_at: new Date(2026, 7, 24, 9).toISOString(),
      seconds: 3_600,
      blocks: 1,
    },
    {
      day: "2026-08-25",
      entity_id: "jira:PAY-231",
      title: "Retry failed SEPA payouts",
      started_at: new Date(2026, 7, 25, 9).toISOString(),
      seconds: 7_200,
      blocks: 2,
    },
  ];
  const { previews, logged } = render(weekOf([row()]), { planned });
  await vi.waitFor(() => expect(button("Log all…")).toBeDefined());

  button("Log all…")!.click();
  await vi.waitFor(() => expect(previews).toHaveLength(1));
  flushSync();

  expect(text()).toContain("This will send 2 worklogs");
  expect(text()).toContain("2026-08-24");
  expect(text()).toContain("2026-08-25");
  expect(text()).toContain("2 h");
  expect(logged).toHaveLength(0);

  button("Send 2")!.click();
  await vi.waitFor(() => expect(logged).toHaveLength(1));
  expect(logged[0]!.map((window) => window.day)).toEqual(DAYS);
});

/** Cancelling sends nothing, and says so by leaving the list. */
test("cancelling the confirmation sends nothing", async () => {
  const { logged } = render(weekOf([row()]), {
    planned: [
      {
        day: "2026-08-24",
        entity_id: "jira:PAY-231",
        title: null,
        started_at: new Date(2026, 7, 24, 9).toISOString(),
        seconds: 3_600,
        blocks: 1,
      },
    ],
  });
  await vi.waitFor(() => expect(button("Log all…")).toBeDefined());
  button("Log all…")!.click();
  await vi.waitFor(() => expect(button("Send 1")).toBeDefined());

  button("Cancel")!.click();
  flushSync();

  expect(logged).toHaveLength(0);
  expect(text()).not.toContain("This will send");
});

/** Nothing to log is a sentence, not a button that appears to do nothing. */
test("a week with nothing to log says so rather than offering a send", async () => {
  render(weekOf([row()]), { planned: [] });
  await vi.waitFor(() => expect(button("Log all…")).toBeDefined());
  button("Log all…")!.click();
  await vi.waitFor(() => expect(text()).toContain("Nothing to log this week"));
  expect(button("Send 0")).toBeUndefined();
});
