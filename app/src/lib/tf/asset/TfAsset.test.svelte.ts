/**
 * An asset's own page: what a reader sees at `#/asset/<id>` and `#/route/<id>`,
 * and what it asked for to draw it.
 *
 * The page is what the asset tree's pane was before the two were split: the
 * tree finds and arranges (`assets/AssetsView.test.svelte.ts`), and the page
 * reads one asset whole. The seam is the one the tree's tests use, **a
 * rendered view in, user-visible text and the reads it issued out**, through
 * `Estate.fixture.svelte`, which draws the tree or the page from one test
 * navigation the way the shell does, so a link from the page into the tree
 * (or back) is followed rather than asserted as a string.
 *
 * The fixture is the tree tests' corner of the real estate, copied rather
 * than shared: each file is about its own surface, and a shared module would
 * make a change for one of them a change for both.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AssetDetail, AssetProperty, AssetRow, AttachedMonitor, RouteRow } from "../../ipc/assets";
import { createTestNav } from "../../assets/test-nav.svelte";
import Estate from "../../assets/Estate.fixture.svelte";
import { waitFor } from "../../shell/test-wait";

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
  // The only asset in this fixture that sets either — so every value the page
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
 * Two of them, on the *other* VM so that the container's own page and the
 * proxy's are different pages: one landing on `gitea` and one landing on
 * nothing. The pair is what makes both halves of the page assertable — a
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
 * found its monitor, and the page must not say it twice -- once as something
 * watching this asset and once as something still waiting to. `pgbouncer` is
 * the one still waiting.
 */
const NAMED_BY_THE_FILE = ["postgres-check", "pgbouncer"];

/**
 * `assets::ROUTES_REACHABLE`'s rule, as the fake bridge answers it: every
 * route whose target is on this asset's containment path, above it or below.
 *
 * A copy of the rule and not of its answer, for `inForce`'s reason -- the
 * seam under test is the *page*, and a hard-coded list would draw the right
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
 * under test here is the *page*, and a fixture that hard-coded "hel1" as the
 * source would still draw the right words if the page read the wrong field.
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
 * The page's read for one asset, ancestors walked over the parent field.
 *
 * Takes the estate it is reading, so a test can hand {@link render} an estate
 * of its own and have the *page* answer from it too — without that, a bespoke
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
            // Declared by the type and filled in by nobody: the page says
            // *Not set*, never the word "null", and it is a button to fill in.
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
 * Mount the estate at `hash` and record the reads it issues.
 *
 * `asked` is the tree's read, which an asset's page must not issue: the page
 * draws one asset from its own read, and a page that walked the columns would
 * be the tree again.
 */
function render(hash: string, estate: AssetRow[] = ESTATE) {
  const asked: (string | null)[] = [];
  const opened: string[] = [];
  const read: string[] = [];
  location.hash = hash;
  const nav = createTestNav();
  app = mount(Estate, {
    target,
    props: {
      nav,
      now: () => NOW,
      ports: {
        // The type table, read on mount for *New asset inside*. Empty is the
        // honest fixture: no test here creates.
        assetTypes: () => Promise.resolve([]),
        // #505's panel, read beside the asset. Empty: its tests are
        // `TfAsset.depends.test.svelte.ts`'.
        dependsOnThis: () => Promise.resolve({ assets: [], routes: [] }),
        assetTree: (parentId?: string | null) => {
          asked.push(parentId ?? null);
          return Promise.resolve(estate.filter((candidate) => candidate.parent_id === (parentId ?? null)));
        },
        getAsset: (assetId: string) => {
          read.push(assetId);
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
            : Promise.resolve({ route, history: [], monitor_targets: [] });
        },
        // The OS browser: a route's URL and a monitor's page in Kuma are
        // buttons, and a port left out would hand the URL to the real opener.
        openExternal: (url: string) => {
          opened.push(url);
          return Promise.resolve();
        },
      },
    },
  });
  flushSync();
  return { nav, asked, opened, read };
}

