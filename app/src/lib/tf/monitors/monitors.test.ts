/**
 * Monitors' pure half (#864, R-11.3, R-11.4): the order, the chip, the bar's
 * segments, the columns' numbers, which rows the filter keeps, which assets
 * the second tab lists, and what *Create monitor* opens with.
 */
import { describe, expect, test } from "vitest";

import type { AssetDetail, AssetProperty, MonitorRow, RouteRow, UnmonitoredAsset } from "../../ipc/assets";
import { jobIndex } from "../job/open-jobs";
import { stateWord } from "../state-word";
import {
  stateChip,
  gaps,
  hiddenFoldOpen,
  kindWord,
  prefill,
  routePrefill,
  responseText,
  segments,
  sorted,
  uptimeText,
  STATE_CHIPS,
  visibleMonitors,
  type KeptMonitors,
} from "./monitors";

const NOW = Date.parse("2026-10-16T14:00:00Z");

function row(over: Partial<MonitorRow>): MonitorRow {
  return {
    entity_id: "kuma:1",
    source_id: "kuma",
    name: "payouts /health",
    state: "up",
    monitor_type: "http",
    target: "https://payouts.example/health",
    response_time_ms: 140,
    checked_at: "2026-10-16T13:59:00Z",
    uptime: [
      { window: "1d", ratio: 1 },
      { window: "30d", ratio: 0.9998 },
    ],
    cert_days_remaining: null,
    web_url: null,
    tombstoned: false,
    hidden: false,
    assets: [{ id: "asset:r1", name: "payouts", path: "edge", type_label: "Service" }],
    actions: ["pause_monitor"],
    refreshes: true,
    samples: [],
    ...over,
  };
}

const sample = (minutesAgo: number, state: string | null) => ({
  taken_at: new Date(NOW - minutesAgo * 60_000).toISOString(),
  state,
});

describe("the order", () => {
  test("is down, warning, paused, up, by name inside each", () => {
    const rows = [
      row({ name: "b-up" }),
      row({ name: "paused", tombstoned: true }),
      row({ name: "a-up" }),
      row({ name: "warn", state: "warn" }),
      row({ name: "down", state: "down" }),
    ];
    expect(sorted(rows).map((r) => r.name)).toEqual(["down", "warn", "paused", "a-up", "b-up"]);
  });
});

describe("the state chip", () => {
  test("says since when a monitor is down, from its samples", () => {
    const down = row({
      state: "down",
      samples: [sample(90, "up"), sample(60, "down"), sample(30, "down"), sample(1, "down")],
    });
    const chip = stateChip(down, new Date(NOW));
    expect(chip.tone).toBe("err");
    expect(chip.label).toBe("down");
    expect(chip.since).toMatch(/^since \d\d:\d\d$/);
    const at = new Date(NOW - 60 * 60_000);
    const pad = (n: number) => String(n).padStart(2, "0");
    expect(chip.since).toBe(`since ${pad(at.getHours())}:${pad(at.getMinutes())}`);
  });

  test("keeps the day on a monitor down since before today (#1986)", () => {
    const at = (day: number) => ({ taken_at: new Date(2026, 9, day, 12, 40).toISOString(), state: "down" });
    const now = new Date(2026, 9, 16, 9, 0);
    expect(stateChip(row({ state: "down", samples: [at(15)] }), now).since).toBe("since yesterday 12:40");
    expect(stateChip(row({ state: "down", samples: [at(12)] }), now).since).toBe("since Mon 12:40");
  });

  test("says down alone when no sample says since when", () => {
    expect(stateChip(row({ state: "down" }))).toEqual({ label: "down", tone: "err" });
  });

  test("says slow on a warning with a response, whose time is the row's Response", () => {
    expect(stateChip(row({ state: "warn", response_time_ms: 8400 }))).toEqual({ label: "slow", tone: "warn" });
  });

  test("says paused for a tombstone whatever its last reading", () => {
    expect(stateChip(row({ state: "up", tombstoned: true }))).toEqual({ label: "paused", tone: null });
  });

  test("says up, and names a state it has no chip for", () => {
    expect(stateChip(row({}))).toEqual({ label: "up", tone: "ok" });
    expect(stateChip(row({ state: "pending" }))).toEqual({ label: "pending", tone: "info" });
    expect(stateChip(row({ state: null }))).toEqual({ label: "no reading", tone: null });
  });
});

describe("the columns", () => {
  test("a kind of check reads in prototype-6's words, an unknown one as the source spells it", () => {
    expect(kindWord("http")).toBe("HTTP");
    expect(kindWord("port")).toBe("TCP");
    expect(kindWord("ping")).toBe("Ping");
    expect(kindWord("docker")).toBe("Docker");
    expect(kindWord("keyword")).toBe("keyword");
    expect(kindWord(null)).toBe("");
  });

  test("the response is in milliseconds, grouped, and a dash when nothing answered", () => {
    expect(responseText(140)).toBe("140 ms");
    expect(responseText(8400)).toBe("8\u00a0400 ms");
    expect(responseText(0.4)).toBe("0.4 ms");
    expect(responseText(null)).toBe("—");
  });

  test("the uptime is the 30-day ratio as a percentage", () => {
    expect(uptimeText(row({}))).toBe("99.98 %");
    expect(uptimeText(row({ uptime: [{ window: "30d", ratio: 1 }] }))).toBe("100 %");
    expect(uptimeText(row({ uptime: [{ window: "30d", ratio: 0.971 }] }))).toBe("97.1 %");
    expect(uptimeText(row({ uptime: [{ window: "1d", ratio: 1 }] }))).toBe("—");
  });
});

