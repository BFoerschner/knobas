/**
 * `?fake-ipc`: The estate (#428, #429): the asset tree, an asset's pane, its writes,
 * routes and dependants, read from `testenv/estate.json`.
 */
// The estate itself. See `FIXTURE_ESTATE` below for why it is read and not
// copied, and for the check that it reaches no production bundle.
import ESTATE_FILE from "../../../../../../testenv/estate.json";
import type { AssetProperty, AssetRow, DependsOnRow, PropertyValue } from "../../../ipc/assets";
import type { Handlers } from "../fake-tauri";
import { keepSeed, onFakeReset } from "../fake-seed";
import { SYNCED_AT } from "./corpus";

// -- the estate -------------------------------------------------------------

/**
 * A corner of the built-in type table -- `asset_types`' answer here.
 *
 * The types the estate below uses and the ones they suggest, with the ids,
 * monograms, schemas and `suggests` lists `knobas_core::asset` declares.
 * **Not all sixteen**: this is a fixture a browser is pointed at, and nine
 * types is what it takes to see a *usual here* line and a typed-property
 * editor. The `suggests` lists are nonetheless the **whole** lists the real
 * table declares, so one of them names a type this subset omits (`container`
 * suggests `runtime`). That is on purpose: `typeChoices` drops ids the list it
 * is given does not carry, so a full copy costs nothing and a trimmed one
 * would be a second list disagreeing with the first -- which is the drift this
 * fixture already had once.
 *
 * It stays a copy where the estate below stopped being one (#440). The estate
 * is a **file**, and a file can be imported; the table is a `const` in Rust,
 * and the only way for TypeScript to have it is for something to write it
 * down. What holds this copy to the real one is
 * `the fixture answers the estate file, entry for entry` in
 * `fake-tauri.test.ts`: every type the file names has to be here, with the
 * monogram and the property schema the pane draws off it -- so a table that
 * drifted from the estate it serves fails rather than draws a `??` chip.
 *
 * It has to be declared **before** the estate, because the estate is built by
 * reading it at module load rather than written out beside it: a `const` read
 * during its own file's initialisation is a temporal-dead-zone error, not a
 * `undefined`.
 */
const ASSET_TYPES: {
  id: string;
  label: string;
  monogram: string;
  properties: { key: string; label: string; kind: "text" | "number" | "date" | "url" }[];
  suggests: string[];
}[] = [
  {
    id: "site",
    label: "Site",
    monogram: "SI",
    properties: [
      { key: "location", label: "Location", kind: "text" },
      { key: "provider", label: "Provider", kind: "text" },
    ],
    suggests: ["site", "hypervisor", "vm", "network"],
  },
  {
    id: "hypervisor",
    label: "Hypervisor",
    monogram: "HV",
    properties: [
      { key: "hostname", label: "Hostname", kind: "text" },
      { key: "ip", label: "IP", kind: "text" },
      { key: "os", label: "OS", kind: "text" },
    ],
    suggests: ["vm"],
  },
  {
    id: "vm",
    label: "VM",
    monogram: "VM",
    properties: [
      { key: "hostname", label: "Hostname", kind: "text" },
      { key: "ip", label: "IP", kind: "text" },
      { key: "os", label: "OS", kind: "text" },
      { key: "size", label: "Size", kind: "text" },
    ],
    suggests: ["container_engine", "service", "database_server", "reverse_proxy"],
  },
  {
    id: "container_engine",
    label: "Container engine",
    monogram: "CE",
    properties: [
      { key: "version", label: "Version", kind: "text" },
      { key: "socket", label: "Socket", kind: "text" },
    ],
    suggests: ["container"],
  },
  {
    id: "container",
    label: "Container",
    monogram: "CT",
    properties: [
      { key: "image", label: "Image", kind: "text" },
      { key: "ports", label: "Ports", kind: "text" },
      { key: "restart_policy", label: "Restart policy", kind: "text" },
    ],
    suggests: ["service", "database", "runtime"],
  },
  {
    id: "service",
    label: "Service",
    monogram: "SV",
    properties: [
      { key: "url", label: "URL", kind: "url" },
      { key: "port", label: "Port", kind: "number" },
      { key: "health_path", label: "Health path", kind: "text" },
    ],
    suggests: ["module"],
  },
  {
    id: "database_server",
    label: "Database server",
    monogram: "DS",
    properties: [
      { key: "engine", label: "Engine", kind: "text" },
      { key: "version", label: "Version", kind: "text" },
      { key: "host", label: "Host", kind: "text" },
      { key: "port", label: "Port", kind: "number" },
    ],
    suggests: ["database"],
  },
  {
    id: "database",
    label: "Database",
    monogram: "DB",
    properties: [
      { key: "engine", label: "Engine", kind: "text" },
      { key: "size_mb", label: "Size (MB)", kind: "number" },
    ],
    suggests: ["schema"],
  },
  { id: "custom", label: "Custom", monogram: "CU", properties: [], suggests: [] },
];

