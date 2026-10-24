/**
 * *Log all…* (#828, R-13.4, R-13.5): the preview, the ticks and comments, and
 * what is sent. The bridge is injected; what the backend logs is
 * `crates/knobas-app/tests/it/worklog_ipc.rs`'s.
 *
 * Instants are local, for `day.test.ts`'s reason.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { DayWindow, Draft, LoggedWork, PlannedWorklog, ReaderDay, RunningTimer, Worklog } from "../../ipc/time";
import { weekWindows } from "../../time/week";
import { createLayers } from "../layers.svelte";
import TfLogAll from "./TfLogAll.svelte";
import type { RefusedWritesPort, WriteAsk } from "../../shell/refused-writes";

const DAYS = weekWindows("2026-08-17");

function planned(day: string, entity: string, minutes: number, hour = 9): PlannedWorklog {
  return {
    day,
    entity_id: entity,
    title: null,
    started_at: new Date(`${day}T${String(hour).padStart(2, "0")}:00:00`).toISOString(),
    seconds: minutes * 60,
    blocks: 1,
  };
}

const PLAN = [
  planned("2026-08-20", "jira:PAY-231", 90),
  planned("2026-08-21", "jira:PAY-231", 164),
  planned("2026-08-21", "jira:PAY-200", 48, 13),
];

const COMMENTS: Record<string, string> = {
  "2026-08-20 jira:PAY-231": "Retry classification for TEMP_UNAVAILABLE; review on PR #142.",
  "2026-08-21 jira:PAY-231": "Backoff jitter (a41f2c, c90d11); replied to review on PR #142.",
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

async function render(
  over: {
    plan?: PlannedWorklog[];
    /** What the second preview, at send time, answers. */
    fresh?: PlannedWorklog[];
    running?: RunningTimer | null;
    refuse?: string;
    refusedWrites?: RefusedWritesPort;
  } = {},
) {
  const drafts: [string, ReaderDay][] = [];
  const logged: [string, ReaderDay, LoggedWork][] = [];
  const done: { made: Worklog[]; failure: string | null }[] = [];
  let previews = 0;
  let closed = 0;
  view = mount(TfLogAll, {
    target,
    props: {
      days: DAYS,
      timer: { current: over.running ?? null },
      layers: createLayers(),
      onclose: () => void (closed += 1),
      ondone: (made: Worklog[], failure: string | null) => void done.push({ made, failure }),
      ports: {
        logAllPreview: async (days: DayWindow[]) => {
          expect(days).toEqual(DAYS);
          previews += 1;
          return previews === 1 ? (over.plan ?? PLAN) : (over.fresh ?? over.plan ?? PLAN);
        },
        worklogDraft: async (entityId: string, when: ReaderDay) => {
          drafts.push([entityId, when]);
          const comment = COMMENTS[`${when.day} ${entityId}`];
          return comment === undefined ? null : ({ comment } as Draft);
        },
        logWork: async (entityId: string, when: ReaderDay, work: LoggedWork) => {
          if (over.refuse === entityId) throw { code: "invalid", message: "PAY-200 takes no worklogs.", source_id: null };
          logged.push([entityId, when, work]);
          return { id: logged.length, entity_id: entityId, write_queue_id: null } as Worklog;
        },
        listSources: async () => [{ id: "jira", display_name: "Jira" }],
        ...(over.refusedWrites ? { refusedWrites: over.refusedWrites } : {}),
      },
    },
  });
  await settle();
  return { drafts, logged, done, closed: () => closed };
}

async function settle() {
  for (let i = 0; i < 6; i++) await tick();
  flushSync();
}

const text = (el: Element | null | undefined) => (el?.textContent ?? "").replace(/\s+/g, " ").trim();
const rows = () => [...target.querySelectorAll("tbody tr")];
const send = () => [...target.querySelectorAll<HTMLButtonElement>("footer button")].find((b) => text(b).startsWith("Log"))!;

test("one ticked row per day and ticket, with the draft's comment to edit", async () => {
  const { drafts } = await render();
  expect(text(target.querySelector("h2"))).toBe("Log all");
  expect(rows().map((tr) => [...tr.querySelectorAll("td")].slice(1, 4).map(text))).toEqual([
    ["Thu 20", "PAY-231", "1h 30m"],
    ["Fri 21", "PAY-231", "2h 44m"],
    ["Fri 21", "PAY-200", "48m"],
  ]);
  const boxes = rows().map((tr) => tr.querySelector<HTMLInputElement>('input[type="checkbox"]')!);
  expect(boxes.every((box) => box.checked)).toBe(true);
  expect(boxes[0]!.getAttribute("aria-label")).toBe("Log this");
  expect(rows().map((tr) => tr.querySelector<HTMLInputElement>('textarea')!.value)).toEqual([
    COMMENTS["2026-08-20 jira:PAY-231"],
    COMMENTS["2026-08-21 jira:PAY-231"],
    "",
  ]);
  expect(drafts[0]![1].day).toBe("2026-08-20");
  expect(text(send())).toBe("Log 3 worklogs");
});

