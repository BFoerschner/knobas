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

import { type SearchQuery, type SearchResponse, noFilters } from "../ipc";
import type { AssetDetail, AssetProperty, AssetRow } from "../ipc/assets";
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
      },
    },
  });
  flushSync();
  return { router, asked, searched };
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
    }),
    row({
      id: "asset:amber",
      name: "amber",
      health: "warn",
      inside: "warn",
      problems_inside: 1,
    }),
    row({
      id: "asset:clean",
      name: "clean",
      health: "up",
      inside: "none",
      problems_inside: 0,
    }),
    row({
      id: "asset:worse",
      name: "worse",
      status: "down",
      health: "down",
      inside: "warn",
      problems_inside: 1,
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
