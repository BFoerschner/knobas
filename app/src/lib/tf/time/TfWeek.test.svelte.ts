/**
 * The Time view's Week tab (#828, R-13.4): what a reader sees of the week.
 * The bridge is injected; what the backend adds up is
 * `crates/knobas-app/tests/it/week_ipc.rs`'s.
 *
 * Instants are local, for `day.test.ts`'s reason.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { QueuedWrite } from "../../ipc/sources";
import type { DayWindow, RunningTimer, Week, WeekCell } from "../../ipc/time";
import TfWeek from "./TfWeek.svelte";

const NOW = new Date(2026, 7, 21, 12, 0);

function cell(tracked: number, logged = 0, held = 0): WeekCell {
  return {
    tracked_seconds: tracked * 60,
    offered_seconds: 0,
    logged_seconds: logged * 60,
    held_seconds: held * 60,
    unlogged_seconds: Math.max(0, tracked - logged - held) * 60,
  };
}

function answer(days: DayWindow[]): Week {
  const zero = [cell(0), cell(0)];
  return {
    days: days.map((window) => window.day),
    rows: [
      {
        target: { kind: "entity", entity_id: "jira:PAY-228" },
        title: "Fix ledger drift on partial refunds",
        cells: [cell(120, 120), cell(60, 60), cell(90, 90), cell(160, 100, 60), cell(35, 35), ...zero],
      },
      {
        target: { kind: "label", label: "Standup and planning" },
        title: null,
        cells: [cell(30, 30), cell(30, 30), cell(30, 30), cell(30, 30), cell(48), ...zero],
      },
      { target: null, title: null, cells: [cell(18), cell(12), cell(36), cell(6), cell(35), ...zero] },
    ],
    past_horizon: days.map(() => false),
  };
}

const HELD: QueuedWrite = {
  id: 40,
  source_id: "jira",
  entity_id: "jira:PAY-228",
  op: "log_work",
  payload: { LogWork: { entity: "jira:PAY-228", started: new Date(2026, 7, 20, 14, 0).toISOString(), seconds: 3600, comment: "" } },
  target_snapshot: null,
  state: "refused",
  wait_reason: null,
  detail: "Jira refused the worklog because the ticket moved to In Review with time tracking closed",
  queued_at: new Date(2026, 7, 20, 15, 0).toISOString(),
  attempted_at: null,
  attempts: 1,
  held_snapshot: null,
  settled_at: null,
  source_enabled: true,
};

let target: HTMLDivElement;
let view: Record<string, unknown> | undefined;
beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});
afterEach(() => {
  if (view) unmount(view);
  view = undefined;
  target.remove();
});

async function render(over: { week?: string | null; running?: RunningTimer | null; writes?: QueuedWrite[] } = {}) {
  const asked: DayWindow[][] = [];
  let queues = 0;
  view = mount(TfWeek, {
    target,
    props: {
      week: over.week ?? null,
      timer: { current: over.running ?? null },
      now: () => NOW,
      onqueue: () => void (queues += 1),
      ports: {
        weekTimesheet: async (days: DayWindow[]) => {
          asked.push(days);
          return answer(days);
        },
        pendingWrites: async () => over.writes ?? [],
      },
    },
  });
  for (let i = 0; i < 4; i++) await tick();
  flushSync();
  return { asked, queues: () => queues };
}

const text = (el: Element | null | undefined) => (el?.textContent ?? "").replace(/\s+/g, " ").trim();

test("reads this week, Monday first, and names it", async () => {
  const { asked } = await render();
  expect(asked[0]!.map((w) => w.day)).toEqual([
    "2026-08-17",
    "2026-08-18",
    "2026-08-19",
    "2026-08-20",
    "2026-08-21",
    "2026-08-22",
    "2026-08-23",
  ]);
  expect(text(target.querySelector("h2"))).toBe("Week of 17 August 2026");
  const heads = [...target.querySelectorAll("thead th")].map(text);
  expect(heads).toEqual(["On", "Mon 17", "Tue 18", "Wed 19", "Thu 20", "Fri 21", "Sat 22", "Sun 23", "Week"]);
});

test("the address's date reads that date's week, and the week after is a step forward", async () => {
  const { asked } = await render({ week: "2026-08-12" });
  expect(asked[0]![0]!.day).toBe("2026-08-10");
  expect(target.querySelector('a[aria-label="Week before"]')?.getAttribute("href")).toBe("#/time/week/2026-08-03");
  expect(target.querySelector('a[aria-label="Week after"]')?.getAttribute("href")).toBe("#/time/week");
});

test("this week has no week after", async () => {
  await render();
  expect(target.querySelector('a[aria-label="Week after"]')).toBeNull();
});

test("the four figures: tracked, logged, to log and held", async () => {
  await render();
  const figures = [...target.querySelectorAll(".tf-tm-sum > div")].map(
    (div) => `${text(div.querySelector("b"))} ${text(div.querySelector("span"))}`,
  );
  // tracked 465 + 168 + 107; logged 405 + 120; to log 48; held 60.
  expect(figures).toEqual(["12h 20m tracked", "8h 45m logged", "48m to log", "1h held"]);
});

test("a cell links to its day and says what is left; weekend columns are shaded", async () => {
  await render();
  const row = [...target.querySelectorAll("tbody tr")].find((tr) => text(tr).includes("PAY-228"))!;
  const cells = [...row.querySelectorAll("td")];
  expect(cells[4]!.querySelector("a")?.getAttribute("href")).toBe("#/time/2026-08-20");
  expect(text(cells[4])).toBe("2h 40m 1h held");
  expect(text(cells[6])).toBe("–");
  expect(cells[6]!.classList.contains("tf-tm-we")).toBe(true);
  expect(cells[1]!.classList.contains("tf-tm-we")).toBe(false);
  expect(text(cells[8])).toBe("7h 45m");
  expect(row.querySelector('a[href="#/job/jira:PAY-228"]')).not.toBeNull();
});

test("the no-target row says what it is, and has no bar", async () => {
  await render();
  const row = [...target.querySelectorAll("tbody tr")].find((tr) => text(tr).includes("No target, app open"))!;
  expect(text(row)).toContain("Focused time no block covers. Assign it in the day.");
  expect(row.querySelector("svg")).toBeNull();
});

test("the total row adds every row per day", async () => {
  await render();
  const total = [...target.querySelectorAll("tbody tr")].at(-1)!;
  expect([...total.querySelectorAll("td")].map(text)).toEqual([
    "Every day",
    "2h 48m",
    "1h 42m",
    "2h 36m",
    "3h 16m",
    "1h 58m",
    "–",
    "–",
    "12h 20m",
  ]);
});

test("a held worklog shows its reason, and Open the queue opens it", async () => {
  const { queues } = await render({ writes: [HELD] });
  const note = target.querySelector(".tf-tm-held");
  expect(text(note)).toBe(
    "1h of PAY-228 on Thursday is held: Jira refused the worklog because the ticket moved to In Review with time tracking closed. Open the queue",
  );
  note!.querySelector("button")!.click();
  expect(queues()).toBe(1);
});

test("no held worklog, no note", async () => {
  await render();
  expect(target.querySelector(".tf-tm-held")).toBeNull();
});

test("the running timer counts into today on its target", async () => {
  await render({
    running: {
      target: { kind: "label", label: "Standup and planning" },
      started_at: new Date(2026, 7, 21, 11, 0).toISOString(),
      last_heartbeat: NOW.toISOString(),
    },
  });
  const row = [...target.querySelectorAll("tbody tr")].find((tr) => text(tr).includes("Standup and planning"))!;
  expect(text(row.querySelectorAll("td")[5])).toBe("1h 48m 1h 48m to log");
});
