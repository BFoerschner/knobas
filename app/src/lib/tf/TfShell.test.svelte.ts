/**
 * The ticket-first shell's frame (#809): the `Esc` ladder in order, the theme
 * toggle, the badges, the timer chip and the status bar, against a shell
 * mounted with stand-ins for every store and read.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { CredentialHealth, SourceSyncStatus } from "../ipc/sources";
import type { RunningTimer } from "../ipc/time";
import { writeQueuePanel, type WriteQueue } from "../shell/write-queue.svelte";
import { push, toasts } from "../shell/toasts.svelte";
import { createBadges } from "./badges.svelte";
import { createLayers, type Layers } from "./layers.svelte";
import type { JobPorts } from "./job/ports";
import TfShell from "./TfShell.svelte";
import { createTheme, type Theme } from "./theme.svelte";

// The Day tab reads the real bridge; its one read here is an open stretch.
vi.mock("../ipc/time", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../ipc/time")>()),
  dayBlocks: () =>
    Promise.resolve({
      blocks: [],
      past_horizon: false,
      open: [{ started_at: new Date(2026, 7, 22, 12, 35).toISOString(), ended_at: new Date(2026, 7, 22, 13, 10).toISOString() }],
    }),
}));

let target: HTMLElement;
let shell: ReturnType<typeof mount> | null = null;

beforeEach(() => {
  location.hash = "";
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (shell) unmount(shell);
  shell = null;
  target.remove();
  document.querySelectorAll("[data-test-field]").forEach((el) => el.remove());
});

async function settle() {
  for (let turn = 0; turn < 5; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

async function at(hash: string) {
  location.hash = hash;
  await settle();
}

function press(key: string, options: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...options });
  (document.activeElement ?? window).dispatchEvent(event);
  return event;
}

const queue = {
  counts: { pending: 2, held: 0, refused: 0 },
  rows: [],
  error: null,
  answered: true,
  busy: false,
  refresh: () => Promise.resolve(),
  flush: () => Promise.resolve(),
  apply: () => Promise.resolve(),
  discard: () => Promise.resolve(),
  amend: () => Promise.resolve(),
} satisfies WriteQueue;

function health(id: string, state: CredentialHealth["state"]): CredentialHealth {
  return { source_id: id, state, checked_at: new Date().toISOString(), detail: null, secret_expires_at: null };
}

/** The job page's reads: `PAY-231` under Jira, holding nothing yet. */
const JOB_PORTS: JobPorts = {
  listSources: () => Promise.resolve([{ id: "jira" }]),
  getEntity: (id) =>
    id === "jira:PAY-231"
      ? Promise.resolve({
          row: { entity_id: id, kind: "ticket", source_id: "jira", title: "Retry failed SEPA payouts", updated_at: null, synced_at: "", path: null, state: null },
          source: { id: "jira", display_name: "Jira", adapter_kind: "jira", enabled: true },
          kind_info: null,
          body_text: "",
          author: null,
          payload: {},
          web_url: null,
          deleted_at: null,
          links: [],
          activity: [],
          builds: [],
          status: "In Progress",
          issue_type: null,
          priority: null,
          estimate_seconds: null,
        })
      : Promise.reject({ code: "not_found", message: "", source_id: null }),
  promoteContext: (id) => Promise.resolve({ id: "ctx:1", kind: "ticket", title: "", anchor_id: id, created_at: "", archived_at: null }),
  listEntities: () => Promise.resolve({ rows: [], total: 0, tally: null, groups: null }),
  reachableTransitions: () => Promise.resolve([]),
  submitWrite: () => Promise.reject(new Error("not in this suite")),
  entityCheckout: () => Promise.reject(new Error("not in this suite")),
  weekTimesheet: () => Promise.resolve({ days: [], rows: [], past_horizon: [] }),
  visitEntity: () => Promise.resolve(null),
  entityThread: () => Promise.resolve({ comments: [], changes: [], me: [] }),
  createLink: () => Promise.reject(new Error("not in this suite")),
  unlink: () => Promise.reject(new Error("not in this suite")),
  roomSuggestions: () => Promise.resolve({ rows: [], total: 0, more: [] }),
  search: () => Promise.reject(new Error("not in this suite")),
  launcherHome: () => Promise.reject(new Error("not in this suite")),
  jobReach: () => Promise.resolve([]),
  jobLinkSets: () => Promise.resolve({ sets: [], parent: null }),
  hideLinkSetMember: (_ticket, _member, hidden) => Promise.resolve(hidden),
  getAsset: () => Promise.reject(new Error("not in this suite")),
  credentialHealth: () => Promise.resolve([]),
  syncStatus: () => Promise.resolve([]),
  startWork: {
    flow: () => Promise.resolve([]),
    run: () => Promise.resolve([]),
    retry: () => Promise.resolve([]),
    skip: () => Promise.resolve([]),
    amend: () => Promise.resolve([]),
  },
};

