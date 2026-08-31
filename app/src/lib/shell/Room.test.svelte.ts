/**
 * The room decides *which* tiles exist; the tiles decide what is in them.
 *
 * That split is the behaviour worth pinning: a room draws a tile only for a
 * kind its corpus actually holds (§3a), and an undeclared kind gets one of its
 * own rather than being dropped on the floor.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type {
  EntityDetail,
  EntityFilter,
  EntityPage,
  EntityRow,
  MiniBoard,
} from "../ipc/entity";

/** A plain function, not a `vi.fn` — see the note in `Tile.test.svelte.ts`. */
const calls: { filter: EntityFilter; limit: number; offset: number }[] = [];
let answer: (filter: EntityFilter) => Promise<EntityPage> = () =>
  Promise.resolve({ rows: [], total: 0 });
let board: (filter: Pick<EntityFilter, "sources" | "context" | "project">) => Promise<MiniBoard> =
  () => Promise.resolve({ columns: [], sources: [] });

/**
 * `getEntity` too, because opening a row mounts the slide-over — which reads.
 * A mock that stopped at `listEntities` would make the navigation test fail
 * inside Svelte's effect runner, several frames after the assertion it is
 * about.
 */
/** Every note *New note* wrote. */
const written: string[] = [];

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
  // The Tickets tile's read (#178). Empty by default -- this file is about
  // the room, not the board -- but answerable, because a project room's
  // narrowing is only observable through what its tiles draw, and the Tickets
  // tile is a board rather than a list of rows.
  miniBoard: (filter: Pick<EntityFilter, "sources" | "context" | "project">) => board(filter),
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

/**
 * One source, and the two projects its corpus shows (#209).
 *
 * `OPS` deliberately reports no name, so the rooms drawn here cover both
 * labellings — the source's own word where there is one, the key where there
 * is not.
 */
const CONTEXTS = builtinContexts(
  [{ id: "jira", label: "Tidewater Jira" }],
  [
    { source_id: "jira", key: "PAY", name: "Payments Platform" },
    { source_id: "jira", key: "OPS", name: null },
  ],
);

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
    /** The key on each mini-board card, in the order drawn. */
    cards: () => [...target.querySelectorAll<HTMLElement>(".card .mono")].map((k) => k.textContent),
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
  board = () => Promise.resolve({ columns: [], sources: [] });
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

/**
 * A corpus with a project dimension in it, answered by both reads.
 *
 * The mocks **honour** the filter rather than recording it, and that is the
 * point of this fixture: a project room narrows because the filter it hands
 * every tile says so, so the only way to see the narrowing is through what the
 * tiles then draw. A test that asserted on the filter object would pass just
 * as well against a room that passed the right filter to a tile which ignored
 * it.
 *
 * `PAY-236` carries the `PAY` prefix in its key and **no project**, which is
 * the demo corpus' own miss case (#207) and the trap a room keying off the
 * ticket key rather than the record's project would fall into.
 */
interface Item {
  kind: string;
  key: string;
  project: string | null;
}

const CORPUS: Item[] = [
  { kind: "ticket", key: "PAY-231", project: "PAY" },
  { kind: "ticket", key: "PAY-236", project: null },
  { kind: "ticket", key: "OPS-77", project: "OPS" },
  { kind: "page", key: "ENG-1", project: "PAY" },
  { kind: "incident", key: "INC-1", project: "OPS" },
];

/** Everything one source holds, narrowed the way the backend narrows it. */
function corpus(filter: Pick<EntityFilter, "sources" | "project"> & { kinds?: string[] }): Item[] {
  return CORPUS.filter(
    (item) =>
      (filter.sources.length === 0 || filter.sources.includes("jira")) &&
      (filter.project === null || filter.project === item.project) &&
      ((filter.kinds ?? []).length === 0 || (filter.kinds ?? []).includes(item.kind)),
  );
}

/** Point both of the room's reads at {@link CORPUS}. */
function serveCorpus() {
  answer = (filter) => {
    const items = corpus(filter);
    return Promise.resolve({
      rows: items.map((item) => row(item.kind, item.key)),
      total: items.length,
    });
  };
  board = (filter) => {
    const cards = corpus({ ...filter, kinds: ["ticket"] }).map((item) => ({
      entity_id: `mock:${item.key}`,
      source_id: "mock",
      key: item.key,
      title: item.key,
      priority: null,
    }));
    return Promise.resolve({
      columns: cards.length === 0 ? [] : [{ status: "To Do", cards }],
      sources: [],
    });
  };
}

