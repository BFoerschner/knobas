/**
 * Monitors' pure half (#864, R-11.3, R-11.4): the order, the chip, the bar's
 * segments, the columns' numbers, which rows the filter keeps, which assets
 * *Assets nothing watches* lists, and what *Create monitor* opens with.
 *
 * Over `monitor_roster` and `unmonitored_assets`, both held whole, so the
 * filter runs in the browser (R-2.3's first mode), as the Inbox's does. The
 * bar's buckets and which chip a row is in are the rooms tab's arithmetic
 * (`assets/monitors.ts`), reused rather than copied.
 */
import type { AssetDetail, AssetProperty, MonitorRow, MonitorTarget, UnmonitoredAsset } from "../../ipc/assets";
import type { MonitorKind } from "../../ipc/sources";
import { BAR_BUCKETS, chipOf as chipStateOf } from "../../assets/monitors";
import { sliceMarks } from "../filter/filter";
import { stampLabel } from "../../time/format";
import { stateWord } from "../state-word";
import type { JobIndex } from "../job/open-jobs";

/** A chip's tone; `null` is the muted chip (paused, no reading). */
export type ChipTone = "ok" | "warn" | "err" | "info" | null;

/** Where each chip sorts: down, slow, then what is neither up nor broken, paused, up. */
const RANK: Record<string, number> = { down: 0, warn: 1, pending: 2, other: 3, paused: 4, up: 5 };

/** The roster in prototype-6's order: down, slow, paused, up; by name inside each. */
export function sorted(rows: readonly MonitorRow[]): MonitorRow[] {
  return [...rows].sort(
    (a, b) => (RANK[chipStateOf(a)] ?? 3) - (RANK[chipStateOf(b)] ?? 3) || a.name.localeCompare(b.name),
  );
}

/** Milliseconds as prototype-6 writes them: `140 ms`, `8 400 ms`, `0.4 ms`. */
export function responseText(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms)) return "—";
  const rounded = ms < 10 ? Math.round(ms * 10) / 10 : Math.round(ms);
  const [whole = "0", fraction] = String(rounded).split(".");
  const grouped = whole.replace(/\B(?=(\d{3})+(?!\d))/g, " ");
  return `${grouped}${fraction ? `.${fraction}` : ""} ms`;
}

/** The 30-day uptime as a percentage (`99.98 %`), or a dash when Kuma gave none. */
export function uptimeText(row: Pick<MonitorRow, "uptime">): string {
  const ratio = row.uptime.find((u) => u.window === "30d")?.ratio;
  if (ratio === undefined) return "—";
  return `${Number((ratio * 100).toFixed(2))} %`;
}

/** When the monitor went down, as the backend read it (`down_since`, the newest run of `down` samples' first). */
function downSince(row: MonitorRow): Date | null {
  const date = row.down_since === null || row.down_since === undefined ? null : new Date(row.down_since);
  return date !== null && !Number.isNaN(date.getTime()) ? date : null;
}

/**
 * The state chip: the word and its tone. A chip holds the state alone, so it
 * never gives up its end: how long a monitor has been down is `since`, plain
 * text beside it (*since 12:40* today, else *since yesterday 12:40* or *since
 * 2026-10-12 12:40*, `stampLabel`, #1986), and a slow monitor's time is the
 * row's Response.
 */
export function stateChip(row: MonitorRow, now: Date = new Date()): { label: string; tone: ChipTone; since?: string } {
  switch (chipStateOf(row)) {
    case "paused":
      return { label: "paused", tone: null };
    case "down": {
      const since = downSince(row);
      return since ? { label: "down", tone: "err", since: `since ${stampLabel(since.toISOString(), now, true)}` } : { label: "down", tone: "err" };
    }
    case "warn":
      return { label: "slow", tone: "warn" };
    case "up":
      return { label: "up", tone: "ok" };
    case "pending":
      return { label: "pending", tone: "info" };
    default:
      return { label: row.state ?? "no reading", tone: null };
  }
}

