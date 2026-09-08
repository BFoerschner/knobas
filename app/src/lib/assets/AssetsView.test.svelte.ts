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
const SITE = row({
  id: "asset:hel1",
  type_id: "site",
  monogram: "SI",
  name: "hel1",
  // The only asset in this fixture that sets either — so every value the pane
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
 * Two of them, on the *other* VM so that the container's own pane and the
 * proxy's are different panes: one landing on `gitea` and one landing on
 * nothing. The pair is what makes both halves of the pane assertable — a
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
 * found its monitor, and the pane must not say it twice -- once as something
 * watching this asset and once as something still waiting to. `pgbouncer` is
 * the one still waiting.
 */
const NAMED_BY_THE_FILE = ["postgres-check", "pgbouncer"];

/**
 * `assets::ROUTES_REACHABLE`'s rule, as the fake bridge answers it: every
 * route whose target is on this asset's containment path, above it or below.
 *
 * A copy of the rule and not of its answer, for `inForce`'s reason -- the
 * seam under test is the *pane*, and a hard-coded list would draw the right
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
 * under test here is the *pane*, and a fixture that hard-coded "hel1" as the
 * source would still draw the right words if the pane read the wrong field.
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
 * The pane's read for one asset, ancestors walked over the parent field.
 *
 * Takes the estate it is reading, so a test can hand {@link render} an estate
 * of its own and have the *pane* answer from it too — without that, a bespoke
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
 * address and the pane's answer.
 */
function render(hash: string, estate: AssetRow[] = ESTATE, over: Over = {}) {
  const asked: (string | null)[] = [];
  const searched: SearchQuery[] = [];
  const opened: string[] = [];
  location.hash = hash;
  const router = createRouter();
  app = mount(AssetsView, {
    target,
    props: {
      router,
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
        // #505's panel: read on every selection, so a port left out falls
        // through to the real `invoke`. Empty is honest -- no test here is
        // about it.
        dependsOnThis: () => Promise.resolve({ assets: [], routes: [] }),
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
        getRoute: (routeId: string) => {
          const route = ROUTES.find((candidate) => candidate.id === routeId);
          return route === undefined
            ? Promise.reject({ code: "not_found", message: `no route ${routeId}` })
            : Promise.resolve({ route, history: [] });
        },
        // The OS browser, for `assetTypes`' reason above: this file is about
        // the read and presses no route's URL, but a port left out falls
        // through to the real `openExternal` and the first test that ever did
        // would hand the URL to `@tauri-apps/plugin-opener`. Pressing it is
        // `AssetsView.write.test.svelte.ts`' claim.
        openExternal: (url: string) => {
          opened.push(url);
          return Promise.resolve();
        },
      },
    },
  });
  flushSync();
  return { router, asked, searched, opened };
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
 * `crates/knobas-app/tests/search_ipc.rs`, where the same filter runs against
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
      rank: 1 - index / 10,
      snippet: [],
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
 * the Tree **consumed** it.
 *
 * Dispatched on the element under the cursor rather than on `window`: the
 * Tree's handler is on its own section precisely so an unanswered `Escape`
 * bubbles on to the shell's ladder, and a press synthesised at the top of the
 * document would never travel that path.
 */
function press(key: string, on: Element | null = target.querySelector("section.view")): Event {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  on?.dispatchEvent(event);
  flushSync();
  return event;
}

/** The spine columns, left to right, by the asset each is labelled with. */
function spines(): string[] {
  return [...target.querySelectorAll("button.spine .vn")].map((label) =>
    (label.textContent ?? "").trim(),
  );
}

/** The search box, which every test that types has to find. */
function box(): HTMLInputElement {
  const input = target.querySelector<HTMLInputElement>("input.q");
  if (input === null) throw new Error("the Tree has no search box");
  return input;
}

/** Type into the search box and wait the debounce out. */
async function type(text: string): Promise<void> {
  const input = box();
  input.value = text;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  await new Promise((resolve) => setTimeout(resolve, DEBOUNCE_MS + 30));
  flushSync();
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
  // An imported asset's own two lines say what happened to it, and name the
  // estate the file called itself. Every asset in the demo profile carries the
  // first of them and nothing else (#440).
  expect(pane).toContain("Imported from knobas test estate");
  expect(pane).toContain("monitors: kept teamcity (tunnel)");
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

/**
 * **The pane says where an inherited value came from, and the note is a link
 * to the asset that set it** — stories 8, 9 and 10 (#431).
 *
 * Nothing sets an environment or an owner between the container and the site,
 * so both values in the pane are the site's; the note names it, and one click
 * on that note opens the site's own address. A pane that drew the value
 * without its source would pass a `toContain("prod")` and leave the reader
 * with no way to find the place `prod` can be changed.
 */
test("an inherited value names the ancestor it came from and links to it", async () => {
  const { router } = render("#/asset/asset:postgres");
  await vi.waitFor(() => expect(text()).toContain("hel1 / vm-db-01 / postgres"));

  const pane = text();
  expect(pane).toContain("Environment prod inherited from hel1");
  expect(pane).toContain("Owner Björn inherited from hel1");
  // Health is the worst of this asset and everything under it. Here they are
  // the same, so the pane does not repeat the own status.
  expect(pane).toContain("Health warn");
  expect(pane).not.toContain("own status");

  const note = [
    ...target.querySelectorAll<HTMLButtonElement>("aside.pane button.src"),
  ].find((button) => button.textContent?.includes("inherited from hel1"));
  expect(note).toBeDefined();
  note?.click();
  flushSync();
  expect(router.route).toEqual({
    view: "assets",
    tab: "tree",
    assetId: SITE.id,
  });
});

/**
 * **A value set on the asset itself reads *set here* and is not a link.**
 *
 * The negative for the test above, and the one that matters: a link that led
 * back to the asset the reader is already looking at would be a click that
 * does nothing, which is worse than no link at all. The site is where both
 * values are set, so its own pane is the case.
 */
test("a value set on the asset itself says so and offers no link", async () => {
  render("#/asset/asset:hel1");
  await vi.waitFor(() => expect(text()).toContain("Environment prod set here"));

  expect(text()).toContain("Owner Björn set here");
  expect(text()).not.toContain("inherited from");
  expect(target.querySelectorAll("aside.pane button.src")).toHaveLength(0);
  // The site is rated by nobody and still reads `warn`, because the container
  // two levels down is — that is the rollup, in the pane.
  expect(text()).toContain("Health warn own status none");
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
  await vi.waitFor(() => expect(columns()).toEqual([["lonely"]]));

  const button = target.querySelector<HTMLButtonElement>("button.row");
  button?.click();
  flushSync();
  await vi.waitFor(() =>
    expect(text()).toContain("Environment Not set anywhere"),
  );
  expect(text()).toContain("Owner Not set anywhere");
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
 * warning. It is a red row with an *amber* badge, and an implementation
 * colouring the badge from `health` draws it red.
 */
test("a column row badges what is wrong inside it, in the worst tone inside", async () => {
  const estate = [
    row({
      id: "asset:red",
      name: "red",
      health: "down",
      inside: "down",
      problems_inside: 2,
      linked_work: 0,
    }),
    row({
      id: "asset:amber",
      name: "amber",
      health: "warn",
      inside: "warn",
      problems_inside: 1,
      linked_work: 0,
    }),
    row({
      id: "asset:clean",
      name: "clean",
      health: "up",
      inside: "none",
      problems_inside: 0,
      linked_work: 0,
    }),
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
  await vi.waitFor(() =>
    expect(columns()).toEqual([["amber", "clean", "red", "worse"]]),
  );

  const badges = [...target.querySelectorAll("ol.col li")].map((item) => {
    const badge = item.querySelector("span.badge");
    return badge === null
      ? null
      : {
          name: (item.querySelector("span.nm")?.textContent ?? "").trim(),
          count: (badge.textContent ?? "").trim(),
          tone: badge.classList.contains("down") ? "down" : "warn",
          title: badge.getAttribute("title"),
        };
  });
  expect(badges).toEqual([
    { name: "amber", count: "1", tone: "warn", title: "1 problem inside" },
    null,
    { name: "red", count: "2", tone: "down", title: "2 problems inside" },
    { name: "worse", count: "1", tone: "warn", title: "1 problem inside" },
  ]);
});

// -- the keyboard walk, the spines and the search (#430) ---------------------

/**
 * **The arrows and Enter walk the estate, and every step is an address.**
 *
 * Story 29's own sentence, through the rendered view: down the top column,
 * right into what the selection holds, and the pane following each step. The
 * assertion is on the address as well as on the markup, because the walk
 * moves the *selection* and the columns are derived from it — a walk with a
 * cursor of its own would draw the same thing here and disagree with the
 * address bar.
 */
test("the arrows and enter walk three levels, and the pane follows", async () => {
  const { router } = render("#/assets/tree");
  await vi.waitFor(() => expect(columns()).toEqual([["hel1"]]));

  press("ArrowDown");
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: SITE.id });
  await vi.waitFor(() => expect(marked()).toEqual(["hel1", null]));

  press("ArrowRight");
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: SIBLING.id });
  await vi.waitFor(() => expect(marked()).toEqual(["hel1", "vm-app-02", null]));

  press("ArrowDown");
  await vi.waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", null]));

  press("Enter");
  await vi.waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", "postgres"]));
  expect(text()).toContain("hel1 / vm-db-01 / postgres");
  // The container holds nothing: `Enter` on it opens no column and moves
  // nothing, rather than selecting out of an empty one.
  press("Enter");
  await vi.waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", "postgres"]));
});

/**
 * **The focused row is the selected one**, which is what makes the walk
 * visible: the global `:focus-visible` ring in `app.css` is drawn on whatever
 * has focus, and a walk that moved the selection without moving focus would
 * leave the ring on the row the reader started from.
 */
test("the walk carries focus to the row it selected", async () => {
  render("#/assets/tree");
  await vi.waitFor(() => expect(columns()).toEqual([["hel1"]]));

  press("ArrowDown");
  await vi.waitFor(() => expect(marked()).toEqual(["hel1", null]));

  const on = target.querySelector('button.row[aria-current="true"]');
  expect(on?.textContent).toContain("hel1");
  await vi.waitFor(() => expect(document.activeElement).toBe(on));
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
  await vi.waitFor(() => expect(columns()).toEqual([["hel1"]]));
  press("ArrowDown");
  await vi.waitFor(() => expect(scrolled).not.toHaveLength(0));
  expect(scrolled.at(-1)?.behavior).toBe("auto");

  reduce.mockImplementation((query: string) => ({ matches: false, media: query }) as MediaQueryList);
  // The same press again, now without the preference — the option is read per
  // press and not once at mount, which is what a reader who changes the
  // system setting while the app is open would expect.
  press("ArrowRight");
  await vi.waitFor(() => expect(scrolled.at(-1)?.behavior).toBe("smooth"));
});

/**
 * **Escape unwinds one step per press, and the press it has no step for is
 * handed on.**
 *
 * The rung above the top of the estate is the bare tree; above that the Tree
 * has nothing to unwind, and the key belongs to the shell's ladder
 * (`shell/keys.ts` rung 3, *back to the room*). A view that consumed it
 * anyway would be the one surface a reader cannot leave with the keyboard —
 * so the last press is asserted **not** to be consumed.
 */
test("escape unwinds the path one level at a time and then falls through", async () => {
  const { router } = render("#/asset/asset:postgres");
  await vi.waitFor(() => expect(columns()).toHaveLength(3));

  expect(press("Escape").defaultPrevented).toBe(true);
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: VM.id });
  // Waited out between presses, and that is a statement about the view rather
  // than about the test: the layout is derived from the pane's answer, so the
  // rung a press lands on is the one the *drawn* path offers.
  await vi.waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", null]));

  press("Escape");
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: SITE.id });
  await vi.waitFor(() => expect(marked()).toEqual(["hel1", null]));

  press("Escape");
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: null });
  await vi.waitFor(() => expect(marked()).toEqual([null]));

  const last = press("Escape");
  expect(last.defaultPrevented).toBe(false);
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: null });
});