function text(): string {
  return (target.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** One of the page's panels, by the name it gives itself. */
function panel(name: string): Element {
  const found = target.querySelector(`section.tf-panel[aria-label="${name}"]`);
  if (found === null) throw new Error(`the page draws no ${name} panel`);
  return found;
}

/** A panel's text, as one line. */
function said(name: string): string {
  return (panel(name).textContent ?? "").replace(/\s+/g, " ").trim();
}

/** The asset the page is about, by its title. */
function title(): string | null {
  const heading = target.querySelector(".tf-as-head h1");
  return heading === null ? null : (heading.textContent ?? "").trim();
}

/** A button by its trimmed text, anywhere in `within`. */
function buttonNamed(label: string, within: ParentNode = target): HTMLButtonElement | undefined {
  return [...within.querySelectorAll<HTMLButtonElement>("button")].find(
    (button) => (button.textContent ?? "").trim() === label,
  );
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
  vi.restoreAllMocks();
});

/**
 * **An asset's address, opened cold, draws the asset: what it is, where it
 * sits, what is stored on it and what happened to it**, and none of the
 * tree.
 *
 * The page reads the asset and nothing of the columns: where it sits comes
 * back with the asset (`held_by`), drawn as the header's line with each step a
 * link to that asset's page, and the breadcrumb's *Asset tree* goes to this
 * asset picked in the tree. The type's own properties come first, a declared
 * one nobody filled in says *Not set*, and the rest are under *Other
 * properties*.
 */
test("an asset's address draws its page, and reads nothing of the tree", async () => {
  const { asked, read } = render("#/asset/asset:postgres");
  await waitFor(() => expect(title()).toBe("postgres"));

  expect(asked).toEqual([]);
  expect(read).toEqual([CONTAINER.id]);
  expect(target.querySelector("ol.tf-et-col")).toBeNull();

  // The separators are spaced by the stylesheet, not by the text: read as one.
  const sub = (target.querySelector(".tf-as-head .tf-sub")?.textContent ?? "")
    .replace(/\s*›\s*/g, " › ")
    .replace(/\s+/g, " ")
    .trim();
  expect(sub).toBe("Container in hel1 › vm-db-01");
  expect([...target.querySelectorAll(".tf-as-head .tf-sub a")].map((link) => link.getAttribute("href"))).toEqual([
    "#/asset/asset:hel1",
    "#/asset/asset:vm-db-01",
  ]);
  const crumbs = [...target.querySelectorAll(".tf-crumbs a, .tf-crumbs strong")].map((part) => [
    (part.textContent ?? "").trim(),
    part.getAttribute("href"),
  ]);
  expect(crumbs).toEqual([
    ["Dependencies", "#/deps"],
    ["Asset tree", "#/assets/asset:postgres"],
    ["postgres", null],
  ]);

  const properties = said("Properties");
  expect(properties).toContain("1 of 2 set");
  expect(properties).toContain("Image postgres:18");
  expect(properties).toContain("Ports Not set");
  expect(properties).toContain("Other properties zzz-note typed on this one");
  expect(said("Holds")).toContain("Nothing inside it.");

  const history = said("History");
  // A property edit's line reads as the change it was, old value included.
  expect(history).toContain("os: Debian 12 → Debian 13");
  expect(history).toContain("Created");
  // An imported asset's own two lines say what happened to it, and name the
  // estate the file called itself (#440).
  expect(history).toContain("Imported from knobas test estate");
  expect(history).toContain("monitors: kept teamcity (tunnel)");
});

/**
 * **The header's actions are the page's own**: *Edit* first as the primary,
 * then *Move…*, *Link to…* and *Delete…*, and *Show in tree* last as the
 * ghost, the link back to where the asset sits among the rest.
 */
test("the header offers the page's actions, and Show in tree goes to the asset's pick", async () => {
  render("#/asset/asset:postgres");
  await waitFor(() => expect(title()).toBe("postgres"));

  const acts = [...target.querySelectorAll(".tf-as-head .tf-acts > *")].map((control) => [
    (control.textContent ?? "").trim(),
    [...control.classList].filter((name) => name !== "tf-btn").join(" "),
  ]);
  expect(acts).toEqual([
    ["Edit", "tf-primary"],
    ["Move…", ""],
    ["Link to…", ""],
    ["Delete…", "tf-danger"],
    ["Show in tree", "tf-ghost"],
  ]);
  expect(target.querySelector(".tf-as-head .tf-acts a")?.getAttribute("href")).toBe("#/assets/asset:postgres");
});

/**
 * **An address naming an asset that is not there draws the failure, and not
 * the last asset.**
 *
 * The deep-link case a notification produces: a page still showing the last
 * asset would be the view lying about what the address names.
 */
test("a deep link to an asset that is gone says so instead of showing the last one", async () => {
  const { nav } = render("#/asset/asset:postgres");
  await waitFor(() => expect(title()).toBe("postgres"));

  nav.go("#/asset/asset:nobody");
  flushSync();

  await waitFor(() => expect(text()).toContain("no asset asset:nobody"));
  expect(title()).toBeNull();
});

/**
 * **The page says where an inherited value came from, and the note is a link
 * to the page of the asset that set it** — stories 8, 9 and 10 (#431).
 *
 * Nothing sets an environment or an owner between the container and the site,
 * so both values are the site's; the note names it, and one click on it opens
 * the site's own page, which is where `prod` can be changed. Health is the
 * worst of this asset and everything under it; here the two agree, so the
 * own status is not repeated.
 */
test("an inherited value names the ancestor it came from and links to its page", async () => {
  const { nav } = render("#/asset/asset:postgres");
  await waitFor(() => expect(title()).toBe("postgres"));

  const force = said("In force");
  expect(force).toContain("Environment prod inherited from hel1");
  expect(force).toContain("Owner Björn inherited from hel1");
  expect(force).toContain("Health warning");
  expect(force).not.toContain("own status");

  const note = [...panel("In force").querySelectorAll<HTMLButtonElement>("button.tf-as-from")].find((button) =>
    button.textContent?.includes("inherited from hel1"),
  );
  expect(note).toBeDefined();
  note?.click();
  flushSync();
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: SITE.id });
  await waitFor(() => expect(title()).toBe("hel1"));
});

