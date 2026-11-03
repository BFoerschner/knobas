/**
 * The asset tree: what a reader sees at a tree address, and what the view
 * asked for to draw it (issue #428, spec #427 stories 27, 33 and 35).
 *
 * The seam is **a rendered view in, user-visible text and the reads it issued
 * out** — `StandupView.test.svelte.ts` and `DayReview.test.svelte.ts`'s, and
 * the reason nothing here reaches into the component's state. The bridge is
 * injected through `ports`, so no Tauri and no database: what the backend puts
 * on a column and in the preview is `crates/knobas-app/tests/it/assets_ipc.rs`'s,
 * and what is asserted here is that a cold address draws one column per level
 * with the path marked in each, and that the preview draws the answer.
 *
 * What an asset's own page draws, the whole of it, is
 * `tf/asset/TfAsset.test.svelte.ts`'s; this file is the tree's.
 *
 * **This is the automated half of criterion 4's first clause.** The other half
 * — a headless-Chrome check against `--demo` — cannot be met as the ticket
 * writes it (`--demo` opens a Tauri window that Chrome cannot attach to, and
 * the demo profile carries no assets until #440), and the PR says so for
 * Björn to rule on. What a browser could only photograph, this file asserts.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import { type SearchQuery, type SearchResponse, noFilters } from "../ipc";
import type {
  AssetDetail,
  AssetProperty,
  AssetRow,
  AttachedMonitor,
  RouteRow,
} from "../ipc/assets";
import { DEBOUNCE_MS } from "../launcher";
import { createTestNav } from "./test-nav.svelte";
import AssetsView from "./AssetsView.svelte";
import { waitFor } from "../shell/test-wait";

/** The clock the history's *ago* readings are relative to. */
const NOW = new Date(2026, 8, 6, 12, 0, 0, 0);

/**
 * A corner of the real estate, which is the fixture spec #427 rules for
 * assets: nothing Tidewater-shaped is invented for them.
 *
 * Two VMs under the site so that a column has more than one row — a
 * one-row column cannot witness which row is marked.
 */
const SITE = row({
  id: "asset:hel1",
  type_id: "site",
  monogram: "SI",
  name: "hel1",
  // The only asset in this fixture that sets either — so every value the preview
  // shows further down is one that had to be walked up to.
  environment: "prod",
  owner: "Björn",
  health: "warn",
  inside: "warn",
  problems_inside: 1,
  linked_work: 0,
});
const VM = row({
  id: "asset:vm-db-01",
  parent_id: SITE.id,
  type_id: "vm",
  monogram: "VM",
  name: "vm-db-01",
  health: "warn",
  inside: "warn",
  problems_inside: 1,
  linked_work: 0,
});
const SIBLING = row({
  id: "asset:vm-app-02",
  parent_id: SITE.id,
  type_id: "vm",
  monogram: "VM",
  name: "vm-app-02",
});
const CONTAINER = row({
  id: "asset:postgres",
  parent_id: VM.id,
  type_id: "container",
  type_label: "Container",
  monogram: "CT",
  name: "postgres",
  has_children: false,
  // The one thing actually wrong in this estate; everything above it is warn
  // because of this row and not on its own account.
  status: "warn",
  health: "warn",
});
/**
 * A fourth level, under the *other* VM (#430).
 *
 * #428's fixture stopped at three because "a fourth would only make the
 * screenshot wider". The spine collapse needs one: four columns is the
 * shallowest path a window can fail to hold, and a three-level fixture cannot
 * witness a column collapsing. It hangs off `vm-app-02` so that every
 * assertion #428 made about the site → VM → container path still reads the
 * same estate it was written against.
 */
const GITEA = row({
  id: "asset:gitea",
  parent_id: SIBLING.id,
  type_id: "container",
  type_label: "Container",
  monogram: "CT",
  name: "gitea",
});
const GITEA_DB = row({
  id: "asset:gitea-db",
  parent_id: GITEA.id,
  type_id: "database",
  type_label: "Database",
  monogram: "DB",
  name: "gitea-db",
  has_children: false,
});
const ESTATE = [SITE, VM, SIBLING, CONTAINER, GITEA, GITEA_DB];

/**
 * The routes this corner of the estate exposes (#432).
 *
 * Two of them, on the *other* VM so that the container's own preview and the
 * proxy's are different previews: one landing on `gitea` and one landing on
 * nothing. The pair is what makes both halves of the preview assertable — a
 * fixture with one targeted route could not tell "exposes" from "reachable
 * via" apart on the exposing asset, and one with no untargeted route could
 * not witness the endpoint case at all.
 */
const GITEA_ROUTE: RouteRow = {
  id: "route:gitea",
  asset_id: SIBLING.id,
  asset_name: SIBLING.name,
  target_id: GITEA.id,
  target_name: GITEA.name,
  name: "Gitea",
  url: "http://127.0.0.1:3000/",
  visibility: "public",
  properties: [
    { key: "opened_by", label: "opened_by", value: { kind: "text", value: "compose" }, custom: true },
  ],
};
const DASHBOARD_ROUTE: RouteRow = {
  id: "route:dashboard",
  asset_id: SIBLING.id,
  asset_name: SIBLING.name,
  target_id: null,
  target_name: null,
  name: "Traefik dashboard",
  url: "http://traefik.hel1.example:8080/dashboard",
  visibility: "internal",
  properties: [],
};
const ROUTES = [DASHBOARD_ROUTE, GITEA_ROUTE];

/**
 * The monitors watching the container, as `get_asset` resolves them (#445).
 *
 * Two, and deliberately unalike: one Kuma is publishing, one it is not. The
 * second is what a **paused** monitor looks like from here -- #442 tombstones
 * a monitor that leaves `/metrics`, so it has no state left to report and no
 * page left to open -- and it is the row a section that simply printed
 * `state` would render as a blank.
 */
const MONITORING: AttachedMonitor[] = [
  {
    entity_id: "kuma:4",
    name: "postgres-check",
    state: "down",
    web_url: "http://127.0.0.1:3001/dashboard/4",
    tombstoned: false,
  },
  {
    entity_id: "kuma:5",
    name: "postgres-slow-query",
    state: null,
    web_url: null,
    tombstoned: true,
  },
  // Still in the mirror — it has a page — but with no state word: the reading
  // is missing on its own, which is the third of the three things this row can
  // say and not the same as the tombstone above. `AttachedMonitor.state` is
  // null for a state code the adapter has no word for, and a blank here would
  // read as "up" beside a live row exactly as a blank on a paused one would.
  {
    entity_id: "kuma:6",
    name: "postgres-wal",
    state: null,
    web_url: "http://127.0.0.1:3001/dashboard/6",
    tombstoned: false,
  },
];

