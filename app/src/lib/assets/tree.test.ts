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
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import { noFilters } from "../ipc";
import type { AssetDetail, AssetRow } from "../ipc/assets";
import {
  addressOf,
  columnPathFor,
  emptyPath,
  heldByPath,
  problemBadge,
  selectionIn,
  sourceOf,
  type ColumnPath,
  stripFor,
  COLUMN_WIDTH,
  SPINE_WIDTH,
  estateQuery,
  matchesIn,
  walk,
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

// -- the keyboard walk (#430) ------------------------------------------------

/** The columns of a three-level estate, aligned with `columnPathFor`'s. */
const HEL1 = row("asset:hel1", "hel1", true);
const NBG1 = row("asset:nbg1", "nbg1", true);
const VM = row("asset:vm", "vm-db-01", true);
const SIBLING = row("asset:vm2", "vm-app-02");
const CONTAINER = row("asset:ct", "postgres");

/** `[top, inside hel1, inside vm-db-01]` — what the view holds while `vm-db-01` is selected. */
const LISTS = [[HEL1, NBG1], [SIBLING, VM], [CONTAINER]];

/** The layout with `vm-db-01` selected: three columns, the third one open. */
function atVm(): ColumnPath {
  return columnPathFor(detail(VM, [HEL1]));
}

/**
 * **Down moves to the next row of the column the selection is in.**
 *
 * The column is `vm-db-01`'s own — the second — and its rows arrive in the
 * order the backend sorted them, so the row under `vm-app-02` is `vm-db-01`
 * and the row under `vm-db-01` is nothing at all.
 */
test("down and up move within the column the selection is in", () => {
  const path = columnPathFor(detail(SIBLING, [HEL1]));
  expect(walk("ArrowDown", path, LISTS)).toEqual({ go: "asset", id: VM.id });
  expect(walk("ArrowUp", path, LISTS)).toEqual({ go: "nowhere" });
  expect(walk("ArrowUp", atVm(), LISTS)).toEqual({ go: "asset", id: SIBLING.id });
});

/**
 * **Nothing selected is the top column with no cursor in it**, so Down takes
 * the first row and Up the last — the two ends a reader arriving from the
 * keyboard means by them.
 */
test("the first arrow press on a bare tree enters the top column", () => {
  expect(walk("ArrowDown", emptyPath(), LISTS)).toEqual({ go: "asset", id: HEL1.id });
  expect(walk("ArrowUp", emptyPath(), LISTS)).toEqual({ go: "asset", id: NBG1.id });
});

/**
 * **Right and Enter descend into the first row of the next column**, and a
 * column with nothing in it is not descended into: an asset that holds
 * nothing has no next column, and the walk must not select out of one.
 */
test("right and enter descend into the first row under the selection", () => {
  expect(walk("ArrowRight", atVm(), LISTS)).toEqual({ go: "asset", id: CONTAINER.id });
  expect(walk("Enter", atVm(), LISTS)).toEqual({ go: "asset", id: CONTAINER.id });

  const leaf = columnPathFor(detail(CONTAINER, [HEL1, VM]));
  expect(walk("ArrowRight", leaf, LISTS)).toEqual({ go: "nowhere" });
});

/**
 * **Left selects the holder**, which is the column to the left with its own
 * row already marked — one step back up the path, not a jump to the top.
 */
test("left selects the asset that holds the selection", () => {
  expect(walk("ArrowLeft", atVm(), LISTS)).toEqual({ go: "asset", id: HEL1.id });
  // Nothing holds a top-level asset, and there is no column left of the
  // first: the key does nothing rather than clearing the selection.
  expect(walk("ArrowLeft", columnPathFor(detail(HEL1, [])), LISTS)).toEqual({ go: "nowhere" });
});

/**
 * **Escape unwinds one step, and the step above the top of the estate is the
 * bare tree** — after which it does nothing at all here, and the shell's own
 * ladder takes the press back to the room.
 *
 * That last rung is the whole reason `nowhere` is a value rather than a
 * silence: the view consumes an Escape it answered and passes on one it did
 * not, and a walk that always claimed the key would make the Tree the one
 * view a reader cannot leave with the keyboard.
 */
test("escape unwinds one step and then hands the key back", () => {
  expect(walk("Escape", atVm(), LISTS)).toEqual({ go: "asset", id: HEL1.id });
  expect(walk("Escape", columnPathFor(detail(HEL1, [])), LISTS)).toEqual({ go: "top" });
  expect(walk("Escape", emptyPath(), LISTS)).toEqual({ go: "nowhere" });
});

/** A key the walk has no meaning for is not a walk. */
test("a key the walk does not know does nothing", () => {
  expect(walk("a", atVm(), LISTS)).toEqual({ go: "nowhere" });
  expect(walk("Tab", atVm(), LISTS)).toEqual({ go: "nowhere" });
});

// -- the spine collapse (#430) -----------------------------------------------

/**
 * **A strip nobody has measured collapses nothing.**
 *
 * Zero is what an element reports before it is laid out — and what jsdom
 * reports for ever. Collapsing on it would put the whole estate behind spines
 * for one frame on every open, so an unknown width draws every column and
 * lets the strip scroll, which is what it did before this ticket.
 */
test("an unmeasured strip draws every column", () => {
  expect(stripFor(5, 0, null)).toEqual({ from: 0, to: 5 });
  expect(stripFor(5, -1, null)).toEqual({ from: 0, to: 5 });
});

/**
 * **Every column that fits is drawn in full**, and the arithmetic is the
 * mockup's: a full column is 220px, a spine 30px, and the widest window that
 * cannot take one more column is the one that collapses.
 *
 * Worked: four columns need 880px and fit in 960; five need 1100 and do not,
 * so one of them becomes a 30px spine and the other four take 910.
 */
test("the oldest columns collapse when the newest no longer fit", () => {
  expect(stripFor(4, 960, null)).toEqual({ from: 0, to: 4 });
  expect(stripFor(5, 960, null)).toEqual({ from: 1, to: 5 });
  // knobas' narrowest allowed window (`minWidth: 1100`) leaves the strip
  // about 780px beside the fixed pane, and there the fourth level collapses:
  // three columns and a spine are 720.
  expect(stripFor(4, 780, null)).toEqual({ from: 1, to: 4 });
  expect(stripFor(6, 780, null)).toEqual({ from: 3, to: 6 });
});

/**
 * **Where the collapse begins in the app as it ships**, which the acceptance
 * criterion's *"four or more levels"* is measured against.
 *
 * `tauri.conf.json` opens the window at 1280 and refuses to go below 1100;
 * the fixed pane takes 320 of it. So the default window holds four columns
 * and collapses at the fifth, and the narrowest one collapses at the fourth.
 * Pinned here rather than argued in a PR: the number a reader meets depends
 * on their window, and this is the sentence that says which.
 */
test("the default window collapses at the fifth column and the narrowest at the fourth", () => {
  const DEFAULT_STRIP = 1280 - 320 - 1;
  const NARROWEST_STRIP = 1100 - 320 - 1;

  expect(stripFor(4, DEFAULT_STRIP, null)).toEqual({ from: 0, to: 4 });
  expect(stripFor(5, DEFAULT_STRIP, null)).toEqual({ from: 1, to: 5 });
  expect(stripFor(3, NARROWEST_STRIP, null)).toEqual({ from: 0, to: 3 });
  expect(stripFor(4, NARROWEST_STRIP, null)).toEqual({ from: 1, to: 4 });
});

/**
 * **The selection's own column is always drawn**, however narrow the window:
 * a strip of nothing but spines would be a Tree with nowhere to stand.
 */
test("one column is always full, even in a window that cannot hold it", () => {
  expect(stripFor(3, 40, null)).toEqual({ from: 2, to: 3 });
  expect(stripFor(1, 10, null)).toEqual({ from: 0, to: 1 });
});

/**
 * **The two widths the arithmetic is measured in are the two the view draws.**
 *
 * `stripFor` decides which columns collapse from numbers that live in a
 * stylesheet, and a pure function cannot read one. So the copy is pinned from
 * this side: `.col`'s width is `COLUMN_WIDTH` and `.col.spine`'s is
 * `SPINE_WIDTH`, read back out of the component's own source. Without this,
 * widening a column in CSS would leave the collapse arithmetic quietly
 * measuring a column nobody draws, and the strip would overflow the pane the
 * collapse exists to protect.
 *
 * `app-css.test.ts`'s device — a rule read off disk rather than rendered —
 * because what is asserted is a *copy*, not a behaviour.
 */
test("the strip's arithmetic measures the widths this view actually draws", () => {
  const view = readFileSync(join(process.cwd(), "src/lib/assets/AssetsView.svelte"), "utf8");
  const widthOf = (selector: string): number => {
    const rule = new RegExp(`\\${selector} \\{[^}]*?width: (\\d+)px`, "s").exec(view);
    if (rule === null) throw new Error(`no width in the ${selector} rule`);
    return Number(rule[1]);
  };

  expect(widthOf(".col")).toBe(COLUMN_WIDTH);
  expect(widthOf(".col.spine")).toBe(SPINE_WIDTH);
});

/**
 * **Clicking a spine re-expands it**: the anchor names the first column drawn
 * in full, so the column the reader clicked is the one they get, and the
 * columns that no longer fit collapse at the *other* end.
 */
test("an anchor re-expands the column it names", () => {
  expect(stripFor(5, 960, 0)).toEqual({ from: 0, to: 4 });
  expect(stripFor(6, 780, 1)).toEqual({ from: 1, to: 4 });
});

/**
 * **A stale anchor is clamped rather than obeyed.**
 *
 * The anchor outlives nothing — the view drops it on every selection — but a
 * path that shortens under one would otherwise anchor the strip past its own
 * last column and draw no columns at all.
 */
test("an anchor past the last column is clamped to the deepest window", () => {
  expect(stripFor(5, 960, 9)).toEqual({ from: 1, to: 5 });
  expect(stripFor(5, 960, -3)).toEqual({ from: 0, to: 4 });
});

// -- search reveals the path (#430) ------------------------------------------

/** A search answer carrying one group of `kind`. */
function answer(groups: { kind: string; hits: { id: string; title: string; path?: string }[] }[]) {
  return {
    interpreted: { text: "", prefix: null, filters: noFilters(), unknown_tokens: [] },
    total: groups.reduce((count, group) => count + group.hits.length, 0),
    took_ms: 1,
    coverage: [],
    groups: groups.map((group) => ({
      kind: group.kind,
      label: group.kind,
      plural: group.kind,
      monogram: "??",
      total: group.hits.length,
      hits: group.hits.map((hit, index) => ({
        entity_id: hit.id,
        kind: group.kind,
        source_id: group.kind,
        updated_at: null,
        synced_at: "2026-09-06T10:00:00Z",
        title: hit.title,
        path: hit.path ?? null,
        rank: 1 - index / 10,
        snippet: [],
      })),
    })),
  };
}

/**
 * **The box asks for assets, by name, and for nothing else.**
 *
 * The kind filter is the whole of what makes this the *estate's* search rather
 * than a second launcher: the same engine, the same corpus list, one
 * dimension narrowed. `corpus::ASSET` (#428) is what answers it.
 */
test("the tree's query narrows the launcher's engine to assets", () => {
  const query = estateQuery("postgres");
  expect(query.raw).toBe("postgres");
  expect(query.filters.kinds).toEqual(["asset"]);
  expect(query.limit).toBeGreaterThan(0);
});

/**
 * **Only the asset group is offered**, whatever else came back.
 *
 * The engine merges the box's grammar over the caller's filters and the
 * *typed* one wins (`query::merge`), so a reader who types `#ticket` in this
 * box gets tickets from the engine. They are not assets, they have no path in
 * the estate, and revealing one would open the Tree at an address that is not
 * in it — so they are dropped here rather than drawn.
 */
test("a search answer offers its asset hits and drops the rest", () => {
  const response = answer([
    { kind: "ticket", hits: [{ id: "jira:PAY-1", title: "Postgres is down" }] },
    {
      kind: "asset",
      hits: [
        { id: "asset:ct", title: "postgres", path: "hel1 / vm-db-01" },
        { id: "asset:vm", title: "postgres-vm" },
      ],
    },
  ]);

  expect(matchesIn(response)).toEqual([
    { id: "asset:ct", name: "postgres", path: "hel1 / vm-db-01" },
    // A root asset sits nowhere, and the corpus answers that with `null`
    // rather than an empty line under every site.
    { id: "asset:vm", name: "postgres-vm", path: null },
  ]);
});

/** Nothing in the estate matched: no offers, and no group to read them from. */
test("an answer with no assets in it offers nothing", () => {
  expect(matchesIn(answer([{ kind: "note", hits: [{ id: "note:1", title: "postgres" }] }]))).toEqual(
    [],
  );
  expect(matchesIn(answer([]))).toEqual([]);
});

/**
 * **The first match is what Enter reveals**, and revealing is the address —
 * which is the whole of "search reveals the path": the columns are derived
 * from the asset's own `held_by`, so opening its address opens its path.
 */
test("revealing a match is opening its address", () => {
  const [first] = matchesIn(
    answer([{ kind: "asset", hits: [{ id: "asset:ct", title: "postgres" }] }]),
  );
  expect(addressOf({ id: first!.id })).toBe("#/asset/asset:ct");
});