function start(
  over: {
    layers?: Layers;
    theme?: Theme;
    timer?: { current: RunningTimer | null; elapsed: string | null };
    ontimer?: () => void;
    prefersDark?: () => boolean;
    onsearch?: () => void;
    monitorsDown?: number;
    onstarttimer?: (entityId: string, title: string) => void;
    nav?: { back: () => void; forward: () => void };
  } = {},
) {
  const finished = new Date(Date.now() - 4 * 60_000).toISOString();
  const sync: SourceSyncStatus[] = [
    { source_id: "jira", running: false, run_id: 1, started_at: null, last_finished_at: finished, last_outcome: "ok", next_run_at: null, backoff_until: null, last_changed: null },
  ];
  shell = mount(TfShell, {
    target,
    props: {
      lifecycle: { ready: true },
      onsearch: over.onsearch ?? (() => {}),
      ontimer: over.ontimer ?? (() => {}),
      timer: over.timer ?? { current: null, elapsed: null },
      inbox: { count: 3 },
      health: { all: [health("jira", "ok"), health("teamcity", "unauthorized")] },
      badges: createBadges({
        failingBuilds: () => Promise.resolve(2),
        monitorsDown: () => Promise.resolve(over.monitorsDown ?? 0),
        proposals: () => Promise.resolve(5),
        listen: () => Promise.resolve(() => {}),
      }),
      layers: over.layers ?? createLayers(),
      theme: over.theme ?? createTheme(),
      queue,
      ports: {
        listSources: () =>
          Promise.resolve([
            { id: "jira", display_name: "Jira" },
            { id: "teamcity", display_name: "TeamCity" },
          ]),
        syncStatus: () => Promise.resolve(sync),
        listen: () => Promise.resolve(() => {}),
      },
      prefersDark: over.prefersDark ?? (() => false),
      onstarttimer: over.onstarttimer ?? (() => {}),
      jobPorts: JOB_PORTS,
      nav: over.nav ?? { back: () => {}, forward: () => {} },
    },
  });
  flushSync();
}

function filterField(text: string): HTMLInputElement {
  const field = document.createElement("input");
  field.setAttribute("data-filter", "");
  field.setAttribute("data-test-field", "");
  field.value = text;
  document.body.appendChild(field);
  field.focus();
  return field;
}

test("Esc unwinds dialog, drawer, filter text, filter focus, sub-tab, job, one layer per press", async () => {
  const layers = createLayers();
  const closed: string[] = [];
  const offDrawer = layers.add({ kind: "drawer", close: () => (closed.push("drawer"), offDrawer()) });
  const offDialog = layers.add({ kind: "dialog", close: () => (closed.push("dialog"), offDialog()) });
  start({ layers });
  await at("#/job/PAY-231/conv");
  const field = filterField("sepa");
  const typed: string[] = [];
  field.addEventListener("input", () => typed.push(field.value));

  press("Escape");
  expect(closed).toEqual(["dialog"]);
  press("Escape");
  expect(closed).toEqual(["dialog", "drawer"]);

  press("Escape");
  expect(field.value).toBe("");
  expect(typed, "clearing the filter tells the view it changed").toEqual([""]);
  expect(document.activeElement).toBe(field);

  press("Escape");
  expect(document.activeElement).not.toBe(field);
  expect(location.hash).toBe("#/job/PAY-231/conv");

  press("Escape");
  await settle();
  expect(location.hash).toBe("#/job/PAY-231");

  press("Escape");
  await settle();
  expect(location.hash).toBe("#/tickets");

  press("Escape");
  await settle();
  expect(location.hash, "Tickets is where the ladder ends").toBe("#/tickets");
});

test("Esc on a full view returns to the job last visited, and names it in the breadcrumb", async () => {
  start();
  await at("#/job/PAY-231");
  await at("#/builds");
  const crumbs = target.querySelector(".tf-crumbs");
  expect(crumbs?.querySelector(".tf-jobchip")?.textContent).toBe("PAY-231");
  expect(crumbs?.querySelector(".tf-esc")?.textContent?.replace(/\s+/g, " ").trim()).toBe("Back to PAY-231 Esc");

  press("Escape");
  await settle();
  expect(location.hash).toBe("#/job/PAY-231");
});

