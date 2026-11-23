/**
 * The Files tab's tree, its pure half (#2311): what *Go to file* finds among
 * every path at the commit, which row the arrow keys go to, and what the file
 * header and the last-commit strip say.
 */
import type { MadeFile, MadeLeftOut, MadeTree } from "../../ipc/app";
import type { LocalEntry, TreeEntry, ViewRow } from "../../ipc/entity";
import { sized } from "../../render/pipeline";

export { initials } from "../tickets/tickets";

/** How many paths *Go to file* draws at most. */
export const GO_TO_FILE_SHOWN = 50;

const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1);

/**
 * The paths holding every word typed (case aside), best first: a file whose
 * name starts with the last word, then one whose name holds it, then the
 * rest; a shorter path first within each. At most `limit`.
 */
export function goToFile(paths: readonly string[], typed: string, limit = GO_TO_FILE_SHOWN): string[] {
  const words = typed.toLowerCase().split(/\s+/).filter(Boolean);
  const last = words.at(-1);
  if (last === undefined) return [];
  const tier = (path: string) => {
    const name = baseName(path).toLowerCase();
    return name.startsWith(last) ? 0 : name.includes(last) ? 1 : 2;
  };
  return paths
    .filter((path) => {
      const lower = path.toLowerCase();
      return words.every((word) => lower.includes(word));
    })
    .map((path) => ({ path, tier: tier(path) }))
    .sort((a, b) => a.tier - b.tier || a.path.length - b.path.length || a.path.localeCompare(b.path))
    .slice(0, limit)
    .map(({ path }) => path);
}

/**
 * One directory of a checkout's tree as the listing the tab draws (the big-folder pass, 2026-11-13): each entry by its
 * name, opened by the section `sectionOf` makes for it (none for a submodule), directories first and then by name,
 * case aside, as the source sorts them.
 */
export function checkoutListing(
  entries: readonly LocalEntry[],
  sectionOf: (kind: "dir" | "file", path: string) => string,
): TreeEntry[] {
  return entries
    .map((entry) => ({
      name: baseName(entry.path),
      kind: entry.kind,
      size: entry.size,
      section: entry.kind === "submodule" ? null : sectionOf(entry.kind === "dir" ? "dir" : "file", entry.path),
    }))
    .sort(
      (a, b) =>
        Number(b.kind === "dir") - Number(a.kind === "dir") ||
        a.name.toLowerCase().localeCompare(b.name.toLowerCase()) ||
        (a.name < b.name ? -1 : a.name > b.name ? 1 : 0),
    );
}

/**
 * One row of the tree as drawn: a directory (folded or not), a file, what opens nothing (a submodule), or the band under
 * a grafted folder's row (ADR-0046).
 */
export interface TreeRow {
  path: string;
  /** The entry's name, as its row reads. */
  name: string;
  kind: "dir" | "file" | "other" | "band";
  depth: number;
  /** A directory drawn unfolded. */
  open: boolean;
  /** The section that opens it, as its listing named it; `null` for what opens nothing. */
  section: string | null;
  /** A row of a generated folder (ADR-0046): the depth of the grafted folder's row, `-1` for the top folder. */
  made?: number;
}

/** What follows a grafted folder's path in its band's row path: no name holds it. */
export const BAND = "\u0000band";

/**
 * The rows the tree draws, in order: each listing read, an open directory's entries after it. The tree draws this
 * flat list through a windowed list (the big-folder pass, 2026-11-13), so a folder of ten thousand entries mounts the
 * rows in view, not ten thousand.
 *
 * `bands` names the grafted folders (ADR-0046): an open one's row is followed by its band, and every row under it is
 * marked `made` with the folder row's depth. The top folder grafted puts its band first.
 */
export function treeRows(listings: Readonly<Record<string, readonly TreeEntry[]>>, open: ReadonlySet<string>, bands?: ReadonlySet<string>): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (dir: string, depth: number, made: number | undefined) => {
    for (const entry of listings[dir] ?? []) {
      const path = dir === "" ? entry.name : `${dir}/${entry.name}`;
      const kind = entry.kind === "dir" ? "dir" : entry.section === null ? "other" : "file";
      const unfolded = kind === "dir" && open.has(path);
      const row: TreeRow = { path, name: entry.name, kind, depth, open: unfolded, section: entry.section };
      if (made !== undefined) row.made = made;
      rows.push(row);
      if (!unfolded) continue;
      const grafted = made === undefined && bands !== undefined && bands.has(path);
      if (grafted) rows.push({ path: `${path}${BAND}`, name: "", kind: "band", depth: depth + 1, open: false, section: null });
      walk(path, depth + 1, grafted ? depth : made);
    }
  };
  if (bands?.has("")) {
    rows.push({ path: BAND, name: "", kind: "band", depth: 0, open: false, section: null });
    walk("", 0, -1);
  } else walk("", 0, undefined);
  return rows;
}