describe("the bar", () => {
  test("is 48 half-hours, the worst reading in each, a gap where there was none", () => {
    const bar = segments(row({ samples: [sample(10, "down"), sample(20, "up"), sample(45, "warn")] }), NOW);
    expect(bar).toHaveLength(48);
    expect(bar[47]).toBe("down");
    expect(bar[46]).toBe("warn");
    expect(bar[0]).toBe("gap");
  });

  test("is all paused for a paused monitor", () => {
    expect(new Set(segments(row({ tombstoned: true, samples: [sample(10, "up")] }), NOW))).toEqual(new Set(["paused"]));
  });
});

describe("the filter", () => {
  const rows = [
    row({ entity_id: "kuma:1", name: "flows-stage", monitor_type: "http", state: "down", assets: [{ id: "asset:a", name: "flows", path: null, type_label: "Route" }] }),
    row({ entity_id: "kuma:2", name: "pg", monitor_type: "port", assets: [{ id: "asset:db", name: "payments", path: null, type_label: "Database" }] }),
    row({ entity_id: "kuma:3", name: "vm ping", monitor_type: "ping", tombstoned: true, assets: [] }),
    row({ entity_id: "kuma:4", name: "noisy", monitor_type: "http", state: "down", hidden: true, assets: [] }),
  ];
  const index = jobIndex([
    { id: "jira:PAY-231", reached: [{ entity_id: "kuma:1" }] },
    { id: "jira:PAY-200", reached: [{ entity_id: "asset:db" }] },
  ]);
  const base = { scope: "all" as const, job: null, states: [] as string[], type: "", text: "" };
  const names = (kept: KeptMonitors) => kept.shown.map((r) => r.name);
  const hiddenNames = (kept: KeptMonitors) => kept.hidden.map((r) => r.name);

  test("This job keeps what the job reaches, by the monitor or the asset it watches", () => {
    expect(names(visibleMonitors(rows, { ...base, scope: "job", job: "PAY-231" }, index))).toEqual(["flows-stage"]);
    expect(names(visibleMonitors(rows, { ...base, scope: "job", job: "PAY-200" }, index))).toEqual(["pg"]);
  });

  test("My jobs keeps what any open job reaches", () => {
    expect(names(visibleMonitors(rows, { ...base, scope: "mine" }, index))).toEqual(["flows-stage", "pg"]);
  });

  test("state chips and the type keep their own", () => {
    expect(names(visibleMonitors(rows, { ...base, states: ["Down", "Paused"] }, index))).toEqual(["flows-stage", "vm ping"]);
    expect(names(visibleMonitors(rows, { ...base, type: "TCP" }, index))).toEqual(["pg"]);
  });

  test("a hidden monitor is kept apart from the roster, and the other filters still apply to it", () => {
    expect(names(visibleMonitors(rows, base, index))).toEqual(["flows-stage", "pg", "vm ping"]);
    expect(hiddenNames(visibleMonitors(rows, base, index))).toEqual(["noisy"]);
    expect(hiddenNames(visibleMonitors(rows, { ...base, states: ["Down"] }, index))).toEqual(["noisy"]);
    expect(hiddenNames(visibleMonitors(rows, { ...base, states: ["Up"] }, index))).toEqual([]);
    expect(hiddenNames(visibleMonitors(rows, { ...base, scope: "mine" }, index))).toEqual([]);
  });

  test("the hidden fold opens by itself only when it holds every match, and otherwise stays as the reader left it", () => {
    const some = visibleMonitors(rows, base, index);
    const onlyHidden = visibleMonitors(rows, { ...base, text: "noisy" }, index);
    const none = visibleMonitors(rows, { ...base, text: "nothing" }, index);
    expect(hiddenFoldOpen(null, some)).toBe(false);
    expect(hiddenFoldOpen(null, onlyHidden)).toBe(true);
    expect(hiddenFoldOpen(null, none)).toBe(false);
    expect(hiddenFoldOpen(true, some)).toBe(true);
    expect(hiddenFoldOpen(false, onlyHidden)).toBe(false);
  });

  test("the words match the monitor, the asset and the type", () => {
    expect(names(visibleMonitors(rows, { ...base, text: "payments" }, index))).toEqual(["pg"]);
    expect(names(visibleMonitors(rows, { ...base, text: "database" }, index))).toEqual(["pg"]);
    expect(names(visibleMonitors(rows, { ...base, text: "ping" }, index))).toEqual(["vm ping"]);
  });
});

