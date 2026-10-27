/**
 * What Dependencies draws once a view, the filters and the reader's own
 * gestures are applied (#872, R-11.15, R-11.17, R-11.18; #873, R-11.16), pure:
 * the read and the state in, the bands, their groups and tiles, the folded
 * chips and the line naming what is in no layer out.
 *
 * - **Touching** is the relations walked both ways from an item: everything
 *   it leads to and everything that leads to it, transitively in each
 *   direction (not the whole connected graph).
 * - **Visible** narrows by *Show only what this touches* (`trace`), by
 *   *Problems and what they touch*, and by the environment chips; an item with
 *   no environment passes the chips.
 * - **Folding**: problems never fold; a group with no problem folds into a
 *   chip once its layer holds more than the view's fold count; an open group
 *   shows its problems and a few more, then "N more, all fine"; a layer folds
 *   to its summary. *Find* opens every fold that holds a match; an open
 *   group draws its first `FIND_SHOWS` matches before the few more, and its
 *   "N more, K found" holds the rest (#2212).
 * - Every item drawn is somewhere: its own tile, or the "more" button, chip
 *   or folded band that stands for it. `where` maps an item to that element,
 *   so a line to a folded item lands on its fold.
 */
import type { DepGroup, DepItem, DepLayer, DepTiles, DepViewSettings, Dependencies, LayerInView } from "../../ipc/assets";
import { isEstate, isProblem, kindOf, ORDER, stateOf, summaryOf, wrongOf, type TileTone } from "./deps";

/** The relations as adjacency, both ways: what each item leads to, and what leads to it. */
export interface Graph {
  out: Map<string, string[]>;
  into: Map<string, string[]>;
}

export function graphOf(read: Dependencies): Graph {
  const out = new Map<string, string[]>();
  const into = new Map<string, string[]>();
  for (const relation of read.relations) {
    // Traversal runs "toward what it stands on". Every relation is read that way
    // (`inside` child -> holder, `lands-on` route -> target) except `exposes`: the
    // read's contract (§10.8, #871) emits it exposing asset -> route, so the graph
    // stores it reversed. Only here; `holders` reads the raw relation.
    const [from, to] = relation.relation === "exposes" ? [relation.to, relation.from] : [relation.from, relation.to];
    (out.get(from) ?? out.set(from, []).get(from)!).push(to);
    (into.get(to) ?? into.set(to, []).get(to)!).push(from);
  }
  return { out, into };
}

function reach(edges: Map<string, string[]>, from: string, into: Set<string>) {
  const seen = new Set([from]);
  let frontier = [from];
  while (frontier.length > 0) {
    const next: string[] = [];
    for (const id of frontier)
      for (const to of edges.get(id) ?? [])
        if (!seen.has(to)) {
          seen.add(to);
          next.push(to);
        }
    frontier = next;
  }
  for (const id of seen) into.add(id);
}

/** Everything `id` touches, up and down, itself included. */
export function touches(graph: Graph, id: string): Set<string> {
  const all = new Set<string>();
  reach(graph.out, id, all);
  reach(graph.into, id, all);
  return all;
}

/** What a view does with a layer it does not name: shows it, unfolded and ungrouped. */
export const UNNAMED_LAYER: Readonly<LayerInView> = { hidden: false, folded: false, group: "none" };

/** A layer as a view shows it. */
export function layerIn(settings: DepViewSettings, layer: string): LayerInView {
  return settings.layers[layer] ?? { ...UNNAMED_LAYER };
}

/** The element keys `where` uses: a folded band, a chip, a "more" button. */
export const bandEl = (layer: string) => `b:${layer}`;
export const chipEl = (group: string) => `c:${group}`;
export const moreEl = (group: string) => `m:${group}`;

/** The reader's own gestures, held for the session (never saved with a view). */
export interface Gestures {
  /** What *Find* holds. */
  query: string;
  /** *Show only what this touches*: an item id, or `null`. */
  trace: string | null;
  /** Groups the reader opened or shut by hand, by group key. */
  open: ReadonlySet<string>;
  shut: ReadonlySet<string>;
  /** Groups whose "N more" the reader opened. */
  more: ReadonlySet<string>;
  /** Layers the reader folded or unfolded by hand, over the view. */
  fold: ReadonlyMap<string, boolean>;
  /**
   * An item just added or saved (#876): its layer, group and tile are drawn
   * open so it can be scrolled to and flash, without opening its whole group.
   */
  reveal?: string | null;
}

