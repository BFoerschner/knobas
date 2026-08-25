/**
 * The room decides *which* tiles exist; the tiles decide what is in them.
 *
 * That split is the behaviour worth pinning: a room draws a tile only for a
 * kind its corpus actually holds (§3a), and an undeclared kind gets one of its
 * own rather than being dropped on the floor.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityDetail, EntityFilter, EntityPage, EntityRow } from "../ipc/entity";

/** A plain function, not a `vi.fn` — see the note in `Tile.test.svelte.ts`. */
const calls: { filter: EntityFilter; limit: number; offset: number }[] = [];
let answer: (filter: EntityFilter) => Promise<EntityPage> = () =>
  Promise.resolve({ rows: [], total: 0 });

/**
 * `getEntity` too, because opening a row mounts the slide-over — which reads.
 * A mock that stopped at `listEntities` would make the navigation test fail
 * inside Svelte's effect runner, several frames after the assertion it is
 * about.
 */
vi.mock("../ipc/entity", () => ({
  listEntities: (filter: EntityFilter, limit: number, offset: number) => {
    calls.push({ filter, limit, offset });
    return answer(filter);
  },
  getEntity: (entityId: string): Promise<EntityDetail> =>
    Promise.reject({ code: "not_found", message: `${entityId} is not in the local index`, source_id: null }),
}));

const { default: Room } = await import("./Room.svelte");
const { builtinContexts } = await import("./contexts");
const { createRouter } = await import("./router.svelte");

const CONTEXTS = builtinContexts([{ id: "jira", label: "Tidewater Jira" }]);

function row(kind: string, key: string): EntityRow {
  return {
    entity_id: `mock:${key}`,
    kind,
    source_id: "mock",
    title: `Title of ${key}`,
    updated_at: "2026-08-22T11:48:00Z",
    synced_at: "2026-08-22T14:30:00Z",
  };
}

function render(hash: string) {
  location.hash = hash;
  const router = createRouter();
  const stop = router.start();
  const target = document.createElement("div");
  document.body.append(target);
  const app = mount(Room, { target, props: { router, contexts: CONTEXTS } });
  flushSync();
  return {
    target,
    router,
    tiles: () => [...target.querySelectorAll<HTMLElement>(".tile .tile-h .lab")].map((l) => l.textContent),
    text: () => target.textContent ?? "",
    done: () => {
      unmount(app);
      stop();
      target.remove();
    },
  };
}

beforeEach(() => {
  calls.length = 0;
  answer = () => Promise.resolve({ rows: [], total: 0 });
});

test("draws one tile per bucket present, and one per open kind", async () => {
  answer = (filter) =>
    Promise.resolve(
      filter.kinds.length === 0
        ? {
            rows: [row("ticket", "PAY-1"), row("page", "ENG-1"), row("incident", "INC-1")],
            total: 3,
          }
        : { rows: [], total: 0 },
    );

  const screen = render("#/ctx/all");
  await vi.waitFor(() => expect(screen.tiles().length).toBeGreaterThan(0));
  flushSync();

  expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents"]);
  // The scan is unfiltered by kind — that is what makes it a survey.
  expect(calls[0]?.filter.kinds).toEqual([]);

  screen.done();
});

/** The heading counts the whole room, not the scan window. */
test("the room bar carries the unpaged total", async () => {
  answer = (filter) =>
    Promise.resolve(
      filter.kinds.length === 0 ? { rows: [row("ticket", "PAY-1")], total: 412 } : { rows: [], total: 0 },
    );

  const screen = render("#/ctx/all");
  await vi.waitFor(() => expect(screen.text()).toContain("412 items"));
  screen.done();
});

/**
 * A source room reads that source and says so.
 *
 * Both halves: the heading is the source's name, and the scan carries its id
 * into the filter — a room that looked right and queried everything would pass
 * a test that checked only the first.
 */
test("a source room filters by its source and is named after it", async () => {
  answer = () => Promise.resolve({ rows: [row("ticket", "PAY-1")], total: 1 });

  const screen = render("#/ctx/src:jira");
  await vi.waitFor(() => expect(screen.tiles()).toEqual(["Tickets"]));
  flushSync();

  expect(screen.text()).toContain("Tidewater Jira");
  expect(calls[0]?.filter.sources).toEqual(["jira"]);
  // ...and the tile inherits it rather than reading the whole mirror.
  expect(calls.every((call) => call.filter.sources.includes("jira"))).toBe(true);

  screen.done();
});

/** An empty corpus is a sentence, not a grid of empty tiles. */
test("a room with nothing in it draws no tiles at all", async () => {
  const screen = render("#/ctx/all");
  await vi.waitFor(() => expect(screen.text()).toContain("Nothing synced into this room yet"));
  expect(screen.tiles()).toEqual([]);
  screen.done();
});

/** A failed survey says why, rather than looking like an empty corpus. */
test("a rejected survey shows its message", async () => {
  answer = () =>
    Promise.reject({ code: "not_ready", message: "the database is still starting", source_id: null });

  const screen = render("#/ctx/all");
  await vi.waitFor(() => expect(screen.text()).toContain("the database is still starting"));
  expect(screen.text()).not.toContain("Nothing synced into this room yet");
  screen.done();
});

/**
 * Opening a row is a navigation, and the address carries the kind so the
 * detail view knows what it is before it has fetched anything.
 */
test("opening a row navigates to that entity's address", async () => {
  answer = () => Promise.resolve({ rows: [row("incident", "INC-1")], total: 1 });

  const screen = render("#/ctx/src:jira");
  await vi.waitFor(() => expect(screen.target.querySelector(".row")).not.toBeNull());

  screen.target.querySelector<HTMLButtonElement>(".row")?.click();
  flushSync();

  expect(location.hash).toBe("#/incident/mock:INC-1");
  // ...and Esc still knows which room it was opened over.
  expect(screen.router.ctx).toBe("src:jira");

  screen.done();
});

/** The grid's row count is a class, never a computed inline style (CSP). */
test("the tile grid sizes itself with a modifier class", async () => {
  answer = (filter) =>
    Promise.resolve(
      filter.kinds.length === 0
        ? { rows: [row("ticket", "T"), row("page", "P"), row("build", "B")], total: 3 }
        : { rows: [], total: 0 },
    );

  const screen = render("#/ctx/all");
  await vi.waitFor(() => expect(screen.tiles().length).toBe(3));
  flushSync();

  const tiles = screen.target.querySelector(".tiles");
  expect(tiles?.className).toContain("rows-2");
  expect(tiles?.getAttribute("style")).toBeNull();

  screen.done();
});