/**
 * The names the estate file kept on the container.
 *
 * `postgres-check` is **also** in {@link MONITORING}: it is the name that
 * found its monitor, and the preview must not say it twice -- once as something
 * watching this asset and once as something still waiting to. `pgbouncer` is
 * the one still waiting.
 */
const NAMED_BY_THE_FILE = ["postgres-check", "pgbouncer"];

/**
 * `assets::ROUTES_REACHABLE`'s rule, as the fake bridge answers it: every
 * route whose target is on this asset's containment path, above it or below.
 *
 * A copy of the rule and not of its answer, for `inForce`'s reason -- the
 * seam under test is the *preview*, and a hard-coded list would draw the right
 * words for the wrong asset.
 */
function reachableVia(asset: AssetRow, heldBy: AssetRow[], estate: AssetRow[]): RouteRow[] {
  const holds = (root: string, id: string): boolean => {
    for (let at = estate.find((candidate) => candidate.id === id); at !== undefined; ) {
      if (at.id === root) return true;
      at = estate.find((candidate) => candidate.id === at?.parent_id);
    }
    return false;
  };
  const path = new Set([asset.id, ...heldBy.map((held) => held.id)]);
  return ROUTES.filter(
    (route) =>
      route.target_id !== null &&
      (path.has(route.target_id) || holds(asset.id, route.target_id)),
  );
}

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
    health: "none",
    inside: "none",
    problems_inside: 0,
    linked_work: 0,
    ...over,
  } as AssetRow;
}

/**
 * The backend's inheritance walk, as the fake bridge answers it — nearest
 * setter at or above, innermost first.
 *
 * A copy of `assets::inherited`'s rule rather than of its answer: the seam
 * under test here is the *preview*, and a fixture that hard-coded "hel1" as the
 * source would still draw the right words if the preview read the wrong field.
 */
function inForce<T>(
  asset: AssetRow,
  heldBy: AssetRow[],
  of: (row: AssetRow) => T | null,
): { value: T; source_id: string; source_name: string } | null {
  for (const at of [asset, ...[...heldBy].reverse()]) {
    const value = of(at);
    if (value !== null)
      return { value, source_id: at.id, source_name: at.name };
  }
  return null;
}

function property(over: Partial<AssetProperty> & Pick<AssetProperty, "key">): AssetProperty {
  return { label: over.key, value: null, custom: false, ...over };
}

/**
 * The preview's read for one asset, ancestors walked over the parent field.
 *
 * Takes the estate it is reading, so a test can hand {@link render} an estate
 * of its own and have the *preview* answer from it too — without that, a bespoke
 * estate draws its columns and then fails every deep read against the default
 * one.
 */
function detailOf(id: string, estate: AssetRow[] = ESTATE): AssetDetail {
  const asset = estate.find((candidate) => candidate.id === id);
  if (asset === undefined) throw { code: "not_found", message: `no asset ${id}` };
  const heldBy: AssetRow[] = [];
  for (let at = asset; at.parent_id !== null; ) {
    const parent = estate.find((candidate) => candidate.id === at.parent_id);
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
            // Declared by the type and filled in by nobody: the preview lists
            // what is filled in, so this one counts in "1 of 2 set" only.
            property({ key: "ports", label: "Ports" }),
            property({
              key: "zzz-note",
              label: "zzz-note",
              value: { kind: "text", value: "typed on this one" },
              custom: true,
            }),
          ]
        : [],
    effective_environment: inForce(
      asset,
      heldBy,
      (candidate) => candidate.environment,
    ),
    effective_owner: inForce(asset, heldBy, (candidate) => candidate.owner),
    held_by: heldBy,
    holds: estate.filter((candidate) => candidate.parent_id === asset.id),
    exposes: ROUTES.filter((route) => route.asset_id === asset.id),
    reachable_via: reachableVia(asset, heldBy, estate),
    // #435's surface has a file of its own (`AssetsView.links.test.svelte.ts`),
    // and this fixture stays about the read: an asset with no links draws the
    // panel's empty state, which is what every test here was written over.
    links: [],
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
            // The two lines an **import** writes (#439), which nothing drew
            // until #440 put an imported estate in front of a reader: the
            // origin line every created asset carries, and the edit that keeps
            // a monitor name the mirror does not hold yet. Both are `imported`
            // and `edited` with details no other writer produces, so both fell
            // through `line()`'s last branch and rendered as
            // `nothing: nothing → nothing`.
            {
              id: 0,
              at: new Date(2026, 8, 4, 9, 0, 0, 0).toISOString(),
              actor: "import",
              verb: "edited",
              entity_id: asset.id,
              detail: { field: "monitors", added: ["teamcity (tunnel)"], estate: "knobas test estate" },
            },
            {
              id: -1,
              at: new Date(2026, 8, 4, 9, 0, 0, 0).toISOString(),
              actor: "import",
              verb: "imported",
              entity_id: asset.id,
              detail: { estate: "knobas test estate" },
            },
          ]
        : [],
    monitors: asset.id === CONTAINER.id ? NAMED_BY_THE_FILE : [],
    monitoring: asset.id === CONTAINER.id ? MONITORING : [],
    monitor_targets: [],
  };
}

let target: HTMLDivElement;
let app: Record<string, unknown> | undefined;
/**
 * Every `scrollIntoView` the view asked for, with the options it asked with.
 *
 * jsdom implements no scrolling at all, so the method is *defined* here rather
 * than spied on — which is also what makes the walk's call observable. It is
 * removed again after each test: a global left behind would make the next
 * file's components think they can scroll.
 */
let scrolled: ScrollIntoViewOptions[] = [];

/**
 * Mount the view at `hash` and record every read it issues.
 *
 * `asked` is the point of the harness as much as the markup is: story 35 says
 * an address opened cold draws what a click draws, and the only way to see
 * that is which parents the view asked for having been given nothing but the
 * address and the preview's answer.
 */