test("with no job, the breadcrumb starts at the view's own name and Esc goes to Tickets", async () => {
  start();
  await at("#/inbox");
  expect(target.querySelector(".tf-crumbs .tf-jobchip")).toBeNull();
  expect(target.querySelector(".tf-crumbs strong")?.textContent).toBe("Inbox");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/tickets");
});

test("an address the new shell has no place for names itself", async () => {
  start();
  // Link sets was the last section still drawn as "not built yet" (#866).
  await at("#/sets");
  expect(target.querySelector(".tf-fhead h1")?.textContent).toBe("Link sets");
  expect(target.textContent).not.toContain("is not built in the new shell yet");
  await at("#/nowhere");
  expect(target.querySelector(".tf-empty")?.textContent).toContain("Nothing lives at #/nowhere in the new shell.");
});

// #1085: a rooms-UI address lands where the new shell keeps that thing, in
// place, so Back does not return to the dead address.
test("a rooms-UI address is replaced by the ticket-first one", async () => {
  start();
  await at("#/tickets");
  const depth = history.length;
  await at("#/ctx/all");
  expect(location.hash).toBe("#/tickets");
  await at("#/ticket/jira:PAY-231");
  expect(location.hash).toBe("#/job/PAY-231");
  await at("#/sources");
  expect(location.hash).toBe("#/settings/sources");
  expect(target.textContent).not.toContain("Nothing lives at");
  expect(history.length - depth, "each old address was a step, its replacement was not").toBe(3);
});

// The theme is on `<html>`, where the tokens are since #881, so a dialog
// mounted outside the shell follows it too.
test("the theme follows the OS until the toggle overrides it, both ways", async () => {
  start({ prefersDark: () => true });
  await settle();
  const root = document.documentElement;
  expect(root.hasAttribute("data-theme"), "no override: the OS decides").toBe(false);
  const toggle = target.querySelector<HTMLButtonElement>('[aria-label="Toggle colour theme"]')!;
  toggle.click();
  flushSync();
  expect(root.getAttribute("data-theme")).toBe("light");
  toggle.click();
  flushSync();
  expect(root.getAttribute("data-theme")).toBe("dark");
});

test("the top bar's toggle and Settings › Appearance are one setting", async () => {
  location.hash = "#/settings/appearance";
  start({ prefersDark: () => false });
  await settle();
  const root = document.documentElement;
  const choice = (label: string) =>
    [...target.querySelectorAll<HTMLButtonElement>('[aria-label="Theme"] button')].find((b) => b.textContent === label)!;
  choice("Dark").click();
  flushSync();
  expect(root.getAttribute("data-theme")).toBe("dark");
  target.querySelector<HTMLButtonElement>('[aria-label="Toggle colour theme"]')!.click();
  flushSync();
  expect(root.getAttribute("data-theme")).toBe("light");
  expect(choice("Light").getAttribute("aria-pressed")).toBe("true");
  choice("Like the system").click();
  flushSync();
  expect(root.hasAttribute("data-theme")).toBe(false);
});

test("a light OS toggles to dark first", async () => {
  start({ prefersDark: () => false });
  target.querySelector<HTMLButtonElement>('[aria-label="Toggle colour theme"]')!.click();
  flushSync();
  expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  expect(target.querySelector(".tf")!.hasAttribute("data-theme"), "the shell's own root carries no theme").toBe(false);
});

test("a forced theme leaves <html> with the shell", async () => {
  start({ prefersDark: () => false });
  target.querySelector<HTMLButtonElement>('[aria-label="Toggle colour theme"]')!.click();
  flushSync();
  expect(document.documentElement.getAttribute("data-theme")).toBe("dark");
  unmount(shell!);
  shell = null;
  expect(document.documentElement.hasAttribute("data-theme")).toBe(false);
});

test("badges: inbox, failing builds in err tone, proposals; nothing drawn at zero", async () => {
  start();
  await settle();
  const counts = Object.fromEntries(
    [...target.querySelectorAll(".tf-nav a")].map((a) => [
      a.firstChild?.textContent?.trim(),
      a.querySelector(".tf-count") ? `${a.querySelector(".tf-count")!.textContent}${a.querySelector(".tf-count.tf-err") ? " err" : ""}` : null,
    ]),
  );
  expect(counts).toEqual({
    Tickets: null,
    Job: null,
    Inbox: "3",
    Code: null,
    Builds: "2 err",
    Docs: null,
    Dependencies: null,
    Monitors: null,
    "Link sets": null,
    "Proposed links": "5",
    Time: null,
  });
});