/**
 * **A value set on the asset itself reads *set here* and is not a link.**
 *
 * The negative for the test above: a link back to the page the reader is on
 * would be a click that does nothing. The site sets both values. It is rated
 * by nobody and still reads *warning*, because the container two levels down
 * is: that is the rollup, said beside the own status it differs from.
 */
test("a value set on the asset itself says so and offers no link", async () => {
  render("#/asset/asset:hel1");
  await waitFor(() => expect(said("In force")).toContain("Environment prod set here"));

  const force = said("In force");
  expect(force).toContain("Owner Björn set here");
  expect(force).not.toContain("inherited from");
  expect(panel("In force").querySelectorAll("button.tf-as-from")).toHaveLength(0);
  expect(force).toContain("Health warning own status no reading");
});

/** An asset with nothing set at or above it says so, rather than defaulting: "not set" and `dev` are different facts. */
test("an asset with no environment anywhere above it says nothing is set", async () => {
  const bare = row({ id: "asset:lonely", name: "lonely", has_children: false });
  render("#/asset/asset:lonely", [bare]);
  await waitFor(() => expect(title()).toBe("lonely"));

  expect(said("In force")).toContain("Environment Not set anywhere");
  expect(said("In force")).toContain("Owner Not set anywhere");
  expect((target.querySelector(".tf-as-head .tf-sub")?.textContent ?? "").replace(/\s+/g, " ").trim()).toBe(
    "VM at the top of the estate",
  );
});

/**
 * **What it holds is a list of pages**: each name opens that asset's own page,
 * and the count is on the heading.
 */
test("the Holds panel lists what the asset holds, each a link to its page", async () => {
  const { nav } = render("#/asset/asset:hel1");
  await waitFor(() => expect(title()).toBe("hel1"));

  const holds = panel("Holds");
  expect(holds.querySelector("header")?.textContent?.replace(/\s+/g, " ").trim()).toContain("Holds 2");
  expect([...holds.querySelectorAll("li .tf-as-to")].map((name) => name.textContent?.trim())).toEqual([
    "vm-db-01",
    "vm-app-02",
  ]);
  buttonNamed("vm-app-02", holds)!.click();
  flushSync();
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: SIBLING.id });
  await waitFor(() => expect(title()).toBe("vm-app-02"));
});

/** `n` VMs under the site, for a page that holds thousands. */
const bulkUnderSite = (n: number) =>
  Array.from({ length: n }, (_, at) =>
    row({ id: `asset:bulk-${at}`, parent_id: SITE.id, type_id: "vm", monogram: "VM", name: `bulk-${String(at).padStart(4, "0")}`, has_children: false }),
  );

/** A page that holds thousands draws the *Holds* lines near the viewport, and counts them all (#2210). */
test("a page that holds thousands draws the Holds lines near the viewport (#2210)", async () => {
  render("#/asset/asset:hel1", [...ESTATE, ...bulkUnderSite(2000)]);

  await waitFor(() => expect(said("Holds")).toContain("bulk-0000"));
  const holds = panel("Holds");
  expect(holds.querySelector("header")?.textContent).toContain("2002");
  expect(holds.querySelectorAll("li:not(.tf-wgap)").length).toBeGreaterThan(5);
  expect(holds.querySelectorAll("li:not(.tf-wgap)").length).toBeLessThan(100);
});