/** What a segment of the bar is drawn as. */
export type Segment = "up" | "warn" | "down" | "paused" | "gap";

/** Each letter of the backend's bar (`MonitorRow::bar`, `0099`) as what its half-hour draws; any other is a gap. */
const LETTER: Record<string, Segment> = { d: "down", w: "warn", p: "paused", m: "paused", u: "up" };

/**
 * The 24-hour bar: 48 half-hours, oldest first, the newest the half-hour now is in; a paused monitor's is paused
 * throughout. The backend keeps each half-hour's worst state as its samples arrive (`0099`), so the bar is 48 letters
 * and not a day of samples (the monitors-page pass, 2026-11-19: 1.4 MB a Kuma poll).
 */
export function segments(row: Pick<MonitorRow, "bar" | "tombstoned">): Segment[] {
  const letters = [...row.bar].slice(-BAR_BUCKETS);
  const bar = [...Array<string>(BAR_BUCKETS - letters.length).fill("-"), ...letters];
  return bar.map((letter) => (row.tombstoned ? "paused" : (LETTER[letter] ?? "gap")));
}

const WORST: readonly Segment[] = ["down", "warn", "paused", "up", "gap"];

/** A bar to draw: its state, its key, and what its title says. */
export interface Bar {
  state: Segment;
  at: number;
  title: string;
}

/** What each state is called in a bar's title; a pale gap is a half-hour with no reading. */
const SEGMENT_WORD: Record<Segment, string> = { up: "up", warn: "slow", down: "down", paused: "paused", gap: "no data" };

/** The bars the roster draws: the 48 half-hours, or below the narrow width one bar an hour, each worst-of. */
export function barsOf(all: readonly Segment[], hourly: boolean): Bar[] {
  const step = hourly ? 2 : 1;
  const bars: Bar[] = [];
  for (let at = 0; at < all.length; at += step) {
    const part = all.slice(at, at + step);
    const state = WORST.find((word) => part.includes(word)) ?? "gap";
    const hoursAgo = (all.length - at) / 2;
    bars.push({ state, at, title: `${hoursAgo} h ago: ${SEGMENT_WORD[state]}` });
  }
  return bars;
}

/**
 * What each state's bar is painted with (the monitors-page pass, 2026-11-19): what is wrong in its tone at full
 * strength, an hour that was up in a pale `--ok` (`--tf-mon-up`, mixed once in `monitors.css`), so a roster of healthy
 * monitors reads as quiet and a red or amber bar is the first thing seen; paused and no reading in the rules' greys.
 */
const BAR_PAINT: Record<Segment, string> = {
  up: "var(--tf-mon-up)",
  down: "var(--err)",
  warn: "var(--warn)",
  paused: "var(--rule)",
  gap: "var(--rule-2)",
};
/** A hidden monitor's bars: its states in grey, as the rest of its row is drawn, a down hour a shade darker. */
const QUIET_PAINT: Record<Segment, string> = { up: "var(--rule)", down: "var(--muted)", warn: "var(--muted)", paused: "var(--rule)", gap: "var(--rule-2)" };

/**
 * The 24-hour strip as one element's background (PR #2479): a bar `width` px wide every `width + 1` px, in its
 * state's colour. Two layers: on top, the 1 px between bars in the colour the strip sits on (`--tf-mon-gap`); under it,
 * one stretch of colour for each run of bars of one state. It was a pair of stops for each of the 48 bars, a
 * `color-mix` in each up one, and a roster drawn whole parsed and resolved 218 such gradients as it drew (the
 * monitors-page pass, 2026-11-19). The open monitor's (`big`) is 5 px a bar, its ends rounded by a mask.
 */