/**
 * One asset in the estate file, at the shape `assets::EstateFile` parses.
 *
 * The section below is the fixture's estate, and it is the **real** one, read
 * out of the file rather than copied into this one.
 *
 * `testenv/estate.json` is the estate as it runs (#438, #634), and since
 * #440 it is the file `--demo` imports -- so a browser pointed at `?fake-ipc`
 * and a person looking at `just demo` are looking at the same sixteen
 * assets and the same nine routes. This module's own docs have said since #428
 * that *"when that file lands, this can read it instead"*, and this is that.
 *
 * The hand-copied corner it replaces is why: five levels of the estate written
 * out by hand had already drifted into a machine the estate does not have
 * (`hel1`, a site holding two VMs) and a container typed as a VM. A copy of a
 * file that is checked in beside it earns nothing and goes stale, and the
 * shortening it needed -- routes re-pointed at the assets the copy stopped
 * short of -- is exactly the kind of quiet difference a QA screenshot then
 * reports as the app's behaviour.
 *
 * Vite resolves the import and Rollup drops it together with this module in a
 * production build, where `import.meta.env.DEV` is `false` and the only door
 * to this file is behind it (`house-rules.test.ts` holds that door). Checked:
 * `grep -c "knobas test estate" dist/assets/*.js` is `0`.
 *
 * What is **not** read off the file is a **status**, because the file carries
 * none and `assets::apply_import` writes none: every row here is `"none"`, and
 * the *N problems inside* badge is therefore absent -- which is what the demo
 * profile actually shows. #431's badge has its own fixtures in
 * `AssetsView.test.svelte.ts`.
 */
export interface EstateFileAsset {
  id: string;
  type: string;
  name: string;
  parent?: string;
  environment?: string;
  owner?: string;
  description?: string;
  properties?: Record<string, string | number>;
  monitors?: string[];
}

/** One route in the file: what exposes it, and what it lands on. */
export interface EstateFileRoute {
  id: string;
  asset: string;
  target?: string;
  name: string;
  url: string;
  description?: string;
  properties?: Record<string, string | number>;
}

/**
 * The file, at the shape `assets::EstateFile` parses it into.
 *
 * Declared here rather than inferred. TypeScript's inference over an imported
 * JSON array is a union of one object type per *distinct set of keys* the file
 * happens to use, so `entry.parent` is a compile error on the one asset that
 * has no parent and the union's shape changes the day somebody gives a second
 * asset an owner. The cast is the seam, and it is narrow on purpose: the
 * authority on this file is the Rust parser, and what holds the file to a
 * shape is `knobas-core`'s `tests/it/estate_file.rs` -- eleven tests over the
 * checked-in bytes, in `just check`.
 */
export const ESTATE = ESTATE_FILE as unknown as {
  name: string;
  assets: EstateFileAsset[];
  routes: EstateFileRoute[];
};

/** One asset as this fixture holds it: the wire's row, plus its properties. */
interface FixtureAsset {
  id: string;
  parent_id: string | null;
  type_id: string;
  type_label: string;
  monogram: string;
  name: string;
  status: "up" | "warn" | "down" | "none";
  environment: "dev" | "stage" | "prod" | "shared" | null;
  owner: string | null;
  properties: AssetProperty[];
  /** The Uptime Kuma names an import kept on the asset (#439). */
  monitors: string[];
}

/** One route as this fixture holds it. */
interface FixtureRoute {
  id: string;
  asset_id: string;
  target_id: string | null;
  name: string;
  url: string;
  visibility: "internal" | "public";
  properties: AssetProperty[];
  /** The monitor names a create recorded on the route (#976), which the Monitors tab reads its create on its way by (#1959). */
  monitors?: string[];
}

/**
 * The file's property bag: its `properties`, plus its `description`.
 *
 * `assets::bag_of`'s rule. A description has no column of its own in this
 * model, so where it goes is the property bag -- and every asset in the real
 * estate has one, which is why the pane's first custom row is `description`
 * on nearly every asset the demo draws.
 */
function bagOf(entry: {
  description?: string;
  properties?: Record<string, string | number>;
}): Record<string, string | number> {
  const bag: Record<string, string | number> = { ...(entry.properties ?? {}) };
  if (entry.description !== undefined) bag.description = entry.description;
  return bag;
}

/**
 * One plain scalar as the tagged value the pane draws.
 *
 * `assets::property_of`'s rule: a **declared** key takes the kind its type
 * declares, an **undeclared** one takes the kind JSON already gave it. A rule
 * and not a table, for this module's stated reason -- a fixture that answered
 * with values somebody typed out would draw the right words over a pane
 * reading the wrong field.
 */
function valueOf(kind: PropertyValue["kind"] | null, raw: string | number | undefined): PropertyValue | null {
  if (raw === undefined) return null;
  return { kind: kind ?? (typeof raw === "number" ? "number" : "text"), value: raw } as PropertyValue;
}

/** The bag's keys, as the pane's custom rows: by key, labelled by the key. */
function customPropertiesOf(bag: Record<string, string | number>) {
  return Object.keys(bag)
    .sort()
    .map((key) => ({ key, label: key, value: valueOf(null, bag[key]), custom: true }));
}

/**
 * `assets::properties_of`: the type's declared keys in the type's own order,
 * unfilled ones included, then whatever else the bag holds, by key.
 */
function propertiesOf(typeId: string, bag: Record<string, string | number>) {
  const declared = typeOf(typeId).properties;
  const claimed = new Set(declared.map((property) => property.key));
  const rest = Object.fromEntries(
    Object.entries(bag).filter(([key]) => !claimed.has(key)),
  );
  return [
    ...declared.map((property) => ({
      key: property.key,
      label: property.label,
      value: valueOf(property.kind, bag[property.key]),
      custom: false,
    })),
    ...customPropertiesOf(rest),
  ];
}

