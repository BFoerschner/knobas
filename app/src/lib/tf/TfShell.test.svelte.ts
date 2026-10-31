/**
 * The ticket-first shell's frame (#809): the `Esc` ladder in order, the theme
 * toggle, the badges, the timer chip and the status bar, against a shell
 * mounted with stand-ins for every store and read.
 */
import { createRawSnippet, flushSync, mount, tick, unmount, type Snippet } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { CredentialHealth, RoleClaim, SourceSyncStatus } from "../ipc/sources";
import type { RunningTimer } from "../ipc/time";
import { writeQueuePanel, type WriteQueue } from "../shell/write-queue.svelte";
import { createHealth, type Health } from "../shell/health.svelte";
import { push, toasts } from "../shell/toasts.svelte";
import Modal from "../shell/Modal.fixture.svelte";
import { createBadges } from "./badges.svelte";
import { createLayers, type Layers } from "./layers.svelte";
import type { JobPorts } from "./job/ports";
import { entityAddress } from "./address";
import { sourceRoles } from "./roles.svelte";
import { goWithoutOpener, replaceAddress } from "./opener";
import { jobHash, NAV, SECTION_NAME } from "./route";
import TfShell, { type TfShellPorts } from "./TfShell.svelte";
import { installCommands } from "../render/command";
import { renderersForFence } from "../render/registry";
import { rendererDefaults } from "../render/defaults.svelte";
import { namedPipelines } from "../render/pipelines.svelte";
import { defaultPipelines } from "./diff/piped";
import { createTheme, type Theme } from "./theme.svelte";
import { createRoleSyncs, type RoleSyncPorts, type RoleSyncs } from "./role-sync.svelte";
import { sourceFacts } from "./job/source-facts.fixture";

/** Every block the Day tab's *Add to* wrote, as its target (#1525). */
const created = vi.hoisted(() => [] as unknown[]);
/** How many times the Tickets board was read (#2106). */
const boardReads = vi.hoisted(() => ({ n: 0 }));
/** Every item `refresh_entity` was asked to read (#2156). */
const refreshed = vi.hoisted(() => [] as string[]);

// An item's *Refresh* reads the bridge's `refresh_entity` (#2107).
vi.mock("../ipc/sources", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../ipc/sources")>()),
  refreshEntity: (id: string) => {
    refreshed.push(id);
    return new Promise(() => {});
  },
}));

// The Day tab reads the real bridge; its one read here is an open stretch.
vi.mock("../ipc/time", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../ipc/time")>()),
  createBlock: (_from: string, _to: string, target: unknown) => {
    created.push(target);
    return Promise.resolve(null);
  },
  dayBlocks: () =>
    Promise.resolve({
      blocks: [],
      past_horizon: false,
      open: [{ started_at: new Date(2026, 7, 22, 12, 35).toISOString(), ended_at: new Date(2026, 7, 22, 13, 10).toISOString() }],
    }),
}));

