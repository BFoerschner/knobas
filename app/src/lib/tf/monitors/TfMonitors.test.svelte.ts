/**
 * Monitors (#864, R-11.3, R-11.4), mounted over stand-in reads: the header and
 * tabs, the roster's order and columns, the scopes, *Ack*, the open monitor
 * with *Pause*, *Resume*, *Hide* and *Refresh*, the hidden monitors after the
 * roster, *Assets nothing watches*, and *Create monitor*'s two writes.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { AssetDetail, AssetEdit, AssetRow, AssetType, MonitorRow, OpenAlert, RouteDetail, RouteEdit, UnmonitoredAsset } from "../../ipc/assets";
import type { Reached } from "../../ipc/entity";
import { EVENTS } from "../../ipc";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import type { Hear } from "../../shell/follow-write";
import { toasts } from "../../shell/toasts.svelte";
import { forgetFilters, sessionFilter } from "../filter/filter-state.svelte";
import type { MonitorPorts } from "./TfMonitors.svelte";
import TfMonitors from "./TfMonitors.svelte";
import { sourceFacts } from "../job/source-facts.fixture";
import { pressRefreshKey, refreshProbe, type RefreshProbe } from "../refresh.fixture";
import { PRESS_HOLD_MS } from "../press-hold.svelte";
import { layers } from "../layers.svelte";

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
    hidden: false,
    assets: [],
    actions: ["pause_monitor"],
    refreshes: true,
    samples: [],
    bar: "u".repeat(48),
    down_since: null,
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
let unacks: string[];
/** Each `set_monitor_hidden` the page made: the monitor, and hidden or not. */
let hides: [string, boolean][];
/** The write follows the page holds, by write id: a test tells each what the queue says. */
let heard: Map<number, Hear>;
/** The page's `sync:state` listeners. */
let syncListeners: Set<(event: { payload: unknown }) => void>;
/** The page's `item:refreshed` listeners (#2105). */
let refreshes: Set<(event: { payload: unknown }) => void>;
/** The rows' *Refresh* and *Sync now* (#2138): the monitors read from their source, the sources run. */
let probe: RefreshProbe;