/**
 * The table's entry for `typeId`.
 *
 * A type the table does not carry falls back to its own id rather than
 * throwing: the fixture's table is a subset, and a file naming a type it omits
 * should draw an unfamiliar chip rather than a blank screen.
 */
export function typeOf(typeId: string) {
  return (
    ASSET_TYPES.find((type) => type.id === typeId) ?? {
      id: typeId,
      label: typeId,
      monogram: "??",
      properties: [] as { key: string; label: string; kind: PropertyValue["kind"] }[],
      suggests: [] as string[],
    }
  );
}

/** The four environments `0017` accepts. Anything else is left unset. */
const ENVIRONMENTS = ["dev", "stage", "prod", "shared"] as const;

/** One of the file's assets as the fixture holds it. */
export function assetFromFile(entry: EstateFileAsset): FixtureAsset {
  const declared = typeOf(entry.type);
  return {
    id: entry.id,
    parent_id: entry.parent ?? null,
    type_id: entry.type,
    type_label: declared.label,
    monogram: declared.monogram,
    name: entry.name,
    status: "none",
    environment: ENVIRONMENTS.find((name) => name === entry.environment) ?? null,
    owner: entry.owner ?? null,
    properties: propertiesOf(entry.type, bagOf(entry)),
    monitors: entry.monitors ?? [],
  };
}

/**
 * The estate the Tree draws: sixteen assets, five levels at the deepest.
 *
 * Stateful within the session, like the contexts: an asset created, renamed,
 * moved or deleted here stays that way for as long as the page lives, so the
 * whole create / edit / move walk can be driven in a browser. Nothing is
 * persisted -- reload and the estate is the file again.
 */
export const FIXTURE_ESTATE: FixtureAsset[] = ESTATE.assets.map(assetFromFile);

/**
 * The ids the file itself names.
 *
 * What it is for is the history: an asset that arrived by import carries an
 * origin line and an asset somebody made in this session does not, and after
 * a `create_asset` the two live in the same array.
 */
const FILE_ASSET_IDS = new Set(ESTATE.assets.map((entry) => entry.id));

/**
 * One of the file's routes as the fixture holds it.
 *
 * A route declares no schema, so every property is a custom row --
 * `assets::route_row_of` calls `custom_properties` and not `properties_of`.
 *
 * **The URL is the file's, verbatim, and one of the nine does not name a
 * loopback address.** `route:compose-gitea` is Gitea on the compose network,
 * and the host in its URL is `gitea` -- the docker network name the TeamCity
 * containers resolve, which is why the estate spells it that way and why it
 * resolves nowhere else. The hand-copy this replaces substituted a loopback
 * address for it, citing `house-rules.test.ts`'s *no runtime network
 * references* rule, and dropping that substitution is a deliberate change
 * rather than an oversight.
 *
 * That rule scans `.ts` and `.svelte` under `app/src/` -- **raw text,
 * comments included, which is why this paragraph does not spell the URL with
 * its scheme** -- and it exists so that a *bundle* reaches nothing but the
 * IPC. The estate file is in no bundle: `import.meta.env.DEV` is the only door
 * to this module, and `grep -c "knobas test estate" dist/assets/*.js` is `0`.
 * More to the point, a route's URL is **data**. In the running app it arrives
 * over the bridge out of `knobas.route` and is whatever a person typed or an
 * import wrote, and this fixture stands in for that table -- so a harness that
 * quietly improved one URL would be drawing a screen the app does not draw.
 */
export function routeFromFile(route: EstateFileRoute): FixtureRoute {
  return {
    id: route.id,
    asset_id: route.asset,
    target_id: route.target ?? null,
    name: route.name,
    url: route.url,
    visibility: "internal",
    properties: customPropertiesOf(bagOf(route)),
  };
}

/**
 * The nine routes the estate exposes (#432, drawn as wires by #433): the eight
 * ports the notebook publishes -- three of them the on-demand products' -- and
 * Gitea's name on the compose network, exposed by the Docker engine. (Until
 * #634 three of them were SSH tunnel forwards to Hetzner servers, and one ran
 * back from a server to Gitea.)
 *
 * They are what makes the Tree's wires visible in a browser at all: a wire
 * runs from a route's row to the row (or the spine) of the asset at its far
 * end, so a fixture with no routes draws none, whatever the code does. Every
 * one of these lands on an asset one to three columns away, which is the case
 * a same-asset route could not photograph.
 */
export const FIXTURE_ROUTES: FixtureRoute[] = ESTATE.routes.map(routeFromFile);

/** One route on the wire -- the exposing and target names read off the estate. */
function routeRow(route: FixtureRoute) {
  const named = (id: string | null) =>
    id === null ? null : (FIXTURE_ESTATE.find((asset) => asset.id === id)?.name ?? null);
  return {
    ...route,
    asset_name: named(route.asset_id) ?? route.asset_id,
    target_name: named(route.target_id),
  };
}

/**
 * `assets::ROUTES_REACHABLE`'s rule: every route whose target is on this
 * asset's containment path, above it or below.
 *
 * The **rule** and not one of its answers, like `inForce` below: a hard-coded
 * list would draw the right route on the wrong pane, and the pane is what a
 * QA walk is looking at.
 */
function routesReaching(asset: FixtureAsset, heldBy: FixtureAsset[]) {
  const under = new Set(descendants(asset).map((held) => held.id));
  const path = new Set([asset.id, ...heldBy.map((held) => held.id)]);
  return FIXTURE_ROUTES.filter(
    (route) => route.target_id !== null && (path.has(route.target_id) || under.has(route.target_id)),
  ).map(routeRow);
}

