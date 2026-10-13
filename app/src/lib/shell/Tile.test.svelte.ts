/**
 * A tile owns its read, and the three states of that read are the behaviour.
 *
 * Over the **Docs** tile since #178, and that is the point of the choice: the
 * Tickets tile is the mini board now, so a list tile is what stands for the
 * four that are still lists. `MiniBoard.test.svelte.ts` puts the same three
 * states, and the same stale-answer guard, over the board.
 *
 * Rows on screen is the easy one. The two that matter are a *failed* read —
 * which must not look like an empty room — and a *superseded* one, where the
 * previous room's rows have to leave the screen the moment the room changes,
 * not when the new answer happens to arrive.
 */
import { flushSync, mount, unmount } from "svelte";
import { beforeEach, expect, test, vi } from "vitest";

import type { EntityFilter, EntityPage, EntityRow } from "../ipc/entity";
import type { TileSpec } from "./kinds";

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
let answer: () => Promise<EntityPage> = () =>
  Promise.resolve({ rows: [], total: 0, tally: null, groups: null });

vi.mock("../ipc/entity", () => ({
  listEntities: (filter: EntityFilter, limit: number, offset: number) => {
    calls.push({ filter, limit, offset });
    return answer();
  },
}));

const { default: Tile } = await import("./Tile.svelte");

const SPEC: TileSpec = { id: "docs", label: "Docs", kinds: ["page"], order: "updated_desc" };

function row(key: string): EntityRow {
  return {
    entity_id: `mock:${key}`,
    kind: "page",
    source_id: "mock",
    title: `Title of ${key}`,
    updated_at: "2026-08-22T11:48:00Z",
    synced_at: "2026-08-22T14:30:00Z",
    path: null,
    state: null,
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

function render(sources: string[] = [], spec: TileSpec = SPEC) {
  const target = document.createElement("div");
  document.body.append(target);
  const props = $state({
    spec,
    sources,
    miniBoardLayout: "columns" as const,
    maximised: false,
    onopen: vi.fn(),
    onlayout: vi.fn(),
    onmaximise: vi.fn(),
  });
  const app = mount(Tile, { target, props });
  flushSync();
  return {
    target,
    props,
    /** The header's maximise control (#250). */
    maxButton: () => target.querySelector<HTMLButtonElement>(".tile-h .acts .tile-max"),
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
  answer = () => Promise.resolve({ rows: [], total: 0, tally: null, groups: null });
});

test("draws a row per item and the unpaged total in the header", async () => {
  answer = () =>
    Promise.resolve({ rows: [row("PAY-1"), row("PAY-2"), row("PAY-3")], total: 17, tally: null, groups: null });
  const screen = render();
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(3));
  flushSync();

  expect(screen.text()).toContain("Title of PAY-2");
  expect(screen.count()).toBe("17");
  // The tile asked for its own kinds, and for the room's sources.
  expect(calls[0]?.filter).toMatchObject({ kinds: ["page"], sources: [] });

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
  expect(screen.text()).not.toContain("No page in this room yet");

  screen.done();
});

/** An empty tile says which kind of nothing it is. */
test("an empty read gets the tile's own sentence", async () => {
  answer = () => Promise.resolve({ rows: [], total: 0, tally: null, groups: null });
  const screen = render();
  await vi.waitFor(() => expect(screen.text()).toContain("No page in this room yet"));
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
  answer = () => Promise.resolve({ rows: [row("PAY-1")], total: 1, tally: null, groups: null });
  const screen = render([]);
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(1));

  const pending = deferred<EntityPage>();
  answer = () => pending.promise;
  screen.props.sources = ["gitea"];
  flushSync();

  expect(screen.rows(), "the previous room's rows are still on screen").toHaveLength(0);
  expect(screen.count()).toBe("");

  pending.resolve({ rows: [row("GT-9")], total: 1, tally: null, groups: null });
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

  second.resolve({ rows: [row("NEW-1")], total: 1, tally: null, groups: null });
  await vi.waitFor(() => expect(screen.text()).toContain("Title of NEW-1"));

  first.resolve({ rows: [row("OLD-1")], total: 99, tally: null, groups: null });
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

  answer = () => Promise.resolve({ rows: [row("NEW-1")], total: 1, tally: null, groups: null });
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

/**
 * The key is everything after the *first* colon.
 *
 * `knobas_core::entity` splits on the first `:` only, so a Confluence page id
 * (`confluence:ENG:SEPA design`) and a Gitea PR (`gitea:acme/svc#142`) both
 * carry more punctuation in the key. A `split(":")[1]` truncates the first and
 * looks perfectly correct on every ticket key in the fixture.
 */
test("a key containing a colon is shown whole", async () => {
  answer = () =>
    Promise.resolve({
      rows: [
        {
          entity_id: "confluence:ENG:SEPA design",
          kind: "page",
          source_id: "confluence",
          title: "SEPA design",
          updated_at: "2026-08-22T11:48:00Z",
          synced_at: "2026-08-22T14:30:00Z",
          path: null,
          state: null,
        },
      ],
      total: 1,
      tally: null,
      groups: null,
    });

  const screen = render();
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(1));
  flushSync();

  expect(screen.target.querySelector(".row .k")?.textContent).toBe("ENG:SEPA design");

  screen.done();
});

/** Clicking a row hands the row back; the room decides where that goes. */
test("a row reports which entity was opened", async () => {
  answer = () => Promise.resolve({ rows: [row("PAY-1")], total: 1, tally: null, groups: null });
  const screen = render();
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(1));

  (screen.rows()[0] as HTMLButtonElement).click();
  expect(screen.props.onopen).toHaveBeenCalledWith(
    expect.objectContaining({ entity_id: "mock:PAY-1" }),
  );

  screen.done();
});