/**
 * **Both ends of a route, on the page of the asset each end is about** (#432).
 *
 * The exposing VM lists what it offers; the container the route lands on is
 * how it is reached. The untargeted route is the control in the same fixture:
 * it is under *Exposes* and reaches nobody, so a page that put every route in
 * both lists fails here. A route's own properties are drawn as they are, a
 * public one says so, and the asset at the far end is a link to its page.
 */
test("the page draws what an asset exposes and what reaches it", async () => {
  const { nav, opened } = render("#/asset/asset:vm-app-02");
  await waitFor(() => expect(said("Routes")).toContain("Exposes"));

  const routes = said("Routes");
  expect(routes).toContain("Gitea");
  expect(routes).toContain("http://127.0.0.1:3000/");
  expect(routes).toContain("public");
  expect(routes).toContain("Traefik dashboard");
  expect(routes).toContain("lands on nothing knobas knows");
  expect(routes).toContain("opened_by compose");
  // The VM holds `gitea`, so the route landing on it also reaches the VM, and
  // the page says which end that is.
  expect(routes).toContain("Reachable via");
  expect(routes).toContain("inside, on gitea, exposed by vm-app-02");
  // A route this asset exposes is its to edit; one it is reached by is not.
  expect([...panel("Routes").querySelectorAll("button")].filter((button) => button.textContent?.trim() === "Edit…")).toHaveLength(2);

  // The URL is the *open URL* action, handed to the OS browser.
  buttonNamed("http://traefik.hel1.example:8080/dashboard", panel("Routes"))!.click();
  await waitFor(() => expect(opened).toEqual(["http://traefik.hel1.example:8080/dashboard"]));

  // The far end of the exposed route opens that asset's page.
  buttonNamed("gitea", panel("Routes"))!.click();
  flushSync();
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: GITEA.id });
});

/**
 * The other end of the same pair, on a page of its own: the container the
 * route lands on says *lands here* and names who offers it, and the
 * untargeted route — which reaches nobody — is not on this page at all.
 */
test("the page of the asset a route lands on names the end it came from", async () => {
  render("#/asset/asset:gitea");
  await waitFor(() => expect(said("Routes")).toContain("Reachable via"));

  const routes = said("Routes");
  expect(routes).toContain("lands here, exposed by vm-app-02");
  expect(routes).toContain("Exposes No routes.");
  expect(routes).not.toContain("Traefik dashboard");
});

/** An asset no route reaches says so under both headings: *nothing* is a real answer to *how is this reached*. */
test("an asset no route reaches says so under both headings", async () => {
  render("#/asset/asset:postgres");
  await waitFor(() => expect(title()).toBe("postgres"));

  expect(said("Routes")).toContain("Exposes No routes.");
  expect(said("Routes")).toContain("Reachable via Nothing lands here.");
});

/**
 * **A route's address draws the page of the asset that exposes it, with the
 * route picked** — story 14: a ticket about a certificate links to the route
 * whose certificate it is, and the link has to land somewhere that says what
 * the route belongs to.
 *
 * The address carries no asset, so the title is the proof that the page
 * resolved one from the route's own read. The address stays the route's: a
 * reader who copied the link must be able to hand it on.
 */
test("a route's address draws the page of the asset that exposes it, with the route picked", async () => {
  const { nav, read } = render("#/route/route:gitea");

  await waitFor(() => expect(title()).toBe("vm-app-02"));
  expect(read).toEqual([SIBLING.id]);
  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: null, routeId: "route:gitea" });
  // Marked in **both** lists, and that is the fixture rather than a bug: the
  // VM exposes this route *and* is reached by it, because it lands on the
  // container the VM holds. What the mark means is "this is the route the
  // address names", and it names one route.
  const picked = [...target.querySelectorAll('li[aria-current="true"]')];
  expect(picked).toHaveLength(2);
  for (const route of picked) {
    expect(route.classList.contains("tf-sel")).toBe(true);
    expect(route.textContent).toContain("Gitea");
  }
  expect(target.querySelectorAll("li[aria-current]")).toHaveLength(2);
  // The picked route's own history, under it: the route has nothing recorded.
  // Once, under the row in *Exposes*: the same route reached from inside says
  // nothing a second time.
  expect(picked[0]!.textContent?.replace(/\s+/g, " ")).toContain("Gitea history Nothing recorded.");
  expect(picked[1]!.textContent).not.toContain("history");
});

/**
 * Clicking a route in *reachable via* goes to the exposing asset's page, with
 * the route picked — the rule the address above follows, reached by a click.
 */