/** The two statuses `assets::ROLLUP` counts as a problem inside. */
const PROBLEM: FixtureAsset["status"][] = ["warn", "down"];

/** `AssetStatus::severity` -- down over warn over up over none, worst first. */
const SEVERITY: Record<FixtureAsset["status"], number> = { down: 0, warn: 1, up: 2, none: 3 };

/**
 * The estate by id and by parent, built at most once per command (#2221).
 *
 * A row's rollup walks its subtree, and a column answers many rows, so a walk
 * that filtered the whole fixture per node cost the square of the estate: 1.5 s
 * of fake time for 5,000 VMs under one host. Every command here starts without
 * one ({@link fresh}), so a read never sees an estate older than its own
 * command: the writes, here and in `estate-import.ts`, are commands of their
 * own. A command that reads the index and then writes would have to call
 * {@link estateChanged} before reading again; none does.
 */
let index: { byId: Map<string, FixtureAsset>; held: Map<string | null, FixtureAsset[]> } | null = null;
/** How many times {@link estateIndex} has scanned the whole fixture: what a test witnesses the reuse by. */
export let estateScans = 0;
function estateIndex() {
  if (index !== null) return index;
  estateScans += 1;
  const byId = new Map<string, FixtureAsset>();
  const held = new Map<string | null, FixtureAsset[]>();
  for (const asset of FIXTURE_ESTATE) {
    byId.set(asset.id, asset);
    const siblings = held.get(asset.parent_id);
    if (siblings) siblings.push(asset);
    else held.set(asset.parent_id, [asset]);
  }
  index = { byId, held };
  return index;
}
/** Drop the index: the estate may have changed since it was built. */
function estateChanged() {
  index = null;
}
onFakeReset(() => {
  index = null;
});

/** What `id` holds directly, in the fixture's order. */
function heldBy(id: string): FixtureAsset[] {
  return estateIndex().held.get(id) ?? [];
}

/** Everything under `asset`, over the parent field. */
function descendants(asset: FixtureAsset): FixtureAsset[] {
  return heldBy(asset.id).flatMap((child) => [child, ...descendants(child)]);
}

/** The worst of a set of statuses, `"none"` for an empty one. */
function worst(statuses: FixtureAsset["status"][]): FixtureAsset["status"] {
  return statuses.reduce(
    (so_far, next) => (SEVERITY[next] < SEVERITY[so_far] ? next : so_far),
    "none",
  );
}

/** The activity lines this session's writes have appended, newest last. */
export const ASSET_HISTORY: {
  entity_id: string;
  verb: string;
  detail: unknown;
  /** `"user"` unless the line was written by an import. */
  actor?: string;
}[] = [];

/** Ids for the assets a QA walk creates. `assets::create` mints a UUID. */
let mintedAssets = 0;

/** The stored asset, or a `not_found` in the shape `IpcError` puts on the wire. */
function assetOr404(id: string): (typeof FIXTURE_ESTATE)[number] {
  const found = FIXTURE_ESTATE.find((row) => row.id === id);
  if (!found) throw { code: "not_found", message: `no asset ${id}`, source_id: null };
  return found;
}

/**
 * `create_asset`: a new asset under `parentId`, with the type's schema drawn
 * as unfilled rows.
 *
 * The id is minted here, the way `assets::create` mints one: the caller names
 * the thing and never its address (story 18).
 */
function createAsset(args: Record<string, unknown>) {
  const typeId = args.typeId as string;
  const name = String(args.name ?? "").trim();
  if (name === "") throw { code: "invalid", message: "an asset needs a name", source_id: null };
  const declared = ASSET_TYPES.find((type) => type.id === typeId);
  if (!declared) {
    throw {
      code: "invalid",
      message: `"${typeId}" is not one of the built-in asset types`,
      source_id: null,
    };
  }
  mintedAssets += 1;
  const created = {
    id: `asset:new-${mintedAssets}`,
    parent_id: (args.parentId as string | null) ?? null,
    type_id: typeId,
    type_label: declared.label,
    monogram: declared.monogram,
    name,
    status: "none" as const,
    environment: null,
    owner: null,
    properties: declared.properties.map((property) => ({
      key: property.key,
      label: property.label,
      value: null,
      custom: false,
    })),
    // None, and that is the honest answer: only an import puts a monitor name
    // on an asset (#439), and creating one by hand is the other door.
    monitors: [],
  };
  FIXTURE_ESTATE.push(created);
  ASSET_HISTORY.push({ entity_id: created.id, verb: "created", detail: {} });
  return assetRow(created);
}

