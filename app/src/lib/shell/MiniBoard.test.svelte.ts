/**
 * The Tickets tile is the mini board (#178, ADR-0009).
 *
 * It is still a tile — same header, same three states of its own read, same
 * stale-answer guard — so those are exercised here through the board rather
 * than assumed from `Tile.test.svelte.ts`, which now stands for the tiles that
 * are still lists.
 *
 * What is new is the body: a column per observed status in the order the read
 * gave them, a count per column, and cards that click through to a stable
 * address. **The order is the command's**, so nothing here re-sorts anything;
 * a test that sorted the columns itself would pass against a client that had
 * quietly taken the grouping back off the backend.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityFilter, MiniBoard, MiniBoardCard, MiniBoardColumn } from "../ipc/entity";
import type { MiniBoardLayout } from "./contexts";

/**
 * Plain functions rather than `vi.fn`, for the reason `Tile.test.svelte.ts`
 * spells out: a `vi.fn` keeps a derived rejected promise with no handler on
 * it, and the run then fails with an "Unknown Error" that looks exactly like
 * the product bug the failed-read test exists to catch.
 */
const calls: Pick<EntityFilter, "sources" | "context" | "project">[] = [];
let answer: () => Promise<MiniBoard> = () => Promise.resolve({ columns: [], sources: [] });

vi.mock("../ipc/entity", () => ({
  miniBoard: (filter: Pick<EntityFilter, "sources" | "context" | "project">) => {
    calls.push(filter);
    return answer();
  },
  listEntities: () => Promise.resolve({ rows: [], total: 0 }),
}));

const { default: Tile } = await import("./Tile.svelte");

const SPEC = { id: "tickets", label: "Tickets", kinds: ["ticket"] };

function card(key: string, priority: string | null = null): MiniBoardCard {
  return {
    entity_id: `mock:${key}`,
    source_id: "mock",
    key,
    title: `Title of ${key}`,
    priority,
  };
}

function column(status: string | null, ...cards: MiniBoardCard[]): MiniBoardColumn {
  return { status, cards };
}

/** A promise plus the two handles that decide when it answers. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function render(
  sources: string[] = [],
  ctx: string | null = null,
  project: string | null = null,
  miniBoardLayout: MiniBoardLayout = "columns",
) {
  const target = document.createElement("div");
  document.body.append(target);
  const onopen = vi.fn();
  const props = $state({ spec: SPEC, sources, ctx, project, miniBoardLayout, onopen });
  const app = mount(Tile, { target, props });
  flushSync();
  return {
    target,
    props,
    onopen,
    /** Each column as `[heading, count]`, in the order drawn. */
    columns: () =>
      [...target.querySelectorAll(".col")].map((col) =>
        [...col.querySelectorAll(".col-h span")].map((span) => span.textContent ?? ""),
      ),
    cards: () => [...target.querySelectorAll<HTMLButtonElement>(".card")],
    /**
     * The layout actually drawn, off the board's own class.
     *
     * The two layouts differ in one thing — how the same groups are arranged —
     * and arrangement is `app.css`'s job, because a computed
     * `grid-template`/`overflow` would be the inline style `style-src 'self'`
     * drops in a bundle. So the class *is* the rendered decision, and
     * `app-css.test.ts` pins that each class still carries its axis.
     */
    layout: () => {
      const board = target.querySelector(".board");
      if (!board) return null;
      return [...board.classList].find((name) => name !== "board") ?? null;
    },
    text: () => target.textContent ?? "",
    count: () => target.querySelector(".tile-h .cnt")?.textContent ?? "",
    label: () => target.querySelector(".tile-h .lab")?.textContent ?? "",
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  calls.length = 0;
  answer = () => Promise.resolve({ columns: [], sources: [] });
});

/**
 * The order is deliberately **not** the board's own: these columns arrive
 * scrambled, and the tile draws them scrambled. A fixture already in board
 * order would pass just as well against a client that had quietly taken the
 * sorting back off the backend, which is the drift this pins shut — one board,
 * one opinion about it, and it is the command's (#177).
 */
test("draws a column per status the read gave, in that order, each with its count", async () => {
  answer = () =>
    Promise.resolve({
      columns: [
        column("Done", card("PAY-201")),
        column(null, card("PAY-9")),
        column("To Do", card("PAY-240"), card("PAY-236")),
        column("In Progress", card("PAY-231")),
      ],
      sources: [],
    });
  const screen = render();
  await vi.waitFor(() => expect(screen.columns()).toHaveLength(4));
  flushSync();

  expect(screen.columns()).toEqual([
    ["Done", "1"],
    ["No status", "1"],
    ["To Do", "2"],
    ["In Progress", "1"],
  ]);

  screen.done();
});

