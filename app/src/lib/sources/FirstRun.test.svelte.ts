/**
 * The first-run wizard — spec §14a: *"initialize the database → add the first
 * source (the §3 flow) → initial sync with progress → land in the launcher."*
 *
 * Two things here are easy to get wrong in a way that looks fine. `sync_now`
 * resolves with the run id **immediately** (P3), so a panel that reported
 * completion from the command resolving would show *Finish* the instant the
 * run started — the progress bar would be decoration over a sync nobody
 * waited for. And a failed first sync must not quietly land the reader in an
 * empty room, which is the state hardest to tell apart from a working knobas
 * that has nothing in it.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { SourceSummary, SyncProgress } from "../ipc/sources";

/**
 * `Channel` reaches into `window.__TAURI_INTERNALS__` in its constructor, so
 * the real one cannot be built under jsdom at all. This stand-in has the only
 * surface the component uses — an assignable `onmessage` — which is also the
 * whole of the contract P3 defines.
 */
vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {
    onmessage: ((progress: SyncProgress) => void) | undefined;
  },
}));

const calls = {
  syncWithProgress: [] as string[],
  demoLoad: 0,
  completeFirstRun: 0,
  listSources: 0,
};

/**
 * The corpus `list_sources` reports — deliberately a different number from
 * anything a run in these tests writes, so a panel reading the run's `upserted`
 * instead cannot pass by coincidence.
 */
let corpus = 213;

/** The channels handed to `sync_now_with_progress`, so a test can drive one. */
const channels: { onmessage?: (progress: SyncProgress) => void }[] = [];
let syncFails: unknown = null;

vi.mock("../ipc/sources", () => ({
  syncNowWithProgress: (sourceId: string, channel: { onmessage?: (p: SyncProgress) => void }) => {
    calls.syncWithProgress.push(sourceId);
    channels.push(channel);
    // P3: the run id, as soon as the run is *recorded*. Not when it finishes.
    return syncFails ? Promise.reject(syncFails) : Promise.resolve(11);
  },
  listSources: () => {
    calls.listSources += 1;
    return Promise.resolve([{ ...summary(), item_count: corpus }, { ...summary("mock"), item_count: corpus }]);
  },
  demoLoad: () => {
    calls.demoLoad += 1;
    return Promise.resolve({ source_id: "mock", upserted: 21, deleted: 0, swept: 0, cursor: "" });
  },
  listAdapters: () => Promise.resolve([]),
  // The wizard hands its new source to the health store, and the store's
  // module reaches for this to build its default port.
  credentialHealth: () => Promise.resolve([]),
  addSource: () => Promise.reject(new Error("the dialog under test does not save here")),
  testSource: () => Promise.reject(new Error("unused")),
}));

vi.mock("../ipc/app", () => ({
  completeFirstRun: () => {
    calls.completeFirstRun += 1;
    return Promise.resolve();
  },
}));

const { default: FirstRun } = await import("./FirstRun.svelte");

