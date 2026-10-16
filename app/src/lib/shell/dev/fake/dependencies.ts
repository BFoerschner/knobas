/**
 * `?fake-ipc`: Dependencies (#871), the `dependencies` read.
 *
 * - The layers are migration `0036`'s seed, restated.
 * - The estate is the testenv one (`FIXTURE_ESTATE`, `FIXTURE_ROUTES`), as it
 *   stands in the session after any write: an asset's health is the worse of
 *   its own status and the newest state of each monitor the fake roster says
 *   watches it, and `monitors` is how many do.
 * - The code is the corpus's repositories and its open pull requests, the
 *   delivery is the pipelines PAY-231's builds ran in (their newest run's
 *   chip), and the linked tickets are those a link joins to one of them.
 *
 * A restatement, which certifies nothing: the read is witnessed in
 * `crates/knobas-app/tests/it/dependencies_ipc.rs`. The estate here is the
 * testenv one, not prototype-6's Tidewater; the screenshot comparison names
 * that.
 */
import type { Dependencies, DepItem, DepRelation } from "../../../ipc/assets";
import type { Tone } from "../../../ipc/entity";
import type { Handler } from "../fake-tauri";
import { CORPUS } from "./corpus";
import { FIXTURE_ESTATE, FIXTURE_ROUTES } from "./estate";
import { JOB_ROWS } from "./job-reach";
import { fakeMonitorRoster } from "./monitors";

const LAYERS: Dependencies["layers"] = [
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

  // A pipeline per build configuration PAY-231's runs name, with its newest run's chip.
  const newest = new Map<string, (typeof JOB_ROWS)[number]>();
  for (const run of JOB_ROWS.filter((row) => row.kind === "build")) {
    const pipeline = run.title.split(" #")[0]!;
    const seen = newest.get(pipeline);
    if (!seen || seen.updated_at < run.updated_at) newest.set(pipeline, run);
  }
  for (const [pipeline, run] of newest) {
    const chip = run.payload["chip"] as { text: string; tone: Tone } | undefined;
    items.push({
      id: `mock:${pipeline}`,
      kind: "pipeline",
      name: pipeline,
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
        name: String(ticket.payload["key"] ?? end.slice(end.indexOf(":") + 1)),
        health: null,
        state: typeof ticket.payload["status"] === "string" ? ticket.payload["status"] : null,
        monitors: 0,
        environment: null,
        owner: null,
      });
    }
    if (known.has(link.from) && known.has(link.to)) relations.push(link);
  }

  return { layers: LAYERS, kinds: KINDS, items, relations };
}

/** This feature's commands. */
export function handlers(): Record<string, Handler> {
  return {
    dependencies: () => fakeDependencies(),
  };
}
