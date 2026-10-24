/**
 * The fixture's suggestion handlers (#237).
 *
 * These tests pin the table itself, and the two properties the tray's own
 * contract rests on: a proposal is a link with `confirmed_at: null`, and a
 * proposal with no reason is not shippable.
 *
 * This file lives under `dev/` on purpose. `house-rules.test.ts` resolves
 * every importer of the dev harness and requires the set to be exactly
 * `App.svelte`, exempting only files inside the harness directory itself;
 * a test one level up would be a second door into the fixture.
 */
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

import { expect, test, vi } from "vitest";

import type { AssetDetail, AssetRow } from "../../ipc/assets";
import { columnPathFor, stripFor, wireKey, wiresFor } from "../../assets/tree";

import type { ProposalEvidence, SuggestionPage } from "../../ipc/entity";
import { demoHandlers } from "./fake-tauri";
import { FAKE_CONTEXTS } from "./fake/corpus";
import { FIXTURE_SOURCES } from "./fake/sources";

test("the three suggestion commands are answered", () => {
  const handlers = demoHandlers();
  for (const cmd of ["room_suggestions", "accept_suggestion", "dismiss_suggestion"]) {
    expect(typeof handlers[cmd], `no handler for ${cmd}`).toBe("function");
  }
});

/** The whole room, the way the tray asks for it on `#/ctx/all`. */
const ROOM = { sources: [], ctx: null, limit: 50 };

function page(handlers: ReturnType<typeof demoHandlers>, args: Record<string, unknown> = ROOM): SuggestionPage {
  return handlers["room_suggestions"]!(args) as SuggestionPage;
}

/**
 * The two facts the tray's contract rests on, and then the two that make a
 * row worth drawing: both evidence classes are present (the badge exists to
 * tell them apart), and both ends resolve through `get_entity`, so every row
 * is openable. `total` is checked against the page's own rows *and* against a
 * narrower `limit`, because the heading shows the room's count, not the
 * window's.
 */
test("every proposal is unconfirmed, gives a reason, shows both classes and opens at both ends", () => {
  const handlers = demoHandlers();
  const answer = page(handlers);

  expect(answer.rows.length).toBeGreaterThan(0);
  expect(answer.total).toBe(answer.rows.length);
  for (const { link, from, to } of answer.rows) {
    expect(link.confirmed_at, `${link.id} is confirmed`).toBeNull();
    // The detector writes a relation the source records under the source's origin.
    expect(link.origin).toBe(link.rule_class === "source_relation" ? "source" : "suggested");
    expect(link.reason, `${link.id} has no reason`).toEqual(expect.any(String));
    expect(link.rule_class, `${link.id} has no class`).not.toBeNull();
    expect(from.entity_id).toBe(link.from_id);
    expect(to.entity_id).toBe(link.to_id);
    for (const end of [from, to]) {
      expect(() => handlers["get_entity"]!({ entityId: end.entity_id }), `${end.entity_id} does not open`).not.toThrow();
    }
  }
  const classes = new Set(answer.rows.map((row) => row.link.rule_class));
  expect(classes.has("exact_key")).toBe(true);
  expect(classes.has("similarity")).toBe(true);

  const window = page(handlers, { ...ROOM, limit: 1 });
  expect(window.rows).toHaveLength(1);
  expect(window.total).toBe(answer.total);
});

/**
 * Scoped like `list_entities`: `[]` is every source, a room the mock source is
 * not in holds nothing, and a room the fixture does not store (`ctx:fake-9`; #1503 seeds
 * `ctx:fake-1`, PAY-231's job) holds nothing.
 */
test("the page is scoped to the room it is read from", () => {
  const handlers = demoHandlers();
  expect(page(handlers, { ...ROOM, sources: ["mock"] }).total).toBe(page(handlers).total);
  expect(page(handlers, { ...ROOM, sources: ["gitea"] })).toEqual({ rows: [], total: 0, more: [] });
  expect(page(handlers, { ...ROOM, ctx: "ctx:fake-9" })).toEqual({ rows: [], total: 0, more: [] });
});

/**
 * Last in the file on purpose: the answer is session state, as it is for the
 * contexts handlers, and a fresh `demoHandlers()` does not reset it. Every
 * expectation here is relative to the read before it for the same reason.
 */
test("accepting or dismissing removes the row from the next read and drops the count", () => {
  const handlers = demoHandlers();
  const before = page(handlers);
  const [accepted, dismissed] = before.rows;
  expect(accepted && dismissed, "the fixture needs two rows to answer differently").toBeTruthy();

  expect(handlers["accept_suggestion"]!({ linkId: accepted!.link.id })).toBeNull();
  const afterAccept = page(handlers);
  expect(afterAccept.total).toBe(before.total - 1);
  expect(afterAccept.rows.map((row) => row.link.id)).not.toContain(accepted!.link.id);

  expect(handlers["dismiss_suggestion"]!({ linkId: dismissed!.link.id })).toBeNull();
  const afterDismiss = page(handlers);
  expect(afterDismiss.total).toBe(before.total - 2);
  expect(afterDismiss.rows.map((row) => row.link.id)).not.toContain(dismissed!.link.id);

  // Idempotent, as the real commands are: a second answer changes nothing.
  handlers["accept_suggestion"]!({ linkId: accepted!.link.id });
  expect(page(handlers).total).toBe(before.total - 2);

  // And an id no proposal carries is refused the way the real one refuses it.
  expect(() => handlers["dismiss_suggestion"]!({ linkId: "link:fake-none" })).toThrow(
    expect.objectContaining({ code: "not_found" }),
  );
});

/**
 * `proposal_evidence` (#836) answers every waiting proposal, and a key or a
 * shared word the corpus bears out is marked: a reason the fixture's text
 * could not show would be a fixture lying about the evidence.
 */
test("every proposal's evidence marks what the corpus bears out", () => {
  const handlers = demoHandlers();
  for (const entry of page(handlers).rows) {
    const evidence = handlers["proposal_evidence"]!({ linkId: entry.link.id }) as ProposalEvidence;
    expect(evidence.reason).toBe(entry.link.reason);
    expect(evidence.against.length, entry.link.id).toBeGreaterThan(0);
    const marked = evidence.excerpts.flatMap((excerpt) => excerpt.parts.filter((part) => part.marked));
    if (entry.link.rule_class === "exact_key") {
      const key = entry.link.to_id.slice(entry.link.to_id.indexOf(":") + 1);
      expect(marked.map((part) => part.text), entry.link.id).toContain(key);
    }
    if (entry.link.rule_class === "similarity") expect(marked.length, entry.link.id).toBeGreaterThan(0);
  }
  expect(() => handlers["proposal_evidence"]!({ linkId: "link:fake-none" })).toThrow(
    expect.objectContaining({ code: "not_found" }),
  );
});

/**
 * The estate the fixture answers with is `testenv/estate.json` (#440).
 *
 * Read here off the **file on disk**, not off the same import the fixture
 * uses: an assertion that compared the fixture to its own input would agree
 * with itself. What is under test is the mapping the fixture does on the way
 * — `assets::properties_of` and `assets::property_of` in TypeScript — because
 * that is the half a browser screenshot is evidence about. If it is wrong, a
 * QA pass photographs a pane the app would never draw.
 */
const ESTATE_TEXT = readFileSync(join(process.cwd(), "../testenv/estate.json"), "utf8");
const ESTATE_ON_DISK = JSON.parse(ESTATE_TEXT) as {
  name: string;
  assets: {
    id: string;
    type: string;
    name: string;
    parent?: string;
    description?: string;
    properties?: Record<string, string | number>;
    /** The Uptime Kuma names this asset's monitors answer to (#439, story 25). */
    monitors?: string[];
  }[];
  routes: { id: string; asset: string; target?: string }[];
};