export function stripPaint(bars: readonly Bar[], hourly: boolean, quiet = false, big = false): { image: string; width: number; pitch: number } {
  const width = big ? 5 : hourly ? 3 : 2;
  const pitch = width + 1;
  const paint = quiet ? QUIET_PAINT : BAR_PAINT;
  const total = Math.max(0, bars.length * pitch - 1);
  const runs: string[] = [];
  for (let at = 0; at < bars.length; ) {
    let end = at + 1;
    while (end < bars.length && bars[end]!.state === bars[at]!.state) end += 1;
    runs.push(`${paint[bars[at]!.state]} ${at * pitch}px ${Math.min(total, end * pitch)}px`);
    at = end;
  }
  const gaps = `repeating-linear-gradient(to right, transparent 0 ${width}px, var(--tf-mon-gap) ${width}px ${pitch}px)`;
  return { image: bars.length === 0 ? "none" : `${gaps}, linear-gradient(to right, ${runs.join(", ")})`, width: total, pitch };
}

/**
 * What a roster row draws, as one string: a row whose drawn parts are the same as before is the same row. Its
 * response time is drawn only while it is slow (a slow monitor's time is the reason), and when it was read not at
 * all, so a poll that moves every monitor's response by a millisecond and changes nothing else draws nothing.
 */
export function drawnKey(row: MonitorRow): string {
  return JSON.stringify([
    row.name, row.source_id, row.state, row.monitor_type, row.bar, row.down_since ?? null, row.tombstoned, row.hidden,
    uptimeText(row), row.actions, row.refreshes, row.assets.map((asset) => [asset.id, asset.name, asset.type_label]),
    row.state === "warn" ? responseText(row.response_time_ms) : null,
  ]);
}

/**
 * The roster read again, with every row whose drawn parts did not change kept as the object it was (the
 * monitors-page pass, 2026-11-19): the list is keyed, so a kept object draws nothing, and an Uptime Kuma poll that
 * changes one monitor redraws one row and not 218.
 */
export function keepDrawn(previous: readonly MonitorRow[] | null, next: MonitorRow[]): MonitorRow[] {
  if (previous === null || previous.length === 0) return next;
  const before = new Map(previous.map((row) => [row.entity_id, row]));
  let same = previous.length === next.length;
  const kept = next.map((row, at) => {
    const was = before.get(row.entity_id);
    const keep = was !== undefined && drawnKey(was) === drawnKey(row) ? was : row;
    if (keep !== previous[at]) same = false;
    return keep;
  });
  // Nothing drawn changed: the very list, so nothing that reads the roster runs again.
  return same ? (previous as MonitorRow[]) : kept;
}

/**
 * The kind of check in prototype-6's words. The roster carries Uptime Kuma's
 * own `monitor_type`; a word with no entry here is drawn as the source spells
 * it rather than guessed at.
 */
const KIND_WORDS: Record<string, string> = { http: "HTTP", port: "TCP", ping: "Ping", docker: "Docker" };

export function kindWord(type: string | null): string {
  if (type === null) return "";
  return KIND_WORDS[type] ?? type;
}

/** The asset type a monitor's kind of check usually watches: a Docker check a container, a database check a database. */
const WATCHED_TYPE: Record<string, string> = {
  docker: "container",
  postgres: "database",
  mysql: "database",
  mongodb: "database",
  redis: "database",
  sqlserver: "database",
  ping: "vm",
};

/**
 * What *New asset for it* on an open monitor starts from (2026-11-19, Assets back in the nav): the monitor's own name and
 * the type its kind of check usually watches, a service where the kind says nothing more (an HTTP or TCP check). Both
 * stay the reader's to change.
 */
export function assetFor(monitor: Pick<MonitorRow, "name" | "monitor_type">): { name: string; typeId: string } {
  return { name: monitor.name, typeId: (monitor.monitor_type !== null && WATCHED_TYPE[monitor.monitor_type]) || "service" };
}

/**
 * The state pills of the filter bar: the words a row's chip says, so a pill keeps the rows that read as it does. *No
 * reading* has no pill, because it is not a state to look for: such a row shows while no pill is on.
 */
export const STATE_CHIPS = ["Down", "Slow", "Up", "Paused", "Pending", "Maintenance"] as const;