function render(hash: string, estate: AssetRow[] = ESTATE, over: Over = {}) {
  const asked: (string | null)[] = [];
  const searched: SearchQuery[] = [];
  const opened: string[] = [];
  location.hash = hash;
  const nav = createTestNav();
  app = mount(AssetsView, {
    target,
    props: {
      nav,
      now: () => NOW,
      // The strip is measured, and jsdom measures every element as zero — so
      // the width a reader's window would have is injected. Zero is also a
      // real state (nothing laid out yet) and is what every test that is not
      // about the collapse leaves it in.
      ...(over.stripWidth === undefined ? {} : { stripWidth: () => over.stripWidth ?? 0 }),
      ports: {
        // #429's type table. This file is about the *read*, and nothing here
        // creates -- but the view asks for the table on mount, so a port left
        // out here falls through to the real `invoke` and every test in the
        // file would render behind a "Creating is unavailable" line and a
        // dead plus. Empty is the honest fixture: no test here presses one.
        assetTypes: () => Promise.resolve([]),
        search: (query: SearchQuery) => {
          searched.push(query);
          return Promise.resolve(over.answer ?? matchesFor(query, estate));
        },
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
            return Promise.resolve(detailOf(assetId, estate));
          } catch (cause) {
            return Promise.reject(cause);
          }
        },
        // The OS browser, for `assetTypes`' reason above: a route's URL in
        // the preview is a button, and a port left out falls through to the
        // real `openExternal`, which would hand the URL to
        // `@tauri-apps/plugin-opener`.
        openExternal: (url: string) => {
          opened.push(url);
          return Promise.resolve();
        },
      },
    },
  });
  flushSync();
  return { nav, asked, searched, opened };
}

/** What a test may put in front of the view besides the estate. */
interface Over {
  /** The strip's measured width in pixels. */
  stripWidth?: number;
  /** A search answer of the test's own, for the shapes `matchesFor` cannot make. */
  answer?: SearchResponse;
}

/**
 * The engine's answer, faked: every asset whose name contains the query.
 *
 * A substring match where the real corpus is PostgreSQL FTS over name and
 * ancestor path. What that difference costs is named rather than hidden: this
 * fake certifies the *box* — what it asks for, which hit it reveals, what the
 * list shows — and what certifies the query is
 * `crates/knobas-app/tests/it/search_ipc.rs`, where the same filter runs against
 * `corpus::ASSET` on a real database.
 */
function matchesFor(query: SearchQuery, estate: AssetRow[]): SearchResponse {
  const needle = query.raw.trim().toLowerCase();
  const hits = estate
    .filter((asset) => needle !== "" && asset.name.toLowerCase().includes(needle))
    .map((asset, index) => ({
      entity_id: asset.id,
      kind: "asset",
      source_id: "asset",
      updated_at: null,
      synced_at: NOW.toISOString(),
      title: asset.name,
      path: pathTextOf(asset, estate),
      state: null,
      rank: 1 - index / 10,
      snippet: [],
      excerpts: [],
    }));
  return {
    interpreted: { text: query.raw, prefix: null, filters: noFilters(), unknown_tokens: [] },
    groups:
      hits.length === 0
        ? []
        : [
            {
              kind: "asset",
              label: "Asset",
              plural: "Assets",
              monogram: "AS",
              total: hits.length,
              hits,
            },
          ],
    total: hits.length,
    took_ms: 1,
    coverage: [],
  };
}

/** `knobas.asset.path_text`: the ancestors' names, outermost first. */
function pathTextOf(asset: AssetRow, estate: AssetRow[]): string | null {
  const names: string[] = [];
  for (let at = asset; at.parent_id !== null; ) {
    const parent = estate.find((candidate) => candidate.id === at.parent_id);
    if (parent === undefined) break;
    names.unshift(parent.name);
    at = parent;
  }
  return names.length === 0 ? null : names.join(" / ");
}

/**
 * Press a key on the view, and hand back the event so a test can ask whether
 * the tree **consumed** it.
 *
 * Dispatched on the element under the cursor rather than on `window`: the
 * tree's handler is on its own section precisely so an unanswered `Escape`
 * bubbles on to the shell's ladder, and a press synthesised at the top of the
 * document would never travel that path.
 */
function press(key: string, on: Element | null = target.querySelector("section.tf-et")): Event {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  on?.dispatchEvent(event);
  flushSync();
  return event;
}

/** The spine columns, left to right, by the asset each is labelled with. */
function spines(): string[] {
  return [...target.querySelectorAll("button.tf-et-spine .tf-et-vn")].map((label) =>
    (label.textContent ?? "").trim(),
  );
}

/** The search box, which every test that types has to find. */
function box(): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>(".tf-et-find input");
  if (input === null) throw new Error("the tree has no search box");
  return input;
}

/**
 * Type into the search box and wait for the search it sends once the debounce
 * is out, then for its answer to draw (#2048; it was a fixed sleep). An empty
 * box asks nothing and arms no timer, so there is nothing to wait for.
 */
