/**
 * **Nothing a component installs may outlive it.**
 *
 * Three findings in one review of this stream were the same hole: a `$effect`
 * teardown that could be deleted, or gutted to `() => {}`, with the whole
 * suite still green — a `window` listener that accumulates on every context
 * switch, a coalescing timer nothing cancels, and a seed that lands after
 * unmount and arms a `setTimeout` *after* the store was stopped. Each was
 * thoroughly tested for what it *draws*. None was tested for what it *leaves*.
 *
 * That is the same species as the `generate_handler!` survivor this PR was
 * built to close — *nothing pins that the thing which must be called is
 * called* — one layer up. So the answer here is the same kind of answer: not
 * three assertions about three teardowns, but one assertion about the class,
 * which holds for the component nobody has written yet.
 *
 * Every component with a `$effect` is mounted twice:
 *
 * 1. **Used, then unmounted.** The ordinary case: the listener was installed,
 *    the timer was armed, the subscription resolved — and all of it has to be
 *    gone.
 * 2. **Unmounted while its reads are still in flight.** `listen` is itself an
 *    `invoke` and resolves a tick or more later, so a shell that navigates
 *    during bring-up unmounts inside exactly that window. Nothing that lands
 *    afterwards may install anything.
 *
 * Only `window` listeners are watched, never `document` — Svelte delegates
 * there and never cleans up, which `residue.ts` explains at length.
 *
 * A Tauri subscription is neither a DOM listener nor a timer, so the mocked
 * `listen` below **models it as one**: it adds a `window` listener under a
 * synthetic `tauri:` type and its unlisten removes it. A forgotten `off?.()`
 * and a forgotten `if (dead) unlisten()` then fail the same assertion as a
 * forgotten `removeEventListener`, which is the whole point of doing this once
 * instead of per component.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";

import type { EntityDetail, EntityPage, EntityRow } from "../ipc/entity";

// ---------------------------------------------------------------------------
// Answers the test hands over on command, never on a timer.
// ---------------------------------------------------------------------------

/**
 * A promise the *test* decides when to resolve.
 *
 * Deliberately not `setTimeout`-based: this file counts outstanding timers, so
 * a mock that answered on one would be indistinguishable from the leak it is
 * looking for.
 */
const pending: { resolve: (value: unknown) => void; value: unknown }[] = [];

function deferred<T>(value: T): Promise<T> {
  return new Promise<T>((resolve) => {
    pending.push({ resolve: resolve as (v: unknown) => void, value });
  });
}

/** Let every outstanding answer land. Safe to call when there are none. */
function land(): void {
  for (const entry of pending.splice(0)) entry.resolve(entry.value);
}

const ROW: EntityRow = {
  entity_id: "mock:PAY-231",
  kind: "ticket",
  source_id: "mock",
  title: "Retry failed SEPA payouts",
  updated_at: "2026-08-22T11:48:00Z",
  synced_at: "2026-08-22T14:30:00Z",
};

const PAGE: EntityPage = { rows: [ROW], total: 1 };

const DETAIL: EntityDetail = {
  row: ROW,
  source: { id: "mock", display_name: "Tidewater (mock)", adapter_kind: "mock" },
  kind_info: null,
  body_text: "body",
  author: "mara",
  payload: { key: "PAY-231" },
  web_url: null,
  deleted_at: null,
  links: [],
  activity: [],
};

const LINE = {
  id: 1,
  at: "2026-08-22T14:30:00Z",
  actor: "sync:mock",
  verb: "synced",
  entity_id: "mock:PAY-231",
  detail: {},
};

vi.mock("../ipc/entity", () => ({
  listEntities: () => deferred(PAGE),
  getEntity: () => deferred(DETAIL),
  recentActivity: () => deferred([LINE]),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (payload: { payload: unknown }) => void) => {
    // Modelled as a real `window` listener so that failing to unsubscribe is
    // the same failure as failing to remove a listener. See the module docs.
    const relay = (raised: Event) => handler({ payload: (raised as CustomEvent).detail });
    window.addEventListener(`tauri:${event}`, relay);
    return deferred(() => window.removeEventListener(`tauri:${event}`, relay));
  },
}));

const { trackResidue } = await import("./residue");
const { createRouter } = await import("./router.svelte");
const { builtinContexts } = await import("./contexts");

