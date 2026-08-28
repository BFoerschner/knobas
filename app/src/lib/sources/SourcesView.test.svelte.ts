/**
 * The sources view: what a reader can see about a source, and what they can do
 * about it.
 *
 * The behaviours pinned here are the ones that are wrong in a way nobody
 * notices: a row that shows a sync age while the credential has been rejected
 * since, a *Sync now* offered on a source that cannot sync until a human types
 * a password, a delete that goes through without saying what it takes with it,
 * and a failed list that renders as "no sources" — which reads as *you have
 * none* rather than *I could not ask*.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { CredentialHealth, SourceSummary, SyncRunRow } from "../ipc/sources";

const NOW = new Date("2026-08-25T12:00:00Z");

/** Plain recorders, not `vi.fn`s, so a mock factory can close over them. */
const calls = {
  syncNow: [] as string[],
  syncAll: 0,
  deleteSource: [] as { id: string; purge: boolean }[],
  setSecret: [] as { id: string; value: string }[],
  listSources: 0,
};

let sources: SourceSummary[] = [];
let listFails: unknown = null;

vi.mock("../ipc/sources", () => ({
  listSources: () => {
    calls.listSources += 1;
    return listFails ? Promise.reject(listFails) : Promise.resolve(sources);
  },
  syncNow: (sourceId: string) => {
    calls.syncNow.push(sourceId);
    return Promise.resolve(7);
  },
  syncAll: () => {
    calls.syncAll += 1;
    return Promise.resolve([7]);
  },
  deleteSource: (id: string, purgeItems: boolean) => {
    calls.deleteSource.push({ id, purge: purgeItems });
    return Promise.resolve();
  },
  setSourceSecret: (id: string, secret: { value: string }) => {
    calls.setSecret.push({ id, value: secret.value });
    return Promise.resolve({
      source_id: id,
      state: "ok",
      checked_at: NOW.toISOString(),
      detail: null,
      secret_expires_at: null,
    } satisfies CredentialHealth);
  },
  listSyncRuns: () => Promise.resolve([]),
  dbStats: () =>
    Promise.resolve({
      db_bytes: 222_298_112,
      entity_count: 12,
      item_count: 21,
      per_source: [],
      oldest_synced_at: null,
      newest_synced_at: null,
    }),
  listAdapters: () => Promise.resolve([]),
  addSource: () => Promise.reject(new Error("not used here")),
  testSource: () => Promise.reject(new Error("not used here")),
  credentialHealth: () => Promise.resolve([]),
  reindexFts: () => Promise.resolve(),
}));

/** The `sync:state` / `source:health` bridge, driven by hand. */
const listeners = new Map<string, ((event: { payload: unknown }) => void)[]>();

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (event: { payload: unknown }) => void) => {
    const existing = listeners.get(event) ?? [];
    existing.push(handler);
    listeners.set(event, existing);
    return Promise.resolve(() => {
      listeners.set(event, (listeners.get(event) ?? []).filter((h) => h !== handler));
    });
  },
}));

function emit(event: string, payload: unknown) {
  for (const handler of [...(listeners.get(event) ?? [])]) handler({ payload });
  flushSync();
}

const { default: SourcesView } = await import("./SourcesView.svelte");
const { createHealth } = await import("../shell/health.svelte");

function run(over: Partial<SyncRunRow> = {}): SyncRunRow {
  return {
    id: 3,
    source_id: "jira",
    trigger: "schedule",
    started_at: "2026-08-25T11:50:00Z",
    finished_at: "2026-08-25T11:50:12Z",
    outcome: "ok",
    upserted: 14,
    deleted: 0,
    swept: 0,
    error: null,
    cursor_after: null,
    ...over,
  };
}

