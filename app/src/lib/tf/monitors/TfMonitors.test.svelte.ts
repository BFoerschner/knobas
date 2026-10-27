/**
 * Monitors (#864, R-11.3, R-11.4), mounted over stand-in reads: the header and
 * tabs, the roster's order and columns, the scopes, *Ack*, *Pause* and
 * *Resume*, *Assets nothing watches*, and *Create monitor*'s two writes.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { AssetDetail, AssetEdit, MonitorRow, OpenAlert, RouteDetail, RouteEdit, UnmonitoredAsset } from "../../ipc/assets";
import type { Reached } from "../../ipc/entity";
import { EVENTS } from "../../ipc";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import type { Hear } from "../../shell/follow-write";
import { toasts } from "../../shell/toasts.svelte";
import type { MonitorPorts } from "./TfMonitors.svelte";
import TfMonitors from "./TfMonitors.svelte";
import { sourceFacts } from "../job/source-facts.fixture";
import { pressRefreshKey, refreshProbe, type RefreshProbe } from "../refresh.fixture";

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
    refreshes: true,
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
  monitor_on_its_way: null,
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
/** The write follows the page holds, by write id: a test tells each what the queue says. */
let heard: Map<number, Hear>;
/** The page's `sync:state` listeners. */
let syncListeners: Set<(event: { payload: unknown }) => void>;
/** The page's `item:refreshed` listeners (#2105). */
let refreshes: Set<(event: { payload: unknown }) => void>;
/** The rows' *Refresh* and *Sync now* (#2138): the monitors read from their source, the sources run. */
let probe: RefreshProbe;