async function type(searched: SearchQuery[], text: string): Promise<void> {
  const sent = searched.length;
  const input = box();
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  if (text.trim() !== "") {
    await waitFor(() => expect(searched.length, `the search for ${text}`).toBeGreaterThan(sent));
    // The answer is a resolved promise: one macrotask turn drains its chain.
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  flushSync();
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** What the preview beside the columns says, as one line of text. */
function preview(): string {
  return (target.querySelector("aside.tf-et-pane")?.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** The asset the preview names, or `null` when it names none. */
function previewed(): string | null {
  const name = target.querySelector("aside.tf-et-pane h2");
  return name === null ? null : (name.textContent ?? "").trim();
}

/** Each column's row labels, left to right. */
function columns(): string[][] {
  return [...target.querySelectorAll("ol.tf-et-col")].map((column) =>
    [...column.querySelectorAll("button.tf-et-row span.tf-et-nm")].map((name) =>
      (name.textContent ?? "").trim(),
    ),
  );
}

/** The row marked as the path in each column, left to right. */
function marked(): (string | null)[] {
  return [...target.querySelectorAll("ol.tf-et-col")].map((column) => {
    const on = column.querySelector('button.tf-et-row[aria-current="true"] span.tf-et-nm');
    return on === null ? null : (on.textContent ?? "").trim();
  });
}

/** A button anywhere in the view, by its trimmed text. */
function buttonNamed(label: string, within: ParentNode = target): HTMLButtonElement | undefined {
  return [...within.querySelectorAll<HTMLButtonElement>("button")].find(
    (button) => (button.textContent ?? "").trim() === label,
  );
}

beforeEach(() => {
  target = document.createElement("div");
  document.body.append(target);
  location.hash = "";
  scrolled = [];
  Element.prototype.scrollIntoView = function scrollIntoView(
    options?: boolean | ScrollIntoViewOptions,
  ) {
    scrolled.push(options as ScrollIntoViewOptions);
  };
});

afterEach(() => {
  if (app) unmount(app);
  app = undefined;
  target.remove();
  location.hash = "";
  Reflect.deleteProperty(Element.prototype, "scrollIntoView");
  vi.restoreAllMocks();
  vi.useRealTimers();
});

/**
 * **`#/assets/tree` is the estate's top level and one column.**
 *
 * The read it issues is `null`, which is a *level* and not a missing filter:
 * a view that asked with no parent at all would draw a first column holding
 * the whole estate. The VM and the container exist and are not in it.
 */
test("the bare address draws the roots and nothing is picked", async () => {
  const { asked } = render("#/assets/tree");

  await waitFor(() => expect(columns()).toEqual([["hel1"]]));
  expect(asked).toEqual([null]);
  expect(marked()).toEqual([null]);
  expect(preview()).toBe("Pick an asset to see what it holds and how it is reached.");
});

/**
 * **No tab strip** (#1459): the *Monitors* tab beside *Tree* named a roster
 * that went with the rooms UI (#1087), and one tab left is no strip. The tree
 * is reached from Dependencies' *Asset tree* button.
 */
test("the tree draws no tab strip and no Monitors tab", async () => {
  render("#/assets");

  await waitFor(() => expect(columns()).toEqual([["hel1"]]));
  expect(target.querySelector("nav[aria-label='Assets views']")).toBeNull();
  const labels = [...target.querySelectorAll("button, a")].map((el) => el.textContent?.trim());
  expect(labels).not.toContain("Monitors");
  expect(labels).not.toContain("Tree");
});

/**
 * **A pick's address, opened cold, draws one column per level with the path
 * marked in each, and the preview beside it.**
 *
 * Story 35: nothing is clicked here; the columns are derived from `held_by`,
 * which arrives with the asset, so what is asserted is that the view walked
 * nothing and cached nothing.
 *
 * No trailing column: the container holds nothing, and an empty column would
 * promise a level that is not there. The preview lists what is filled in and
 * counts the type's own fields; the unfilled *Ports* is for the asset's page.
 */
test("a pick's address draws its whole path and the preview", async () => {
  const { asked } = render("#/assets/asset:postgres");

  await waitFor(() =>
    expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres"]]),
  );
  // Four reads, and the shape of the list is the point. The first `null` is
  // the cold frame: the layout is `emptyPath()` until the preview's read
  // lands, so the top column is asked for before there is a path to derive.
  // Then the path arrives and all three levels are asked for together -- one
  // assignment, because a half-drawn Miller layout is a path with a hole in
  // it. Nothing above the site is asked for twice, and nothing is walked.
  expect(asked).toEqual([null, null, SITE.id, VM.id]);
  expect(marked()).toEqual(["hel1", "vm-db-01", "postgres"]);
  // The deepest of the path is the pick, and only it carries the outline.
  const picked = [...target.querySelectorAll("button.tf-et-row.tf-sel")];
  expect(picked.map((row) => row.getAttribute("data-asset"))).toEqual([CONTAINER.id]);

  expect(previewed()).toBe("postgres");
  const said = preview();
  expect(said).toContain("Properties 1 of 2 set");
  expect(said).toContain("Image postgres:18");
  expect(said).toContain("zzz-note typed on this one");
  expect(said).not.toContain("Ports");
  expect(said).toContain("Holds Nothing");
  expect(said).toContain("Watched by postgres-check, postgres-slow-query, postgres-wal");
});

/**
 * **The preview's way on is the asset's page**: *Open asset* goes to
 * `#/asset/<id>`, a double click on a row does the same, and so does `Enter`
 * on a row with nothing under it, where the walk has no column to go into.
 */
test("Open asset, a double click and Enter on a leaf each open the asset's page", async () => {
  const { nav } = render("#/assets/asset:postgres");
  await waitFor(() => expect(previewed()).toBe("postgres"));

  buttonNamed("Open asset", target.querySelector("aside.tf-et-pane")!)!.click();
  flushSync();
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: CONTAINER.id });

  nav.go("#/assets/asset:postgres");
  flushSync();
  await waitFor(() => expect(previewed()).toBe("postgres"));
  target
    .querySelector(`button.tf-et-row[data-asset="${VM.id}"]`)!
    .dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
  flushSync();
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: VM.id });

  nav.go("#/assets/asset:postgres");
  flushSync();
  await waitFor(() => expect(previewed()).toBe("postgres"));
  const enter = press("Enter");
  expect(enter.defaultPrevented).toBe(true);
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: CONTAINER.id });
});

/**
 * **A pick that holds something opens the column under it, and the columns
 * follow the address rather than the click.**
 *
 * The click sets the address; the address is read back; the columns follow the
 * answer. That is the whole reason a cold address and a click land in the same
 * place, so the assertion is on the address as well as on the markup.
 */
test("picking an asset that holds something opens the next column", async () => {
  const { nav } = render("#/assets/asset:postgres");
  await waitFor(() => expect(columns()).toHaveLength(3));

  const vm = [...target.querySelectorAll<HTMLButtonElement>("button.tf-et-row")].find(
    (button) => button.textContent?.includes("vm-db-01"),
  );
  vm?.click();
  flushSync();

  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: VM.id });
  await waitFor(() =>
    expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres"]]),
  );
  expect(marked()).toEqual(["hel1", "vm-db-01", null]);
  await waitFor(() => expect(previewed()).toBe("vm-db-01"));
});

/**
 * **An estate with nothing in it says so in its own words**, and offers the
 * two ways out of it.
 *
 * The branch a fresh install is in until #439's import or a hand-made asset,
 * and the one a reader would otherwise take for a failed read.
 */
test("an empty estate is told apart from a read that has not landed", async () => {
  render("#/assets/tree", []);

  await waitFor(() => expect(text()).toContain("Nothing in the estate yet."));
  expect(columns()).toEqual([]);
  const empty = target.querySelector(".tf-et-empty")!;
  expect(buttonNamed("New asset", empty)).toBeDefined();
  expect(buttonNamed("Import…", empty)).toBeDefined();
});

/**
 * **A pick naming an asset that is not there draws the failure, and drops
 * the pick.**
 *
 * A preview still showing the last asset would be the view lying about what
 * the address names. The page's half, a deep link to a gone asset's page, is
 * `TfAsset.test.svelte.ts`'.
 */
test("a pick of an asset that is gone says so instead of showing the last one", async () => {
  const { nav } = render("#/assets/asset:postgres");
  await waitFor(() => expect(previewed()).toBe("postgres"));

  nav.go("#/assets/asset:nobody");
  flushSync();

  await waitFor(() => expect(text()).toContain("no asset asset:nobody"));
  expect(previewed()).toBeNull();
  expect(columns()).toEqual([]);
});

/**
 * **The preview says where an inherited value came from, and the note is a
 * link to the asset that set it**, picked in the tree — stories 8, 9 and 10
 * (#431).
 *
 * Nothing sets an environment or an owner between the container and the site,
 * so both values are the site's; the note names it, and one click opens the
 * site in the columns. A preview that drew the value without its source would
 * leave the reader with no way to find the place `prod` can be changed.
 */
