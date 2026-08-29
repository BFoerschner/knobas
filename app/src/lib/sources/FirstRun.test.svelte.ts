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

/**
 * A latch `list_sources` waits behind, so a test can hold the corpus read open
 * for as long as it likes.
 *
 * The defect #120 records is a **race**, and a race the fast path hides: the
 * panel renders once between the run's ending arriving and the corpus read
 * answering. Under an unlatched mock that gap is a microtask, and every
 * assertion after `settle()` is taken on the far side of it — so a test that
 * proved anything about the gap would have to prove it by inspection. This is
 * how it is proved by observation instead: the read is a real round trip, and
 * a slow one is what a large corpus on a busy machine actually produces.
 */
let corpusGate: { held: Promise<void>; release: () => void } | null = null;

/**
 * How many of the next `list_sources` calls throw before one answers.
 *
 * A *throwing* read is a different interleaving from the one #120 covered and
 * from a read that answers without this source in it, and #137 is about that
 * one: the same round trip, failing. Counted rather than a boolean because the
 * panel retries once, so "fails and then works" and "fails twice" are two
 * behaviours and each needs saying.
 */
let corpusFailures = 0;

function holdTheCorpusRead() {
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  corpusGate = { held, release };
  return corpusGate;
}

vi.mock("../ipc/sources", () => ({
  syncNowWithProgress: (sourceId: string, channel: { onmessage?: (p: SyncProgress) => void }) => {
    calls.syncWithProgress.push(sourceId);
    channels.push(channel);
    // P3: the run id, as soon as the run is *recorded*. Not when it finishes.
    return syncFails ? Promise.reject(syncFails) : Promise.resolve(11);
  },
  listSources: async () => {
    calls.listSources += 1;
    if (corpusGate) await corpusGate.held;
    if (corpusFailures > 0) {
      corpusFailures -= 1;
      throw new Error("list_sources is unavailable");
    }
    return [{ ...summary(), item_count: corpus }, { ...summary("mock"), item_count: corpus }];
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
  corpusGate = null;
  corpusFailures = 0;
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

test("a real source's finished run lands on the DONE step (#156)", async () => {
  // `stepIndex = 3` used to be assigned in exactly one place, `loadDemo`. A real
  // source finished at step 2 and was offered a bare *Finish*, so the sentence
  // three pieces of work were ruled about — #84/ADR-0005 (the count means the
  // corpus), #120 (no microtask flash of *0 items*), #137 (no digit it cannot
  // vouch for) — only ever rendered after a demo load. The breadcrumb promised
  // the step to every user regardless: `STEPS` draws *Done* for a real source
  // too.
  //
  // Click-neutral, which is the whole argument for landing here rather than
  // restating the rulings against the stats row: a finished step 2 offered one
  // button and DONE offers one button. What the reader gains is the sentence
  // and the ⌘K pointer.
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  progress({ phase: "fetching", items: 40, elapsed_ms: 900 });
  expect(step()).toBe("First sync");

  progress({ phase: "finished", items: 7, elapsed_ms: 4200 });
  await settle();
  expect(step()).toBe("Done");
  expect(text()).toContain("knobas mirrored");
  expect(text()).toContain("⌘K");
  // One button before, one button after.
  expect(target.querySelectorAll(".acts button")).toHaveLength(1);
  expect(button("Finish")).toBeTruthy();
});

test("a wizard whose only message is the ending lands on DONE too (#156)", async () => {
  // Off the *ending*, not off having watched the run: ADR-0005 says every
  // caller handed a run id receives an ending whether it started the run,
  // joined one in flight, or was served a terminal message synthesised for a
  // run already over. The last of those is the ordinary shape after
  // `add_source`'s wake, and it is the one where this ending is the only
  // message the channel ever carries — so it is the one a `stepIndex` driven by
  // anything but the ending would miss.
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  progress({ phase: "finished", items: 0, elapsed_ms: 4200 });
  await settle();
  expect(step()).toBe("Done");
  // …and the corpus, not the nothing that run wrote.
  expect(text()).toContain("213 items");
});

test("a failed run stays at the first-sync step (#156)", async () => {
  // DONE is for an ending that said `finished`. A failure keeps its stats row,
  // its message and its *Retry* / *Skip for now* pair, where the reader decides
  // what to do with it — landing them on a success panel would be the defect
  // #156 exists to fix, inverted.
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  progress({ phase: "failed", items: 0, elapsed_ms: 300, message: "401 from /rest/api/2/myself" });
  await settle();
  expect(step()).toBe("First sync");
  expect(text()).not.toContain("knobas mirrored");
  expect(button("Retry")).toBeTruthy();
  expect(button("Skip for now")).toBeTruthy();

  // And a retry that works walks the reader on, so the failure is a detour
  // rather than a dead end.
  button("Retry")!.click();
  await settle();
  progress({ phase: "finished", items: 3, elapsed_ms: 4200 });
  await settle();
  expect(step()).toBe("Done");
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

test("a slow corpus read never lets the panel claim a count it does not have yet", async () => {
  // #120, and the sentence ADR-0005 exists to eliminate arriving through the
  // *render* path rather than the data path: `readCorpus` is fired on
  // `finished` and the panel renders before `list_sources` answers, so for that
  // window it showed the run's own count — *0 items* over a full mirror. The
  // window widens with the corpus, so it is at its most visible exactly when a
  // first sync is at its most impressive.
  corpus = 213;
  const gate = holdTheCorpusRead();
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  // The run this wizard was handed found the corpus already mirrored and wrote
  // nothing, which is the ordinary interleaving after `add_source`'s wake.
  progress({ phase: "finished", items: 0, elapsed_ms: 4200 });
  await settle();

  expect(calls.listSources).toBe(1);
  expect(text()).not.toContain("0 items");
  // The sync is over and it worked, so the panel says so and offers the way on
  // — waiting on a count is not waiting on the sync.
  expect(button("Finish")).toBeTruthy();

  gate.release();
  await settle();
  expect(text()).toContain("213 items");
});

test("a mirror that really is empty reads as zero, not as pending for ever", async () => {
  // The other half of the same rule: *not known yet* and *known to be none* are
  // different states, and a panel that could not tell them apart would trade
  // one wrong sentence for a permanent one.
  corpus = 0;
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  progress({ phase: "finished", items: 0, elapsed_ms: 4200 });
  await settle();
  expect(text()).toContain("0 items");
  expect(button("Finish")).toBeTruthy();
});

test("a corpus that cannot be read costs the count, not the sentence", async () => {
  // #137, ruled 2026-08-29: this used to fall back to the run's count, and the
  // run's count is an `Upserted` — the very number `CONTEXT.md` forbids the
  // word *mirrored* for, and the one that is zero in exactly the interleaving
  // ADR-0005 was written for. So the count goes and nothing takes its place.
  //
  // `list_sources` answers, but not about this source — a read that resolved
  // with no count to be had.
  render({ source: summary("nowhere") });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  progress({ phase: "finished", items: 9, elapsed_ms: 4200 });
  await settle();
  // On the DONE panel, and on the **real** path (#156): this is the sentence
  // the ruling is about, and until the ending moved the wizard here it rendered
  // only after a demo load.
  expect(step()).toBe("Done");
  expect(text()).toContain("knobas mirrored your items");
  expect(text()).not.toContain("9 items");
  expect(text()).not.toMatch(/\d+ items/);
  // A resolved answer is the answer: nothing to retry for.
  expect(calls.listSources).toBe(1);
  // The sync worked: a count that could not be re-read is not a failed sync.
  expect(button("Finish")).toBeTruthy();
  expect(button("Retry")).toBeUndefined();
});

test("a corpus read that is held open and then fails renders no count at all", async () => {
  // The failure path, proved by delaying and then failing the round trip rather
  // than by reading the source. Both attempts fail, so the panel concedes.
  const gate = holdTheCorpusRead();
  corpusFailures = 2;
  render({ source: summary() });
  button("Next")!.click();
  flushSync();
  button("Next")!.click();
  flushSync();
  button("Start the first sync")!.click();
  await settle();

  // The run this wizard was handed found the corpus already mirrored and wrote
  // nothing — the ordinary interleaving after `add_source`'s wake, and the one
  // that makes the old fallback say *0 items* over a full mirror.
  progress({ phase: "finished", items: 0, elapsed_ms: 4200 });
  await settle();
  // Still waiting, and saying so — on the DONE panel now (#156), which is where
  // the ending puts a real source. Asserted as the string it renders rather
  // than as the absence of a digit, because *no digit* is equally true of the
  // state this test is about — so a negative here and a negative after the
  // release would be one assertion taken twice, passing over a panel that never
  // moved.
  expect(step()).toBe("Done");
  expect(text()).toContain("knobas mirrored … items");

  gate.release();
  await settle();
  expect(calls.listSources).toBe(2);
  // Conceded, and that is a different thing from still pending: the sentence
  // drops the count instead. Without this line every assertion below it also
  // holds of a panel stuck on `…` for ever, which is not what the ruling asked
  // for and is the failure a `corpus` left `undefined` would produce.
  expect(text()).toContain("knobas mirrored your items");
  expect(text()).not.toContain("0 items");
  expect(text()).not.toMatch(/\d+ items/);
  expect(button("Finish")).toBeTruthy();
  expect(button("Retry")).toBeUndefined();
});

test("one failed corpus read is retried before the panel concedes", async () => {
  // Permitted by the ruling and taken: the wizard is already waiting, `…` is
  // already what the wait looks like, and a `list_sources` that fails at this
  // instant is likelier transient than terminal. It changes how often the
  // count-free sentence is reached, never what it claims.
  corpusFailures = 1;
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
  expect(calls.listSources).toBe(2);
  expect(text()).toContain("213 items");
});

test("a demo load whose corpus read fails says so too, and never the run's count", async () => {
  // The demo path earns no exception (#137). `demo_load` registers and syncs in
  // one call, so its `upserted` really is the corpus — which makes the fallback
  // harmless there, not right. It is also the only path that reaches the DONE
  // panel's own sentence, so it is where that sentence is asserted.
  corpusFailures = 2;
  render({ demo: true });
  button("Next")!.click();
  flushSync();
  target.querySelector<HTMLButtonElement>(".modules button")!.click();
  await settle();

  expect(calls.demoLoad).toBe(1);
  expect(step()).toBe("Done");
  expect(text()).toContain("knobas mirrored your items");
  // 21 is `demo_load`'s own `upserted`, and it is not what this sentence is
  // about.
  expect(text()).not.toContain("21 items");
  expect(text()).not.toMatch(/mirrored \d/);
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