beforeEach(() => {
  layers.reset();
  toasts.items.splice(0);
  writes = [];
  edits = [];
  routeEdits = [];
  acks = [];
  unacks = [];
  hides = [];
  forgetFilters();
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
    unackAlert: async (id) => {
      unacks.push(id);
      return {} as never;
    },
    setMonitorHidden: async (id, hidden) => {
      hides.push([id, hidden]);
      return hidden;
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
/**
 * The roster's monitors shown (`.tf-mon-row`; one the typed words leave out stays drawn, `hidden`, the ux24 pass), or on
 * *Assets nothing watches* its table's rows.
 */
const rows = () => [...target.querySelectorAll(".tf-mon-row:not([hidden]), tbody tr")];
/** A row's first cell: the monitor's name and type, or the asset's name. */
const first = (tr: Element) => tr.querySelector(".tf-mon-name, td");
/** The open monitor beside the roster (`#/monitors/at/<id>`), where its actions are. */
const panel = () => target.querySelector(".tf-mon-panel")!;
const button = (tr: Element, label: string) =>
  [...tr.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b) === label);

test("a down monitor's chip holds the word, and the time beside it is plain text", async () => {
  const at = new Date(NOW.getTime() - 60 * 60_000).toISOString();
  const down = monitor("kuma:9", "jira", { state: "down", down_since: at });
  await draw([], null, ports({ monitorRoster: async () => [down] }));
  expect(text(rows()[0]!.querySelector(".tf-chip"))).toBe("Down");
  expect(text(rows()[0]!.querySelector(".tf-mon-since"))).toMatch(/^since \d\d:\d\d$/);
});

test("Hide and Unhide tell the nav's badge, which leaves a hidden monitor out", async () => {
  let told = 0;
  await draw(["at", "kuma:2"], "PAY-231", ports({ badgesChanged: () => (told += 1) }));
  button(panel(), "Hide")!.click();
  await settle();
  expect(told).toBe(1);
});

test("#/monitors draws the header, both tabs with counts, and the roster sorted down, warning, paused, up", async () => {
  await draw([]);
  expect(text(target.querySelector("h1"))).toBe("Monitors");
  expect(text(target.querySelector(".tf-sub"))).toBe(
    "4 checks from Uptime Kuma, synced 1 min ago. 2 of them watch an asset.",
  );
  const tabs = [...target.querySelectorAll(".tf-tabs a")];
  expect(tabs.map(text)).toEqual(["Monitors 4", "Assets nothing watches 2"]);
  expect(tabs[0]!.getAttribute("aria-current")).toBe("page");
  expect(rows().map((tr) => text(first(tr)))).toEqual([
    "flows-stage HTTP",
    "pg-payments TCP",
    "yard-planner HTTP",
    "payouts /health HTTP",
  ]);
  const [down, , , up] = rows();
  // Up is a quiet word, not a chip; what is wrong is a chip, its tone down the row's edge.
  const cells = [...up!.children].map(text);
  expect(cells).toEqual(["payouts /health HTTP", "Up", "", "99.98 %", "", "payouts"]);
  expect(up!.querySelector(".tf-chip")).toBeNull();
  expect(down!.classList.contains("tf-mon-err")).toBe(true);
  expect(text(down!.querySelector(".tf-chip.tf-err"))).toBe("Down");
  // The strip is one element, its 48 half-hours its background (`stripPaint`): 48 bars of 2 px a pixel apart.
  const strip = up!.querySelector<HTMLElement>(".tf-mbar")!;
  expect(strip.children).toHaveLength(0);
  expect(strip.style.width).toBe("143px");
  expect(up!.querySelector<HTMLAnchorElement>(".tf-mon-watch a")?.getAttribute("href")).toBe("#/asset/asset%3Ar");
  // The name is the row's button, which opens the monitor beside the roster.
  expect(up!.querySelector(".tf-mon-name button.tf-mon-open")?.getAttribute("aria-expanded")).toBe("false");
  // What is wrong, first, in words a glance takes in.
  expect(text(target.querySelector(".tf-mon-sum"))).toBe("1 down 1 slow 1 up 1 paused");
  expect(target.querySelector(".tf-mon-sum .tf-mon-figerr")).not.toBeNull();
});

test("every monitor up is one quiet figure, and a row's response is drawn only while it is slow", async () => {
  await draw([], null, ports({ monitorRoster: async () => [monitor("kuma:1", "a"), monitor("kuma:2", "b")] }));
  expect(text(target.querySelector(".tf-mon-sum"))).toBe("2 up");
  expect(rows().map((tr) => text(tr.querySelector(".tf-mon-state")))).toEqual(["Up", "Up"]);
  unmount(view!);
  view = null;
  await draw([], null);
  expect(text(rows()[1]!.querySelector(".tf-mon-state"))).toBe("Slow 1 900 ms");
});

test("a row press opens the monitor beside the roster on the page, Esc's panel, and the address's opens it too", async () => {
  await draw([]);
  const before = location.hash;
  rows()[3]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  flushSync();
  // No address of its own: nothing above the roster comes or goes as it opens.
  expect(location.hash).toBe(before);
  expect(rows()[3]!.getAttribute("aria-current")).toBe("true");
  expect(text(panel().querySelector("h2"))).toBe("payouts /health");
  expect(layers.open.map((layer) => layer.kind)).toEqual(["panel"]);
  layers.open[0]!.close();
  flushSync();
  expect(target.querySelector(".tf-mon-panel")).toBeNull();
  expect(layers.open).toEqual([]);
  // The name's button does the same from the keyboard, and pressed again closes it.
  rows()[0]!.querySelector<HTMLButtonElement>(".tf-mon-open")!.click();
  flushSync();
  expect(text(panel().querySelector("h2"))).toBe("flows-stage");
  rows()[0]!.querySelector<HTMLButtonElement>(".tf-mon-open")!.click();
  flushSync();
  expect(target.querySelector(".tf-mon-panel")).toBeNull();
  unmount(view!);
  view = null;
  await draw(["at", "kuma:1"]);
  expect(rows()[3]!.getAttribute("aria-current")).toBe("true");
  // Opened by its address, the way back is the address's: no panel of the page's on the stack.
  expect(layers.open).toEqual([]);
  expect(text(panel().querySelector("h2"))).toBe("payouts /health");
  expect(text(panel().querySelector(".tf-mon-facts"))).toContain("Response 140 ms");
  expect(text(panel().querySelector(".tf-mon-facts"))).toContain("99.98 % over 30 days");
  expect(text(panel().querySelector("header button"))).toBe("Close");
  expect([...panel().querySelectorAll(".tf-mon-pacts button")].map(text)).toEqual(["Pause", "Hide", "Refresh"]);
  expect(panel().querySelector<HTMLElement>(".tf-mbar")!.style.width).toBe("287px");
});

test("Ack is on a down monitor only, acks it by the monitor, and reads Acked", async () => {
  await draw([]);
  const [down, warn] = rows();
  expect(button(warn!, "Ack")).toBeUndefined();
  button(down!, "Ack")!.click();
  await settle();
  expect(acks).toEqual(["kuma:2"]);
  expect(button(down!, "Acked")?.getAttribute("aria-pressed")).toBe("true");
  expect(toasts.items.map((t) => t.text)).toContain("Acked. The alert stays open until the monitor recovers.");
});

const alertOf = (monitor_id: string, acked_at: string | null): OpenAlert =>
  ({ id: 1, monitor_id, monitor_name: "flows-stage", state: "down", opened_at: "2026-10-16T12:00:00Z", acked_at }) as OpenAlert;

test("a down row whose open alert is acked reads a pressed Acked on first draw, and Ack when it is not", async () => {
  await draw([], "PAY-231", ports({ openAlerts: async () => [alertOf("kuma:2", "2026-10-16T13:00:00Z")] }));
  const down = rows()[0]!;
  expect(button(down, "Ack")).toBeUndefined();
  expect(button(down, "Acked")?.getAttribute("aria-pressed")).toBe("true");
  expect(button(down, "Acked")?.title).toContain("take it back");
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
  expect(button(rows()[0]!, "Acked")).toBeDefined();
  unmount(view!);
  view = null;
  await draw([], "PAY-231", stand);
  expect(button(rows()[0]!, "Acked")).toBeDefined();
  expect(button(rows()[0]!, "Ack")).toBeUndefined();
});

test("Acked holds about two seconds, so a second press isn't an un-ack, and Undo and a later press still are (F-351)", async () => {
  await draw([]);
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    const run = async (ms: number) => {
      await vi.advanceTimersByTimeAsync(ms);
      flushSync();
    };
    button(rows()[0]!, "Ack")!.click();
    await run(10);
    button(rows()[0]!, "Acked")!.click();
    await run(10);
    expect(unacks, "the second press lands on the hold").toEqual([]);
    expect(button(rows()[0]!, "Acked")).toBeDefined();
    toasts.items.find((t) => t.text.startsWith("Acked"))!.action!.run();
    await run(10);
    expect(unacks, "Undo isn't held").toEqual(["kuma:2"]);

    button(rows()[0]!, "Ack")!.click();
    await run(10);
    await run(PRESS_HOLD_MS);
    button(rows()[0]!, "Acked")!.click();
    await run(10);
    expect(unacks, "after the hold, a press takes it back").toEqual(["kuma:2", "kuma:2"]);
  } finally {
    vi.useRealTimers();
  }
});

test("Undo on the ack's toast takes it back", async () => {
  await draw([]);
  const down = rows()[0]!;
  button(down, "Ack")!.click();
  await settle();
  const toast = toasts.items.find((t) => t.text.startsWith("Acked"))!;
  expect(toast.action?.label).toBe("Undo");
  toast.action!.run();
  await settle();
  expect(unacks).toEqual(["kuma:2"]);
  expect(button(rows()[0]!, "Ack")).toBeDefined();
  expect(toasts.items.at(-1)!.text).toContain("is asking again");
});

test("Pause and Resume queue the source's op for that monitor", async () => {
  await draw(["at", "kuma:1"]);
  button(panel(), "Pause")!.click();
  await settle();
  unmount(view!);
  view = null;
  await draw(["at", "kuma:3"]);
  button(panel(), "Resume")!.click();
  await settle();
  expect(writes).toEqual([{ PauseMonitor: { entity: "kuma:1" } }, { ResumeMonitor: { entity: "kuma:3" } }]);
  expect(toasts.items.map((t) => t.text).filter((t) => t.startsWith("Queued:"))).toEqual([
    "Queued: pause payouts /health.",
    "Queued: resume yard-planner.",
  ]);
});

/** The line the hidden monitors are drawn under, after the roster. */
const hiddenLine = () => target.querySelector(".tf-mon-hidhead");
const names = () => rows().map((tr) => text(first(tr)).replace(/ HTTP$| TCP$/, ""));
const count = () => text(target.querySelector(".tf-fcount")) + text(target.querySelector(".tf-mon-hidn"));
const row = (name: string) => rows().find((tr) => text(first(tr)).startsWith(name))!;

test("Hide moves a monitor after the roster under its line, dimmed, with no tone and no Ack; Unhide brings it back", async () => {
  let roster = ROSTER;
  const stand = ports({
    monitorRoster: async () => roster,
    setMonitorHidden: async (id, hidden) => {
      hides.push([id, hidden]);
      roster = roster.map((each) => (each.entity_id === id ? { ...each, hidden } : each));
      return hidden;
    },
  });
  await draw(["at", "kuma:2"], "PAY-231", stand);
  expect(hiddenLine()).toBeNull();
  expect(button(row("flows-stage"), "Ack")).toBeDefined();
  button(panel(), "Hide")!.click();
  await settle();
  expect(hides).toEqual([["kuma:2", true]]);
  // Every hidden monitor is drawn, at once: nothing folds, so nothing redraws or moves as it opens.
  expect(names()).toEqual(["pg-payments", "yard-planner", "payouts /health", "flows-stage"]);
  expect(text(hiddenLine())).toBe("1 hidden monitor Its alerts don’t reach the inbox or notify you.");
  expect(hiddenLine()!.nextElementSibling!.firstElementChild).toBe(row("flows-stage"));
  expect(count()).toBe("3 monitors, 1 hidden");
  expect(toasts.items.map((t) => t.text)).toContain("Hid flows-stage. Its alerts stay quiet.");
  const hidden = row("flows-stage");
  expect(hidden.classList.contains("tf-mon-hid")).toBe(true);
  expect(hidden.classList.contains("tf-mon-err")).toBe(false);
  expect(hidden.querySelector(".tf-chip")).toBeNull();
  expect(button(hidden, "Ack")).toBeUndefined();
  expect(text(target.querySelector(".tf-mon-sum")), "a hidden monitor is not down as far as the page says").toBe("0 down 1 slow 1 up 1 paused");
  expect(button(panel(), "Hide")).toBeUndefined();
  // The press turned *Hide* into *Unhide* under the pointer, so it holds, as *Acked* does (F-351): a double press is one hide.
  button(panel(), "Unhide")!.click();
  await settle();
  expect(hides).toEqual([["kuma:2", true]]);
  toasts.items.find((t) => t.text.startsWith("Hid "))!.action!.run();
  await settle();
  expect(hides).toEqual([
    ["kuma:2", true],
    ["kuma:2", false],
  ]);
  expect(names()).toEqual(["flows-stage", "pg-payments", "yard-planner", "payouts /health"]);
  expect(hiddenLine()).toBeNull();
  expect(count()).toBe("4 monitors");
});

test("an open monitor that is hidden stays with the hidden monitors, counted once", async () => {
  const hiddenTwo = ROSTER.map((each) => (each.entity_id === "kuma:2" || each.entity_id === "kuma:1" ? { ...each, hidden: true } : each));
  await draw(["at", "kuma:2"], "PAY-231", ports({ monitorRoster: async () => hiddenTwo }));
  expect(target.querySelector(".tf-mon-row[aria-current]")?.textContent).toContain("flows-stage");
  expect(text(hiddenLine())).toMatch(/^2 hidden monitors /);
  expect(names()).toEqual(["pg-payments", "yard-planner", "flows-stage", "payouts /health"]);
  expect(count()).toBe("2 monitors, 2 hidden");
});

test("every filter applies to the hidden monitors: a search only they answer draws them, and none matching draws no line", async () => {
  await draw([], "PAY-231", ports({ monitorRoster: async () => ROSTER.map((each) => (each.entity_id === "kuma:1" ? { ...each, hidden: true } : each)) }));
  expect(names()).toEqual(["flows-stage", "pg-payments", "yard-planner", "payouts /health"]);
  sessionFilter("monitors").text = "payouts";
  await settle();
  expect(names()).toEqual(["payouts /health"]);
  expect(target.querySelector(".tf-empty")).toBeNull();
  expect(count()).toBe("0 of 3 monitors, 1 hidden");
  sessionFilter("monitors").text = "";
  sessionFilter("monitors").chips["st"] = ["Down"];
  await settle();
  expect(names()).toEqual(["flows-stage"]);
  expect(hiddenLine()).toBeNull();
  expect(count()).toBe("1 of 3 monitors");
});

test("a hide the backend refuses keeps the row and says why", async () => {
  await draw(["at", "kuma:1"], "PAY-231", ports({ setMonitorHidden: async () => Promise.reject({ code: "not_found", message: "no monitor \"kuma:1\" in the mirror" }) }));
  button(panel(), "Hide")!.click();
  await settle();
  expect(names()).toContain("payouts /health");
  expect(toasts.items.some((t) => t.tone === "err")).toBe(true);
});

test("This job keeps the monitors watching what the job reaches; My jobs and All offer the rest", async () => {
  await draw([]);
  const scope = (label: string) =>
    [...target.querySelectorAll<HTMLButtonElement>("button")].find((b) => text(b).startsWith(label))!;
  scope("This job").click();
  await settle();
  expect(rows().map((tr) => text(first(tr)))).toEqual(["flows-stage HTTP"]);
  scope("All").click();
  await settle();
  expect(rows()).toHaveLength(4);
});

test("#/monitors/unmonitored lists the assets and routes worth a check with env and owner, and has no filter bar", async () => {
  await draw(["unmonitored"]);
  expect(target.querySelector(".tf-tabs [aria-current='page']")?.textContent).toContain("Assets nothing watches");
  expect(text(target.querySelector(".tf-sub"))).toBe("Containers, services, routes, databases and VMs that no monitor watches.");
  expect(rows().map((tr) => [...tr.querySelectorAll("td")].map(text))).toEqual([
    ["pg-main", "Database server", "prod", "ops", "Create monitor…"],
    ["Gitea", "Route", "prod", "ops", "Create monitor…"],
  ]);
  expect(rows()[1]!.querySelector("td a")?.getAttribute("href")).toBe("#/route/route%3Agitea");
  // Its own band, over its own rows.
  expect(target.querySelector<HTMLInputElement>(".tf-fbar input")?.placeholder).toBe("Filter assets");
});

test("Create monitor on a route opens as HTTP on its URL plus /health and records the name on the route", async () => {
  await draw(["unmonitored"]);
  button(rows()[1]!, "Create monitor…")!.click();
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
  button(rows()[0]!, "Create monitor…")!.click();
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
  button(rows()[0]!, "Create monitor…")!.click();
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
  expect(button(pg!, "Create monitor…")).toBeUndefined();
  expect(button(route!, "Create monitor…")).toBeDefined();
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
  button(rows()[0]!, "Create monitor…")!.click();
  await settle();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => text(b) === "Create monitor")!.click();
  await settle();
  expect(reads).toBe(2);
  expect(lastCell(rows()[0]!)).toBe("Monitor queued");
  answer();
  await settle();
  expect(lastCell(rows()[0]!)).toBe("Monitor queued");
  expect(button(rows()[0]!, "Create monitor…")).toBeUndefined();
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
  button(rows()[0]!, "Create monitor…")!.click();
  await settle();
  [...document.querySelectorAll<HTMLButtonElement>(".tf-dialog footer button")].find((b) => text(b) === "Create monitor")!.click();
  await settle();
  expect(lastCell(rows()[0]!)).toBe("Monitor queued");

  going = null;
  heard.get(1)!(onItsWay(1, "refused", "no Docker host"));
  await settle();
  expect(button(rows()[0]!, "Create monitor…")).toBeDefined();
  expect(toasts.items.map((t) => t.text)).toContain("Monitor pg-main was refused: no Docker host");
});

