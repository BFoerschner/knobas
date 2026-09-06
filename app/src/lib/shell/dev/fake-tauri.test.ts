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
const ESTATE_ON_DISK = JSON.parse(
  readFileSync(join(process.cwd(), "../testenv/hetzner/estate.json"), "utf8"),
) as {
  name: string;
  assets: {
    id: string;
    type: string;
    name: string;
    parent?: string;
    description?: string;
    properties?: Record<string, string | number>;
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
const ESTATE_TEXT = readFileSync(join(process.cwd(), "../testenv/hetzner/estate.json"), "utf8");

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