/** What the filter bar has set. */
export interface MonitorFilter {
  scope: "job" | "mine" | "all";
  /** The job's ticket key; *This job* keeps nothing without one. */
  job: string | null;
  /** The state chips on; none keeps every state. */
  states: readonly string[];
  /** The type chosen, in {@link kindWord}'s words; `""` is *Any*. The typed words rank {@link monitorText} through the app. */
  type: string;
}

/**
 * What the filter keeps, split in two: the roster proper, and the monitors the
 * reader hid, which the page draws after it, under their own line, dimmed.
 * Every filter applies to both, so a hidden monitor is found by the same words
 * as any other.
 */
export interface KeptMonitors {
  shown: MonitorRow[];
  hidden: MonitorRow[];
}

/** The ids a row is reached by: the monitor's own, and every asset it watches. */
const idsOf = (row: MonitorRow) => [row.entity_id, ...row.assets.map((asset) => asset.id)];

/** The rows the filter keeps, in the order given, the hidden ones apart. */
export function visibleMonitors(rows: readonly MonitorRow[], filter: MonitorFilter, index: JobIndex): KeptMonitors {
  const kept = rows.filter((row) => {
    if (filter.scope === "job") {
      const job = filter.job;
      if (job === null || !idsOf(row).some((id) => index.holds(job, { entity_id: id }))) return false;
    }
    if (filter.scope === "mine" && !idsOf(row).some((id) => index.jobOf({ entity_id: id, kind: null }) !== null)) {
      return false;
    }
    if (filter.states.length > 0 && !filter.states.includes(stateWord(stateChip(row).label))) return false;
    return filter.type === "" || kindWord(row.monitor_type) === filter.type;
  });
  return { shown: kept.filter((row) => !row.hidden), hidden: kept.filter((row) => row.hidden) };
}

/** What the filter ranks a monitor by (#2535): its name, its type, and the name and type of each asset it watches. */
export function monitorText(row: MonitorRow): string {
  return [row.name, kindWord(row.monitor_type), ...row.assets.flatMap((a) => [a.name, a.type_label])].join(" ");
}

/** The ranking's `pairs` over {@link monitorText} cut onto the cells that draw them: the name, and each asset's name. */
export function monitorMarks(row: MonitorRow, pairs: readonly number[]): { name: readonly number[]; assets: (readonly number[])[] } {
  let at = row.name.length + 1 + kindWord(row.monitor_type).length + 1;
  const assets = row.assets.map((asset) => {
    const cut = sliceMarks(pairs, at, asset.name.length);
    at += asset.name.length + 1 + asset.type_label.length + 1;
    return cut;
  });
  return { name: sliceMarks(pairs, 0, row.name.length), assets };
}

/** What the filter of *Assets nothing watches* ranks an asset by (#2535): its name, type, owner and environment. */
export function unwatchedText(asset: UnmonitoredAsset): string {
  return [asset.name, asset.type_label, asset.owner ?? "", asset.environment ?? ""].join(" ");
}

/** The ranking's `pairs` over {@link unwatchedText} cut onto the asset's cells. */
export function unwatchedMarks(asset: UnmonitoredAsset, pairs: readonly number[]): Record<"name" | "type" | "owner" | "environment", readonly number[]> {
  const type = asset.name.length + 1;
  const owner = type + asset.type_label.length + 1;
  const environment = owner + (asset.owner ?? "").length + 1;
  return {
    name: sliceMarks(pairs, 0, asset.name.length),
    type: sliceMarks(pairs, type, asset.type_label.length),
    owner: sliceMarks(pairs, owner, (asset.owner ?? "").length),
    environment: sliceMarks(pairs, environment, (asset.environment ?? "").length),
  };
}

/**
 * The asset types *Assets nothing watches* lists (R-11.3): containers,
 * services, databases and VMs. A database server is a database here; it gets
 * the same TCP check.
 */
const WATCHABLE = ["vm", "container", "service", "database_server", "database", "route"];