test("after a reload, a pending create is followed: held on drift, it gives Create monitor back", async () => {
  let going: QueuedWrite | null = onItsWay(7, "pending");
  await draw(["unmonitored"], null, ports(withCreate(() => going)));
  expect([...heard.keys()]).toEqual([7]);
  going = null;
  heard.get(7)!(onItsWay(7, "held"));
  await settle();
  expect(button(rows()[0]!, "Create monitor…")).toBeDefined();
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
  expect(button(rows()[0]!, "Create monitor…")).toBeUndefined();
  expect([...heard.keys()]).toEqual([7]);

  going = null;
  heard.get(7)!(undefined);
  await settle();
  expect(button(rows()[0]!, "Create monitor…")).toBeDefined();
});

test("a pending create the queue holds as unconfirmed stays Monitor queued and followed, with no Edit and resend (#1966)", async () => {
  let going: QueuedWrite | null = onItsWay(7, "pending");
  await draw(["unmonitored"], null, ports(withCreate(() => going)));
  going = unconfirmed(7);
  heard.get(7)!(unconfirmed(7));
  await settle();
  expect(lastCell(rows()[0]!)).toBe("Monitor queued");
  expect(button(rows()[0]!, "Create monitor…")).toBeUndefined();
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
  expect(button(rows()[0]!, "Create monitor…")).toBeDefined();
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
  expect(rows().map((tr) => text(first(tr)))).toEqual(["Gitea"]);
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
  button(rows()[0]!, "Create monitor…")!.click();
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
  expect(rows().map((tr) => text(first(tr)))).toEqual(["Gitea"]);
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
  const props = $state({ ready: true, rest: ["at", "kuma:3"], job: null as string | null, ports: stand, now: NOW });
  view = mount(TfMonitors, { target, props });
  await settle();
  const row = (name: string) => rows().find((tr) => text(first(tr)).startsWith(name))!;
  // In the State cell, after the state chip, because it says the state is changing (F-337, F-349).
  const echo = (name: string) => row(name).querySelector(".tf-mon-state .tf-queued-mark")?.textContent ?? null;
  button(panel(), "Resume")!.click();
  await settle();
  props.rest = ["at", "kuma:1"];
  await settle();
  button(panel(), "Pause")!.click();
  await settle();
  expect([echo("payouts /health"), echo("yard-planner")]).toEqual(["Queued", "Queued"]);
  expect(text(panel().querySelector(".tf-mon-pstate .tf-queued-mark"))).toBe("Queued");
  // The button stays where it was, turned off, and a second press is nothing.
  const pause = button(panel(), "Pause")!;
  expect(pause.getAttribute("aria-disabled")).toBe("true");
  pause.click();
  await settle();
  expect(writes.filter((write) => "PauseMonitor" in write)).toHaveLength(1);

  heard.get(30)!(undefined);
  heard.get(31)!(undefined);
  await settle();
  expect([echo("payouts /health"), echo("yard-planner")]).toEqual(["Sent", "Sent"]);

  // A run's end before Kuma's /metrics moved: still on its way.
  syncEnds();
  await settle();
  expect([echo("payouts /health"), echo("yard-planner")]).toEqual(["Sent", "Sent"]);

  roster = ROSTER.map((one) =>
    one.entity_id === "kuma:1" ? { ...one, tombstoned: true, actions: ["resume_monitor"] } : one.entity_id === "kuma:3" ? { ...one, tombstoned: false, actions: ["pause_monitor"] } : one,
  );
  refreshed("kuma:1", 30);
  await settle();
  expect([echo("payouts /health"), echo("yard-planner")]).toEqual([null, null]);
  expect(button(panel(), "Resume")).toBeDefined();
});