/** The grafted folder a band's row path names. */
export const bandFolder = (path: string) => path.slice(0, path.length - BAND.length);

// ---- A generated folder in the tree (ADR-0046) ----

/** A generated folder's tree, grouped by folder and sorted a folder at a time, the first time each is asked for. */
export interface MadeIndex {
  token: string;
  /** Where it is kept on disk. */
  dir: string;
  files: readonly MadeFile[];
  bytes: number;
  /** What the run wrote that was not kept: the first fifty, and how many in all. */
  leftOut: readonly MadeLeftOut[];
  leftOutTotal: number;
  /** Every folder it holds, from its top (`""` the top itself). */
  dirs: readonly string[];
  /** A folder's entries as a listing (folders first, then by name, case aside), its paths from the generated folder's top. */
  listingOf(dir: string): readonly TreeEntry[] | undefined;
  /** Whether it holds a file at `path`, from its top. */
  has(path: string): boolean;
}

/** The entries' order a checkout's listing has (`checkoutListing`): folders first, then by name, case aside. */
function byListing(a: TreeEntry, b: TreeEntry): number {
  return (
    Number(b.kind === "dir") - Number(a.kind === "dir") ||
    a.name.toLowerCase().localeCompare(b.name.toLowerCase()) ||
    (a.name < b.name ? -1 : a.name > b.name ? 1 : 0)
  );
}

/**
 * `tree` (the manifest `folder_render_tree` answers) as the tree's index, in one pass over its files. Sorting twenty
 * thousand names up front would cost the press: a folder's entries are sorted the first time it is asked for, then kept.
 * `compare` counts in tests.
 */
export function madeIndex(
  tree: MadeTree,
  token: string,
  sectionOf: (kind: "dir" | "file", path: string) => string,
  compare: (a: TreeEntry, b: TreeEntry) => number = byListing,
): MadeIndex {
  const subdirs = new Map<string, Set<string>>([["", new Set()]]);
  const filesIn = new Map<string, MadeFile[]>();
  const held = new Set<string>();
  for (const file of tree.files) {
    held.add(file.path);
    const cut = file.path.lastIndexOf("/");
    const dir = cut === -1 ? "" : file.path.slice(0, cut);
    const list = filesIn.get(dir);
    if (list === undefined) filesIn.set(dir, [file]);
    else list.push(file);
    // Each folder on the way is made and named in its parent's names, up to the first one held before this file. Its
    // own parent names that one already. `held` is read before a folder is made, never after. The walk used to make a
    // parent so it could name the child in it, then found that parent held and stopped there. A folder two levels under
    // the last one held (`src/main/java/demo`, with folders alone above it) was named nowhere, so the tree drew neither
    // it nor its files, though *Go to file* found them (ux31).
    let at = dir;
    let child: string | null = null;
    for (;;) {
      const held = subdirs.get(at);
      const names = held ?? new Set<string>();
      if (held === undefined) subdirs.set(at, names);
      if (child !== null) names.add(child);
      if (held !== undefined) break;
      const cut = at.lastIndexOf("/");
      child = at.slice(cut + 1);
      at = cut === -1 ? "" : at.slice(0, cut);
    }
  }
  const sorted = new Map<string, readonly TreeEntry[]>();
  const join = (dir: string, name: string) => (dir === "" ? name : `${dir}/${name}`);
  return {
    token,
    dir: tree.dir,
    files: tree.files,
    bytes: tree.bytes,
    leftOut: tree.left_out,
    leftOutTotal: tree.left_out_total,
    dirs: [...subdirs.keys()],
    has: (path) => held.has(path),
    listingOf(dir) {
      const kept = sorted.get(dir);
      if (kept !== undefined) return kept;
      const names = subdirs.get(dir);
      if (names === undefined) return undefined;
      const entries: TreeEntry[] = [
        ...[...names].map((name): TreeEntry => ({ name, kind: "dir", size: null, section: sectionOf("dir", join(dir, name)) })),
        ...(filesIn.get(dir) ?? []).map((file): TreeEntry => {
          const name = file.path.slice(file.path.lastIndexOf("/") + 1);
          return { name, kind: "file", size: file.size, section: sectionOf("file", file.path) };
        }),
      ].sort(compare);
      sorted.set(dir, entries);
      return entries;
    },
  };
}

