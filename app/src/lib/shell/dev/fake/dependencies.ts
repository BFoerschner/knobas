/**
 * `?fake-ipc`: Dependencies (#871), the `dependencies` read.
 *
 * - The layers are migration `0036`'s seed, restated.
 * - The estate is the testenv one (`FIXTURE_ESTATE`, `FIXTURE_ROUTES`), as it
 *   stands in the session after any write: an asset's health is the worse of
 *   its own status and the newest state of each monitor the fake roster says
 *   watches it, and `monitors` is how many do.
 * - The code is the corpus's repositories and its open pull requests, the
 *   delivery is the Builds fixture's pipelines (`fakePipelines`, their newest
 *   run's chip, under the ids `job_pipelines` answers with, so a job's Map
 *   keeps them, #874), and the linked tickets are those a link joins to one
 *   of them.
 *
 * - The layers and the views (#873) are session state: migration `0038`'s
 *   four views, and the layer and view writes with the refusals the backend
 *   makes (a taken or blank name, the last view).
 *
 * A restatement, which certifies nothing: the read is witnessed in
 * `crates/knobas-app/tests/it/dependencies_ipc.rs`. The estate here is the
 * testenv one, not prototype-6's Tidewater; the screenshot comparison names
 * that.
 */
import type { DepLayer, Dependencies, DepItem, DepRelation, DepView, DepViews, DepViewSettings, LayerInView } from "../../../ipc/assets";
import type { Tone } from "../../../ipc/entity";
import type { Handlers } from "../fake-tauri";
import { onFakeReset, seedOf } from "../fake-seed";
import { CORPUS } from "./corpus";
import { FIXTURE_ESTATE, FIXTURE_ROUTES } from "./estate";
import { fakePipelines } from "./builds";
import { fakeMonitorRoster } from "./monitors";
import { keyOf } from "../../../time/format";

const SEED: Dependencies["layers"] = [
  { id: "work", name: "Work", kinds: ["ticket", "pr", "page"] },
  { id: "code", name: "Code", kinds: ["repo"] },
  { id: "delivery", name: "Delivery", kinds: ["pipeline"] },
  { id: "entry", name: "Entry points", kinds: ["route"] },
  { id: "services", name: "Services", kinds: ["service", "reverse_proxy", "middleware", "runtime", "module", "custom"] },
  { id: "containers", name: "Containers", kinds: ["container_engine", "container"] },
  { id: "data", name: "Data", kinds: ["database_server", "database", "schema", "table"] },
  { id: "machines", name: "Machines", kinds: ["vm"] },
  { id: "hardware", name: "Hardware and network", kinds: ["hypervisor", "site", "network"] },
];

/** `kinds()`: the six other kinds, then `knobas_core::asset::TYPES` in order. */
const KINDS: Dependencies["kinds"] = [
  ["ticket", "Ticket", "TK"],
  ["pr", "Pull request", "PR"],
  ["page", "Page", "PG"],
  ["repo", "Repository", "RE"],
  ["pipeline", "Pipeline", "PL"],
  ["route", "Route", "RO"],
  ["site", "Site", "SI"],
  ["hypervisor", "Hypervisor", "HV"],
  ["vm", "VM", "VM"],
  ["container_engine", "Container engine", "CE"],
  ["container", "Container", "CT"],
  ["service", "Service", "SV"],
  ["module", "Module", "MD"],
  ["runtime", "Runtime", "RT"],
  ["database_server", "Database server", "DS"],
  ["database", "Database", "DB"],
  ["schema", "Schema", "SM"],
  ["table", "Table", "TB"],
  ["reverse_proxy", "Reverse proxy", "RP"],
  ["middleware", "Middleware", "MW"],
  ["network", "Network", "NW"],
  ["custom", "Custom", "CU"],
].map(([id, label, monogram]) => ({ id: id!, label: label!, monogram: monogram! }));

/** Smaller is worse, as `ROLLUP` ranks a state. */
const RANK: Record<string, number> = { down: 0, warn: 1, up: 2 };
const TONE_OF_RANK: (Tone | null)[] = ["err", "warn", "ok", null];

/** The fixture's links Dependencies draws: PAY-231's PR and the Gitea container it deploys to. */
const LINKS: DepRelation[] = [
  { from: "mock:PAY-231", to: "mock:payout-service#142", relation: "related" },
  { from: "mock:PAY-231", to: "asset:knobas-gitea", relation: "deployed-to" },
  { from: "mock:PAY-228", to: "mock:tidewater/ledger", relation: "related" },
];

