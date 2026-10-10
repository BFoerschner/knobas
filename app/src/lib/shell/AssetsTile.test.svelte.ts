/**
 * The Assets tile: what a reader sees, and the three states of its read.
 *
 * The seam is `Tile.test.svelte.ts`': a rendered tile in, user-visible text
 * and the read it issued out. Who is a *member* is the backend's one statement
 * and is asserted over a real database in
 * `crates/knobas-app/tests/it/assets_ipc.rs`; what is asserted here is that the
 * tile draws the answer whole — the path included — and that a failed read
 * does not look like an empty room.
 *
 * The bridge arrives through `ports`, the way `AssetsView` takes it, so this
 * file mocks nothing and needs no Tauri.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import AssetsTile from "./AssetsTile.svelte";
import type { AssetRow, AssetStatus, MemberAsset } from "../ipc/assets";
import type { RoomFilter } from "./assets-tile";

const STORED: RoomFilter = { sources: [], context: "ctx:pay", project: null };

/** One member row, at the health and path the case is about. */
function member(
  name: string,
  path: string | null,
  health: AssetStatus,
  monogram = "CT",
): MemberAsset {
  return {
    asset: {
      id: `asset:${name}`,
      parent_id: null,
      type_id: "container",
      type_label: "Container",
      monogram,
      name,
      status: "none",
      environment: null,
      owner: null,
      has_children: false,
      health,
      inside: "none",
      problems_inside: 0,
      linked_work: 0,
    },
    path,
  };
}

/**
 * The estate corner every case below draws, in the order the backend answers
 * with it: worst first, and the well one last though it is alphabetically
 * first.
 */
const ROWS: MemberAsset[] = [
  member("redis", "hel1 / vm-db-01", "down"),
  member("vm-db-01", "hel1", "down", "VM"),
  member("postgres", "hel1 / vm-db-01", "up"),
];

/** A promise plus the handles that decide when it answers. */
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
  contextAssets: (ctxId: string) => Promise<MemberAsset[]>,
  filter: RoomFilter = STORED,
  over: {
    assetTree?: (parentId?: string | null) => Promise<AssetRow[]>;
    sourceAssets?: (sourceId: string) => Promise<MemberAsset[]>;
  } = {},
) {
  const target = document.createElement("div");
  document.body.append(target);
  const onopen = vi.fn();
  const props = $state({
    filter,
    ports: {
      contextAssets,
      // Every read the rule can ask for is refusable by default, so a test
      // that names one is the only one that can issue it: a tile reaching for
      // the wrong read fails loudly rather than drawing an empty box.
      assetTree: over.assetTree ?? (() => Promise.reject(new Error("no tree read here"))),
      sourceAssets: over.sourceAssets ?? (() => Promise.reject(new Error("no source read here"))),
    },
    maximised: false,
    onopen,
    onmaximise: vi.fn(),
  });
  const app = mount(AssetsTile, { target, props });
  flushSync();
  return {
    target,
    props,
    onopen,
    rows: () => [...target.querySelectorAll<HTMLButtonElement>(".arow")],
    /** Each row as the reader reads it: name, path, health. */
    read: () =>
      [...target.querySelectorAll(".arow")].map((row) => [
        row.querySelector(".nm")?.textContent ?? "",
        row.querySelector(".pth")?.textContent ?? "",
        row.querySelector(".hl")?.textContent ?? "",
      ]),
    /**
     * Each row's problems-inside badge: its number, its tone and its title —
     * `null` where there is no badge at all.
     *
     * The tone is read off `classList` rather than off `className`, which
     * carries Svelte's own scoping class as well.
     */
    badges: () =>
      [...target.querySelectorAll(".arow")].map((row) => {
        const badge = row.querySelector(".badge");
        return badge === null
          ? null
          : [
              badge.textContent ?? "",
              badge.classList.contains("down") ? "down" : "warn",
              badge.getAttribute("title"),
            ];
      }),
    count: () => target.querySelector(".tile-h .cnt")?.textContent ?? "",
    text: () => target.textContent ?? "",
    done: () => {
      unmount(app);
      target.remove();
    },
  };
}

test("draws each member asset with the path it sits at, in the order it was given", async () => {
  const asked: string[] = [];
  const tile = render((ctxId) => {
    asked.push(ctxId);
    return Promise.resolve(ROWS);
  });
  await Promise.resolve();
  flushSync();

  expect(asked, "the room's context, and nothing else, scopes the read").toEqual(["ctx:pay"]);
  expect(tile.read()).toEqual([
    ["redis", "hel1 / vm-db-01", "down"],
    ["vm-db-01", "hel1", "down"],
    ["postgres", "hel1 / vm-db-01", "up"],
  ]);
  tile.done();
});

