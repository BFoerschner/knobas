/**
 * The room decides *which* tiles exist; the tiles decide what is in them.
 *
 * That split is the behaviour worth pinning: a room draws a tile only for a
 * kind its corpus actually holds (§3a), and an undeclared kind gets one of its
 * own rather than being dropped on the floor.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { AssetRow, MemberAsset } from "../ipc/assets";
import type {
  ContextRow,
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

/**
 * The Assets tile's three reads (#434, #435), one per kind of room that has
 * one.
 *
 * Plain functions for `Tile.test.svelte.ts`' reason, and mocked at all because
 * the room mounts the tile with no `ports` — the real module reaches Tauri,
 * which is not here. What the tile draws is `AssetsTile.test.svelte.ts`'; what
 * this file is about is *which rooms draw it and what each one asks for*.
 *
 * `assetCalls` records every read as `<kind> <argument>`, so one array can say
 * that a project room asked nothing while a source room asked about itself.
 */
const assetCalls: string[] = [];
let assets: (ctxId: string) => Promise<MemberAsset[]> = () => Promise.resolve([]);
let roots: () => Promise<AssetRow[]> = () => Promise.resolve([]);
let monitored: (sourceId: string) => Promise<MemberAsset[]> = () => Promise.resolve([]);

vi.mock("../ipc/assets", () => ({
  contextAssets: (ctxId: string) => {
    assetCalls.push(`members ${ctxId}`);
    return assets(ctxId);
  },
  assetTree: (parentId?: string | null) => {
    assetCalls.push(`tree ${parentId ?? "null"}`);
    return roots();
  },
  sourceAssets: (sourceId: string) => {
    assetCalls.push(`source ${sourceId}`);
    return monitored(sourceId);
  },
}));

const { default: Room } = await import("./Room.svelte");
const { builtinContexts, storedContext } = await import("./contexts");
const { createRouter } = await import("./router.svelte");
const { createMiniBoardOverrides } = await import("./mini-board-overrides.svelte");

/**
 * Two sources, and the projects their corpus shows (#209).
 *
 * `OPS` deliberately reports no name, so the rooms drawn here cover both
 * labellings — the source's own word where there is one, the key where there
 * is not. TeamCity's `PAY` shares Jira's key on purpose: two sources, one
 * key, and each room may draw only its own source's half.
 */
const CONTEXTS = builtinContexts(
  [
    { id: "jira", label: "Tidewater Jira" },
    { id: "teamcity", label: "TeamCity" },
  ],
  [
    { source_id: "jira", key: "PAY", name: "Payments Platform" },
    { source_id: "jira", key: "OPS", name: null },
    // A project the census shows and this corpus has nothing in: a room exists
    // for a quiet project as well as a busy one.
    { source_id: "jira", key: "QUIET", name: "Quiet project" },
    { source_id: "teamcity", key: "PAY", name: "Payments pipelines" },
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
    path: null,
  };
}