test("an inherited value names the ancestor it came from and picks it", async () => {
  const { nav } = render("#/assets/asset:postgres");
  await waitFor(() => expect(previewed()).toBe("postgres"));

  expect(preview()).toContain("Environment prod inherited from hel1");
  expect(preview()).toContain("Owner Björn inherited from hel1");
  // Health is the worst of this asset and everything under it, on the chip.
  expect(target.querySelector("aside.tf-et-pane header .tf-chip")?.textContent?.trim()).toBe("warning");

  const note = [
    ...target.querySelectorAll<HTMLButtonElement>("aside.tf-et-pane button.tf-et-src"),
  ].find((button) => button.textContent?.includes("inherited from hel1"));
  expect(note).toBeDefined();
  note?.click();
  flushSync();
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: SITE.id });
});

/**
 * **A value set on the asset itself reads *set here* and is not a link.**
 *
 * The negative for the test above: a link that led back to the asset the
 * reader is already looking at would be a click that does nothing, which is
 * worse than no link at all. The site is where both values are set.
 */
test("a value set on the asset itself says so and offers no link", async () => {
  render("#/assets/asset:hel1");
  await waitFor(() => expect(preview()).toContain("Environment prod set here"));

  expect(preview()).toContain("Owner Björn set here");
  expect(preview()).not.toContain("inherited from");
  expect(target.querySelectorAll("aside.tf-et-pane button.tf-et-src")).toHaveLength(0);
});

/**
 * **An asset with nothing set above it says so, rather than defaulting.**
 *
 * "Not set anywhere" and `dev` are different facts, and an estate whose top
 * has no environment is the state every fresh import starts in.
 */
test("an asset with no environment anywhere above it says nothing is set", async () => {
  const bare = row({ id: "asset:lonely", name: "lonely", has_children: false });
  render("#/assets/tree", [bare]);
  await waitFor(() => expect(columns()).toEqual([["lonely"]]));

  target.querySelector<HTMLButtonElement>("button.tf-et-row")?.click();
  flushSync();
  await waitFor(() => expect(preview()).toContain("Environment Not set anywhere"));
  expect(preview()).toContain("Owner Not set anywhere");
});

/**
 * **The badge counts what is inside a row and takes its colour from the worst
 * of it** — story 32.
 *
 * Three roots in one column, which is what makes this a test rather than a
 * screenshot: the red one, the amber one, and the one holding nothing wrong,
 * side by side. The third is the negative — a badge reading `0` is a mark the
 * eye stops on to learn there is nothing to learn.
 *
 * The **fourth** row is the case that decides whether the tone is read off
 * `inside` or off `health`: an asset that is itself down while holding one
 * warning. It is a red row with an *amber* badge — its own red is the dot
 * beside its name — and an implementation colouring the badge from `health`
 * draws it red.
 */
test("a column row badges what is wrong inside it, in the worst tone inside", async () => {
  const estate = [
    row({ id: "asset:red", name: "red", health: "down", inside: "down", problems_inside: 2, linked_work: 0 }),
    row({ id: "asset:amber", name: "amber", health: "warn", inside: "warn", problems_inside: 1, linked_work: 0 }),
    row({ id: "asset:clean", name: "clean", health: "up", inside: "none", problems_inside: 0, linked_work: 0 }),
    row({
      id: "asset:worse",
      name: "worse",
      status: "down",
      health: "down",
      inside: "warn",
      problems_inside: 1,
      linked_work: 0,
    }),
  ];
  render("#/assets/tree", estate);
  await waitFor(() => expect(columns()).toEqual([["amber", "clean", "red", "worse"]]));

  const badges = [...target.querySelectorAll("ol.tf-et-col li")].map((item) => {
    const badge = item.querySelector("span.tf-et-badge");
    return badge === null
      ? null
      : {
          name: (item.querySelector("span.tf-et-nm")?.textContent ?? "").trim(),
          count: (badge.textContent ?? "").trim(),
          tone: badge.classList.contains("tf-et-down") ? "down" : "warn",
          title: badge.getAttribute("title"),
        };
  });
  expect(badges).toEqual([
    { name: "amber", count: "1", tone: "warn", title: "1 problem inside" },
    null,
    { name: "red", count: "2", tone: "down", title: "2 problems inside" },
    { name: "worse", count: "1", tone: "warn", title: "1 problem inside" },
  ]);
  // The row's own status, beside its name: only the one rated down by hand.
  const own = [...target.querySelectorAll("ol.tf-et-col li")].map(
    (item) => item.querySelector("button.tf-et-row > .tf-dot")?.classList.contains("tf-err") ?? false,
  );
  expect(own).toEqual([false, false, false, true]);
});

// -- the keyboard walk, the spines and the search (#430) ---------------------

/**
 * **The arrows and Enter walk the estate, and every step is an address.**
 *
 * Story 29's own sentence, through the rendered view: down the top column,
 * right into what the pick holds, and the preview following each step. The
 * assertion is on the address as well as on the markup, because the walk
 * moves the *pick* and the columns are derived from it.
 *
 * At the end, `Enter` on the container, which holds nothing, opens no column:
 * it opens the container's page, where the walk goes on.
 */
test("the arrows and enter walk three levels, and the preview follows", async () => {
  const { nav } = render("#/assets/tree");
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));

  press("ArrowDown");
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: SITE.id });
  await waitFor(() => expect(marked()).toEqual(["hel1", null]));

  press("ArrowRight");
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: SIBLING.id });
  await waitFor(() => expect(marked()).toEqual(["hel1", "vm-app-02", null]));

  press("ArrowDown");
  await waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", null]));

  press("Enter");
  await waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", "postgres"]));
  await waitFor(() => expect(previewed()).toBe("postgres"));

  press("Enter");
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: CONTAINER.id });
});

/**
 * **The focused row is the picked one**, which is what makes the walk
 * visible: the `:focus-visible` ring is drawn on whatever has focus, and a
 * walk that moved the pick without moving focus would leave the ring on the
 * row the reader started from.
 */
test("the walk carries focus to the row it picked", async () => {
  render("#/assets/tree");
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));

  press("ArrowDown");
  await waitFor(() => expect(marked()).toEqual(["hel1", null]));

  const on = target.querySelector('button.tf-et-row[aria-current="true"]');
  expect(on?.textContent).toContain("hel1");
  await waitFor(() => expect(document.activeElement).toBe(on));
});

/** A step into a deeper column carries focus to the row picked there, not to the path's row in the first column (#2210). */
test("the walk carries focus to the row it picked in a deeper column", async () => {
  render("#/assets/tree");
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));
  press("ArrowDown");
  await waitFor(() => expect(marked()).toEqual(["hel1", null]));
  press("ArrowRight");
  await waitFor(() => expect(marked()).toEqual(["hel1", "vm-app-02", null]));

  await waitFor(() => expect(document.activeElement?.getAttribute("data-asset")).toBe(SIBLING.id));
});

