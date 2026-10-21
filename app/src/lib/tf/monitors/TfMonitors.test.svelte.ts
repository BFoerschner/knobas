/**
 * Monitors (#864, R-11.3, R-11.4), mounted over stand-in reads: the header and
 * tabs, the roster's order and columns, the scopes, *Ack*, *Pause* and
 * *Resume*, *Assets nothing watches*, and *Create monitor*'s two writes.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { AssetDetail, AssetEdit, MonitorRow, RouteDetail, RouteEdit, UnmonitoredAsset } from "../../ipc/assets";
import type { Reached } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { toasts } from "../../shell/toasts.svelte";
import { forgetFilters } from "../filter/filter-state.svelte";
import type { MonitorPorts } from "./TfMonitors.svelte";
import TfMonitors from "./TfMonitors.svelte";

const NOW = new Date("2026-10-16T14:00:00Z");

function monitor(entity_id: string, name: string, over: Partial<MonitorRow> = {}): MonitorRow {
  return {
    entity_id,
    source_id: "kuma",
    name,
    state: "up",
    monitor_type: "http",
    target: null,
    response_time_ms: 140,
    checked_at: "2026-10-16T13:59:00Z",
    uptime: [{ window: "30d", ratio: 0.9998 }],
    cert_days_remaining: null,
    web_url: null,
    tombstoned: false,
    assets: [],
    actions: ["pause_monitor"],
    samples: [],
    ...over,
  };
}

const ROSTER = [
  monitor("kuma:1", "payouts /health", { assets: [{ id: "asset:r", name: "payouts", path: null, type_label: "Service" }] }),
  monitor("kuma:2", "flows-stage", { state: "down", response_time_ms: null, assets: [{ id: "asset:f", name: "flows-stage", path: null, type_label: "Container" }] }),
  monitor("kuma:3", "yard-planner", { tombstoned: true, actions: ["resume_monitor"] }),
  monitor("kuma:4", "pg-payments", { state: "warn", monitor_type: "port", response_time_ms: 1900 }),
];

const gap = (id: string, type_id: string, type_label: string, name: string): UnmonitoredAsset => ({
  id,
  type_id,
  type_label,
  monogram: "XX",
  name,
  path: null,
  environment: "prod",
  owner: "ops",
  url: null,
});
const UNWATCHED = [
  gap("asset:site", "site", "Site", "hel1"),
  gap("asset:pg", "database_server", "Database server", "pg-main"),
  { ...gap("route:gitea", "route", "Route", "Gitea"), url: "https://gitea.test/" },
];

const ROUTE_DETAIL = {
  route: { id: "route:gitea", name: "Gitea", url: "https://gitea.test/" },
  history: [],
  monitor_targets: [{ source_id: "kuma", display_name: "Uptime Kuma", entity: "kuma:monitors" }],
} as unknown as RouteDetail;

const DETAIL = {
  asset: { id: "asset:pg", type_id: "database_server", name: "pg-main" },
  properties: [
    { key: "host", label: "Host", value: { kind: "text", value: "db.internal" }, custom: false },
    { key: "port", label: "Port", value: { kind: "number", value: 5432 }, custom: false },
  ],
  exposes: [],
  reachable_via: [],
  monitor_targets: [{ source_id: "kuma", display_name: "Uptime Kuma", entity: "kuma:monitors" }],
} as unknown as AssetDetail;

let target: HTMLElement;
let view: ReturnType<typeof mount> | null = null;
let writes: WriteOpPayload[];
let edits: [string, AssetEdit[]][];
let routeEdits: [string, RouteEdit[]][];
let acks: string[];

beforeEach(() => {
  forgetFilters();
  toasts.items.splice(0);
  writes = [];
  edits = [];
  routeEdits = [];
  acks = [];
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  if (view) unmount(view);
  view = null;
  target.remove();
});

const queued = (id: number) => ({ id }) as QueuedWrite;

function ports(over: Partial<MonitorPorts> = {}): MonitorPorts {
  return {
    monitorRoster: async () => ROSTER,
    unmonitoredAssets: async () => UNWATCHED,
    ackAlert: async (id) => {
      acks.push(id);
      return {} as never;
    },
    submitWrite: async (payload) => {
      writes.push(payload);
      return queued(writes.length);
    },
    getAsset: async () => DETAIL,
    editAsset: async (id, list) => {
      edits.push([id, list]);
      return {} as never;
    },
    getRoute: async () => ROUTE_DETAIL,
    editRoute: async (id, list) => {
      routeEdits.push([id, list]);
      return {} as never;
    },
    listSources: async () => [{ id: "kuma", display_name: "Uptime Kuma" }, { id: "jira", display_name: "Jira" }],
    listContexts: async () => [],
    getEntity: async (id) => {
      if (id !== "jira:PAY-231") throw { code: "not_found" };
      return { row: { entity_id: id, kind: "ticket" } } as never;
    },
    jobReach: async (id): Promise<Reached[]> =>
      id === "jira:PAY-231" ? [{ entity_id: "asset:f", kind: "asset", step: "asset:f", origin: { kind: "own" } as never }] : [],
    ...over,
  };
}

async function settle() {
  for (let i = 0; i < 10; i += 1) await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

async function draw(rest: string[], job: string | null = "PAY-231", stand: MonitorPorts = ports()) {
  view = mount(TfMonitors, { target, props: { ready: true, rest, job, ports: stand, now: NOW } });
  await settle();
}

const text = (el: Element | null) => el?.textContent?.replace(/\s+/g, " ").trim() ?? "";
const rows = () => [...target.querySelectorAll("tbody tr")];
const button = (tr: Element, label: string) =>
  [...tr.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label);

test("#/monitors draws the header, both tabs with counts, and the roster sorted down, warning, paused, up", async () => {
  await draw([]);
  expect(text(target.querySelector("h1"))).toBe("Monitors");
  expect(text(target.querySelector(".tf-sub"))).toBe(
    "4 checks from Uptime Kuma, synced 1 min ago. Each one watches an asset. Bars cover the last 24 hours in half-hours.",
  );
  const tabs = [...target.querySelectorAll(".tf-tabs a")];
  expect(tabs.map(text)).toEqual(["Monitors 4", "Assets nothing watches 2"]);
  expect(tabs[0]!.getAttribute("aria-current")).toBe("page");
  expect(rows().map((tr) => text(tr.querySelector("td")))).toEqual([
    "flows-stage HTTP",
    "pg-payments TCP",
    "yard-planner HTTP",
    "payouts /health HTTP",
  ]);
  const [, , , up] = rows();
  const cells = [...up!.querySelectorAll("td")].map(text);
  expect(cells.slice(1, 6)).toEqual(["up", "", "140 ms", "99.98 %", "payouts Service"]);
  expect(up!.querySelectorAll(".tf-mbar i")).toHaveLength(48);
  expect(up!.querySelector<HTMLAnchorElement>("td a")?.getAttribute("href")).toBe("#/asset/asset%3Ar");
});

test("Ack is on a down monitor only, acks it by the monitor, and reads Acked", async () => {
  await draw([]);
  const [down, warn] = rows();
  expect(button(warn!, "Ack")).toBeUndefined();
  button(down!, "Ack")!.click();
  await settle();
  expect(acks).toEqual(["kuma:2"]);
  expect(button(down!, "Acked")?.disabled).toBe(true);
  expect(toasts.items.map((t) => t.text)).toContain("Acked in knobas. The alert stays open until the monitor recovers.");
});

test("Pause and Resume queue the source's op for that monitor", async () => {
  await draw([]);
  const [, , paused, up] = rows();
  button(up!, "Pause")!.click();
  await settle();
  button(paused!, "Resume")!.click();
  await settle();
  expect(writes).toEqual([{ PauseMonitor: { entity: "kuma:1" } }, { ResumeMonitor: { entity: "kuma:3" } }]);
  expect(toasts.items.map((t) => t.text).filter((t) => t.startsWith("Queued:"))).toEqual([
    "Queued: pause payouts /health.",
    "Queued: resume yard-planner.",
  ]);
});

test("This job keeps the monitors watching what the job reaches; My jobs and All offer the rest", async () => {
  await draw([]);
  const scope = (label: string) =>
    [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b).startsWith(label))!;
  scope("This job").click();
  await settle();
  expect(rows().map((tr) => text(tr.querySelector("td")))).toEqual(["flows-stage HTTP"]);
  scope("All").click();
  await settle();
  expect(rows()).toHaveLength(4);
});

test("#/monitors/unmonitored lists the assets and routes worth a check with env and owner, and has no filter bar", async () => {
  await draw(["unmonitored"]);
  expect(target.querySelector(".tf-tabs [aria-current='page']")?.textContent).toContain("Assets nothing watches");
  expect(text(target.querySelector(".tf-mon-lead"))).toBe("Containers, services, routes, databases and VMs that no monitor watches.");
  expect(rows().map((tr) => [...tr.querySelectorAll("td")].map(text))).toEqual([
    ["pg-main", "Database server", "prod", "ops", "Create monitor"],
    ["Gitea", "Route", "prod", "ops", "Create monitor"],
  ]);
  expect(rows()[1]!.querySelector("td a")?.getAttribute("href")).toBe("#/route/route%3Agitea");
  expect(target.querySelector("input[type='search'], .tf-fbar")).toBeNull();
});

test("Create monitor on a route opens as HTTP on its URL plus /health and records the name on the route", async () => {
  await draw(["unmonitored"]);
  button(rows()[1]!, "Create monitor")!.click();
  await settle();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(text(dialog.querySelector("h2"))).toBe("Create a monitor for Gitea");
  const inputs = [...dialog.querySelectorAll<HTMLInputElement>("input")];
  expect(inputs.map((input) => input.value)).toEqual(["Gitea /health", "https://gitea.test/health"]);
  expect(dialog.querySelector<HTMLSelectElement>("select")!.value).toBe("http");
  expect(text(dialog.querySelector(".tf-note"))).toBe("Created in Uptime Kuma through the write queue, attached to this route.");

  [...dialog.querySelectorAll<HTMLButtonElement>("footer button")].find((b) => text(b) === "Create monitor")!.click();
  await settle();
  expect(routeEdits).toEqual([["route:gitea", [{ field: "monitors", added: ["Gitea /health"] }]]]);
  expect(edits).toEqual([]);
  expect(writes).toEqual([
    { CreateMonitor: { entity: "kuma:monitors", name: "Gitea /health", url: "https://gitea.test/health", kind: "http" } },
  ]);
});

test("Create monitor opens prefilled as TCP, records the name on the asset, then queues the create with its kind", async () => {
  await draw(["unmonitored"]);
  button(rows()[0]!, "Create monitor")!.click();
  await settle();
  const dialog = document.querySelector(".tf-dialog")!;
  expect(text(dialog.querySelector("h2"))).toBe("Create a monitor for pg-main");
  const inputs = [...dialog.querySelectorAll<HTMLInputElement>("input")];
  expect(inputs.map((input) => input.value)).toEqual(["pg-main", "db.internal:5432"]);
  expect(dialog.querySelector<HTMLSelectElement>("select")!.value).toBe("tcp");
  expect(text(dialog.querySelector(".tf-note"))).toBe("Created in Uptime Kuma through the write queue, attached to this asset.");

  [...dialog.querySelectorAll<HTMLButtonElement>("footer button")].find((b) => text(b) === "Create monitor")!.click();
  await settle();
  expect(edits).toEqual([["asset:pg", [{ field: "monitors", added: ["pg-main"] }]]]);
  expect(writes).toEqual([{ CreateMonitor: { entity: "kuma:monitors", name: "pg-main", url: "db.internal:5432", kind: "tcp" } }]);
  expect(document.querySelector(".tf-dialog")).toBeNull();
  expect(toasts.items.map((t) => t.text)).toContain("Queued: create monitor for pg-main.");
});

test("a create the backend refuses stays open and says why", async () => {
  await draw(["unmonitored"], null, ports({ submitWrite: async () => Promise.reject({ code: "invalid", message: "no Docker host" }) }));
  button(rows()[0]!, "Create monitor")!.click();
  await settle();
  const dialog = document.querySelector(".tf-dialog")!;
  [...dialog.querySelectorAll<HTMLButtonElement>("footer button")].find((b) => text(b) === "Create monitor")!.click();
  await settle();
  expect(text(document.querySelector(".tf-dialog [role='alert']"))).toContain("no Docker host");
});

test("Monitors draws the roster as read and keeps no rows by source: the read narrows itself to monitoring's fillers (#1297)", async () => {
  const beacon = monitor("pulse:9", "beacon /ping", { source_id: "pulse" });
  await draw([], "PAY-231", ports({ monitorRoster: async () => [...ROSTER, beacon] }));

  expect(rows()).toHaveLength(ROSTER.length + 1);
  expect(text(target.querySelector(".tf-tabs a"))).toBe(`Monitors ${ROSTER.length + 1}`);
});