function render(hash: string, overrides = createMiniBoardOverrides()) {
  location.hash = hash;
  const router = createRouter();
  const stop = router.start();
  const target = document.createElement("div");
  document.body.append(target);
  /**
   * The switcher's list, reassignable: `App.svelte` derives it afresh after
   * every census, so a mounted room is handed a new list of the same rooms
   * several times a session (#250).
   */
  let list = $state.raw(CONTEXTS);
  const app = mount(Room, {
    target,
    props: {
      router,
      get contexts() {
        return list;
      },
      overrides,
    },
  });
  flushSync();
  /** The Tickets tile's layout control (#245): the `.seg` in its header's `.acts` slot. */
  const SEG = ".tile-h .acts .seg";
  /** The Tickets tile's layout option that reads `label` (#245), or undefined where none does. */
  function layoutOption(label: string): HTMLButtonElement | undefined {
    return [...target.querySelectorAll<HTMLButtonElement>(`${SEG} button`)].find(
      (node) => node.textContent?.trim() === label,
    );
  }
  /** The maximise control (#250) of the tile labelled `label`, or null where no such tile is drawn. */
  function maxButton(label: string): HTMLButtonElement | null {
    const tile = [...target.querySelectorAll<HTMLElement>(".tile")].find(
      (node) => node.querySelector(".tile-h .lab")?.textContent === label,
    );
    return tile?.querySelector<HTMLButtonElement>(".tile-h .acts .tile-max") ?? null;
  }
  return {
    target,
    router,
    overrides,
    /** The mini board's layout, off its own class (see `MiniBoard.test.svelte.ts`). */
    layout: () => {
      const board = target.querySelector(".board");
      if (!board) return null;
      return [...board.classList].find((name) => name !== "board") ?? null;
    },
    /** Each mini board group as `[heading, count]`, in the order drawn. */
    groups: () =>
      [...target.querySelectorAll(".col")].map((col) =>
        [...col.querySelectorAll(".col-h span")].map((span) => span.textContent ?? ""),
      ),
    /** The Tickets tile's layout control: `[word, pressed, refused, reason]` per option. */
    control: () =>
      [...target.querySelectorAll<HTMLButtonElement>(`${SEG} button`)].map((option) => [
        option.textContent?.trim() ?? "",
        option.getAttribute("aria-pressed") === "true",
        option.getAttribute("aria-disabled") === "true",
        option.getAttribute("title"),
      ]),
    /**
     * What the layout option `label` is described by (#258). A dangling
     * `aria-describedby` is no description to assistive technology, and
     * `getElementById` gives it back as the same null (the precedent is
     * `Modal.test.svelte.ts`).
     */
    describedBy: (label: string) => {
      const id = layoutOption(label)?.getAttribute("aria-describedby");
      if (!id) return null;
      return document.getElementById(id)?.textContent?.trim() ?? null;
    },
    /** The visually hidden reasons the layout control carries, in the order drawn. */
    hiddenReasons: () =>
      [...target.querySelectorAll<HTMLElement>(`${SEG} .vh`)].map((node) => node.textContent?.trim() ?? ""),
    /** Press the layout option that carries `label`. */
    press: (label: string) => {
      const option = layoutOption(label);
      expect(option, `the Tickets tile offers ${label}`).toBeDefined();
      option!.click();
      flushSync();
    },
    tiles: () => [...target.querySelectorAll<HTMLElement>(".tile .tile-h .lab")].map((l) => l.textContent),
    /** The grid's modifier classes (#250): `max` while a tile is maximised. */
    grid: () => [...(target.querySelector(".tiles")?.classList ?? [])].filter((name) => name !== "tiles"),
    /** The maximise control of the tile labelled `label`, by the word it reads. */
    maxButton,
    /** Press the maximise control of the tile labelled `label`. */
    maximise: (label: string) => {
      const button = maxButton(label);
      expect(button, `the ${label} tile is drawn and offers a maximise control`).not.toBeNull();
      button!.click();
      flushSync();
    },
    /** Escape's rung 4, as `App.svelte` reaches it through `installKeys` (#250). */
    restoreTile: () => app.restoreTile(),
    /** Hand the room a fresh list, as the shell does after a census (#250). */
    relist: (next: typeof CONTEXTS) => {
      list = next;
      flushSync();
    },
    /** The key on each mini-board card, in the order drawn. */
    cards: () => [...target.querySelectorAll<HTMLElement>(".card .mono")].map((k) => k.textContent),
    /** Every list tile's rows, by the title they show. */
    rows: () => [...target.querySelectorAll<HTMLElement>(".row .t")].map((t) => t.textContent),
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
  assetCalls.length = 0;
  answer = () => Promise.resolve({ rows: [], total: 0 });
  board = () => Promise.resolve({ columns: [], sources: [] });
  assets = () => Promise.resolve([]);
  roots = () => Promise.resolve([]);
  monitored = () => Promise.resolve([]);
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

  // The Assets tile is last and is not a bucket: it comes from the room's
  // filter (#434, #435), not from the survey, which is why the scan below is
  // still unfiltered by kind.
  expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents", "Assets"]);
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
  await vi.waitFor(() => expect(screen.tiles()).toEqual(["Tickets", "Assets"]));
  flushSync();

  expect(screen.text()).toContain("Tidewater Jira");
  expect(calls[0]?.filter.sources).toEqual(["jira"]);
  // ...and the tile inherits it rather than reading the whole mirror.
  expect(calls.every((call) => call.filter.sources.includes("jira"))).toBe(true);

  screen.done();
});

/**
 * An empty corpus is a sentence, not a grid of empty tiles.
 *
 * A **project** room, because it is the one kind with no Assets tile of its
 * own (story 45): every other room draws one whatever the mirror holds, and an
 * empty *room* is now exactly a room with no tile of any kind.
 */