/** One column of the Tree, as `asset_tree` answers it. */
function column(handlers: ReturnType<typeof demoHandlers>, parentId: string | null) {
  return handlers["asset_tree"]!({ parentId }) as { id: string; name: string }[];
}

test("the estate is the checked-in file, and every type it names has a chip", () => {
  const handlers = demoHandlers();

  // The top of the estate: the file has exactly one asset with no parent, and
  // a fixture that lost the `parent` mapping would draw all sixteen here.
  const roots = ESTATE_ON_DISK.assets.filter((asset) => asset.parent === undefined);
  expect(column(handlers, null).map((row) => row.id)).toEqual(roots.map((asset) => asset.id));

  // Every type the estate uses is in the table, with a two-character monogram
  // — `typeOf`'s fallback draws `??`, which is a chip that says the table has
  // drifted from the estate it serves.
  const types = handlers["asset_types"]!({}) as { id: string; monogram: string }[];
  for (const asset of ESTATE_ON_DISK.assets) {
    const declared = types.find((type) => type.id === asset.type);
    expect(declared, `no type ${asset.type} for ${asset.id}`).toBeDefined();
    expect(declared!.monogram).toHaveLength(2);
  }

  // Every route has both its ends in the estate, which is what a wire needs:
  // one row to leave and one row to land on.
  const ids = new Set(ESTATE_ON_DISK.assets.map((asset) => asset.id));
  for (const route of ESTATE_ON_DISK.routes) {
    expect(ids.has(route.asset), `${route.id} is exposed by nothing`).toBe(true);
    expect(ids.has(route.target ?? ""), `${route.id} lands on nothing`).toBe(true);
    expect(() => handlers["get_route"]!({ routeId: route.id })).not.toThrow();
  }
});

/**
 * The machines and their containers — criterion 1's own words — and the
 * property mapping, on the panes of the two machines that between them
 * exercise every branch of it: a declared key the file fills, a declared key
 * it does not, a custom string, a custom **number**, and the `description`
 * that has no column. (Until #634 one Hetzner server's pane carried all five;
 * the servers are gone and the notebook and OrbStack's VM split them.)
 */
test("a machine's pane draws the file's properties at the kinds the type declares", () => {
  const handlers = demoHandlers();

  // The notebook holds OrbStack's VM, the VM one Docker engine, and the engine
  // every container the estate runs.
  const vms = column(handlers, "asset:notebook");
  expect(vms.map((row) => row.id)).toEqual(["asset:orbstack"]);
  const engines = column(handlers, "asset:orbstack");
  expect(engines.map((row) => row.id)).toEqual(["asset:orbstack-docker"]);
  expect(column(handlers, "asset:orbstack-docker").length).toBeGreaterThan(5);

  type Detail = { properties: { key: string; label: string; value: unknown; custom: boolean }[] };
  const notebook = handlers["get_asset"]!({ assetId: "asset:notebook" }) as Detail;
  const notebookFile = ESTATE_ON_DISK.assets.find((asset) => asset.id === "asset:notebook")!;
  const on = (detail: Detail, key: string) =>
    detail.properties.find((property) => property.key === key);

  // Declared by `hypervisor`, in the type's order, and labelled by the type.
  expect(notebook.properties.slice(0, 3).map((property) => property.key)).toEqual([
    "hostname",
    "ip",
    "os",
  ]);
  expect(on(notebook, "os")).toEqual({
    key: "os",
    label: "OS",
    value: { kind: "text", value: notebookFile.properties!.os },
    custom: false,
  });
  // Declared and unfilled: `null`, which is the em dash the pane draws.
  expect(on(notebook, "hostname")!.value).toBeNull();
  // Custom, labelled by its own key.
  expect(on(notebook, "role")).toEqual({
    key: "role",
    label: "role",
    value: { kind: "text", value: notebookFile.properties!.role },
    custom: true,
  });
  // The description has no column in this model, so it is a property.
  expect(on(notebook, "description")!.value).toEqual({
    kind: "text",
    value: notebookFile.description,
  });

  // A number stays a number — the one place `valueOf` reads the JSON's type
  // rather than the schema's.
  const vm = handlers["get_asset"]!({ assetId: "asset:orbstack" }) as Detail;
  const vmFile = ESTATE_ON_DISK.assets.find((asset) => asset.id === "asset:orbstack")!;
  expect(on(vm, "memory_gb")).toEqual({
    key: "memory_gb",
    label: "memory_gb",
    value: { kind: "number", value: vmFile.properties!.memory_gb },
    custom: true,
  });

  // And the custom rows are in key order, after the declared ones.
  for (const detail of [notebook, vm]) {
    const custom = detail.properties.filter((property) => property.custom).map((p) => p.key);
    expect(custom).toEqual([...custom].sort());
  }
});

/**
 * **A route's row draws its wire, over the real estate** (#440, criterion 2).
 *
 * The geometry is `tree.ts`'s and has its own tests over a fixture built to
 * exercise it; what this asserts is that the **real** estate feeds it a wire
 * at all — that the file's routes and the file's containment put a far end in
 * a column the Tree has open. Composed from the fixture's own answers, so the
 * chain is the one a browser walks: `get_asset` → `columnPathFor` →
 * `asset_tree` per column → `wiresFor`.
 *
 * `knobas-gitea` is the asset to stand on because both of the estate's ways of
 * reaching a container meet on it: the notebook's published port, whose
 * exposer is three columns to the left, and the compose network's name for
 * it, whose exposer is the Docker engine one column to the left -- two wires
 * from two different columns. (Until #634 the second was the SSH tunnel's `-R`
 * forward, exposed by a Hetzner server in another branch and in no open
 * column, so one wire was drawn and one was not; that negative is
 * `tree.ts`'s own suite's now, since nothing in the real estate is exposed
 * off the path any more.)
 */
test("a route in the real estate draws a wire to the asset that exposes it", () => {
  const handlers = demoHandlers();
  const detail = handlers["get_asset"]!({ assetId: "asset:knobas-gitea" }) as AssetDetail;
  const columns = columnPathFor(detail).parents.map(
    (parent) => handlers["asset_tree"]!({ parentId: parent }) as AssetRow[],
  );
  expect(columns).toHaveLength(5);

  // A window wide enough for every column, so what comes back is about the
  // routes rather than about the collapse.
  const wide = stripFor(columns.length, 5000, null);
  expect(wiresFor(detail, columns, wide)).toEqual([
    {
      key: wireKey("via", "route:notebook-gitea"),
      column: 1,
      rowId: "asset:notebook",
      dashed: false,
    },
    {
      key: wireKey("via", "route:compose-gitea"),
      column: 3,
      rowId: "asset:orbstack-docker",
      dashed: false,
    },
  ]);

  // The same wires at a window too narrow for five columns: both rows are
  // behind spines, so each lands on its spine and is dashed.
  const narrow = stripFor(columns.length, 400, null);
  expect(wiresFor(detail, columns, narrow)).toEqual([
    { key: wireKey("via", "route:notebook-gitea"), column: 1, rowId: null, dashed: true },
    { key: wireKey("via", "route:compose-gitea"), column: 3, rowId: null, dashed: true },
  ]);
});

/**
 * `depends_on_this`' routes are the ones landing on the asset **or on anything
 * that breaks with it** (#1553), the rule `assets::depends_on_this` follows and
 * `crates/knobas-app/tests/it/assets_ipc.rs` witnesses over the same file.
 *
 * Three subjects, one per reason: the VM, which no route targets and which
 * every route in the file breaks with, since each lands on a container inside
 * it; Gitea, where TeamCity's route arrives through a drawn `depends-on` link
 * rather than through the tree; and TeamCity's database, under a container a
 * route lands on, which keeps answering when the database goes down.
 */
