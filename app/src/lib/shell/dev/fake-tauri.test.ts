/**
 * The fixture's suggestion handlers (#237).
 *
 * The tray's refresh awaits `detect_suggestions` and then `room_suggestions`,
 * and renders a rejection rather than an empty state — so a fixture missing
 * either key put a standing red line on every room under `?fake-ipc`. These
 * tests pin the table itself, and the two properties the tray's own contract
 * rests on: a proposal is a link with `confirmed_at: null`, and a proposal
 * with no reason is not shippable.
 *
 * This file lives under `dev/` on purpose. `house-rules.test.ts` resolves
 * every importer of the dev harness and requires the set to be exactly
 * `App.svelte`, exempting only files inside the harness directory itself;
 * a test one level up would be a second door into the fixture.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "vitest";

import type { AssetDetail, AssetRow } from "../../ipc/assets";
import { columnPathFor, stripFor, wireKey, wiresFor } from "../../assets/tree";

import type { SuggestionPage } from "../../ipc/entity";
import { demoHandlers } from "./fake-tauri";

test("the four commands the tray calls are answered", () => {
  const handlers = demoHandlers();
  for (const cmd of ["detect_suggestions", "room_suggestions", "accept_suggestion", "dismiss_suggestion"]) {
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
    expect(link.origin).toBe("suggested");
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
 * not in holds nothing, and a stored context's room is empty because the
 * fixture has no link graph to resolve its membership over.
 */
test("the page is scoped to the room it is read from", () => {
  const handlers = demoHandlers();
  expect(page(handlers, { ...ROOM, sources: ["mock"] }).total).toBe(page(handlers).total);
  expect(page(handlers, { ...ROOM, sources: ["gitea"] })).toEqual({ rows: [], total: 0 });
  expect(page(handlers, { ...ROOM, ctx: "ctx:fake-1" })).toEqual({ rows: [], total: 0 });
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
 * The estate the fixture answers with is `testenv/hetzner/estate.json` (#440).
 *
 * Read here off the **file on disk**, not off the same import the fixture
 * uses: an assertion that compared the fixture to its own input would agree
 * with itself. What is under test is the mapping the fixture does on the way
 * — `assets::properties_of` and `assets::property_of` in TypeScript — because
 * that is the half a browser screenshot is evidence about. If it is wrong, a
 * QA pass photographs a pane the app would never draw.
 */
const ESTATE_TEXT = readFileSync(join(process.cwd(), "../testenv/hetzner/estate.json"), "utf8");
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
  // a fixture that lost the `parent` mapping would draw all twenty-three here.
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
 * The three servers and their containers — criterion 1's own words — and the
 * property mapping, on the pane of a machine that exercises every branch of
 * it: a declared key the file fills, a declared key it does not, a custom
 * string, a custom **number**, and the `description` that has no column.
 */
test("a server's pane draws the file's properties at the kinds the type declares", () => {
  const handlers = demoHandlers();

  const nbg1 = column(handlers, "asset:hetzner-nbg1");
  expect(nbg1.map((row) => row.name)).toEqual([
    "knobas-confluence",
    "knobas-jira",
    "knobas-teamcity",
  ]);
  // Each of the three holds a Docker engine, and each engine holds containers.
  for (const server of nbg1) {
    const engines = column(handlers, server.id);
    expect(engines).toHaveLength(1);
    expect(column(handlers, engines[0]!.id).length).toBeGreaterThan(0);
  }

  const detail = handlers["get_asset"]!({ assetId: "asset:hetzner-teamcity" }) as {
    properties: { key: string; label: string; value: unknown; custom: boolean }[];
  };
  const file = ESTATE_ON_DISK.assets.find((asset) => asset.id === "asset:hetzner-teamcity")!;
  const at = (key: string) => detail.properties.find((property) => property.key === key);

  // Declared by `vm`, in the type's order, and labelled by the type.
  expect(detail.properties.slice(0, 4).map((property) => property.key)).toEqual([
    "hostname",
    "ip",
    "os",
    "size",
  ]);
  expect(at("os")).toEqual({
    key: "os",
    label: "OS",
    value: { kind: "text", value: file.properties!.os },
    custom: false,
  });
  // Declared and unfilled: `null`, which is the em dash the pane draws.
  expect(at("hostname")!.value).toBeNull();
  // Custom, labelled by its own key, and a number stays a number — the one
  // place `valueOf` reads the JSON's type rather than the schema's.
  expect(at("vcpu")).toEqual({
    key: "vcpu",
    label: "vcpu",
    value: { kind: "number", value: file.properties!.vcpu },
    custom: true,
  });
  // The description has no column in this model, so it is a property.
  expect(at("description")!.value).toEqual({ kind: "text", value: file.description });

  // And the custom rows are in key order, after the declared ones.
  const custom = detail.properties.filter((property) => property.custom).map((p) => p.key);
  expect(custom).toEqual([...custom].sort());
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
 * exposer is four columns to the left and **is** in the layout, and the
 * tunnel's `-R` forward, whose exposer is a Hetzner server in another branch
 * entirely and is in no open column — so one wire is drawn and one is not,
 * which is #433's rule rather than an absence of routes.
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
  ]);

  // The same wire at a window too narrow for five columns: the row is behind a
  // spine, so it lands on the spine and is dashed.
  const narrow = stripFor(columns.length, 400, null);
  expect(wiresFor(detail, columns, narrow)).toEqual([
    { key: wireKey("via", "route:notebook-gitea"), column: 1, rowId: null, dashed: true },
  ]);
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

  const named = ESTATE_ON_DISK.assets.flatMap((asset) => asset.monitors ?? []);
  expect(named.length).toBeGreaterThan(0);
  expect(roster.map((row) => row.name).sort()).toEqual([...named].sort());

  // Every chip has something in it: a roster where everything is `up` can be
  // photographed without showing that the chips do anything.
  const chips = new Set(roster.map((row) => (row.tombstoned ? "paused" : row.state)));
  for (const state of ["up", "warn", "down", "pending", "paused", "maintenance"]) {
    expect(chips.has(state), `nothing in the fixture is ${state}`).toBe(true);
  }

  for (const row of roster) {
    expect(row.state, `${row.name} is not its last sample`).toBe(row.samples.at(-1)?.state ?? null);
    expect(row.assets.length, `${row.name} watches nothing`).toBeGreaterThan(0);
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
      count: number;
      changed: boolean;
      description: string;
      saved: boolean;
    }[];
    recent: unknown[];
    sources: unknown[];
    pending_writes: number;
  };

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
  // A badge is a comparison against a stamp this fixture cannot keep.
  expect(board.smart_lists.every((list) => list.changed === false)).toBe(true);
  expect(board.recent.length).toBeGreaterThan(0);
  expect(board.sources.length).toBeGreaterThan(0);

  // And the rows behind the counts: assets, with the count the rail promised.
  for (const id of ["not-monitored", "alerts-in-context", "certs-expiring"]) {
    const response = handlers["smart_list_items"]!({ id, limit: 20 }) as {
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
  expect(() => handlers["smart_list_items"]!({ id: "nope", limit: 20 })).toThrow();
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

  const rows = handlers["smart_list_items"]!({ id: "certs-expiring", limit: 20 }) as {
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