/**
 * **Four levels in a window that holds three: the oldest column becomes a
 * spine labelled by its asset, and clicking the spine re-expands it.**
 *
 * Story 28, and the collapse is width-driven — 780px is what knobas' narrowest
 * allowed window (`minWidth: 1100`) leaves beside the fixed pane. The label is
 * the asset the reader walked *through*, not the column's parent: a spine
 * reading "top level" would tell them nothing about the path they are in.
 *
 * Clicking it anchors the strip there, which pushes the collapse to the other
 * end — the deepest column becomes the spine and the site is a full column
 * again. The pane does not move, which is the whole point of the collapse.
 */
test("a path too deep for the window collapses its oldest columns to spines", async () => {
  render("#/asset/asset:gitea-db", ESTATE, { stripWidth: 780 });

  // Four levels, three of them drawn: the site is a spine and the columns the
  // reader just walked into are the ones that keep their width.
  await vi.waitFor(() => expect(spines()).toEqual(["hel1"]));
  expect(columns()).toEqual([["vm-app-02", "vm-db-01"], ["gitea"], ["gitea-db"]]);
  expect(marked()).toEqual(["vm-app-02", "gitea", "gitea-db"]);
  expect(text()).toContain("hel1 / vm-app-02 / gitea / gitea-db");

  target.querySelector<HTMLButtonElement>("button.spine")?.click();
  flushSync();

  // Anchored at the site, the collapse moves to the other end: the deepest
  // column is the spine now, and it is labelled with the asset the reader has
  // selected in it.
  await vi.waitFor(() => expect(spines()).toEqual(["gitea-db"]));
  expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["gitea"]]);
  expect(text()).toContain("hel1 / vm-app-02 / gitea / gitea-db");
});