test("a room with nothing in it draws no tiles at all", async () => {
  const screen = render("#/ctx/proj:jira:QUIET");
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
  // Three buckets and the room's own Assets tile.
  await vi.waitFor(() => expect(screen.tiles().length).toBe(4));
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
  source: string;
}

/**
 * `PAY-9` is the **second source's** `PAY` — the corpus item that makes the
 * source half of a project room's filter witnessable here: a room that
 * narrowed by its key alone, or dropped `sources` on the way to a tile, would
 * draw it into `proj:jira:PAY` and the counts and cards below would say so.
 * Without it this corpus was single-source and that mutant rendered
 * identically (the mutant-8 lesson, one dimension over).
 */
const CORPUS: Item[] = [
  { kind: "ticket", key: "PAY-231", project: "PAY", source: "jira" },
  { kind: "ticket", key: "PAY-236", project: null, source: "jira" },
  { kind: "ticket", key: "OPS-77", project: "OPS", source: "jira" },
  { kind: "page", key: "ENG-1", project: "PAY", source: "jira" },
  { kind: "page", key: "OPS-DOC", project: "OPS", source: "jira" },
  { kind: "incident", key: "INC-1", project: "OPS", source: "jira" },
  { kind: "ticket", key: "PAY-9", project: "PAY", source: "teamcity" },
];

/** Everything the corpus holds, narrowed the way the backend narrows it. */
function corpus(filter: Pick<EntityFilter, "sources" | "project"> & { kinds?: string[] }): Item[] {
  return CORPUS.filter(
    (item) =>
      (filter.sources.length === 0 || filter.sources.includes(item.source)) &&
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
 * Per-tile maximise (#250), through what the room draws.
 *
 * Three tiles on purpose: a room of one could not witness "the others are
 * not drawn", and a room of two could not tell "the others" from "the other".
 * The grid's class is asserted beside the tile count because the two are
 * different rules -- the `each` decides what is drawn, the stylesheet decides
 * how much of the grid it gets -- and either could be dropped alone.
 */
test("maximising a tile draws it alone and Restore draws the grid again", async () => {
  serveCorpus();
  const screen = render("#/ctx/src:jira");
  await vi.waitFor(() =>
    expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents", "Assets"]),
  );
  await settle();
  expect(screen.grid()).not.toContain("max");
  expect(screen.maxButton("Docs")?.textContent?.trim()).toBe("Maximise");

  screen.maximise("Docs");
  expect(screen.tiles()).toEqual(["Docs"]);
  expect(screen.grid()).toContain("max");
  expect(screen.maxButton("Docs")?.textContent?.trim()).toBe("Restore");
  // The tile kept its own read; the tray under the grid kept its place.
  expect(screen.rows()).toEqual(["Title of ENG-1", "Title of OPS-DOC"]);
  expect(screen.target.querySelector(".tray"), "the tray stays under a maximised tile").not.toBeNull();

  screen.maximise("Docs");
  await settle();
  expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents", "Assets"]);
  expect(screen.grid()).not.toContain("max");
  expect(screen.maxButton("Docs")?.textContent?.trim()).toBe("Maximise");

  // The next choice, from the grid, is the whole of the choice. A *second*
  // maximise over a maximised tile has no button to come from -- the other
  // tiles are not drawn -- so decision 2 holds by the shape of the field, and
  // this is the nearest thing the room's own surface can witness.
  screen.maximise("Tickets");
  expect(screen.tiles()).toEqual(["Tickets"]);
  expect(screen.cards()).toEqual(["PAY-231", "PAY-236", "OPS-77"]);

  screen.done();
});

/**
 * Decision 1: a maximised tile is a viewing gesture of *this visit*. Walking
 * to another room draws that room's grid, and walking back finds the grid
 * too -- nothing waited. The second room holds a Docs tile on purpose: a
 * choice keyed by nothing would carry `Docs` into it and draw that one tile,
 * where a room without Docs would draw nothing and blur the two failures.
 */
test("walking to another room restores the grid, and walking back finds it restored", async () => {
  serveCorpus();
  const screen = render("#/ctx/src:jira");
  await vi.waitFor(() =>
    expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents", "Assets"]),
  );
  await settle();
  screen.maximise("Docs");
  expect(screen.tiles()).toEqual(["Docs"]);

  screen.router.go("#/ctx/proj:jira:PAY");
  await settle();
  await vi.waitFor(() => expect(screen.tiles()).toEqual(["Tickets", "Docs"]));
  expect(screen.grid()).not.toContain("max");

  screen.router.go("#/ctx/src:jira");
  await settle();
  await vi.waitFor(() =>
    expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents", "Assets"]),
  );
  expect(screen.grid()).not.toContain("max");

  screen.done();
});

/**
 * Decision 1, the other way round: the room's *id* is what resets the choice,
 * not the object that carries it. `App.svelte` derives the switcher's list
 * afresh after every census -- `projects.reseed()` on each `sync:state` that
 * ends a run, `health.replace()` on each `source:health` -- so a mounted room
 * is handed a new `RoomContext` for the same id several times a session. A
 * reset keyed on the object would draw the grid under the reader every time
 * a sync finished. The copies are new objects with the same ids, which is
 * exactly what `switcherContexts` hands down.
 */
test("a fresh list of the same rooms leaves the tile maximised", async () => {
  serveCorpus();
  const screen = render("#/ctx/src:jira");
  await vi.waitFor(() =>
    expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents", "Assets"]),
  );
  await settle();
  screen.maximise("Docs");
  expect(screen.tiles()).toEqual(["Docs"]);

  screen.relist(CONTEXTS.map((context) => ({ ...context, filter: { ...context.filter } })));
  // The room re-reads its kinds for the new object; wait for that read to land.
  await settle();
  await vi.waitFor(() => expect(screen.tiles()).not.toEqual([]));
  expect(screen.tiles()).toEqual(["Docs"]);
  expect(screen.grid()).toContain("max");
  expect(screen.maxButton("Docs")?.textContent?.trim()).toBe("Restore");

  screen.done();
});

/**
 * The detail over a maximised tile (decisions 1 and 4): opening an item from
 * the maximised tile opens the slide-over as it does from the grid, and
 * neither opening nor closing it touches the tile. Escape's *order* is the
 * ladder's and pinned in `keys.test.svelte.ts`; what the room owns is that
 * the state survives the detail's round trip, and that `restoreTile` -- the
 * rung's handle -- says whether it had anything to do.
 */
test("a detail opens over the maximised tile and leaves it maximised; restoreTile answers honestly", async () => {
  serveCorpus();
  const screen = render("#/ctx/src:jira");
  await vi.waitFor(() =>
    expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents", "Assets"]),
  );
  await settle();

  expect(screen.restoreTile(), "nothing to restore in a plain room").toBe(false);
  screen.maximise("Docs");

  screen.target.querySelector<HTMLButtonElement>(".row")?.click();
  await settle();
  expect(location.hash).toBe("#/page/mock:ENG-1");
  expect(screen.target.querySelector(".detail"), "the slide-over opened").not.toBeNull();
  expect(screen.tiles()).toEqual(["Docs"]);

  screen.router.back();
  await settle();
  expect(screen.target.querySelector(".detail")).toBeNull();
  expect(screen.tiles()).toEqual(["Docs"]);

  expect(screen.restoreTile()).toBe(true);
  flushSync();
  await settle();
  expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents", "Assets"]);
  expect(screen.restoreTile(), "a second press has nothing left to restore").toBe(false);

  screen.done();
});

/**
 * Decision 5: the mini board's layout rule does not know about maximise. A
 * stacked-default room past the backstop is drawn stacked in the maximised
 * Tickets tile too, with columns refused for the same reason.
 */
test("a maximised Tickets tile follows the unchanged layout rule", async () => {
  ticketsEverywhere();
  board = () => Promise.resolve(statusBoard(7));
  const screen = render("#/ctx/src:jira");
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(7));
  await settle();

  screen.maximise("Tickets");
  expect(screen.tiles()).toEqual(["Tickets"]);
  expect(screen.layout()).toBe("stacked");
  expect(screen.control()).toEqual([
    ["columns", false, true, "7 statuses; columns holds 6"],
    ["stacked", true, false, null],
  ]);

  screen.done();
});

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
  // Its own read: two of the six items this source holds are `PAY`.
  expect(project.text()).toContain("2 items");
  // ...and the incident belongs to `OPS`, so the room has no tile for one.
  expect(project.tiles()).toEqual(["Tickets", "Docs"]);
  // The board, and a list tile beside it: *every* tile in the room narrows,
  // not only the one the room was designed around.
  expect(project.cards()).toEqual(["PAY-231"]);
  expect(project.rows()).toEqual(["Title of ENG-1"]);
  project.done();

  // The same corpus, one room out: everything the project room narrowed away.
  const source = render("#/ctx/src:jira");
  await vi.waitFor(() => expect(source.tiles().length).toBeGreaterThan(0));
  await settle();

  expect(source.text()).toContain("6 items");
  expect(source.tiles()).toEqual(["Tickets", "Docs", "Incidents", "Assets"]);
  expect(source.cards()).toEqual(["PAY-231", "PAY-236", "OPS-77"]);
  expect(source.rows()).toEqual(["Title of ENG-1", "Title of OPS-DOC", "Title of INC-1"]);
  source.done();
});

/**
 * Criterion 4 at the rendered seam: one key in two sources is two rooms, and
 * what each room **draws** is its own source's half only. The filter-level
 * pin is `contexts.test.ts`'s; this is the room actually not painting the
 * other source's `PAY-9` / `PAY-231`.
 */
test("two sources' rooms for one key each draw only their own source's work", async () => {
  serveCorpus();

  const jira = render("#/ctx/proj:jira:PAY");
  await vi.waitFor(() => expect(jira.tiles().length).toBeGreaterThan(0));
  await settle();
  expect(jira.text()).toContain("2 items");
  expect(jira.cards()).toEqual(["PAY-231"]);
  jira.done();

  const teamcity = render("#/ctx/proj:teamcity:PAY");
  await vi.waitFor(() => expect(teamcity.tiles().length).toBeGreaterThan(0));
  await settle();
  expect(teamcity.text()).toContain("Payments pipelines");
  expect(teamcity.text()).toContain("1 item");
  expect(teamcity.cards()).toEqual(["PAY-9"]);
  teamcity.done();
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
  expect(screen.text()).toContain("3 items");
  expect(screen.tiles()).toEqual(["Tickets", "Docs", "Incidents"]);
  expect(screen.cards()).toEqual(["OPS-77"]);
  expect(screen.rows()).toEqual(["Title of OPS-DOC", "Title of INC-1"]);

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

/**
 * A quiet project still gets a room, and an empty one reads as empty rather
 * than as broken — the room says what is missing, and a failed read would say
 * something else entirely.
 */
test("an empty project room says so, distinguishably from a broken one", async () => {
  serveCorpus();

  const screen = render("#/ctx/proj:jira:QUIET");
  await vi.waitFor(() => expect(screen.text()).toContain("Nothing synced into this room yet"));
  await settle();

  expect(screen.text()).toContain("Quiet project");
  expect(screen.text()).toContain("0 items");
  expect(screen.text()).not.toContain("the database is still starting");
  expect(screen.tiles()).toEqual([]);

  screen.done();
});

/**
 * The room's layout answer actually reaches the board it is about (#210).
 *
 * Pinned here because nowhere else can see it. `contexts.ts` knows which
 * layout each room kind carries and the mini board knows how to draw the one
 * it is handed, and both are tested where they live — but a room that dropped
 * the answer on the way to the tile, or hardcoded one, satisfies both of those
 * and still draws every room the same. So: one board fixture, two rooms, and
 * the difference has to come from the rooms.
 */
test("a room hands its own layout to the mini board it draws", async () => {
  const STATUSES = ["To Do", "In Progress", "Done"];
  answer = (filter) =>
    Promise.resolve(
      filter.kinds.length === 0 || filter.kinds.includes("ticket")
        ? { rows: [row("ticket", "PAY-231")], total: 1 }
        : { rows: [], total: 0 },
    );
  board = () =>
    Promise.resolve({
      columns: STATUSES.map((status) => ({
        status,
        cards: [
          {
            entity_id: `mock:${status}`,
            source_id: "mock",
            key: status,
            title: status,
            priority: null,
          },
        ],
      })),
      sources: [],
    });

  const drawn: Record<string, string | undefined> = {};
  for (const hash of ["#/ctx/src:jira", "#/ctx/proj:jira:PAY"]) {
    const screen = render(hash);
    await vi.waitFor(() => expect(screen.cards()).toHaveLength(3));
    await settle();
    drawn[hash] = screen.target.querySelector(".board")?.className;
    screen.done();
  }

  expect(drawn).toEqual({
    "#/ctx/src:jira": "board stacked",
    "#/ctx/proj:jira:PAY": "board columns",
  });
});

/** A board of `n` distinct statuses, one card each, as the read would give it. */
function statusBoard(n: number): MiniBoard {
  return {
    columns: Array.from({ length: n }, (_, at) => ({
      status: `S${at + 1}`,
      cards: [
        { entity_id: `mock:S${at + 1}`, source_id: "mock", key: `PAY-${at + 1}`, title: `S${at + 1}`, priority: null },
      ],
    })),
    sources: [],
  };
}

/** Every room holds a ticket, so every room draws a Tickets tile. */
function ticketsEverywhere() {
  answer = (filter) =>
    Promise.resolve(
      filter.kinds.length === 0 || filter.kinds.includes("ticket")
        ? { rows: [row("ticket", "PAY-231")], total: 1 }
        : { rows: [], total: 0 },
    );
}

/**
 * The wire from the control to the store and back to the board (#245):
 * pressing the other layout redraws this room's mini board in it, the choice
 * is the **room's** — walking to another room finds that room's default, and
 * walking back finds the override still in force — and pressing the room's
 * own default clears it rather than recording it.
 *
 * Both endpoints are tested where they live (`mini-board-overrides`,
 * `MiniBoard.test.svelte.ts`); what only a mounted room can witness is that
 * the room keys the store by its own id and hands the answer to its tile. The
 * second room is a *columns* room on purpose: a store keyed by anything
 * constant would carry `stacked` into it, and a stacked second room could not
 * tell.
 */
test("a reader's override redraws the mini board, sticks to its room, and clears on the default", async () => {
  const STATUSES = ["Done", "To Do", "In Progress"];
  ticketsEverywhere();
  board = () =>
    Promise.resolve({
      columns: STATUSES.map((status) => ({
        status,
        cards: [{ entity_id: `mock:${status}`, source_id: "mock", key: status, title: status, priority: null }],
      })),
      sources: [],
    });
  const GROUPS = [
    ["Done", "1"],
    ["To Do", "1"],
    ["In Progress", "1"],
  ];

  const screen = render("#/ctx/proj:jira:PAY");
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(3));
  await settle();
  expect(screen.layout()).toBe("columns");
  expect(screen.control()).toEqual([
    ["columns", true, false, null],
    ["stacked", false, false, null],
  ]);

  screen.press("stacked");
  expect(screen.layout()).toBe("stacked");
  expect(screen.groups()).toEqual(GROUPS);
  expect(screen.control().map(([word, on]) => [word, on])).toEqual([
    ["columns", false],
    ["stacked", true],
  ]);

  // Another columns room: its own default, not this room's override.
  screen.router.go("#/ctx/proj:jira:OPS");
  await settle();
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(3));
  await settle();
  expect(screen.layout()).toBe("columns");

  // ...and back: the override waited here.
  screen.router.go("#/ctx/proj:jira:PAY");
  await settle();
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(3));
  await settle();
  expect(screen.layout()).toBe("stacked");
  expect(screen.groups()).toEqual(GROUPS);

  // The room's own default clears the override; nothing is recorded.
  screen.press("columns");
  expect(screen.layout()).toBe("columns");
  expect(screen.groups()).toEqual(GROUPS);
  expect(screen.overrides.overrideFor("proj:jira:PAY")).toBeUndefined();

  screen.done();
});