test("Depends on this lists every route landing on what breaks, and none above it", () => {
  const handlers = demoHandlers();
  const routesOf = (assetId: string) =>
    (handlers["depends_on_this"]!({ assetId }) as { routes: { id: string }[] }).routes
      .map((route) => route.id)
      .sort();

  expect(routesOf("asset:orbstack")).toEqual([
    "route:compose-gitea",
    "route:notebook-confluence",
    "route:notebook-gitea",
    "route:notebook-jira",
    "route:notebook-kuma",
    "route:notebook-mockd-admin",
    "route:notebook-mockd-jira",
    "route:notebook-mockd-teamcity",
    "route:notebook-teamcity",
  ]);
  expect(routesOf("asset:knobas-gitea")).toEqual([
    "route:compose-gitea",
    "route:notebook-gitea",
    "route:notebook-teamcity",
  ]);
  expect(routesOf("asset:db-teamcity")).toEqual([]);
});

/**
 * The Import dialog under `?fake-ipc` (#440).
 *
 * The estate this fixture draws **is** the file `--demo` imports, so choosing
 * that file is the one Import gesture a reader will make here, and what it has
 * to answer is criterion 3's sentence: every entry already in the tree.
 *
 * Last in the file, and the two tests are ordered: the second one deletes an
 * asset, and the fixture's estate is session state that a fresh
 * `demoHandlers()` does not reset — the same rule the suggestion tests above
 * are written under.
 */
test("importing the estate file previews every entry as already in the tree", () => {
  const handlers = demoHandlers();
  const preview = handlers["preview_estate_import"]!({ file: ESTATE_TEXT }) as {
    name: string;
    known: { id: string; kind: string }[];
    new: unknown[];
    changes: unknown[];
  };

  expect(preview.name).toBe(ESTATE_ON_DISK.name);
  expect(preview.known.map((entry) => entry.id)).toEqual([
    ...ESTATE_ON_DISK.assets.map((asset) => asset.id),
    ...ESTATE_ON_DISK.routes.map((route) => route.id),
  ]);
  expect(preview.new).toEqual([]);
  // Applying an all-known import writes nothing, and says six zeroes.
  expect(handlers["apply_estate_import"]!({ file: ESTATE_TEXT })).toEqual({
    assets_created: 0,
    routes_created: 0,
    properties_set: 0,
    properties_kept: 0,
    monitors_kept: 0,
    monitors_linked: 0,
  });

  // Any other file is the real command's to parse, and is refused by name
  // rather than answered from this estate.
  expect(() => handlers["preview_estate_import"]!({ file: '{"name":"somewhere else"}' })).toThrow(
    expect.objectContaining({ code: "invalid" }),
  );
  expect(() => handlers["preview_estate_import"]!({ file: "not json" })).toThrow(
    expect.objectContaining({ code: "invalid" }),
  );
});

test("an asset deleted here comes back as new, and the import brings it back", () => {
  const handlers = demoHandlers();
  // A leaf, so the delete is not refused for what it holds.
  handlers["delete_asset"]!({ assetId: "asset:db-confluence" });

  const gone = handlers["preview_estate_import"]!({ file: ESTATE_TEXT }) as {
    new: { id: string; name: string; type_label: string | null }[];
  };
  expect(gone.new).toEqual([
    { id: "asset:db-confluence", kind: "asset", name: "confluence", type_label: "Database", parent_id: "asset:knobas-confluence-db" },
  ]);

  expect(handlers["apply_estate_import"]!({ file: ESTATE_TEXT })).toMatchObject({
    assets_created: 1,
    routes_created: 0,
  });
  // And it is in the tree again, with the origin line the real import writes.
  const detail = handlers["get_asset"]!({ assetId: "asset:db-confluence" }) as {
    history: { actor: string; verb: string }[];
  };
  expect(detail.history[0]).toMatchObject({ actor: "import", verb: "imported" });
  expect(
    (handlers["preview_estate_import"]!({ file: ESTATE_TEXT }) as { new: unknown[] }).new,
  ).toEqual([]);
});

/**
 * The roster the Monitors tab draws under `?fake-ipc` (#448).
 *
 * Read against the **file on disk**, like the estate tests above and for their
 * reason: what a QA screenshot of that tab is evidence about is the mapping,
 * so a fixture compared to its own input would agree with itself. The file
 * names each asset's monitors (story 25), so the set of monitors the tab
 * should show is already written down once.
 *
 * The three properties a screenshot of it has to be able to show: every chip
 * has something in it, a paused monitor has no page left in Kuma to open, and
 * the row's state is the last sample's — which is the rule the backend follows
 * and the reason the chip and the bar's right-hand end cannot disagree.
 */
test("the fixture roster is the file's monitors, with every chip reachable", () => {
  const handlers = demoHandlers();
  const roster = handlers["monitor_roster"]!({}) as {
    name: string;
    state: string | null;
    tombstoned: boolean;
    web_url: string | null;
    assets: { id: string }[];
    samples: { taken_at: string; state: string | null }[];
  }[];

  // Every monitor the seed creates is a row, and every name the estate keeps
  // is one of them (the canary is the one row nothing in the estate names).
  const seeded = (
    JSON.parse(readFileSync(join(process.cwd(), "../testenv/monitors.json"), "utf8")) as {
      name: string;
    }[]
  ).map((monitor) => monitor.name);
  expect(roster.map((row) => row.name).sort()).toEqual([...seeded].sort());
  const named = ESTATE_ON_DISK.assets.flatMap((asset) => asset.monitors ?? []);
  expect(named.length).toBeGreaterThan(0);
  for (const name of named) expect(seeded).toContain(name);

  // Every chip has something in it: a roster where everything is `up` can be
  // photographed without showing that the chips do anything.
  const chips = new Set(roster.map((row) => (row.tombstoned ? "paused" : row.state)));
  for (const state of ["up", "warn", "down", "pending", "paused", "maintenance"]) {
    expect(chips.has(state), `nothing in the fixture is ${state}`).toBe(true);
  }

  for (const row of roster) {
    expect(row.state, `${row.name} is not its last sample`).toBe(row.samples.at(-1)?.state ?? null);
    expect(row.assets.length > 0, `${row.name} watches nothing`).toBe(named.includes(row.name));
    // A monitor Kuma no longer publishes has no page left to open, so the
    // button is absent rather than dead.
    if (row.tombstoned) expect(row.web_url, `${row.name} is paused and still links`).toBeNull();
  }
  expect(roster.some((row) => row.tombstoned), "no paused monitor in the fixture").toBe(true);
});

/**
 * The open alerts the strip draws under `?fake-ipc` (#444's gap, filled with
 * #446).
 *
 * Read against the **roster handler beside it**, because that is the claim:
 * one fixture, two surfaces, and a screenshot of the strip that disagreed with
 * the tab would be evidence about nothing. The two properties the fixture has
 * to have are that only the two trouble words open one — `pending` and
 * `maintenance` neither open an alert nor close one — and that an acked alert
 * is among them, since *seen, not fixed* is the distinction the list exists to
 * draw.
 */