/** A window with room for every column draws no spine at all. */
test("a path the window holds collapses nothing", async () => {
  render("#/asset/asset:gitea-db", ESTATE, { stripWidth: 1400 });

  await vi.waitFor(() => expect(columns()).toHaveLength(4));
  expect(spines()).toEqual([]);
});

/**
 * **Typing a name in the Tree's search opens the path to the first match and
 * selects it** — criterion 3, and story 30.
 *
 * The reveal is an *address*: the box hands the first match's id to the
 * router, the pane reads it, and the columns are derived from the `held_by`
 * that comes back. So the box needs to know nothing about where an asset
 * lives, and a match four levels down opens four columns without the reader
 * having walked one of them.
 */
test("typing a name opens the path to the first match and selects it", async () => {
  const { router, searched } = render("#/assets/tree");
  await vi.waitFor(() => expect(columns()).toEqual([["hel1"]]));

  await type("gitea-db");
  // What the box asked for, which is the estate's corpus and nothing else.
  expect(searched.at(-1)?.filters.kinds).toEqual(["asset"]);
  expect(searched.at(-1)?.raw).toBe("gitea-db");
  // The list says where each match lives before the reader takes it.
  expect(text()).toContain("hel1 / vm-app-02 / gitea");

  press("Enter", box());
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: GITEA_DB.id });
  await vi.waitFor(() =>
    expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["gitea"], ["gitea-db"]]),
  );
  expect(marked()).toEqual(["hel1", "vm-app-02", "gitea", "gitea-db"]);
  expect(text()).toContain("hel1 / vm-app-02 / gitea / gitea-db");
});