/**
 * The backstop through the room: a room whose board draws seven statuses is
 * stacked whatever the reader presses, the control says why, and a refused
 * press reaches the store no more than it reaches the board.
 *
 * A **stacked-default** room on purpose. On a columns room a press that
 * leaked through would choose the room's own default, which clears, and the
 * store would look untouched either way; here the leak would record
 * `columns`, and the last assertion is what sees it.
 *
 * The reason reaches readers who cannot hover (#258): the refused option's
 * accessible description resolves to the same string its `title` carries,
 * and the option that is not refused describes itself with nothing.
 */
test("a room past six statuses refuses columns with its reason, and records no override", async () => {
  ticketsEverywhere();
  board = () => Promise.resolve(statusBoard(7));

  const screen = render("#/ctx/src:jira");
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(7));
  await settle();

  expect(screen.layout()).toBe("stacked");
  expect(screen.control()).toEqual([
    ["columns", false, true, "7 statuses; columns holds 6"],
    ["stacked", true, false, null],
  ]);
  expect(screen.describedBy("columns")).toBe("7 statuses; columns holds 6");
  expect(screen.describedBy("stacked")).toBeNull();
  expect(screen.hiddenReasons()).toEqual(["7 statuses; columns holds 6"]);

  screen.press("columns");
  expect(screen.layout()).toBe("stacked");
  expect(screen.overrides.overrideFor("src:jira")).toBeUndefined();

  screen.done();
});