// -- Refresh (#2138) ----------------------------------------------------------

const rowNamed = (name: string) => rows().find((tr) => text(first(tr)).startsWith(name))!;

test("an open monitor whose source reads one monitor draws Refresh, one whose source does not draws Sync now (#2138)", async () => {
  const roster = [monitor("kuma:1", "payouts /health"), monitor("other:9", "edge ping", { source_id: "other", refreshes: false })];
  await draw(["at", "kuma:1"], null, ports({ monitorRoster: async () => roster }));
  expect(text(panel().querySelector(".tf-refresh"))).toBe("Refresh");
  unmount(view!);
  view = null;
  await draw(["at", "other:9"], null, ports({ monitorRoster: async () => roster }));
  expect(text(panel().querySelector(".tf-refresh"))).toBe("Sync now");
  panel().querySelector<HTMLButtonElement>(".tf-refresh")!.click();
  await settle();
  expect(probe.synced).toEqual(["other"]);
  expect(probe.refreshed).toEqual([]);
});

test("the open monitor's actions are small buttons, Refresh after Pause and Hide, and no row holds them (#2288)", async () => {
  await draw(["at", "kuma:1"], null, ports({ monitorRoster: async () => [monitor("kuma:1", "payouts /health", { web_url: "http://kuma.test/dashboard/1" })] }));
  expect([...panel().querySelectorAll(".tf-mon-pacts :is(button, a)")].map((b) => [text(b), b.className])).toEqual([
    ["Pause", "tf-btn tf-sm"],
    ["Hide", "tf-btn tf-sm"],
    ["Refresh", "tf-btn tf-sm tf-refresh"],
    ["Open in Uptime Kuma", "tf-btn tf-sm tf-ghost"],
  ]);
  expect(rowNamed("payouts /health").querySelector("button:not(.tf-mon-open)")).toBeNull();
});