/**
 * An asset at the top of the estate has a place, and the tile says what it is.
 *
 * A blank cell would read as a value that failed to load, which is the one
 * thing the path column must not be confusable with.
 */
test("an asset at the top of the estate reads as top level, not as a blank", async () => {
  const tile = render(() => Promise.resolve([member("hel1", null, "up", "SI")]));
  await Promise.resolve();
  flushSync();
  expect(tile.read()).toEqual([["hel1", "top level", "up"]]);
  tile.done();
});

/**
 * The header counts what the rows say, so the two cannot disagree.
 *
 * `down` wins over `warn` where both are present — the header has room for one
 * clause and the worse one is the one worth having.
 */
test("the header counts the members and names the worst of them", async () => {
  const tile = render(() => Promise.resolve(ROWS));
  await Promise.resolve();
  flushSync();
  expect(tile.count()).toBe("3 here · 2 down");

  const warned = render(() =>
    Promise.resolve([member("redis", "hel1", "warn"), member("postgres", "hel1", "up")]),
  );
  await Promise.resolve();
  flushSync();
  expect(warned.count()).toBe("2 here · 1 warning");

  const well = render(() => Promise.resolve([member("postgres", "hel1", "up")]));
  await Promise.resolve();
  flushSync();
  expect(well.count()).toBe("1 here");

  tile.done();
  warned.done();
  well.done();
});

test("a context with no assets in it says so and offers the two ways in", async () => {
  const tile = render(() => Promise.resolve([]));
  await Promise.resolve();
  flushSync();
  expect(tile.rows()).toEqual([]);
  expect(tile.text()).toContain("No machine, container or database belongs to this context yet.");
  expect(tile.count(), "a count of nothing is no count").toBe("");
  tile.done();
});

/**
 * A failed read must not look like a room with nothing in it — `Tile.svelte`'s
 * rule, and the reason this tile shows the message rather than swallowing it.
 */
test("a failed read says so rather than drawing an empty tile", async () => {
  const failing = deferred<MemberAsset[]>();
  const tile = render(() => failing.promise);
  expect(tile.text()).toContain("Reading…");

  failing.reject({ code: "internal", message: "the database went away", source_id: null });
  await failing.promise.catch(() => {});
  flushSync();

  expect(tile.text()).toContain("the database went away");
  expect(tile.text()).not.toContain("No machine, container or database");
  tile.done();
});

/**
 * The stale-answer guard: a room switch clears the rows *before* the new read
 * goes out, and the previous room's answer landing late may not paint over it.
 */
test("the previous room's assets leave the moment the room changes", async () => {
  const first = deferred<MemberAsset[]>();
  const second = deferred<MemberAsset[]>();
  let call = 0;
  const tile = render(() => (++call === 1 ? first.promise : second.promise));

  tile.props.filter = { sources: [], context: "ctx:other", project: null };
  flushSync();
  expect(tile.rows(), "cleared before the second read, not after it").toEqual([]);

  first.resolve(ROWS);
  await first.promise;
  flushSync();
  expect(tile.rows(), "the room you just left may not paint over this one").toEqual([]);

  second.resolve([member("nginx", "hel1 / vm-app-02", "up")]);
  await second.promise;
  flushSync();
  expect(tile.read()).toEqual([["nginx", "hel1 / vm-app-02", "up"]]);
  tile.done();
});

/** Opening a row hands the whole member up: the room turns it into an address. */
test("pressing a row opens that asset", async () => {
  const tile = render(() => Promise.resolve(ROWS));
  await Promise.resolve();
  flushSync();

  tile.rows()[1]!.click();
  flushSync();
  expect(tile.onopen).toHaveBeenCalledTimes(1);
  expect(tile.onopen.mock.calls[0]![0]).toEqual(ROWS[1]);
  tile.done();
});

/**
 * A room the rule gives no read is drawn empty rather than asked about: the
 * room does not mount this tile for such a room, and the tile agrees.
 *
 * The project room is the one that has no read (story 45), and it is the one
 * that would otherwise be asked for a *source* room's — its filter names a
 * source too.
 */
test("a room with no assets read never asks the backend", async () => {
  const asked: string[] = [];
  const tile = render(
    (ctxId) => {
      asked.push(ctxId);
      return Promise.resolve(ROWS);
    },
    { sources: ["kuma"], context: null, project: "PAY" },
  );
  await Promise.resolve();
  flushSync();
  expect(asked).toEqual([]);
  expect(tile.rows()).toEqual([]);
  tile.done();
});

