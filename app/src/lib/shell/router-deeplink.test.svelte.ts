/**
 * Every address the app can be *opened at* renders something.
 *
 * The window is hash-addressed (spec §2, "entity addressing as the navigation
 * contract"), which means an address can arrive from outside: a bookmark, a
 * link in a note, an M2 notification firing at an M1 build. None of those may
 * produce a blank window — the failure a person cannot report, because there
 * is nothing on screen to describe.
 *
 * The whole app is mounted, at the address, rather than the router being asked
 * what it parses to: `router.test.svelte.ts` already pins the parse, and the
 * thing this file is about is what a reader *sees*. A route that parses
 * perfectly and renders nothing passes the parse test and fails this one.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, EntityPage } from "../ipc/entity";

const PAGE: EntityPage = {
  rows: [
    {
      entity_id: "mock:PAY-231",
      kind: "ticket",
      source_id: "mock",
      title: "Retry failed SEPA payouts",
      updated_at: "2026-08-22T11:48:00Z",
      synced_at: "2026-08-22T14:30:00Z",
    },
  ],
  total: 1,
};

const DETAIL: EntityDetail = {
  row: PAGE.rows[0]!,
  source: { id: "mock", display_name: "Mock", adapter_kind: "mock", enabled: true },
  kind_info: null,
  body_text: "the SEPA batch",
  author: null,
  payload: {},
  web_url: null,
  deleted_at: null,
  links: [],
  activity: [],
};

vi.mock("../ipc/entity", () => ({
  // The write the ticket detail's status select queues (#179). Not what this
  // file is about, so it refuses.
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  // Contexts (#47): the store imports these at module level, so every mock of
  // this module has to define them even where no context is ever made.
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  // The Tickets tile's read (#178); this file is about addresses, not the
  // board.
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  listEntities: () => Promise.resolve(PAGE),
  getEntity: () => Promise.resolve(DETAIL),
  recentActivity: () => Promise.resolve([]),
}));

vi.mock("../ipc/app", () => ({
  appStatus: () =>
    Promise.resolve({
      db: { state: "ready" },
      // Not a first run: the wizard takes the whole window when it *is* one,
      // and this file is about the addressed surfaces behind it.
      first_run: false,
      demo: false,
      source_count: 1,
      app_version: "0.1.0",
    }),
  frontendReady: () => Promise.resolve(),
  retryDatabase: () => Promise.resolve(),
  completeFirstRun: () => Promise.resolve(),
  ping: () => Promise.resolve("pong"),
}));

vi.mock("../ipc/sources", () => ({
  listSources: () => Promise.resolve([]),
  syncNow: () => Promise.resolve(1),
  syncAll: () => Promise.resolve([]),
  deleteSource: () => Promise.resolve(),
  setSourceSecret: () => Promise.resolve({ source_id: "mock", state: "ok", checked_at: null, detail: null, secret_expires_at: null }),
  credentialHealth: () => Promise.resolve([]),
  syncStatus: () => Promise.resolve([]),
  listSyncRuns: () => Promise.resolve([]),
  dbStats: () => Promise.resolve({ db_bytes: 0, entity_count: 0, item_count: 0, per_source: [], oldest_synced_at: null, newest_synced_at: null }),
  listAdapters: () => Promise.resolve([]),
  addSource: () => Promise.reject(new Error("unused")),
  testSource: () => Promise.reject(new Error("unused")),
  reindexFts: () => Promise.resolve(),
  demoLoad: () => Promise.resolve({ source_id: "mock", upserted: 0, deleted: 0, swept: 0, cursor: "" }),
  syncNowWithProgress: () => Promise.resolve(1),
}));

vi.mock("../ipc/search", () => ({
  search: () =>
    Promise.resolve({
      interpreted: { text: "", prefix: null, filters: { sources: [], kinds: [], updated_within_days: null, mine: false }, unknown_tokens: [] },
      groups: [],
      total: 0,
      took_ms: 1,
    }),
  launcherHome: () => Promise.resolve({ smart_lists: [], recent: [], sources: [], pending_writes: 0 }),
  smartLists: () => Promise.resolve([]),
  smartListItems: () => Promise.resolve([]),
}));

vi.mock("../ipc/backup", () => ({
  backupStatus: () =>
    Promise.resolve({
      schedule: { enabled: true, hour: 3, minute: 0, keep: 7 },
      directory: "/tmp/knobas-backups",
      last: null,
      next_due_at: null,
      archives: [],
    }),
  backupNow: () => Promise.reject(new Error("unused")),
  setBackupSchedule: () => Promise.reject(new Error("unused")),
  restoreBackup: () => Promise.reject(new Error("unused")),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

const { default: App } = await import("../../App.svelte");

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;
/**
 * Errors thrown where nothing was awaiting them — a render, an event handler —
 * which jsdom reports as a `window` `error` rather than as a thrown exception
 * the test could catch.
 *
 * *Rejections* are not recorded here: nothing dispatches
 * `window.unhandledrejection` under Vitest's jsdom, so the listener this file
 * used to carry never fired even once. They are Node's event, and the shared
 * guard in `shell/test-setup.ts` is the one place that watches for them (#103).
 */