test("an open monitor's Open in names its own source and opens its own page: two monitoring sources, one an installed adapter's", async () => {
  // The build-header pass, 2026-11-20: it said "Open in Uptime Kuma" whatever source the monitor came from.
  const sources = async () => [
    sourceFacts("kuma", { display_name: "Kuma (EU)" }),
    sourceFacts("pingly", { display_name: "Pingly, an installed adapter" }),
  ];
  const rows = [
    monitor("kuma:1", "payouts /health", { web_url: "http://kuma.test/dashboard/1" }),
    { ...monitor("pingly:7", "ledger /ready", { web_url: "https://pingly.test/checks/7" }), source_id: "pingly" },
  ];
  for (const [id, words, url] of [
    ["kuma:1", "Open in Kuma (EU)", "http://kuma.test/dashboard/1"],
    ["pingly:7", "Open in Pingly, an installed adapter", "https://pingly.test/checks/7"],
  ] as const) {
    if (view) unmount(view);
    view = null;
    await draw(["at", id], null, ports({ monitorRoster: async () => rows, listSources: sources }));
    const open = [...panel().querySelectorAll<HTMLAnchorElement>(".tf-mon-pacts a")].find((a) => text(a).startsWith("Open in"))!;
    expect([text(open), open.title, open.getAttribute("href")]).toEqual([words, words, url]);
  }
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
  await draw(["at", "kuma:1"], null, stand);
  button(panel(), "Pause")!.click();
  await settle();
  heard.get(40)!(undefined);
  await settle();
  expect(rowNamed("payouts /health").querySelector(".tf-mon-state .tf-queued-mark")?.textContent).toBe("Sent");
  const before = reads;

  const refresh = panel().querySelector<HTMLButtonElement>(".tf-refresh")!;
  refresh.click();
  await settle();
  expect(probe.refreshed).toEqual(["kuma:1"]);
  expect(reads).toBe(before + 1);
  // The roster stays drawn while its read is out, the button saying so.
  expect(rows()).toHaveLength(4);
  expect(text(panel().querySelector(".tf-refresh"))).toBe("Refreshing…");

  roster = ROSTER.map((one) => (one.entity_id === "kuma:1" ? { ...one, tombstoned: true, actions: ["resume_monitor"] } : one));
  answer(roster);
  await settle();
  expect(text(panel().querySelector(".tf-refresh"))).toBe("Refresh");
  expect(rowNamed("payouts /health").querySelector(".tf-queued-mark")).toBeNull();
  expect(button(panel(), "Resume")).toBeDefined();
});

test("a monitor whose refresh failed keeps the roster and says why under its actions (#2138)", async () => {
  await draw(["at", "kuma:4"], null, ports({ refresh: refreshProbe({ answer: async () => ({ outcome: "failed", reason: "Unreachable" }) }).ports }));
  panel().querySelector<HTMLButtonElement>(".tf-refresh")!.click();
  await settle();
  const failed = target.querySelector(".tf-refresh-failed");
  expect(text(failed)).toBe("Couldn't refresh: Unreachable");
  expect(failed!.previousElementSibling!.classList.contains("tf-mon-pacts")).toBe(true);
  expect(rows().filter((tr) => tr.querySelector(".tf-mbar"))).toHaveLength(4);
});

test("a refresh whose roster read a second refresh's read replaced settles on that newer read (#2138)", async () => {
  let reads = 0;
  const answers: ((rows: MonitorRow[]) => void)[] = [];
  const props = $state({
    ready: true,
    rest: ["at", "kuma:1"],
    job: null as string | null,
    now: NOW,
    ports: ports({
      monitorRoster: () => {
        reads += 1;
        if (reads === 1) return Promise.resolve(ROSTER);
        return new Promise<MonitorRow[]>((resolve) => answers.push(resolve));
      },
    }),
  });
  view = mount(TfMonitors, { target, props });
  await settle();
  panel().querySelector<HTMLButtonElement>(".tf-refresh")!.click();
  await settle();
  props.rest = ["at", "kuma:4"];
  await settle();
  panel().querySelector<HTMLButtonElement>(".tf-refresh")!.click();
  await settle();
  expect(answers).toHaveLength(2);

  // The first read answers after the second replaced it, then the second answers.
  answers[0]!(ROSTER);
  await settle();
  answers[1]!(ROSTER);
  await settle();
  expect(text(panel().querySelector(".tf-refresh"))).toBe("Refresh");
  props.rest = ["at", "kuma:1"];
  await settle();
  expect(text(panel().querySelector(".tf-refresh"))).toBe("Refresh");
});

test("⌘R on the roster presses no row's Refresh: the page's is the header's Sync now (#2156)", async () => {
  await draw(["at", "kuma:1"], null, ports({ monitorRoster: async () => ROSTER }));
  expect(panel().querySelector(".tf-refresh")).not.toBeNull();
  expect(pressRefreshKey().defaultPrevented).toBe(true);
  await settle();
  expect([probe.refreshed, probe.synced]).toEqual([[], []]);
});

/** Until every block of the roster holds its rows (`drawnBlocks`: a few blocks a frame after the first draw). */
async function filled() {
  for (let i = 0; i < 200 && target.querySelector(".tf-mon-later") !== null; i += 1) await new Promise((resolve) => setTimeout(resolve, 10));
  flushSync();
  expect(target.querySelector(".tf-mon-later"), "every block drawn").toBeNull();
}

