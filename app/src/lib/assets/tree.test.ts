/**
 * The Tree's column arithmetic (#428).
 *
 * The layout is the one thing on this surface that is neither a fetch nor a
 * paint: given what the backend said about one asset, which columns are open
 * and what is selected in each. It is asserted here rather than through the
 * rendered view because a Miller layout drawn wrong is *drawable* — three
 * columns that do not lead to the selection still look like three columns —
 * and a component test would be asserting the same arithmetic through a
 * screenshot's worth of DOM.
 */
import { expect, test } from "vitest";

import type { AssetDetail, AssetRow } from "../ipc/assets";
import {
  addressOf,
  columnPathFor,
  emptyPath,
  heldByPath,
  problemBadge,
  selectionIn,
  sourceOf,
} from "./tree";

function row(id: string, name: string, hasChildren = false): AssetRow {
  return {
    id,
    parent_id: null,
    type_id: "vm",
    type_label: "VM",
    monogram: "VM",
    name,
    status: "none",
    environment: null,
    owner: null,
    has_children: hasChildren,
    health: "none",
    inside: "none",
    problems_inside: 0,
  };
}

function detail(asset: AssetRow, ancestors: AssetRow[]): AssetDetail {
  return {
    asset,
    properties: [],
    effective_environment: null,
    effective_owner: null,
    held_by: ancestors,
    holds: [],
    history: [],
  };
}

/** Nothing selected is one column — the estate's top level. */
test("no selection is the top level and nothing else", () => {
  const path = columnPathFor(null);
  expect(path).toEqual({ parents: [null], selected: [null] });
  expect(path).toEqual(emptyPath());
});

/**
 * The three-level case the acceptance criterion names: site → VM → container.
 *
 * Three ancestors' worth of columns and no fourth, because the container holds
 * nothing. `parents[0]` is `null` — every layout starts at the top of the
 * estate, whatever is selected.
 */
test("a three-level selection opens one column per level", () => {
  const site = row("asset:site", "hel1", true);
  const vm = row("asset:vm", "vm-db-01", true);
  const container = row("asset:ct", "postgres");

  const path = columnPathFor(detail(container, [site, vm]));
  expect(path.parents).toEqual([null, "asset:site", "asset:vm"]);
  expect(path.selected).toEqual(["asset:site", "asset:vm", "asset:ct"]);
  expect(path.parents).toHaveLength(path.selected.length);
});

/**
 * A selection that holds something opens **one more** column, with nothing
 * selected in it — which is what makes a click on a folder show its contents
 * rather than only highlight it.
 */
test("a selection that holds something opens the column under it", () => {
  const site = row("asset:site", "hel1", true);
  const vm = row("asset:vm", "vm-db-01", true);

  const path = columnPathFor(detail(vm, [site]));
  expect(path.parents).toEqual([null, "asset:site", "asset:vm"]);
  expect(path.selected).toEqual(["asset:site", "asset:vm", null]);
});

/**
 * A leaf opens **no** trailing column, and that is the assertion the previous
 * test cannot make on its own: an implementation that always appended one
 * would pass that test and leave every leaf with an empty column beside it,
 * promising a level that is not there.
 */
test("a leaf opens no trailing column", () => {
  const site = row("asset:site", "hel1", true);
  const leaf = row("asset:ct", "postgres");

  const path = columnPathFor(detail(leaf, [site]));
  expect(path.parents).toEqual([null, "asset:site"]);
  expect(path.selected).toEqual(["asset:site", "asset:ct"]);
});

/** An asset at the top of the estate is one column, selected in it. */
test("a top-level selection is the first column and nothing before it", () => {
  const site = row("asset:site", "hel1", true);
  const path = columnPathFor(detail(site, []));
  expect(path.parents).toEqual([null, "asset:site"]);
  expect(path.selected).toEqual(["asset:site", null]);
});

/**
 * `selectionIn` answers `null` past the end of the path, which is the state a
 * column is in while its own read is still out — as against throwing, which
 * would take the view down in the one frame it is least able to afford it.
 */