// An item's page (#1201) reads the bridge: a survey of the fixture adapter's
// custom role, and the view its `view` answers (#1202).
vi.mock("../ipc/entity", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../ipc/entity")>()),
  miniBoard: () => {
    boardReads.n += 1;
    return Promise.reject({ code: "not_ready", message: "no board in this suite", source_id: null });
  },
  getEntity: (id: string) =>
    id === "surveys:survey-1"
      ? Promise.resolve({
          row: { entity_id: id, kind: "survey", source_id: "surveys", title: "Quay crane 4 inspection", updated_at: null, synced_at: "", path: null, state: null },
          source: { id: "surveys", display_name: "Harbour surveys", adapter_kind: "wasm-fixture", enabled: true },
          kind_info: { id: "survey", label: "Survey", plural: "Surveys", monogram: "SV", full_sync_exhaustive: true },
          refreshes: true,
          body_text: "",
          web_url: null,
        })
      : Promise.reject({ code: "not_found", message: `${id} is not in the mirror`, source_id: null }),
  entityView: () =>
    Promise.resolve({
      sections: [
        {
          id: "survey",
          title: "Quay crane 4 inspection",
          count: null,
          deferred: false,
          blocks: [
            { type: "facts", facts: [{ label: "Surveyor", value: "harbourmaster", tone: null }] },
            { type: "text", text: "hoist brake worn, booked for Tuesday", markdown: false },
          ],
        },
      ],
      actions: [],
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
  refreshed.length = 0;
  if (shell) unmount(shell);
  shell = null;
  target.remove();
  document.querySelectorAll("[data-test-field]").forEach((el) => el.remove());
});

/** The elements matching `selector` a reader sees: none inside a page keep-alive holds hidden (#2247). */
function seen<E extends Element = HTMLElement>(selector: string): E[] {
  return [...target.querySelectorAll<E>(selector)].filter((element) => element.closest("[hidden]") === null);
}

async function settle() {
  for (let turn = 0; turn < 5; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

async function at(hash: string) {
  location.hash = hash;
  await settle();
}

/** Go to `hash` with no opener, as the ⌘K launcher, a deep link and an `Esc` step do (#2229). */
async function reached(hash: string) {
  goWithoutOpener(hash);
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
  flushing: false,
  refresh: () => Promise.resolve(),
  flush: () => Promise.resolve(),
  apply: () => Promise.resolve(),
  discard: () => Promise.resolve(),
  amend: () => Promise.resolve(true),
} satisfies WriteQueue;

function health(id: string, state: CredentialHealth["state"]): CredentialHealth {
  return { source_id: id, state, checked_at: new Date().toISOString(), detail: null, secret_expires_at: null };
}

/** The job page's reads: `PAY-231` under Jira, holding nothing yet. */
const JOB_PORTS: JobPorts = {
  listSources: () => Promise.resolve([sourceFacts("jira")]),
  getEntity: (id) =>
    id === "jira:PAY-231"
      ? Promise.resolve({
          row: { entity_id: id, kind: "ticket", source_id: "jira", title: "Retry failed SEPA payouts", updated_at: null, synced_at: "", path: null, state: null },
          source: { id: "jira", display_name: "Jira", adapter_kind: "jira", enabled: true },
          kind_info: null,
          refreshes: true,
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
  entityThread: () => Promise.resolve({ comments: [], me: [] }),
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
    sources?: { id: string; display_name: string; roles?: RoleClaim[]; enabled?: boolean }[];
    roleSyncs?: RoleSyncs;
    listSources?: TfShellPorts["listSources"];
    listen?: TfShellPorts["listen"];
    counts?: WriteQueue["counts"];
    jobPorts?: JobPorts;
    dialogs?: Snippet<[SourceSyncStatus[]]>;
    health?: Pick<Health, "all">;
    pagesKeptOpen?: TfShellPorts["pagesKeptOpen"];
    userRenderers?: TfShellPorts["userRenderers"];
    rendererDefaults?: TfShellPorts["rendererDefaults"];
    namedPipelines?: TfShellPorts["namedPipelines"];
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
      health: over.health ?? { all: [health("jira", "ok"), health("teamcity", "unauthorized")] },
      badges: createBadges({
        failingBuilds: () => Promise.resolve(2),
        monitorsDown: () => Promise.resolve(over.monitorsDown ?? 0),
        proposals: () => Promise.resolve(5),
        listen: () => Promise.resolve(() => {}),
      }),
      layers: over.layers ?? createLayers(),
      theme: over.theme ?? createTheme(),
      queue: over.counts ? { ...queue, counts: over.counts } : queue,
      ports: {
        listSources:
          over.listSources ??
          (() =>
            Promise.resolve(
              // As `list_sources` sends a source: its claims and misses, empty unless the test names them.
              (
                over.sources ?? [
                  { id: "jira", display_name: "Jira" },
                  { id: "teamcity", display_name: "TeamCity" },
                ]
              ).map((source) => ({ roles: [], role_misses: [], ...source })),
            )),
        syncStatus: () => Promise.resolve(sync),
        listen: over.listen ?? (() => Promise.resolve(() => {})),
        ...(over.pagesKeptOpen ? { pagesKeptOpen: over.pagesKeptOpen } : {}),
        ...(over.userRenderers ? { userRenderers: over.userRenderers } : {}),
        ...(over.rendererDefaults ? { rendererDefaults: over.rendererDefaults } : {}),
        ...(over.namedPipelines ? { namedPipelines: over.namedPipelines } : {}),
      },
      prefersDark: over.prefersDark ?? (() => false),
      onstarttimer: over.onstarttimer ?? (() => {}),
      jobPorts: over.jobPorts ?? JOB_PORTS,
      nav: over.nav ?? { back: () => {}, forward: () => {} },
      ...(over.roleSyncs ? { roleSyncs: over.roleSyncs } : {}),
      ...(over.dialogs ? { dialogs: over.dialogs } : {}),
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

test("Esc unwinds dialog, drawer, filter text, filter focus, then the job's sub-tab, one layer per press; the first tab is the top", async () => {
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
  expect(location.hash, "a sub-tab goes to the job's first tab").toBe("#/job/PAY-231");

  const last = press("Escape");
  await settle();
  expect(last.defaultPrevented, "the job's first tab is a top page: Esc does nothing").toBe(false);
  expect(location.hash).toBe("#/job/PAY-231");
});

/** The breadcrumb's `Esc` hint as read, or `null` where none is drawn. */
function escHint(): string | null {
  return seen(".tf-esc")[0]?.textContent?.replace(/\s+/g, " ").trim() ?? null;
}

test("Esc on every nav entry's top page and on Settings does nothing, and no breadcrumb names it", async () => {
  start();
  const tops = [
    ...NAV.map((section) => (section === "job" ? jobHash(null) : `#/${section}`)),
    "#/settings",
    "#/tickets/view-1",
    "#/job/PAY-231",
  ];
  for (const top of tops) {
    await at(top);
    const event = press("Escape");
    await settle();
    expect(event.defaultPrevented, `${top} takes no Esc`).toBe(false);
    expect(location.hash, `${top} stays`).toBe(top);
    expect(escHint(), `${top} names no Esc`).toBeNull();
  }
});

test("Esc on a detail page reached with no opener goes to its structural parent; deeper inside a section, to the section's top page", async () => {
  start();
  const parents: [string, string][] = [
    ["#/build/pipe-1/run-4812", "#/builds"],
    ["#/builds/pipe-1", "#/builds"],
    ["#/pr/gh:pr-12", "#/code"],
    ["#/pr/gh:pr-12/files", "#/code"],
    ["#/repo/gh:repo-1", "#/code"],
    ["#/repo/gh:repo-1/branches", "#/code"],
    // A commit opens under its repository's Commits tab: its parent is the repository's page.
    ["#/repo/gh:repo-1/commits/gh:c-9", "#/repo/gh%3Arepo-1"],
    // A branch or commit address the shell has not placed under a repository yet.
    ["#/branch/gh:b-1", "#/code"],
    ["#/commit/gh:c-9", "#/code"],
    ["#/page/confluence:2", "#/docs"],
    ["#/docs/space/ENG", "#/docs"],
    ["#/asset/assets:svc-1", "#/deps"],
    ["#/route/assets:route-1", "#/deps"],
    ["#/assets", "#/deps"],
    ["#/code/repos", "#/code"],
    ["#/monitors/unmonitored", "#/monitors"],
    ["#/settings/appearance", "#/settings"],
    // A ticket page (spec #2227), here one the mirror cannot read: its parent is Tickets either way.
    ["#/ticket/jira:PAY-999", "#/tickets"],
  ];
  for (const [from, parent] of parents) {
    await reached(from);
    press("Escape");
    await settle();
    expect(location.hash, `Esc on ${from}`).toBe(parent);
  }
});

/** The window's own history, so `Esc` stepping back and `⌘]` going forward move the address for real. */
const windowNav = { back: () => history.back(), forward: () => history.forward() };

test("a run opened from the Job page: Esc returns to the Job page, and the hint said Back to Job", async () => {
  start({ nav: windowNav });
  await at("#/job/PAY-231/builds");
  await at("#/build/pipe-1/run-4812");
  expect(escHint()).toBe("Back to Job Esc");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/job/PAY-231/builds");
});

test("a job's row opens a run by its bare id; the address the run page puts in its place keeps the Job page as opener", async () => {
  start({ nav: windowNav });
  await at("#/job/PAY-231");
  await at("#/build/run-4812");
  // What Builds does once it knows the run's pipeline.
  replaceAddress("#/build/pipe-1/run-4812");
  await settle();
  expect(escHint()).toBe("Back to Job Esc");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/job/PAY-231");
});

test("a run opened from the Builds list: Esc returns to the Builds list", async () => {
  start({ nav: windowNav });
  await at("#/builds/pipe-1");
  await at("#/build/pipe-1/run-4812");
  expect(escHint()).toBe("Back to Builds Esc");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/builds/pipe-1");
});

test("a run opened from another run's link: Esc returns to that run, and a second Esc to that run's own opener", async () => {
  start({ nav: windowNav });
  await at("#/job/PAY-231");
  await at("#/build/pipe-1/run-4811");
  await at("#/build/pipe-1/run-4812");
  expect(escHint()).toMatch(/^Back to Run\b/);
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/build/pipe-1/run-4811");
  expect(escHint()).toBe("Back to Job Esc");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/job/PAY-231");
});

test("a run reached by ⌘K or by a deep link: Esc goes to Builds, its structural parent", async () => {
  start({ nav: windowNav });
  await at("#/job/PAY-231");
  // The launcher's pick, as `App.svelte` makes it.
  goWithoutOpener("#/build/pipe-1/run-4812");
  await settle();
  expect(escHint()).toBe("Back to Builds Esc");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/builds");

  unmount(shell!);
  shell = null;
  await openedOn("#/build/pipe-1/run-4812");
  start({ nav: windowNav });
  await settle();
  expect(escHint()).toBe("Back to Builds Esc");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/builds");
});

test("a pull request opened from Code, switched to its Files tab: Esc returns to Code, not to the previous tab", async () => {
  start({ nav: windowNav });
  await at("#/code/repos");
  await at("#/pr/gh:pr-12");
  await at("#/pr/gh:pr-12/files");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/code/repos");
});

test("after an Esc that stepped back, ⌘] goes forward to the detail again", async () => {
  start({ nav: windowNav });
  await at("#/job/PAY-231");
  await at("#/build/pipe-1/run-4812");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/job/PAY-231");
  press("]", { metaKey: true });
  await settle();
  expect(location.hash).toBe("#/build/pipe-1/run-4812");
  expect(escHint(), "the entry kept its own opener").toBe("Back to Job Esc");
});

test("after ⌘[ and ⌘] through history, Esc on an entry still uses that entry's own opener", async () => {
  start({ nav: windowNav });
  await at("#/builds");
  await at("#/build/pipe-1/run-4811");
  await at("#/job/PAY-231");
  await at("#/build/pipe-1/run-4812");
  press("[", { metaKey: true });
  await settle();
  press("[", { metaKey: true });
  await settle();
  expect(location.hash).toBe("#/build/pipe-1/run-4811");
  expect(escHint()).toBe("Back to Builds Esc");
  press("]", { metaKey: true });
  await settle();
  press("]", { metaKey: true });
  await settle();
  expect(location.hash).toBe("#/build/pipe-1/run-4812");
  expect(escHint()).toBe("Back to Job Esc");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/job/PAY-231");
});

test("a top page never navigates on Esc, even when it has an opener", async () => {
  start({ nav: windowNav });
  await at("#/build/pipe-1/run-4812");
  await at("#/inbox");
  const event = press("Escape");
  await settle();
  expect(event.defaultPrevented).toBe(false);
  expect(location.hash).toBe("#/inbox");
  expect(escHint()).toBeNull();
});

test("the breadcrumb on a full view shows the path with no job chip, and its hint names where Esc goes", async () => {
  start();
  await at("#/job/PAY-231");
  await reached("#/asset/assets:svc-1");
  let crumbs = seen(".tf-crumbs")[0]!;
  expect(crumbs.querySelector(".tf-jobchip"), "no last job's chip").toBeNull();
  const parent = crumbs.querySelector<HTMLAnchorElement>("a.tf-crumb")!;
  expect(parent.textContent).toBe("Dependencies");
  expect(parent.getAttribute("href")).toBe("#/deps");
  expect(escHint()).toBe("Back to Dependencies Esc");

  await reached("#/settings/appearance");
  crumbs = seen(".tf-crumbs")[0]!;
  expect(crumbs.querySelector(".tf-jobchip")).toBeNull();
  expect(crumbs.querySelector("strong")?.textContent).toBe("Appearance");
  expect(escHint()).toBe("Back to Settings Esc");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/settings");
});

test("a commit's page with no opener names its repository's page as the breadcrumb's map calls it, until the page has its title", async () => {
  start();
  await reached("#/repo/r1/commits/c1");
  expect(escHint()).toBe("Back to Repository Esc");
});

test("an address the new shell has no place for names itself", async () => {
  start();
  // Link sets was the last section still drawn as "not built yet" (#866).
  await at("#/sets");
  expect(target.querySelector(".tf-fhead h1")?.textContent).toBe("Link sets");
  expect(target.textContent).not.toContain("is not built in the new shell yet");
  await at("#/nowhere");
  expect(seen(".tf-empty")[0]?.textContent).toContain("Nothing lives at #/nowhere in the new shell.");
});

// #1085: a rooms-UI address lands where the new shell keeps that thing, in
// place, so Back does not return to the dead address.
test("a rooms-UI address is replaced by the ticket-first one", async () => {
  start();
  await at("#/tickets");
  const depth = history.length;
  await at("#/ctx/all");
  expect(location.hash).toBe("#/tickets");
  await at("#/start-work/jira:PAY-231");
  expect(location.hash).toBe("#/job/jira%3APAY-231");
  await at("#/sources");
  expect(location.hash).toBe("#/settings/sources");
  expect(target.textContent).not.toContain("Nothing lives at");
  expect(history.length - depth, "each old address was a step, its replacement was not").toBe(3);
});

// #1459: the Assets view's old *Monitors* tab address opens Monitors at its
// own address, lit in the nav, rather than the Tree under a dead tab.
test("the Assets view's old Monitors tab address lands on Monitors", async () => {
  start();
  await at("#/assets/monitors");
  expect(location.hash).toBe("#/monitors");
  const lit = target.querySelector(".tf-nav a[aria-current='page']");
  expect(lit?.firstChild?.textContent?.trim()).toBe("Monitors");
  expect(target.querySelector(".tf-asset-tree")).toBeNull();
  expect(target.textContent).not.toContain("Nothing lives at");
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

// ---- home and the chosen job (spec #2227, #2230) ---------------------------

test("the nav reads Job first, then Tickets and the rest in order; Settings stays the cog", async () => {
  start();
  await at("#/job");
  const names = seen<HTMLAnchorElement>(".tf-nav a").map((a) => a.firstChild?.textContent?.trim());
  expect(names).toEqual(["Job", "Tickets", "Inbox", "Code", "Builds", "Docs", "Dependencies", "Monitors", "Link sets", "Proposed links", "Time"]);
  expect(target.querySelector(".tf-top [aria-label='Settings']"), "Settings is the cog, not a nav entry").not.toBeNull();
});

/** The bare `#/job`'s empty state, or `null` where it is not drawn. */
function emptyHome(): HTMLElement | null {
  return seen(".tf-full").find((page) => page.querySelector("h1")?.textContent === "Job" && page.querySelector(".tf-empty") !== null) ?? null;
}

test("with no chosen job, #/job says how a ticket becomes the job, and its button goes to Tickets", async () => {
  start();
  await at("#/job");
  const home = emptyHome();
  expect(home, "the empty state").not.toBeNull();
  expect(home!.textContent).toMatch(/start the timer on it/i);
  expect(home!.textContent).toMatch(/Start work/);
  const button = [...home!.querySelectorAll<HTMLAnchorElement>("a.tf-btn")].find((a) => a.textContent?.trim() === "All tickets")!;
  expect(button.getAttribute("href")).toBe("#/tickets");
  expect(location.hash).toBe("#/job");
});

/** A browser storage that keeps what the shell wrote, across remounts. */
function installStorage(): Map<string, string> {
  const kept = new Map<string, string>();
  const storage = {
    getItem: (key: string) => kept.get(key) ?? null,
    setItem: (key: string, value: string) => void kept.set(key, String(value)),
    removeItem: (key: string) => void kept.delete(key),
  };
  Object.defineProperty(window, "localStorage", { value: storage, configurable: true, writable: true });
  return kept;
}

function removeStorage(): void {
  Object.defineProperty(window, "localStorage", { value: undefined, configurable: true, writable: true });
}

/** Unmount the shell and mount it again on `hash`, as a restart does. */
async function restartOn(hash: string): Promise<void> {
  if (shell) unmount(shell);
  shell = null;
  location.hash = hash;
  start();
  await settle();
}

test("opening #/job/<id> makes it the chosen job: after a restart with the storage kept, #/job draws it and keeps its address", async () => {
  installStorage();
  try {
    start();
    await at("#/job/PAY-231");
    expect(target.querySelector(".tf-jobcard .tf-jobkey")?.textContent).toBe("PAY-231");
    await restartOn("#/job");
    expect(seen(".tf-jobcard .tf-jobkey").map((key) => key.textContent)).toEqual(["PAY-231"]);
    expect(emptyHome(), "the chosen job, not the empty state").toBeNull();
    expect(location.hash, "#/job stays the home address").toBe("#/job");
  } finally {
    removeStorage();
  }
});

test("a browser storage that throws, or is empty, leaves the shell working with no chosen job", async () => {
  Object.defineProperty(window, "localStorage", {
    value: {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
      removeItem: () => {
        throw new Error("blocked");
      },
    },
    configurable: true,
    writable: true,
  });
  try {
    start();
    await at("#/job");
    expect(emptyHome(), "a failed read: no chosen job").not.toBeNull();
    await at("#/job/PAY-231");
    expect(target.querySelector(".tf-jobcard .tf-jobkey")?.textContent, "a failed write still opens the job").toBe("PAY-231");
  } finally {
    removeStorage();
  }
  installStorage();
  try {
    await restartOn("#/job");
    expect(emptyHome(), "an empty storage: no chosen job").not.toBeNull();
  } finally {
    removeStorage();
  }
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
  expect(target.querySelector(".tf-nav a:nth-child(1)")?.textContent?.replace(/\s+/g, " ").trim(), "with no chosen job, the Job tab names the timer's ticket").toBe("Job PAY-231");
});

test("the chip's target carries its full text in a title: a label reads in the sans face, a ticket key stays mono (#1430)", async () => {
  const label = "DB config for the migration";
  const timer = {
    current: { target: { kind: "label", label }, started_at: "2026-08-22T12:00:00Z", last_heartbeat: "2026-08-22T12:00:00Z" } as RunningTimer,
    elapsed: "0:05",
  };
  start({ timer });
  await settle();
  const on = target.querySelector<HTMLElement>(".tf-timer-key")!;
  expect(on.textContent).toBe(label);
  expect(on.getAttribute("title")).toBe(label);
  expect(on.classList.contains("tf-mono"), "a label is not a code value").toBe(false);
});

test("a timer on a ticket key keeps the mono face and its title (#1430)", async () => {
  const timer = {
    current: { target: { kind: "entity", entity_id: "jira:PAY-231" }, started_at: "2026-08-22T12:00:00Z", last_heartbeat: "2026-08-22T12:00:00Z" } as RunningTimer,
    elapsed: "0:05",
  };
  start({ timer });
  await settle();
  const on = target.querySelector<HTMLElement>(".tf-timer-key")!;
  expect(on.classList.contains("tf-mono")).toBe(true);
  expect(on.getAttribute("title")).toBe("PAY-231");
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

test("history stays put while a dialog is open, keys and mouse alike, and moves again once it closes", () => {
  const nav = { back: vi.fn(), forward: vi.fn() };
  const layers = createLayers();
  const offDialog = layers.add({ kind: "dialog", close: () => offDialog() });
  start({ nav, layers });
  expect(press("[", { metaKey: true }).defaultPrevented, "the webview still adds no navigation").toBe(true);
  press("ArrowRight", { metaKey: true });
  window.dispatchEvent(new MouseEvent("mouseup", { button: 3, bubbles: true, cancelable: true }));
  expect(nav.back).not.toHaveBeenCalled();
  expect(nav.forward).not.toHaveBeenCalled();
  offDialog();
  press("[", { metaKey: true });
  expect(nav.back).toHaveBeenCalledOnce();
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

test("a source removed or added leaves or joins the status bar without a reload (#2059)", async () => {
  // No event says a source was removed: Settings re-lists the sources and
  // hands the health store the new membership, as `TfSettings.load` does.
  const store = createHealth({ credentialHealth: () => Promise.resolve([]), listen: () => Promise.resolve(() => {}) });
  store.replace([health("jira", "ok"), health("tide", "ok")]);
  let listed = [
    { id: "jira", display_name: "Jira" },
    { id: "tide", display_name: "Tidewater Gitea" },
  ];
  start({ health: store, listSources: () => Promise.resolve(listed.map((s) => ({ roles: [], role_misses: [], ...s }))) });
  await settle();
  const bar = () => target.querySelector(".tf-status")!.textContent;
  expect(bar()).toContain("Tidewater Gitea not synced yet");

  // Remove, or Uninstall with either answer: the source is gone.
  listed = [{ id: "jira", display_name: "Jira" }];
  store.replace([health("jira", "ok")]);
  await settle();
  expect(bar()).toContain("Jira synced 4 min ago");
  expect(bar()).not.toContain("Tidewater Gitea");

  // Add source, or a restore: it is there again.
  listed = [...listed, { id: "tide", display_name: "Tidewater Gitea" }];
  store.replace([health("jira", "ok"), health("tide", "ok")]);
  await settle();
  expect(bar()).toContain("Tidewater Gitea not synced yet");
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

/** The write queue panel, as the shell draws it. */
function queuePanel(): HTMLElement | null {
  return target.querySelector<HTMLElement>(".tf-wq-dialog");
}

/** Open the panel from the bar, and close it whatever the test did (#1474). */
async function withQueueOpen(run: () => Promise<void>) {
  try {
    queueControl()!.click();
    flushSync();
    expect(queuePanel()).not.toBeNull();
    await run();
  } finally {
    writeQueuePanel.open = false;
    flushSync();
  }
}

test("Esc on the window closes the write queue panel and leaves the page where it is (#1474)", async () => {
  start();
  await at("#/job/PAY-231");
  await withQueueOpen(async () => {
    // Focus outside the panel, as after the live pass's clicks: the window's
    // Esc, not the dialog's own.
    (document.activeElement as HTMLElement | null)?.blur();
    press("Escape");
    await settle();
    expect(writeQueuePanel.open).toBe(false);
    expect(queuePanel()).toBeNull();
    expect(location.hash).toBe("#/job/PAY-231");
  });
});

test("a route change closes the write queue panel (#1474)", async () => {
  start();
  await at("#/job/PAY-231");
  await withQueueOpen(async () => {
    await at("#/tickets");
    expect(writeQueuePanel.open).toBe(false);
    expect(queuePanel()).toBeNull();
  });
});

test("a dialog opened over the write queue panel closes first on Esc, then the panel (#1474)", async () => {
  const layers = createLayers();
  start({ layers });
  await at("#/job/PAY-231");
  await withQueueOpen(async () => {
    const closed: string[] = [];
    const off = layers.add({ kind: "dialog", close: () => (closed.push("draft"), off()) });
    press("Escape");
    await settle();
    expect(closed).toEqual(["draft"]);
    expect(writeQueuePanel.open, "the panel under it stays").toBe(true);
    press("Escape");
    await settle();
    expect(writeQueuePanel.open).toBe(false);
    expect(layers.open).toEqual([]);
    expect(location.hash).toBe("#/job/PAY-231");
  });
});

/**
 * A rooms-UI `Modal` under this shell (the asset tree's dialogs, #1527) is a
 * layer like a `TfDialog`: with focus outside it, the window's `Esc` closes it
 * and the page stays, and history does not move while it is up (#1086).
 */
test("Esc on the window closes a Modal and leaves the page; history waits for it (#1527)", async () => {
  const nav = { back: vi.fn(), forward: vi.fn() };
  const layers = createLayers();
  start({ nav, layers });
  await at("#/job/PAY-231");
  const host = document.createElement("div");
  document.body.appendChild(host);
  let modal: ReturnType<typeof mount> | null = null;
  const onclose = vi.fn(() => {
    if (modal !== null) unmount(modal);
    modal = null;
  });
  try {
    modal = mount(Modal, { target: host, props: { onclose, layers } });
    flushSync();
    (document.activeElement as HTMLElement | null)?.blur();

    press("[", { metaKey: true });
    window.dispatchEvent(new MouseEvent("mouseup", { button: 3, bubbles: true, cancelable: true }));
    expect(nav.back, "history stays put under the dialog").not.toHaveBeenCalled();

    press("Escape");
    await settle();
    expect(onclose).toHaveBeenCalledOnce();
    expect(host.querySelector('[role="dialog"]')).toBeNull();
    expect(layers.open).toEqual([]);
    expect(location.hash).toBe("#/job/PAY-231");

    press("[", { metaKey: true });
    expect(nav.back, "and moves again once it is gone").toHaveBeenCalledOnce();
  } finally {
    if (modal !== null) unmount(modal);
    host.remove();
  }
});

/**
 * The bar fits its width (#1410): the healthy readings share one run that
 * shrinks with an ellipsis, and its title keeps every word. A failed
 * credential and the queue control sit outside it, so they never shrink.
 */
test("only the healthy readings shrink; a failed credential and the queue stay whole", async () => {
  start({
    sources: [
      { id: "jira", display_name: "Jira" },
      { id: "teamcity", display_name: "TeamCity" },
      { id: "gitea", display_name: "Gitea" },
    ],
  });
  await settle();
  const bar = target.querySelector(".tf-status")!;
  const run = bar.querySelector<HTMLElement>(".tf-healthy")!;
  expect(run.textContent).toContain("Jira synced 4 min ago");
  expect(run.textContent).toContain("Gitea not synced yet");
  expect(run.title).toBe("Jira synced 4 min ago · Gitea not synced yet");
  expect(run.querySelector(".tf-bad")).toBeNull();
  expect(run.querySelector(".tf-queued")).toBeNull();
  expect(bar.querySelector(".tf-bad")?.textContent).toContain("Re-enter password");
  expect(bar.querySelector(".tf-queued")).not.toBeNull();
});

/** The bar's queue control (#1409): what waits, and what needs the reader. */
function queueControl(): HTMLButtonElement | null {
  return target.querySelector<HTMLButtonElement>(".tf-status button.tf-queued");
}

test("held and refused writes make the queue control a button that opens the queue", async () => {
  start({ counts: { pending: 0, held: 1, refused: 2 } });
  await settle();
  try {
    const control = queueControl()!;
    expect(control.textContent?.replace(/\s+/g, " ").trim()).toBe("0 queued writes · 3 need you");
    expect(control.querySelector(".tf-need")?.textContent).toBe("3 need you");
    expect(control.title).toBe("Writes knobas still owes a source. Open to see what is queued.");
    control.click();
    flushSync();
    expect(writeQueuePanel.open).toBe(true);
  } finally {
    writeQueuePanel.open = false;
    flushSync();
  }
});

test("the queue control counts waiting writes and decisions together", async () => {
  start({ counts: { pending: 1, held: 1, refused: 2 } });
  await settle();
  expect(queueControl()?.textContent?.replace(/\s+/g, " ").trim()).toBe("1 queued write · 3 need you");
});

test("with no decision owed the queue control says nothing about one", async () => {
  start({ counts: { pending: 2, held: 0, refused: 0 } });
  await settle();
  expect(queueControl()?.textContent?.trim()).toBe("2 queued writes");
  expect(target.querySelector(".tf-status")!.textContent).not.toContain("need you");
});

test("an empty queue is plain words, not a button", async () => {
  start({ counts: { pending: 0, held: 0, refused: 0 } });
  await settle();
  expect(queueControl()).toBeNull();
  expect(target.querySelector(".tf-status .tf-queue")?.textContent?.trim()).toBe("No queued writes");
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
  await reached("#/build/pipe-1/run-4812");
  expect(target.querySelector(".tf-crumbs .tf-jobchip")).toBeNull();
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/builds");
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

/**
 * `PAY-231` under two sources (#1476, #1525): Jira's, listed first, and
 * Tidewater's, which a bare `#/job/PAY-231` would never open.
 */
const TWO_SOURCE_PORTS: JobPorts = {
  ...JOB_PORTS,
  listSources: () => Promise.resolve([sourceFacts("jira"), sourceFacts("tide")]),
  getEntity: (id) =>
    id === "tide:PAY-231"
      ? JOB_PORTS.getEntity("jira:PAY-231").then((detail) => ({
          ...detail,
          row: { ...detail.row, entity_id: id, source_id: "tide" },
          source: { id: "tide", display_name: "Tidewater Jira", adapter_kind: "jira", enabled: true },
        }))
      : JOB_PORTS.getEntity(id),
};

/** The Job tab's link: it names the job once one is opened. */
function jobTab(): HTMLAnchorElement | undefined {
  return [...target.querySelectorAll<HTMLAnchorElement>(".tf-nav a")].find((a) => a.textContent?.includes("PAY-231"));
}

test("a job opened at its source's address is the one the Job tab returns to (#1525)", async () => {
  start({ jobPorts: TWO_SOURCE_PORTS });
  await at("#/job/tide:PAY-231");
  for (const view of ["#/builds", "#/time/2026-08-22", "#/settings"]) {
    await at(view);
    expect(jobTab()?.getAttribute("href"), view).toBe(jobHash("tide:PAY-231"));
    expect(seen(".tf-crumbs .tf-jobchip"), `${view}: the breadcrumb names no job (#2228)`).toEqual([]);
  }
});

test("the Day tab's Add to writes to the ticket of the address the job was opened at (#1525)", async () => {
  created.length = 0;
  start({ jobPorts: TWO_SOURCE_PORTS });
  await at("#/job/tide:PAY-231");
  await at("#/time/2026-08-22");
  expect(addToButton()?.textContent?.trim()).toBe("Add to PAY-231");
  addToButton()!.click();
  await settle();
  expect(created).toEqual([{ kind: "entity", entity_id: "tide:PAY-231" }]);
});

test("a job opened by its bare key is chosen as the ticket it resolved to, the first listed source's (#1525, spec #2227)", async () => {
  start({ jobPorts: TWO_SOURCE_PORTS });
  await at("#/job/tide:PAY-231");
  await at("#/job/PAY-231");
  await at("#/settings");
  expect(jobTab()?.getAttribute("href"), "same key, another source: the one resolved last is chosen").toBe(jobHash("jira:PAY-231"));
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

/** The job ports, counting the ticket reads the nav job and the job page make (#1687). */
function countingPorts(): { reads: { listSources: number; getEntity: number }; jobPorts: JobPorts } {
  const reads = { listSources: 0, getEntity: 0 };
  const jobPorts: JobPorts = {
    ...JOB_PORTS,
    listSources: () => {
      reads.listSources += 1;
      return JOB_PORTS.listSources();
    },
    getEntity: (id) => {
      reads.getEntity += 1;
      return JOB_PORTS.getEntity(id);
    },
  };
  return { reads, jobPorts };
}

test("a timer start and stop with a job last opened re-reads no ticket for the nav job (#1687)", async () => {
  const { reads, jobPorts } = countingPorts();
  const timer = $state<{ current: RunningTimer | null; elapsed: string | null }>({ current: null, elapsed: null });
  start({ jobPorts, timer });
  await at("#/job/PAY-231");
  await at("#/time/2026-08-22");
  const before = { ...reads };
  expect(before.listSources, "the nav job was read once").toBeGreaterThan(0);

  timer.current = { target: { kind: "entity", entity_id: "jira:PAY-231" }, started_at: "2026-08-22T12:00:00Z", last_heartbeat: "2026-08-22T12:00:00Z" } as RunningTimer;
  await settle();
  timer.current = null;
  await settle();
  expect(reads, "a timer start and stop costs the nav job no IPC").toEqual(before);
});

test("a tab switch on a job page re-reads no ticket for the nav job; another job reads once (#1987)", async () => {
  const { reads, jobPorts } = countingPorts();
  start({ jobPorts });
  await at("#/job/PAY-231");
  const opened = { ...reads };
  expect(opened.listSources, "the job was read").toBeGreaterThan(0);

  await at("#/job/PAY-231/conversation");
  await at("#/job/PAY-231");
  expect(reads, "a tab switch on the same job reads no ticket").toEqual(opened);

  await at("#/job/PAY-232");
  await at("#/job/PAY-232/conversation");
  expect(reads.listSources - opened.listSources, "another job is resolved once, by its page: the chosen job is an id, read by nobody").toBe(1);
});

test("a job left before its page resolved is not chosen: the job chosen before stays, and the one resolved on screen replaces it (#1782, spec #2227)", async () => {
  let release: () => void = () => {};
  let hold: Promise<void> | null = null;
  const jobPorts: JobPorts = {
    ...TWO_SOURCE_PORTS,
    listSources: async () => {
      if (hold !== null) await hold;
      return TWO_SOURCE_PORTS.listSources();
    },
  };
  created.length = 0;
  start({ jobPorts });
  await at("#/job/PAY-231");
  await at("#/time/2026-08-22");
  expect(addToButton()?.textContent?.trim(), "the bare key resolved to Jira's ticket").toBe("Add to PAY-231");

  hold = new Promise<void>((resolve) => (release = resolve));
  await at("#/job/tide:PAY-231");
  await at("#/time/2026-08-22");
  release();
  hold = null;
  await settle();
  addToButton()!.click();
  await settle();
  expect(created, "Tidewater's never drew, so Jira's is still the chosen job").toEqual([{ kind: "entity", entity_id: "jira:PAY-231" }]);

  created.length = 0;
  await at("#/job/tide:PAY-231");
  await at("#/time/2026-08-22");
  addToButton()!.click();
  await settle();
  expect(created, "Tidewater's drew: it is the chosen job").toEqual([{ kind: "entity", entity_id: "tide:PAY-231" }]);
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

test("the shell hands the source list's role claims to the surfaces (#1196)", async () => {
  start({
    sources: [
      { id: "forge", display_name: "Acme Forge", roles: [{ role: "code-host", reads: [] }] },
      { id: "jira", display_name: "Jira", roles: [{ role: "ticket-system", reads: ["reachable_transitions"] }] },
    ],
  });
  await at("#/tickets");

  expect(sourceRoles.answers("ticket-system", "jira", "reachable_transitions")).toBe(true);
  expect(sourceRoles.answers("code-host", "jira", "reachable_transitions")).toBe(false);
});

test("a finished sync reads the source list again, so a role its records missed stops answering (#1199)", async () => {
  const claim = [{ role: "ticket-system", reads: ["reachable_transitions"] }];
  const miss = { role: "ticket-system", kind: "ticket", field: "priority", path: "fields.priority.name" };
  let answer: Awaited<ReturnType<TfShellPorts["listSources"]>> = [{ id: "jira", display_name: "Jira", roles: claim, role_misses: [] }];
  const handlers = new Map<string, (event: { payload: unknown }) => void>();
  start({
    listSources: () => Promise.resolve(answer),
    listen: (event, handler) => (handlers.set(event, handler), Promise.resolve(() => {})),
  });
  await tick();
  expect(sourceRoles.answers("ticket-system", "jira", "reachable_transitions")).toBe(true);

  const row = (running: boolean) => ({
    source_id: "jira", running, run_id: 2, started_at: null, last_finished_at: null,
    last_outcome: "ok", next_run_at: null, backoff_until: null, last_changed: null,
  });
  answer = [{ id: "jira", display_name: "Jira", roles: claim, role_misses: [miss] }];
  handlers.get("sync:state")!({ payload: row(true) });
  handlers.get("sync:state")!({ payload: row(false) });
  await tick();
  expect(sourceRoles.answers("ticket-system", "jira", "reachable_transitions")).toBe(false);
});

test("an item of a custom-role source opens its block view, and the role gets no nav entry (#1201)", async () => {
  start({
    sources: [
      { id: "jira", display_name: "Jira", roles: [{ role: "ticket-system", reads: ["reachable_transitions"] }] },
      { id: "surveys", display_name: "Harbour surveys", roles: [{ role: "wasm-fixture:survey-desk", reads: [] }] },
    ],
  });
  // Opened as every surface opens an item: by its kind's address.
  await at(entityAddress("survey", "surveys:survey-1"));

  const main = target.querySelector(".tf-full")!;
  expect(main.querySelector("h1")?.textContent).toBe("Quay crane 4 inspection");
  expect([...main.querySelectorAll("dt")].map((dt) => dt.textContent)).toEqual(["Surveyor"]);
  expect(main.textContent).toContain("hoist brake worn, booked for Tuesday");

  const nav = target.querySelector('nav[aria-label="Main"]')!;
  const entries = [...nav.querySelectorAll("a")];
  expect(entries.map((a) => a.textContent?.replace(/\d+$/, "").trim())).toEqual(NAV.map((section) => SECTION_NAME[section]));
  expect(nav.textContent).not.toMatch(/survey/i);
  expect(entries.filter((a) => a.getAttribute("aria-current") === "page")).toEqual([]);
});

test("an item's page draws the kind's label as its breadcrumb; with no parent and no opener, Esc does nothing and no hint names it (#1408)", async () => {
  start({ sources: [{ id: "surveys", display_name: "Harbour surveys", roles: [{ role: "wasm-fixture:survey-desk", reads: [] }] }] });
  await at("#/job/PAY-231");
  const item = entityAddress("survey", "surveys:survey-1");
  await reached(item);

  const crumbs = seen(".tf-crumbs")[0]!;
  expect(crumbs.querySelector(".tf-jobchip")).toBeNull();
  expect(crumbs.querySelector("strong")?.textContent).toBe("Survey");
  expect(escHint()).toBeNull();
  expect(target.querySelector(".tf-fhead h1")?.textContent).toBe("Quay crane 4 inspection");

  const event = press("Escape");
  await settle();
  expect(event.defaultPrevented).toBe(false);
  expect(location.hash).toBe(item);
});

test("a note's page has no parent: with no opener Esc does nothing and no hint names it; opened from the Job page, it returns there", async () => {
  start({ nav: windowNav });
  await at("#/job/PAY-231");
  await reached("#/note/note-1");
  expect(escHint()).toBeNull();
  const event = press("Escape");
  await settle();
  expect(event.defaultPrevented).toBe(false);
  expect(location.hash).toBe("#/note/note-1");

  await at("#/job/PAY-231");
  await at("#/note/note-1");
  expect(escHint()).toBe("Back to Job Esc");
  press("Escape");
  await settle();
  expect(location.hash).toBe("#/job/PAY-231");
});

// #1415: a window opened on an old address (a bookmark, a notification) lands
// on the mapped page; the rewrite must not depend on a listener added later.
/** The window's first address: no `hashchange` is queued, as when a window opens on a bookmark. */
async function openedOn(hash: string) {
  // `beforeEach` set the hash, and jsdom delivers that `hashchange` a task later: let it pass before mounting.
  await new Promise((resolve) => setTimeout(resolve, 0));
  history.replaceState(null, "", hash);
}
const current = () => target.querySelector(".tf-nav a[aria-current='page']")?.textContent?.trim() ?? "";

test("a window opened on #/ctx/all draws Tickets and rewrites the address", async () => {
  await openedOn("#/ctx/all");
  start();
  await settle();
  expect(location.hash).toBe("#/tickets");
  expect(target.textContent).not.toContain("Nothing lives at");
  expect(current()).toContain("Tickets");
});

test("a window opened on #/start-work/<key> draws that job", async () => {
  await openedOn("#/start-work/PAY-231");
  start();
  await settle();
  expect(location.hash).toBe("#/job/PAY-231");
  expect(target.textContent).not.toContain("Nothing lives at");
  expect(target.querySelector(".tf-jobcard .tf-jobkey")?.textContent).toBe("PAY-231");
});

test("a window opened on #/standup draws Time", async () => {
  await openedOn("#/standup");
  start();
  await settle();
  expect(location.hash).toBe("#/time/standup");
  expect(target.textContent).not.toContain("Nothing lives at");
  expect(current()).toContain("Time");
});

/** The launcher reads its sources' last runs from the shell's own (#1581): the `dialogs` snippet is handed them. */
test("the dialogs snippet receives the sources' run statuses the shell holds (#1581)", async () => {
  let runs: () => SourceSyncStatus[] = () => [];
  const dialogs = createRawSnippet((given: () => SourceSyncStatus[]) => {
    runs = given;
    return { render: () => "<i data-runs></i>" };
  });
  start({ dialogs });
  await settle();
  expect(target.querySelector("[data-runs]")).not.toBeNull();
  expect(runs().map((row) => `${row.source_id}:${row.last_outcome}`)).toEqual(["jira:ok"]);
});

// ---- *Sync now* in every role view's header (#2106) ----

/** Every built-in role, as one source claims it. */
const ALL_ROLES: RoleClaim[] = ["ticket-system", "wiki", "ci", "monitoring", "code-host"].map((role) => ({ role, reads: [] }));

/** The role views, each with the role its page names. */
const ROLE_VIEWS = ["#/tickets", "#/docs", "#/builds", "#/monitors", "#/code"];

/** Role syncs whose `sync_role` answers one run of `mock`, and whose `sync:state` the test sends. */
function roleSyncHarness() {
  const asked: string[] = [];
  const handlers = new Set<(event: { payload: unknown }) => void>();
  const ports: RoleSyncPorts = {
    syncRole: (role) => {
      asked.push(role);
      return Promise.resolve([{ source_id: "mock", run_id: 40 + asked.length }]);
    },
    syncNow: () => Promise.reject(new Error("a role view never runs one source")),
    syncStatus: () => Promise.resolve([]),
    listen: (_event, handler) => {
      handlers.add(handler);
      return Promise.resolve(() => handlers.delete(handler));
    },
  };
  const end = (run_id: number) => {
    const row: SourceSyncStatus = { source_id: "mock", running: false, run_id, started_at: null, last_finished_at: null, last_outcome: "ok", next_run_at: null, backoff_until: null, last_changed: null };
    for (const handler of [...handlers]) handler({ payload: row });
  };
  return { asked, syncs: createRoleSyncs(ports), end };
}

function syncButton(): HTMLButtonElement | null {
  return (
    seen<HTMLButtonElement>("button").find((button) =>
      ["Sync now", "Syncing…"].includes(button.textContent?.trim() ?? ""),
    ) ?? null
  );
}

test("every role view's header draws Sync now, once, and a view of no role draws none", async () => {
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }] });
  for (const view of ROLE_VIEWS) {
    await at(view);
    const buttons = seen("button").filter((button) => button.textContent?.trim() === "Sync now");
    expect(buttons, view).toHaveLength(1);
  }
  await at("#/inbox");
  expect(syncButton()).toBeNull();
});

test("Code's Sync now is a default-size header button, last after New pull request (#2238, #2295)", async () => {
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }] });
  await at("#/code");
  const acts = seen(".tf-fhead .tf-acts").find((el) => el.contains(syncButton()));
  const names = [...(acts?.children ?? [])].map((el) => el.textContent?.trim());
  expect(names).toEqual(["New pull request", "Sync now"]);
  expect(syncButton()!.classList.contains("tf-btn")).toBe(true);
  expect(syncButton()!.classList.contains("tf-sm"), "a header button is default size").toBe(false);
});

test("a role with no enabled source draws no Sync now", async () => {
  start({
    sources: [
      { id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: false },
      { id: "jira", display_name: "Jira", roles: [{ role: "ticket-system", reads: [] }], enabled: true },
    ],
  });
  await at("#/tickets");
  expect(syncButton()?.textContent?.trim()).toBe("Sync now");
  for (const view of ROLE_VIEWS.slice(1)) {
    await at(view);
    expect(syncButton(), view).toBeNull();
  }
});

test("Sync now says Syncing… and is disabled until its runs end, then returns", async () => {
  const { asked, syncs, end } = roleSyncHarness();
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }], roleSyncs: syncs });
  await at("#/docs");
  syncButton()!.click();
  await settle();
  expect(asked).toEqual(["wiki"]);
  expect(syncButton()?.textContent?.trim()).toBe("Syncing…");
  expect(syncButton()?.disabled).toBe(true);

  // Still going on another view of the role? No: each role has its own runs.
  await at("#/builds");
  expect(syncButton()?.textContent?.trim()).toBe("Sync now");
  await at("#/docs");
  expect(syncButton()?.textContent?.trim()).toBe("Syncing…");

  end(41);
  await settle();
  expect(syncButton()?.textContent?.trim()).toBe("Sync now");
  expect(syncButton()?.disabled).toBe(false);
});

test("a view not re-read on a run's end reads again when its Sync now runs end", async () => {
  const { asked, syncs, end } = roleSyncHarness();
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }], roleSyncs: syncs });
  await at("#/tickets");
  const before = boardReads.n;
  syncButton()!.click();
  await settle();
  expect(asked).toEqual(["ticket-system"]);
  expect(boardReads.n).toBe(before);

  end(41);
  await settle();
  expect(boardReads.n).toBeGreaterThan(before);
});

// ---- ⌘R presses the page's Refresh or Sync now (#2156) ----

/** ⌘R, from the focused element as the webview sends it. */
function cmdR(options: KeyboardEventInit = {}) {
  return press("r", { metaKey: true, ...options });
}

test("⌘R on every role view runs its header's Sync now, as a click does, and the webview never reloads", async () => {
  const { asked, syncs } = roleSyncHarness();
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }], roleSyncs: syncs });
  for (const view of ROLE_VIEWS) {
    await at(view);
    expect(cmdR().defaultPrevented, view).toBe(true);
    await settle();
  }
  expect(asked).toEqual(["ticket-system", "wiki", "ci", "monitoring", "code-host"]);
  // Ctrl+R off the Mac, and ⇧⌘R, the webview's other reload.
  await at("#/tickets");
  expect(press("r", { ctrlKey: true }).defaultPrevented).toBe(true);
  expect(press("R", { metaKey: true, shiftKey: true }).defaultPrevented).toBe(true);
});