describe("Assets nothing watches", () => {
  const gap = (type_id: string, name: string): UnmonitoredAsset => ({
    id: `asset:${name}`,
    type_id,
    type_label: type_id,
    monogram: "XX",
    name,
    path: null,
    environment: null,
    owner: null,
    url: null,
    monitor_on_its_way: null,
  });

  test("lists containers, services, routes, databases and VMs, nothing else", () => {
    const rows = ["site", "hypervisor", "vm", "container_engine", "container", "service", "route", "database_server", "database", "network"].map(
      (type) => gap(type, type),
    );
    expect(gaps(rows).map((r) => r.type_id)).toEqual(["vm", "container", "service", "route", "database_server", "database"]);
  });

  test("a route opens as HTTP on its URL plus /health, trailing slash or not; no http(s) URL leaves the target empty", () => {
    expect(routePrefill({ name: "Gitea", url: "https://gitea.test/" })).toEqual({
      name: "Gitea /health",
      kind: "http",
      target: "https://gitea.test/health",
    });
    expect(routePrefill({ name: "Api", url: "http://api.test:8080/v1" }).target).toBe("http://api.test:8080/v1/health");
    expect(routePrefill({ name: "Tunnel", url: "ssh://host" }).target).toBe("");
    expect(routePrefill({ name: "None", url: null }).target).toBe("");
  });
});

describe("Create monitor opens with", () => {
  const text = (key: string, value: string): AssetProperty => ({ key, label: key, value: { kind: "text", value }, custom: false });
  const num = (key: string, value: number): AssetProperty => ({ key, label: key, value: { kind: "number", value }, custom: false });
  const route = (url: string): RouteRow => ({ url } as RouteRow);
  function detail(type_id: string, name: string, properties: AssetProperty[] = [], exposes: RouteRow[] = []): AssetDetail {
    return {
      asset: { id: `asset:${name}`, type_id, name } as AssetDetail["asset"],
      properties,
      exposes,
      reachable_via: [],
    } as unknown as AssetDetail;
  }

  test("HTTP on a service's URL plus /health", () => {
    expect(prefill(detail("service", "payouts", [], [route("https://payouts.example/")]))).toEqual({
      name: "payouts /health",
      kind: "http",
      target: "https://payouts.example/health",
    });
  });

  test("HTTP on the service's own URL and health path when it has them", () => {
    expect(
      prefill(detail("service", "ledger", [{ key: "url", label: "URL", value: { kind: "url", value: "http://ledger.test:8080" }, custom: false }, text("health_path", "/ready")])),
    ).toEqual({ name: "ledger /health", kind: "http", target: "http://ledger.test:8080/ready" });
  });

  test("TCP on a database's host and port, or the address a property spells", () => {
    expect(prefill(detail("database_server", "pg", [text("host", "db.internal"), num("port", 5432)]))).toEqual({
      name: "pg",
      kind: "tcp",
      target: "db.internal:5432",
    });
    expect(prefill(detail("database", "jira", [text("jdbc_url", "jdbc:postgresql://jira-db:5432/jira")])).target).toBe("jira-db:5432");
    expect(prefill(detail("database", "lonely")).target).toBe("");
  });

  test("Docker on a container, by its container name or its own", () => {
    expect(prefill(detail("container", "knobas-jira", [text("container_name", "jira-1")]))).toEqual({
      name: "knobas-jira",
      kind: "docker",
      target: "jira-1",
    });
    expect(prefill(detail("container", "worker")).target).toBe("worker");
  });

  test("Ping on a VM's hostname, else its IP", () => {
    expect(prefill(detail("vm", "vm-pay-01", [text("ip", "10.0.0.4")]))).toEqual({ name: "vm-pay-01", kind: "ping", target: "10.0.0.4" });
    expect(prefill(detail("vm", "vm-pay-01", [text("ip", "10.0.0.4"), text("hostname", "vm-pay-01.lan")])).target).toBe("vm-pay-01.lan");
  });
});

describe("the state pills read what the chips read", () => {
  const NO_JOBS = jobIndex([]);
  const states = ["down", "warn", "up", "pending", "maintenance"] as const;
  test.each(states)("a %s row is kept by the pill its chip names, and by no other", (state) => {
    const it = row({ state, response_time_ms: state === "warn" ? 8400 : 40 });
    const word = stateWord(stateChip(it).label);
    expect(STATE_CHIPS as readonly string[]).toContain(word);
    for (const pill of STATE_CHIPS) {
      const kept = visibleMonitors([it], { scope: "all", job: null, states: [pill], type: "", text: "" }, NO_JOBS).shown;
      expect(kept.length).toBe(pill === word ? 1 : 0);
    }
  });

  test("a paused monitor is kept by Paused whatever it last read, and slow needs no response to say so", () => {
    const paused = row({ state: "up", tombstoned: true });
    expect(visibleMonitors([paused], { scope: "all", job: null, states: ["Paused"], type: "", text: "" }, NO_JOBS).shown).toEqual([paused]);
    expect(stateChip(row({ state: "warn", response_time_ms: null })).label).toBe("slow");
  });
});