export const NO_GESTURES: Gestures = {
  query: "",
  trace: null,
  open: new Set(),
  shut: new Set(),
  more: new Set(),
  fold: new Map(),
};

const words = (query: string) => query.toLowerCase().split(/\s+/).filter(Boolean);

/** Whether an item matches *Find*: every word in its name, kind, environment or owner. */
export function matcher(read: Dependencies, query: string): (item: DepItem) => boolean {
  const want = words(query);
  if (want.length === 0) return () => false;
  const texts = textsOf(read);
  return (item) => {
    let text = texts.get(item);
    if (text === undefined) {
      text = [item.name, kindOf(read.kinds, item.kind).label, item.state, item.environment, item.owner]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      texts.set(item, text);
    }
    return want.every((word) => text.includes(word));
  };
}

/** The read's items in tile order (problems first, then by name), sorted once per read: a keystroke need not sort them again (#2212). */
const sorted = new WeakMap<Dependencies, DepItem[]>();
function sortedOf(read: Dependencies): DepItem[] {
  let items = sorted.get(read);
  if (items === undefined) {
    items = [...read.items].sort(
      (a, b) => ORDER.indexOf(stateOf(a).tone) - ORDER.indexOf(stateOf(b).tone) || a.name.localeCompare(b.name),
    );
    sorted.set(read, items);
  }
  return items;
}

/** What *Find* searches in each item, kept per read so a keystroke does not build it again (#2212). */
const searched = new WeakMap<Dependencies, WeakMap<DepItem, string>>();
const textsOf = (read: Dependencies) => searched.get(read) ?? searched.set(read, new WeakMap()).get(read)!;

/** Which items the view, the trace and the filters leave. */
export function visibleOf(read: Dependencies, graph: Graph, settings: DepViewSettings, trace: string | null): (item: DepItem) => boolean {
  let keep: Set<string> | null = null;
  if (trace !== null && read.items.some((item) => item.id === trace)) keep = touches(graph, trace);
  if (settings.problems) {
    const near = new Set<string>();
    for (const item of read.items) if (isProblem(item)) for (const id of touches(graph, item.id)) near.add(id);
    keep = keep === null ? near : new Set([...keep].filter((id) => near.has(id)));
  }
  const envs = settings.environments;
  return (item) =>
    (keep === null || keep.has(item.id)) &&
    (envs.length === 0 || item.environment === null || envs.includes(item.environment));
}

/** The name of the asset each item sits inside or is exposed by, for *What it runs on*. */
function holders(read: Dependencies): Map<string, string> {
  const name = new Map(read.items.map((item) => [item.id, item.name]));
  const at = new Map<string, string>();
  for (const relation of read.relations) {
    if (relation.relation === "inside") {
      const holder = name.get(relation.to);
      if (holder !== undefined) at.set(relation.from, holder);
    } else if (relation.relation === "exposes") {
      const holder = name.get(relation.from);
      if (holder !== undefined) at.set(relation.to, holder);
    }
  }
  return at;
}

/** The group an item falls in under one *Group by*, in prototype-6's words; `""` is no heading. */
export function groupOf(item: DepItem, mode: DepGroup, holder: string | undefined): string {
  switch (mode) {
    case "environment":
      return item.environment ?? (isEstate(item.kind) ? "no environment" : "");
    case "owner":
      return item.owner ?? "no owner";
    case "parent":
      return holder !== undefined ? `on ${holder}` : "top of the estate";
    case "project":
      return item.owner ?? "no project";
    case "status":
      return item.kind === "ticket" ? (item.state ?? "") : item.kind === "pr" ? "Open pull requests" : item.kind === "page" ? "Pages" : "";
    default:
      return "";
  }
}

/** One tone's share of a health bar. */
export interface Mix {
  tone: TileTone;
  count: number;
}

function mixOf(items: readonly DepItem[]): Mix[] {
  return ORDER.map((tone) => ({ tone, count: items.filter((item) => stateOf(item).tone === tone).length })).filter((part) => part.count > 0);
}

/** An open group: its heading (`""` for none), its drawn tiles and the button for the rest. */
export interface OpenGroup {
  key: string;
  name: string;
  count: number;
  wrong: string;
  tiles: DepItem[];
  /** The rest: how many, what is wrong with them, and how many *Find* matches among them. */
  more: { el: string; count: number; wrong: string; hits: number } | null;
}

