/**
 * A tile owns its read, and the three states of that read are the behaviour.
 *
 * Rows on screen is the easy one. The two that matter are a *failed* read —
 * which must not look like an empty room — and a *superseded* one, where the
 * previous room's rows have to leave the screen the moment the room changes,
 * not when the new answer happens to arrive.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityFilter, EntityPage, EntityRow } from "../ipc/entity";

/**
 * The mock is a plain function, deliberately, and not a `vi.fn`.
 *
 * A `vi.fn` records the promise its implementation returned so `settledResults`
 * can report it, and that bookkeeping leaves a *derived* rejected promise with
 * no handler on it. The component handles its own rejection perfectly well, and
 * the run still fails with an "Unknown Error" attributed to whichever test was
 * in flight — a harness artefact that would look exactly like the product bug
 * these tests exist to catch. Calls are recorded here instead.
 */
const calls: { filter: EntityFilter; limit: number; offset: number }[] = [];
let answer: () => Promise<EntityPage> = () => Promise.resolve({ rows: [], total: 0 });

vi.mock("../ipc/entity", () => ({
  listEntities: (filter: EntityFilter, limit: number, offset: number) => {
    calls.push({ filter, limit, offset });
    return answer();
  },
}));

const { default: Tile } = await import("./Tile.svelte");

const SPEC = { id: "tickets", label: "Tickets", kinds: ["ticket"] };

function row(key: string): EntityRow {
  return {
    entity_id: `mock:${key}`,
    kind: "ticket",
    source_id: "mock",
    title: `Title of ${key}`,
    updated_at: "2026-08-22T11:48:00Z",
    synced_at: "2026-08-22T14:30:00Z",
  };
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

function render(sources: string[] = []) {
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({ spec: SPEC, sources, onopen: vi.fn() });
  const app = mount(Tile, { target, props });
  flushSync();
  return {
    target,
    props,
    rows: () => [...target.querySelectorAll(".row")],
    text: () => target.textContent ?? "",
    count: () => target.querySelector(".tile-h .cnt")?.textContent ?? "",
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

beforeEach(() => {
  calls.length = 0;
  answer = () => Promise.resolve({ rows: [], total: 0 });
});

test("draws a row per item and the unpaged total in the header", async () => {
  answer = () => Promise.resolve({ rows: [row("PAY-1"), row("PAY-2"), row("PAY-3")], total: 17 });
  const screen = render();
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(3));
  flushSync();

  expect(screen.text()).toContain("Title of PAY-2");
  expect(screen.count()).toBe("17");
  // The tile asked for its own kinds, and for the room's sources.
  expect(calls[0]?.filter).toMatchObject({ kinds: ["ticket"], sources: [] });

  screen.done();
});

/**
 * A failed read is a *message*, not an empty tile.
 *
 * The two look identical on screen otherwise, and they mean opposite things:
 * one is "there is nothing here", the other is "knobas does not know".
 */
test("a rejected read renders its message rather than an empty tile", async () => {
  answer = () =>
    Promise.reject({ code: "not_ready", message: "the database is still starting", source_id: null });
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("the database is still starting"));
  flushSync();

  expect(screen.rows()).toHaveLength(0);
  expect(screen.text()).not.toContain("No ticket in this room yet");

  screen.done();
});

/** An empty tile says which kind of nothing it is. */
test("an empty read gets the tile's own sentence", async () => {
  answer = () => Promise.resolve({ rows: [], total: 0 });
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("No ticket in this room yet"));
  screen.done();
});

/**
 * Changing rooms clears the old rows *immediately*.
 *
 * Asserted while the new read is still in flight, which is the only window in
 * which the bug exists: a tile that cleared on arrival instead would show the
 * previous room's tickets for as long as the query took, and they are not in
 * this room at all.
 */
test("a new room does not leave the old room's rows on screen", async () => {
  answer = () => Promise.resolve({ rows: [row("PAY-1")], total: 1 });
  const screen = render([]);
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(1));

  const pending = deferred<EntityPage>();
  answer = () => pending.promise;
  screen.props.sources = ["gitea"];
  flushSync();

  expect(screen.rows(), "the previous room's rows are still on screen").toHaveLength(0);
  expect(screen.count()).toBe("");

  pending.resolve({ rows: [row("GT-9")], total: 1 });
  await vi.waitFor(() => expect(screen.text()).toContain("Title of GT-9"));
  expect(calls[1]?.filter).toMatchObject({ sources: ["gitea"] });

  screen.done();
});

/**
 * A slow read must not overwrite a newer one.
 *
 * Both requests are answered, oldest **last**, which is exactly the ordering
 * the `token` guard exists for and the one a plain `.then(p => page = p)`
 * gets wrong.
 */
test("a slow answer from the previous room is discarded, not rendered", async () => {
  const first = deferred<EntityPage>();
  answer = () => first.promise;
  const screen = render([]);
  flushSync();

  const second = deferred<EntityPage>();
  answer = () => second.promise;
  screen.props.sources = ["gitea"];
  flushSync();

  second.resolve({ rows: [row("NEW-1")], total: 1 });
  await vi.waitFor(() => expect(screen.text()).toContain("Title of NEW-1"));

  first.resolve({ rows: [row("OLD-1")], total: 99 });
  await first.promise;
  flushSync();

  expect(screen.text()).not.toContain("Title of OLD-1");
  expect(screen.text()).toContain("Title of NEW-1");
  expect(screen.count()).toBe("1");

  screen.done();
});

/** ...including a slow *rejection*, which would blank a tile that has rows. */
test("a slow rejection from the previous room does not blank the new one", async () => {
  const first = deferred<EntityPage>();
  answer = () => first.promise;
  const screen = render([]);
  flushSync();

  answer = () => Promise.resolve({ rows: [row("NEW-1")], total: 1 });
  screen.props.sources = ["gitea"];
  flushSync();
  await vi.waitFor(() => expect(screen.text()).toContain("Title of NEW-1"));

  first.reject({ code: "internal", message: "pool closed", source_id: null });
  await first.promise.catch(() => undefined);
  flushSync();

  expect(screen.text()).not.toContain("pool closed");
  expect(screen.rows()).toHaveLength(1);

  screen.done();
});

/** Clicking a row hands the row back; the room decides where that goes. */
test("a row reports which entity was opened", async () => {
  answer = () => Promise.resolve({ rows: [row("PAY-1")], total: 1 });
  const screen = render();
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(1));

  (screen.rows()[0] as HTMLButtonElement).click();
  expect(screen.props.onopen).toHaveBeenCalledWith(
    expect.objectContaining({ entity_id: "mock:PAY-1" }),
  );

  screen.done();
});