test("the first blocks draw at once and the rest a frame or two later, each holding its rows' room meanwhile (the ux30 Windows pass)", async () => {
  const bulk = Array.from({ length: 300 }, (_, i) => monitor(`kuma:bulk-${i}`, `api-${i} /health`));
  await draw([], null, ports({ monitorRoster: async () => [...ROSTER, ...bulk] }));
  // 304 monitors are 19 blocks of 16: the first four are drawn in the first draw, more than a full-screen window holds.
  const blocks = [...target.querySelectorAll<HTMLElement>(".tf-mon-block")];
  expect(blocks).toHaveLength(19);
  expect(blocks.slice(0, 4).every((b) => !b.classList.contains("tf-mon-later") && b.querySelectorAll(".tf-mon-row").length === 16)).toBe(true);
  const later = blocks.slice(4);
  expect(later.every((b) => b.classList.contains("tf-mon-later") && b.querySelector(".tf-mon-row") === null)).toBe(true);
  // A block waiting for its rows holds their room: the last one its 16, as a drawn one does.
  expect(later.map((b) => b.style.getPropertyValue("--tf-mon-rows"))).toEqual(later.map(() => "16"));
  expect(rows()).toHaveLength(64);
  await filled();
  expect(rows()).toHaveLength(304);
  // The blocks are the elements they were: filling one builds its rows, never the block.
  expect([...target.querySelectorAll(".tf-mon-block")]).toEqual(blocks);
});

test("every monitor is drawn, each one row of one height, and a typed word shows the rows it matches without building any (the monitors-page pass, ux24)", async () => {
  const bulk = Array.from({ length: 300 }, (_, i) => monitor(`kuma:bulk-${i}`, i < 60 ? `gate-${i} /health` : `api-${i} /health`));
  await draw([], null, ports({ monitorRoster: async () => [...ROSTER, ...bulk] }));
  await filled();
  // No windowing and no spacer rows: a roster this size is cheaper drawn whole than kept near the viewport.
  expect(rows()).toHaveLength(304);
  expect(target.querySelector(".tf-wgap")).toBeNull();
  const kept = rows()[10];
  const field = target.querySelector<HTMLInputElement>("input[data-filter]")!;
  field.value = "gate-";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(rows()).toHaveLength(60);
  expect(target.querySelectorAll(".tf-mon-row"), "a row the word leaves out stays drawn, hidden").toHaveLength(304);
  expect((kept as HTMLElement).hidden).toBe(true);
  // Blocks of 16 the word left no row in are hidden as a whole, and each block's room is its shown rows'.
  const blocks = [...target.querySelectorAll<HTMLElement>(".tf-mon-block")];
  expect(blocks).toHaveLength(Math.ceil(304 / 16));
  for (const b of blocks) {
    const open = b.querySelectorAll(".tf-mon-row:not([hidden])").length;
    expect(b.hidden).toBe(open === 0);
    if (open > 0) expect(b.style.getPropertyValue("--tf-mon-rows")).toBe(String(open));
  }
  expect(blocks.some((b) => b.hidden)).toBe(true);
  // The first shown row draws no rule over it, whichever row that is.
  expect([...target.querySelectorAll(".tf-mon-top")]).toEqual([rows()[0]]);
  // Backspace to nothing typed: every row shows again, and every one is the element it was (none built again).
  const before = [...target.querySelectorAll(".tf-mon-row")];
  field.value = "";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await settle();
  expect(rows()).toHaveLength(304);
  expect(rows(), "a Backspace shows the rows it hid, not new ones").toEqual(before);
  expect(kept!.isConnected && !(kept as HTMLElement).hidden).toBe(true);
  expect(blocks.every((b) => !b.hidden && b.isConnected)).toBe(true);
  expect([...target.querySelectorAll(".tf-mon-top")]).toEqual([rows()[0]]);
});

test("a word no monitor holds keeps the list drawn and hidden and says so, and Backspace shows it again", async () => {
  await draw([], null);
  const list = target.querySelector(".tf-mon-list")!;
  sessionFilter("monitors").text = "zzz-nothing";
  await settle();
  expect(rows()).toHaveLength(0);
  expect(target.querySelector<HTMLElement>(".tf-mon-list")).toBe(list);
  expect((list as HTMLElement).hidden).toBe(true);
  expect(target.querySelector(".tf-empty, .tf-nomatch")).not.toBeNull();
  sessionFilter("monitors").text = "";
  await settle();
  expect((list as HTMLElement).hidden).toBe(false);
  expect(rows()).toHaveLength(ROSTER.length);
});

test("a read that changes one monitor draws that row again and leaves every other as it was", async () => {
  let read = ROSTER;
  const props = $state({ ready: true, rest: [] as string[], job: null as string | null, ports: ports({ monitorRoster: async () => read }), now: NOW, syncRevision: 0 });
  view = mount(TfMonitors, { target, props });
  await settle();
  const before = rows();
  // A poll: every response moved, every monitor was read again, and pg-payments' newest half-hour went down.
  read = ROSTER.map((one) => ({ ...one, response_time_ms: (one.response_time_ms ?? 0) + 1, checked_at: "2026-10-16T14:00:00Z", bar: one.entity_id === "kuma:1" ? `${"u".repeat(47)}d` : one.bar }));
  const seen = new Set<Element>();
  const watch = new MutationObserver((list) => {
    for (const m of list) {
      const row = (m.target instanceof Element ? m.target : m.target.parentElement)?.closest(".tf-mon-row");
      if (row) seen.add(row);
    }
  });
  watch.observe(target, { subtree: true, childList: true, attributes: true, characterData: true });
  props.syncRevision = 1;
  await settle();
  watch.disconnect();
  expect(rows()).toEqual(before);
  // pg-payments is slow, so its response is drawn and moved; payouts' bar changed; the rest drew nothing.
  expect([...seen].map((tr) => text(first(tr)))).toEqual(["pg-payments TCP", "payouts /health HTTP"]);
});

const NO_TOKEN = { id: "kuma", health: { state: "missing_secret", checked_at: "2026-08-22T08:00:00Z" }, auth_kind: "ApiToken" };

test("a source with no token draws the band with its fix, and the header stops saying synced", async () => {
  await draw([], "PAY-231", ports({ listSources: async () => [{ ...sourceFacts("kuma", { display_name: "Uptime Kuma" }), ...NO_TOKEN } as never], syncStatus: async () => [] }));
  const band = target.querySelector(".tf-stale")!;
  expect(band.textContent).toContain("no token stored. Pauses and new monitors wait in the queue until it works again.");
  expect(band.querySelector("a")?.textContent).toBe("Enter token…");
  expect(band.querySelector("a")?.getAttribute("href")).toBe("#/settings/source/kuma/reenter");
  expect(target.querySelector(".tf-fhead .tf-sub")?.textContent).not.toContain("synced");
});