function source(over: Partial<SourceSummary> = {}): SourceSummary {
  const id = over.id ?? "jira";
  return {
    id,
    adapter_kind: "jira",
    display_name: "Tidewater Jira",
    base_url: "https://jira.tidewater.example",
    enabled: true,
    sync_interval_secs: 900,
    config: {},
    health: {
      source_id: id,
      state: "ok",
      checked_at: "2026-08-25T11:50:00Z",
      detail: null,
      secret_expires_at: null,
    },
    last_run: run({ source_id: id }),
    next_run_at: "2026-08-25T12:05:00Z",
    item_count: 213,
    kinds: [{ id: "ticket", label: "Ticket", plural: "Tickets", monogram: "TK", full_sync_exhaustive: true }],
    ...over,
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/** A store with no bridge behind it — the view supplies the seed from its rows. */
function health() {
  return createHealth({
    credentialHealth: () => Promise.resolve([]),
    listen: () => Promise.resolve(() => {}),
  });
}

function render(props: Record<string, unknown> = {}) {
  app = mount(SourcesView, {
    target,
    props: { now: NOW, health: health(), onnavigate: () => {}, ...props },
  });
  flushSync();
  return app;
}

async function settle() {
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
  flushSync();
}

function rows() {
  return [...target.querySelectorAll<HTMLElement>(".src[data-source-id]")];
}

function rowFor(id: string) {
  return target.querySelector<HTMLElement>(`.src[data-source-id="${id}"]`);
}

/** A button by its visible label, anywhere in the view. */
function button(label: string, within: ParentNode = target) {
  return [...within.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent?.trim() === label,
  );
}

function text() {
  return target.textContent ?? "";
}

/**
 * A real `Storage` for the two "the secret is not persisted" assertions.
 *
 * This runner's jsdom has no `localStorage` at all, so
 * `JSON.stringify(localStorage)` is `undefined` and every `not.toContain`
 * against it passes without looking at anything — an assertion that cannot
 * fail is not evidence. Installing a working one, and proving below that the
 * dump can actually *see* a stored value, is what makes the negative mean
 * something.
 */
function installStorage(name: "localStorage" | "sessionStorage") {
  const map = new Map<string, string>();
  const storage = {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => void map.set(key, String(value)),
    removeItem: (key: string) => void map.delete(key),
    clear: () => map.clear(),
    key: (index: number) => [...map.keys()][index] ?? null,
    get length() {
      return map.size;
    },
  };
  Object.defineProperty(window, name, { value: storage, configurable: true, writable: true });
  return { storage, dump: () => JSON.stringify([...map.entries()]) };
}

beforeEach(() => {
  calls.syncNow = [];
  calls.syncAll = 0;
  calls.deleteSource = [];
  calls.setSecret = [];
  calls.listSources = 0;
  listeners.clear();
  sources = [];
  listFails = null;
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
});

test("renders one row per source with its health, item count and last run", async () => {
  sources = [
    source(),
    source({
      id: "gitea",
      adapter_kind: "gitea",
      display_name: "Tidewater Gitea",
      base_url: "https://git.tidewater.example",
      item_count: 48,
      kinds: [
        { id: "pr", label: "Pull request", plural: "Pull requests", monogram: "PR", full_sync_exhaustive: false },
        { id: "repo", label: "Repository", plural: "Repositories", monogram: "RP", full_sync_exhaustive: true },
      ],
    }),
  ];
  render();
  await settle();

  expect(rows().length).toBe(2);
  const jira = rowFor("jira")!;
  expect(jira.textContent).toContain("Tidewater Jira");
  expect(jira.textContent).toContain("https://jira.tidewater.example");
  expect(jira.textContent).toContain("213");
  // The last run, not merely "synced": a reader looking at this view is
  // looking for when it last worked.
  expect(jira.textContent).toContain("10 min ago");

  // Capability chips come from `SourceSummary.kinds`, so a new adapter's kinds
  // appear with no table here (§3a).
  const caps = [...rowFor("gitea")!.querySelectorAll(".caps span")].map((c) => c.textContent);
  expect(caps).toEqual(["Pull requests", "Repositories"]);
});

test("Sync now calls syncNow with that source's id and shows the run as started", async () => {
  sources = [source(), source({ id: "gitea", adapter_kind: "gitea", display_name: "G" })];
  render();
  await settle();

  button("Sync now", rowFor("gitea")!)!.click();
  await settle();

  // The id of *that* row, which is the assertion a one-source fixture cannot
  // make.
  expect(calls.syncNow).toEqual(["gitea"]);
  expect(rowFor("gitea")!.textContent).toContain("syncing");
  expect(rowFor("jira")!.textContent).not.toContain("syncing");
});

test("a sync:state event patches the matching row rather than re-listing", async () => {
  sources = [source(), source({ id: "gitea", adapter_kind: "gitea", display_name: "G" })];
  render();
  await settle();
  const listedOnce = calls.listSources;

  emit("sync:state", {
    source_id: "gitea",
    running: true,
    run_id: 9,
    started_at: NOW.toISOString(),
    last_finished_at: null,
    last_outcome: null,
    next_run_at: null,
    backoff_until: null,
  });
  await settle();

  expect(rowFor("gitea")!.textContent).toContain("syncing");
  expect(rowFor("jira")!.textContent).not.toContain("syncing");
  // Re-listing on every transition would make a sync of five sources fetch the
  // whole view twenty times; the event carries the status it needs.
  expect(calls.listSources).toBe(listedOnce);
});

test("a source whose health is unauthorized offers Re-enter, not Sync now", async () => {
  sources = [
    source({
      health: {
        source_id: "jira",
        state: "unauthorized",
        checked_at: NOW.toISOString(),
        detail: "401 from /rest/api/2/myself",
        secret_expires_at: null,
      },
    }),
  ];
  render();
  await settle();

  const row = rowFor("jira")!;
  expect(button("Re-enter", row)).toBeTruthy();
  // The scheduler never retries a 401 on its own (P7), so a *Sync now* here is
  // a button that cannot work — it would run, fail on the same credential, and
  // teach the reader nothing.
  expect(button("Sync now", row)).toBeUndefined();
  expect(row.className).toContain("err");
  expect(row.textContent).toContain("401 from /rest/api/2/myself");
});

test("a source:health event turns a healthy row into one that needs a human", async () => {
  sources = [source()];
  const store = health();
  render({ health: store });
  await settle();
  expect(button("Sync now", rowFor("jira")!)).toBeTruthy();

  store.patch({
    source_id: "jira",
    state: "unauthorized",
    checked_at: NOW.toISOString(),
    detail: "401",
    secret_expires_at: null,
  });
  flushSync();

  expect(button("Re-enter", rowFor("jira")!)).toBeTruthy();
  expect(button("Sync now", rowFor("jira")!)).toBeUndefined();
});

test("Re-enter posts the secret once, clears the field, and syncs", async () => {
  sources = [
    source({
      health: {
        source_id: "jira",
        state: "unauthorized",
        checked_at: NOW.toISOString(),
        detail: "401",
        secret_expires_at: null,
      },
    }),
  ];
  render();
  await settle();

  button("Re-enter", rowFor("jira")!)!.click();
  flushSync();

  const strip = target.querySelector(".src-fix")!;
  // Spec §14: the sentence is on screen because it is true, and because a
  // person typing a password deserves to be told where it goes.
  expect(strip.textContent).toContain("Stored in the OS keychain, never in the database.");

  const input = strip.querySelector<HTMLInputElement>("input")!;
  expect(input.type).toBe("password");
  input.value = "s3cret";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  button("Save and retry sync", strip)!.click();
  await settle();

  expect(calls.setSecret).toEqual([{ id: "jira", value: "s3cret" }]);
  // Re-entering a password is a request to make the sync work again, so the
  // retry is the same gesture rather than a second one.
  expect(calls.syncNow).toEqual(["jira"]);
  // The strip is gone, and with it the only place the typed value lived.
  expect(target.querySelector(".src-fix")).toBeNull();
  expect(text()).not.toContain("s3cret");
});

test("the typed secret never reaches storage or a DOM attribute", async () => {
  const local = installStorage("localStorage");
  const session = installStorage("sessionStorage");
  // The control: the dumps below can see a value that *is* stored, so a clean
  // dump is a fact about the secret and not about the dump.
  local.storage.setItem("knobas.canary", "s3cret");
  expect(local.dump()).toContain("s3cret");
  local.storage.clear();

  sources = [
    source({
      health: {
        source_id: "jira",
        state: "unauthorized",
        checked_at: NOW.toISOString(),
        detail: "401",
        secret_expires_at: null,
      },
    }),
  ];
  render();
  await settle();
  button("Re-enter", rowFor("jira")!)!.click();
  flushSync();

  const input = target.querySelector<HTMLInputElement>(".src-fix input")!;
  input.value = "s3cret";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();

  // While it is still on screen: the value lives in the input's *property*,
  // which is where a person's typing lives, and nowhere a serializer reaches.
  expect(target.innerHTML).not.toContain("s3cret");
  expect(local.dump()).not.toContain("s3cret");
  expect(session.dump()).not.toContain("s3cret");

  button("Save and retry sync", target.querySelector(".src-fix")!)!.click();
  await settle();
  expect(local.dump()).not.toContain("s3cret");
  expect(session.dump()).not.toContain("s3cret");
  expect(target.innerHTML).not.toContain("s3cret");
});

test("a rejected re-entry keeps the strip open and says why", async () => {
  sources = [
    source({
      health: {
        source_id: "jira",
        state: "unauthorized",
        checked_at: NOW.toISOString(),
        detail: "401",
        secret_expires_at: null,
      },
    }),
  ];
  const store = health();
  render({ health: store });
  await settle();
  button("Re-enter", rowFor("jira")!)!.click();
  flushSync();

  const input = target.querySelector<HTMLInputElement>(".src-fix input")!;
  input.value = "wrong";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Save and retry sync", target.querySelector(".src-fix")!)!.click();
  await settle();

  // `set_source_secret` answers with *health*, not with a rejection, when the
  // new secret is also wrong. This fixture answers `ok`, so the strip closes —
  // and then the store is moved the way the backend would move it, which is
  // the half that says the row goes back to asking rather than staying green
  // on a password that does not work.
  expect(target.querySelector(".src-fix")).toBeNull();
  store.patch({
    source_id: "jira",
    state: "unauthorized",
    checked_at: NOW.toISOString(),
    detail: "401 again",
    secret_expires_at: null,
  });
  flushSync();
  expect(button("Re-enter", rowFor("jira")!)).toBeTruthy();
  expect(rowFor("jira")!.textContent).toContain("401 again");
});

test("a PAT nearing expiry is announced on the row before it stops working", async () => {
  sources = [
    source({
      health: {
        source_id: "jira",
        state: "ok",
        checked_at: NOW.toISOString(),
        detail: null,
        secret_expires_at: "2026-09-01T12:00:00Z",
      },
    }),
  ];
  render();
  await settle();
  expect(rowFor("jira")!.textContent).toContain("PAT expires in 7 days");
});

test("Delete asks first, and the confirm dialog names the source and the purge choice", async () => {
  sources = [source()];
  render();
  await settle();

  button("Delete", rowFor("jira")!)!.click();
  flushSync();
  // Nothing has happened yet — that is the point of asking.
  expect(calls.deleteSource).toEqual([]);

  const dialog = target.querySelector<HTMLElement>('[role="dialog"]')!;
  expect(dialog.textContent).toContain("Tidewater Jira");
  // The number is the decision: 213 mirrored items either stay searchable or
  // go, and a confirm that does not say so is asking about the wrong thing.
  expect(dialog.textContent).toContain("213");

  const purge = dialog.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
  expect(purge.checked).toBe(false);
  purge.click();
  flushSync();

  button("Delete source", dialog)!.click();
  await settle();
  expect(calls.deleteSource).toEqual([{ id: "jira", purge: true }]);
  // A mutation changes the row set, so this one *does* re-list.
  expect(calls.listSources).toBeGreaterThan(1);
});

/**
 * The delete has to reach the *store*, not just the row set.
 *
 * The store is what the top strip's monograms and the shell's per-source room
 * tabs are drawn from, so a source that is gone from `list_sources` but still
 * in the store is a tab strip disagreeing with this view about which sources
 * exist. It was: `load()` patched each surviving row, and `patch` can only add.
 *
 * The `mock` control is the half that makes this an assertion about *forgetting
 * one* rather than about clearing everything.
 */
/**
 * The row's monogram is the **source id**, which is P10's whole point.
 *
 * `jira` and `tidewater-jira` are two instances of one adapter (§4.2's
 * multi-instance form), so a monogram taken from the *adapter kind* draws `JI`
 * over both — while the top strip, which has always taken the id, draws `JI`
 * and `TI`. Same two sources, told apart in one place and not in the other.
 * The fixture used to hide this: its id and its adapter kind were the same
 * word.
 */
test("two instances of one adapter get two monograms, not one", async () => {
  sources = [
    source(),
    source({ id: "tidewater-jira", display_name: "Tidewater Jira EU" }),
  ];
  render();
  await settle();

  const monograms = [...target.querySelectorAll(".src:not(.hd) .mg")].map((el) =>
    el.textContent?.trim(),
  );
  expect(monograms).toEqual(["JI", "TI"]);
});

test("a deleted source leaves the shared health store, not just the list", async () => {
  sources = [source(), source({ id: "mock", display_name: "Tidewater mock" })];
  const store = health();
  render({ health: store });
  await settle();
  expect(store.all.map((row) => row.source_id)).toEqual(["jira", "mock"]);

  sources = sources.filter((row) => row.id !== "jira");
  button("Delete", rowFor("jira")!)!.click();
  flushSync();
  button("Delete source", target.querySelector<HTMLElement>('[role="dialog"]')!)!.click();
  await settle();

  expect(store.get("jira"), "the strip would keep drawing a source that is gone").toBeNull();
  expect(store.all.map((row) => row.source_id)).toEqual(["mock"]);
});

test("cancelling the confirm deletes nothing", async () => {
  sources = [source()];
  render();
  await settle();
  button("Delete", rowFor("jira")!)!.click();
  flushSync();
  button("Cancel", target.querySelector<HTMLElement>('[role="dialog"]')!)!.click();
  await settle();
  expect(calls.deleteSource).toEqual([]);
  expect(target.querySelector('[role="dialog"]')).toBeNull();
});

test("listSources failing renders an error panel, not an empty list", async () => {
  listFails = { code: "not_ready", message: "the database is still starting", source_id: null };
  render();
  await settle();

  // "No sources yet" would be a lie with a button on it: the view does not
  // know whether there are none, it knows it could not ask.
  expect(text()).not.toContain("No sources");
  expect(text()).toContain("the database is still starting");
  expect(button("Retry")).toBeTruthy();

  listFails = null;
  sources = [source()];
  button("Retry")!.click();
  await settle();
  expect(rows().length).toBe(1);
});

test("no sources at all invites adding one, and says nothing about failure", async () => {
  sources = [];
  render();
  await settle();
  expect(text()).toContain("No sources");
  expect(button("Add source")).toBeTruthy();
  expect(rows().length).toBe(0);
});

test("Sync all runs every source in one call, not one call per row", async () => {
  sources = [source(), source({ id: "gitea", adapter_kind: "gitea", display_name: "G" })];
  render();
  await settle();
  button("Sync all")!.click();
  await settle();
  expect(calls.syncAll).toBe(1);
  expect(calls.syncNow).toEqual([]);
});

test("a disabled source says so and is not offered a sync", async () => {
  sources = [source({ enabled: false, next_run_at: null })];
  render();
  await settle();
  expect(rowFor("jira")!.textContent).toContain("disabled");
  expect(button("Sync now", rowFor("jira")!)).toBeUndefined();
});

test("a source that has never run says so rather than showing an empty age", async () => {
  sources = [source({ last_run: null, next_run_at: null })];
  render();
  await settle();
  expect(rowFor("jira")!.textContent).toContain("never synced");
});

test("a failed last run shows its error as text", async () => {
  sources = [
    source({
      last_run: run({
        outcome: "error",
        error: '<img src=x onerror="alert(1)"> upstream said 500',
      }),
    }),
  ];
  render();
  await settle();
  const row = rowFor("jira")!;
  expect(row.textContent).toContain('<img src=x onerror="alert(1)"> upstream said 500');
  // Gotcha 7: an error string comes from a source system, so it is characters
  // and not an element.
  expect(row.querySelector("img")).toBeNull();
});

test("the subscription is torn down when the view goes away", async () => {
  sources = [source()];
  render();
  await settle();
  expect((listeners.get("sync:state") ?? []).length).toBe(1);
  unmount(app!);
  app = undefined;
  flushSync();
  expect((listeners.get("sync:state") ?? []).length).toBe(0);
});