// ---------------------------------------------------------------------------
// The derived rooms (#435)
// ---------------------------------------------------------------------------

const ALL_WORK: RoomFilter = { sources: [], context: null, project: null };
const SOURCE_ROOM: RoomFilter = { sources: ["kuma"], context: null, project: null };

/** The estate's top level, as `asset_tree(null)` answers with it. */
const ROOTS: AssetRow[] = [
  { ...member("hel1", null, "warn", "SI").asset, problems_inside: 2 },
  { ...member("notebook", null, "up", "VM").asset, problems_inside: 0 },
];

/**
 * Story 43: *All work* reads the estate's **top level**, through the same
 * command the Tree's first column reads.
 *
 * The path column says *top level* on every row, because a root sits nowhere —
 * and the count that matters on these rows is `problems_inside`, which is what
 * makes "the widest room gives the widest view" a view of anything.
 */
test("All work lists the estate's top level with its problem counts", async () => {
  const asked: (string | null | undefined)[] = [];
  const tile = render(() => Promise.reject(new Error("no context read here")), ALL_WORK, {
    assetTree: (parentId) => {
      asked.push(parentId);
      return Promise.resolve(ROOTS);
    },
  });
  await Promise.resolve();
  await Promise.resolve();
  flushSync();

  expect(asked, "the top level, not every asset").toEqual([null]);
  expect(tile.read()).toEqual([
    ["hel1", "top level", "warn"],
    ["notebook", "top level", "up"],
  ]);
  // *…with problem counts*: the criterion's second clause, and the reason the
  // widest room's tile is a view of anything. A root that holds nothing wrong
  // carries no badge, which is the negative in the same read.
  expect(tile.badges()).toEqual([["2", "warn", "2 problems inside"], null]);
  tile.done();
});

/**
 * The badge counts and colours what is **inside**, not the row itself — the
 * Tree's own rule (`assets/tree.ts`), which this tile calls rather than
 * restates.
 *
 * A `down` VM holding one `warn` container is the case that tells the two
 * apart: the lamp and the word are red, the badge is amber, and a badge
 * coloured from `health` would fail only here.
 */
test("a row worse than what it holds draws a red lamp and an amber badge", async () => {
  const worse = member("vm-db-01", "hel1", "down", "VM");
  const tile = render(() => Promise.reject(new Error("no context read here")), ALL_WORK, {
    assetTree: () =>
      Promise.resolve([{ ...worse.asset, status: "down", inside: "warn", problems_inside: 1 }]),
  });
  await Promise.resolve();
  await Promise.resolve();
  flushSync();

  expect(tile.read()).toEqual([["vm-db-01", "top level", "down"]]);
  expect(tile.badges()).toEqual([["1", "warn", "1 problem inside"]]);
  tile.done();
});

/** Story 44: a source room reads the assets that source's monitors watch. */
test("a source room lists the assets its own monitors watch", async () => {
  const asked: string[] = [];
  const tile = render(() => Promise.reject(new Error("no context read here")), SOURCE_ROOM, {
    sourceAssets: (sourceId) => {
      asked.push(sourceId);
      return Promise.resolve([member("postgres", "hel1 / vm-db-01", "down")]);
    },
  });
  await Promise.resolve();
  await Promise.resolve();
  flushSync();

  expect(asked, "its own source scopes the read").toEqual(["kuma"]);
  expect(tile.read()).toEqual([["postgres", "hel1 / vm-db-01", "down"]]);
  tile.done();
});

/**
 * An empty answer means something different in each room, so it is worded per
 * room.
 *
 * A source room is empty until its monitors exist (M4.1), and telling that
 * reader to "link one to a ticket in this room" — the stored room's invitation
 * — would be an instruction that does nothing.
 */
test("each kind of room says in its own words why it is empty", async () => {
  const source = render(() => Promise.reject(new Error("no")), SOURCE_ROOM, {
    sourceAssets: () => Promise.resolve([]),
  });
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
  expect(source.text()).toContain("No asset is monitored by kuma yet.");
  source.done();

  const all = render(() => Promise.reject(new Error("no")), ALL_WORK, {
    assetTree: () => Promise.resolve([]),
  });
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
  expect(all.text()).toContain("Nothing is in the estate yet.");
  all.done();

  const stored = render(() => Promise.resolve([]));
  await Promise.resolve();
  await Promise.resolve();
  flushSync();
  expect(stored.text()).toContain("belongs to this context yet");
  stored.done();
});
