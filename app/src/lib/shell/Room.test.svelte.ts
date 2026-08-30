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
/** Every note *New note* wrote. */
const written: string[] = [];

vi.mock("../ipc/entity", () => ({
  // The ticket detail's status select (#179) reads the granted board and
  // queues through the write queue. Not what this file is about, so both
  // answer with nothing.
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  submitWrite: () => Promise.reject(new Error("no write in this test")),
  // Contexts (#47): the store imports these at module level, so every mock of
  // this module has to define them even where no context is ever made.
  listContexts: () => Promise.resolve([]),
  contextMembers: () => Promise.resolve([]),
  createContext: () => Promise.reject(new Error("no context creation in this test")),
  promoteContext: () => Promise.reject(new Error("no promotion in this test")),
  // The Tickets tile's read (#178): this file is about the room, not the
  // board, so it answers with an empty one.
  miniBoard: () => Promise.resolve({ columns: [], sources: [] }),
  listEntities: (filter: EntityFilter, limit: number, offset: number) => {
    calls.push({ filter, limit, offset });
    return answer(filter);
  },
  getEntity: (entityId: string): Promise<EntityDetail> =>
    Promise.reject({ code: "not_found", message: `${entityId} is not in the local index`, source_id: null }),
  createNote: () => {
    written.push("note:new");
    return Promise.resolve({
      note: {
        id: "note:new",
        title: "Untitled note",
        body_md: "",
        created_at: "2026-08-22T14:30:00Z",
        updated_at: "2026-08-22T14:30:00Z",
      },
      refs: [],
      links: [],
    });
  },
  // The note view reads once it is mounted, and the room is what mounts it.
  getNote: () =>
    Promise.resolve({
      note: {
        id: "note:new",
        title: "Untitled note",
        body_md: "",
        created_at: "2026-08-22T14:30:00Z",
        updated_at: "2026-08-22T14:30:00Z",
      },
      refs: [],
      links: [],
    }),
  saveNote: () => Promise.reject(new Error("this test never saves")),
  deleteNote: () => Promise.reject(new Error("this test never deletes")),
  unlink: () => Promise.reject(new Error("this test never unlinks")),
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

/** Let every queued promise and the DOM catch up. */
async function settle() {
  for (let turn = 0; turn < 6; turn += 1) {
    await Promise.resolve();
    flushSync();
  }
}

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

/**
 * Story 2, from the affordance end: *New note* writes the row **before** the
 * editor exists, and the address it navigates to is that note's.
 *
 * The order is the point. A *New note* that opened an empty editor and wrote
 * on the first save would lose whatever was typed into a window that closed
 * first, which is the story this whole feature is arranged around.
 */
test("New note writes the note first and opens it", async () => {
  written.length = 0;
  answer = () => Promise.resolve({ rows: [row("ticket", "PAY-1")], total: 1 });
  const screen = render("#/ctx/all");
  await settle();

  const button = [...screen.target.querySelectorAll<HTMLButtonElement>("button")].find(
    (node) => node.textContent?.trim() === "New note",
  );
  expect(button, "the room offers somewhere to start writing").toBeDefined();

  button!.click();
  await settle();

  expect(written).toEqual(["note:new"]);
  expect(location.hash).toBe("#/note/note:new");
  screen.done();
});

/**
 * A note opens in the note view and not in `Detail`.
 *
 * Decided on the id, so `#/entity/<note id>` -- which carries no kind -- lands
 * in the same place. The tell is which read happened: `Detail` calls
 * `get_entity`, which for a note is a `not_found` this file's mock produces on
 * purpose, so a room that sent a note there would draw the not-found panel.
 */
test("a note address opens the note view rather than the mirror's detail", async () => {
  answer = () => Promise.resolve({ rows: [], total: 0 });
  for (const hash of ["#/note/note:new", "#/entity/note:new"]) {
    const screen = render(hash);
    await settle();
    expect(screen.text(), hash).toContain("Untitled note");
    expect(screen.text(), hash).not.toContain("is not in the local index");
    screen.done();
  }
});