/** `edit_asset`: the two edits this fixture's surface can make. */
function editAsset(args: Record<string, unknown>) {
  const asset = assetOr404(args.assetId as string);
  const edits = (args.edits ?? []) as Record<string, unknown>[];
  for (const edit of edits) {
    if (edit.field === "name") {
      const from = asset.name;
      asset.name = String(edit.value ?? "").trim();
      ASSET_HISTORY.push({
        entity_id: asset.id,
        verb: "renamed",
        detail: { field: "name", from, to: asset.name },
      });
      continue;
    }
    if (edit.field === "monitors") {
      // The append `assets::AssetEdit::Monitors` makes, duplicates dropped:
      // the name is what attaches the monitor, so a fixture that stored it
      // twice would draw two waiting entries for one check (#453).
      const added = ((edit.added ?? []) as string[])
        .map((name) => name.trim())
        .filter((name) => name !== "" && !asset.monitors.includes(name));
      if (added.length === 0) continue;
      asset.monitors.push(...added);
      ASSET_HISTORY.push({
        entity_id: asset.id,
        verb: "edited",
        detail: { field: "monitors", added },
      });
      continue;
    }
    if (edit.field !== "property") continue;
    const key = String(edit.key);
    const at = asset.properties.findIndex((property) => property.key === key);
    const from = at === -1 ? null : asset.properties[at]!.value;
    if (edit.value === null) {
      // A declared key keeps its row and loses its value; a custom key goes.
      if (at !== -1 && asset.properties[at]!.custom) asset.properties.splice(at, 1);
      else if (at !== -1) asset.properties[at]!.value = null;
    } else if (at === -1) {
      asset.properties.push({ key, label: key, value: edit.value as PropertyValue, custom: true });
    } else {
      asset.properties[at]!.value = edit.value as PropertyValue;
    }
    ASSET_HISTORY.push({
      entity_id: asset.id,
      verb: "edited",
      detail: { field: "property", key, from, to: edit.value ?? null },
    });
  }
  return assetRow(asset);
}

/**
 * `move_asset`, cycle refusal included.
 *
 * The refusal is in `assets::move_to`'s own words, naming both ends and the
 * asset that closes the loop -- so a QA walk that tries the move a person
 * would try sees the sentence the app really answers with.
 */
function moveAsset(args: Record<string, unknown>) {
  const asset = assetOr404(args.assetId as string);
  const parentId = (args.newParentId as string | null) ?? null;
  if (parentId !== null) {
    let below: string | null = null;
    for (let walk: string | null = parentId; walk !== null; ) {
      const step = FIXTURE_ESTATE.find((row) => row.id === walk);
      if (!step) break;
      if (step.id === asset.id) {
        throw {
          code: "invalid",
          message:
            `moving "${asset.name}" under "${assetOr404(parentId).name}" would make a cycle: ` +
            `"${below ?? step.name}" is already held by "${asset.name}"`,
          source_id: null,
        };
      }
      below = step.name;
      walk = step.parent_id;
    }
  }
  asset.parent_id = parentId;
  ASSET_HISTORY.push({
    entity_id: asset.id,
    verb: "moved",
    detail: { field: "parent", to: parentId },
  });
  return assetRow(asset);
}

/** `delete_asset`: a leaf goes, a branch is refused by name. */
function deleteAsset(args: Record<string, unknown>) {
  const asset = assetOr404(args.assetId as string);
  const held = FIXTURE_ESTATE.filter((row) => row.parent_id === asset.id).length;
  if (held > 0) {
    throw {
      code: "conflict",
      message: `"${asset.name}" still holds ${held} asset(s) -- move or delete them first`,
      source_id: null,
    };
  }
  // `assets::delete`'s two route rules (#432), because a QA walk that deleted
  // an exposer here and not in the app would be reading the wrong app: a route
  // still exposed refuses the delete, and a route still pointing at it is
  // cleared rather than left dangling.
  const exposed = FIXTURE_ROUTES.filter((route) => route.asset_id === asset.id).length;
  if (exposed > 0) {
    throw {
      code: "conflict",
      message: `"${asset.name}" still exposes ${exposed} route(s) -- delete them first`,
      source_id: null,
    };
  }
  for (const route of FIXTURE_ROUTES) {
    if (route.target_id === asset.id) route.target_id = null;
  }
  FIXTURE_ESTATE.splice(FIXTURE_ESTATE.indexOf(asset), 1);
  return null;
}

/**
 * One asset as a column row -- everything but its properties and its path,
 * plus the rollup `assets::ROLLUP` computes (#431).
 */
function assetRow(asset: FixtureAsset): AssetRow {
  const { properties: _properties, ...row } = asset;
  const under = descendants(asset);
  const inside = worst(under.map((held) => held.status));
  return {
    ...row,
    has_children: heldBy(asset.id).length > 0,
    health: worst([asset.status, inside]),
    inside,
    problems_inside: under.filter((held) => PROBLEM.includes(held.status)).length,
    // Zero for the same reason `links` below is empty: no link graph here, so
    // no asset is linked to any work and no row carries the badge (#435).
    linked_work: 0,
  };
}

/**
 * `assets::inherited`: the nearest asset at or above that sets a value.
 *
 * A copy of the store's **rule**, not of one of its answers, for this module's
 * stated reason: a fixture that returned "hel1" as the source because someone
 * typed it would draw the right words over a pane reading the wrong field.
 * Answering the way the store answers is what makes a `?fake-ipc` screenshot
 * evidence about the view.
 *
 * `heldBy` arrives outermost first, so the walk is the asset and then that
 * list reversed -- nearest ancestor first.
 */
function inForce<T>(
  asset: FixtureAsset,
  heldBy: FixtureAsset[],
  of: (row: FixtureAsset) => T | null,
) {
  for (const at of [asset, ...[...heldBy].reverse()]) {
    const value = of(at);
    if (value !== null) return { value, source_id: at.id, source_name: at.name };
  }
  return null;
}

/**
 * `?fake-assets=<n>` (perf audit, #2200's class): a top-level *Bulk rack*
 * holding `n` VMs, every twentieth one down, added at load. Its column is
 * answered from rows built once, so the fake's own walk stays out of a
 * measurement.
 */