/**
 * **A reader who asked for less motion is scrolled without any.**
 *
 * The row the walk lands on is brought into view, and *how* is the one thing
 * on this surface that animates. `Flap.svelte`'s reading of the media query,
 * for its reason.
 */
test("reduced motion is respected when the walk scrolls a row into view", async () => {
  const reduce = vi
    .spyOn(window, "matchMedia")
    .mockImplementation(
      (query: string) => ({ matches: query.includes("reduce"), media: query }) as MediaQueryList,
    );

  render("#/assets/tree");
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));
  press("ArrowDown");
  await waitFor(() => expect(scrolled).not.toHaveLength(0));
  expect(scrolled.at(-1)?.behavior).toBe("auto");

  reduce.mockImplementation((query: string) => ({ matches: false, media: query }) as MediaQueryList);
  // The same press again, now without the preference — the option is read per
  // press and not once at mount, which is what a reader who changes the
  // system setting while the app is open would expect.
  press("ArrowRight");
  await waitFor(() => expect(scrolled.at(-1)?.behavior).toBe("smooth"));
});

/**
 * **Escape unwinds one step per press, and the press it has no step for is
 * handed on.**
 *
 * The rung above the top of the estate is the bare tree; above that the tree
 * has nothing to unwind, and the key belongs to the shell's ladder, which
 * takes it back to Dependencies. A view that consumed it anyway would be the
 * one surface a reader cannot leave with the keyboard — so the last press is
 * asserted **not** to be consumed.
 */
test("escape unwinds the path one level at a time and then falls through", async () => {
  const { nav } = render("#/assets/asset:postgres");
  await waitFor(() => expect(columns()).toHaveLength(3));

  expect(press("Escape").defaultPrevented).toBe(true);
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: VM.id });
  // Waited out between presses: the layout is derived from the preview's
  // answer, so the rung a press lands on is the one the *drawn* path offers.
  await waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", null]));

  press("Escape");
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: SITE.id });
  await waitFor(() => expect(marked()).toEqual(["hel1", null]));

  press("Escape");
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: null });
  await waitFor(() => expect(marked()).toEqual([null]));

  const last = press("Escape");
  expect(last.defaultPrevented).toBe(false);
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: null });
});

/**
 * **Four levels in a window that holds three: the oldest column becomes a
 * spine labelled by its asset, and clicking the spine re-expands it.**
 *
 * Story 28, and the collapse is width-driven — 780px is what knobas' narrowest
 * allowed window (`minWidth: 1100`) leaves beside the fixed preview. The label
 * is the asset the reader walked *through*, not the column's parent.
 *
 * Clicking it anchors the strip there, which pushes the collapse to the other
 * end — the deepest column becomes the spine and the site is a full column
 * again. The preview does not move, which is the whole point of the collapse.
 */
test("a path too deep for the window collapses its oldest columns to spines", async () => {
  render("#/assets/asset:gitea-db", ESTATE, { stripWidth: 780 });

  await waitFor(() => expect(spines()).toEqual(["hel1"]));
  expect(columns()).toEqual([["vm-app-02", "vm-db-01"], ["gitea"], ["gitea-db"]]);
  expect(marked()).toEqual(["vm-app-02", "gitea", "gitea-db"]);
  expect(previewed()).toBe("gitea-db");

  target.querySelector<HTMLButtonElement>("button.tf-et-spine")?.click();
  flushSync();

  // Anchored at the site, the collapse moves to the other end: the deepest
  // column is the spine now, labelled with the asset picked in it.
  await waitFor(() => expect(spines()).toEqual(["gitea-db"]));
  expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["gitea"]]);
  expect(previewed()).toBe("gitea-db");
});

/** A window with room for every column draws no spine at all. */
test("a path the window holds collapses nothing", async () => {
  render("#/assets/asset:gitea-db", ESTATE, { stripWidth: 1400 });

  await waitFor(() => expect(columns()).toHaveLength(4));
  expect(spines()).toEqual([]);
});

/**
 * **Typing a name in the tree's search opens the path to the first match and
 * picks it** — story 30.
 *
 * The reveal is an *address*: the box hands the first match's id to the
 * nav, the preview reads it, and the columns are derived from the `held_by`
 * that comes back. So the box needs to know nothing about where an asset
 * lives, and a match four levels down opens four columns.
 */
test("typing a name opens the path to the first match and picks it", async () => {
  const { nav, searched } = render("#/assets/tree");
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));

  await type(searched, "gitea-db");
  // What the box asked for, which is the estate's corpus and nothing else.
  expect(searched.at(-1)?.filters.kinds).toEqual(["asset"]);
  expect(searched.at(-1)?.raw).toBe("gitea-db");
  // The list says where each match lives before the reader takes it.
  expect(target.querySelector(".tf-et-res")?.textContent).toContain("hel1 / vm-app-02 / gitea");

  press("Enter", box());
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: GITEA_DB.id });
  await waitFor(() =>
    expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["gitea"], ["gitea-db"]]),
  );
  expect(marked()).toEqual(["hel1", "vm-app-02", "gitea", "gitea-db"]);
  expect(previewed()).toBe("gitea-db");
});

/**
 * **One keystroke is not one query**, and a box with nothing in it asks
 * nothing at all: the debounce is the launcher's own, so the two boxes in this
 * app wait the same amount of time.
 */
test("the box waits for the typing to stop, and an empty box asks nothing", async () => {
  // A fake clock, so "past the debounce" is a step and not a sleep (#2048).
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  const { searched } = render("#/assets/tree");
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));

  const input = box();
  for (const text of ["g", "gi", "git"]) {
    input.value = text;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
  }
  vi.advanceTimersByTime(DEBOUNCE_MS);
  expect(searched.map((query) => query.raw)).toEqual(["git"]);

  await type(searched, "");
  vi.advanceTimersByTime(DEBOUNCE_MS);
  expect(searched.map((query) => query.raw)).toEqual(["git"]);
});

/**
 * **A search that matches nothing says so**, and the columns are left alone.
 *
 * The box is a way into the estate, not a filter over it: a query with no
 * match must not blank the path the reader is standing in.
 */
test("a query nothing matches leaves the columns where they were", async () => {
  const { searched } = render("#/assets/asset:postgres");
  await waitFor(() => expect(columns()).toHaveLength(3));

  await type(searched, "nothing-is-called-this");

  expect(text()).toContain("Nothing in the estate matches");
  expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["postgres"]]);
  expect(marked()).toEqual(["hel1", "vm-db-01", "postgres"]);
});

/**
 * **Escape in the box clears the query first**, which is the launcher's rung 1
 * and the reason it is a rung: a reader who typed by mistake gets their
 * columns back without losing the asset they were reading.
 */