test("⌘R while Sync now says Syncing… runs nothing more, and a held key presses once", async () => {
  const { asked, syncs, end } = roleSyncHarness();
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }], roleSyncs: syncs });
  await at("#/docs");
  cmdR();
  await settle();
  expect(syncButton()?.disabled).toBe(true);
  expect(cmdR().defaultPrevented).toBe(true);
  await settle();
  expect(asked).toEqual(["wiki"]);

  end(41);
  await settle();
  cmdR({ repeat: true });
  await settle();
  expect(asked).toEqual(["wiki"]);
});

test("⌘R where no button is drawn does nothing, and still never reloads", async () => {
  const { asked, syncs } = roleSyncHarness();
  start({
    sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: false }],
    roleSyncs: syncs,
  });
  for (const view of ["#/docs", "#/settings", "#/time/2026-08-22", "#/inbox"]) {
    await at(view);
    expect(syncButton(), view).toBeNull();
    expect(cmdR().defaultPrevented, view).toBe(true);
    await settle();
  }
  expect(asked).toEqual([]);
});

test("⌘R with the launcher or a dialog open does nothing, and runs again once it closes", async () => {
  const { asked, syncs } = roleSyncHarness();
  const layers = createLayers();
  const offDialog = layers.add({ kind: "dialog", close: () => offDialog() });
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }], roleSyncs: syncs, layers });
  await at("#/builds");
  expect(cmdR().defaultPrevented).toBe(true);
  await settle();
  expect(asked).toEqual([]);

  offDialog();
  cmdR();
  await settle();
  expect(asked).toEqual(["ci"]);
});