test("asks for each draft with the offset at that day's midnight, in November too", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  try {
    vi.setSystemTime(new Date(2026, 10, 5, 12));
    const { drafts } = await render();
    expect(drafts.map(([, when]) => when.offsetMinutes)).toEqual([120, 120, 120]);
  } finally {
    vi.useRealTimers();
  }
});

test("says it leaves out no-target time, and the running timer while it runs", async () => {
  await render({
    running: {
      target: { kind: "entity", entity_id: "jira:PAY-231" },
      started_at: new Date(2026, 7, 21, 13, 58).toISOString(),
      last_heartbeat: new Date(2026, 7, 21, 14, 40).toISOString(),
    },
  });
  const said = text(target.querySelector(".tf-body"));
  expect(said).toContain("The running timer on PAY-231 is not in it until you stop it.");
  expect(said).toContain('Time on "No target, app open" is not offered. Assign it to a ticket in the day first.');
});

test("with no timer running, nothing is said about one", async () => {
  await render();
  expect(text(target.querySelector(".tf-body"))).not.toContain("running timer");
});

test("logs only the ticked rows, with their edited comments, at the time the week has when sent", async () => {
  const { logged, done } = await render({
    fresh: [planned("2026-08-20", "jira:PAY-231", 90), planned("2026-08-21", "jira:PAY-231", 170), PLAN[2]!],
  });
  const third = rows()[2]!.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
  third.click();
  const comment = rows()[1]!.querySelector<HTMLInputElement>('textarea')!;
  comment.value = "Backoff jitter.";
  comment.dispatchEvent(new Event("input"));
  flushSync();
  expect(text(send())).toBe("Log 2 worklogs");
  send().click();
  await settle();
  expect(logged.map(([entity, when, work]) => [entity, when.day, work.seconds, work.comment])).toEqual([
    ["jira:PAY-231", "2026-08-20", 90 * 60, COMMENTS["2026-08-20 jira:PAY-231"]],
    ["jira:PAY-231", "2026-08-21", 170 * 60, "Backoff jitter."],
  ]);
  expect(logged[1]![2].startedAt).toBe(PLAN[1]!.started_at);
  expect(done).toEqual([{ made: [expect.anything(), expect.anything()], failure: null }]);
});

test("a day with 163 s unlogged sends 120 s, and a day under a minute sends nothing", async () => {
  const odd = { ...planned("2026-08-20", "jira:PAY-231", 0), seconds: 163 };
  const tiny = { ...planned("2026-08-21", "jira:PAY-231", 0), seconds: 59 };
  const { logged } = await render({ fresh: [odd, tiny] });
  send().click();
  await settle();
  expect(logged.map(([, when, work]) => [when.day, work.seconds])).toEqual([["2026-08-20", 120]]);
});

test("a row the week no longer offers when sent is skipped, not logged twice", async () => {
  const { logged } = await render({ fresh: [PLAN[0]!, PLAN[2]!] });
  send().click();
  await settle();
  expect(logged.map(([entity, when]) => `${when.day} ${entity}`)).toEqual([
    "2026-08-20 jira:PAY-231",
    "2026-08-21 jira:PAY-200",
  ]);
});

test("a refused worklog does not stop the rest, and is said", async () => {
  const { logged, done } = await render({ refuse: "jira:PAY-200" });
  send().click();
  await settle();
  expect(logged).toHaveLength(2);
  expect(done).toEqual([{ made: [expect.anything(), expect.anything()], failure: "PAY-200 takes no worklogs." }]);
});

test("nothing ticked, nothing to send", async () => {
  await render();
  for (const tr of rows()) tr.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
  flushSync();
  expect(send().disabled).toBe(true);
});

test("a week with nothing to log says so", async () => {
  await render({ plan: [] });
  expect(rows()).toHaveLength(0);
  expect(text(target.querySelector(".tf-body"))).toContain("Nothing to log this week.");
  expect(send()).toBeUndefined();
});

test("Cancel closes without sending", async () => {
  const { logged, closed } = await render();
  [...target.querySelectorAll<HTMLButtonElement>("footer button")].find((b) => text(b) === "Cancel")!.click();
  expect(closed()).toBe(1);
  expect(logged).toHaveLength(0);
});

test("a row whose source's credential cannot log work is drawn unticked and disabled saying why, and is not sent (#2052)", async () => {
  const asked: WriteAsk[][] = [];
  const reason = "This Jira user cannot log work in PAY: it lacks the Work On Issues permission";
  const { logged } = await render({
    refusedWrites: (asks) => {
      asked.push(asks);
      return Promise.resolve([{ entity: "jira:PAY-200", op: "log_work", reason }]);
    },
  });
  await settle();
  // Each ticket once, whatever the days it is on.
  expect(asked).toEqual([
    [
      { entity: "jira:PAY-231", op: "log_work" },
      { entity: "jira:PAY-200", op: "log_work" },
    ],
  ]);
  const box = rows()[2]!.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
  expect(box.checked).toBe(false);
  expect(box.disabled).toBe(true);
  expect(box.title).toBe(reason);
  expect(text(rows()[2])).toContain(reason);
  expect(text(send())).toBe("Log 2 worklogs");
  send().click();
  await settle();
  expect(logged.map(([entity]) => entity)).toEqual(["jira:PAY-231", "jira:PAY-231"]);
});