/** How many of a group's matches *Find* draws as tiles; the rest wait behind its "more" (#2212). */
export const FIND_SHOWS = 24;

/** A group folded into a chip. */
export interface ShutGroup {
  key: string;
  el: string;
  name: string;
  count: number;
  wrong: string;
  mix: Mix[];
}

/** A band, ready to draw. */
export interface DrawnBand {
  id: string;
  name: string;
  kinds: readonly string[];
  /** How the view groups this layer. */
  group: DepGroup;
  summary: string;
  wrong: string;
  mix: Mix[];
  count: number;
  /** Folded to its summary: `el` stands for every item in it. */
  folded: boolean;
  el: string;
  compact: boolean;
  open: OpenGroup[];
  shut: ShutGroup[];
  /** The words before the chips: "Folded, all fine" beside open groups, else "All fine, folded". */
  shutLabel: string;
}

export interface Drawn {
  bands: DrawnBand[];
  /** The layers this view hides, with how many items each would show. */
  hidden: { id: string; name: string; count: number }[];
  /** The rail: every layer, its visible count and problems. */
  rail: { id: string; name: string; count: number; problems: number; shown: boolean }[];
  /** Items whose kind sits in no layer, and those kinds' names. */
  unplaced: { count: number; kinds: string[] };
  /** How many visible items *Find* matches in the shown layers. */
  hits: number;
  /** Which element stands for each drawn item: `t:<id>` its tile, else a fold's `el`. */
  where: Map<string, string>;
  /** Every problem in the read, for the toolbar's count. */
  problems: number;
}

/** Above this many items in a layer, `auto` tiles shrink to one line (R-11.12). */
export const COMPACT_ABOVE = 30;

export function compactFor(tiles: DepTiles, count: number): boolean {
  return tiles === "compact" || (tiles === "auto" && count > COMPACT_ABOVE);
}

export const tileEl = (id: string) => `t:${id}`;