function summary(id = "jira"): SourceSummary {
  return {
    id,
    adapter_kind: id,
    display_name: "Tidewater Jira",
    base_url: "https://jira.tidewater.example",
    enabled: true,
    sync_interval_secs: 900,
    config: {},
    health: { source_id: id, state: "ok", checked_at: null, detail: null, secret_expires_at: null },
    last_run: null,
    next_run_at: null,
    item_count: 0,
    kinds: [],
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;
let done = 0;

function render(props: Record<string, unknown> = {}) {
  done = 0;
  app = mount(FirstRun, {
    target,
    props: { demo: false, onfinish: () => (done += 1), ...props },
  });
  flushSync();
  return app;
}

async function settle() {
  for (let i = 0; i < 8; i += 1) await Promise.resolve();
  flushSync();
}

function button(label: string) {
  return [...target.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent?.trim() === label,
  );
}

function step() {
  return target.querySelector(".steps span.on")?.textContent?.trim();
}

function text() {
  return (target.textContent ?? "").replace(/\s+/g, " ");
}

/** Progress messages arrive on the channel, not on the command's promise. */
function progress(over: Partial<SyncProgress>) {
  const channel = channels[channels.length - 1];
  channel?.onmessage?.({
    run_id: 11,
    source_id: "jira",
    phase: "fetching",
    items: 0,
    elapsed_ms: 0,
    message: null,
    ...over,
  });
  flushSync();
}

beforeEach(() => {
  calls.syncWithProgress = [];
  calls.demoLoad = 0;
  calls.completeFirstRun = 0;
  calls.listSources = 0;
  corpus = 213;
  channels.length = 0;
  syncFails = null;
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("the database step is already done, because the shell only gets here once it is", () => {
  render();
  expect(step()).toBe("Database");
  expect(text()).toMatch(/ready/i);
  expect(button("Next")).toBeTruthy();
});

test("the demo profile offers the Tidewater dataset as the first option", async () => {
  render({ demo: true });
  button("Next")!.click();
  flushSync();
  expect(step()).toBe("Source");
  // §14a: in the demo profile the fastest route to a populated window is the
  // fixture, and it is offered *first* because it is what a person opening a
  // demo build came for.
  const first = target.querySelector<HTMLButtonElement>(".modules button")!;
  expect(first.textContent).toMatch(/Tidewater/i);

  first.click();
  await settle();
  expect(calls.demoLoad).toBe(1);
  // Loading the demo set finishes the source step outright — it registers the
  // mock source and syncs it in one call.
  expect(step()).toBe("Done");
  // …and the Done panel counts the mirror, not the run: `demo_load` answers
  // with a `SyncReport`, whose `upserted` is 21 here and is the wrong half.
  expect(text()).toContain("213 items");
});

test("the default profile does not offer demo data at all", () => {
  render({ demo: false });
  button("Next")!.click();
  flushSync();
  // P13: demo data never mixes with a real corpus, so the button that would
  // put it there is not drawn.
  expect(text()).not.toMatch(/Tidewater/i);
});

test("progress from the channel is rendered per phase and item count", async () => {
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  expect(step()).toBe("First sync");

  button("Start the first sync")!.click();
  await settle();
  expect(calls.syncWithProgress).toEqual(["jira"]);

  // While it runs, the run's own count is the only number there is, and it is
  // the honest one: it says how far this run has got.
  progress({ phase: "fetching", items: 40, elapsed_ms: 900 });
  expect(text()).toContain("40 items");
  expect(text()).toMatch(/fetching/i);

  // P3 again: the command already resolved. Only the channel says it is done.
  expect(button("Finish")).toBeUndefined();

  progress({ phase: "finished", items: 7, elapsed_ms: 4200 });
  await settle();
  expect(button("Finish")).toBeTruthy();
  expect(button("Retry")).toBeUndefined();
});

test("the finished panel reports the corpus, never what the run happened to write", async () => {
  // The interleaving ADR-0005 is about: adding a source wakes the scheduler,
  // so the run this wizard is watching may be the one that already found the
  // corpus mirrored and wrote nothing. `CONTEXT.md`: *a run that writes nothing
  // over a full mirror upserted zero* — and *knobas mirrored 0 items* over a
  // full first sync is the sentence this test exists to stop.
  corpus = 213;
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  progress({ phase: "finished", items: 0, elapsed_ms: 4200 });
  await settle();

  expect(calls.listSources).toBe(1);
  expect(text()).toContain("213 items");
  expect(text()).not.toContain("0 items");
});

test("a corpus that cannot be read falls back to the run's count rather than a failure panel", async () => {
  render({ source: summary("nowhere") });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  // `list_sources` answers, but not about this source — the same shape as a
  // read that failed outright. The sync worked, so the panel still offers
  // *Finish*: a count that could not be re-read is not a failed sync.
  progress({ phase: "finished", items: 9, elapsed_ms: 4200 });
  await settle();
  expect(text()).toContain("9 items");
  expect(button("Finish")).toBeTruthy();
  expect(button("Retry")).toBeUndefined();
});

test("a sync that only started does not offer Finish", async () => {
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  // The command has resolved with its run id and nothing else has happened.
  // A panel reading completion off that promise would be showing Finish here,
  // over a sync that has fetched nothing.
  expect(button("Finish")).toBeUndefined();
  expect(text()).toMatch(/syncing|started/i);
});

test("a failed phase offers Retry and Skip, and never silently lands in an empty room", async () => {
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  progress({ phase: "failed", items: 0, elapsed_ms: 300, message: "401 from /rest/api/2/myself" });

  expect(text()).toContain("401 from /rest/api/2/myself");
  expect(button("Retry")).toBeTruthy();
  // Skip exists, but it is a decision the reader makes with the failure in
  // front of them — not a landing that happens on its own.
  expect(button("Skip for now")).toBeTruthy();
  expect(button("Finish")).toBeUndefined();

  button("Retry")!.click();
  await settle();
  expect(calls.syncWithProgress).toEqual(["jira", "jira"]);
});

test("a sync command that rejects outright is reported, not swallowed", async () => {
  syncFails = { code: "not_ready", message: "the scheduler is not running", source_id: "jira" };
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  expect(text()).toContain("the scheduler is not running");
  expect(button("Retry")).toBeTruthy();
});

test("Finish calls complete_first_run and hands the shell back", async () => {
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();
  progress({ phase: "finished", items: 213, elapsed_ms: 4200 });

  button("Finish")!.click();
  await settle();

  expect(calls.completeFirstRun).toBe(1);
  expect(done).toBe(1);
});

test("Skip still records the first run as complete, or the wizard returns for ever", async () => {
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();
  progress({ phase: "failed", items: 0, elapsed_ms: 10, message: "upstream is down" });

  button("Skip for now")!.click();
  await settle();
  // The source is configured; the sync can be retried from the sources view.
  // Not recording completion would show the wizard again on the next launch,
  // over a knobas that is already set up.
  expect(calls.completeFirstRun).toBe(1);
  expect(done).toBe(1);
});

test("the sync step is not reachable before there is a source to sync", () => {
  render({ source: null });
  button("Next")!.click();
  flushSync();
  expect(step()).toBe("Source");
  // Nothing to sync, so the step that syncs is not offered.
  expect(button("Next")).toBeUndefined();
});

test("the progress bar is a native element, never an inline-styled div", async () => {
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();
  progress({ phase: "fetching", items: 40, elapsed_ms: 900 });

  // `style-src 'self'` drops an inline style attribute in a bundle and
  // honours it under `just dev`, so a computed width would work all through
  // development and be flat in the shipped app.
  expect(target.querySelector("progress")).toBeTruthy();
  expect(target.querySelector("[style]")).toBeNull();
});

test("a progress message from a source system is text, never markup", async () => {
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();
  progress({ phase: "failed", items: 0, elapsed_ms: 1, message: "<img src=x onerror=alert(1)>" });
  expect(text()).toContain("<img src=x onerror=alert(1)>");
  expect(target.querySelector("img")).toBeNull();
});
