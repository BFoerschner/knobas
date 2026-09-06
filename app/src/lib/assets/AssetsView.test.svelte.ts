/**
 * The Tree: what a reader sees at an asset's address, and what the view asked
 * for to draw it (issue #428, spec #427 stories 27, 33 and 35).
 *
 * The seam is **a rendered view in, user-visible text and the reads it issued
 * out** — `StandupView.test.svelte.ts` and `DayReview.test.svelte.ts`'s, and
 * the reason nothing here reaches into the component's state. The bridge is
 * injected through `ports`, so no Tauri and no database: what the backend puts
 * on a column and in the pane is `crates/knobas-app/tests/assets_ipc.rs`'s,
 * and what is asserted here is that a cold address draws one column per level
 * with the path marked in each, and that the pane draws the answer.
 *
 * **This is the automated half of criterion 4's first clause.** The other half
 * — a headless-Chrome check against `--demo` — cannot be met as the ticket
 * writes it (`--demo` opens a Tauri window that Chrome cannot attach to, and
 * the demo profile carries no assets until #440), and the PR says so for
 * Björn to rule on. What a browser could only photograph, this file asserts.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AssetDetail, AssetProperty, AssetRow } from "../ipc/assets";
import { createRouter } from "../shell/router.svelte";
import AssetsView from "./AssetsView.svelte";

/** The clock the history's *ago* readings are relative to. */
const NOW = new Date(2026, 8, 6, 12, 0, 0, 0);

/**
 * A corner of the real estate, which is the fixture spec #427 rules for
 * assets: nothing Tidewater-shaped is invented for them.
 *
 * Two VMs under the site so that a column has more than one row — a
 * one-row column cannot witness which row is marked.
 */
const SITE = row({ id: "asset:hel1", type_id: "site", monogram: "SI", name: "hel1" });
const VM = row({
  id: "asset:vm-db-01",
  parent_id: SITE.id,
  type_id: "vm",
  monogram: "VM",
  name: "vm-db-01",
});
const SIBLING = row({
  id: "asset:vm-app-02",
  parent_id: SITE.id,
  type_id: "vm",
  monogram: "VM",
  name: "vm-app-02",
  has_children: false,
});
const CONTAINER = row({
  id: "asset:postgres",
  parent_id: VM.id,
  type_id: "container",
  type_label: "Container",
  monogram: "CT",
  name: "postgres",
  has_children: false,
});
const ESTATE = [SITE, VM, SIBLING, CONTAINER];

function row(over: Partial<AssetRow> & Pick<AssetRow, "id" | "name">): AssetRow {
  return {
    parent_id: null,
    type_id: "custom",
    type_label: "VM",
    monogram: "??",
    status: "none",
    environment: null,
    owner: null,
    has_children: true,
    ...over,
  } as AssetRow;
}

function property(over: Partial<AssetProperty> & Pick<AssetProperty, "key">): AssetProperty {
  return { label: over.key, value: null, custom: false, ...over };
}