/**
 * Where decision 7 meets decision 8 (#245). A reader chose `columns` on a
 * stacked-default room and the board then grew past six: the override is
 * kept, stacked is drawn, and the control shows `stacked` pressed with
 * `columns` refused. Pressing the pressed `stacked` is the reader asking for
 * the room's own default, and the default clears — so once the board fits
 * six again the room draws stacked, where a kept override would have drawn
 * columns. The return trip is what lets the board, not only the store,
 * witness the clear.
 *
 * The store is seeded before the mount because the override predates this
 * visit: it is what a walk back into the room finds.
 */
test("pressing the effective default on a demoted override clears it, seen once the board fits", async () => {
  ticketsEverywhere();
  board = () => Promise.resolve(statusBoard(7));
  const overrides = createMiniBoardOverrides();
  const source = CONTEXTS.find((context) => context.id === "src:jira")!;
  expect(source.miniBoardLayout).toBe("stacked");
  overrides.choose(source, "columns");

  const screen = render("#/ctx/src:jira", overrides);
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(7));
  await settle();
  expect(screen.layout()).toBe("stacked");
  expect(screen.control()).toEqual([
    ["columns", false, true, "7 statuses; columns holds 6"],
    ["stacked", true, false, null],
  ]);

  screen.press("stacked");
  expect(screen.layout()).toBe("stacked");
  expect(overrides.overrideFor("src:jira")).toBeUndefined();

  // Away, and back to a board that fits: the room's default draws, not the
  // columns the override once asked for.
  board = () => Promise.resolve(statusBoard(5));
  screen.router.go("#/ctx/proj:jira:PAY");
  await settle();
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(5));
  board = () => Promise.resolve(statusBoard(6));
  screen.router.go("#/ctx/src:jira");
  await settle();
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(6));
  await settle();
  expect(screen.layout()).toBe("stacked");
  expect(screen.control().map(([word, on, refused]) => [word, on, refused])).toEqual([
    ["columns", false, false],
    ["stacked", true, false],
  ]);
  // Allowed again, so no reason to give (#258): no description on either
  // option, and no hidden element left behind for a reader to stumble on.
  expect(screen.describedBy("columns")).toBeNull();
  expect(screen.describedBy("stacked")).toBeNull();
  expect(screen.hiddenReasons()).toEqual([]);

  screen.done();
});

