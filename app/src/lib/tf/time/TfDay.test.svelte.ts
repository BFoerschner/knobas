/**
 * The Time view's Day tab (#827): what a reader sees, and what each action
 * sends. The bridge is injected; what the backend does with the calls is
 * `crates/knobas-app/tests/it/time_ipc.rs`'s.
 *
 * Instants are local, for `day.test.ts`'s reason.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EntityRow } from "../../ipc/entity";
import type { SearchHit, SearchQuery, SearchResponse } from "../../ipc/search";
import type { AdHocBlock, Block, DayBlock, OpenStretch, RunningTimer, TimerTarget } from "../../ipc/time";
import { toasts } from "../../shell/toasts.svelte";
import { createLayers } from "../layers.svelte";
import TfDay from "./TfDay.svelte";
import type { RefusedWritesPort, WriteAsk } from "../../shell/refused-writes";

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

/** What the launcher's board lists as recent: a pull request and a ticket. */
const RECENT = [
  { entity_id: "gitea:payments#139", kind: "pull_request", title: "Retry window" },
  { entity_id: "jira:PAY-228", kind: "ticket", title: "Payout export to CSV" },
] as EntityRow[];
/** What any search finds: one ticket. */
const FOUND = { entity_id: "jira:OPS-77", kind: "ticket", title: "Rotate the gateway certificates" } as SearchHit;