/**
 * The layout control is the mini board's, and only the Tickets tile draws a
 * mini board (#245): a list tile's header offers no layout to choose.
 */
test("a list tile's header carries no layout control", async () => {
  answer = () => Promise.resolve({ rows: [row("PAY-1")], total: 1, tally: null, groups: null });
  const screen = render();
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(1));
  flushSync();

  expect(screen.target.querySelector(".tile-h .acts .seg")).toBeNull();
  // ...and the slot holds the maximise control alone (#250): one button, not
  // a layout word that lost its group.
  expect(screen.target.querySelectorAll(".tile-h .acts button")).toHaveLength(1);
  expect(screen.maxButton()).not.toBeNull();

  screen.done();
});

/**
 * The maximise control (#250) is on every tile, and it only asks: the room
 * decides, and tells the tile through `maximised`, so a tile with the prop
 * flipped reads *Restore* whether or not it was the one pressed. The name is
 * the visible word either way -- what a screen reader announces and what the
 * label reads are one string, so neither can drift from the other.
 */
test("the header offers Maximise, asks the room, and reads Restore once maximised", async () => {
  answer = () => Promise.resolve({ rows: [row("PAY-1")], total: 1, tally: null, groups: null });
  const screen = render();
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(1));
  flushSync();

  const button = screen.maxButton();
  expect(button, "the tile header carries a maximise control").not.toBeNull();
  expect(button!.textContent?.trim()).toBe("Maximise");
  expect(button!.getAttribute("aria-label")).toBeNull();

  button!.click();
  expect(screen.props.onmaximise).toHaveBeenCalledTimes(1);
  // Asking is not deciding: the tile did not flip itself.
  expect(screen.maxButton()?.textContent?.trim()).toBe("Maximise");

  screen.props.maximised = true;
  flushSync();
  expect(screen.maxButton()?.textContent?.trim()).toBe("Restore");
  // ...and the rows under it are still the tile's own.
  expect(screen.rows()).toHaveLength(1);

  screen.done();
});

/**
 * A room scoped to one source — a source room, a project room — drops the
 * source column (#666): every row would repeat the same id. *All work* reads
 * every source, so there the column is what tells two rows apart.
 */
test("a one-source room drops the source column and All work keeps it", async () => {
  answer = () => Promise.resolve({ rows: [row("PAY-1")], total: 1, tally: null, groups: null });
  const scoped = render(["mock"]);
  await vi.waitFor(() => expect(scoped.rows()).toHaveLength(1));
  flushSync();
  expect(scoped.target.querySelector(".row .src")).toBeNull();
  expect(scoped.rows()[0]?.classList.contains("g4w")).toBe(true);
  scoped.done();

  const everything = render([]);
  await vi.waitFor(() => expect(everything.rows()).toHaveLength(1));
  flushSync();
  expect(everything.target.querySelector(".row .src")?.textContent).toBe("mock");
  expect(everything.rows()[0]?.classList.contains("g5w")).toBe(true);
  everything.done();
});