/** Everything Dependencies draws for this read, view and session. */
export function drawOf(read: Dependencies, settings: DepViewSettings, gestures: Gestures, graph: Graph = graphOf(read)): Drawn {
  const visible = visibleOf(read, graph, settings, gestures.trace);
  const hit = matcher(read, gestures.query);
  const held = holders(read);
  const layerOfKind = new Map<string, DepLayer>();
  for (const layer of read.layers) for (const kind of layer.kinds) layerOfKind.set(kind, layer);

  const byLayer = new Map<string, DepItem[]>(read.layers.map((layer) => [layer.id, []]));
  const unplaced: DepItem[] = [];
  for (const item of read.items) if (!layerOfKind.has(item.kind)) unplaced.push(item);
  // In tile order already: each layer's items keep it.
  for (const item of sortedOf(read)) {
    const layer = layerOfKind.get(item.kind);
    if (layer !== undefined && visible(item)) byLayer.get(layer.id)!.push(item);
  }

  const where = new Map<string, string>();
  const put = (items: readonly DepItem[], el: string) => {
    for (const item of items) if (!where.has(item.id)) where.set(item.id, el);
  };
  let hits = 0;
  const bands: DrawnBand[] = [];

  for (const layer of read.layers) {
    const shown = layerIn(settings, layer.id);
    if (shown.hidden) continue;
    const items = byLayer.get(layer.id)!;
    const anyHit = items.some((item) => hit(item) || item.id === gestures.reveal);
    const folded = (gestures.fold.get(layer.id) ?? shown.folded) && !anyHit;
    const compact = compactFor(settings.tiles, items.length);
    const big = items.length > settings.fold_at;
    const cap = big ? (compact ? 6 : 4) : compact ? 60 : 24;
    const band: DrawnBand = {
      id: layer.id,
      name: layer.name,
      kinds: layer.kinds,
      group: shown.group,
      summary: summaryOf(read, items),
      wrong: wrongOf(items),
      mix: mixOf(items),
      count: items.length,
      folded,
      el: bandEl(layer.id),
      compact,
      open: [],
      shut: [],
      shutLabel: "",
    };
    bands.push(band);
    hits += items.filter(hit).length;
    if (folded) {
      put(items, band.el);
      continue;
    }

    const groups = new Map<string, DepItem[]>();
    for (const item of items) {
      const name = shown.group === "none" ? "" : groupOf(item, shown.group, held.get(item.id));
      (groups.get(name) ?? groups.set(name, []).get(name)!).push(item);
    }
    const ordered = [...groups.entries()].sort(
      (a, b) => Number(b[1].some(isProblem)) - Number(a[1].some(isProblem)) || b[1].length - a[1].length,
    );
    for (const [name, members] of ordered) {
      const key = `${layer.id}|${name}`;
      const problems = members.some(isProblem);
      const matched = members.some(hit);
      const revealed = gestures.reveal != null && members.some((item) => item.id === gestures.reveal);
      const isOpen =
        gestures.open.has(key) || matched || revealed || (!gestures.shut.has(key) && (problems || !big || ordered.length === 1));
      if (!isOpen) {
        const el = chipEl(key);
        put(members, el);
        band.shut.push({ key, el, name, count: members.length, wrong: wrongOf(members), mix: mixOf(members) });
        continue;
      }
      // Problems and the revealed item always, then the first `FIND_SHOWS`
      // matches, then the usual few; the rest, matches included, behind the
      // group's own "more" (#2212).
      let tiles = members;
      let left: DepItem[] = [];
      if (!gestures.more.has(key)) {
        const lead: DepItem[] = [];
        const rest: DepItem[] = [];
        let found = 0;
        for (const item of members) {
          if (isProblem(item) || item.id === gestures.reveal || (hit(item) && found++ < FIND_SHOWS)) lead.push(item);
          else rest.push(item);
        }
        const room = Math.max(0, cap - lead.length);
        const fill = rest.filter((item) => !hit(item)).slice(0, room);
        const filled = new Set(fill);
        tiles = [...lead, ...fill];
        left = rest.filter((item) => !filled.has(item));
      }
      for (const tile of tiles) where.set(tile.id, tileEl(tile.id));
      const more = moreEl(key);
      put(left, more);
      band.open.push({
        key,
        name,
        count: members.length,
        wrong: wrongOf(members),
        tiles,
        more: left.length > 0 ? { el: more, count: left.length, wrong: wrongOf(left), hits: left.filter(hit).length } : null,
      });
    }
    band.shutLabel = band.open.length > 0 ? "Folded, all fine" : "All fine, folded";
  }

  const rail = read.layers.map((layer) => {
    const items = byLayer.get(layer.id)!;
    return {
      id: layer.id,
      name: layer.name,
      count: items.length,
      problems: items.filter(isProblem).length,
      shown: !layerIn(settings, layer.id).hidden,
    };
  });
  return {
    bands,
    hidden: rail.filter((row) => !row.shown).map(({ id, name, count }) => ({ id, name, count })),
    rail,
    unplaced: { count: unplaced.length, kinds: [...new Set(unplaced.map((item) => kindOf(read.kinds, item.kind).label))] },
    hits,
    where,
    problems: read.items.filter(isProblem).length,
  };
}

/** What *Find* says beside its field. */
export function hitsLine(query: string, hits: number): string {
  if (query.trim() === "") return "";
  if (hits === 0) return "none found";
  return hits === 1 ? "1 found" : `${hits.toLocaleString("en")} found, Enter for the next`;
}

/** "N more, all fine" or "N more, 2 broken"; with *Find*'s matches among them, "N more, 3 found" (#2212). */
export function moreLine(count: number, wrong: string, hits = 0): string {
  const more = `${count.toLocaleString("en")} more`;
  if (hits > 0) return `${more}, ${hits.toLocaleString("en")} found${wrong ? `, ${wrong}` : ""}`;
  return `${more}, ${wrong || "all fine"}`;
}

/** A tile's place on screen, for the arrow keys. */
export interface Rect {
  left: number;
  top: number;
}

/**
 * Which tile an arrow key moves to: left and right along the same row, up and
 * down to the nearest row above or below, nearest first (prototype-6's rule).
 */
export function nextTile(from: Rect, others: readonly Rect[], key: string): number {
  const fits = (to: Rect) =>
    key === "ArrowRight"
      ? to.left > from.left + 4 && Math.abs(to.top - from.top) < 8
      : key === "ArrowLeft"
        ? to.left < from.left - 4 && Math.abs(to.top - from.top) < 8
        : key === "ArrowDown"
          ? to.top > from.top + 4
          : key === "ArrowUp"
            ? to.top < from.top - 4
            : false;
  let best = -1;
  let score = Infinity;
  others.forEach((to, index) => {
    if (!fits(to)) return;
    const s = Math.abs(to.top - from.top) + Math.abs(to.left - from.left) / 3;
    if (s < score) {
      score = s;
      best = index;
    }
  });
  return best;
}