// ---------------------------------------------------------------------------
// The Assets tile (#434)
// ---------------------------------------------------------------------------

/** A stored room: a context somebody made, whose room narrows by membership. */
const STORED: ContextRow = {
  id: "ctx:pay",
  kind: "adhoc",
  title: "payments stack",
  anchor_id: null,
  created_at: "2026-09-06T09:00:00Z",
  archived_at: null,
};

/** One member asset, as the tile's read answers with it. */
function memberAsset(name: string, path: string | null): MemberAsset {
  return {
    asset: {
      id: `asset:${name}`,
      parent_id: null,
      type_id: "vm",
      type_label: "VM",
      monogram: "VM",
      name,
      status: "none",
      environment: null,
      owner: null,
      has_children: false,
      health: "up",
      inside: "none",
      problems_inside: 0,
      linked_work: 0,
    },
    path,
  };
}

/**
 * Which rooms draw an Assets tile, and what each one asks for (spec #427's
 * per-room rule, #434 and #435).
 *
 * All four kinds in one test, over one mounted shell, because the claim is
 * about the *switch*: a tile drawn for every room would pass a stored-room
 * assertion, and a tile drawn for none would pass the project room's. The read
 * is the second half — a project room must not merely hide the tile, it must
 * not ask the backend anything, and a source room must ask about **itself**
 * rather than about the project inside it.
 *
 * The *set* of reads, not the number: the room re-mounts its tiles while its
 * own survey is in flight, so how often a tile reads is the room's business
 * and what it reads for is this one's.
 */
