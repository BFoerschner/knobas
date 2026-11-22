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
  /** Lower-cased title, username, address and path: what the filter reads. */
  hay: string;
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
  /** The entries directly in it, by title. */
  own: Entry[];
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
  entries: Map<string, Entry>;
  /** Every entry, by title. */
  all: Entry[];
}

/** The id of a source's top node. */
export const sourceNode = (sourceId: string): string => `src:${sourceId}`;

const byTitle = (a: Entry, b: Entry): number => a.title.localeCompare(b.title, undefined, { sensitivity: "base" }) || a.id.localeCompare(b.id);

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
  const entries = new Map<string, Entry>();
  const all: Entry[] = [];
  for (const row of held.entries as PwRow[]) {
    const [id, src, folder, title, user, address, changed, flags, expires, also] = row;
    const e: Entry = { id, src, folder, title, user, address, changed, flags, expires, also, hay: "" };
    entries.set(id, e);
    all.push(e);
    const homes = [folder, ...e.also];
    for (const home of homes) {
      const node = (home !== null ? nodes.get(home) : undefined) ?? tops[src] ?? root;
      node.own.push(e);
    }
  }
  // Depth, order and counts, top down.
  const visit = (node: Node, depth: number): number => {
    node.depth = depth;
    node.kids.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
    node.own.sort(byTitle);
    let total = node.own.length;
    for (const kid of node.kids) total += visit(kid, depth + 1);
    node.total = total;
    return total;
  };
  visit(root, -1);
  // An entry in several folders counts once in All entries.
  root.total = all.length;
  all.sort(byTitle);
  const m: Model = { held, root, nodes, entries, all };
  for (const e of all) e.hay = [e.title, e.user ?? "", e.address ?? "", pathOf(m, e).join(" ")].join("\n").toLowerCase();
  return m;
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
  rows: Entry[];
}

/** How many rows a panel further down shows before *Show all*. */
export const CUT = 5;
/** A panel cut at {@link CUT} has more than this many rows. */
export const CUT_OVER = 6;

/**
 * What a folder lists: at the top, the recent entries; then its own entries; then one panel for each folder below
 * that holds entries, in tree order. With a filter, one panel of every match, by title.
 */
export function panels(m: Model, node: Node, query: string): Panel[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length > 0) {
    return [{ kind: "hits", node: null, rows: m.all.filter((e) => words.every((w) => e.hay.includes(w))) }];
  }
  const out: Panel[] = [];
  if (node === m.root) {
    const recent = m.held.recent.map((id) => m.entries.get(id)).filter((e): e is Entry => e !== undefined);
    if (recent.length > 0) out.push({ kind: "recent", node: null, rows: recent });
  }
  const walk = (at: Node): void => {
    if (at.own.length > 0) out.push({ kind: "folder", node: at, rows: at.own });
    for (const kid of at.kids) walk(kid);
  };
  walk(node);
  return out;
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