test("clicking a route reached from elsewhere opens the page of the asset exposing it", async () => {
  const { nav } = render("#/asset/asset:gitea");
  await waitFor(() => expect(said("Routes")).toContain("Reachable via"));

  const route = [...panel("Routes").querySelectorAll<HTMLButtonElement>("button.tf-as-rname")].find(
    (button) => button.textContent?.trim() === "Gitea",
  );
  route?.click();
  flushSync();

  expect(nav.route).toEqual({ view: "assets", tab: "asset", assetId: null, routeId: "route:gitea" });
  await waitFor(() => expect(title()).toBe("vm-app-02"));
});

// -- monitoring (#445) ---------------------------------------------------------

/** The attached monitors' rows, as a reader reads them: not the import's queue under them. */
function watching(): string[] {
  return [...panel("Monitoring").querySelectorAll("ul.tf-as-list:not(.tf-as-named) li")].map((item) =>
    (item.textContent ?? "").replace(/\s+/g, " ").trim(),
  );
}

/**
 * **The page's monitoring panel** (issue #445, spec #427 stories 33 and 71).
 *
 * What the reader is owed for each monitor watching this asset: its name, the
 * state Kuma last published, and one click to its own page in Kuma. The
 * paused one is the row that makes the panel honest — it is still attached,
 * it has no state and no page, and it says so rather than rendering as a blank
 * beside a live one. The third separates the two halves of that: a monitor
 * the mirror still holds, with a page to open and no state word.
 */
test("the page lists the monitors watching the asset, with their state", async () => {
  render(`#/asset/${CONTAINER.id}`);
  await waitFor(() => expect(watching()).toHaveLength(3));

  expect(watching()).toEqual([
    "postgres-check down Open in Kuma",
    "postgres-slow-query Paused or gone from Kuma",
    "postgres-wal no reading Open in Kuma",
  ]);
});

/**
 * A monitor's name opens the ticket-first Monitors page directly: the page
 * hands the nav `entityAddress`'s address, not a rooms `#/monitor/<id>` that
 * the shell would have to rewrite on arrival (#1092).
 */
test("a monitor's name goes to the Monitors page, not a rooms address", async () => {
  const { nav } = render(`#/asset/${CONTAINER.id}`);
  await waitFor(() => expect(watching()).toHaveLength(3));
  const goes: string[] = [];
  const go = nav.go.bind(nav);
  nav.go = (hash: string) => {
    goes.push(hash);
    go(hash);
  };

  buttonNamed("postgres-check", panel("Monitoring"))!.click();

  await waitFor(() => expect(goes).toEqual(["#/monitors"]));
});

/**
 * Story 71: *a deep link from a monitor to its page in Uptime Kuma, so that
 * what knobas does not do is one click away.* The URL handed over is the
 * **mirror's own** `web_url`; nothing here builds one.
 */
test("Open in Kuma hands the monitor's own URL to the browser", async () => {
  const { opened } = render(`#/asset/${CONTAINER.id}`);
  await waitFor(() => expect(watching()).toHaveLength(3));

  buttonNamed("Open in Kuma", panel("Monitoring"))!.click();
  await waitFor(() => expect(opened).toHaveLength(1));

  expect(opened).toEqual(["http://127.0.0.1:3001/dashboard/4"]);
});

/**
 * The two halves of one sentence: what the file *said* and what has been
 * *found*. A name that found its monitor is in the list above and must not be
 * repeated below it — the list underneath is the queue, not the roster.
 */
test("only the names still waiting for a monitor are listed as named by the import", async () => {
  render(`#/asset/${CONTAINER.id}`);
  await waitFor(() => expect(panel("Monitoring").querySelector("ul.tf-as-named")).not.toBeNull());

  expect(said("Monitoring")).toContain("Named by the import, not in Kuma yet");
  expect(
    [...panel("Monitoring").querySelectorAll("ul.tf-as-named li")].map((item) => (item.textContent ?? "").trim()),
  ).toEqual(["pgbouncer"]);
});

/**
 * An asset nothing watches says so, in a panel drawn on every page: on a page
 * of its own the question *what watches this* has a place whether or not an
 * import ever named a monitor, and an absent panel would read as one the page
 * forgot. Neither list is drawn under it.
 */
test("an asset with no monitors says nothing watches it, and draws neither list", async () => {
  render(`#/asset/${VM.id}`);
  await waitFor(() => expect(title()).toBe("vm-db-01"));

  expect(said("Monitoring")).toBe("Monitoring Nothing watches it.");
  expect(panel("Monitoring").querySelector("ul")).toBeNull();
});
