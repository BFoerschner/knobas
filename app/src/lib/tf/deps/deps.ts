/**
 * Dependencies' drawing rules (#871, R-11.11 to R-11.14), pure: the read in,
 * the bands, the rail and the line naming what is in no layer out.
 *
 * Nothing here reads the window or a command, so every word a band says can
 * be tested as a string. Folding, grouping, filters and search are `draw.ts`'s.
 */
import type { Dependencies, DepItem, DepKind } from "../../ipc/assets";
import { entityAddress } from "../address";

/**
 * How a tile is drawn: solid red, amber, outlined blue, fine, hatched when
 * nothing watches it, or plain where nothing says (R-11.12).
 */
export type TileTone = "err" | "warn" | "info" | "ok" | "none" | "plain";

/** Problems first, then what is moving, then what nobody watches, then the rest. */
export const ORDER: readonly TileTone[] = ["err", "warn", "info", "none", "ok", "plain"];

/** The kinds that are not assets: their state word is not about monitors. */
const NOT_ASSETS = new Set(["ticket", "pr", "page", "repo", "pipeline"]);

/** Whether a kind is an asset or a route: something the estate holds, with an environment. */
export const isEstate = (kind: string): boolean => !NOT_ASSETS.has(kind);

/** Whether a kind is an asset proper: the estate less its routes. What the editor opens. */
export const isAsset = (kind: string): boolean => isEstate(kind) && kind !== "route";

/** Whether a tile's name is code (a repository, an asset) rather than words (a ticket, a PR). */
export const nameIsCode = (kind: string): boolean => kind !== "ticket" && kind !== "pr";

/** A tile's tone and its one state word, in prototype-6's words. */
export function stateOf(item: DepItem): { tone: TileTone; word: string } {
  const kind = item.kind;
  if (kind === "pipeline") {
    if (item.health === "err") return { tone: "err", word: "failed" };
    if (item.health === "info") return { tone: "info", word: "running" };
    if (item.health === "ok") return { tone: "ok", word: "passed" };
    return { tone: "plain", word: item.state ?? "" };
  }
  if (kind === "pr") {
    return item.health === "info" ? { tone: "info", word: item.state ?? "open" } : { tone: "plain", word: item.state ?? "" };
  }
  if (NOT_ASSETS.has(kind)) return { tone: "plain", word: item.state ?? "" };
  // An asset or a route.
  if (item.health === "err") return { tone: "err", word: "down" };
  if (item.health === "warn") return { tone: "warn", word: "warning" };
  if (item.monitors === 0) return { tone: "none", word: "no check" };
  return { tone: "ok", word: item.monitors === 1 ? "1 check" : `${item.monitors} checks` };
}

/** Whether a tile counts as a problem: broken or warning. */
export function isProblem(item: DepItem): boolean {
  return item.health === "err" || item.health === "warn";
}

/** The words a count of one kind takes: `1 VM`, `17 services`, `2 reverse proxies`. */
const PLURAL: Record<string, string> = {
  VM: "VMs",
  "Reverse proxy": "reverse proxies",
  Middleware: "middleware",
  Repository: "repositories",
};

function counted(count: number, label: string): string {
  const keepCase = label === label.toUpperCase();
  const one = keepCase ? label : label.toLowerCase();
  const many = PLURAL[label] ?? (keepCase ? `${label}s` : `${label.toLowerCase()}s`);
  return `${count.toLocaleString("en")} ${count === 1 ? one : many}`;
}

/** What a kind is called, or its id where the read names no label. */
export function kindOf(kinds: readonly DepKind[], id: string): DepKind {
  return kinds.find((kind) => kind.id === id) ?? { id, label: id, monogram: "??" };
}

/** "2 broken, 7 warning", or the empty string when nothing is. */
export function wrongOf(items: readonly DepItem[]): string {
  const broken = items.filter((item) => item.health === "err").length;
  const warning = items.filter((item) => item.health === "warn").length;
  return [broken ? `${broken.toLocaleString("en")} broken` : "", warning ? `${warning.toLocaleString("en")} warning` : ""]
    .filter(Boolean)
    .join(", ");
}

/** The three biggest kinds a list holds: "17 services, 1 reverse proxy", and more. */
export function summaryOf(read: Dependencies, items: readonly DepItem[]): string {
  const by = new Map<string, number>();
  for (const item of items) by.set(item.kind, (by.get(item.kind) ?? 0) + 1);
  const ranked = [...by.entries()].sort((a, b) => b[1] - a[1]);
  const words = ranked.slice(0, 3).map(([kind, count]) => counted(count, kindOf(read.kinds, kind).label));
  return words.join(", ") + (ranked.length > 3 ? ", and more" : "");
}

/**
 * What a tile's second line says: the state word, and the environment where
 * one is set, unless its layer already groups by environment.
 */
export function lineOf(item: DepItem, byEnvironment = false): string {
  const { word } = stateOf(item);
  const env = !byEnvironment && item.environment !== null && item.environment !== "shared" ? item.environment : null;
  return [word, env].filter(Boolean).join(", ");
}

/** Where a click on a tile goes, for the kinds with a view of their own (R-11.15). */
export function hrefOf(item: DepItem): string {
  const id = encodeURIComponent(item.id);
  switch (item.kind) {
    case "ticket":
      return entityAddress("ticket", item.id);
    case "pr":
      return `#/pr/${id}`;
    case "repo":
      return `#/repo/${id}`;
    case "pipeline":
      return `#/build/${id}`;
    case "page":
      return `#/page/${id}`;
    case "route":
      return `#/route/${id}`;
    default:
      return `#/asset/${id}`;
  }
}