test("the header keeps its label and counts every card on the board", async () => {
  answer = () =>
    Promise.resolve({
      columns: [column("To Do", card("PAY-1"), card("PAY-2")), column("Done", card("PAY-3"))],
      sources: [],
    });
  const screen = render();
  await vi.waitFor(() => expect(screen.count()).toBe("3"));
  flushSync();

  expect(screen.label()).toBe("Tickets");

  screen.done();
});

test("a card shows its key, its priority and its title", async () => {
  answer = () =>
    Promise.resolve({ columns: [column("To Do", card("PAY-240", "Medium"))], sources: [] });
  const screen = render();
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(1));
  flushSync();

  const [only] = screen.cards();
  expect(only?.querySelector(".k .mono")?.textContent).toBe("PAY-240");
  expect(only?.querySelector(".k .pr")?.textContent).toBe("Medium");
  expect(only?.querySelector(".s")?.textContent).toBe("Title of PAY-240");

  screen.done();
});

/**
 * The other half of the read's pinned miss (ADR-0007): a record that says no
 * priority renders none. Absent from the DOM, not an empty element — a blank
 * chip reads as a priority somebody deleted.
 */
test("a card whose record carries no priority simply omits it", async () => {
  answer = () => Promise.resolve({ columns: [column("To Do", card("PAY-240"))], sources: [] });
  const screen = render();
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(1));
  flushSync();

  expect(screen.cards()[0]?.querySelector(".k .pr")).toBeNull();
  expect(screen.cards()[0]?.querySelector(".s")?.textContent).toBe("Title of PAY-240");

  screen.done();
});

test("clicking a card opens that ticket at its stable address", async () => {
  answer = () =>
    Promise.resolve({
      columns: [column("To Do", card("PAY-240")), column("Done", card("PAY-201"))],
      sources: [],
    });
  const screen = render();
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(2));
  flushSync();

  screen.cards()[1]?.click();
  flushSync();

  expect(screen.onopen).toHaveBeenCalledWith({ kind: "ticket", entity_id: "mock:PAY-201" });

  screen.done();
});

test("the tile reads with the room's own filter, every dimension of it", async () => {
  const screen = render(["jira"], null);
  await vi.waitFor(() => expect(calls).toHaveLength(1));

  expect(calls[0]).toEqual({ sources: ["jira"], context: null, project: null });

  screen.props.ctx = "ctx:5b1c";
  screen.props.sources = [];
  flushSync();
  await vi.waitFor(() => expect(calls).toHaveLength(2));
  expect(calls[1]).toEqual({ sources: [], context: "ctx:5b1c", project: null });

  // A project room's, which is the one that carries two at once: the key is
  // unique only inside its own source, so the board is told both (#209).
  screen.props.ctx = null;
  screen.props.sources = ["jira"];
  screen.props.project = "PAY";
  flushSync();
  await vi.waitFor(() => expect(calls).toHaveLength(3));
  expect(calls[2]).toEqual({ sources: ["jira"], context: null, project: "PAY" });

  screen.done();
});

/** An empty board is distinguishable from a broken one. */
test("an empty board says there is no ticket in this room", async () => {
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("No ticket in this room yet."));
  flushSync();

  expect(screen.columns()).toEqual([]);

  screen.done();
});

/**
 * A failed read is a *message*. A board that silently drew nothing on a
 * rejection is indistinguishable from a room with no ticket in it, which is
 * the one thing the empty copy above exists to say.
 */
test("a failed read says so rather than drawing an empty board", async () => {
  answer = () => Promise.reject({ code: "internal", message: "the mirror is unreadable" });
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("the mirror is unreadable"));
  flushSync();

  expect(screen.text()).not.toContain("No ticket in this room yet.");
  expect(screen.columns()).toEqual([]);

  screen.done();
});

/**
 * The stale-answer guard, over the new read: a room switch clears the board
 * *before* the request, and an answer to the previous room's question is
 * dropped when it finally lands. Without both halves a slow switch paints the
 * room you just left.
 */
test("a slow room switch never paints the previous room's board", async () => {
  const first = deferred<MiniBoard>();
  answer = () => first.promise;
  const screen = render([], "ctx:one");
  await vi.waitFor(() => expect(calls).toHaveLength(1));

  const second = deferred<MiniBoard>();
  answer = () => second.promise;
  screen.props.ctx = "ctx:two";
  flushSync();
  await vi.waitFor(() => expect(calls).toHaveLength(2));

  // The previous room's answer arrives late, and is the loser.
  first.resolve({ columns: [column("To Do", card("STALE-1"))], sources: [] });
  await first.promise;
  flushSync();
  expect(screen.text()).not.toContain("STALE-1");

  second.resolve({ columns: [column("Done", card("FRESH-1"))], sources: [] });
  await second.promise;
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(1));
  expect(screen.text()).toContain("FRESH-1");
  expect(screen.text()).not.toContain("STALE-1");

  screen.done();
});