function fakeDependencies(): Dependencies {
  const items: DepItem[] = [];
  const relations: DepRelation[] = [];
  const roster = fakeMonitorRoster();
  const inForce = (id: string | null, field: "environment" | "owner"): string | null => {
    let walk = id;
    while (walk !== null) {
      const asset = FIXTURE_ESTATE.find((row) => row.id === walk);
      if (!asset) return null;
      if (asset[field] !== null) return asset[field];
      walk = asset.parent_id;
    }
    return null;
  };

  for (const asset of FIXTURE_ESTATE) {
    const watching = roster.filter((monitor) => monitor.assets.some((watched) => watched.id === asset.id));
    const rank = Math.min(RANK[asset.status] ?? 3, ...watching.map((monitor) => RANK[monitor.state ?? ""] ?? 3));
    items.push({
      id: asset.id,
      kind: asset.type_id,
      name: asset.name,
      health: TONE_OF_RANK[rank] ?? null,
      state: null,
      monitors: watching.length,
      environment: inForce(asset.id, "environment"),
      owner: inForce(asset.id, "owner"),
    });
    if (asset.parent_id !== null) relations.push({ from: asset.id, to: asset.parent_id, relation: "inside" });
  }
  for (const route of FIXTURE_ROUTES) {
    items.push({
      id: route.id,
      kind: "route",
      name: route.name,
      health: null,
      state: null,
      monitors: 0,
      environment: inForce(route.asset_id, "environment"),
      owner: null,
    });
    relations.push({ from: route.asset_id, to: route.id, relation: "exposes" });
    if (route.target_id !== null) relations.push({ from: route.id, to: route.target_id, relation: "lands-on" });
  }

  for (const row of CORPUS) {
    if (row.deleted_at !== null) continue;
    if (row.kind === "repo") {
      items.push({ id: row.entity_id, kind: "repo", name: row.title, health: null, state: null, monitors: 0, environment: null, owner: null });
    } else if (row.kind === "pr" && row.payload["state"] === "open") {
      items.push({ id: row.entity_id, kind: "pr", name: row.title, health: "info", state: "open", monitors: 0, environment: null, owner: null });
      const repo = CORPUS.find((other) => other.kind === "repo" && other.title.endsWith(`/${String(row.payload["repo"])}`));
      if (repo) relations.push({ from: row.entity_id, to: repo.entity_id, relation: "in-repository" });
    }
  }

  // Every pipeline the Builds fixture lists, with its newest run's chip, as the
  // real read takes them from `pipelines` (#874: so `job_pipelines` names them).
  for (const pipeline of fakePipelines()) {
    const chip = pipeline.recent[0]?.state ?? null;
    items.push({
      id: pipeline.entity_id,
      kind: "pipeline",
      name: pipeline.name,
      health: chip?.tone ?? null,
      state: chip?.text ?? null,
      monitors: 0,
      environment: null,
      owner: null,
    });
  }

  const known = new Set(items.map((item) => item.id));
  for (const link of LINKS) {
    for (const end of [link.from, link.to]) {
      if (known.has(end)) continue;
      const ticket = CORPUS.find((row) => row.entity_id === end && row.kind === "ticket");
      if (!ticket) continue;
      known.add(end);
      items.push({
        id: end,
        kind: "ticket",
        name: String(ticket.payload["key"] ?? keyOf(end)),
        health: null,
        state: typeof ticket.payload["status"] === "string" ? ticket.payload["status"] : null,
        monitors: 0,
        environment: null,
        owner: null,
      });
    }
    if (known.has(link.from) && known.has(link.to)) relations.push(link);
  }

  return { layers: layers.map((layer) => ({ ...layer, kinds: [...layer.kinds] })), kinds: KINDS, items, relations };
}

// -- #873: the layers and the views, for the session --------------------------

let layers: DepLayer[] = SEED.map((layer) => ({ ...layer, kinds: [...layer.kinds] }));
let seq = 0;

const refuse = (code: string, message: string) => ({ code, message, source_id: null });
const answer = () => layers.map((layer) => ({ ...layer, kinds: [...layer.kinds] }));

function layerName(raw: unknown, except: string | null): string {
  const name = String(raw ?? "").trim();
  if (name === "") throw refuse("invalid", "A layer needs a name.");
  if (layers.some((layer) => layer.id !== except && layer.name.trim().toLowerCase() === name.toLowerCase()))
    throw refuse("conflict", `There is already a layer called ${name}. Names are unique, so a name always means the same things.`);
  return name;
}

function layerAt(id: unknown): DepLayer {
  const layer = layers.find((l) => l.id === id);
  if (!layer) throw refuse("not_found", `No layer ${String(id)}.`);
  return layer;
}