test("the timer chip ticks the store's reading and stops through the shell's verb, as ⌘T does", async () => {
  const ontimer = vi.fn();
  const timer = $state<{ current: RunningTimer | null; elapsed: string | null }>({
    current: { target: { kind: "entity", entity_id: "jira:PAY-231" }, started_at: "2026-08-22T12:00:00Z", last_heartbeat: "2026-08-22T12:00:00Z" } as RunningTimer,
    elapsed: "4:07",
  });
  start({ timer, ontimer });
  await settle();
  const chip = target.querySelector<HTMLButtonElement>(".tf-timer")!;
  expect(chip.classList.contains("tf-card"), "a running timer is on the card yellow").toBe(true);
  expect(chip.textContent?.replace(/\s+/g, " ").trim()).toBe("PAY-231 0:04:07");
  timer.elapsed = "4:08";
  flushSync();
  expect(chip.querySelector(".tf-timer-t")?.textContent).toBe("0:04:08");
  chip.click();
  press("t", { metaKey: true });
  expect(ontimer).toHaveBeenCalledTimes(2);
  expect(target.querySelector(".tf-nav a:nth-child(2)")?.textContent?.replace(/\s+/g, " ").trim(), "the Job tab names the timer's ticket").toBe("Job PAY-231");
});

test("⌘K opens search and ⌘, opens Settings", async () => {
  const onsearch = vi.fn();
  start({ onsearch });
  press("k", { metaKey: true });
  expect(onsearch).toHaveBeenCalledOnce();
  press(",", { metaKey: true });
  await settle();
  expect(location.hash).toBe("#/settings");
});

test("⌘[ ⌘] ⌘← ⌘→ and the mouse's back and forward buttons step through history (#693, #1086)", () => {
  const nav = { back: vi.fn(), forward: vi.fn() };
  start({ nav });
  expect(press("[", { metaKey: true }).defaultPrevented).toBe(true);
  press("[", { ctrlKey: true });
  press("ArrowLeft", { metaKey: true });
  expect(nav.back).toHaveBeenCalledTimes(3);
  press("]", { metaKey: true });
  press("]", { ctrlKey: true });
  press("ArrowRight", { metaKey: true });
  expect(nav.forward).toHaveBeenCalledTimes(3);

  for (const [button, called] of [
    [3, nav.back],
    [4, nav.forward],
  ] as const) {
    const down = new MouseEvent("mousedown", { button, bubbles: true, cancelable: true });
    window.dispatchEvent(down);
    expect(down.defaultPrevented, "the webview adds no navigation of its own").toBe(true);
    expect(called).toHaveBeenCalledTimes(3);
    window.dispatchEvent(new MouseEvent("mouseup", { button, bubbles: true, cancelable: true }));
    expect(called).toHaveBeenCalledTimes(4);
  }
});

test("history keys leave a text field's own keys, Ctrl+arrows, ⌥ with Ctrl and a key already answered alone", () => {
  const nav = { back: vi.fn(), forward: vi.fn() };
  start({ nav });
  const field = filterField("PAY");
  expect(press("ArrowLeft", { metaKey: true }).defaultPrevented, "⌘← is start of line in a field").toBe(false);
  press("[", { metaKey: true });
  field.blur();
  press("ArrowLeft", { ctrlKey: true });
  press("[", { ctrlKey: true, altKey: true });
  press("[", { metaKey: true, shiftKey: true });
  const answered = new KeyboardEvent("keydown", { key: "[", metaKey: true, bubbles: true, cancelable: true });
  answered.preventDefault();
  window.dispatchEvent(answered);
  window.dispatchEvent(new MouseEvent("mouseup", { button: 0, bubbles: true }));
  expect(nav.back).not.toHaveBeenCalled();
  press("[", { metaKey: true, altKey: true });
  expect(nav.back, "⌥ with ⌘ is a layout typing the bracket").toHaveBeenCalledOnce();
});

test("the status bar: sync ages, a 401 with Re-enter password, the queued writes", async () => {
  start();
  await settle();
  const bar = target.querySelector(".tf-status")!;
  expect(bar.textContent).toContain("Jira synced 4 min ago");
  const bad = bar.querySelector(".tf-bad")!;
  expect(bad.textContent).toContain("TeamCity: sign-in failed");
  expect(bad.querySelector("a")?.getAttribute("href")).toBe("#/settings/source/teamcity");
  expect(bar.querySelector("button.tf-queued")?.textContent).toBe("2 queued writes");
});