/** The pane's read for one asset, ancestors walked over the parent field. */
function detailOf(id: string): AssetDetail {
  const asset = ESTATE.find((candidate) => candidate.id === id);
  if (asset === undefined) throw { code: "not_found", message: `no asset ${id}` };
  const heldBy: AssetRow[] = [];
  for (let at = asset; at.parent_id !== null; ) {
    const parent = ESTATE.find((candidate) => candidate.id === at.parent_id);
    if (parent === undefined) break;
    heldBy.unshift(parent);
    at = parent;
  }
  return {
    asset,
    properties:
      asset.id === CONTAINER.id
        ? [
            property({
              key: "image",
              label: "Image",
              value: { kind: "text", value: "postgres:18" },
            }),
            // Declared by the type and filled in by nobody — the em dash the
            // pane draws instead of the word "null".
            property({ key: "ports", label: "Ports" }),
            property({
              key: "zzz-note",
              label: "zzz-note",
              value: { kind: "text", value: "typed on this one" },
              custom: true,
            }),
          ]
        : [],
    held_by: heldBy,
    holds: ESTATE.filter((candidate) => candidate.parent_id === asset.id),
    history:
      asset.id === CONTAINER.id
        ? [
            {
              id: 2,
              at: new Date(2026, 8, 6, 11, 0, 0, 0).toISOString(),
              actor: "user",
              verb: "edited",
              entity_id: asset.id,
              detail: { field: "property", key: "os", from: "Debian 12", to: "Debian 13" },
            },
            {
              id: 1,
              at: new Date(2026, 8, 5, 9, 0, 0, 0).toISOString(),
              actor: "user",
              verb: "created",
              entity_id: asset.id,
              detail: {},
            },
          ]
        : [],
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

/**
 * Mount the view at `hash` and record every read it issues.
 *
 * `asked` is the point of the harness as much as the markup is: story 35 says
 * an address opened cold draws what a click draws, and the only way to see
 * that is which parents the view asked for having been given nothing but the
 * address and the pane's answer.
 */
function render(hash: string, estate: AssetRow[] = ESTATE) {
  const asked: (string | null)[] = [];
  location.hash = hash;
  const router = createRouter();
  app = mount(AssetsView, {
    target,
    props: {
      router,
      now: () => NOW,
      ports: {
        assetTree: (parentId?: string | null) => {
          const parent = parentId ?? null;
          asked.push(parent);
          // Sorted the way `assets::CHILDREN` sorts -- `order by a.name asc,
          // a.id asc`. A column arrives ordered and the view does not reorder
          // it, which is only assertable if the fake orders too.
          return Promise.resolve(
            estate
              .filter((candidate) => candidate.parent_id === parent)
              .sort(
                (left, right) =>
                  left.name.localeCompare(right.name) || left.id.localeCompare(right.id),
              ),
          );
        },
        getAsset: (assetId: string) => {
          try {
            return Promise.resolve(detailOf(assetId));
          } catch (cause) {
            return Promise.reject(cause);
          }
        },
      },
    },
  });
  flushSync();
  return { router, asked };
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** Each column's row labels, left to right. */
function columns(): string[][] {
  return [...target.querySelectorAll("ol.col")].map((column) =>
    [...column.querySelectorAll("button.row span.nm")].map((name) =>
      (name.textContent ?? "").trim(),
    ),
  );
}

/** The row marked as the selection in each column, left to right. */
function marked(): (string | null)[] {
  return [...target.querySelectorAll("ol.col")].map((column) => {
    const on = column.querySelector('button.row[aria-current="true"] span.nm');
    return on === null ? null : (on.textContent ?? "").trim();
  });
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  location.hash = "";
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
});

/**
 * **`#/assets/tree` is the estate's top level and one column.**
 *
 * The read it issues is `null`, which is a *level* and not a missing filter:
 * a view that asked with no parent at all would draw a first column holding
 * the whole estate. The VM and the container exist and are not in it.
 */
test("the bare address draws the roots and nothing is selected", async () => {
  const { asked } = render("#/assets/tree");

  await vi.waitFor(() => expect(columns()).toEqual([["hel1"]]));
  expect(asked).toEqual([null]);
  expect(marked()).toEqual([null]);
  expect(text()).toContain("Select an asset to see what it holds.");
});

/**
 * **An asset's address, opened cold, draws one column per level with the path
 * marked in each — and the pane beside it.**
 *
 * Story 35, and the criterion's own sentence: *a three-level path and the
 * pane's properties*. Nothing is clicked here; the columns are derived from
 * `held_by`, which arrives with the asset, so what is asserted is that the
 * view walked nothing and cached nothing.
 *
 * No trailing column: the container holds nothing, and an empty column would
 * promise a level that is not there.
 */
test("an asset's address draws its whole path and its pane", async () => {
  const { asked } = render("#/asset/asset:postgres");

  await vi.waitFor(() =>
    expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres"]]),
  );
  // Four reads, and the shape of the list is the point. The first `null` is
  // the cold frame: the layout is `emptyPath()` until the pane's read lands,
  // so the top column is asked for before there is a path to derive. Then the
  // path arrives and all three levels are asked for together -- one assignment,
  // because a half-drawn Miller layout is a path with a hole in it. Nothing
  // above the site is asked for twice, and nothing is walked.
  expect(asked).toEqual([null, null, SITE.id, VM.id]);
  expect(marked()).toEqual(["hel1", "vm-db-01", "postgres"]);

  const pane = text();
  // The held-by line ends at the asset: it is *where this thing is*.
  expect(pane).toContain("hel1 / vm-db-01 / postgres");
  expect(pane).toContain("Image postgres:18");
  // Declared and unfilled: an em dash, never the word "null".
  expect(pane).toContain("Ports —");
  expect(pane).toContain("zzz-note typed on this one");
  expect(pane).toContain("Holds Nothing.");
  // A property edit's line reads as the change it was, old value included.
  expect(pane).toContain("os: Debian 12 → Debian 13");
  expect(pane).toContain("Created");
});

/**
 * **A selection that holds something opens the column under it, and the
 * columns follow the address rather than the click.**
 *
 * The click sets the address; the address is read back; the columns follow the
 * answer. That is the whole reason a cold address and a click land in the same
 * place, so the assertion is on the address as well as on the markup.
 */
test("selecting an asset that holds something opens the next column", async () => {
  const { router } = render("#/asset/asset:postgres");
  await vi.waitFor(() => expect(columns()).toHaveLength(3));

  const vm = [...target.querySelectorAll<HTMLButtonElement>("button.row")].find(
    (button) => button.textContent?.includes("vm-db-01"),
  );
  vm?.click();
  flushSync();

  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: VM.id });
  await vi.waitFor(() =>
    expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres"]]),
  );
  expect(marked()).toEqual(["hel1", "vm-db-01", null]);
  expect(text()).toContain("hel1 / vm-db-01");
});

/**
 * **An estate with nothing in it says so in its own words.**
 *
 * The branch a fresh install is in until #439's import or a hand-created
 * asset, and the one a reader would otherwise take for a failed read.
 */
test("an empty estate is told apart from a read that has not landed", async () => {
  render("#/assets/tree", []);

  await vi.waitFor(() => expect(text()).toContain("Nothing in the estate yet."));
  expect(columns()).toEqual([]);
});

/**
 * **An address naming an asset that is not there draws the failure, and drops
 * the selection.**
 *
 * A pane still showing the last asset would be the view lying about what the
 * address names — and this is the deep-link case a notification produces.
 */
test("a deep link to an asset that is gone says so instead of showing the last one", async () => {
  const { router } = render("#/asset/asset:postgres");
  await vi.waitFor(() => expect(text()).toContain("hel1 / vm-db-01 / postgres"));

  router.go("#/asset/asset:nobody");
  flushSync();

  await vi.waitFor(() => expect(text()).toContain("no asset asset:nobody"));
  expect(text()).not.toContain("hel1 / vm-db-01 / postgres");
  expect(columns()).toEqual([]);
});
