/**
 * The linked-work count a column row of the asset tree carries (issue #435,
 * spec #427 story 32): the count the backend gave, toneless, and no badge at
 * all where there is nothing to count. Linking itself is the asset page's
 * (`tf/asset/TfAsset.links.test.svelte.ts`).
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { AssetDetail, AssetRow } from "../ipc/assets";
import { createTestNav } from "./test-nav.svelte";
import AssetsView from "./AssetsView.svelte";

const NOW = new Date(2026, 8, 6, 12, 0, 0, 0);

function row(over: Partial<AssetRow> & Pick<AssetRow, "id" | "name">): AssetRow {
  return {
    parent_id: null,
    type_id: "custom",
    type_label: "Custom",
    monogram: "??",
    status: "none",
    environment: null,
    owner: null,
    has_children: true,
    health: "none",
    inside: "none",
    problems_inside: 0,
    linked_work: 0,
    ...over,
  } as AssetRow;
}

const SITE = row({ id: "asset:hel1", type_id: "site", monogram: "SI", name: "hel1" });
const VM = row({
  id: "asset:vm-db-01",
  parent_id: SITE.id,
  type_id: "vm",
  type_label: "VM",
  monogram: "VM",
  name: "vm-db-01",
  // One ticket links to the VM, which is what the badge on its row counts.
  linked_work: 1,
});
const CONTAINER = row({
  id: "asset:postgres",
  parent_id: VM.id,
  type_id: "container",
  type_label: "Container",
  monogram: "CT",
  name: "postgres",
  has_children: false,
  linked_work: 2,
});
const ESTATE = [SITE, VM, CONTAINER];

function detailOf(assetId: string): AssetDetail {
  const asset = ESTATE.find((candidate) => candidate.id === assetId);
  if (asset === undefined) throw { code: "not_found", message: `no asset ${assetId}` };
  const heldBy: AssetRow[] = [];
  for (let at = asset; at.parent_id !== null; ) {
    const parent = ESTATE.find((candidate) => candidate.id === at.parent_id);
    if (parent === undefined) break;
    heldBy.unshift(parent);
    at = parent;
  }
  return {
    asset,
    properties: [],
    effective_environment: null,
    effective_owner: null,
    held_by: heldBy,
    holds: ESTATE.filter((candidate) => candidate.parent_id === asset.id),
    exposes: [],
    reachable_via: [],
    history: [],
    links: [],
    monitors: [],
    monitoring: [],
    monitor_targets: [],
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;

function render(hash: string) {
  location.hash = hash;
  const nav = createTestNav();
  app = mount(AssetsView, {
    target,
    props: {
      nav,
      now: () => NOW,
      ports: {
        assetTypes: () => Promise.resolve([]),
        assetTree: (parentId?: string | null) =>
          Promise.resolve(ESTATE.filter((candidate) => candidate.parent_id === (parentId ?? null))),
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
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
});

/** Wait for the reads the mount issued, then redraw. */
async function settle() {
  for (let at = 0; at < 6; at += 1) await Promise.resolve();
  flushSync();
}

/**
 * Story 32's first badge, on the column rows: the count the backend gave, and
 * **no badge at all** where there is nothing to count.
 *
 * The site is the negative and it is in the same column as nothing else, so
 * the two assertions are about the same read: `hel1` links to no work and
 * draws no badge, while `vm-db-01` draws one and `postgres` draws two.
 */
test("a column row carries its linked-work count and nothing when it is zero", async () => {
  render("#/assets/asset:postgres");
  await settle();

  const badges = [...target.querySelectorAll(".tf-et-cols .tf-et-row")].map((rowEl) => [
    rowEl.querySelector(".tf-et-nm")?.textContent ?? "",
    rowEl.querySelector(".tf-et-badge.tf-et-work")?.textContent ?? "",
  ]);
  expect(badges).toEqual([
    ["hel1", ""],
    ["vm-db-01", "1"],
    ["postgres", "2"],
  ]);
});

/** The badge says what it counts, so a number in a circle is readable. */
test("the linked-work badge names what it counts", async () => {
  render("#/assets/asset:postgres");
  await settle();

  const titles = [...target.querySelectorAll(".tf-et-cols .tf-et-row .tf-et-badge.tf-et-work")].map((node) =>
    node.getAttribute("title"),
  );
  expect(titles).toEqual(["1 linked work item", "2 linked work items"]);
});