/**
 * Story 2: a project room narrows **every** tile in it, its own kinds-and-count
 * read included.
 *
 * Read through what the room draws, never through the filter it passes: the
 * heading's count, which tiles exist at all, and which cards the mini board
 * puts on screen are three independent readers of one filter, and all three
 * have to be that project's.
 */
test("a project room narrows every tile in it, its own count included", async () => {
  serveCorpus();

  const project = render("#/ctx/proj:jira:PAY");
  await vi.waitFor(() => expect(project.tiles().length).toBeGreaterThan(0));
  await settle();

  expect(project.text()).toContain("Payments Platform");
  expect(project.text()).toContain("project");
  // Its own read: two of the five items this source holds are `PAY`.
  expect(project.text()).toContain("2 items");
  // ...and the incident belongs to `OPS`, so the room has no tile for one.
  expect(project.tiles()).toEqual(["Tickets", "Docs"]);
  expect(project.cards()).toEqual(["PAY-231"]);
  project.done();

  // The same corpus, one room out: everything the project room narrowed away.
  const source = render("#/ctx/src:jira");
  await vi.waitFor(() => expect(source.tiles().length).toBeGreaterThan(0));
  await settle();

  expect(source.text()).toContain("5 items");
  expect(source.tiles()).toEqual(["Tickets", "Docs", "Incidents"]);
  expect(source.cards()).toEqual(["PAY-231", "PAY-236", "OPS-77"]);
  source.done();
});

/**
 * The failure direction, from the reader's end (ADR-0007 requirement 3,
 * ADR-0010): absence, never a wrong room.
 *
 * A ticket whose record carries no readable project is in *All work* and in
 * its source's room, and in no project room — nothing is hidden by a dimension
 * the record does not carry, and there is no "No project" room to put it in.
 */
test("a ticket with no readable project is in All work and its source's room, and in no project room", async () => {
  serveCorpus();

  for (const hash of ["#/ctx/all", "#/ctx/src:jira"]) {
    const screen = render(hash);
    await vi.waitFor(() => expect(screen.cards().length).toBeGreaterThan(0));
    await settle();
    expect(screen.cards(), hash).toContain("PAY-236");
    screen.done();
  }

  for (const hash of ["#/ctx/proj:jira:PAY", "#/ctx/proj:jira:OPS"]) {
    const screen = render(hash);
    await vi.waitFor(() => expect(screen.cards().length).toBeGreaterThan(0));
    await settle();
    expect(screen.cards(), hash).not.toContain("PAY-236");
    screen.done();
  }

  // ...and the switcher offers nowhere else it could have gone.
  expect(CONTEXTS.map((context) => context.label)).not.toContain("No project");
});

/** A project the source named nothing readable is still a room, headed by its key. */
test("a project room with no readable name is headed by its key", async () => {
  serveCorpus();

  const screen = render("#/ctx/proj:jira:OPS");
  await vi.waitFor(() => expect(screen.tiles().length).toBeGreaterThan(0));
  await settle();

  expect(screen.text()).toContain("OPS");
  expect(screen.text()).toContain("2 items");
  expect(screen.tiles()).toEqual(["Tickets", "Incidents"]);
  expect(screen.cards()).toEqual(["OPS-77"]);

  screen.done();
});

/**
 * A project room is a way *into* the work, not a separate world: the card
 * opens the same ticket at the same address it opens at from anywhere else,
 * and `Esc` still returns to the room it was opened over.
 */
test("a card opens at the same address from a project room as from All work", async () => {
  serveCorpus();

  const addresses: string[] = [];
  for (const hash of ["#/ctx/all", "#/ctx/proj:jira:PAY"]) {
    const screen = render(hash);
    await vi.waitFor(() => expect(screen.cards().length).toBeGreaterThan(0));
    await settle();

    screen.target.querySelector<HTMLButtonElement>(".card")?.click();
    flushSync();
    addresses.push(location.hash);
    expect(screen.router.ctx).toBe(hash.slice("#/ctx/".length));
    screen.done();
  }

  expect(addresses).toEqual(["#/ticket/mock:PAY-231", "#/ticket/mock:PAY-231"]);
});