const BULK_RACK = "asset:bulk-rack";
function bulkAssets(n: number): FixtureAsset[] {
  if (n <= 0) return [];
  const rack: FixtureAsset = { id: BULK_RACK, parent_id: null, type_id: "host", type_label: "Host", monogram: "HO", name: "Bulk rack", status: "warn", environment: "prod", owner: null, properties: [], monitors: [] };
  return [
    rack,
    ...Array.from({ length: n }, (_, i): FixtureAsset => ({
      id: `asset:bulk-${i}`, parent_id: BULK_RACK, type_id: "vm", type_label: "VM", monogram: "VM", name: `bulk-vm-${String(i).padStart(5, "0")}`,
      status: i % 20 === 0 ? "down" : "up", environment: null, owner: null, properties: [], monitors: [],
    })),
  ];
}
const BULK_ASSETS = bulkAssets(Number(new URLSearchParams(globalThis.location?.search).get("fake-assets") ?? 0) || 0);
FIXTURE_ESTATE.push(...BULK_ASSETS);
const BULK_COLUMN = BULK_ASSETS.slice(1).map(
  ({ properties: _properties, monitors: _monitors, ...vm }) => ({ ...vm, monitors: [], has_children: false, health: vm.status, inside: "none", problems_inside: 0, linked_work: 0 }) as unknown as AssetRow,
);

/**
 * `asset_tree`: what one asset holds, or the top of the estate.
 *
 * `name` then `id`, which is `assets::CHILDREN`'s `order by a.name asc, a.id
 * asc`: without the tiebreak two siblings sharing a name would come back in
 * whichever order the array happened to hold them, and a QA screenshot would
 * disagree with the app for a reason nobody would look for.
 */
function assetColumn(args: Record<string, unknown>) {
  const parent = (args.parentId as string | null) ?? null;
  if (parent === BULK_RACK && BULK_COLUMN.length > 0) return BULK_COLUMN;
  return FIXTURE_ESTATE.filter((asset) => asset.parent_id === parent)
    .sort((left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id))
    .map(assetRow);
}

/** `get_asset`: the pane's read, ancestors walked over the parent field. */
function assetDetail(args: Record<string, unknown>) {
  const id = args.assetId as string;
  const asset = FIXTURE_ESTATE.find((row) => row.id === id);
  if (!asset) throw { code: "not_found", message: `no asset ${id}`, source_id: null };

  const heldBy: FixtureAsset[] = [];
  let walk = asset.parent_id;
  while (walk !== null) {
    const held = FIXTURE_ESTATE.find((row) => row.id === walk);
    if (!held) break;
    heldBy.unshift(held);
    walk = held.parent_id;
  }

  return {
    asset: assetRow(asset),
    properties: asset.properties,
    effective_environment: inForce(asset, heldBy, (row) => row.environment),
    effective_owner: inForce(asset, heldBy, (row) => row.owner),
    held_by: heldBy.map(assetRow),
    holds: asset.id === BULK_RACK && BULK_COLUMN.length > 0 ? BULK_COLUMN : FIXTURE_ESTATE.filter((row) => row.parent_id === asset.id).map(assetRow),
    exposes: FIXTURE_ROUTES.filter((route) => route.asset_id === asset.id).map(routeRow),
    reachable_via: routesReaching(asset, heldBy),
    // Empty for the mirror detail's reason: this fixture has no link graph,
    // so an asset is linked to nothing and the pane draws the links panel's
    // empty state. Absent rather than empty (#435 landed the field and the
    // panel; the fixture kept neither) it is `undefined`, and `groupLinks`
    // throws on the pane of every asset.
    links: [],
    // The monitor names an import kept (#439), for `links`' reason exactly:
    // the pane reads `detail.monitors.length`, so a fixture short of the key
    // throws on the pane of every asset rather than drawing nothing.
    monitors: asset.monitors,
    // Nothing is *attached*, and that is the honest answer: an attachment is a
    // `monitored-by` link to a mirrored monitor, this fixture has no link
    // graph and no mirror, so every name the file gave is still a name (#445).
    // The pane therefore draws the waiting list and no *Monitoring* section.
    monitoring: [],
    // **One target**, so `?fake-ipc` can walk *Create monitor for this asset*
    // (#453). The fixture's Uptime Kuma is the one a reader configured with an
    // account -- the absent direction has its own witness in
    // `AssetsView.monitor.test.svelte.ts`, and a fixture that answered nothing
    // here would leave the control unreachable in the browser.
    monitor_targets: [
      { source_id: "kuma", display_name: "Uptime Kuma", entity: "kuma:monitors" },
    ],
    history: [
      // This session's own writes first, newest first, which is what makes
      // *every mutation appears in the pane's history immediately* (#429)
      // something a QA walk can see rather than take on trust.
      ...ASSET_HISTORY.filter((line) => line.entity_id === asset.id)
        .map((line, index) => ({ id: 1000 + index, at: SYNCED_AT, actor: "user", ...line }))
        .reverse(),
      // The line every asset **the file names** carries, and the only one: it
      // arrived by import, and `assets::insert_asset` writes no `created` line
      // beside the origin line (#439, story 23). The actor is `import` and not
      // `user`, which is the whole of `assets::HAND_EDITED`'s question -- a
      // fixture that said `user` here would draw a pane in which every
      // property is hand-edited and frozen against the next import. An asset
      // made in this session by hand has a `created` line of its own instead,
      // pushed by `createAsset`, and no origin line, because nothing imported
      // it.
      ...(FILE_ASSET_IDS.has(asset.id)
        ? [
            {
              id: 1,
              at: SYNCED_AT,
              actor: "import",
              verb: "imported",
              entity_id: asset.id,
              detail: { estate: ESTATE.name },
            },
          ]
        : []),
    ],
  };
}

