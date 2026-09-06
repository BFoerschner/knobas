/**
 * The Assets tile: what a reader sees, and the three states of its read.
 *
 * The seam is `Tile.test.svelte.ts`': a rendered tile in, user-visible text
 * and the read it issued out. Who is a *member* is the backend's one statement
 * and is asserted over a real database in
 * `crates/knobas-app/tests/assets_ipc.rs`; what is asserted here is that the
 * tile draws the answer whole — the path included — and that a failed read
 * does not look like an empty room.
 *
 * The bridge arrives through `ports`, the way `AssetsView` takes it, so this
 * file mocks nothing and needs no Tauri.
 */
import { flushSync, mount, unmount } from "svelte";
import { expect, test, vi } from "vitest";

import AssetsTile from "./AssetsTile.svelte";
import type { AssetStatus, MemberAsset } from "../ipc/assets";
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
) {
  const target = document.createElement("div");
  document.body.append(target);
  const onopen = vi.fn();
  const props = $state({
    filter,
    ports: { contextAssets },
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
 */
test("a room with no assets read never asks the backend", async () => {
  const asked: string[] = [];
  const tile = render(
    (ctxId) => {
      asked.push(ctxId);
      return Promise.resolve(ROWS);
    },
    { sources: ["kuma"], context: null, project: null },
  );
  await Promise.resolve();
  flushSync();
  expect(asked).toEqual([]);
  expect(tile.rows()).toEqual([]);
  tile.done();
});
