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

/**
 * Plain functions rather than `vi.fn`, for the reason `Tile.test.svelte.ts`
 * spells out: a `vi.fn` keeps a derived rejected promise with no handler on
 * it, and the run then fails with an "Unknown Error" that looks exactly like
 * the product bug the failed-read test exists to catch.
 */
const calls: Pick<EntityFilter, "sources" | "context">[] = [];
let answer: () => Promise<MiniBoard> = () => Promise.resolve({ columns: [], sources: [] });

vi.mock("../ipc/entity", () => ({
  miniBoard: (filter: Pick<EntityFilter, "sources" | "context">) => {
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

function render(sources: string[] = [], ctx: string | null = null) {
  const target = document.createElement("div");
  document.body.append(target);
  const onopen = vi.fn();
  const props = $state({ spec: SPEC, sources, ctx, onopen });
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

test("the tile reads with the room's own filter, both dimensions of it", async () => {
  const screen = render(["jira"], null);
  await vi.waitFor(() => expect(calls).toHaveLength(1));

  expect(calls[0]).toEqual({ sources: ["jira"], context: null });

  screen.props.ctx = "ctx:5b1c";
  screen.props.sources = [];
  flushSync();
  await vi.waitFor(() => expect(calls).toHaveLength(2));
  expect(calls[1]).toEqual({ sources: [], context: "ctx:5b1c" });

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