test("the fixture's open alerts are the roster's monitors in trouble", () => {
  const handlers = demoHandlers();
  const roster = handlers["monitor_roster"]!({}) as { name: string; state: string | null }[];
  const open = handlers["open_alerts"]!({}) as {
    monitor_name: string;
    state: string;
    acked_at: string | null;
    assets: { id: string }[];
  }[];

  expect(open.map((row) => row.monitor_name).sort()).toEqual(
    roster
      .filter((row) => row.state === "down" || row.state === "warn")
      .map((row) => row.name)
      .sort(),
  );
  expect(open.length).toBeGreaterThan(0);
  for (const row of open) {
    expect(["down", "warn"], `${row.monitor_name} opened an alert on ${row.state}`).toContain(
      row.state,
    );
    expect(row.assets.length, `${row.monitor_name} watches nothing`).toBeGreaterThan(0);
  }
  expect(
    open.some((row) => row.acked_at !== null),
    "nothing in the fixture is acked, so the list cannot show what seen-not-fixed looks like",
  ).toBe(true);
});

/**
 * The Monitors tab's second list under `?fake-ipc` (#449).
 *
 * Read against the **estate file itself**, because that is the claim the
 * fixture makes: the roster of gaps is the assets `estate.json` names no
 * Uptime Kuma monitor for, so a browser screenshot of the tab is a picture of
 * the real estate's real gaps rather than of a list invented beside it.
 *
 * Both directions, since a handler answering the whole estate would pass a
 * one-directional check: every asset with monitors is out, and every asset
 * without them is in.
 */
test("the fixture's Not monitored roster is the estate's assets with no monitor named", () => {
  const handlers = demoHandlers();
  const gaps = handlers["unmonitored_assets"]!({}) as {
    id: string;
    name: string;
    type_label: string;
  }[];
  const roster = handlers["monitor_roster"]!({}) as { assets: { id: string }[] }[];
  const watched = new Set(roster.flatMap((row) => row.assets.map((asset) => asset.id)));

  expect(gaps.length, "an estate where everything is watched cannot show the roster").toBeGreaterThan(0);
  for (const gap of gaps) {
    expect(watched.has(gap.id), `${gap.name} has a monitor and is still on the roster`).toBe(false);
    expect(gap.type_label, `${gap.name} has no type label to filter by`).not.toBe("");
  }
  // And the other way: nothing the file gives a monitor is on this list. The
  // size check first, or an estate whose roster watched nothing would pass
  // this loop without entering it.
  expect(
    watched.size,
    "no asset in the fixture has a monitor, so the reverse direction is vacuous",
  ).toBeGreaterThan(0);
  for (const id of watched) {
    expect(gaps.some((gap) => gap.id === id)).toBe(false);
  }
});

/**
 * *Create monitor* under `?fake-ipc` (#1959): the fixture mirrors no monitor,
 * so the asset the create recorded its name on stays on the list with the
 * create on its way, as the real read says before the poll, and no other row
 * carries it.
 */
test("the fixture keeps a created monitor's asset listed with its create on its way", () => {
  const handlers = demoHandlers();
  type Gap = { id: string; name: string; monitor_on_its_way: { id: number } | null };
  const before = handlers["unmonitored_assets"]!({}) as Gap[];
  const asset = before.find((gap) => gap.id.startsWith("asset:"))!;
  handlers["edit_asset"]!({ assetId: asset.id, edits: [{ field: "monitors", added: ["fixture-1959"] }] });
  const write = handlers["submit_write"]!({
    payload: { CreateMonitor: { entity: "kuma:monitors", name: "fixture-1959", url: "https://x.test/", kind: "http" } },
  }) as { id: number };

  const after = handlers["unmonitored_assets"]!({}) as Gap[];
  expect(after.map((gap) => gap.id)).toEqual(before.map((gap) => gap.id));
  expect(after.filter((gap) => gap.monitor_on_its_way !== null).map((gap) => [gap.id, gap.monitor_on_its_way!.id])).toEqual([
    [asset.id, write.id],
  ]);
});

/**
 * The ack a card offers under `?fake-ipc` (#449's button over #446's write).
 *
 * The two things the real command guarantees and a fixture can get wrong: the
 * alert **stays open** and comes back reading acked — only a return to `up`
 * closes one — and a monitor with nothing open is refused rather than
 * answered.
 */
test("the fixture's ack leaves the alert open and refuses a monitor with none", () => {
  const handlers = demoHandlers();
  const open = handlers["open_alerts"]!({}) as { monitor_id: string; acked_at: string | null }[];
  const unacked = open.find((row) => row.acked_at === null);
  expect(unacked, "every alert is already acked, so the ack cannot be exercised").toBeDefined();

  const acked = handlers["ack_alert"]!({ monitorId: unacked!.monitor_id }) as {
    acked_at: string | null;
  };
  expect(acked.acked_at).not.toBeNull();
  const after = handlers["open_alerts"]!({}) as { monitor_id: string; acked_at: string | null }[];
  expect(
    after.find((row) => row.monitor_id === unacked!.monitor_id)?.acked_at,
    "an ack is not a close",
  ).not.toBeNull();

  expect(() => handlers["ack_alert"]!({ monitorId: "kuma:nothing-is-wrong-here" })).toThrow();
});

/**
 * The launcher board's rail under `?fake-ipc` (#504).
 *
 * What a walk in a browser needs to be able to see, and what a fixture can get
 * wrong without anybody noticing: the rail is the **seven built-ins in the
 * registry's order followed by the saved lists** (#506), the three estate lists
 * carry the counts the fixture's own estate implies, and the rows one of them
 * answers with are **assets** — because a row of any other kind opens a room
 * detail instead of the Tree, which is the whole of #504's criterion 2.
 *
 * The four mirror lists reading 0 is asserted rather than tolerated: this
 * fixture's corpus is frozen at `SYNCED_AT` and no source in it carries a
 * username, so a non-zero count there would be this file inventing a mirror.
 */
test("the fixture's launcher board draws the seven built-ins, with the estate's three counted", () => {
  const handlers = demoHandlers();
  const board = handlers["launcher_home"]!({}) as {
    smart_lists: {
      id: string;
      label: string;
      count: number;
      changed: boolean;
      description: string;
      saved: boolean;
    }[];
    recent: unknown[];
    sources: unknown[];
    pending_writes: number;
    notes_shown: boolean;
  };

  expect(board.notes_shown).toBe(false);
  expect(board.smart_lists.filter((list) => !list.saved).map((list) => list.id)).toEqual([
    "changed-today",
    "mine",
    "mine-stale",
    "just-synced",
    "not-monitored",
    "alerts-in-context",
    "certs-expiring",
  ]);
  // The built-ins come first and the saved ones after, which is the order
  // `Searcher::smart_lists` builds and the order the rail draws.
  const savedFrom = board.smart_lists.findIndex((list) => list.saved);
  expect(savedFrom).toBe(7);
  expect(board.smart_lists.slice(savedFrom).every((list) => list.saved)).toBe(true);
  const count = (id: string) => board.smart_lists.find((list) => list.id === id)!.count;
  for (const id of ["changed-today", "mine", "mine-stale", "just-synced"]) {
    expect(count(id), `${id} has no mirror here to count`).toBe(0);
  }
  // Every estate list has something in it, or the walk has nothing to open.
  for (const id of ["not-monitored", "alerts-in-context", "certs-expiring"]) {
    expect(count(id), `${id} is empty and cannot be walked`).toBeGreaterThan(0);
  }
  // #1503: the sixth list is named for the reader's jobs, and its count is the
  // alerted assets an unarchived fake context holds -- one, the TeamCity asset
  // PAY-231 holds -- and falls to nothing when that context is archived.
  expect(board.smart_lists.find((list) => list.id === "alerts-in-context")!.label).toBe(
    "Open alerts on my jobs",
  );
  expect(count("alerts-in-context")).toBe(1);
  const jobs = FAKE_CONTEXTS.filter((context) => context.archived_at === null);
  jobs.forEach((context) => (context.archived_at = "2026-10-21T00:00:00Z"));
  try {
    const archived = handlers["smart_lists"]!({}) as { id: string; count: number }[];
    expect(archived.find((list) => list.id === "alerts-in-context")!.count).toBe(0);
  } finally {
    jobs.forEach((context) => (context.archived_at = null));
  }
  // A badge is a comparison against a stamp this fixture cannot keep.
  expect(board.smart_lists.every((list) => list.changed === false)).toBe(true);
  expect(board.recent.length).toBeGreaterThan(0);
  expect(board.sources.length).toBeGreaterThan(0);

  // And the rows behind the counts: assets, with the count the rail promised.
  for (const id of ["not-monitored", "alerts-in-context", "certs-expiring"]) {
    const response = handlers["search"]!({ query: { raw: `list:${id}`, filters: {} } }) as {
      total: number;
      groups: { kind: string; total: number; hits: { entity_id: string; kind: string }[] }[];
    };
    expect(response.total, `${id}'s rows and its count are one answer`).toBe(count(id));
    expect(response.groups.map((group) => group.kind)).toEqual(["asset"]);
    for (const hit of response.groups[0]!.hits) {
      expect(hit.kind).toBe("asset");
      expect(hit.entity_id.startsWith("asset:")).toBe(true);
    }
  }

  // A list nobody ships is refused, not answered with nothing.
  expect(() => handlers["search"]!({ query: { raw: "list:nope", filters: {} } })).toThrow();
});

