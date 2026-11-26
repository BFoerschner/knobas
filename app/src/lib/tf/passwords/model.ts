/**
 * The Passwords page's model (ADR-0056), computed in the window from the one read the page holds (`passwords_held`):
 * the folder tree, a folder's panels, the filter's hits, an entry's path. Pure, so a folder press and a keystroke cost
 * no request and are tested as data.
 */
import type { Held, PwFolder, PwRow, PwSource } from "../../ipc/passwords";

/** One entry, read out of its row. */
export interface Entry {
  id: string;
  src: number;
  folder: string | null;
  title: string;
  user: string | null;
  address: string | null;
  changed: string | null;
  flags: number;
  expires: string | null;
  also: string[];
}

/** Row flags, as `passwords::reads` sets them. */
export const HAS_CODE = 1;
export const NEEDS_APPROVAL = 2;
export const COMMENT_REQUIRED = 4;
export const CANNOT_READ = 8;

/** One node of the tree: All entries, a source's top (with several), or a folder. */
export interface Node {
  /** `""` for All entries, `src:<id>` for a source's top, else the folder's entity id. */
  id: string;
  name: string;
  parent: Node | null;
  kids: Node[];
  /**
   * The entries directly in it, as indices into the read's rows (`Entries.at`). {@link ordered} makes them entries
   * sorted by title, once, when they are drawn.
   */
  own: number[];
  /** Every entry under it, itself included. */
  total: number;
  depth: number;
  /** Its source's index; -1 for All entries. */
  src: number;
  /** A usage comment is asked for every read here. */
  comment: boolean;
  /** A source's top row. */
  top: boolean;
}

/** The model over one read. */
export interface Model {
  held: Held;
  root: Node;
  nodes: Map<string, Node>;
  entries: Entries;
}

/**
 * The read's entries, each made out of its row when it is first asked for. The first open draws a dozen rows of
 * 20,000: making an object and a map slot for every row before its first frame was most of what was left of its cost
 * (the ux30 end gate, 2026-11-23: 38 ms at 4x in the page's profile).
 */
export class Entries {
  readonly #rows: readonly PwRow[];
  readonly #made: (Entry | undefined)[];
  /** The entries made so far, by id. */
  readonly #madeById = new Map<string, Entry>();
  /** Every row's index by id, once `index` has run. */
  #byId: Map<string, number> | null = null;

  constructor(rows: readonly PwRow[]) {
    this.#rows = rows;
    this.#made = new Array<Entry | undefined>(rows.length);
  }

  /** How many rows the read holds. */
  get size(): number {
    return this.#rows.length;
  }

  /** How many entries have been made out of their rows. */
  get made(): number {
    return this.#madeById.size;
  }