const lay = (hidden: boolean, group: LayerInView["group"]): LayerInView => ({ hidden, folded: false, group });
const GROUPS: Record<string, LayerInView["group"]> = {
  work: "status", code: "owner", delivery: "project", entry: "environment", services: "environment",
  containers: "parent", data: "environment", machines: "environment", hardware: "none",
};
function seedView(id: string, name: string, hide: (layer: string) => boolean, group: (layer: string) => LayerInView["group"]): DepView {
  const settings: DepViewSettings = { layers: {}, environments: [], problems: false, tiles: "auto", fold_at: 24 };
  for (const layer of SEED) settings.layers[layer.id] = lay(hide(layer.id), group(layer.id));
  return { id, name, settings };
}
const CODE = ["work", "code", "delivery"];
let views: DepViews = {
  views: [
    seedView("everything", "Everything", (l) => l === "work", (l) => GROUPS[l]!),
    { id: "ticket", name: "This ticket", settings: { layers: {}, environments: [], problems: false, tiles: "auto", fold_at: 24 } },
    seedView("runtime", "Runtime by environment", (l) => CODE.includes(l), (l) => (CODE.includes(l) ? GROUPS[l]! : l === "hardware" ? "none" : "environment")),
    seedView("code", "Code and delivery", (l) => !CODE.includes(l), (l) => GROUPS[l]!),
  ],
  opens_from_ticket: "ticket",
  opens_from_top: "everything",
};

function viewsAnswer(): DepViews {
  const first = views.views[0]?.id ?? "";
  const has = (id: string) => views.views.some((view) => view.id === id);
  return {
    views: structuredClone(views.views),
    opens_from_ticket: has(views.opens_from_ticket) ? views.opens_from_ticket : first,
    opens_from_top: has(views.opens_from_top) ? views.opens_from_top : first,
  };
}

/** This feature's commands. */
export function handlers(): Handlers {
  return {
    dependencies: () => fakeDependencies(),
    create_dep_layer: (args) => {
      layers.push({ id: `layer-${++seq}`, name: layerName(args["name"], null), kinds: [] });
      return answer();
    },
    rename_dep_layer: (args) => {
      const layer = layerAt(args["layerId"]);
      layer.name = layerName(args["name"], layer.id);
      return answer();
    },
    delete_dep_layer: (args) => {
      const layer = layerAt(args["layerId"]);
      layers = layers.filter((l) => l !== layer);
      return answer();
    },
    order_dep_layers: (args) => {
      const ids = args["layerIds"] as string[];
      if (ids.length !== layers.length || !layers.every((layer) => ids.includes(layer.id)))
        throw refuse("invalid", "The order must name every layer exactly once.");
      layers = ids.map((id) => layerAt(id));
      return answer();
    },
    place_dep_kind: (args) => {
      const kind = String(args["kind"]);
      if (!KINDS.some((known) => known.id === kind)) throw refuse("invalid", `${kind} is not a kind Dependencies knows.`);
      const to = args["layerId"] == null ? null : layerAt(args["layerId"]);
      for (const layer of layers) layer.kinds = layer.kinds.filter((k) => k !== kind);
      to?.kinds.push(kind);
      return answer();
    },
    dep_views: () => viewsAnswer(),
    save_dep_view: (args) => {
      const name = String(args["name"] ?? "").trim();
      if (name === "") throw refuse("invalid", "A view needs a name.");
      const settings = structuredClone(args["settings"] as DepViewSettings);
      const id = args["viewId"] as string | null | undefined;
      if (id == null) {
        const view = { id: `view-${++seq}`, name, settings };
        views.views.push(view);
        return structuredClone(view);
      }
      const view = views.views.find((v) => v.id === id);
      if (!view) throw refuse("not_found", `No view ${id}.`);
      view.name = name;
      view.settings = settings;
      return structuredClone(view);
    },
    delete_dep_view: (args) => {
      const id = String(args["viewId"]);
      if (!views.views.some((v) => v.id === id)) throw refuse("not_found", `No view ${id}.`);
      if (views.views.length <= 1) throw refuse("conflict", "The last view cannot be deleted.");
      views = { ...views, views: views.views.filter((v) => v.id !== id) };
      return viewsAnswer();
    },
    set_dep_view_defaults: (args) => {
      for (const key of ["fromTicket", "fromTop"])
        if (!views.views.some((v) => v.id === args[key])) throw refuse("not_found", `No view ${String(args[key])}.`);
      views = { ...views, opens_from_ticket: String(args["fromTicket"]), opens_from_top: String(args["fromTop"]) };
      return viewsAnswer();
    },
  };
}

const seededLayers = seedOf(layers);
const seededViews = seedOf(views);
onFakeReset(() => {
  layers = seededLayers();
  views = seededViews();
});