/** Whether `path` is the folder `folder` or under it (`""` the top holds every path). */
export const underFolder = (path: string, folder: string) => folder === "" || path === folder || path.startsWith(`${folder}/`);

/** A generated folder in the tree, as `graftedListings` and `graftedPaths` take it. */
export interface Grafted {
  folder: string;
  dirs: readonly string[];
  listingOf(dir: string): readonly TreeEntry[] | undefined;
}

/**
 * The listings the tree draws with grafts on (ADR-0046): the real listings less every key at or under a grafted
 * folder, plus each graft's folders keyed by their tree paths (`folder`, `folder/sub`), each read from the graft's index
 * when first asked for. The real listings are never written to, so switching back draws from what is already read.
 */
export function graftedListings(real: Readonly<Record<string, readonly TreeEntry[]>>, grafts: readonly Grafted[]): Record<string, readonly TreeEntry[]> {
  const out: Record<string, readonly TreeEntry[]> = {};
  for (const [key, entries] of Object.entries(real)) if (!grafts.some((g) => underFolder(key, g.folder))) out[key] = entries;
  for (const graft of grafts) {
    for (const dir of graft.dirs) {
      const key = graft.folder === "" ? dir : dir === "" ? graft.folder : `${graft.folder}/${dir}`;
      Object.defineProperty(out, key, { get: () => graft.listingOf(dir), enumerable: true, configurable: true });
    }
  }
  return out;
}

/** *Go to file* with grafts on (ADR-0046): the real paths less those under a graft, plus each graft's as tree paths. */
export function graftedPaths(real: readonly string[], grafts: readonly { folder: string; files: readonly { path: string }[] }[]): string[] {
  const kept = real.filter((path) => !grafts.some((g) => underFolder(path, g.folder)));
  for (const graft of grafts) for (const file of graft.files) kept.push(graft.folder === "" ? file.path : `${graft.folder}/${file.path}`);
  return kept;
}

/** What an arrow key does in the tree: focus a row, or fold or unfold a directory. */
export type TreeMove = { focus: string } | { fold: string } | { unfold: string };

/**
 * The tree's arrow keys from the row at `at` (`null`: no row has focus), as a
 * tree view's: `↑`/`↓` the row before or after, `→` unfolds a folded
 * directory and steps into an open one, `←` folds an open one and steps out
 * of anything else to its directory. `null` for a key that does nothing here.
 */
export function treeKey(rows: readonly TreeRow[], at: string | null, key: string): TreeMove | null {
  const index = at === null ? -1 : rows.findIndex((row) => row.path === at);
  const here = rows[index];
  switch (key) {
    case "ArrowDown":
      return rows.length === 0 ? null : { focus: rows[Math.min(rows.length - 1, index + 1)]!.path };
    case "ArrowUp":
      return rows.length === 0 ? null : { focus: rows[Math.max(0, index - 1)]!.path };
    case "ArrowRight": {
      if (here?.kind !== "dir") return null;
      if (!here.open) return { unfold: here.path };
      const first = rows[index + 1];
      return first !== undefined && first.depth > here.depth ? { focus: first.path } : null;
    }
    case "ArrowLeft": {
      if (here === undefined) return null;
      if (here.kind === "dir" && here.open) return { fold: here.path };
      // A band steps out to its folder's row; the top folder's band has none.
      if (here.kind === "band") return here.path === BAND ? null : { focus: bandFolder(here.path) };
      const up = here.path.includes("/") ? here.path.slice(0, here.path.lastIndexOf("/")) : null;
      return up === null ? null : { focus: up };
    }
    default:
      return null;
  }
}

/** A binary file's kind, by its extension. */
const KINDS: Record<string, string> = {
  pdf: "PDF",
  png: "PNG image",
  jpg: "JPEG image",
  jpeg: "JPEG image",
  gif: "GIF image",
  webp: "WebP image",
  ico: "icon",
  zip: "zip archive",
  gz: "gzip",
  zst: "zstd",
  wasm: "WebAssembly",
};

/**
 * The file header's meta (the mockup's `1.4 KB · 38 lines · at 4c1d0aa`): the
 * file's size, its lines (a text file) or its kind (a binary one), and the
 * commit it is read at, short, where the source names one.
 */