function found(): SearchResponse {
  return {
    interpreted: { prefix: null },
    groups: [{ kind: "ticket", label: "Tickets", plural: "tickets", monogram: "T", total: 1, hits: [FOUND] }],
    total: 1,
    took_ms: 0,
  } as unknown as SearchResponse;
}

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
    job?: { key: string; entityId: string } | null;
    offers?: Record<number, AdHocBlock | null>;
    clock?: Date;
    stripWidth?: number;
    plainPx?: number;
    refuseDelete?: boolean;
    refusedWrites?: RefusedWritesPort;
  } = {},
) {
  const offers = over.offers ?? OFFERS;
  let listed = over.blocks ?? friday();
  const open = over.open ?? [{ started_at: at(12, 35), ended_at: at(13, 10) }];
  const updates: [number, string, string, TimerTarget][] = [];
  const creates: [string, string, TimerTarget][] = [];
  const deletes: number[] = [];
  const dismissals: number[] = [];
  const logs: [string, string][] = [];
  const searches: string[] = [];
  let stops = 0;
  const layers = createLayers();
  view = mount(TfDay, {
    target,
    props: {
      day: over.day === undefined ? DAY : over.day,
      timer: { current: over.running ?? null },
      job: over.job ?? null,
      ontimer: () => (stops += 1),
      onlog: (entityId: string, day: string) => logs.push([entityId, day]),
      now: () => over.clock ?? NOW,
      // Six hundred pixels wide and seven a character, unless a test says otherwise:
      // jsdom lays nothing out and measures no text.
      stripWidth: () => over.stripWidth ?? 600,
      measureLabel: (text: string, plain: boolean) => text.length * (plain ? (over.plainPx ?? 7) : 7),
      layers,
      ports: {
        search: (query: SearchQuery) => {
          searches.push(query.raw);
          return Promise.resolve(found());
        },
        launcherHome: () => Promise.resolve({ recent: RECENT, smart_lists: [], stale_sources: [] } as never),
        ...(over.refusedWrites ? { refusedWrites: over.refusedWrites } : {}),
        dayBlocks: () => Promise.resolve({ blocks: listed, past_horizon: over.pastHorizon ?? false, open }),
        adHocBlock: (id: number) => Promise.resolve(offers[id] ?? null),
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
          if (over.refuseDelete) return Promise.reject(new Error("no such block"));
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
  return { updates, creates, deletes, dismissals, logs, layers, searches, stops: () => stops };
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

test("after 18:00 the strip widens to hold now, and the now line is where the clock is (#1434)", async () => {
  render({ clock: new Date(2026, 7, 22, 19, 43, 0, 0) });
  await settle();
  const hours = [...target.querySelectorAll(".tf-tm-hl")].map((label) => label.textContent);
  expect(hours.at(-1)).toBe("20");
  const now = target.querySelector(".tf-tm-now")!;
  // 19:43 is 703 of 720 minutes into 08:00 to 20:00.
  expect(now.getAttribute("x")).toBe("97.64%");
  expect(squash(now.querySelector("title")?.textContent)).toBe("Now, 19:43");
});

test("a strip label that does not fit its block ends in an ellipsis, one with no room is dropped, and the title is whole (#1429)", async () => {
  render();
  await settle();
  const strip = (cls: string) => target.querySelector(`.tf-tm-strip .${cls}`)!;
  const label = (cls: string) => strip(cls).querySelector("text")?.textContent ?? null;
  // 09:40 to 11:50 is 130 of 600 px, less the label's padding: PAY-231 fits whole.
  expect(label("tf-tm-manual")).toBe("PAY-231");
  // 13:10 to 13:58 is 48 px, 36 of them for text: five characters with the ellipsis.
  expect(label("tf-tm-label")).toBe("Stan…");
  expect(squash(strip("tf-tm-label").querySelector("title")?.textContent)).toBe("13:10 to 13:58, Standup and planning");
  // 09:25 to 09:40 is 15 px: no room for a character and the ellipsis.
  expect(strip("tf-tm-passive").querySelector("text")).toBeNull();
  expect(squash(strip("tf-tm-passive").querySelector("title")?.textContent)).toBe("09:25 to 09:40, payments#139");
  // No nested <svg> clips a label at the block's edge any more.
  expect(target.querySelectorAll(".tf-tm-b svg")).toHaveLength(0);
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

test("Log… on a ticket whose source's credential cannot log work is drawn disabled saying why, and opens nothing (#2052)", async () => {
  const asked: WriteAsk[][] = [];
  const reason = "This Jira user cannot log work in PAY: it lacks the Work On Issues permission";
  const sent = render({
    refusedWrites: (asks) => {
      asked.push(asks);
      return Promise.resolve([{ entity: "jira:PAY-231", op: "log_work", reason }]);
    },
  });
  await settle();
  expect(asked).toEqual([[{ entity: "jira:PAY-231", op: "log_work" }]]);
  const log = button(row("unlogged"), "Log…");
  expect(log.getAttribute("aria-disabled")).toBe("true");
  expect(log.title).toBe(reason);
  log.click();
  expect(sent.logs).toEqual([]);
});

test("Edit moves a block and keeps its target, and the dialog sits on the shell's layers", async () => {
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
});

/**
 * *Delete block* is held for the toast's life and only then sent (#1429, the
 * #835 pattern), so *Undo* cancels a write that never happened and the block
 * keeps its id, context and worklog mark.
 */
test("Delete block hides the block at once and sends the delete when the toast's Undo has had its time", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    const sent = render();
    await settle();
    button(row("unlogged"), "Edit").click();
    flushSync();
    button(target, "Delete block").click();
    await settle();
    expect(sent.layers.open).toHaveLength(0);
    expect(rows()).toEqual(["logged", "offered", "open", "label"]);
    const toast = toasts.items.at(-1)!;
    expect([toast.text, toast.action?.label]).toEqual(["Deleted the block", "Undo"]);
    // Held for the toast's whole life, and sent as it ends.
    vi.advanceTimersByTime(6_499);
    await settle();
    expect(sent.deletes).toEqual([]);
    vi.advanceTimersByTime(1);
    await settle();
    expect(sent.deletes).toEqual([3]);
    expect(rows()).toEqual(["logged", "offered", "open", "label"]);
  } finally {
    vi.useRealTimers();
  }
});

test("Undo on the delete toast puts the block back and no delete is ever sent", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    const sent = render();
    await settle();
    button(row("unlogged"), "Edit").click();
    flushSync();
    button(target, "Delete block").click();
    await settle();
    expect(rows()).not.toContain("unlogged");
    toasts.items.at(-1)!.action!.run();
    await settle();
    expect(rows()).toEqual(["logged", "offered", "unlogged", "open", "label"]);
    expect(target.querySelectorAll(".tf-tm-strip .tf-tm-manual")).toHaveLength(1);
    vi.advanceTimersByTime(60_000);
    await settle();
    expect(sent.deletes).toEqual([]);
  } finally {
    vi.useRealTimers();
  }
});

test("a held delete the backend refuses brings the block back and says why", async () => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    render({ refuseDelete: true });
    await settle();
    button(row("unlogged"), "Edit").click();
    flushSync();
    button(target, "Delete block").click();
    await settle();
    vi.advanceTimersByTime(6_500);
    await settle();
    expect(rows()).toContain("unlogged");
    expect(toasts.items.at(-1)).toMatchObject({ text: "no such block", tone: "err" });
  } finally {
    vi.useRealTimers();
  }
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

test("an open stretch reads Assign… in the sans face on the strip, and pressing it only selects the row (#1523)", async () => {
  Element.prototype.scrollIntoView = () => {};
  render({ stripWidth: 2000 });
  await settle();
  const gap = target.querySelector(".tf-tm-strip .tf-tm-gap")!;
  const text = gap.querySelector("text")!;
  expect(text.textContent).toBe("Assign…");
  expect(text.getAttribute("class")).toContain("tf-tm-plain");
  (gap as SVGElement).dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await settle();
  expect(target.querySelector('[role="dialog"]')).toBeNull();
  expect(gap.getAttribute("class")).toContain("tf-on");
});

test("an open stretch's Assign… is fitted to a narrow block like any label (#1523)", async () => {
  render({ stripWidth: 600 });
  await settle();
  expect(target.querySelector(".tf-tm-strip .tf-tm-gap text")?.textContent).toBe("As…");
});

test("an open stretch is measured in the sans face: Assign… fits where the mono face would not (#1523)", async () => {
  // The stub measures 6 px a character plain and 7 mono, so 42 px of room holds Assign… only in sans.
  render({ stripWidth: 1000, plainPx: 6 });
  await settle();
  expect(target.querySelector(".tf-tm-strip .tf-tm-gap text")?.textContent).toBe("Assign…");
});

/** Type into the dialog's search and wait out the launcher session's debounce. */
async function search(text: string) {
  const box = target.querySelector<HTMLInputElement>('[aria-label="Search for a ticket"]')!;
  box.value = text;
  box.dispatchEvent(new Event("input", { bubbles: true }));
  await new Promise((resolve) => setTimeout(resolve, 120));
  await settle();
}

function options(): string[] {
  return [...target.querySelectorAll('[role="dialog"] [role="option"]')].map((option) => squash(option.textContent));
}

test("Assign… on an open stretch picks a ticket by name and writes a block on its id over exactly that stretch (#1429)", async () => {
  const sent = render();
  await settle();
  button(row("open"), "Assign…").click();
  await settle();
  // No raw `ns:id` field: the reader searches, and the free-text placeholder is gone.
  expect(target.querySelector('[role="dialog"]')!.innerHTML).not.toContain("jira:PAY-231");
  expect(target.querySelector('[role="dialog"] select')).toBeNull();
  await search("gateway");
  expect(sent.searches).toEqual(["gateway"]);
  expect(options()).toEqual(["OPS-77 Rotate the gateway certificates"]);
  button(target, "OPS-77 Rotate the gateway certificates").click();
  flushSync();
  expect(squash(target.querySelector(".tf-tm-picked")?.textContent)).toBe("OPS-77 Rotate the gateway certificates");
  button(target, "Assign").click();
  await settle();
  expect(sent.creates).toEqual([[at(12, 35), at(13, 10), { kind: "entity", entity_id: "jira:OPS-77" }]]);
});

test("Assign… on an offered block lists the recent tickets, and only tickets, before anything is typed, and Enter picks one", async () => {
  const sent = render();
  await settle();
  button(row("offered"), "Assign…").click();
  await settle();
  expect(options()).toEqual(["PAY-228 Payout export to CSV"]);
  // Assign waits for a pick.
  expect(button(target, "Assign").disabled).toBe(true);
  const box = target.querySelector<HTMLInputElement>('[aria-label="Search for a ticket"]')!;
  box.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  flushSync();
  button(target, "Assign").click();
  await settle();
  expect(sent.updates).toEqual([[2, at(9, 25), at(9, 40), { kind: "entity", entity_id: "jira:PAY-228" }]]);
});

test("+ Add a block on A label makes an ad-hoc block on that label", async () => {
  const sent = render();
  await settle();
  button(target, "+ Add a block").click();
  await settle();
  button(target, "A label").click();
  flushSync();
  const on = target.querySelector<HTMLInputElement>("#tf-tm-on")!;
  expect(on.placeholder).toBe("What it was");
  on.value = "Lunch walk";
  on.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button(target, "Add").click();
  await settle();
  expect(sent.creates).toEqual([[at(9), at(10), { kind: "label", label: "Lunch walk" }]]);
});

/** The app's stylesheets in the document, so a computed style is the cascade the window draws with. */
function loadAppStyles(): HTMLStyleElement {
  const APP = process.cwd();
  const tf = readFileSync(join(APP, "src/lib/tf/tf.css"), "utf8")
    .replace(/^@import "(\.\/css\/[^"]+)";$/gm, (_, part: string) => readFileSync(join(APP, "src/lib/tf", part), "utf8"))
    .replace(/^@import [^;]+;$/gm, "");
  const style = document.createElement("style");
  style.textContent = readFileSync(join(APP, "src/unported.css"), "utf8") + "\n" + tf;
  document.head.append(style);
  return style;
}

/**
 * #1428: the *On* row's target field takes the row, and what it is on can be
 * read. The kind switch is as wide as its words and does not grow; the field
 * grows and may shrink below its own width rather than push out of the row.
 * jsdom lays nothing out, so the cascade's flex values are what is witnessed.
 */
test("the On row's field takes the row and shows what the block is on; the kind switch does not grow (#1428)", async () => {
  const style = loadAppStyles();
  try {
    render();
    await settle();
    button(row("label"), "Edit").click();
    await settle();
    const on = target.querySelector<HTMLInputElement>("#tf-tm-on")!;
    expect(on.value).toBe("Standup and planning");
    const kind = target.querySelector<HTMLElement>(".tf-tm-on > .tf-seg")!;
    expect([getComputedStyle(kind).flexGrow, getComputedStyle(kind).flexShrink]).toEqual(["0", "0"]);
    expect([getComputedStyle(on).flexGrow, getComputedStyle(on).minWidth]).toEqual(["1", "0px"]);
    button(target, "Cancel").click();
    flushSync();

    // A ticket reads by its key and title, in a field that grows the same way.
    button(row("unlogged"), "Edit").click();
    await settle();
    const picked = target.querySelector<HTMLElement>(".tf-tm-on > .tf-tm-picked")!;
    expect(squash(picked.textContent)).toBe("PAY-231 Retry failed SEPA payouts");
    expect([getComputedStyle(picked).flexGrow, getComputedStyle(picked).minWidth]).toEqual(["1", "0px"]);
  } finally {
    style.remove();
  }
});

test("an open stretch offers Add to the current job, and makes a block on it over exactly that stretch", async () => {
  const sent = render({ job: { key: "PAY-231", entityId: "jira:PAY-231" } });
  await settle();
  expect(button(row("open"), "Add to PAY-231").className).not.toContain("tf-ghost");
  expect(button(row("open"), "Assign…").className).toContain("tf-ghost");
  button(row("open"), "Add to PAY-231").click();
  await settle();
  expect(sent.creates).toEqual([[at(12, 35), at(13, 10), { kind: "entity", entity_id: "jira:PAY-231" }]]);
  expect(toasts.items.at(-1)?.text).toBe("Made a block on PAY-231, 12:35 to 13:10");
});

test("without a current job an open stretch offers Assign… only", async () => {
  render();
  await settle();
  expect(button(row("open"), "Assign…").className).not.toContain("tf-ghost");
  expect(() => button(row("open"), "Add to PAY-231")).toThrow();
  expect(row("open").textContent).not.toContain("Add to");
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

// #1108: job, its pull request (confirmed link to the job), job again.
const PR_VISIT = { kind: "entity" as const, entity_id: "gitea:payments#139" };
function jobPrJob(): DayBlock[] {
  const job = { kind: "entity" as const, entity_id: "jira:PAY-231" };
  return [
    block(21, at(9), at(9, 20), { kind: "passive", target: job }),
    block(22, at(9, 20), at(9, 25), { kind: "passive", target: PR_VISIT }),
    block(23, at(9, 25), at(9, 47), { kind: "passive", target: job }),
  ];
}
const linked = (rule: "linked_to_target" | "last_logged" | "context_anchor") => ({
  21: null,
  22: { suggestion: { entity_id: "jira:PAY-231", title: null, rule } },
  23: null,
});

function runs(): HTMLElement[] {
  return [...target.querySelectorAll<HTMLElement>(".tf-tm-group")];
}

test("a run of offered rows going to one ticket offers one Add over all of them", async () => {
  const sent = render({ blocks: jobPrJob(), offers: linked("linked_to_target"), open: [] });
  await settle();
  expect(runs()).toHaveLength(1);
  expect(squash(runs()[0]!.textContent)).toContain("3 offered rows go to PAY-231");
  button(runs()[0]!, "Add 47m to PAY-231").click();
  await settle();
  // A block per visit, each on the ticket over its own stretch: the worklog
  // draft sums them, and the record still says a pull request was in front.
  const ticket = { kind: "entity", entity_id: "jira:PAY-231" };
  expect(sent.updates).toEqual([
    [21, at(9), at(9, 20), ticket],
    [22, at(9, 20), at(9, 25), ticket],
    [23, at(9, 25), at(9, 47), ticket],
  ]);
  expect(toasts.items.at(-1)?.text).toBe("Added 47m to PAY-231");
  expect(rows()).not.toContain("offered");
  expect(runs()).toHaveLength(0);
});

test("each row in a run keeps its own Add to, Assign… and Dismiss", async () => {
  render({ blocks: jobPrJob(), offers: linked("linked_to_target"), open: [] });
  await settle();
  const offered = [...target.querySelectorAll<HTMLElement>('.tf-tm-row[data-state="offered"]')];
  expect(offered).toHaveLength(3);
  for (const each of offered) {
    for (const label of ["Add to PAY-231", "Assign…", "Dismiss"]) expect(() => button(each, label)).not.toThrow();
  }
});

test("a guess in the middle breaks the run: no Add over the rows", async () => {
  for (const rule of ["last_logged", "context_anchor"] as const) {
    if (view) unmount(view);
    target.replaceChildren();
    render({ blocks: jobPrJob(), offers: linked(rule), open: [] });
    await settle();
    expect(rows()).toEqual(["offered", "offered", "offered"]);
    expect(runs()).toHaveLength(0);
  }
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