const raised: unknown[] = [];

function onerror(event: ErrorEvent) {
  raised.push(event.error ?? event.message);
}

beforeEach(() => {
  raised.length = 0;
  window.addEventListener("error", onerror);
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  window.removeEventListener("error", onerror);
  if (app) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
});

/**
 * Mount the whole app at `hash` and let it finish booting.
 *
 * `App.svelte` awaits a dynamic `import()` before it starts the lifecycle, so
 * the boot screen is what a fixed number of microtasks would measure. Waiting
 * for the boot screen to go is what makes this a test about the addressed
 * surface rather than about how many ticks happened to be enough today.
 */
async function open(hash: string) {
  location.hash = hash;
  app = mount(App, { target });
  flushSync();
  await vi.waitFor(() => {
    flushSync();
    expect(target.querySelector(".booting, .boot")).toBeNull();
    expect((target.textContent ?? "").length).toBeGreaterThan(40);
  });
  flushSync();
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

const ADDRESSES = [
  // M1 surfaces.
  "#/ctx/all",
  "#/ctx/src:jira",
  "#/sources",
  "#/settings",
  "#/first-run",
  "#/ticket/mock:PAY-231",
  // A Gitea key: `#` truncates a fragment at the browser level, so it is
  // percent-encoded in the address and decoded back by the router.
  "#/pr/mock:payout-service%23142",
  "#/entity/mock:c90d11",
  // The inbox is a real view since #45; it is in this list because it has to
  // render from a cold deep link like every other address.
  "#/inbox",
  // M3-M4 addresses, reserved so an open kind never collides with a view.
  "#/time",
  "#/standup",
  "#/assets/board",
  "#/monitor/kuma",
  "#/start-work/mock:PAY-231",
  // And what a person can type.
  "#/nonsense",
  "#/",
  "#/ctx/src:deleted-long-ago",
  "#/ticket/",
  "#/ticket/%zz",
];

test.each(ADDRESSES)("%s renders something and never throws", async (hash) => {
  const text = await open(hash);

  // "Something" means characters a reader can act on, not an empty frame with
  // a status bar bolted to it. The status bar alone is roughly 60 characters,
  // so the floor is above it on purpose.
  expect(text.length, `${hash} rendered a blank window`).toBeGreaterThan(80);
  expect(raised, `${hash} raised`).toEqual([]);
});

test("an address a later milestone owns says which, and offers the way back", async () => {
  // Opened *in* a room and then navigated, which is the shape this arrives in:
  // a notification fires while the reader is somewhere, and `Back` has to
  // return them there. The router is a module singleton, so it also remembers
  // the room across the mounts in this file — starting from a known one is
  // what makes the assertion below about the ladder rather than about
  // whichever address ran last.
  await open("#/ctx/all");
  location.hash = "#/standup";
  window.dispatchEvent(new HashChangeEvent("hashchange"));
  flushSync();

  const text = (target.textContent ?? "").replace(/\s+/g, " ");
  expect(text).toContain("#/standup");
  expect(text).toMatch(/milestone/i);
  const back = [...target.querySelectorAll<HTMLButtonElement>("button")].find(
    (button) => button.textContent?.trim() === "Back to the room",
  );
  expect(back, "a future address is a dead end without a way out").toBeTruthy();

  back!.click();
  flushSync();
  expect(location.hash).toBe("#/ctx/all");
});

test("the sources view is what #/sources renders, not a placeholder", async () => {
  const text = await open("#/sources");
  expect(text).toContain("Sources");
  expect(text).toContain("No sources");
  // The line the placeholder used to carry. Its absence is the seam being
  // real rather than promised.
  expect(text).not.toMatch(/arrives? in phase/i);
});

/**
 * `#/settings` reaches the real §14 surface (#69), not the "arrives in a later
 * milestone" pane the address used to fall through to.
 */
test("the settings view is what #/settings renders, not a later milestone", async () => {
  const text = await open("#/settings");
  expect(text).toContain("Settings");
  expect(text).toContain("Backup");
  // The boundary sentence, all the way through the shell.
  expect(text).toContain("after 03:00");
  expect(text).not.toMatch(/arrives in a later milestone/i);
});

test("the first-run wizard is what #/first-run renders, not a placeholder", async () => {
  const text = await open("#/first-run");
  expect(text).toMatch(/Welcome to knobas/i);
  expect(text).not.toMatch(/arrives? in phase/i);
});

test("a deep link to an entity opens the detail over the room it names", async () => {
  const text = await open("#/ticket/mock:PAY-231");
  expect(text).toContain("Retry failed SEPA payouts");
  // Over the room, not instead of it: Esc has to have somewhere to land.
  expect(target.querySelector(".room")).toBeTruthy();
  expect(target.querySelector('[role="complementary"], .detail')).toBeTruthy();
});