/** The unwatched assets of a type worth a check, in the read's order. */
export function gaps(rows: readonly UnmonitoredAsset[]): UnmonitoredAsset[] {
  return rows.filter((row) => WATCHABLE.includes(row.type_id));
}

/** What *Create monitor* opens with. Every field is the reader's to change. */
export interface Prefill {
  name: string;
  kind: MonitorKind;
  target: string;
}

/**
 * What *Create monitor* is for and how it opens (#976): an asset or a route.
 * The dialog writes the name through `record` (the asset's or the route's own
 * `monitors` edit) and then queues the create in one of `targets`.
 */
export interface MonitorSubject {
  id: string;
  name: string;
  targets: MonitorTarget[];
  opening: Prefill;
}

/** The unwatched row is a route (#976): a `route:<uuid>` id, type `route`. */
export function isRoute(row: Pick<UnmonitoredAsset, "type_id">): boolean {
  return row.type_id === "route";
}

/** An entity id that names a route (`route:<uuid>`) rather than an asset. */
export function isRouteId(id: string): boolean {
  return id.startsWith("route:");
}

/** A route's opening (#976): HTTP on the route's URL plus `/health`, named as an asset's HTTP check is. */
export function routePrefill(row: Pick<UnmonitoredAsset, "name" | "url">): Prefill {
  const base = httpUrl([row.url]);
  return {
    name: `${row.name} /health`,
    kind: "http",
    target: base === null ? "" : `${base.replace(/\/+$/, "")}/health`,
  };
}

/** A property's value as text, or `null` for none. */
function valueOf(properties: readonly AssetProperty[], key: string): string | null {
  const value = properties.find((property) => property.key === key)?.value;
  if (!value) return null;
  const text = String(value.value).trim();
  return text === "" ? null : text;
}

/** The first `host:port` a property spells inside an address (`jdbc:postgresql://jira-db:5432/jira`). */
function addressIn(properties: readonly AssetProperty[]): string | null {
  for (const property of properties) {
    if (property.value?.kind !== "text" && property.value?.kind !== "url") continue;
    const found = /\/\/([^/:\s]+):(\d+)/.exec(property.value.value);
    if (found) return `${found[1]}:${found[2]}`;
  }
  return null;
}

/** The first http(s) URL among `urls`. */
function httpUrl(urls: readonly (string | null)[]): string | null {
  return urls.find((url) => url !== null && /^https?:\/\//i.test(url.trim()))?.trim() ?? null;
}

/**
 * *Create monitor*'s opening (R-11.4): HTTP for a route or a service with its
 * URL plus `/health`, TCP for a database, Docker for a container, Ping for a
 * VM. The target comes from what the asset records; where it records nothing
 * the field is left for the reader.
 */
export function prefill(detail: AssetDetail): Prefill {
  const { asset, properties } = detail;
  const name = asset.name;
  switch (asset.type_id) {
    case "database":
    case "database_server": {
      const host = valueOf(properties, "host") ?? valueOf(properties, "hostname") ?? valueOf(properties, "ip");
      const port = valueOf(properties, "port");
      const target = host !== null && port !== null ? `${host}:${port}` : (addressIn(properties) ?? "");
      return { name, kind: "tcp", target };
    }
    case "container":
      return { name, kind: "docker", target: valueOf(properties, "container_name") ?? name };
    case "vm":
    case "hypervisor":
      return { name, kind: "ping", target: valueOf(properties, "hostname") ?? valueOf(properties, "ip") ?? "" };
    default: {
      const base = httpUrl([
        valueOf(properties, "url"),
        ...detail.exposes.map((route) => route.url),
        ...detail.reachable_via.map((route) => route.url),
      ]);
      const path = valueOf(properties, "health_path") ?? "/health";
      const target = base === null ? "" : `${base.replace(/\/+$/, "")}/${path.replace(/^\/+/, "")}`;
      return { name: `${name} /health`, kind: "http", target };
    }
  }
}