test("⌘R in a text field still presses the page's button", async () => {
  const { asked, syncs } = roleSyncHarness();
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }], roleSyncs: syncs });
  await at("#/code");
  const field = filterField("payments");
  expect(cmdR().defaultPrevented).toBe(true);
  await settle();
  expect(asked).toEqual(["code-host"]);
  expect(field.value).toBe("payments");
});

test("⌘R on an item's page runs its Refresh, as a click does", async () => {
  start({ sources: [{ id: "surveys", display_name: "Harbour surveys", roles: [{ role: "wasm-fixture:survey-desk", reads: [] }] }] });
  await at(entityAddress("survey", "surveys:survey-1"));
  const button = [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent?.trim() === "Refresh");
  expect(button).toBeDefined();
  expect(cmdR().defaultPrevented).toBe(true);
  await settle();
  expect(refreshed).toEqual(["surveys:survey-1"]);
  expect(button?.textContent?.trim()).toBe("Refreshing…");
  // Disabled while it reads: a second press asks nothing more.
  cmdR();
  await settle();
  expect(refreshed).toEqual(["surveys:survey-1"]);
});

// ---- a page keep-alive holds hidden (#2247) ----

test("a page kept hidden takes no keys: / focuses nothing in it", async () => {
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }] });
  for (const view of ["#/tickets", "#/docs", "#/builds", "#/deps"]) {
    await at(view);
  }
  for (const view of ["#/time", "#/settings"]) {
    await at(view);
    (document.activeElement as HTMLElement | null)?.blur();
    press("/");
    expect(document.activeElement?.closest("[hidden]") ?? null, view).toBeNull();
  }
});

