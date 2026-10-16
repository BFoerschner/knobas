/**
 * The Time view's Day tab (#827): what a reader sees, and what each action
 * sends. The bridge is injected; what the backend does with the calls is
 * `crates/knobas-app/tests/it/time_ipc.rs`'s.
 *
 * Instants are local, for `day.test.ts`'s reason.
 */
import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { AdHocBlock, Block, DayBlock, OpenStretch, RunningTimer, TimerTarget } from "../../ipc/time";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import TfDay from "./TfDay.svelte";

const DAY = "2026-08-22";
const NOW = new Date(2026, 7, 22, 14, 42, 0, 0);

function at(hour: number, minute = 0, day = 22): string {
  return new Date(2026, 7, day, hour, minute, 0, 0).toISOString();
}

function block(id: number, from: string, to: string, over: Partial<Block> = {}, title: string | null = null): DayBlock {
  return {
    block: {
      id,
      started_at: from,
      ended_at: to,
      target: { kind: "entity", entity_id: "jira:PAY-231" },
      kind: "manual",
      ended_by_relaunch: false,
      worklog_id: null,
      ...over,
    },
    title,
  };
}

/** prototype-6's Friday, less the running timer. */
function friday(): DayBlock[] {
  return [
    block(1, at(8, 50), at(9, 25), { worklog_id: 7, target: { kind: "entity", entity_id: "jira:PAY-228" } }),
    block(2, at(9, 25), at(9, 40), { kind: "passive", target: { kind: "entity", entity_id: "gitea:payments#139" } }),
    block(3, at(9, 40), at(11, 50), {}, "Retry failed SEPA payouts"),
    block(5, at(13, 10), at(13, 58), { target: { kind: "label", label: "Standup and planning" } }),
  ];
}

const OFFERS: Record<number, AdHocBlock | null> = {
  2: { suggestion: { entity_id: "jira:PAY-228", title: null, rule: "linked_to_target" } },
  3: null,
  5: { suggestion: { entity_id: "jira:PAY-200", title: null, rule: "last_logged" } },
};