export function fileMeta(file: { path: string; size: number | bigint; binary: boolean; commit?: string | null }, lines = 0): string {
  const ext = baseName(file.path).toLowerCase().split(".").at(-1) ?? "";
  const kind = file.binary ? (KINDS[ext] ?? "binary") : `${lines} ${lines === 1 ? "line" : "lines"}`;
  const at = file.commit ? ` · at ${file.commit.slice(0, 7)}` : "";
  return `${sized(Number(file.size))} · ${kind}${at}`;
}

/**
 * A commit row's second reading: the source's `author · age`, or *You · age*
 * on the reader's own commit (F-221), as the page's other lines call them.
 * `mine` is `code_facts`' word, so a commit the mirror does not hold keeps its
 * author's name, and a line that names no author stays as the source wrote it.
 */
export function commitLine(row: ViewRow, mine: boolean): string {
  const meta = row.meta ?? "";
  if (!mine || !row.author) return meta;
  const { when } = authorOf(row);
  return when !== null && when !== row.author ? `You · ${when}` : "You";
}

/**
 * A commit row's author, drawn apart from when it was made: `author` where
 * the source names one, and `when`, the rest of its meta (`mara · 3 hours
 * ago` without `mara · `). With no author, the meta is drawn whole.
 */
export function authorOf(row: ViewRow): { author: string | null; when: string | null } {
  const author = row.author ?? null;
  if (author === null) return { author: null, when: row.meta };
  const lead = `${author} · `;
  return { author, when: row.meta?.startsWith(lead) ? row.meta.slice(lead.length) : row.meta };
}

// ---- The working tree (2026-11-21, `docs/decisions/2026-11-21-the-working-tree-is-a-choice-in-the-branch-box.md`) ----

/** The order `checkoutListing` draws a folder in: folders first, then by name with case aside, then by bytes. */
const listingOrder = (a: TreeEntry, b: TreeEntry) =>
  Number(b.kind === "dir") - Number(a.kind === "dir") ||
  a.name.toLowerCase().localeCompare(b.name.toLowerCase()) ||
  (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);

/**
 * The checkout's `HEAD` as the tab has read it (`listings`, a folder at a time), with the working tree's changes laid
 * over it: a modified file is opened by `diskSection` (read from disk), an added or untracked one gets a row (and the
 * folders above it that `HEAD` lacks get theirs), and a deleted one keeps its row, so nothing under it moves. Only the
 * folders read so far, and the new folders an added file makes, are listed; a folder not read yet stays unread.
 */
export function diskListings(
  listings: Readonly<Record<string, readonly TreeEntry[]>>,
  marks: ReadonlyMap<string, "M" | "A" | "D">,
  diskSection: (path: string) => string,
  dirSection: (path: string) => string,
): Record<string, TreeEntry[]> {
  const out: Record<string, TreeEntry[]> = {};
  for (const [dir, entries] of Object.entries(listings)) out[dir] = [...entries];
  if (marks.size === 0) return out;
  const parent = (path: string) => (path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "");
  const name = (path: string) => path.slice(path.lastIndexOf("/") + 1);
  const touched = new Set<string>();
  const add = (dir: string, entry: TreeEntry) => {
    const into = out[dir];
    if (into === undefined) return;
    if (into.some((held) => held.name === entry.name)) return;
    into.push(entry);
    touched.add(dir);
  };
  for (const [path, mark] of marks) {
    if (mark === "M") {
      const into = out[parent(path)];
      const at = into?.findIndex((entry) => entry.name === name(path) && entry.kind !== "dir") ?? -1;
      if (into !== undefined && at >= 0) into[at] = { ...into[at]!, section: diskSection(path) };
      // A file git names changed that the folder read does not hold still gets its row: the list is what is on disk.
      else add(parent(path), { name: name(path), kind: "file", size: null, section: diskSection(path) });
      continue;
    }
    if (mark !== "A") continue;
    // The folders above it that the tab has not listed: a new folder of the working tree is listed by its files alone.
    const chain: string[] = [];
    for (let dir = parent(path); dir !== ""; dir = parent(dir)) chain.unshift(dir);
    for (const dir of chain) {
      if (out[dir] === undefined && out[parent(dir)] !== undefined && !(listings[parent(dir)] ?? []).some((entry) => entry.name === name(dir))) out[dir] = [];
      add(parent(dir), { name: name(dir), kind: "dir", size: null, section: dirSection(dir) });
    }
    add(parent(path), { name: name(path), kind: "file", size: null, section: diskSection(path) });
  }
  for (const dir of touched) out[dir]!.sort(listingOrder);
  return out;
}