test("each kind of room draws the Assets tile its rule gives it, and reads for itself", async () => {
  ticketsEverywhere();
  assets = () => Promise.resolve([memberAsset("vm-db-01", "hel1")]);
  roots = () => Promise.resolve([memberAsset("hel1", null).asset]);
  monitored = () => Promise.resolve([memberAsset("postgres", "hel1 / vm-db-01")]);

  // All work: the estate's top level.
  const screen = render("#/ctx/all");
  await vi.waitFor(() => expect(screen.tiles()).toContain("Tickets"));
  await settle();
  expect(screen.tiles()).toContain("Assets");
  expect([...new Set(assetCalls)]).toEqual(["tree null"]);
  expect(screen.text()).toContain("hel1");

  // A project room: nothing at all, and nothing asked.
  assetCalls.length = 0;
  screen.router.go("#/ctx/proj:jira:PAY");
  await settle();
  expect(screen.tiles(), "a project is a source's grouping of its own items").not.toContain(
    "Assets",
  );
  expect(assetCalls).toEqual([]);

  // A source room: its own monitors' assets.
  assetCalls.length = 0;
  screen.router.go("#/ctx/src:jira");
  await settle();
  expect(screen.tiles()).toContain("Assets");
  expect([...new Set(assetCalls)]).toEqual(["source jira"]);
  expect(screen.text()).toContain("postgres");

  // A stored room: its member assets, with the path each sits at.
  assetCalls.length = 0;
  screen.relist([...CONTEXTS, storedContext(STORED)]);
  screen.router.go("#/ctx/ctx:pay");
  await settle();
  expect(screen.tiles()).toContain("Assets");
  expect([...new Set(assetCalls)]).toEqual(["members ctx:pay"]);
  expect(screen.text()).toContain("vm-db-01");
  expect(screen.text(), "with the path it sits at").toContain("hel1");

  screen.done();
});