test("⌘R never presses the Sync now of a page kept hidden", async () => {
  const { asked, syncs } = roleSyncHarness();
  start({ sources: [{ id: "mock", display_name: "Tidewater", roles: ALL_ROLES, enabled: true }], roleSyncs: syncs });
  await at("#/docs");
  await at("#/settings");
  expect(seen(".tf-syncnow")).toHaveLength(0);
  cmdR();
  await settle();
  expect(asked).toEqual([]);
});

test("the stored Pages kept open reaches the page host once the database answers", async () => {
  start({ pagesKeptOpen: () => Promise.resolve(0) });
  await settle();
  await at("#/time");
  await at("#/inbox");
  expect(target.querySelectorAll(".tf-kept"), "none kept besides the page in front").toHaveLength(1);
});

test("the saved command renderers are in the registry once the database answers (#2262)", async () => {
  start({
    userRenderers: () =>
      Promise.resolve([{ id: "plantuml", name: "PlantUML", command: "plantuml -tsvg -pipe", patterns: ["*.puml"], fences: ["plantuml"], input: "text", output: "svg", timeout_secs: 10, diff: false }]),
  });
  try {
    await settle();
    expect(renderersForFence("plantuml").map((r) => r.id)).toEqual(["command:plantuml", "source"]);
  } finally {
    installCommands([]);
  }
});