const ContextTabs = (await import("./ContextTabs.svelte")).default;
const Flap = (await import("./Flap.svelte")).default;
const ModalFixture = (await import("./Modal.fixture.svelte")).default;
const Room = (await import("./Room.svelte")).default;
const ShellFixture = (await import("./Shell.fixture.svelte")).default;
const StatusBar = (await import("./StatusBar.svelte")).default;
const Tile = (await import("./Tile.svelte")).default;
const Detail = (await import("../detail/Detail.svelte")).default;

const CONTEXTS = builtinContexts([{ id: "jira", label: "Tidewater Jira" }]);

/** A lifecycle that is up, so the components that gate on it actually read. */
function readyLifecycle() {
  return {
    db: { state: "ready" as const },
    status: {
      db: { state: "ready" as const },
      first_run: false,
      demo: true,
      source_count: 1,
      app_version: "0.1.0",
    },
    ready: true,
    error: null,
    start: async () => {},
    stop: () => {},
    retry: async () => {},
  };
}

/**
 * One component under test.
 *
 * `source` is the file it lives in, and it is what the coverage guard at the
 * bottom matches against — so a new component with an effect fails this suite
 * until somebody puts it here on purpose.
 */
interface Case {
  name: string;
  source: string;
  open(target: HTMLElement): { app: Record<string, unknown> };
  /** Drive it into the state where it has actually installed something. */
  exercise?(target: HTMLElement): void;
}

const CASES: Case[] = [
  {
    name: "ContextTabs",
    source: "lib/shell/ContextTabs.svelte",
    open: (target) => ({
      app: mount(ContextTabs, { target, props: { router: createRouter(), contexts: CONTEXTS } }),
    }),
    // The window listener only exists while the popover is open, so a pass
    // that never opened it would prove nothing about the teardown.
    exercise: (target) => target.querySelector<HTMLButtonElement>(".ctx-name")?.click(),
  },
  {
    name: "StatusBar",
    source: "lib/shell/StatusBar.svelte",
    open: (target) => ({
      app: mount(StatusBar, { target, props: { lifecycle: readyLifecycle() } }),
    }),
  },
  {
    name: "Tile",
    source: "lib/shell/Tile.svelte",
    open: (target) => ({
      app: mount(Tile, {
        target,
        props: {
          spec: { id: "tickets", label: "Tickets", kinds: ["ticket"] },
          sources: [],
          onopen: () => {},
        },
      }),
    }),
  },
  {
    name: "Room",
    source: "lib/shell/Room.svelte",
    open: (target) => ({
      app: mount(Room, { target, props: { router: createRouter(), contexts: CONTEXTS } }),
    }),
  },
  {
    name: "Detail",
    source: "lib/detail/Detail.svelte",
    open: (target) => ({
      app: mount(Detail, {
        target,
        props: {
          entityId: "mock:PAY-231",
          kind: "ticket",
          contextLabel: "All work",
          onclose: () => {},
        },
      }),
    }),
  },
  {
    name: "Flap",
    source: "lib/shell/Flap.svelte",
    open: (target) => {
      const props = $state({ value: "first" });
      const app = mount(Flap, { target, props });
      // A flap only arms its animation timers when the value moves.
      return { app: { ...app, __props: props } as unknown as Record<string, unknown> };
    },
    exercise: () => {},
  },
  {
    name: "Modal",
    source: "lib/shell/Modal.svelte",
    open: (target) => ({ app: mount(ModalFixture, { target, props: { onclose: () => {} } }) }),
  },
  {
    name: "Shell",
    source: "lib/shell/Shell.svelte",
    open: (target) => ({
      app: mount(ShellFixture, {
        target,
        props: { router: createRouter(), onsearch: () => {} },
      }),
    }),
  },
];

afterEach(() => {
  pending.length = 0;
  document.body.replaceChildren();
});

test.each(CASES.map((entry) => [entry.name, entry] as const))(
  "%s leaves nothing behind after it has been used",
  async (_name, entry) => {
    const track = trackResidue();
    try {
      const target = document.createElement("div");
      document.body.append(target);
      const { app } = entry.open(target);
      flushSync();

      entry.exercise?.(target);
      flushSync();
      // Everything it asked for arrives while it is still mounted.
      land();
      await track.settle();
      flushSync();
      // ...and whatever that started (a coalescing window, say) is now open.
      land();
      flushSync();

      unmount(app);
      target.remove();
      await track.settle();

      expect(track.residue()).toEqual({ listeners: [], timers: [] });
    } finally {
      track.stop();
    }
  },
);

