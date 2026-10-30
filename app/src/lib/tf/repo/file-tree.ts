/**
 * The Files tab's tree, its pure half (#2311): what *Go to file* finds among
 * every path at the commit, which row the arrow keys go to, and what the file
 * header and the last-commit strip say.
 */
import type { TreeEntry, ViewRow } from "../../ipc/entity";
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

/** One row of the tree as drawn: a directory (folded or not), a file, or what opens nothing (a submodule). */
export interface TreeRow {
  path: string;
  kind: "dir" | "file" | "other";
  depth: number;
  /** A directory drawn unfolded. */
  open: boolean;
}

/** The rows the tree draws, in order: each listing read, an open directory's entries after it. */
export function treeRows(listings: Readonly<Record<string, readonly TreeEntry[]>>, open: ReadonlySet<string>): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (dir: string, depth: number) => {
    for (const entry of listings[dir] ?? []) {
      const path = dir === "" ? entry.name : `${dir}/${entry.name}`;
      const kind = entry.kind === "dir" ? "dir" : entry.section === null ? "other" : "file";
      const unfolded = kind === "dir" && open.has(path);
      rows.push({ path, kind, depth, open: unfolded });
      if (unfolded) walk(path, depth + 1);
    }
  };
  walk("", 0);
  return rows;
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