/**
 * The `depends-on` and `runs-on` links this fixture pretends somebody drew.
 *
 * **Fixture-only.** The estate file is a tree and its routes and carries no
 * link at all, so without these three rows the *Depends on this* panel (#505)
 * would show containment on every asset in the browser and the two relation
 * words would be unwalkable. They exist nowhere else: no seed writes them, no
 * import produces them, and the panel's real witness is
 * `crates/knobas-app/tests/it/assets_ipc.rs` over a scratch PostgreSQL, where the
 * links are drawn by the test.
 *
 * `from` depends on / runs on `to`, which is the direction the read walks:
 * what breaks when `to` goes down is `from`.
 */
const FIXTURE_DEPENDENCIES: { from: string; to: string; relation: string }[] = [
  // TeamCity's VCS roots are the Gitea repositories it reaches through
  // `route:compose-gitea`, which is in the file.
  { from: "asset:knobas-teamcity", to: "asset:knobas-gitea", relation: "depends-on" },
  // An agent is no use without the server it registers with.
  { from: "asset:knobas-teamcity-agent", to: "asset:knobas-teamcity", relation: "depends-on" },
  // The mockd container answers the loopback ports the laptop's suites use.
  { from: "asset:notebook", to: "asset:knobas-mockd", relation: "runs-on" },
];

/**
 * `depends_on_this`: what breaks if this asset goes down (#505).
 *
 * `assets::depends_on_this`' **rule** and not one of its answers, the
 * discipline `inForce` above states: breadth-first over containment and over
 * {@link FIXTURE_DEPENDENCIES}, with a visited set, so a cycle drawn into that
 * list would walk once here as it does in the store. The relation is `null`
 * for a containment step, which the pane reads as *inside*.
 *
 * The routes are the ones landing on **this asset or anything that breaks
 * with it** (#1553), listed apart and not counted -- a different read from
 * `reachable_via` above, which takes the whole containment path, upwards
 * included.
 */
function dependsOnThis(args: Record<string, unknown>) {
  const id = args.assetId as string;
  if (!FIXTURE_ESTATE.some((row) => row.id === id)) {
    throw { code: "not_found", message: `no asset ${id}`, source_id: null };
  }

  const seen = new Set([id]);
  let frontier = [id];
  const assets: DependsOnRow[] = [];
  while (frontier.length > 0) {
    const reached = new Map<string, string | null>();
    /**
     * The store's `distinct on (dst) ... order by dst, relation asc nulls
     * first`, over the **whole** step and not over one frontier entry:
     * containment beats a link, and two links between the same pair settle by
     * the relation's own text. Written out rather than first-writer-wins,
     * because the order the frontier happens to be in is not a rule.
     */
    const claim = (dst: string, relation: string | null) => {
      if (!reached.has(dst)) return void reached.set(dst, relation);
      const held = reached.get(dst) ?? null;
      if (held === null || relation === null) return void reached.set(dst, null);
      if (relation < held) reached.set(dst, relation);
    };
    for (const from of frontier) {
      for (const held of heldBy(from)) claim(held.id, null);
      for (const edge of FIXTURE_DEPENDENCIES.filter((link) => link.to === from)) {
        claim(edge.from, edge.relation);
      }
    }
    const layer = [...reached]
      .filter(([reachedId]) => !seen.has(reachedId))
      .map(([reachedId, relation]) => ({
        // Non-null because every id in `reached` came out of `FIXTURE_ESTATE`
        // or out of a link this file wrote between two of its assets.
        asset: estateIndex().byId.get(reachedId)!,
        relation,
      }))
      .sort((left, right) => left.asset.name.localeCompare(right.asset.name));
    for (const line of layer) seen.add(line.asset.id);
    for (const line of layer) {
      assets.push({
        asset: assetRow(line.asset),
        path: assetPathText(line.asset),
        relation: line.relation,
      });
    }
    frontier = layer.map((line) => line.asset.id);
  }

  // `seen` is now the asset and its whole closure: everything that went down.
  // By name, `ROUTES_TO`'s order, rather than the file's.
  const routes = FIXTURE_ROUTES.filter((route) => route.target_id !== null && seen.has(route.target_id)).sort(
    (left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id),
  );
  return { assets, routes: routes.map(routeRow) };
}

/** `get_route`: one route and its history, which this fixture has none of. */
function routeDetail(args: Record<string, unknown>) {
  const id = args.routeId as string;
  const route = FIXTURE_ROUTES.find((candidate) => candidate.id === id);
  if (!route) throw { code: "not_found", message: `no route ${id}`, source_id: null };
  // Empty rather than invented: every line in a route's history is written by
  // a write, and this fixture answers none of the three that write one.
  return {
    route: routeRow(route),
    history: [],
    // One target, `assetDetail`'s (#976): *Create monitor* on a route.
    monitor_targets: [{ source_id: "kuma", display_name: "Uptime Kuma", entity: "kuma:monitors" }],
  };
}

let mintedRoutes = 0;