test("escape in the search box clears the query before it unwinds anything", async () => {
  const { nav, searched } = render("#/assets/asset:postgres");
  await waitFor(() => expect(columns()).toHaveLength(3));

  await type(searched, "gitea");
  expect(text()).toContain("gitea-db");

  press("Escape", box());
  expect(box().value).toBe("");
  expect(text()).not.toContain("gitea-db");
  // The pick is untouched: clearing the box is not unwinding the path.
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: CONTAINER.id });
});

/**
 * **The arrows walk the offered matches while the box has the focus**, and
 * the columns underneath do not move: two lists and one keyboard, so the one
 * in front of the reader is the one that answers.
 */
test("the arrows move through the matches instead of the columns", async () => {
  const { nav, searched } = render("#/assets/tree");
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));

  await type(searched, "vm-");
  press("ArrowDown", box());
  expect(target.querySelector(".tf-et-hit.tf-et-on")?.textContent).toContain("vm-app-02");
  press("Enter", box());

  // `vm-db-01` is the first match and `vm-app-02` the second: the press moved
  // the cursor down the offers, and `Enter` took the one it had moved to.
  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: SIBLING.id });
});

/**
 * **A re-expanded spine is a look at one path, and the next pick is
 * another.**
 *
 * So the anchor is kept beside the pick it was clicked in and does not apply
 * to a different one: carried, it would collapse the columns the reader had
 * just walked into, at a level they had never clicked a spine on.
 *
 * The move is to `gitea`, which is one level *up* and still four columns wide
 * — it holds `gitea-db`, so a column opens under it. That is what makes the
 * assertion able to fail: a surviving anchor of 0 would draw the deepest
 * column as the spine instead of the oldest.
 */
test("clicking a spine anchors this path and not the next one", async () => {
  const { nav } = render("#/assets/asset:gitea-db", ESTATE, { stripWidth: 780 });
  await waitFor(() => expect(spines()).toEqual(["hel1"]));

  target.querySelector<HTMLButtonElement>("button.tf-et-spine")?.click();
  flushSync();
  await waitFor(() => expect(spines()).toEqual(["gitea-db"]));

  nav.go(`#/assets/${GITEA.id}`);
  flushSync();

  await waitFor(() => expect(marked()).toEqual(["vm-app-02", "gitea", null]));
  expect(spines()).toEqual(["hel1"]);
  expect(columns()).toEqual([["vm-app-02", "vm-db-01"], ["gitea"], ["gitea-db"]]);
});

/**
 * **Both ends of a route, in the preview of the asset each end is about**
 * (#432): the rows the wires leave from.
 *
 * The exposing VM lists what it offers and how it is reached; the untargeted
 * route is under *Exposes* and reaches nobody. Every row carries the key its
 * wire is measured by. The route's whole story, its properties and history,
 * are on the asset's page.
 */
test("the preview draws what an asset exposes and what reaches it, each row a wire's end", async () => {
  const { opened } = render("#/assets/asset:vm-app-02");
  await waitFor(() => expect(preview()).toContain("Exposes"));

  const said = preview();
  expect(said).toContain("Gitea");
  expect(said).toContain("public");
  expect(said).toContain("Traefik dashboard");
  expect(said).toContain("lands on nothing knobas knows");
  expect(said).toContain("Reachable via");
  expect(said).toContain("inside, on gitea, exposed by vm-app-02");
  expect(
    [...target.querySelectorAll("aside.tf-et-pane li[data-wire]")].map((row) => row.getAttribute("data-wire")),
  ).toEqual(["exposes:route:dashboard", "exposes:route:gitea", "via:route:gitea"]);

  // The URL is the *open URL* action, handed to the OS browser.
  buttonNamed("http://127.0.0.1:3000/", target.querySelector("aside.tf-et-pane")!)!.click();
  await waitFor(() => expect(opened).toEqual(["http://127.0.0.1:3000/"]));
});

/**
 * An asset that exposes nothing and that nothing lands on draws neither route
 * heading in the preview: the page says *No routes.*, the preview has room
 * only for what is there.
 */
test("the preview of an asset with no routes draws no route headings", async () => {
  render("#/assets/asset:postgres");
  await waitFor(() => expect(previewed()).toBe("postgres"));

  expect(preview()).not.toContain("Exposes");
  expect(preview()).not.toContain("Reachable via");
});

// -- the wires (#433) --------------------------------------------------------

/**
 * Every wire the view drew, as *which route row it leaves* → *where it lands*,
 * with the dashed ones marked.
 *
 * The geometry is deliberately not read: jsdom lays nothing out, so every
 * rectangle it measures is zero and every wire's path would be the same
 * string. What a browser has to certify — that the curve reaches the row it
 * names — is the headless-Chrome pass; what this asserts is the half a
 * browser photograph cannot: *which* route is wired to *which* asset, and
 * whether the line says "through".
 */
function wires(): string[] {
  return [...target.querySelectorAll("svg.tf-et-wires path.tf-et-wire")].map((wire) => {
    const dashed = wire.classList.contains("tf-et-dashed") ? " dashed" : "";
    return `${wire.getAttribute("data-wire")} → ${wire.getAttribute("data-to")}${dashed}`;
  });
}

/**
 * **A route whose target is drawn in a column gets a solid wire to that row,
 * and a route that lands on nothing gets none** — criterion 1, and its
 * control in the same preview.
 *
 * The VM's own preview is where both ends of the pair are on screen at once:
 * it *exposes* the Gitea route, whose target sits in the column under it, and
 * it is also *reachable via* that same route, because the route lands on
 * something it holds. So one route draws two wires, and the untargeted
 * *Traefik dashboard* draws none.
 */
test("a route to an asset in an open column draws a solid wire to its row", async () => {
  render("#/assets/asset:vm-app-02");

  await waitFor(() =>
    expect(wires()).toEqual([
      // From *Exposes*: to the container the route lands on, one column right.
      "exposes:route:gitea → asset:gitea",
      // From *Reachable via*: to the asset that exposes it, which is this one.
      "via:route:gitea → asset:vm-app-02",
    ]),
  );
});

/**
 * **A wire lands where its far end is drawn** — criterion 2, in the window
 * that draws the column.
 *
 * Dashed, and for a reason of its own — the route reaches this database
 * *through* the container above it, which is story 31's other clause.
 */
test("a route whose exposer is drawn takes its wire to that row", async () => {
  render("#/assets/asset:gitea-db", ESTATE, { stripWidth: 780 });

  await waitFor(() => expect(wires()).toEqual(["via:route:gitea → asset:vm-app-02 dashed"]));
});

/**
 * The same route in a window that cannot hold the column: its exposer is
 * behind a spine now, and the wire lands there.
 */
