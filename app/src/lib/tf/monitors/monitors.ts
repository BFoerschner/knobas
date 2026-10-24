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
import { bar, chipOf as chipStateOf } from "../../assets/monitors";
import { matchesAll } from "../filter/filter";
import { stampLabel } from "../../time/format";
import type { JobIndex } from "../job/open-jobs";

/** A chip's tone; `null` is the muted chip (paused, no reading). */
export type ChipTone = "ok" | "warn" | "err" | "info" | null;

/** Where each chip sorts: down, warning, then what is neither up nor broken, paused, up. */
const RANK: Record<string, number> = { down: 0, warn: 1, pending: 2, other: 3, paused: 4, up: 5 };

/** The roster in prototype-6's order: down, warning, paused, up; by name inside each. */
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

/** When the newest run of `down` samples began, or `null` when no sample says. */
function downSince(row: MonitorRow): Date | null {
  let since: string | null = null;
  for (let at = row.samples.length - 1; at >= 0; at -= 1) {
    const sample = row.samples[at]!;
    if (sample.state === null) continue;
    if (sample.state !== "down") break;
    since = sample.taken_at;
  }
  const date = since === null ? null : new Date(since);
  return date !== null && !Number.isNaN(date.getTime()) ? date : null;
}

/**
 * The state chip: its words, as prototype-6 words them, and its tone. *down
 * since 12:40* today, else *yesterday 12:40* or *2026-10-12 12:40*
 * (`stampLabel`, #1986).
 */
export function stateChip(row: MonitorRow, now: Date = new Date()): { label: string; tone: ChipTone } {
  switch (chipStateOf(row)) {
    case "paused":
      return { label: "paused", tone: null };
    case "down": {
      const since = downSince(row);
      return { label: since ? `down since ${stampLabel(since.toISOString(), now, true)}` : "down", tone: "err" };
    }
    case "warn":
      return {
        label: row.response_time_ms !== null ? `slow, ${responseText(row.response_time_ms)}` : "warning",
        tone: "warn",
      };
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

/** The 24-hour bar: 48 half-hours, oldest first; a paused monitor's is paused throughout. */
export function segments(row: MonitorRow, now: number): Segment[] {
  return bar(row.samples, now).map((bucket) => {
    if (row.tombstoned) return "paused";
    if (bucket.state === null) return "gap";
    if (bucket.state === "down") return "down";
    if (bucket.state === "warn") return "warn";
    if (bucket.state === "up") return "up";
    return "paused";
  });
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

/** The state chips of the filter bar, and which chip of a row each keeps. */
export const STATE_CHIPS = ["Down", "Warning", "Up", "Paused"] as const;
const CHIP_STATE: Record<string, string> = { Down: "down", Warning: "warn", Up: "up", Paused: "paused" };

/** What the filter bar has set. */
export interface MonitorFilter {
  scope: "job" | "mine" | "all";
  /** The job's ticket key; *This job* keeps nothing without one. */
  job: string | null;
  /** The state chips on; none keeps every state. */
  states: readonly string[];
  /** The type chosen, in {@link kindWord}'s words; `""` is *Any*. */
  type: string;
  text: string;
}

/** The ids a row is reached by: the monitor's own, and every asset it watches. */
const idsOf = (row: MonitorRow) => [row.entity_id, ...row.assets.map((asset) => asset.id)];

/** The rows the filter keeps, in the order given. */
export function visibleMonitors(rows: readonly MonitorRow[], filter: MonitorFilter, index: JobIndex): MonitorRow[] {
  return rows.filter((row) => {
    if (filter.scope === "job") {
      const job = filter.job;
      if (job === null || !idsOf(row).some((id) => index.holds(job, { entity_id: id }))) return false;
    }
    if (filter.scope === "mine" && !idsOf(row).some((id) => index.jobOf({ entity_id: id, kind: null }) !== null)) {
      return false;
    }
    if (filter.states.length > 0 && !filter.states.some((chip) => CHIP_STATE[chip] === chipStateOf(row))) return false;
    if (filter.type !== "" && kindWord(row.monitor_type) !== filter.type) return false;
    const haystack = [row.name, kindWord(row.monitor_type), ...row.assets.flatMap((a) => [a.name, a.type_label])].join(" ");
    return matchesAll(haystack, filter.text);
  });
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