/**
 * `create_route` (#876's *Add route*): minted here the way `assets::create_route`
 * mints one. A route's properties are all custom rows, so a property pair
 * becomes one. Refused as the backend refuses: a missing exposer, an empty
 * name or URL, a target that is not an asset. The history stays empty
 * (see `routeDetail`).
 */
function createRoute(args: Record<string, unknown>) {
  const assetId = String(args.assetId ?? "");
  assetOr404(assetId);
  const name = String(args.name ?? "").trim();
  const url = String(args.url ?? "").trim();
  if (name === "") throw { code: "invalid", message: "a route needs a name", source_id: null };
  if (url === "") throw { code: "invalid", message: "a route needs a URL", source_id: null };
  const targetId = (args.targetId as string | null) ?? null;
  if (targetId !== null) assetOr404(targetId);
  mintedRoutes += 1;
  const created: FixtureRoute = {
    id: `route:new-${mintedRoutes}`,
    asset_id: assetId,
    target_id: targetId,
    name,
    url,
    visibility: (args.visibility as "internal" | "public" | null) ?? "internal",
    properties: ((args.properties ?? []) as [string, PropertyValue | null][]).map(([key, value]) => ({
      key,
      label: key,
      value,
      custom: true,
    })),
  };
  FIXTURE_ROUTES.push(created);
  return routeRow(created);
}

/** `edit_route`: the five edits `RouteEdit` names, applied in order. */
function editRoute(args: Record<string, unknown>) {
  const id = String(args.routeId ?? "");
  const route = FIXTURE_ROUTES.find((candidate) => candidate.id === id);
  if (!route) throw { code: "not_found", message: `no route ${id}`, source_id: null };
  for (const edit of (args.edits ?? []) as Record<string, unknown>[]) {
    if (edit.field === "name") route.name = String(edit.value ?? "").trim();
    else if (edit.field === "url") route.url = String(edit.value ?? "").trim();
    else if (edit.field === "visibility") route.visibility = edit.value as "internal" | "public";
    else if (edit.field === "target") {
      const target = (edit.value as string | null) ?? null;
      if (target !== null) assetOr404(target);
      route.target_id = target;
    } else if (edit.field === "property") {
      const key = String(edit.key);
      const at = route.properties.findIndex((property) => property.key === key);
      if (edit.value === null) {
        if (at !== -1) route.properties.splice(at, 1);
      } else if (at === -1) route.properties.push({ key, label: key, value: edit.value as PropertyValue, custom: true });
      else route.properties[at]!.value = edit.value as PropertyValue;
    }
    // `monitors` (#976) names a monitor the poll will attach. The fixture
    // mirrors no monitor, so the name only says which create is on its way
    // to the route (#1959), duplicates dropped as on an asset.
    else if (edit.field === "monitors") {
      const known = (route.monitors ??= []);
      for (const name of ((edit.added ?? []) as string[]).map((added) => added.trim())) {
        if (name !== "" && !known.includes(name)) known.push(name);
      }
    }
  }
  return routeRow(route);
}

/** `delete_route`: gone, with nothing pointing at it to clear. */
function deleteRoute(args: Record<string, unknown>) {
  const id = String(args.routeId ?? "");
  const at = FIXTURE_ROUTES.findIndex((candidate) => candidate.id === id);
  if (at === -1) throw { code: "not_found", message: `no route ${id}`, source_id: null };
  FIXTURE_ROUTES.splice(at, 1);
  return null;
}

/** `knobas.asset.path_text`: the ancestors' names, outermost first. */
export function assetPathText(asset: (typeof FIXTURE_ESTATE)[number]): string | null {
  const names: string[] = [];
  let walk = asset.parent_id;
  while (walk !== null) {
    const held = FIXTURE_ESTATE.find((row) => row.id === walk);
    if (!held) break;
    names.unshift(held.name);
    walk = held.parent_id;
  }
  return names.length === 0 ? null : names.join(" / ");
}

/** This feature's commands. */
/** `handle` with no index left from an earlier command: another module's write may have come between. */
function fresh<A extends unknown[], R>(handle: (...args: A) => R): (...args: A) => R {
  return (...args) => {
    estateChanged();
    return handle(...args);
  };
}

export function handlers(): Handlers {
  return {
    // The estate (#428, writable since #429). Stateful within the session,
    // like the stored contexts: an asset created, renamed, moved or deleted
    // here stays that way for as long as the page lives, so the whole create /
    // edit / move walk can be driven in a browser. Nothing is persisted --
    // reload and the estate is the fixture again.
    asset_tree: fresh((args) => assetColumn(args)),
    get_asset: fresh((args) => assetDetail(args)),
    asset_types: () => ASSET_TYPES,
    create_asset: fresh((args) => createAsset(args)),
    edit_asset: fresh((args) => editAsset(args)),
    move_asset: fresh((args) => moveAsset(args)),
    delete_asset: fresh((args) => deleteAsset(args)),
    // A route's own read, for the `#/route/<id>` address a click on a route's
    // name opens (#432), and its three writes (#876): in memory like the
    // assets', with no history line, since `routeDetail` has none to draw.
    depends_on_this: fresh((args) => dependsOnThis(args)),
    get_route: fresh((args) => routeDetail(args)),
    create_route: fresh((args) => createRoute(args)),
    edit_route: fresh((args) => editRoute(args)),
    delete_route: fresh((args) => deleteRoute(args)),
  };
}

keepSeed(FIXTURE_ESTATE, FIXTURE_ROUTES, ASSET_HISTORY);