/**
 * The other half of that guard, and the half a dropped late answer cannot
 * prove: the switch **clears** the board it had.
 *
 * The test above never paints a first board — the read it supersedes is still
 * in flight — so it pins only that the loser's answer is discarded. A tile
 * that kept `board` across the switch would pass it while leaving the previous
 * room's cards on screen for as long as the new read takes, which is the thing
 * the ticket forbids. So: paint a room, leave it, and look at the tile while
 * the next read is still in flight. It says it is reading — not that the new
 * room is empty, which is a different sentence about a different fact.
 */
test("leaving a room takes its board off the screen while the next read is in flight", async () => {
  const first = deferred<MiniBoard>();
  answer = () => first.promise;
  const screen = render([], "ctx:one");
  await vi.waitFor(() => expect(calls).toHaveLength(1));
  first.resolve({ columns: [column("To Do", card("STALE-1"))], sources: [] });
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(1));
  expect(screen.text()).toContain("STALE-1");

  const second = deferred<MiniBoard>();
  answer = () => second.promise;
  screen.props.ctx = "ctx:two";
  flushSync();
  await vi.waitFor(() => expect(calls).toHaveLength(2));

  expect(screen.text()).not.toContain("STALE-1");
  expect(screen.columns()).toEqual([]);
  expect(screen.count()).toBe("");
  expect(screen.text()).toContain("Reading…");
  expect(screen.text()).not.toContain("No ticket in this room yet.");

  second.resolve({ columns: [column("Done", card("FRESH-1"))], sources: [] });
  await vi.waitFor(() => expect(screen.cards()).toHaveLength(1));
  expect(screen.text()).toContain("FRESH-1");

  screen.done();
});

/** The four statuses of one workflow, scrambled, plus the terminal group. */
function workflow(): MiniBoardColumn[] {
  return [
    column("To Do", card("PAY-240"), card("PAY-236")),
    column("In Progress", card("PAY-231")),
    column("Done", card("PAY-201")),
    column(null, card("PAY-9")),
  ];
}

/** What both layouts have to agree about: the groups, their order, their counts. */
const WORKFLOW_COLUMNS = [
  ["To Do", "2"],
  ["In Progress", "1"],
  ["Done", "1"],
  ["No status", "1"],
];

/**
 * The board is **told** which layout to draw (#210).
 *
 * Both cases render the same four columns, so nothing here can be satisfied by
 * a board that inferred its own layout from what it happened to hold: the
 * fixture is identical and only the room's answer differs.
 */
test("draws the layout the room told it to, over one and the same board", async () => {
  answer = () => Promise.resolve({ columns: workflow(), sources: [] });

  const stacked = render([], null, null, "stacked");
  await vi.waitFor(() => expect(stacked.columns()).toHaveLength(4));
  flushSync();
  expect(stacked.layout()).toBe("stacked");
  expect(stacked.columns()).toEqual(WORKFLOW_COLUMNS);
  stacked.done();

  const columns = render([], null, null, "columns");
  await vi.waitFor(() => expect(columns.columns()).toHaveLength(4));
  flushSync();
  expect(columns.layout()).toBe("columns");
  expect(columns.columns()).toEqual(WORKFLOW_COLUMNS);
  columns.done();
});

/**
 * ...down to the cards and the addresses they open.
 *
 * The layout is how the groups are arranged, never what they are, so a card
 * carries the same key, priority and title in either and clicks through to the
 * same entity.
 */
test("a card reads the same and opens the same address in either layout", async () => {
  answer = () =>
    Promise.resolve({ columns: [column("To Do", card("PAY-240", "Medium"))], sources: [] });

  for (const layout of ["columns", "stacked"] as const) {
    const screen = render([], null, null, layout);
    await vi.waitFor(() => expect(screen.cards()).toHaveLength(1));
    flushSync();

    const [only] = screen.cards();
    expect(only?.querySelector(".k .mono")?.textContent).toBe("PAY-240");
    expect(only?.querySelector(".k .pr")?.textContent).toBe("Medium");
    expect(only?.querySelector(".s")?.textContent).toBe("Title of PAY-240");

    only?.click();
    flushSync();
    expect(screen.onopen).toHaveBeenCalledWith({ kind: "ticket", entity_id: "mock:PAY-240" });

    screen.done();
  }
});

/** An empty room reads as empty in either layout, rather than as a bare frame. */
test("an empty board still says there is no ticket here in either layout", async () => {
  for (const layout of ["columns", "stacked"] as const) {
    const screen = render([], null, null, layout);
    await vi.waitFor(() => expect(screen.text()).toContain("No ticket in this room yet."));
    flushSync();
    expect(screen.layout()).toBeNull();
    screen.done();
  }
});