/**
 * A stored room whose corpus is empty is not an empty room: the estate is
 * knobas' own and no sync ever puts an asset in the mirror, so the survey that
 * decides the kind tiles cannot see one.
 *
 * Without this the room would draw *"Nothing synced into this room yet"* over
 * a context that holds four servers — which is the failure the room's tile
 * count exists to avoid, and it is the reason the Assets tile is counted
 * before the empty state is chosen rather than after.
 */
test("a stored room with assets and nothing synced draws the tile, not the empty page", async () => {
  assets = () => Promise.resolve([memberAsset("hel1", null)]);

  // A project room is where the empty page still lives, and it is the room
  // this one has to be told apart from.
  const screen = render("#/ctx/proj:jira:QUIET");
  await vi.waitFor(() => expect(screen.text()).toContain("Nothing synced into this room yet"));

  screen.relist([...CONTEXTS, storedContext(STORED)]);
  screen.router.go("#/ctx/ctx:pay");
  await settle();

  expect(screen.tiles()).toEqual(["Assets"]);
  expect(screen.text()).not.toContain("Nothing synced into this room yet");
  expect(screen.grid(), "one tile is a list, not a two-column board").toContain("one");
  expect(screen.text()).toContain("top level");

  screen.done();
});

/** The maximise gesture is the room's, so it reaches the Assets tile too (#250). */
test("the Assets tile maximises and restores like any other tile", async () => {
  ticketsEverywhere();
  assets = () => Promise.resolve([memberAsset("vm-db-01", "hel1")]);

  const screen = render("#/ctx/all");
  await vi.waitFor(() => expect(screen.tiles()).toContain("Tickets"));
  screen.relist([...CONTEXTS, storedContext(STORED)]);
  screen.router.go("#/ctx/ctx:pay");
  await settle();
  expect(screen.tiles()).toEqual(["Tickets", "Assets"]);

  screen.maximise("Assets");
  expect(screen.tiles()).toEqual(["Assets"]);
  expect(screen.grid()).toContain("max");
  expect(screen.maxButton("Assets")?.textContent?.trim()).toBe("Restore");

  expect(screen.restoreTile()).toBe(true);
  flushSync();
  expect(screen.tiles()).toEqual(["Tickets", "Assets"]);

  screen.done();
});