/**
 * The fixture's *Certificates expiring* is the roster's own certificate, and
 * the roster invents exactly one — nine days.
 *
 * Both directions, the `unmonitored_assets` test's rule: the asset the
 * certificate is on is in, and every asset whose monitor has no certificate is
 * out. A handler answering the whole estate would pass a one-sided check.
 */
test("the fixture's expiring certificates are the roster rows under thirty days", () => {
  const handlers = demoHandlers();
  const roster = handlers["monitor_roster"]!({}) as {
    cert_days_remaining: number | null;
    assets: { id: string }[];
  }[];
  const expiring = new Set(
    roster
      .filter((row) => row.cert_days_remaining !== null && row.cert_days_remaining < 30)
      .flatMap((row) => row.assets.map((asset) => asset.id)),
  );
  expect(expiring.size, "a roster with no certificate cannot show this list").toBeGreaterThan(0);

  const rows = handlers["search"]!({ query: { raw: "list:certs-expiring", filters: {} } }) as {
    groups: { hits: { entity_id: string }[] }[];
  };
  expect(new Set(rows.groups[0]!.hits.map((hit) => hit.entity_id))).toEqual(expiring);
  // And nothing whose monitor watches no certificate at all.
  const noCert = roster
    .filter((row) => row.cert_days_remaining === null)
    .flatMap((row) => row.assets.map((asset) => asset.id));
  expect(noCert.length).toBeGreaterThan(0);
  for (const id of noCert) {
    expect(expiring.has(id), `${id} has no certificate and is on the list`).toBe(false);
  }
});

/**
 * # The census: what the `?fake-ipc` walk cannot reach (#544)
 *
 * `invoke` throws *"no handler for ${cmd}"* on a command this fixture does
 * not answer, and a section that reads before it draws — `BackupSection.svelte`
 * puts its three header actions behind `{#if status && !error}` and everything
 * below them behind `{:else if status}` — then renders that error instead of
 * itself, leaving only a *Retry* that rejects the same way. So the walk's reach
 * is exactly the fixture's
 * handler table, and [`UNANSWERED`] is everything outside it: whole surfaces,
 * not stray commands.
 *
 * That is not a bug to be fixed by teaching the fixture everything. The
 * deputy's ruling of 2026-09-08 on #507 gives the reason for one of them and
 * it holds for the rest: *"a fake `share_export` would certify nothing about
 * an archive and would be a second copy of `ShareParts::tables()`, the copy
 * that drifts"*. What was missing was the **record** — nothing said which
 * surfaces the substituted witness cannot see, so the next criterion written
 * around a walk over Settings → Backup would have been found out by hand.
 *
 * This is that record, kept as a test rather than as prose, so it cannot go
 * stale quietly:
 *
 * * the command set is **parsed from `app/src/lib/ipc/*.ts`**, never hand
 *   listed, so a command that lands there arrives here on its own;
 * * the handler set is `demoHandlers()` itself, not a copy of it;
 * * their difference must equal [`UNANSWERED`] exactly — a new unanswered
 *   command fails this test until someone either answers it or writes it down
 *   with the seam suite that witnesses it instead, and a command that *gains*
 *   a handler fails it until it leaves the list.
 *
 * Editing [`UNANSWERED`] is the point, not a workaround. What the test forbids
 * is editing it by accident.
 */
const IPC_DIR = join(process.cwd(), "src/lib/ipc");

/** The repo root, which Vitest's own root (`app/`) is one level under. */
const REPO_ROOT = join(process.cwd(), "..");

/** The IPC barrel's modules, with their text, read off disk. */
function ipcModules(): { name: string; text: string }[] {
  return readdirSync(IPC_DIR)
    .filter((name) => name.endsWith(".ts"))
    .map((name) => ({ name, text: readFileSync(join(IPC_DIR, name), "utf8") }));
}