beforeEach(() => {
  toasts.items.splice(0);
  writes = [];
  edits = [];
  routeEdits = [];
  acks = [];
  heard = new Map();
  syncListeners = new Set();
  refreshes = new Set();
  probe = refreshProbe();
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
    openAlerts: async () => [],
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
    listSources: async () => [sourceFacts("kuma", { display_name: "Uptime Kuma" }), sourceFacts("jira", { display_name: "Jira" })],
    listContexts: async () => [],
    getEntity: async (id) => {
      if (id !== "jira:PAY-231") throw { code: "not_found" };
      return { row: { entity_id: id, kind: "ticket" } } as never;
    },
    jobReach: async (id): Promise<Reached[]> =>
      id === "jira:PAY-231" ? [{ entity_id: "asset:f", kind: "asset", step: "asset:f", origin: { kind: "own" } as never }] : [],
    listen: async (event, handler) => {
      if (event === EVENTS.itemRefreshed) {
        refreshes.add(handler);
        return () => refreshes.delete(handler);
      }
      if (event !== EVENTS.syncState) return () => {};
      syncListeners.add(handler);
      return () => syncListeners.delete(handler);
    },
    follower: () => ({
      follow(id, hear) {
        heard.set(id, hear);
        return () => heard.delete(id);
      },
    }),
    refresh: probe.ports,
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

const alertOf = (monitor_id: string, acked_at: string | null): OpenAlert =>
  ({ id: 1, monitor_id, monitor_name: "flows-stage", state: "down", opened_at: "2026-10-16T12:00:00Z", acked_at }) as OpenAlert;

test("a down row whose open alert is acked reads a disabled Acked on first draw, and Ack when it is not", async () => {
  await draw([], "PAY-231", ports({ openAlerts: async () => [alertOf("kuma:2", "2026-10-16T13:00:00Z")] }));
  const down = rows()[0]!;
  expect(button(down, "Ack")).toBeUndefined();
  expect(button(down, "Acked")?.disabled).toBe(true);
  expect(button(down, "Acked")?.title).toContain("2026-10-16");
  unmount(view!);
  view = null;
  await draw([], "PAY-231", ports({ openAlerts: async () => [alertOf("kuma:2", null)] }));
  expect(button(rows()[0]!, "Ack")?.disabled).toBe(false);
});

test("an ack survives leaving the page: a remount over the now-acked alert still reads Acked", async () => {
  let alerts = [alertOf("kuma:2", null)];
  const stand = ports({
    openAlerts: async () => alerts,
    ackAlert: async (id) => {
      acks.push(id);
      alerts = [alertOf(id, "2026-10-16T13:59:30Z")];
      return alerts[0]!;
    },
  });
  await draw([], "PAY-231", stand);
  button(rows()[0]!, "Ack")!.click();
  await settle();
  expect(button(rows()[0]!, "Acked")?.disabled).toBe(true);
  unmount(view!);
  view = null;
  await draw([], "PAY-231", stand);
  expect(button(rows()[0]!, "Acked")?.disabled).toBe(true);
  expect(button(rows()[0]!, "Ack")).toBeUndefined();
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

// -- a create on its way (#1959) ----------------------------------------------

const onItsWay = (id: number, state: QueuedWrite["state"], detail: string | null = null) =>
  ({ id, state, detail, op: "create_monitor" }) as QueuedWrite;

/** The unwatched list as the backend reads it, with `going` on pg-main's row: the create the queue holds for its name. */
function withCreate(going: () => QueuedWrite | null): Pick<MonitorPorts, "unmonitoredAssets"> {
  return {
    unmonitoredAssets: async () => UNWATCHED.map((row) => (row.id === "asset:pg" ? { ...row, monitor_on_its_way: going() } : row)),
  };
}

const lastCell = (tr: Element) => text([...tr.querySelectorAll("td")].at(-1)!);

function syncEnds(source_id = "kuma") {
  for (const handler of syncListeners) handler({ payload: { source_id, running: false } });
}

test("after a reload, a create on its way draws Monitor queued on its row and offers no Create monitor there", async () => {
  await draw(["unmonitored"], null, ports(withCreate(() => onItsWay(7, "pending"))));
  const [pg, route] = rows();
  expect(lastCell(pg!)).toBe("Monitor queued");
  expect(button(pg!, "Create monitor")).toBeUndefined();
  expect(button(route!, "Create monitor")).toBeDefined();
});

test("a create pressed now draws Monitor queued at once, before the re-read answers, and the re-read keeps it", async () => {
  let going: QueuedWrite | null = null;
  let reads = 0;
  let answer: () => void = () => {};
  const stand = ports({
    unmonitoredAssets: async () => {
      reads += 1;
      if (reads > 1) await new Promise<void>((resolve) => (answer = resolve));
      return UNWATCHED.map((row) => (row.id === "asset:pg" ? { ...row, monitor_on_its_way: going } : row));
    },
    submitWrite: async (payload) => {
      writes.push(payload);
      going = onItsWay(writes.length, "pending");
      return going;
    },
  });
  await draw(["unmonitored"], null, stand);
  button(rows()[0]!, "Create monitor")!.click();
  await settle();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => text(b) === "Create monitor")!.click();
  await settle();
  expect(reads).toBe(2);
  expect(lastCell(rows()[0]!)).toBe("Monitor queued");
  answer();
  await settle();
  expect(lastCell(rows()[0]!)).toBe("Monitor queued");
  expect(button(rows()[0]!, "Create monitor")).toBeUndefined();
});

test("a create the queue refuses gives Create monitor back and says why", async () => {
  let going: QueuedWrite | null = null;
  const stand = ports({
    ...withCreate(() => going),
    submitWrite: async (payload) => {
      writes.push(payload);
      going = onItsWay(writes.length, "pending");
      return going;
    },
  });
  await draw(["unmonitored"], null, stand);
  button(rows()[0]!, "Create monitor")!.click();
  await settle();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => text(b) === "Create monitor")!.click();
  await settle();
  expect(lastCell(rows()[0]!)).toBe("Monitor queued");

  going = null;
  heard.get(1)!(onItsWay(1, "refused", "no Docker host"));
  await settle();
  expect(button(rows()[0]!, "Create monitor")).toBeDefined();
  expect(toasts.items.map((t) => t.text)).toContain("Monitor pg-main was refused: no Docker host");
});

test("after a reload, a pending create is followed: held on drift, it gives Create monitor back", async () => {
  let going: QueuedWrite | null = onItsWay(7, "pending");
  await draw(["unmonitored"], null, ports(withCreate(() => going)));
  expect([...heard.keys()]).toEqual([7]);
  going = null;
  heard.get(7)!(onItsWay(7, "held"));
  await settle();
  expect(button(rows()[0]!, "Create monitor")).toBeDefined();
  expect(heard.size).toBe(0);
});

/** A create held as unconfirmed: sent to Kuma and never answered, so the monitor may be there (#1125). */
const unconfirmed = (id: number) =>
  ({
    ...onItsWay(id, "held", "Uptime Kuma did not answer"),
    source_id: "kuma",
    held_snapshot: { op: "create_monitor", unconfirmed: { source: "Uptime Kuma", detail: "the connection closed" } },
  }) as QueuedWrite;

test("after a reload, a create held as unconfirmed draws Monitor queued, says what to check, and is followed until discarded (#1966)", async () => {
  let going: QueuedWrite | null = unconfirmed(7);
  await draw(["unmonitored"], null, ports(withCreate(() => going)));
  const chip = rows()[0]!.querySelector<HTMLElement>(".tf-chip.tf-info");
  expect(text(chip)).toBe("Monitor queued");
  expect(chip?.title).toBe("Sent to Uptime Kuma, which did not answer, so it may be there. Check Uptime Kuma, then send it again or discard it in the write queue.");
  expect(button(rows()[0]!, "Create monitor")).toBeUndefined();
  expect([...heard.keys()]).toEqual([7]);

  going = null;
  heard.get(7)!(undefined);
  await settle();
  expect(button(rows()[0]!, "Create monitor")).toBeDefined();
});

test("a pending create the queue holds as unconfirmed stays Monitor queued and followed, with no Edit and resend (#1966)", async () => {
  let going: QueuedWrite | null = onItsWay(7, "pending");
  await draw(["unmonitored"], null, ports(withCreate(() => going)));
  going = unconfirmed(7);
  heard.get(7)!(unconfirmed(7));
  await settle();
  expect(lastCell(rows()[0]!)).toBe("Monitor queued");
  expect(button(rows()[0]!, "Create monitor")).toBeUndefined();
  expect([...heard.keys()]).toEqual([7]);
  const told = toasts.items.find((t) => t.text.startsWith("Monitor pg-main may have arrived"));
  expect(told?.more ?? []).toEqual([]);
});

test("a create withdrawn in the queue leaves the open rows, and the re-read gives Create monitor back", async () => {
  let going: QueuedWrite | null = onItsWay(7, "pending");
  await draw(["unmonitored"], null, ports(withCreate(() => going)));
  going = null;
  heard.get(7)!(undefined);
  await settle();
  expect(button(rows()[0]!, "Create monitor")).toBeDefined();
});

test("a sent create reads Monitor sent until the end of the sync that brings its monitor in, and is not followed (#2105)", async () => {
  let watched = false;
  const stand = ports({
    unmonitoredAssets: async () =>
      watched ? UNWATCHED.filter((row) => row.id !== "asset:pg") : withCreate(() => onItsWay(7, "sent")).unmonitoredAssets(),
  });
  await draw(["unmonitored"], null, stand);
  expect(lastCell(rows()[0]!)).toBe("Monitor sent");
  expect(heard.size).toBe(0);
  syncEnds();
  await settle();
  expect(lastCell(rows()[0]!)).toBe("Monitor sent");

  watched = true;
  syncEnds();
  await settle();
  expect(rows().map((tr) => text(tr.querySelector("td")))).toEqual(["Gitea"]);
  expect(syncListeners.size).toBe(0);
});

// -- the echo's one lifecycle: queued, sent, gone on the roster's read (#2105) --

/** Raise the refresh that followed write `write` on `entity` (#2095). */
function refreshed(entity: string, write: number | null) {
  for (const handler of refreshes) handler({ payload: { entity_id: entity, write_id: write, ended: "refreshed" } });
}

test("a create says Monitor queued, then Monitor sent, and goes once the roster holds the monitor Kuma made (#2105)", async () => {
  let going: QueuedWrite | null = null;
  let roster = ROSTER;
  const stand = ports({
    ...withCreate(() => going),
    monitorRoster: async () => roster,
    submitWrite: async (payload) => {
      writes.push(payload);
      going = onItsWay(writes.length, "pending");
      return going;
    },
  });
  await draw(["unmonitored"], null, stand);
  button(rows()[0]!, "Create monitor")!.click();
  await settle();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => text(b) === "Create monitor")!.click();
  await settle();
  const name = (writes[0] as { CreateMonitor: { name: string } }).CreateMonitor.name;
  expect(lastCell(rows()[0]!)).toBe("Monitor queued");

  // Sent: the queue no longer lists it, and the read names it sent.
  going = onItsWay(1, "sent");
  heard.get(1)!(undefined);
  await settle();
  expect(lastCell(rows()[0]!)).toBe("Monitor sent");

  // Kuma made it and the poll attached it: the roster holds it, the asset is watched.
  roster = [...ROSTER, monitor("kuma:9", name, { assets: [{ id: "asset:pg", name: "pg-main", path: null, type_label: "Database server" }] })];
  going = null;
  const watched = stand.unmonitoredAssets;
  stand.unmonitoredAssets = async () => (await watched()).filter((row) => row.id !== "asset:pg");
  refreshed("kuma:9", 1);
  await settle();
  expect(rows().map((tr) => text(tr.querySelector("td")))).toEqual(["Gitea"]);
});

test("a pause and a resume each draw a row echo queued, then sent, gone once the roster's read shows the new state (#2105)", async () => {
  let roster = ROSTER;
  let next = 30;
  const stand = ports({
    monitorRoster: async () => roster,
    submitWrite: async (payload) => {
      writes.push(payload);
      return { id: next++, state: "pending" } as QueuedWrite;
    },
  });
  await draw([], null, stand);
  const row = (name: string) => rows().find((tr) => text(tr.querySelector("td")).startsWith(name))!;
  const echo = (name: string) => row(name).querySelector(".tf-mon-acts .tf-chip")?.textContent ?? null;
  button(row("payouts /health"), "Pause")!.click();
  await settle();
  button(row("yard-planner"), "Resume")!.click();
  await settle();
  expect([echo("payouts /health"), echo("yard-planner")]).toEqual(["queued", "queued"]);
  expect(button(row("payouts /health"), "Pause")).toBeUndefined();

  heard.get(30)!(undefined);
  heard.get(31)!(undefined);
  await settle();
  expect([echo("payouts /health"), echo("yard-planner")]).toEqual(["sent", "sent"]);

  // A run's end before Kuma's /metrics moved: still on its way.
  syncEnds();
  await settle();
  expect([echo("payouts /health"), echo("yard-planner")]).toEqual(["sent", "sent"]);

  roster = ROSTER.map((one) =>
    one.entity_id === "kuma:1" ? { ...one, tombstoned: true, actions: ["resume_monitor"] } : one.entity_id === "kuma:3" ? { ...one, tombstoned: false, actions: ["pause_monitor"] } : one,
  );
  refreshed("kuma:1", 30);
  await settle();
  expect([echo("payouts /health"), echo("yard-planner")]).toEqual([null, null]);
  expect(button(row("payouts /health"), "Resume")).toBeDefined();
});

// -- Refresh (#2138) ----------------------------------------------------------

const rowNamed = (name: string) => rows().find((tr) => text(tr.querySelector("td")).startsWith(name))!;

test("a row whose source reads one monitor draws Refresh, one whose source does not draws Sync now (#2138)", async () => {
  const roster = [monitor("kuma:1", "payouts /health"), monitor("other:9", "edge ping", { source_id: "other", refreshes: false })];
  await draw([], null, ports({ monitorRoster: async () => roster }));
  expect(text(rowNamed("payouts /health").querySelector(".tf-mon-acts .tf-refresh"))).toBe("Refresh");
  expect(text(rowNamed("edge ping").querySelector(".tf-mon-acts .tf-refresh"))).toBe("Sync now");

  rowNamed("edge ping").querySelector<HTMLButtonElement>(".tf-refresh")!.click();
  await settle();
  expect(probe.synced).toEqual(["other"]);
  expect(probe.refreshed).toEqual([]);
});

test("Refresh reads the monitor from its source, then the roster again over what it drew, and settles a pause on that read (#2138)", async () => {
  let roster = ROSTER;
  let reads = 0;
  let answer: (rows: MonitorRow[]) => void = () => {};
  const stand = ports({
    monitorRoster: () => {
      reads += 1;
      if (reads === 1) return Promise.resolve(roster);
      return new Promise((resolve) => (answer = resolve));
    },
    submitWrite: async (payload) => {
      writes.push(payload);
      return { id: 40, state: "pending" } as QueuedWrite;
    },
  });
  await draw([], null, stand);
  button(rowNamed("payouts /health"), "Pause")!.click();
  await settle();
  heard.get(40)!(undefined);
  await settle();
  expect(rowNamed("payouts /health").querySelector(".tf-mon-acts .tf-chip")?.textContent).toBe("sent");
  const before = reads;

  const refresh = rowNamed("payouts /health").querySelector<HTMLButtonElement>(".tf-refresh")!;
  refresh.click();
  await settle();
  expect(probe.refreshed).toEqual(["kuma:1"]);
  expect(reads).toBe(before + 1);
  // The roster stays drawn while its read is out, the button saying so.
  expect(rows()).toHaveLength(4);
  expect(text(rowNamed("payouts /health").querySelector(".tf-refresh"))).toBe("Refreshing…");

  roster = ROSTER.map((one) => (one.entity_id === "kuma:1" ? { ...one, tombstoned: true, actions: ["resume_monitor"] } : one));
  answer(roster);
  await settle();
  expect(text(rowNamed("payouts /health").querySelector(".tf-refresh"))).toBe("Refresh");
  expect(rowNamed("payouts /health").querySelector(".tf-mon-acts .tf-chip")).toBeNull();
  expect(button(rowNamed("payouts /health"), "Resume")).toBeDefined();
});

test("a monitor whose refresh failed keeps the roster and says why under its row (#2138)", async () => {
  await draw([], null, ports({ refresh: refreshProbe({ answer: async () => ({ outcome: "failed", reason: "Unreachable" }) }).ports }));
  rowNamed("pg-payments").querySelector<HTMLButtonElement>(".tf-refresh")!.click();
  await settle();
  const failed = target.querySelector(".tf-refresh-failed");
  expect(text(failed)).toBe("Couldn't refresh: Unreachable");
  expect(failed!.closest("tr")!.previousElementSibling).toBe(rowNamed("pg-payments"));
  expect(text(rowNamed("payouts /health").querySelector(".tf-refresh"))).toBe("Refresh");
  expect(rows().filter((tr) => tr.querySelector(".tf-mbar"))).toHaveLength(4);
});

test("a refresh whose roster read a second refresh's read replaced settles on that newer read (#2138)", async () => {
  let reads = 0;
  const answers: ((rows: MonitorRow[]) => void)[] = [];
  await draw(
    [],
    null,
    ports({
      monitorRoster: () => {
        reads += 1;
        if (reads === 1) return Promise.resolve(ROSTER);
        return new Promise((resolve) => answers.push(resolve));
      },
    }),
  );
  rowNamed("payouts /health").querySelector<HTMLButtonElement>(".tf-refresh")!.click();
  await settle();
  rowNamed("pg-payments").querySelector<HTMLButtonElement>(".tf-refresh")!.click();
  await settle();
  expect(answers).toHaveLength(2);

  // The first read answers after the second replaced it, then the second answers.
  answers[0]!(ROSTER);
  await settle();
  answers[1]!(ROSTER);
  await settle();
  expect(text(rowNamed("payouts /health").querySelector(".tf-refresh"))).toBe("Refresh");
  expect(text(rowNamed("pg-payments").querySelector(".tf-refresh"))).toBe("Refresh");
});

test("⌘R on the roster presses no row's Refresh: the page's is the header's Sync now (#2156)", async () => {
  await draw([], null, ports({ monitorRoster: async () => ROSTER }));
  expect(rowNamed("payouts /health").querySelector(".tf-refresh")).not.toBeNull();
  expect(pressRefreshKey().defaultPrevented).toBe(true);
  await settle();
  expect([probe.refreshed, probe.synced]).toEqual([[], []]);
});

test("a long roster draws the monitors near the viewport, spacer rows standing for the rest below (#2210)", async () => {
  const bulk = Array.from({ length: 2000 }, (_, i) => monitor(`kuma:bulk-${i}`, `api-${i} /health`));
  await draw([], null, ports({ monitorRoster: async () => [...ROSTER, ...bulk] }));
  const drawn = target.querySelectorAll(".tf-mon tbody tr:not(.tf-wgap)").length;
  expect(drawn).toBeGreaterThan(5);
  expect(drawn).toBeLessThan(100);
  expect(text(target.querySelector(".tf-mon tbody tr td"))).toBe("flows-stage HTTP");
  expect(target.querySelectorAll(".tf-mon tbody tr.tf-wgap")).toHaveLength(1);
  expect(target.querySelector<HTMLElement>(".tf-mon tbody tr.tf-wgap:last-child td")!.style.getPropertyValue("height")).not.toBe("");

  const field = target.querySelector<HTMLInputElement>("input[data-filter]")!;
  field.value = "api-1999";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(rows().map((tr) => text(tr.querySelector("td")))).toEqual(["api-1999 /health HTTP"]);
});