/**
 * **One keystroke is not one query**, and a box with nothing in it asks
 * nothing at all: the debounce is the launcher's own, so the two boxes in this
 * app wait the same amount of time.
 */
test("the box waits for the typing to stop, and an empty box asks nothing", async () => {
  const { searched } = render("#/assets/tree");
  await vi.waitFor(() => expect(columns()).toEqual([["hel1"]]));

  const input = box();
  for (const text of ["g", "gi", "git"]) {
    input.value = text;
    input.dispatchEvent(new Event("input", { bubbles: true }));
    flushSync();
  }
  await new Promise((resolve) => setTimeout(resolve, DEBOUNCE_MS + 30));
  expect(searched.map((query) => query.raw)).toEqual(["git"]);

  await type("");
  expect(searched.map((query) => query.raw)).toEqual(["git"]);
});

/**
 * **A search that matches nothing says so**, and the columns are left alone.
 *
 * The box is a way into the estate, not a filter over it: a query with no
 * match must not blank the path the reader is standing in.
 */
test("a query nothing matches leaves the columns where they were", async () => {
  render("#/asset/asset:postgres");
  await vi.waitFor(() => expect(columns()).toHaveLength(3));

  await type("nothing-is-called-this");

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
  const { router } = render("#/asset/asset:postgres");
  await vi.waitFor(() => expect(columns()).toHaveLength(3));

  await type("gitea");
  expect(text()).toContain("gitea-db");

  press("Escape", box());
  expect(box().value).toBe("");
  expect(text()).not.toContain("gitea-db");
  // The selection is untouched: clearing the box is not unwinding the path.
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: CONTAINER.id });
});