test("a source that syncs draws no band, and the header says when", async () => {
  await draw([], "PAY-231", ports({ syncStatus: async () => [] }));
  expect(target.querySelector(".tf-stale")).toBeNull();
  expect(target.querySelector(".tf-fhead .tf-sub")?.textContent).toContain("synced");
});

test("an alert that closed under the row says so in words, not as a failure, and the roster is read again", async () => {
  let reads = 0;
  const gone = { code: "not_found", message: 'no open alert on "kuma:2"', source_id: null };
  await draw([], "PAY-231", ports({
    monitorRoster: async () => {
      reads += 1;
      return ROSTER;
    },
    ackAlert: async () => Promise.reject(gone),
  }));
  const before = reads;
  button(rows()[0]!, "Ack")!.click();
  await settle();
  const said = toasts.items.at(-1)!;
  expect(said.text).toMatch(/ has recovered, so its alert is closed\.$/);
  expect(said.tone).toBeUndefined();
  expect(reads).toBeGreaterThan(before);
});

test("taking an ack back takes its toast's Undo with it", async () => {
  await draw([]);
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  try {
    const run = async (ms: number) => {
      await vi.advanceTimersByTimeAsync(ms);
      flushSync();
    };
    button(rows()[0]!, "Ack")!.click();
    await run(10);
    expect(toasts.items.some((t) => t.text.startsWith("Acked"))).toBe(true);
    await run(PRESS_HOLD_MS);
    button(rows()[0]!, "Acked")!.click();
    await run(10);
    expect(toasts.items.some((t) => t.text.startsWith("Acked"))).toBe(false);
  } finally {
    vi.useRealTimers();
  }
});

test("the page shown again reads the sources again, so a token entered in Settings takes the band away", async () => {
  let health = "missing_secret";
  const stand = ports({
    listSources: async () => [{ ...sourceFacts("kuma", { display_name: "Uptime Kuma" }), id: "kuma", health: { state: health, checked_at: "2026-08-22T08:00:00Z" }, auth_kind: "ApiToken" } as never],
    syncStatus: async () => [],
  });
  const props = $state({ ready: true, rest: [] as string[], job: "PAY-231" as string | null, ports: stand, now: NOW, syncRevision: 0 });
  view = mount(TfMonitors, { target, props });
  await settle();
  expect(target.querySelector(".tf-stale")).not.toBeNull();
  health = "ok";
  props.syncRevision = 1;
  await settle();
  expect(target.querySelector(".tf-stale")).toBeNull();
  expect(target.querySelector(".tf-fhead .tf-sub")?.textContent).toContain("synced");
});

test("a kept page's bars move with each roster read: a check that went down since the page first drew shows down", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  try {
    vi.setSystemTime(NOW);
    let read = [monitor("kuma:1", "payouts /health")];
    const props = $state({ ready: true, rest: [] as string[], job: null, ports: ports({ monitorRoster: async () => read }), syncRevision: 0 });
    view = mount(TfMonitors, { target, props });
    await settle();
    // Two hours on, a sync brings a sample of the check down; the page reads the roster again.
    vi.setSystemTime(new Date(NOW.getTime() + 2 * 3_600_000));
    read = [monitor("kuma:1", "payouts /health", { state: "down", down_since: "2026-10-16T15:55:00Z", bar: `${"-".repeat(43)}uuuud` })];
    props.syncRevision = 1;
    await settle();
    const strip = target.querySelector<HTMLElement>(".tf-mbar")!;
    // The newest half-hour is the strip's right end: down, painted in --err, not a gap.
    expect(strip.style.getPropertyValue("background-image")).toMatch(/var\(--err\) 141px 143px\)$/);
  } finally {
    vi.useRealTimers();
  }
});

// ---- what a monitor watches (2026-11-19: Assets has its nav entry again, and the open monitor's *Watches* offered nothing) ----

const assetRow = (id: string, name: string, parent_id: string | null, over: Partial<AssetRow> = {}): AssetRow => ({
  id,
  parent_id,
  type_id: "service",
  type_label: "Service",
  monogram: "Sv",
  name,
  status: "up",
  environment: null,
  owner: null,
  has_children: false,
  health: "up",
  inside: "none",
  problems_inside: 0,
  linked_work: 0,
  ...over,
});
const HOST = assetRow("asset:h", "hel1", null, { type_id: "vm", type_label: "VM", monogram: "Vm", has_children: true });
const SVC = assetRow("asset:s", "payments-db", "asset:h");
const TYPES: AssetType[] = [
  { id: "vm", label: "VM", monogram: "Vm", properties: [], suggests: [] },
  { id: "service", label: "Service", monogram: "Sv", properties: [], suggests: [] },
  { id: "container", label: "Container", monogram: "Ct", properties: [], suggests: [] },
];
const dialog = () => target.querySelector<HTMLElement>(".tf-dialog");
const dialogButton = (label: string) => [...(dialog()?.querySelectorAll<HTMLButtonElement>("button") ?? [])].find((b) => text(b) === label);

/** Ports whose link lands: the next roster read has `kuma:4` watching what was linked. */
function linking(estate: Record<string, AssetRow[]> = { "": [HOST], "asset:h": [SVC] }) {
  const links: [string, string][] = [];
  const created: [string, string, string | null | undefined][] = [];
  const stand = ports({
    monitorRoster: async () =>
      ROSTER.map((row) => (row.entity_id === "kuma:4" && links.length > 0 ? { ...row, assets: links.map(([id]) => ({ id, name: id === SVC.id ? SVC.name : "pg-payments", path: null, type_label: "Service" })) } : row)),
    assetTree: async (parent) => estate[parent ?? ""] ?? [],
    assetTypes: async () => TYPES,
    createAsset: async (typeId, name, parentId) => {
      created.push([typeId, name, parentId]);
      return assetRow("asset:new", name, null, { type_id: typeId });
    },
    linkAsset: async (assetId, monitorId) => {
      links.push([assetId, monitorId]);
    },
  });
  return { stand, links, created };
}