test("a column past the end of the path has nothing selected", () => {
  const path = columnPathFor(detail(row("asset:site", "hel1"), []));
  expect(selectionIn(path, 0)).toBe("asset:site");
  expect(selectionIn(path, 1)).toBe(null);
  expect(selectionIn(path, 9)).toBe(null);
});

/**
 * The pane's held-by line **ends at the asset**; `held_by` does not.
 *
 * *Where this thing is* and *what holds it* are different sentences, and a
 * line that stopped at the parent would read as the path to somewhere else.
 */
test("the held-by line ends at the asset and held_by does not", () => {
  const site = row("asset:site", "hel1", true);
  const vm = row("asset:vm", "vm-db-01", true);
  const container = row("asset:ct", "postgres");

  const asset = detail(container, [site, vm]);
  expect(heldByPath(asset)).toEqual(["hel1", "vm-db-01", "postgres"]);
  expect(asset.held_by.map((held) => held.name)).toEqual(["hel1", "vm-db-01"]);
  expect(heldByPath(detail(site, []))).toEqual(["hel1"]);
});

/**
 * A row's address keeps the namespace colon and encodes the rest — the rule
 * every entity address in this app follows, and the reason `addressOf`
 * delegates to `hashFor` instead of building the string itself.
 */
test("a row's address is the asset's own, encoded once", () => {
  expect(addressOf({ id: "asset:7f2c" })).toBe("#/asset/asset:7f2c");
  expect(addressOf({ id: "asset:knobas/jira#1" })).toBe("#/asset/asset:knobas%2Fjira%231");
});

/**
 * The badge counts what is inside and takes its colour from the *worst* thing
 * inside — story 32, issue #431.
 *
 * The four cases are the ones the tone can get wrong: nothing wrong (no
 * badge), a warn (amber), a down (red), and the case that makes `inside` a
 * separate field — an asset **worse than what it holds**, which is a red row
 * with an amber badge. A badge coloured from `health` would pass the first
 * three and fail the fourth.
 */
test("the badge counts what is inside and is coloured by the worst of it", () => {
  const clean = row("asset:vm", "vm-app-02", true);
  expect(problemBadge(clean)).toBe(null);

  const warning = { ...clean, health: "warn", inside: "warn", problems_inside: 1 } as AssetRow;
  expect(problemBadge(warning)).toEqual({ count: 1, tone: "warn" });

  const broken = { ...clean, health: "down", inside: "down", problems_inside: 3 } as AssetRow;
  expect(problemBadge(broken)).toEqual({ count: 3, tone: "down" });

  // Down itself, holding one warning container: the row is red and the badge
  // is amber, because the badge is about what is inside it.
  const worseThanInside = {
    ...clean,
    status: "down",
    health: "down",
    inside: "warn",
    problems_inside: 1,
  } as AssetRow;
  expect(problemBadge(worseThanInside)).toEqual({ count: 1, tone: "warn" });
});

/**
 * A row whose own status is bad but which holds nothing wrong carries **no**
 * badge.
 *
 * The negative for the test above: it is the count that decides whether there
 * is a badge at all, and an implementation keyed on `health` would put a red
 * "0" on every broken leaf in the estate.
 */
test("a bad row that holds nothing wrong carries no badge", () => {
  const leaf = row("asset:ct", "postgres");
  expect(
    problemBadge({ ...leaf, status: "down", health: "down" } as AssetRow),
  ).toBe(null);
});

/**
 * Where a value in force came from, as the pane says it — story 10.
 *
 * Both directions, because they differ in all three fields: set here is a
 * note with no name and **no link**, and inherited names the ancestor and
 * links to it. A link back to the asset the reader is already looking at is
 * the failure this pins.
 */
test("a value set here has no link and an inherited one names its ancestor", () => {
  const site = row("asset:hel1", "hel1", true);
  const container = row("asset:postgres", "postgres");
  const asset = detail(container, [site]);

  expect(
    sourceOf(asset, { value: "prod", source_id: site.id, source_name: "hel1" }),
  ).toEqual({
    here: false,
    note: "inherited from hel1",
    goTo: "#/asset/asset:hel1",
  });

  expect(
    sourceOf(asset, {
      value: "prod",
      source_id: container.id,
      source_name: "postgres",
    }),
  ).toEqual({ here: true, note: "set here", goTo: null });
});