/**
 * **The arrows walk the offered matches while the box has the focus**, and
 * the columns underneath do not move: two lists and one keyboard, so the one
 * in front of the reader is the one that answers.
 */
test("the arrows move through the matches instead of the columns", async () => {
  const { router } = render("#/assets/tree");
  await vi.waitFor(() => expect(columns()).toEqual([["hel1"]]));

  await type("vm-");
  press("ArrowDown", box());
  press("Enter", box());

  // `vm-db-01` is the first match and `vm-app-02` the second: the press moved
  // the cursor down the offers, and `Enter` took the one it had moved to.
  expect(router.route).toEqual({ view: "assets", tab: "tree", assetId: SIBLING.id });
});

/**
 * **A re-expanded spine is a look at one path, and the next selection is
 * another.**
 *
 * So the anchor is kept beside the selection it was clicked in and does not
 * apply to a different one: carried, it would collapse the columns the reader
 * had just walked into, at a level they had never clicked a spine on.
 *
 * The move is to `gitea`, which is one level *up* and still four columns wide
 * — it holds `gitea-db`, so a column opens under it. That is what makes the
 * assertion able to fail: a surviving anchor of 0 would draw the deepest
 * column as the spine instead of the oldest, and the two are told apart by
 * which asset the spine is labelled with.
 */
test("clicking a spine anchors this path and not the next one", async () => {
  const { router } = render("#/asset/asset:gitea-db", ESTATE, { stripWidth: 780 });
  await vi.waitFor(() => expect(spines()).toEqual(["hel1"]));

  target.querySelector<HTMLButtonElement>("button.spine")?.click();
  flushSync();
  await vi.waitFor(() => expect(spines()).toEqual(["gitea-db"]));

  router.go(`#/asset/${GITEA.id}`);
  flushSync();

  await vi.waitFor(() => expect(marked()).toEqual(["vm-app-02", "gitea", null]));
  expect(spines()).toEqual(["hel1"]);
  expect(columns()).toEqual([["vm-app-02", "vm-db-01"], ["gitea"], ["gitea-db"]]);
});

/**
 * **Both ends of a route, in the pane of the asset each end is about** (#432).
 *
 * The exposing VM lists what it offers; the container the route lands on lists
 * how it is reached, with the exposing asset named. The untargeted route is
 * the control in the same fixture: it is under *Exposes* and reaches nobody,
 * so a pane that put every route in both lists fails here.
 */
test("the pane draws what an asset exposes and what reaches it", async () => {
  render("#/asset/asset:vm-app-02");
  await vi.waitFor(() => expect(text()).toContain("Exposes"));

  const exposing = text();
  expect(exposing).toContain("Gitea");
  expect(exposing).toContain("http://127.0.0.1:3000/");
  // A public route says so; an internal one is the unmarked default.
  expect(exposing).toContain("public");
  expect(exposing).toContain("Traefik dashboard");
  expect(exposing).toContain("lands on nothing knobas knows");
  // A route's properties are the reader's own and are drawn as they are.
  expect(exposing).toContain("opened_by compose");
  // The VM holds `gitea`, so the route landing on it also reaches the VM --
  // and the pane says which end that is.
  expect(exposing).toContain("inside, on gitea");

});

