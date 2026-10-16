/**
 * Dependencies' drawing rules (#871, R-11.11 to R-11.14), pure: the read in,
 * the bands, the rail and the line naming what is in no layer out.
 *
 * Nothing here reads the window or a command, so every word a band says can
 * be tested as a string. The layers and the items come from one read
 * (`dependencies`); folding, grouping, lines and search are #872's and #873's.
 */
import type { Dependencies, DepItem, DepKind } from "../../ipc/assets";

/**
 * How a tile is drawn: solid red, amber, outlined blue, fine, hatched when
 * nothing watches it, or plain where nothing says (R-11.12).
 */
export type TileTone = "err" | "warn" | "info" | "ok" | "none" | "plain";

/** Problems first, then what is moving, then what nobody watches, then the rest. */
const ORDER: readonly TileTone[] = ["err", "warn", "info", "none", "ok", "plain"];

/** Above this many items in a layer the tiles shrink to one line (R-11.12). */
export const COMPACT_ABOVE = 30;

/** The kinds that are not assets: their state word is not about monitors. */
const NOT_ASSETS = new Set(["ticket", "pr", "page", "repo", "pipeline"]);

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

/** One band, ready to draw. */
export interface Band {
  id: string;
  name: string;
  /** The three biggest kinds it holds: "17 services, 1 reverse proxy". */
  summary: string;
  /** What is wrong: "2 broken, 7 warning", or "" when all is fine. */
  wrong: string;
  /** How many tiles of each tone, for the health bar, problems first. */
  mix: { tone: TileTone; count: number }[];
  /** Its tiles, problems first, then by name. */
  items: DepItem[];
  /** Whether the tiles shrink to one line. */
  compact: boolean;
}

function layerOfKind(read: Dependencies): Map<string, string> {
  const at = new Map<string, string>();
  for (const layer of read.layers) for (const kind of layer.kinds) at.set(kind, layer.id);
  return at;
}

function itemsByLayer(read: Dependencies): Map<string, DepItem[]> {
  const at = layerOfKind(read);
  const by = new Map<string, DepItem[]>(read.layers.map((layer) => [layer.id, []]));
  for (const item of read.items) {
    const layer = at.get(item.kind);
    if (layer !== undefined) by.get(layer)!.push(item);
  }
  return by;
}

function summaryOf(read: Dependencies, items: readonly DepItem[]): string {
  const by = new Map<string, number>();
  for (const item of items) by.set(item.kind, (by.get(item.kind) ?? 0) + 1);
  const ranked = [...by.entries()].sort((a, b) => b[1] - a[1]);
  const words = ranked.slice(0, 3).map(([kind, count]) => counted(count, kindOf(read.kinds, kind).label));
  return words.join(", ") + (ranked.length > 3 ? ", and more" : "");
}

/** The bands to draw, top to bottom: every layer not in `hidden`. */
export function bandsOf(read: Dependencies, hidden: ReadonlySet<string>): Band[] {
  const by = itemsByLayer(read);
  return read.layers
    .filter((layer) => !hidden.has(layer.id))
    .map((layer) => {
      const items = [...by.get(layer.id)!].sort(
        (a, b) => ORDER.indexOf(stateOf(a).tone) - ORDER.indexOf(stateOf(b).tone) || a.name.localeCompare(b.name),
      );
      const mix = ORDER.map((tone) => ({ tone, count: items.filter((item) => stateOf(item).tone === tone).length })).filter(
        (part) => part.count > 0,
      );
      return {
        id: layer.id,
        name: layer.name,
        summary: summaryOf(read, items),
        wrong: wrongOf(items),
        mix,
        items,
        compact: items.length > COMPACT_ABOVE,
      };
    });
}

/** One row of the rail. */
export interface RailRow {
  id: string;
  name: string;
  count: number;
  problems: number;
  shown: boolean;
}

/** Every layer with its count and its problem count (R-11.14). */
export function railOf(read: Dependencies, hidden: ReadonlySet<string>): RailRow[] {
  const by = itemsByLayer(read);
  return read.layers.map((layer) => {
    const items = by.get(layer.id)!;
    return {
      id: layer.id,
      name: layer.name,
      count: items.length,
      problems: items.filter(isProblem).length,
      shown: !hidden.has(layer.id),
    };
  });
}

/** The items whose kind sits in no layer, and those kinds' names (R-11.13). */
export function unplacedOf(read: Dependencies): { count: number; kinds: string[] } {
  const at = layerOfKind(read);
  const left = read.items.filter((item) => !at.has(item.kind));
  return { count: left.length, kinds: [...new Set(left.map((item) => kindOf(read.kinds, item.kind).label))] };
}

/** What a tile's second line says: the state word, and the environment where one is set. */
export function lineOf(item: DepItem): string {
  const { word } = stateOf(item);
  const env = item.environment !== null && item.environment !== "shared" ? item.environment : null;
  return [word, env].filter(Boolean).join(", ");
}

/** Where a click on a tile goes, for the kinds with a view of their own (R-11.15). */
export function hrefOf(item: DepItem): string {
  const id = encodeURIComponent(item.id);
  switch (item.kind) {
    case "ticket":
      return `#/job/${encodeURIComponent(item.name)}`;
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