test("a route whose exposer is behind a spine draws a dashed wire to the spine", async () => {
  render("#/assets/asset:gitea-db", ESTATE, { stripWidth: 470 });

  await waitFor(() => expect(spines()).toEqual(["hel1", "vm-app-02", "gitea"]));
  expect(wires()).toEqual(["via:route:gitea → spine:1 dashed"]);
});

/**
 * **A column change leaves no wire behind** — criterion 3's second half.
 *
 * The wires are derived from the columns on screen rather than kept beside
 * them, so an asset no route reaches draws none the moment its columns are
 * drawn.
 */
test("moving to an asset no route reaches leaves no wire behind", async () => {
  const { nav } = render("#/assets/asset:vm-app-02");
  await waitFor(() => expect(wires()).toHaveLength(2));

  nav.go("#/assets/asset:postgres");
  flushSync();

  await waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", "postgres"]));
  expect(wires()).toEqual([]);
});

/**
 * Lay the tree out for one test: jsdom measures every element as zero, which
 * is the unlaid branch every other wire test runs on. `rows` is where each
 * route row sits, by its wire key, in the window's pixels; the frame and the
 * preview run `tall` pixels down, past the window's bottom edge (#1460).
 */
function layOut(rows: Record<string, number>, tall: number): void {
  const rect = (left: number, top: number, width: number, height: number) =>
    ({ left, top, width, height, right: left + width, bottom: top + height, x: left, y: top, toJSON: () => ({}) }) as DOMRect;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    if (this.matches("div.tf-et-tree")) return rect(0, 0, 1000, tall);
    if (this.matches("aside.tf-et-pane")) return rect(680, 0, 320, tall);
    if (this.matches("div.tf-et-cols")) return rect(0, 0, 680, tall);
    if (this.matches("ol.tf-et-col")) return rect(0, 0, 220, window.innerHeight);
    if (this.matches("button.tf-et-row")) return rect(20, 40, 200, 24);
    const key = this.getAttribute("data-wire");
    if (this.matches("li[data-wire]") && key !== null && rows[key] !== undefined) return rect(690, rows[key], 300, 20);
    return rect(0, 0, 0, 0);
  });
}

/**
 * **A wire starts only from a route row the reader can see** (#1460).
 *
 * The visible part of the preview ends at the window, so the row below it
 * draws nothing and the one inside it still draws.
 */
test("a route row below the window draws no wire, and one inside it does", async () => {
  const below = window.innerHeight + 400;
  layOut({ "exposes:route:gitea": 100, "via:route:gitea": below }, window.innerHeight + 1500);
  render("#/assets/asset:vm-app-02");

  await waitFor(() => expect(wires()).toEqual(["exposes:route:gitea → asset:gitea"]));
});

/**
 * **Scrolling the preview re-measures** (#1460): the preview is its own
 * scroller, and a row scrolled into view gains its wire at once.
 */
test("a scroll on the preview brings a row scrolled into view its wire", async () => {
  const rows = { "exposes:route:gitea": 100, "via:route:gitea": window.innerHeight + 400 };
  layOut(rows, window.innerHeight);
  render("#/assets/asset:vm-app-02");
  await waitFor(() => expect(wires()).toEqual(["exposes:route:gitea → asset:gitea"]));

  rows["via:route:gitea"] = 300;
  expect(wires(), "nothing re-measures before the scroll").toHaveLength(1);
  target.querySelector("aside.tf-et-pane")?.dispatchEvent(new Event("scroll"));
  flushSync();

  expect(wires()).toEqual(["exposes:route:gitea → asset:gitea", "via:route:gitea → asset:vm-app-02"]);
});

/**
 * **The reduced-motion rule switches the wires' fade off and does not hide
 * them** — story 31's *off under reduced motion's no-animation rule (still
 * drawn, not animated)*, as far as a stylesheet can be asked.
 *
 * Read off disk: jsdom applies no media query. Asserted as **one rule**: the
 * wire's selector and `animation: none` in the same block, because two
 * independent substring checks pass on a block that turns some other
 * element's animation off.
 */
test("switches the wires' fade off under reduced motion rather than hiding them", () => {
  const view = readFileSync(join(process.cwd(), "src/lib/assets/AssetsView.svelte"), "utf8");
  const block = /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{[\s\S]*?\n  \}/.exec(view);

  expect(block, "no prefers-reduced-motion block in the tree's stylesheet").not.toBeNull();
  expect(block?.[0]).toMatch(/\.tf-et-wire[^{}]*\{[^}]*animation:\s*none/);
  // Switched off, not hidden: a wire nobody can see is not the same promise.
  for (const hiding of ["display: none", "visibility: hidden", "opacity: 0"]) {
    expect(block?.[0], `the reduced-motion block hides the wires with ${hiding}`).not.toContain(hiding);
  }
});

// -- a long column (#2210) -----------------------------------------------------

/** `n` VMs under the site, named so they sort before `vm-app-02` and push it far down its column. */
const bulkUnderSite = (n: number) =>
  Array.from({ length: n }, (_, at) =>
    row({ id: `asset:bulk-${at}`, parent_id: SITE.id, type_id: "vm", monogram: "VM", name: `bulk-${String(at).padStart(4, "0")}`, has_children: false }),
  );

test("a long column draws the rows near the viewport, and a wire to a row it has not drawn still lands (#2210)", async () => {
  render("#/assets/asset:gitea-db", [...ESTATE, ...bulkUnderSite(2000)], { stripWidth: 780 });

  await waitFor(() => expect(wires()).toEqual(["via:route:gitea → asset:vm-app-02 dashed"]));
  const long = [...target.querySelectorAll("ol.tf-et-col")].find((column) => column.querySelector('[data-asset="asset:bulk-0"]'))!;
  expect(long.querySelectorAll("button.tf-et-row").length).toBeGreaterThan(5);
  expect(long.querySelectorAll("button.tf-et-row").length).toBeLessThan(100);
  expect(long.querySelector('button.tf-et-row[data-asset="asset:vm-app-02"]')).toBeNull();
});

test("a search hit deep in a long column is drawn, picked and focused (#2210)", async () => {
  const { nav, searched } = render("#/assets/tree", [...ESTATE, ...bulkUnderSite(2000)]);
  await waitFor(() => expect(columns()).toEqual([["hel1"]]));

  await type(searched, "bulk-1999");
  press("Enter", box());

  expect(nav.route).toEqual({ view: "assets", tab: "tree", assetId: "asset:bulk-1999" });
  await waitFor(() => expect(marked()).toEqual(["hel1", "bulk-1999"]));
  await waitFor(() => expect(document.activeElement?.getAttribute("data-asset")).toBe("asset:bulk-1999"));
});