test("the default renderers per file pattern are held once the database answers (#2263)", async () => {
  start({ rendererDefaults: () => Promise.resolve([{ pattern: "*.md", renderer: "source", pipeline: null }]) });
  await settle();
  expect(rendererDefaults.list).toEqual([{ pattern: "*.md", renderer: "source", pipeline: null }]);
});

test("the named pipelines are held once the database answers, and every diff reads its file's default from them (#2265, #2270)", async () => {
  start({
    rendererDefaults: () => Promise.resolve([{ pattern: "*.json.b64", renderer: "", pipeline: "b64" }]),
    namedPipelines: () => Promise.resolve([{ id: "b64", name: "Base64 JSON", steps: [{ step: "base64_decode" }] }]),
  });
  await settle();
  expect(defaultPipelines.of("fixtures/hooks.json.b64")).toEqual({ name: "Base64 JSON", pattern: "*.json.b64", steps: [{ step: "base64_decode" }] });
  expect(defaultPipelines.of("README.md")).toBeNull();
  // An edit in Settings reaches the diffs: the lookup reads the held list at each ask.
  namedPipelines.hold([{ id: "b64", name: "Base64 JSON", steps: [{ step: "base64_decode" }, { step: "pretty", format: "json" }] }]);
  expect(defaultPipelines.of("x.json.b64")?.steps).toHaveLength(2);
});