/**
 * The other end of the same pair, in a pane of its own so that what is on
 * screen is one asset's: the container the route lands on says *lands here*
 * and names who offers it, and the untargeted route — which reaches nobody —
 * is not on this pane at all.
 */
test("the pane of the asset a route lands on names the end it came from", async () => {
  render("#/asset/asset:gitea");
  await vi.waitFor(() => expect(text()).toContain("Reachable via"));

  const landing = text();
  expect(landing).toContain("lands here");
  expect(landing).toContain("exposed by vm-app-02");
  expect(landing).toContain("Exposes No routes.");
  // The endpoint lands on nothing, so it reaches nobody and is nowhere here.
  expect(landing).not.toContain("Traefik dashboard");
});

/**
 * **A route's address opens the Tree at the asset exposing it, with the route
 * selected** — this ticket's third criterion, and story 14's point: a ticket
 * about a certificate links to the route whose certificate it is, and the link
 * has to land somewhere that says what the route belongs to.
 *
 * The address carries no asset, so the columns below are the proof that the
 * view resolved one: `#/route/gitea` draws `hel1 / vm-app-02` from nothing but
 * the route's own read.
 */
test("a route's address opens the Tree at the asset that exposes it", async () => {
  const { router, asked } = render("#/route/route:gitea");

  // Three columns: the estate, what the site holds, and what the *exposing*
  // asset holds — the layout `#/asset/asset:vm-app-02` draws, from an address
  // that named no asset at all.
  await vi.waitFor(() =>
    expect(columns()).toEqual([["hel1"], ["vm-app-02", "vm-db-01"], ["gitea"]]),
  );
  expect(marked()).toEqual(["hel1", "vm-app-02", null]);
  expect(asked).toContain(SITE.id);
  // The address is kept as the route's: a reader who copied this link must be
  // able to hand it on, and the pane marks the route it names.
  expect(router.route).toEqual({
    view: "assets",
    tab: "tree",
    assetId: null,
    routeId: "route:gitea",
  });
  // Marked in **both** lists, and that is the fixture rather than a bug: the
  // VM exposes this route *and* is reached by it, because it lands on the
  // container the VM holds. What the mark means is "this is the route the
  // address names", and it names one route.
  const selected = [...target.querySelectorAll('li[aria-current="true"]')];
  expect(selected).toHaveLength(2);
  for (const row of selected) expect(row.textContent).toContain("Gitea");
  expect(target.querySelectorAll("li[aria-current]")).toHaveLength(2);
});

/**
 * Clicking a route in *reachable via* goes to the exposing asset, not to the
 * one being read — which is the same rule the address above follows, reached
 * by a click instead of a link.
 */
test("clicking a route reached from elsewhere opens the asset exposing it", async () => {
  const { router } = render("#/asset/asset:gitea");
  await vi.waitFor(() => expect(text()).toContain("Reachable via"));

  const route = [...target.querySelectorAll<HTMLButtonElement>("li button.link")].find(
    (button) => button.textContent?.trim() === "Gitea",
  );
  route?.click();
  flushSync();

  expect(router.route).toEqual({
    view: "assets",
    tab: "tree",
    assetId: null,
    routeId: "route:gitea",
  });
  await vi.waitFor(() => expect(marked()).toEqual(["hel1", "vm-app-02", null]));
});

// -- the wires (#433) --------------------------------------------------------

/**
 * Every wire the view drew, as *which route row it leaves* → *where it lands*,
 * with the dashed ones marked.
 *
 * The geometry is deliberately not read: jsdom lays nothing out, so every
 * rectangle it measures is zero and every wire's path would be the same
 * string. What a browser has to certify — that the curve reaches the row it
 * names — is the headless-Chrome pass in the PR; what this asserts is the
 * half a browser photograph cannot: *which* route is wired to *which* asset,
 * and whether the line says "through".
 */