test.each(CASES.map((entry) => [entry.name, entry] as const))(
  "%s leaves nothing behind when it is unmounted mid-flight",
  async (_name, entry) => {
    const track = trackResidue();
    try {
      const target = document.createElement("div");
      document.body.append(target);
      const { app } = entry.open(target);
      flushSync();

      // Gone before a single answer has come back. `listen` is itself an
      // `invoke`, so this is not a contrived ordering — it is the one a
      // navigation during bring-up produces.
      unmount(app);
      target.remove();

      land();
      await track.settle();
      land();
      await track.settle();

      expect(track.residue()).toEqual({ listeners: [], timers: [] });
    } finally {
      track.stop();
    }
  },
);

/**
 * The tracker reports a leak — or every assertion above is vacuous.
 *
 * This is the instrument's own calibration, and it is not ceremony: replacing
 * `residue()` with `{ listeners: [], timers: [] }` left the whole suite green
 * until this existed. A net that cannot be shown to catch anything is not
 * evidence that there was nothing to catch.
 */
test("the tracker sees what is left behind, and stops seeing it when it is cleaned up", async () => {
  const before = globalThis.setTimeout;
  const track = trackResidue();
  try {
    const leak = () => {};
    window.addEventListener("pointerdown", leak, true);
    const timeout = setTimeout(() => {}, 60_000);
    const interval = setInterval(() => {}, 60_000);

    // A timer that has already *fired* is not residue -- only one still armed
    // when its owner is gone is, which is what the wrapper's self-removal is
    // for. Nothing below would notice if it stopped working.
    let fired = false;
    setTimeout(() => {
      fired = true;
    }, 0);
    await track.settle();
    expect(fired, "the wrapper swallowed the callback").toBe(true);

    expect(track.residue()).toEqual({
      listeners: ["pointerdown (capture) on window"],
      timers: ["setInterval", "setTimeout"],
    });

    // The capture flag is part of the identity: a bubble-phase remove does not
    // balance a capture-phase add, and reporting that it did would hide the
    // exact shape of this stream's first finding.
    window.removeEventListener("pointerdown", leak);
    expect(track.residue().listeners).toEqual(["pointerdown (capture) on window"]);

    window.removeEventListener("pointerdown", leak, true);
    clearTimeout(timeout);
    clearInterval(interval);
    expect(track.residue()).toEqual({ listeners: [], timers: [] });
  } finally {
    track.stop();
  }

  // ...and the globals are the ones it found. A tracker left installed would
  // count the next test file's timers as this one's.
  expect(globalThis.setTimeout).toBe(before);
});

/**
 * A `removeEventListener` for something never added cannot pay for a real leak.
 *
 * The DOM treats such a call as a no-op, and so must the count: letting it go
 * negative would mean the *next* genuine add on that type balanced to zero and
 * a real leak was reported as clean. That is the instrument lying in the one
 * direction that matters, and nothing else here exercises it.
 */
test("an unmatched remove does not pay for a later leak", async () => {
  const track = trackResidue();
  try {
    const never = () => {};
    window.removeEventListener("pointerdown", never, true);

    const leak = () => {};
    window.addEventListener("pointerdown", leak, true);
    await track.settle();

    expect(track.residue().listeners).toEqual(["pointerdown (capture) on window"]);

    window.removeEventListener("pointerdown", leak, true);
  } finally {
    track.stop();
  }
});

/**
 * The table covers every component that installs anything.
 *
 * Without this, the suite above is a statement about eight components rather
 * than about the class, and the ninth — written next week, with a `listen` in
 * it — is caught by whoever reviews it or by nobody. Growing the table is
 * allowed; growing the codebase past it without noticing is not.
 */
test("every component with an effect is in the table", () => {
  const root = join(process.cwd(), "src") + "/";
  const withEffects: string[] = [];

  const walk = (dir: string) => {
    for (const item of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, item.name);
      if (item.isDirectory()) {
        walk(path);
      } else if (item.name.endsWith(".svelte") && !item.name.endsWith(".fixture.svelte")) {
        if (readFileSync(path, "utf8").includes("$effect(")) {
          withEffects.push(path.slice(root.length));
        }
      }
    }
  };
  walk(join(root, "lib"));

  expect(withEffects.length, "no components were scanned at all").toBeGreaterThan(0);
  const covered = new Set(CASES.map((entry) => entry.source));
  expect(withEffects.filter((file) => !covered.has(file)).sort()).toEqual([]);
});