test("the queued-writes count opens the queue list (R-1.5)", async () => {
  start();
  await settle();
  try {
    target.querySelector<HTMLButtonElement>(".tf-status button.tf-queued")!.click();
    flushSync();
    expect(writeQueuePanel.open).toBe(true);
  } finally {
    writeQueuePanel.open = false;
    flushSync();
  }
});

test("the Monitors badge counts monitors down, in the err tone", async () => {
  start({ monitorsDown: 1 });
  await settle();
  const monitors = [...target.querySelectorAll(".tf-nav a")].find((a) => a.firstChild?.textContent?.trim() === "Monitors")!;
  expect(monitors.querySelector(".tf-count")?.textContent).toBe("1");
  expect(monitors.querySelector(".tf-count.tf-err"), "down is the err tone").not.toBeNull();
});

test("a running timer names the Job tab but is not a job the breadcrumb or Esc returns to", async () => {
  const timer = {
    current: { target: { kind: "entity", entity_id: "jira:PAY-231" }, started_at: "2026-08-22T12:00:00Z", last_heartbeat: "2026-08-22T12:00:00Z" } as RunningTimer,
    elapsed: "1:00",
  };
  start({ timer });
  await at("#/builds");
  expect(target.querySelector(".tf-crumbs .tf-jobchip")).toBeNull();
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/tickets");
});

test("a toast from anywhere lands in the shell's stack, as text, and its action runs and dismisses it (R-1.8)", async () => {
  start();
  const run = vi.fn();
  push({ text: "Queued: re-run <b>#1187</b>", tone: "err", action: { label: "Show queue", run }, ms: 60_000 });
  flushSync();
  try {
    const toast = target.querySelector(".tf-toasts .tf-toast")!;
    expect(toast.classList.contains("tf-err")).toBe(true);
    expect(toast.querySelector("span")?.textContent, "text, never markup").toBe("Queued: re-run <b>#1187</b>");
    const action = [...toast.querySelectorAll("button")].find((button) => button.textContent === "Show queue")!;
    action.click();
    flushSync();
    expect(run).toHaveBeenCalledOnce();
    expect(target.querySelector(".tf-toasts .tf-toast"), "the action dismisses its toast").toBeNull();
  } finally {
    toasts.items = [];
  }
});

test("a job address draws the job inside the frame, and its Start timer reaches the shell's timer verb", async () => {
  const started: [string, string][] = [];
  start({ onstarttimer: (id, title) => started.push([id, title]) });
  await at("#/job/PAY-231");
  expect(target.querySelector(".tf-jobcard .tf-jobkey")?.textContent).toBe("PAY-231");
  expect(target.querySelector(".tf-nav a[aria-current='page']")?.textContent).toContain("PAY-231");
  [...target.querySelectorAll<HTMLButtonElement>(".tf-jobcard button")].find((b) => b.textContent === "Start timer")!.click();
  expect(started).toEqual([["jira:PAY-231", "Retry failed SEPA payouts"]]);
});

test("a job opened by its id names its key on the Job tab", async () => {
  start();
  await at("#/job/jira:PAY-231");
  expect(target.querySelector(".tf-nav a[aria-current='page']")?.textContent).toContain("PAY-231");
  expect(target.querySelector(".tf-nav a[aria-current='page']")?.textContent).not.toContain("jira:");
});

function addToButton(): HTMLButtonElement | undefined {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim().startsWith("Add to"));
}

test("the Day tab's open rows offer Add to the job last opened, found by its key (#1027)", async () => {
  start();
  await at("#/job/PAY-231");
  await at("#/time/2026-08-22");
  expect(addToButton()?.textContent?.trim()).toBe("Add to PAY-231");
});

test("with no job opened, the Day tab's open rows offer Add to the timer's ticket (#1027)", async () => {
  const timer = {
    current: { target: { kind: "entity", entity_id: "jira:PAY-231" }, started_at: "2026-08-22T12:00:00Z", last_heartbeat: "2026-08-22T12:00:00Z" } as RunningTimer,
    elapsed: "1:00",
  };
  start({ timer });
  await at("#/time/2026-08-22");
  expect(addToButton()?.textContent?.trim()).toBe("Add to PAY-231");
});

test("with no job and no timer, the Day tab's open rows offer Assign… only (#1027)", async () => {
  start();
  await at("#/time/2026-08-22");
  expect(addToButton()).toBeUndefined();
  expect([...target.querySelectorAll("button")].some((b) => b.textContent?.trim() === "Assign…")).toBe(true);
});