test("the open monitor's Watches offers Link an asset, which walks the estate and links where it stands", async () => {
  const { stand, links } = linking();
  await draw(["at", "kuma:4"], null, stand);
  const watches = panel().querySelector(".tf-mon-pwatch")!;
  expect(text(watches.querySelector("span"))).toBe("Nothing yet.");
  button(watches, "Link an asset…")!.click();
  await settle();
  expect(text(dialog()!.querySelector("h2, .tf-dtitle, header"))).toContain("Link pg-payments to an asset");
  // At the top of the estate there is nothing to link to yet: the primary says what it waits for.
  expect(dialogButton("Link")?.disabled).toBe(true);
  expect(text(dialog()!.querySelector(".tf-dwhy"))).toBe("Open the asset it watches.");
  dialogButton("Vm hel1 ›")!.click();
  await settle();
  // Standing on a host links the host; walking on into it stands on what it holds.
  expect(dialogButton("Link to hel1")?.disabled).toBe(false);
  dialogButton("Sv payments-db")!.click();
  await settle();
  dialogButton("Link to payments-db")!.click();
  await settle();
  expect(links).toEqual([["asset:s", "kuma:4"]]);
  expect(dialog()).toBeNull();
  // The roster is read again, and the panel says what the monitor watches now.
  expect(text(panel().querySelector(".tf-mon-pwatch a"))).toBe("payments-db");
  expect(toasts.items.map((toast) => toast.text)).toContain("pg-payments watches payments-db");
});

test("an asset the monitor already watches is marked in the picker, and linking it again is not offered", async () => {
  const watched = assetRow("asset:r", "payouts", null);
  await draw(["at", "kuma:1"], null, linking({ "": [watched] }).stand);
  button(panel(), "Link an asset…")!.click();
  await settle();
  const option = [...dialog()!.querySelectorAll<HTMLButtonElement>(".opt")].find((b) => text(b).includes("payouts"))!;
  expect(text(option)).toContain("watched by it");
  option.click();
  await settle();
  expect(dialogButton("Already linked")?.disabled).toBe(true);
  expect(text(dialog()!.querySelector(".tf-dwhy"))).toBe("payouts /health already watches payouts.");
});

test("New asset for it makes an asset at the top of the estate, named and typed for the monitor, and links it", async () => {
  const { stand, links, created } = linking({ "": [] });
  await draw(["at", "kuma:4"], null, stand);
  button(panel(), "New asset for it…")!.click();
  await settle();
  const name = dialog()!.querySelector<HTMLInputElement>("input[type=text]")!;
  expect(name.value).toBe("pg-payments");
  // A TCP check watches a service as far as its kind says.
  expect(dialog()!.querySelector<HTMLSelectElement>("select")!.value).toBe("service");
  expect(text(dialog()!.querySelector(".tf-dsub"))).toBe("At the top of the estate, watched by pg-payments");
  dialogButton("Create")!.click();
  await settle();
  expect(created).toEqual([["service", "pg-payments", null]]);
  expect(links).toEqual([["asset:new", "kuma:4"]]);
  const toast = toasts.items.find((item) => item.text === "Added pg-payments to Assets, watched by pg-payments")!;
  expect(toast.action?.label).toBe("Open");
  toast.action!.run();
  expect(location.hash).toBe("#/asset/asset%3Anew");
});

test("the picker of an empty estate says so and leads to New asset", async () => {
  const { stand } = linking({ "": [] });
  await draw(["at", "kuma:4"], null, stand);
  button(panel(), "Link an asset…")!.click();
  await settle();
  expect(text(dialog()!.querySelector(".empty"))).toBe("No assets yet. Make the one it watches with New asset.");
  dialogButton("New asset…")!.click();
  await settle();
  expect(dialog()!.querySelector<HTMLInputElement>("input[type=text]")!.value).toBe("pg-payments");
});

test("with no asset watched anywhere, the roster's Watches header names no column", async () => {
  await draw([], null, ports({ monitorRoster: async () => [monitor("kuma:1", "a"), monitor("kuma:2", "b")] }));
  expect(text(target.querySelector(".tf-mon-head .tf-mon-watch"))).toBe("");
  unmount(view!);
  view = null;
  await draw([], null);
  expect(text(target.querySelector(".tf-mon-head .tf-mon-watch"))).toBe("Watches");
});

test("Assets nothing watches with no asset at all says the estate is empty, not that every asset has a monitor", async () => {
  let estate: AssetRow[] = [];
  await draw(["unmonitored"], null, ports({ unmonitoredAssets: async () => [], assetTree: async () => estate }));
  const empty = target.querySelector(".tf-empty")!;
  expect(text(empty)).toBe("No assets yet, so nothing is left unwatched. Add what you run under Assets, or open a monitor and press New asset for it.");
  expect(empty.querySelector('a[href="#/assets"]')).not.toBeNull();
  unmount(view!);
  view = null;
  estate = [HOST];
  await draw(["unmonitored"], null, ports({ unmonitoredAssets: async () => [], assetTree: async () => estate }));
  expect(text(target.querySelector(".tf-empty"))).toBe("Every container, service, route, database and VM has a monitor.");
});

test("the header says what the monitors watch as it is: each, some, or nothing", async () => {
  const sub = () => text(target.querySelector(".tf-sub"));
  await draw([], null, ports({ monitorRoster: async () => [monitor("kuma:1", "a"), monitor("kuma:2", "b")] }));
  expect(sub()).toBe("2 checks from Uptime Kuma, synced 1 min ago.");
  unmount(view!);
  view = null;
  const watched = { assets: [{ id: "asset:r", name: "payouts", path: null, type_label: "Service" }] };
  await draw([], null, ports({ monitorRoster: async () => [monitor("kuma:1", "a", watched), monitor("kuma:2", "b", watched)] }));
  expect(sub()).toBe("2 checks from Uptime Kuma, synced 1 min ago. Each one watches an asset.");
});

test("with a monitor open, what each row watches gives way to the panel and keeps its names in its title", async () => {
  await draw([]);
  const main = target.querySelector(".tf-mon-main")!;
  expect(main.classList.contains("tf-mon-paneled")).toBe(false);
  expect(row("payouts /health").querySelector(".tf-mon-watch")?.getAttribute("title")).toBe("payouts");
  expect(row("pg-payments").querySelector(".tf-mon-watch")?.hasAttribute("title")).toBe(false);
  row("payouts /health").querySelector<HTMLButtonElement>(".tf-mon-open")!.click();
  flushSync();
  expect(main.classList.contains("tf-mon-paneled")).toBe(true);
});