  /** The entry of row `i`. */
  at(i: number): Entry {
    let e = this.#made[i];
    if (e === undefined) {
      const [id, src, folder, title, user, address, changed, flags, expires, also] = this.#rows[i]!;
      e = { id, src, folder, title, user, address, changed, flags, expires, also };
      this.#made[i] = e;
      if (!this.#madeById.has(id)) this.#madeById.set(id, e);
    }
    return e;
  }

  /**
   * The entry `id`, or `undefined`. One already made, or any once {@link index} has run, is a map's lookup; before
   * that, the rows are walked (the recent entries at the first open, an entry named in the address).
   */
  get(id: string): Entry | undefined {
    const made = this.#madeById.get(id);
    if (made !== undefined) return made;
    if (this.#byId !== null) {
      const i = this.#byId.get(id);
      return i === undefined ? undefined : this.at(i);
    }
    for (let i = 0; i < this.#rows.length; i++) if (this.#rows[i]![0] === id) return this.at(i);
    return undefined;
  }

  /** The rows indexed by id so far, while `index` runs. */
  #indexing = new Map<string, number>();
  #indexedTo = 0;

  /**
   * Every row's index by id, so a lookup no longer walks the rows; built once the page has drawn, in slices: it stops
   * when `more` says so, after at least one block of rows, and answers whether it is done.
   */
  index(more: () => boolean = () => true): boolean {
    if (this.#byId !== null) return true;
    const rows = this.#rows;
    do {
      const end = Math.min(rows.length, this.#indexedTo + 1000);
      for (let i = this.#indexedTo; i < end; i++) {
        const id = rows[i]![0];
        if (!this.#indexing.has(id)) this.#indexing.set(id, i);
      }
      this.#indexedTo = end;
    } while (this.#indexedTo < rows.length && more());
    if (this.#indexedTo < rows.length) return false;
    this.#byId = this.#indexing;
    return true;
  }

  /** Every entry, in the read's order, each made. */
  *values(): IterableIterator<Entry> {
    for (let i = 0; i < this.#rows.length; i++) yield this.at(i);
  }
}

/**
 * What the filter reads: every entry by title, and each one's lower-cased title, username, address and path, in the
 * same order. Built on the filter's first use or once the page has drawn (`searchIndex`), never with the model: at
 * 20,000 entries the sort and the strings were most of the model's cost, on the first open's path (the ux30 end gate,
 * 2026-11-23: 418 ms at 6x against the line's 300).
 */
export interface SearchIndex {
  all: Entry[];
  hay: string[];
}

/** The id of a source's top node. */
export const sourceNode = (sourceId: string): string => `src:${sourceId}`;

/**
 * One collator for every sort: `localeCompare` with options builds a collator per call, which made the model of
 * 20,000 entries cost half a second (the page probe, 2026-11-22).
 */
const collate = new Intl.Collator(undefined, { sensitivity: "base" }).compare;
const byTitle = (a: Entry, b: Entry): number => collate(a.title, b.title) || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);

/** The page's model over `held`. With more than one source, each source's folders sit under a top named after it. */
export function model(held: Held): Model {
  const root: Node = { id: "", name: "All entries", parent: null, kids: [], own: [], total: 0, depth: -1, src: -1, comment: false, top: false };
  const nodes = new Map<string, Node>([["", root]]);
  const multi = held.sources.length > 1;
  const tops = held.sources.map((s: PwSource, i: number) => {
    if (!multi) return root;
    const top: Node = { id: sourceNode(s.id), name: s.name, parent: root, kids: [], own: [], total: 0, depth: 0, src: i, comment: false, top: true };
    nodes.set(top.id, top);
    root.kids.push(top);
    return top;
  });
  for (const f of held.folders as PwFolder[]) {
    nodes.set(f.id, { id: f.id, name: f.name, parent: null, kids: [], own: [], total: 0, depth: 0, src: f.source, comment: f.comment_read, top: false });
  }
  for (const f of held.folders as PwFolder[]) {
    const node = nodes.get(f.id)!;
    const parent = (f.parent !== null ? nodes.get(f.parent) : undefined) ?? tops[f.source] ?? root;
    node.parent = parent;
    parent.kids.push(node);
  }
  const rows = held.entries as PwRow[];
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i]!;
    const folder = row[2];
    ((folder !== null ? nodes.get(folder) : undefined) ?? tops[row[1]] ?? root).own.push(i);
    const also = row[9];
    for (let k = 0; k < also.length; k++) ((also[k] !== null ? nodes.get(also[k]!) : undefined) ?? tops[row[1]] ?? root).own.push(i);
  }
  // Depth, order and counts, top down.
  const visit = (node: Node, depth: number): number => {
    node.depth = depth;
    node.kids.sort((a, b) => collate(a.name, b.name));
    let total = node.own.length;
    for (const kid of node.kids) total += visit(kid, depth + 1);
    node.total = total;
    return total;
  };
  visit(root, -1);
  // An entry in several folders counts once in All entries.
  root.total = rows.length;
  return { held, root, nodes, entries: new Entries(rows) };
}

const sorted = new WeakMap<Node, Entry[]>();

/**
 * `node`'s own entries by title, made and sorted on the first call. Not with the model: the top of the page lists a
 * panel per folder, and sorting every folder's entries before the first frame cost about 8 ms at 1x at 20,000 entries,
 * while the frame draws a dozen rows (the ux30 end gate, 2026-11-23).
 */
export function ordered(m: Model, node: Node): Entry[] {
  let out = sorted.get(node);
  if (out === undefined) {
    out = node.own.map((i) => m.entries.at(i)).sort(byTitle);
    sorted.set(node, out);
  }
  return out;
}

/** How many entries a run of the index's merge sort sorts at once. */
const RUN = 512;
/** How many rows or merged entries a slice of the index's build does between looks at `more`. */
const BLOCK = 500;

/**
 * The filter's index over one model, built in slices (`step`) while the page is idle, or at once (`finish`) when the
 * filter is used first. Built at once it was one task of about 140 ms at 4x at 20,000 entries, which delayed the frame
 * after the first open and would hold a press that came then (the ux30 end gate, 2026-11-23). So the sort is a merge
 * sort that can stop between any two steps: runs of {@link RUN} sorted, then merged pair by pair.
 */
export class IndexBuild {
  readonly #m: Model;
  #stage: "index" | "make" | "runs" | "merge" | "hay" | "done" = "index";
  #at = 0;
  #src: Entry[] = [];
  #dst: Entry[] = [];
  #width = RUN;
  #lo = 0;
  #a = 0;
  #b = 0;
  #k = 0;
  #mid = 0;
  #hi = 0;
  #inPair = false;
  #hay: string[] = [];
  readonly #paths = new Map<string | null, string>();
  #index: SearchIndex | null = null;

  constructor(m: Model) {
    this.#m = m;
  }

  /** The index, once built. */
  get index(): SearchIndex | null {
    return this.#index;
  }

  /** Builds on until `more` answers false, one block at least; answers whether the index is built. */
  step(more: () => boolean): boolean {
    do this.#block();
    while (this.#stage !== "done" && more());
    return this.#stage === "done";
  }

  /** The index, built now if it is not yet. */
  finish(): SearchIndex {
    while (this.#stage !== "done") this.#block();
    return this.#index!;
  }

  #block(): void {
    const m = this.#m;
    const n = m.entries.size;
    switch (this.#stage) {
      case "index":
        if (m.entries.index(() => false)) this.#stage = "make";
        return;
      case "make": {
        const end = Math.min(n, this.#at + BLOCK);
        for (let i = this.#at; i < end; i++) this.#src.push(m.entries.at(i));
        this.#at = end;
        if (end === n) {
          this.#stage = "runs";
          this.#at = 0;
        }
        return;
      }
      case "runs": {
        const end = Math.min(n, this.#at + RUN);
        const run = this.#src.slice(this.#at, end).sort(byTitle);
        for (let i = 0; i < run.length; i++) this.#src[this.#at + i] = run[i]!;
        this.#at = end;
        if (end === n) {
          this.#stage = this.#width >= n ? "hay" : "merge";
          this.#dst = new Array<Entry>(n);
          this.#at = 0;
        }
        return;
      }
      case "merge": {
        const src = this.#src;
        const dst = this.#dst;
        for (let budget = BLOCK; budget > 0; budget--) {
          if (!this.#inPair) {
            if (this.#lo >= n) {
              this.#src = dst;
              this.#dst = src;
              this.#width *= 2;
              this.#lo = 0;
              if (this.#width >= n) this.#stage = "hay";
              return;
            }
            this.#mid = Math.min(this.#lo + this.#width, n);
            this.#hi = Math.min(this.#lo + 2 * this.#width, n);
            this.#a = this.#lo;
            this.#b = this.#mid;
            this.#k = this.#lo;
            this.#inPair = true;
          }
          if (this.#k < this.#hi) {
            dst[this.#k++] = this.#b >= this.#hi || (this.#a < this.#mid && byTitle(src[this.#a]!, src[this.#b]!) <= 0) ? src[this.#a++]! : src[this.#b++]!;
          } else {
            this.#inPair = false;
            this.#lo = this.#hi;
          }
        }
        return;
      }
      case "hay": {
        const all = this.#src;
        const end = Math.min(n, this.#at + BLOCK);
        for (let i = this.#at; i < end; i++) {
          const e = all[i]!;
          // One lower-casing of the whole: a flat string, where a join of four is flattened by the first keystroke's
          // `includes`, 20,000 of them in its task.
          this.#hay.push(`${e.title}\n${e.user ?? ""}\n${e.address ?? ""}\n${this.#pathText(e.folder)}`.toLowerCase());
        }
        this.#at = end;
        if (end === n) {
          this.#index = { all, hay: this.#hay };
          this.#stage = "done";
        }
        return;
      }
      case "done":
        return;
    }
  }

  /** A folder's path as the filter reads it, once per folder rather than once per entry. */
  #pathText(folder: string | null): string {
    let text = this.#paths.get(folder);
    if (text === undefined) {
      text = chainOf(this.#m, folder !== null ? (this.#m.nodes.get(folder) ?? null) : null).map((n) => n.name).join(" ").toLowerCase();
      this.#paths.set(folder, text);
    }
    return text;
  }
}

const builds = new WeakMap<Model, IndexBuild>();

/** The build of the filter's index over `m`: one per model, so the idle slices and the filter share it. */
export function indexBuild(m: Model): IndexBuild {
  let build = builds.get(m);
  if (build === undefined) {
    build = new IndexBuild(m);
    builds.set(m, build);
  }
  return build;
}

/** The filter's index over `m`, finished now if the slices have not finished it, and kept with the model. */
export function searchIndex(m: Model): SearchIndex {
  return indexBuild(m).finish();
}

/** The chain of folders an entry sits in, names, outermost first (a source's top included, with several sources). */
export function pathOf(m: Model, e: Entry): string[] {
  return chainOf(m, e.folder !== null ? m.nodes.get(e.folder) ?? null : null).map((n) => n.name);
}

/** The nodes from the top down to `node`, All entries left out. */
export function chainOf(_m: Model, node: Node | null): Node[] {
  const out: Node[] = [];
  for (let at = node; at !== null && at.id !== ""; at = at.parent) out.unshift(at);
  return out;
}

/** Whether `node` is `under` or below it. */
export function within(node: Node | null, under: Node): boolean {
  for (let at = node; at !== null; at = at.parent) if (at === under) return true;
  return false;
}

/** The nearest folder at or above `node` that asks for a usage comment, or `null`. */
export function commentFolder(node: Node | null): Node | null {
  for (let at = node; at !== null; at = at.parent) if (at.comment) return at;
  return null;
}

/** One panel of a folder's list. */
export interface Panel {
  kind: "recent" | "folder" | "hits";
  /** The folder whose own entries it lists (folder panels). */
  node: Node | null;
  /** How many rows it holds. */
  count: number;
  /** Its rows, by title: a folder panel's are sorted when first read (`ordered`). */
  readonly rows: Entry[];
}

/** How many rows a panel further down shows before *Show all*. */
export const CUT = 5;
/** A panel cut at {@link CUT} has more than this many rows. */
export const CUT_OVER = 6;

const scopes = new WeakMap<Node, ReadonlySet<Entry>>();

/**
 * The entries the filter reads at `node` (#2531): every entry listed in it or in a folder under it, an entry also
 * listed there included, as `model` placed them in the folders' `own`. `null` at All entries, which reads every entry.
 * Kept per node, so a keystroke costs a set's lookup per entry. The seam the Rust filter (#2535) takes: what it ranks
 * is held to this set.
 */
export function scopeOf(m: Model, node: Node): ReadonlySet<Entry> | null {
  if (node === m.root) return null;
  let scope = scopes.get(node);
  if (scope === undefined) {
    const out = new Set<Entry>();
    const walk = (at: Node): void => {
      for (let i = 0; i < at.own.length; i++) out.add(m.entries.at(at.own[i]!));
      for (const kid of at.kids) walk(kid);
    };
    walk(node);
    scope = out;
    scopes.set(node, scope);
  }
  return scope;
}

/**
 * What a folder lists: at the top, the recent entries; then its own entries; then one panel for each folder below
 * that holds entries, in tree order. With a filter, one panel of every match inside the folder and the folders under
 * it ({@link scopeOf}), by title.
 */
export function panels(m: Model, node: Node, query: string): Panel[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length > 0) {
    const { all, hay } = searchIndex(m);
    const scope = scopeOf(m, node);
    // A plain loop, not a closure per entry and word: a keystroke reads all 20,000.
    const rows: Entry[] = [];
    outer: for (let i = 0; i < all.length; i++) {
      if (scope !== null && !scope.has(all[i]!)) continue;
      const text = hay[i]!;
      for (let w = 0; w < words.length; w++) if (!text.includes(words[w]!)) continue outer;
      rows.push(all[i]!);
    }
    return [{ kind: "hits", node: null, count: rows.length, rows }];
  }
  const out: Panel[] = [];
  if (node === m.root) {
    const recent = m.held.recent.map((id) => m.entries.get(id)).filter((e): e is Entry => e !== undefined);
    if (recent.length > 0) out.push({ kind: "recent", node: null, count: recent.length, rows: recent });
  }
  const walk = (at: Node): void => {
    if (at.own.length > 0) {
      out.push({
        kind: "folder",
        node: at,
        count: at.own.length,
        get rows() {
          return ordered(m, at);
        },
      });
    }
    for (const kid of at.kids) walk(kid);
  };
  walk(node);
  return out;
}

/** The nodes whose own entries hold `e`, as `model` places it: its folder and the folders it is also in. */
export function homesOf(m: Model, e: Entry): Node[] {
  const top = m.held.sources.length > 1 ? (m.nodes.get(sourceNode(m.held.sources[e.src]?.id ?? "")) ?? m.root) : m.root;
  return [e.folder, ...e.also].map((home) => (home !== null ? m.nodes.get(home) : undefined) ?? top);
}

/** The ids of every folder at or above an entry's folders: the folds that hold it. */
export function holding(m: Model, e: Entry | null): Set<string> {
  const out = new Set<string>();
  if (e === null) return out;
  for (const home of [e.folder, ...e.also]) {
    for (let at = home !== null ? m.nodes.get(home) ?? null : null; at !== null; at = at.parent) out.add(at.id);
  }
  return out;
}

/** One row of the tree as drawn: a node and whether its fold is open. */
export interface TreeRow {
  node: Node;
  open: boolean;
}

/** The tree's rows on show: All entries, then each open node's kids in order. */
export function treeRows(m: Model, open: ReadonlySet<string>): TreeRow[] {
  const out: TreeRow[] = [{ node: m.root, open: true }];
  const walk = (node: Node): void => {
    for (const kid of node.kids) {
      const isOpen = open.has(kid.id) && kid.kids.length > 0;
      out.push({ node: kid, open: isOpen });
      if (isOpen) walk(kid);
    }
  };
  walk(m.root);
  return out;
}

/** The marks of `query`'s words in `text`, as runs of plain and marked text. */
export function marks(text: string, query: string): { text: string; hit: boolean }[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0 || text === "") return [{ text, hit: false }];
  const lower = text.toLowerCase();
  const hit = new Array<boolean>(text.length).fill(false);
  for (const w of words) {
    for (let at = lower.indexOf(w); at !== -1; at = lower.indexOf(w, at + 1)) hit.fill(true, at, at + w.length);
  }
  const out: { text: string; hit: boolean }[] = [];
  for (let i = 0; i < text.length; i++) {
    const last = out[out.length - 1];
    if (last && last.hit === hit[i]) last.text += text[i];
    else out.push({ text: text[i]!, hit: hit[i]! });
  }
  return out;
}

/** An expiry as the row and the entry say it: the tone and the words, `null` far off or with none. */
export function expiry(expires: string | null, now: Date = new Date()): { days: number; tone: "" | "warn" | "err"; text: string } | null {
  if (expires === null) return null;
  const end = new Date(`${expires}T00:00:00`);
  if (Number.isNaN(end.getTime())) return null;
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Math.round((end.getTime() - today.getTime()) / 86_400_000);
  const tone = days <= 3 ? "err" : days <= 14 ? "warn" : "";
  const text = days < 0 ? "Expired" : days === 0 ? "Expires today" : days === 1 ? "Expires in 1 day" : `Expires in ${days} days`;
  return { days, tone, text };
}

/** An address that opens in the browser: `http://` or `https://`. */
export const isWeb = (address: string): boolean => /^https?:\/\//i.test(address.trim());

/** An address without its scheme and trailing slash, as a row shows it. */
export const bare = (address: string): string => address.replace(/^https?:\/\//i, "").replace(/\/$/, "");

/** The source's word in a sentence: *the password manager*, or its own name with several sources (`sourceWord`). */
export function sourceWord(held: Held, src: number): string {
  return held.sources.length > 1 ? held.sources[src]?.name ?? "the password manager" : "the password manager";
}

/** `n entry`, `n entries`. */
export const plural = (n: number, one: string, many = `${one}s`): string => `${n.toLocaleString("en")} ${n === 1 ? one : many}`;