/** An `invoke` call up to its command name, which is the match's one group. */
const INVOKE_CALL = /\binvoke(?:<[^>]*>)?\(\s*"([a-z0-9_]+)"/g;

/**
 * Every command name the barrel invokes, against the module that invokes it.
 *
 * The generic is optional in the pattern on purpose: `invoke<T>("cmd")` is the
 * house style, and a future `invoke("cmd")` written without one must still be
 * counted rather than silently slip past the census.
 */
function barrelCommands(): Map<string, string> {
  const found = new Map<string, string>();
  for (const { name, text } of ipcModules()) {
    for (const [, cmd] of text.matchAll(INVOKE_CALL)) {
      found.set(cmd!, name);
    }
  }
  return found;
}

/**
 * The surfaces `?fake-ipc` cannot open, grouped, each with the seam suites that
 * are the witness there instead.
 *
 * A group's witness is where the behaviour is actually asserted — a real
 * database and, where a source is on the other end, the trait-level fake or
 * the live instance ADR-0013 asks for. None of them is a browser walk, which
 * is the whole point: the walk certifies layout and interaction, and for these
 * surfaces it certifies nothing at all because it never reaches them.
 */
const UNANSWERED: { surface: string; witnesses: string[]; commands: string[] }[] = [
  {
    // The status, *Back up now* and the schedule are answered since #921
    // (`fake/settings.ts`); the two that remain stay out for the reason in
    // the block comment above: an archive a fake wrote certifies nothing.
    surface: "Settings → Backup: the Share… export and the restore",
    witnesses: [
      "crates/knobas-app/tests/it/backup_ipc.rs",
      "crates/knobas-app/tests/share_exit.rs",
      "app/src/lib/tf/settings/TfSettingsSections.test.svelte.ts",
    ],
    commands: ["share_export", "restore_backup"],
  },
  {
    // The flow itself is answered since #818 (`fake/job-reach.ts`), so the
    // job's stepper can be walked; the merge follow is not.
    surface: "The merge follow that closes a started ticket",
    witnesses: ["crates/knobas-app/tests/it/start_work.rs"],
    commands: ["follow_merges"],
  },
];

/** Every command [`UNANSWERED`] claims the fixture does not answer. */
function recorded(): string[] {
  return UNANSWERED.flatMap((group) => group.commands);
}

/**
 * The negative control, and it is load-bearing: a parse that found nothing
 * would make the census's difference empty and the census would pass while
 * saying nothing at all. So the parse has to find **every command the fixture
 * does answer**, and at least one command in every barrel module that imports
 * `invoke` — the second half because a module whose commands are all on the
 * census (`backup.ts` is exactly that) could drop out of the parse silently
 * under the first.
 */
test("the parse finds every command the fixture answers, in every module that invokes one", () => {
  const commands = barrelCommands();
  const answered = Object.keys(demoHandlers());

  expect(answered.length, "a fixture this small cannot control anything").toBeGreaterThan(50);
  for (const cmd of answered) {
    expect([...commands.keys()], `${cmd} is answered but the parse did not find it in the barrel`).toContain(cmd);
  }

  // This half walks the directory itself rather than calling `ipcModules()`,
  // and the duplication is the point: a control that shared the parse's own
  // filter could not see a module the parse stopped reading. (It could not, and
  // the mutant proved it: narrowing `ipcModules()`'s filter left this loop
  // iterating the same shortened list and agreeing with it.) The two halves do
  // still share `IPC_DIR`, so this controls which files are read, not where.
  const modules = new Set(commands.values());
  for (const name of readdirSync(IPC_DIR)) {
    if (!name.endsWith(".ts")) continue;
    if (!readFileSync(join(IPC_DIR, name), "utf8").includes('from "@tauri-apps/api/core"')) continue;
    expect(modules, `${name} imports invoke and the parse found no command in it`).toContain(name);
  }
});

/**
 * The census itself. See the block comment above [`UNANSWERED`] for why a
 * failure here is a prompt to write the new command down rather than to teach
 * the fixture to answer it.
 */
test("every barrel command is either answered by the fixture or recorded as out of the walk's reach", () => {
  const commands = [...barrelCommands().keys()];
  const answered = new Set(Object.keys(demoHandlers()));

  // First the direction a fixture change breaks: a command that has gained a
  // handler has to leave the list, or the record lies about the walk. This is
  // checked before the set equality so that the failure names the command and
  // the group it is still filed under, rather than a diff of every name here.
  for (const { surface, commands: group } of UNANSWERED) {
    for (const cmd of group) {
      expect(answered.has(cmd), `${cmd} is answered now; drop it from "${surface}"`).toBe(false);
    }
  }

  // Then the direction a barrel change breaks: a command that landed with no
  // handler and no entry here.
  const unanswered = commands.filter((cmd) => !answered.has(cmd)).sort();
  expect(unanswered).toEqual([...recorded()].sort());
});

/**
 * Every group says what witnesses it instead, and no command is filed twice.
 *
 * The witness paths are **resolved on disk as files**, not pattern-matched: a
 * record pointing at a suite somebody has since renamed would be a record of a
 * witness that is not there, which is the failure this whole file exists to
 * stop one level up. It has to be a file and not merely a path that resolves,
 * because the two paths that resolve without naming a suite are exactly the
 * two a slip would produce -- `""`, which is the repo root, and a directory.
 */
test("the census names a witness that exists for every surface, and files each command once", () => {
  const all = recorded();
  expect(new Set(all).size, "a command is recorded in two groups").toBe(all.length);
  for (const { surface, witnesses, commands } of UNANSWERED) {
    expect(commands.length, `${surface} records no command`).toBeGreaterThan(0);
    expect(witnesses.length, `${surface} names no witness`).toBeGreaterThan(0);
    for (const path of witnesses) {
      const full = join(REPO_ROOT, path);
      const isSuite = existsSync(full) && statSync(full).isFile();
      expect(isSuite, `${surface} names ${path}, which is not a file in the tree`).toBe(true);
    }
  }
});

/**
 * # The census's argument half (#1324)
 *
 * The census above holds the fixture to the barrel by command name. Tauri
 * reads a command's arguments by name as well, camel-cased, so a handler that
 * reads `args.sourceId` after the barrel's call was renamed to send `id` draws
 * the demo wrong with nothing failing: the read is `undefined`, and a fixture
 * that falls back on `undefined` quietly answers something else.
 *
 * So every faked handler is called with a recording proxy for its arguments,
 * and every key it reads must be one the barrel's `invoke` call for that
 * command sends. Both sides are read the way the census reads them: the keys
 * sent are **parsed from the call sites in `app/src/lib/ipc/*.ts`**, and the
 * keys read are whatever the handler itself touches.
 *
 * The proxy answers `undefined` to every read, so a handler that dereferences
 * an argument throws. That is expected and harmless: the keys it read before
 * the throw are already recorded, and they count. What the proxy cannot see is
 * a read behind a branch the `undefined` answer does not take, or one made by
 * listing the keys (`Object.keys`, a rest pattern), which no handler does; this
 * half pins every read a bare call reaches, not every read there is.
 */

/**
 * `text` with every comment and the inside of every string literal blanked to
 * spaces, at the same offsets, so a scan for brackets and commas reads code
 * only: a `"{"` in a value or an apostrophe in a comment is not a bracket or a
 * quote. An `invoke(` the regex finds inside a comment is blank here too.
 */
function codeOnly(text: string): string {
  let code = "";
  for (let at = 0; at < text.length; at++) {
    const char = text[at]!;
    const next = text[at + 1];
    if (char === "/" && (next === "/" || next === "*")) {
      const close = next === "/" ? text.indexOf("\n", at) : text.indexOf("*/", at + 2) + 2;
      const end = close < at ? text.length : close;
      code += " ".repeat(end - at);
      at = end - 1;
    } else if (char === '"' || char === "'" || char === "`") {
      let end = at + 1;
      while (end < text.length && text[end] !== char) end += text[end] === "\\" ? 2 : 1;
      if (end >= text.length) throw new Error(`a string opened at ${at} never closes`);
      code += char + " ".repeat(end - at - 1) + char;
      at = end;
    } else {
      code += char;
    }
  }
  return code;
}

/**
 * The second argument of the call whose command name ends at `from`: the
 * code up to the comma or paren that closes it at the call's own depth.
 */
function secondArgument(code: string, from: number): string {
  let depth = 0;
  let argument = "";
  for (const char of code.slice(from).replace(/^\s*,/, "")) {
    if ("([{".includes(char)) depth++;
    if (")]}".includes(char) || char === ",") {
      if (depth === 0) return argument;
      if (char !== ",") depth--;
    }
    argument += char;
  }
  throw new Error(`an invoke call at ${from} never closes`);
}

/**
 * The keys of every object literal at the top of an `invoke` call's argument:
 * one literal in the house style, two for a call that chooses between them
 * (`submit_write` sends `linkTo` only when it has one).
 *
 * A spread or a bare identifier is a set of keys the parse cannot read, so it
 * fails rather than counting the call as sending nothing.
 */
function keysSent(cmd: string, argument: string): string[] {
  const keys: string[] = [];
  let objectLiterals = 0;
  let depth = 0;
  let entry = "";
  const endEntry = () => {
    const key = entry.trim().split(":")[0]!.trim();
    if (key.startsWith("...")) throw new Error(`${cmd}'s invoke spreads ${key.slice(3)}, which the census cannot read`);
    if (key) keys.push(key);
    entry = "";
  };
  for (const char of argument) {
    if ("([{".includes(char)) {
      depth++;
      if (char === "{" && depth === 1) {
        objectLiterals++;
        continue;
      }
    } else if (")]}".includes(char)) {
      depth--;
      if (char === "}" && depth === 0) endEntry();
    } else if (char === "," && depth === 1) {
      endEntry();
      continue;
    }
    if (depth >= 1) entry += char;
  }
  if (objectLiterals === 0 && argument.trim()) {
    throw new Error(`${cmd}'s invoke sends ${argument.trim()}, which is not an object literal the census can read`);
  }
  return keys;
}

/** Every key each command's `invoke` call sends, against the command. */
function barrelArguments(): Map<string, Set<string>> {
  const sent = new Map<string, Set<string>>();
  for (const { text } of ipcModules()) {
    const code = codeOnly(text);
    for (const call of text.matchAll(INVOKE_CALL)) {
      if (!code.startsWith("invoke", call.index)) continue;
      const cmd = call[1]!;
      const keys = sent.get(cmd) ?? new Set<string>();
      for (const key of keysSent(cmd, secondArgument(code, call.index + call[0].length))) keys.add(key);
      sent.set(cmd, keys);
    }
  }
  return sent;
}

/**
 * Every argument key `handler` reads, called once with a recording proxy.
 *
 * Recorded as the read happens, so a handler that throws on the `undefined`
 * it was handed still contributes every key it read first; an async handler's
 * rejection is awaited and swallowed for the same reason.
 */
async function keysRead(handler: (args: Record<string, unknown>) => unknown): Promise<string[]> {
  const read = new Set<string>();
  const record = (key: string | symbol) => {
    if (typeof key === "string") read.add(key);
  };
  const args = new Proxy({} as Record<string, unknown>, {
    get: (_, key) => (record(key), undefined),
    has: (_, key) => (record(key), false),
  });
  try {
    await handler(args);
  } catch {
    // The reads before the throw are the point; the throw is the proxy's doing.
  }
  return [...read];
}

/**
 * The controls. The parse has to read the shapes the barrel actually writes —
 * a literal over several lines, a conditional between two literals — or an
 * empty parse would pass every handler that reads nothing; and the recorder
 * has to keep a read that a throw follows.
 */
test("the argument parse reads what the barrel sends, and the recorder keeps a read a throw follows", async () => {
  const sent = barrelArguments();
  expect([...sent.get("mini_board")!].sort()).toEqual(
    ["ctxId", "lanes", "lanesFirst", "perColumn", "project", "query", "role", "savedView"].sort(),
  );
  expect([...sent.get("submit_write")!].sort()).toEqual(["linkTo", "payload"]);
  expect([...sent.get("list_sources")!]).toEqual([]);
  const tricky = codeOnly(`invoke<X>("c", {\n  // the queue's own id\n  q: "a,b}", id }, { headers: {} });`);
  expect(keysSent("c", secondArgument(tricky, tricky.indexOf(",")))).toEqual(["q", "id"]);

  expect(
    await keysRead((args) => {
      const id = args["sourceId"] as { length: number };
      return id.length + (args["limit"] as number);
    }),
  ).toEqual(["sourceId"]);
});

test("every argument a faked handler reads is a camel-cased key the barrel's invoke sends it", async () => {
  // A fixture of its own: calling every write handler with nothing may touch
  // the module state the other tests in this file read.
  vi.resetModules();
  const handlers = (await import("./fake-tauri")).demoHandlers();
  const sent = barrelArguments();

  let reads = 0;
  for (const [cmd, handler] of Object.entries(handlers)) {
    const keys = sent.get(cmd);
    expect(keys, `${cmd} is answered but the barrel never invokes it`).toBeDefined();
    for (const key of keys!) {
      expect(key, `${cmd}'s invoke sends ${key}, which Tauri would not match to a camel-cased argument`).toMatch(
        /^[a-z][A-Za-z0-9]*$/,
      );
    }
    for (const key of await keysRead(handler)) {
      reads++;
      expect(keys!.has(key), `${cmd}'s handler reads ${key}, which the barrel's invoke does not send`).toBe(true);
    }
  }
  expect(reads, "no handler read an argument: the recorder is not recording").toBeGreaterThan(20);
});

// -- the Gitea source as a ticket system (#1248) -------------------------------

test("the fake Gitea source claims ticket-system and its tickets reach the board", () => {
  const handlers = demoHandlers();
  const sources = handlers["list_sources"]!({}) as { id: string; roles: { role: string; reads: string[] }[] }[];
  const gitea = sources.find((source) => source.id === "gitea");
  expect(gitea?.roles.find((claim) => claim.role === "ticket-system")?.reads).toContain("reachable_transitions");

  const board = handlers["mini_board"]!({ role: "ticket-system", ctxId: null, project: null, perColumn: null }) as {
    columns: { status: string | null; cards: { source_id: string; key: string; priority: string | null; assignee: string | null; labels: string[] }[] }[];
    sources: { source_id: string; statuses: string[] }[];
  };
  const cards = board.columns.flatMap((column) => column.cards).filter((card) => card.source_id === "gitea");
  expect(cards.length).toBeGreaterThan(0);
  expect(cards.every((card) => /^tidewater\/[^/]+\/issues\/\d+$/.test(card.key))).toBe(true);
  expect(cards.find((card) => card.key === "tidewater/ledger/issues/57")).toMatchObject({
    priority: "medium",
    assignee: "jonas",
    labels: ["bug", "export"],
  });
  expect(board.sources.find((source) => source.source_id === "gitea")?.statuses.sort()).toEqual(["closed", "open"]);
});

test("the fixture's mini_board reads a saved view by its savedView argument (#1245)", () => {
  const board = demoHandlers()["mini_board"]!({
    sources: [],
    ctxId: null,
    project: null,
    perColumn: null,
    query: null,
    savedView: "jira:board-21",
    lanes: null,
  }) as { columns: { status: string | null }[] };
  // The board's own columns, as `saved_view_board` answers, not one per status.
  expect(board.columns.map((column) => column.status)).toEqual(["Ready", "In flight", "Shipped"]);
});

test("the fixture's mini_board reads a role's fillers: the demo board whole, a miss's source off it, nobody's role empty (#1299)", () => {
  const handlers = demoHandlers();
  type Board = { columns: { status: string | null; total: number; cards: { source_id: string }[] }[]; sources: unknown[] };
  const board = (role: string | null) =>
    handlers["mini_board"]!({ ctxId: null, role, project: null, perColumn: null, query: null, savedView: null, lanes: null }) as Board;
  const drawnBy = (read: Board) => [...new Set(read.columns.flatMap((column) => column.cards.map((card) => card.source_id)))].sort();
  // Every demo source with tickets fills the ticket system: the board is the one every source draws.
  expect(board("ticket-system")).toEqual(board(null));
  // Jira holds three of the mock's keys too (#1476).
  expect(drawnBy(board("ticket-system"))).toEqual(["gitea", "jira", "mock"]);

  const gitea = FIXTURE_SOURCES.find((source) => source.id === "gitea") as { role_misses: { role: string }[] };
  gitea.role_misses = [{ role: "ticket-system" }];
  try {
    const narrowed = board("ticket-system");
    expect(drawnBy(narrowed)).toEqual(["jira", "mock"]);
    const counted = narrowed.columns.reduce((all, column) => all + column.total, 0);
    expect(counted).toBe(narrowed.columns.flatMap((column) => column.cards).length);
  } finally {
    gitea.role_misses = [];
  }
  expect(board("role_nobody:fills-this")).toEqual({ columns: [], sources: [], lanes: [], more_lanes: 0 });
});

test("the fixture's ticket_views carries a saved view's facts under saved_view_source, and none on a view of your own (#1273)", () => {
  const rows = demoHandlers()["ticket_views"]!({}) as { id: string; saved_view: string | null; saved_view_source: { kind: string; source_query: string } | null }[];
  const board = rows.find((row) => row.saved_view === "jira:board-21")!;
  expect(board.saved_view_source).toMatchObject({ kind: "board", source_query: "project in (PAY, OPS) AND labels != tech-debt ORDER BY Rank ASC" });
  expect(rows.find((row) => row.saved_view === null)!.saved_view_source).toBeNull();
  expect(rows.every((row) => !("jira" in row))).toBe(true);
});

test("the fixture has a token source whose credential needs the reader: Uptime Kuma, no token stored (#1405)", () => {
  const handlers = demoHandlers();
  type Source = { id: string; auth_kind: string | null; health: { state: string } };
  const kuma = (handlers["list_sources"]!({}) as Source[]).find((source) => source.id === "kuma")!;
  expect(kuma.auth_kind).toBe("ApiToken");
  expect(kuma.health.state).toBe("missing_secret");
  const health = handlers["credential_health"]!({}) as { source_id: string; state: string }[];
  expect(health.find((row) => row.source_id === "kuma")?.state).toBe("missing_secret");
});

test("the fixture's TeamCity has a run the server never answered, with the long transport error a live one gave (#1478)", () => {
  const handlers = demoHandlers();
  type Run = { id: number; outcome: string | null; error: string | null };
  const runs = handlers["list_sync_runs"]!({ sourceId: "teamcity" }) as Run[];
  const unanswered = runs.filter((run) => run.outcome === "unreachable");
  expect(unanswered).toHaveLength(1);
  expect(unanswered[0]!.error).toMatch(/^source: unreachable: error sending request for url \(https:\/\/ci\.tidewater\.example\/.*%2C/);
  expect(unanswered[0]!.error!.length).toBeGreaterThan(200);
  // Its id is its own: the source's run log keys its rows on it.
  expect(runs.filter((run) => run.id === unanswered[0]!.id)).toHaveLength(1);
});

test("the fixture's install adds the descriptor to list_adapters and add_source adds the row to list_sources (#1406)", async () => {
  // A fixture of its own: the install and the new source are module state the other tests read.
  vi.resetModules();
  const handlers = (await import("./fake-tauri")).demoHandlers();
  type Source = { id: string; adapter_kind: string; health: { source_id: string }; kinds: { id: string }[] };
  const before = (handlers["list_sources"]!({}) as Source[]).length;
  handlers["install_adapter"]!({ manifest: "/releases/knobas-adapter.json" });
  const kinds = (handlers["list_adapters"]!({}) as { adapter_kind: string }[]).map((row) => row.adapter_kind);
  expect(kinds).toContain("wasm-fixture");

  handlers["add_source"]!({
    input: { id: "harbour", adapter_kind: "wasm-fixture", display_name: "Harbour", base_url: "https://surveys.harbour.example", config: { fault: "none" } },
  });
  const sources = handlers["list_sources"]!({}) as Source[];
  expect(sources).toHaveLength(before + 1);
  const harbour = sources.find((source) => source.id === "harbour")!;
  expect(harbour.adapter_kind).toBe("wasm-fixture");
  // Its own health and kinds, not the spread fixture's.
  expect(harbour.health.source_id).toBe("harbour");
  expect(harbour.kinds.map((kind) => kind.id)).toEqual(["survey"]);
  expect(() => handlers["add_source"]!({ input: { id: "harbour", adapter_kind: "wasm-fixture" } })).toThrow();
});

test("the fixture's Jira credential expires inside the amber window from today, so the countdown is drawn (#1499)", () => {
  const handlers = demoHandlers();
  type Source = { id: string; health: { secret_expires_at: string | null } };
  const jira = (handlers["list_sources"]!({}) as Source[]).find((source) => source.id === "jira")!;
  const days = (new Date(jira.health.secret_expires_at!).getTime() - Date.now()) / 86_400_000;
  expect(days).toBeGreaterThan(1);
  expect(days).toBeLessThanOrEqual(14);
});

test("the fixture's Jira source holds PAY-231 as the mock does, each a ticket under its own source's name (#1476)", () => {
  const handlers = demoHandlers();
  type Detail = { row: { entity_id: string; kind: string; source_id: string }; source: { display_name: string } };
  const both = ["mock:PAY-231", "jira:PAY-231"].map((entityId) => handlers["get_entity"]!({ entityId }) as Detail);
  expect(both.map(({ row, source }) => [row.entity_id, row.kind, row.source_id, source.display_name])).toEqual([
    ["mock:PAY-231", "ticket", "mock", "Tidewater (mock)"],
    ["jira:PAY-231", "ticket", "jira", "Tidewater Jira"],
  ]);
  const names = (handlers["list_sources"]!({}) as { id: string; display_name: string }[])
    .filter((source) => source.id === "mock" || source.id === "jira")
    .map((source) => source.display_name);
  expect(new Set(names).size).toBe(2);
});

test("the fixture's search answers a ticket key with every source's ticket of it, and refuses other words as before (#1476)", () => {
  const handlers = demoHandlers();
  const search = (raw: string) => handlers["search"]!({ query: { raw, filters: {} } }) as { groups: { hits: { entity_id: string }[] }[] };
  expect(search("pay-231").groups[0]!.hits.map((hit) => hit.entity_id)).toEqual(["mock:PAY-231", "jira:PAY-231"]);
  expect(() => search("sepa")).toThrow();
});

/**
 * #1417: `>` is not a corpus search, and the real backend answers it with the
 * marker stripped and no groups (`empty_corpus`). The fixture does the same,
 * so the palette draws under `?fake-ipc` instead of the #430 refusal.
 */
test("the fixture's search answers > as the backend does: the text after the marker, no groups", () => {
  const handlers = demoHandlers();
  type Answer = { interpreted: { text: string; prefix: string | null }; groups: unknown[]; total: number };
  const search = (raw: string) => handlers["search"]!({ query: { raw, filters: {} } }) as Answer;
  for (const [raw, text] of [
    [">", ""],
    [">In", "In"],
    ["> set ", "set"],
  ] as const) {
    const answer = search(raw);
    expect(answer.interpreted, raw).toMatchObject({ prefix: "action", text });
    expect(answer.groups, raw).toEqual([]);
    expect(answer.total, raw).toBe(0);
  }
  expect(() => search("sepa >")).toThrow();
});

test("the fixture's resolve_url opens the SEPA page from the forms Confluence declares, not only its stored address (#1475)", () => {
  const resolve = demoHandlers()["resolve_url"]!;
  const page = { entity_id: "confluence:sepa", kind: "page" };
  for (const url of [
    "http://127.0.0.1:5399/confluence/pages/viewpage.action?pageId=sepa",
    "http://127.0.0.1:5399/confluence/display/ENG/SEPA+retry+design",
    "http://127.0.0.1:5399/confluence/display/ENG/SEPA%20retry%20design#comment-3",
  ]) {
    expect(resolve({ url }), url).toEqual(page);
  }
  for (const url of [
    "http://127.0.0.1:5399/confluence/pages/viewpage.action?pageId=nope",
    "http://127.0.0.1:5399/confluence/display/OPS/SEPA+retry+design",
    "http://other.example/confluence/pages/viewpage.action?pageId=sepa",
  ]) {
    expect(resolve({ url }), url).toBeNull();
  }
});

/**
 * The typing half (#1670): `fake-answers.ts` holds one line per barrel command, so a command the
 * barrels gain cannot stay untyped, and a line cannot outlive its command.
 */
test("fake-answers.ts types every barrel command, and only those", () => {
  const source = readFileSync(join(REPO_ROOT, "app/src/lib/shell/dev/fake-answers.ts"), "utf8");
  const typed = [...source.matchAll(/^ {2}([a-z0-9_]+): Answer</gm)].map((match) => match[1]!).sort();
  expect(typed).toEqual([...barrelCommands().keys()].sort());
});