function wires(): string[] {
  return [...target.querySelectorAll("svg.wires path.wire")].map((wire) => {
    const dashed = wire.classList.contains("dashed") ? " dashed" : "";
    return `${wire.getAttribute("data-wire")} → ${wire.getAttribute("data-to")}${dashed}`;
  });
}

/**
 * **A route whose target is drawn in a column gets a solid wire to that row,
 * and a route that lands on nothing gets none** — criterion 1, and its
 * control in the same pane.
 *
 * The VM's own pane is where both ends of the pair are on screen at once: it
 * *exposes* the Gitea route, whose target sits in the column under it, and it
 * is also *reachable via* that same route, because the route lands on
 * something it holds. So one route draws two wires — from each of the two rows
 * it has in this pane — and the untargeted *Traefik dashboard* draws none.
 */
test("a route to an asset in an open column draws a solid wire to its row", async () => {
  render("#/asset/asset:vm-app-02");

  await vi.waitFor(() =>
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
 * that draws the column, and then in the one that does not.
 *
 * Four levels, and the *route* is the same in both: at 780px the exposing VM's
 * column is drawn and the wire lands on its row; at 470px only the deepest
 * column fits and the same wire lands on the spine standing in for it.
 *
 * Dashed in both, and here for a reason of its own — the route reaches this
 * database *through* the container above it, which is story 31's other clause.
 * The pure test in `tree.test.ts` is where a spine landing is dashed on the
 * spine's account alone.
 */
test("a route whose exposer is drawn takes its wire to that row", async () => {
  render("#/asset/asset:gitea-db", ESTATE, { stripWidth: 780 });

  await vi.waitFor(() => expect(wires()).toEqual(["via:route:gitea → asset:vm-app-02 dashed"]));
});

/**
 * The same route in a window that cannot hold the column: its exposer is
 * behind a spine now, and the wire lands there. One test each rather than one
 * test that unmounts and re-renders — the width is a prop, and a second
 * mounting inside one test would be the harness's `afterEach` written out by
 * hand.
 */
test("a route whose exposer is behind a spine draws a dashed wire to the spine", async () => {
  render("#/asset/asset:gitea-db", ESTATE, { stripWidth: 470 });

  await vi.waitFor(() => expect(spines()).toEqual(["hel1", "vm-app-02", "gitea"]));
  expect(wires()).toEqual(["via:route:gitea → spine:1 dashed"]);
});

/**
 * **A column change leaves no wire behind** — criterion 3's second half.
 *
 * The wires are derived from the columns on screen rather than kept beside
 * them, so an asset no route reaches draws none the moment its columns are
 * drawn. A view that patched an SVG as it went would leave the VM's two lines
 * hanging over the new path, pointing at rows that are no longer there.
 */
test("moving to an asset no route reaches leaves no wire behind", async () => {
  const { router } = render("#/asset/asset:vm-app-02");
  await vi.waitFor(() => expect(wires()).toHaveLength(2));

  router.go("#/asset/asset:postgres");
  flushSync();

  await vi.waitFor(() => expect(marked()).toEqual(["hel1", "vm-db-01", "postgres"]));
  expect(wires()).toEqual([]);
});

/**
 * **The reduced-motion rule switches the wires' fade off and does not hide
 * them** — story 31's *off under reduced motion's no-animation rule (still
 * drawn, not animated)*, as far as a stylesheet can be asked.
 *
 * The rule is a stylesheet's, so it is read off disk the way `app-css.test.ts`
 * reads the shell's and `tree.test.ts` reads the column widths: jsdom applies
 * no media query, so *whether a browser then draws the wire* is the browser
 * pass's claim and not this one. What this asserts is the regression with the
 * least visible symptom — nothing looks broken, the setting is simply
 * ignored — and it asserts it as **one rule**: `.wires .wire` and
 * `animation: none` in the same block, because two independent substring
 * checks pass on a block that turns some other element's animation off.
 */
test("switches the wires' fade off under reduced motion rather than hiding them", () => {
  const view = readFileSync(join(process.cwd(), "src/lib/assets/AssetsView.svelte"), "utf8");
  const block = /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{[\s\S]*?\n  \}/.exec(view);

  expect(block, "no prefers-reduced-motion block in the Tree's stylesheet").not.toBeNull();
  expect(block?.[0]).toMatch(/\.wires \.wire[^{}]*\{[^}]*animation:\s*none/);
  // Switched off, not hidden: a wire nobody can see is not the same promise,
  // and there is more than one way to make one invisible.
  for (const hiding of ["display: none", "visibility: hidden", "opacity: 0"]) {
    expect(block?.[0], `the reduced-motion block hides the wires with ${hiding}`).not.toContain(
      hiding,
    );
  }
});

/**
 * **The pane's monitoring section** (issue #445, spec #427 stories 33 and 71).
 *
 * What the reader is owed for each monitor watching this asset: its name, the
 * state Kuma last published, and one click to its own page in Kuma. The
 * paused one is the row that makes the section honest — it is still attached,
 * it has no state and no page, and it says so rather than rendering as a blank
 * beside a live one. The third row separates the two halves of that: a monitor
 * the mirror still holds, with a page to open and no state word, is not the
 * same fact as one that has left it, and one row cannot witness both branches.
 */
test("the pane lists the monitors watching the asset, with their state", async () => {
  render(`#/asset/${CONTAINER.id}`);
  await vi.waitFor(() => expect(target.querySelector(".watch")).not.toBeNull());

  const section = target.querySelector(".watch");
  const rows = [...section!.querySelectorAll("li")].map((row) =>
    (row.textContent ?? "").replace(/\s+/g, " ").trim(),
  );
  expect(rows).toEqual([
    "postgres-check down Open in Kuma",
    "postgres-slow-query Paused or gone from Kuma",
    "postgres-wal no reading Open in Kuma",
  ]);
});

/**
 * Story 71: *a deep link from a monitor to its page in Uptime Kuma, so that
 * what knobas does not do is one click away.*
 *
 * The URL handed over is the **mirror's own** `web_url`, which is the adapter's
 * — nothing here builds one out of a base URL and an id.
 */
test("Open in Kuma hands the monitor's own URL to the browser", async () => {
  const { opened } = render(`#/asset/${CONTAINER.id}`);
  await vi.waitFor(() => expect(target.querySelector(".watch")).not.toBeNull());

  const open = [...target.querySelectorAll<HTMLButtonElement>(".watch button")].find(
    (candidate) => candidate.textContent?.trim() === "Open in Kuma",
  );
  expect(open).toBeDefined();
  open!.click();
  await vi.waitFor(() => expect(opened).toHaveLength(1));

  expect(opened).toEqual(["http://127.0.0.1:3001/dashboard/4"]);
});

/**
 * The two halves of one sentence: what the file *said* and what has been
 * *found*. A name that found its monitor is in the section above and must not
 * be repeated below it — the list underneath is the queue, not the roster.
 */
test("only the names still waiting for a monitor are listed as named by the import", async () => {
  render(`#/asset/${CONTAINER.id}`);
  await vi.waitFor(() => expect(target.querySelector(".named")).not.toBeNull());

  const waiting = target.querySelector(".named");
  expect(
    [...waiting!.querySelectorAll("li")].map((row) => (row.textContent ?? "").trim()),
  ).toEqual(["pgbouncer"]);
});

/**
 * An asset nothing watches draws neither section. A *Monitoring* heading with
 * "nothing" under it on every hand-made asset would be a sentence about a file
 * nobody imported — the reason #439 drew the names conditionally, kept.
 */
test("an asset with no monitors draws no monitoring section at all", async () => {
  render(`#/asset/${VM.id}`);
  await vi.waitFor(() => expect(text()).toContain("vm-db-01"));

  expect(target.querySelector(".watch")).toBeNull();
  expect(target.querySelector(".named")).toBeNull();
  expect(text()).not.toContain("Monitoring");
});