/** The Code tile as `kinds.ts` builds it (#680). */
const CODE: TileSpec = { id: "code", label: "Code", kinds: ["pr", "branch"], order: "attention" };

function codeRow(key: string, kind: "pr" | "branch"): EntityRow {
  return {
    ...row(key),
    kind,
    state: kind === "pr" ? { text: "open", tone: "info" } : null,
  };
}

/**
 * The Code tile reads by attention and says what it counts (#680): open pull
 * requests and branches, in the words the kind vocabulary has for them, from
 * the tally the backend counted over the whole set rather than the page.
 */
test("the Code tile asks for attention and counts open pull requests and branches", async () => {
  answer = () =>
    Promise.resolve({
      rows: [codeRow("PR-1", "pr"), codeRow("PR-2", "pr"), codeRow("main", "branch")],
      total: 8,
      tally: [
        { kind: "branch", tone: null, count: 5 },
        { kind: "pr", tone: "info", count: 3 },
      ],
      groups: null,
    });
  const screen = render([], CODE);
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(3));
  flushSync();

  expect(calls[0]?.filter).toMatchObject({ kinds: ["pr", "branch"], order: "attention" });
  expect(screen.count()).toBe("3 open pull requests · 5 branches");

  screen.done();
});

/**
 * Maximised, the tile asks the same read in the same order for more rows,
 * draws them in the order they came, and keeps the rows it had until the
 * longer page lands (#680).
 */
test("a maximised tile keeps its order and shows more rows", async () => {
  const first = [codeRow("PR-1", "pr"), codeRow("main", "branch")];
  const more = [...first, codeRow("dev", "branch"), codeRow("old", "branch")];
  const tally = [
    { kind: "pr", tone: "info" as const, count: 1 },
    { kind: "branch", tone: null, count: 3 },
  ];
  const longer = deferred<EntityPage>();
  answer = () => Promise.resolve({ rows: first, total: 4, tally, groups: null });
  const screen = render([], CODE);
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(2));
  flushSync();

  answer = () => longer.promise;
  screen.props.maximised = true;
  flushSync();
  expect(calls).toHaveLength(2);
  expect(calls[1]?.filter).toEqual(calls[0]?.filter);
  expect(calls[1]!.limit).toBeGreaterThan(calls[0]!.limit);
  expect(screen.rows(), "the rows on screen stay while the longer page loads").toHaveLength(2);

  longer.resolve({ rows: more, total: 4, tally, groups: null });
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(4));
  flushSync();
  const titles = screen.rows().map((r) => r.textContent ?? "");
  ["PR-1", "main", "dev", "old"].forEach((key, i) =>
    expect(titles[i]).toContain(`Title of ${key}`),
  );
  expect(screen.count()).toBe("1 open pull request · 3 branches");

  screen.done();
});

/**
 * The Code tile answers "what needs me" first (#712): the backend's groups
 * draw as muted headers with their counts, in the order they came, and the
 * header counts them in so many words. A group with no row is not drawn.
 */
test("the Code tile draws its groups under headers and counts them", async () => {
  answer = () =>
    Promise.resolve({
      rows: [
        codeRow("PR-2", "pr"),
        codeRow("PR-3", "pr"),
        codeRow("PR-4", "pr"),
        codeRow("PR-1", "pr"),
        codeRow("spike", "branch"),
        codeRow("theirs", "branch"),
      ],
      total: 6,
      tally: [
        { kind: "pr", tone: "info", count: 4 },
        { kind: "branch", tone: null, count: 2 },
      ],
      groups: [
        { group: "review_requested", count: 2 },
        { group: "my_pull_requests", count: 1 },
        { group: "open_pull_requests", count: 1 },
        { group: "my_branches", count: 1 },
        { group: "other_branches", count: 1 },
      ],
    });
  const screen = render([], CODE);
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(5));
  flushSync();

  const heads = [...screen.target.querySelectorAll(".tile-g")].map((h) =>
    (h.textContent ?? "").replace(/\s+/g, " ").trim(),
  );
  expect(heads).toEqual([
    "Review requested 2",
    "My pull requests 1",
    "Other open pull requests 1",
    "My branches without a pull request 1",
  ]);
  // Each group's rows sit under its own header.
  const groups = [...screen.target.querySelectorAll(".tile-grp")];
  expect(groups.map((g) => g.querySelectorAll(".row").length)).toEqual([
    2, 1, 1, 1,
  ]);
  expect(groups[0]?.textContent).toContain("Title of PR-3");
  expect(groups[3]?.textContent).toContain("Title of spike");
  expect(
    screen.text(),
    "other branches wait for the maximised tile",
  ).not.toContain("Title of theirs");
  expect(screen.count()).toBe("2 to review · 1 mine · 1 open · 1 branch");

  screen.done();
});