const RUNNING: RunningTimer = {
  target: { kind: "entity", entity_id: "jira:PAY-231" },
  started_at: at(13, 58),
  last_heartbeat: at(14, 40),
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

function render(
  over: {
    blocks?: DayBlock[];
    running?: RunningTimer | null;
    pastHorizon?: boolean;
    open?: OpenStretch[];
    day?: string | null;
  } = {},
) {
  let listed = over.blocks ?? friday();
  const open = over.open ?? [{ started_at: at(12, 35), ended_at: at(13, 10) }];
  const updates: [number, string, string, TimerTarget][] = [];
  const creates: [string, string, TimerTarget][] = [];
  const deletes: number[] = [];
  const dismissals: number[] = [];
  const logs: [string, string][] = [];
  let stops = 0;
  const layers = createLayers();
  view = mount(TfDay, {
    target,
    props: {
      day: over.day === undefined ? DAY : over.day,
      timer: { current: over.running ?? null },
      ontimer: () => (stops += 1),
      onlog: (entityId: string, day: string) => logs.push([entityId, day]),
      now: () => NOW,
      layers,
      ports: {
        dayBlocks: () => Promise.resolve({ blocks: listed, past_horizon: over.pastHorizon ?? false, open }),
        adHocBlock: (id: number) => Promise.resolve(OFFERS[id] ?? null),
        updateBlock: (id: number, from: string, to: string, on: TimerTarget) => {
          updates.push([id, from, to, on]);
          const entry = listed.find((candidate) => candidate.block.id === id)!;
          const edited = { ...entry, block: { ...entry.block, started_at: from, ended_at: to, target: on, kind: "manual" as const } };
          listed = listed.map((candidate) => (candidate.block.id === id ? edited : candidate));
          return Promise.resolve(edited);
        },
        createBlock: (from: string, to: string, on: TimerTarget) => {
          creates.push([from, to, on]);
          const made = block(90, from, to, { target: on });
          listed = [...listed, made];
          return Promise.resolve(made);
        },
        deleteBlock: (id: number) => {
          deletes.push(id);
          listed = listed.filter((candidate) => candidate.block.id !== id);
          return Promise.resolve();
        },
        dismissBlock: (id: number) => {
          dismissals.push(id);
          listed = listed.filter((candidate) => candidate.block.id !== id);
          return Promise.resolve();
        },
      },
    },
  });
  flushSync();
  return { updates, creates, deletes, dismissals, logs, layers, stops: () => stops };
}

async function settle() {
  for (let round = 0; round < 6; round += 1) {
    await tick();
    await Promise.resolve();
  }
  flushSync();
}

const squash = (value: string | null | undefined) => (value ?? "").replace(/\s+/g, " ").trim();

function figures(): string[] {
  return [...target.querySelectorAll(".tf-tm-sum > div")].map((cell) =>
    [...cell.children].map((part) => squash(part.textContent)).join(" "),
  );
}

function rows(): string[] {
  return [...target.querySelectorAll(".tf-tm-row")].map((row) => row.getAttribute("data-state") ?? "");
}

function row(state: string): HTMLElement {
  return target.querySelector<HTMLElement>(`.tf-tm-row[data-state="${state}"]`)!;
}

function button(within: ParentNode, label: string): HTMLButtonElement {
  const found = [...within.querySelectorAll<HTMLButtonElement>("button")].find(
    (candidate) => squash(candidate.textContent) === label,
  );
  if (!found) throw new Error(`no button "${label}"`);
  return found;
}

test("the day draws its five figures and a row per block, in time order, with its state", async () => {
  render({ running: RUNNING });
  await settle();
  expect(figures()).toEqual([
    "4h 17m tracked",
    "35m logged",
    "2h 10m to log",
    "15m offered by knobas",
    "35m open, app in front",
  ]);
  expect(rows()).toEqual(["logged", "offered", "unlogged", "open", "label", "running"]);
  const when = (state: string) =>
    [...row(state).querySelector(".tf-tm-when")!.childNodes].map((part) => squash(part.textContent)).join(" ");
  expect(when("unlogged")).toBe("09:40 to 11:50 2h 10m");
  expect(squash(row("unlogged").textContent)).toContain("PAY-231 Retry failed SEPA payouts");
  expect(when("running")).toBe("13:58 to now 44m");
  expect(squash(row("offered").textContent)).toContain("knobas saw payments#139");
  expect(squash(row("offered").textContent)).toContain("It is linked to PAY-228.");
});

test("yellow is on the running timer's block and on nothing else", async () => {
  render({ running: RUNNING });
  await settle();
  const yellow = [...target.querySelectorAll(".tf-tm-strip .tf-tm-run")];
  expect(yellow).toHaveLength(1);
  expect(yellow[0]!.getAttribute("aria-label")).toBe("13:58 to now, PAY-231");

  if (view) unmount(view);
  target.replaceChildren();
  render({ running: null });
  await settle();
  expect(target.querySelectorAll(".tf-tm-strip .tf-tm-run")).toHaveLength(0);
});

test("the strip places a block by its share of 08:00 to 18:00", async () => {
  render();
  await settle();
  const unlogged = target.querySelector(".tf-tm-strip .tf-tm-manual")!;
  // 09:40 is 100 of 600 minutes in, and 2h 10m is 130 of them.
  expect([unlogged.getAttribute("x"), unlogged.getAttribute("width")]).toEqual(["16.67%", "21.67%"]);
  // The now line, at 14:42.
  expect(target.querySelector(".tf-tm-now")!.getAttribute("x")).toBe("67%");
});

test("Add to <ticket> gives an offered block to the ticket it is linked to", async () => {
  const sent = render();
  await settle();
  button(row("offered"), "Add to PAY-228").click();
  await settle();
  expect(sent.updates).toEqual([[2, at(9, 25), at(9, 40), { kind: "entity", entity_id: "jira:PAY-228" }]]);
});

test("Log to <ticket> gives a label block the ticket ad_hoc_block suggests", async () => {
  const sent = render();
  await settle();
  button(row("label"), "Log to PAY-200").click();
  await settle();
  expect(sent.updates).toEqual([[5, at(13, 10), at(13, 58), { kind: "entity", entity_id: "jira:PAY-200" }]]);
});

test("Log… opens the draft for that ticket and day; Stop and log… is the timer verb", async () => {
  const sent = render({ running: RUNNING });
  await settle();
  button(row("unlogged"), "Log…").click();
  button(row("running"), "Stop and log…").click();
  expect(sent.logs).toEqual([["jira:PAY-231", DAY]]);
  expect(sent.stops()).toBe(1);
});

test("Edit moves a block and Delete block removes it, and Esc closes the dialog first", async () => {
  const sent = render();
  await settle();
  button(row("unlogged"), "Edit").click();
  flushSync();
  expect(sent.layers.open).toHaveLength(1);
  const from = target.querySelector<HTMLInputElement>("#tf-tm-from")!;
  from.value = "09:30";
  from.dispatchEvent(new Event("input", { bubbles: true }));
  button(target, "Save").click();
  await settle();
  expect(sent.updates).toEqual([[3, at(9, 30), at(11, 50), { kind: "entity", entity_id: "jira:PAY-231" }]]);
  expect(sent.layers.open).toHaveLength(0);

  button(row("unlogged"), "Edit").click();
  flushSync();
  button(target, "Delete block").click();
  await settle();
  expect(sent.deletes).toEqual([3]);
});

test("an open stretch is a dashed open row, and the open figure is the sum of the open rows", async () => {
  render({
    open: [
      { started_at: at(12, 10), ended_at: at(12, 20) },
      { started_at: at(12, 35), ended_at: at(13, 10) },
    ],
  });
  await settle();
  expect(rows()).toEqual(["logged", "offered", "unlogged", "open", "open", "label"]);
  expect(figures()[4]).toBe("45m open, app in front");
  const what = squash(row("open").textContent);
  expect(what).toContain("No target, app open");
  expect(what).toContain("knobas was open and in front, with no timer and nothing it could name.");
  expect(squash(row("open").querySelector(".tf-chip")?.textContent)).toBe("open");
  // Dashed on the strip, and the legend says what dashed means.
  expect(target.querySelectorAll(".tf-tm-strip .tf-tm-gap")).toHaveLength(2);
  expect(squash(target.querySelector(".tf-tm-legend")?.textContent)).toContain("Open: app in front, no target");
  // 11:50 to 12:10 is neither a block nor focused time, and is no row.
  expect(squash(target.textContent)).not.toContain("Nothing tracked");
});

test("Assign… on an open stretch writes a block over exactly that stretch", async () => {
  const sent = render();
  await settle();
  button(row("open"), "Assign…").click();
  flushSync();
  const on = target.querySelector<HTMLInputElement>("#tf-tm-on")!;
  on.value = "jira:OPS-77";
  on.dispatchEvent(new Event("input", { bubbles: true }));
  button(target, "Assign").click();
  await settle();
  expect(sent.creates).toEqual([[at(12, 35), at(13, 10), { kind: "entity", entity_id: "jira:OPS-77" }]]);
});

test("Dismiss on an offered block dismisses it, says knobas will not offer it again, and the day re-reads", async () => {
  const sent = render();
  await settle();
  expect(button(row("offered"), "Dismiss").className).toContain("tf-ghost");
  button(row("offered"), "Dismiss").click();
  await settle();
  expect(sent.dismissals).toEqual([2]);
  expect(sent.deletes).toEqual([]);
  expect(toasts.items.at(-1)?.text).toBe("Dismissed. knobas will not offer it again.");
  expect(rows()).not.toContain("offered");
  // Only an offer has it.
  expect(() => button(row("unlogged"), "Dismiss")).toThrow();
});

test("a day past the observation horizon with nothing tracked says No record", async () => {
  render({ blocks: [], pastHorizon: true, open: [], day: "2026-07-01" });
  await settle();
  expect(squash(target.textContent)).toContain("No record for this day.");
  expect(target.querySelector(".tf-tm-sum")).toBeNull();
});

test("a day with nothing on it says so, and a day before today offers the day after and Today", async () => {
  render({ blocks: [], open: [], day: "2026-08-20" });
  await settle();
  expect(squash(target.textContent)).toContain("Nothing tracked on this day.");
  expect(target.querySelector('a[aria-label="Day after"]')!.getAttribute("href")).toBe("#/time/2026-08-21");
  expect(squash(target.querySelector('a[href="#/time"]')?.textContent)).toBe("Today");
});

test("past the horizon, what knobas saw is absent rather than zero, and the blocks you tracked stay", async () => {
  render({ blocks: [block(3, at(9, 40, 1), at(11, 50, 1))], pastHorizon: true, open: [], day: "2026-08-01" });
  await settle();
  expect(figures()).toEqual(["2h 10m tracked", "0m logged", "2h 10m to log", "– offered by knobas", "– open, app in front"]);
  expect(squash(target.textContent)).toContain("No record of what was in front of you.");
  expect(rows()).toEqual(["unlogged"]);
});