/** An empty group is not drawn, and the count leaves it out too. */
test("the Code tile hides an empty group", async () => {
  answer = () =>
    Promise.resolve({
      rows: [codeRow("PR-1", "pr"), codeRow("spike", "branch")],
      total: 2,
      tally: [
        { kind: "pr", tone: "info", count: 1 },
        { kind: "branch", tone: null, count: 1 },
      ],
      groups: [
        { group: "open_pull_requests", count: 1 },
        { group: "my_branches", count: 1 },
      ],
    });
  const screen = render([], CODE);
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(2));
  flushSync();

  expect(screen.text()).not.toContain("Review requested");
  expect(screen.text()).not.toContain("My pull requests");
  expect(screen.count()).toBe("1 open · 1 branch");

  screen.done();
});

/** Maximised, the tile draws every other branch too, under its own header. */
test("a maximised Code tile shows the other branches", async () => {
  answer = () =>
    Promise.resolve({
      rows: [
        codeRow("PR-1", "pr"),
        codeRow("theirs", "branch"),
        codeRow("old", "branch"),
      ],
      total: 3,
      tally: [
        { kind: "pr", tone: "info", count: 1 },
        { kind: "branch", tone: null, count: 2 },
      ],
      groups: [
        { group: "my_pull_requests", count: 1 },
        { group: "other_branches", count: 2 },
      ],
    });
  const screen = render([], CODE);
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(1));
  flushSync();

  screen.props.maximised = true;
  flushSync();
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(3));
  flushSync();
  const heads = [...screen.target.querySelectorAll(".tile-g")].map(
    (h) => h.textContent ?? "",
  );
  expect(heads.at(-1)).toContain("Other branches");
  expect(screen.text()).toContain("Title of theirs");

  screen.done();
});

/**
 * A room whose rows only another person's branches fill is not an empty
 * room: the tile says the branches are there and how to see them.
 */
test("a Code tile with only other branches says so rather than looking empty", async () => {
  answer = () =>
    Promise.resolve({
      rows: [codeRow("theirs", "branch")],
      total: 1,
      tally: [{ kind: "branch", tone: null, count: 1 }],
      groups: [{ group: "other_branches", count: 1 }],
    });
  const screen = render([], CODE);
  await vi.waitFor(() => expect(screen.text()).toContain("1 other branch"));
  flushSync();
  expect(screen.rows()).toHaveLength(0);

  screen.done();
});

/**
 * The page is a window on the set: a group whose rows all fall past it has
 * no row on screen, and its header is not drawn over nothing.
 */
test("a group past the page's end draws no header", async () => {
  answer = () =>
    Promise.resolve({
      rows: [codeRow("PR-1", "pr"), codeRow("PR-2", "pr")],
      total: 5,
      tally: [
        { kind: "pr", tone: "info", count: 2 },
        { kind: "branch", tone: null, count: 3 },
      ],
      groups: [
        { group: "open_pull_requests", count: 2 },
        { group: "my_branches", count: 3 },
      ],
    });
  const screen = render([], CODE);
  await vi.waitFor(() => expect(screen.rows()).toHaveLength(2));
  flushSync();

  const heads = [...screen.target.querySelectorAll(".tile-g")].map((h) => h.textContent ?? "");
  expect(heads).toHaveLength(1);
  expect(heads[0]).toContain("Other open pull requests");
  expect(screen.count(), "the header still counts the whole set").toBe("2 open · 3 branches");

  screen.done();
});
