/**
 * The diff's file tree and what each file starts as (#2268, #2269, spec #2234
 * stories 63, 65, 66; the layout is `mockups/code-area/diff.html`).
 *
 * - **Tree**: files under their directory, one level, each directory named in
 *   full (`src/generated`), root files first; directories and files by name.
 * - **Badge**: added `A`, deleted `D`, renamed `R` with its similarity where
 *   the source says it, binary `BIN`, an executable bit set `+x` or cleared
 *   `−x`, otherwise `M`.
 * - **Collapsed at first**: lock files and generated files (story 65).
 * - **Load diff**: a file over {@link LOAD_OVER} changed lines draws a prompt
 *   in place of its lines until the reader asks (story 66); nothing is cut.
 */
import type { DiffFile } from "../../ipc/entity";
import { keyOf, type DrawnFile } from "./diff";

/** A file over this many changed lines (added and removed) draws *Load diff* first. */
export const LOAD_OVER = 1_500;

/**
 * A grouped folder as the rail lists it (ux26): its path, and the generated files that differ under it, each named by
 * its path from the generated folder's top (`file.path`).
 */
export interface TreeGroup {
  folder: string;
  files: readonly DrawnFile[];
}

export type TreeRow =
  | { kind: "dir"; path: string; open: boolean }
  | { kind: "file"; file: DrawnFile; name: string; depth: 0 | 1; made?: true }
  /** A grouped folder's row, where its directory header would be, then its band, then its generated files (ux26). */
  | { kind: "gdir"; group: TreeGroup; open: boolean }
  | { kind: "gband"; group: TreeGroup };

/** A collapsible directory or grouped-folder header row in the changed-files rail. */
export type FolderRow = Extract<TreeRow, { kind: "dir" | "gdir" }>;

/** Stable collapse-set key distinguishing regular directories (`d:`) from grouped folders (`g:`). */
export function folderKey(row: Pick<Extract<TreeRow, { kind: "dir" }>, "kind" | "path"> | Pick<Extract<TreeRow, { kind: "gdir" }>, "kind" | "group">): string {
  return row.kind === "dir" ? `d:${row.path}` : `g:${row.group.folder}`;
}

/** Directory path of a folder or grouped-folder row (`""` for a top-level grouped folder). */
export function folderPath(row: Pick<Extract<TreeRow, { kind: "dir" }>, "kind" | "path"> | Pick<Extract<TreeRow, { kind: "gdir" }>, "kind" | "group">): string {
  return row.kind === "dir" ? row.path : row.group.folder;
}

/**
 * Keys of `target` and every nested folder header under its path in `rows` (for recursive `Alt+click`).
 * A top-level grouped folder (`""`) covers every folder header in the list.
 */
export function descendantFolderKeys(rows: readonly TreeRow[], target: FolderRow): string[] {
  const targetKey = folderKey(target);
  const base = folderPath(target);
  const prefix = base === "" ? "" : `${base}/`;
  const out: string[] = [];
  for (const row of rows) {
    if (row.kind !== "dir" && row.kind !== "gdir") continue;
    const key = folderKey(row);
    const path = folderPath(row);
    if (key === targetKey || base === "" || path.startsWith(prefix)) out.push(key);
  }
  return out;
}

/**
 * Applies `collapsed` folder keys to an expanded `rows` list in one pass: collapsed headers carry `open: false` and
 * their immediate file/band rows are omitted while root files and other folders remain intact.
 */
export function collapseTree(rows: readonly TreeRow[], collapsed: ReadonlySet<string>): TreeRow[] {
  if (collapsed.size === 0) return rows as TreeRow[];
  const out: TreeRow[] = [];
  let hidden = false;
  for (const row of rows) {
    if (row.kind === "dir" || row.kind === "gdir") {
      const shut = collapsed.has(folderKey(row));
      hidden = shut;
      out.push(shut ? { ...row, open: false } : row.open ? row : { ...row, open: true });
      continue;
    }
    if (row.kind === "file" && row.depth === 0) {
      hidden = false;
      out.push(row);
      continue;
    }
    if (!hidden) out.push(row);
  }
  return out;
}

const dirOf = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf("/")));

/**
 * The rail's rows: each directory's files under it, root files first, directories and files by name; a grouped folder
 * (`groups`) sorts with the directories by its path, as its row, its band and its generated files.
 */
export function treeOf(files: readonly DrawnFile[], groups: readonly TreeGroup[] = [], collapsed?: ReadonlySet<string>): TreeRow[] {
  const byDir = new Map<string, DrawnFile[]>();
  for (const file of files) {
    const dir = dirOf(keyOf(file));
    // Pushed, not copied: a folder of thousands of files was a copy of its list per file (the compare pass, 2026-11-13).
    const into = byDir.get(dir);
    if (into === undefined) byDir.set(dir, [file]);
    else into.push(file);
  }
  const grouped = new Map(groups.map((group) => [group.folder, group] as const));
  const rows: TreeRow[] = [];
  const sorted = (keys: Iterable<string>) => [...keys].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  // A grouped folder's key is told from a directory's of the same path by a trailing NUL, so it sorts just after it.
  const all = grouped.size === 0 ? sorted(byDir.keys()) : sorted([...byDir.keys(), ...[...grouped.keys()].map((g) => `${g}\u0000`)]);
  for (const key of all) {
    if (key.endsWith("\u0000")) {
      const group = grouped.get(key.slice(0, -1))!;
      rows.push({ kind: "gdir", group, open: true }, { kind: "gband", group });
      for (const file of group.files) rows.push({ kind: "file", file, name: file.path, depth: 1, made: true });
      continue;
    }
    const dir = key;
    if (dir !== "") rows.push({ kind: "dir", path: dir, open: true });
    const named = byDir.get(dir)!.map((file) => ({ file, name: keyOf(file).slice(dir === "" ? 0 : dir.length + 1) }));
    for (const { file, name } of named.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0)))
      rows.push({ kind: "file", file, name, depth: dir === "" ? 0 : 1 });
  }
  return collapsed !== undefined && collapsed.size > 0 ? collapseTree(rows, collapsed) : rows;
}

const EXEC = 0o111;

export function badgeOf(file: DiffFile): string {
  if (file.change === "added") return "A";
  if (file.change === "deleted") return "D";
  if (file.change === "binary") return "BIN";
  if (file.change === "renamed") return file.similarity === undefined || file.similarity === null ? "R" : `R ${file.similarity}%`;
  if (file.old_mode && file.new_mode && file.old_mode !== file.new_mode) {
    const was = parseInt(file.old_mode, 8) & EXEC;
    const is = parseInt(file.new_mode, 8) & EXEC;
    if (!was && is) return "+x";
    if (was && !is) return "−x";
  }
  return "M";
}

/** Lock files by name; generated files by name or a `generated`/`dist` directory. */
const LOCKS = /(^|\/)(package-lock\.json|npm-shrinkwrap\.json|pnpm-lock\.yaml|go\.sum|[^/]+\.lock)$/;
const GENERATED = /(^|\/)(generated|dist)\/|\.(min\.js|min\.css|map)$|\.generated\.|_pb2\.py$|\.pb\.go$/;

export function isLock(path: string): boolean {
  return LOCKS.test(path);
}

export function isGenerated(path: string): boolean {
  return GENERATED.test(path);
}

/** Each path's answer, kept: a list of 10,000 files asks every one of them each time it is drawn or filled. */
const atFirst = new Map<string, boolean>();

export function collapsedAtFirst(path: string): boolean {
  let shut = atFirst.get(path);
  if (shut === undefined) {
    if (atFirst.size >= 50_000) atFirst.clear();
    shut = isLock(path) || isGenerated(path);
    atFirst.set(path, shut);
  }
  return shut;
}

export function needsLoad(file: DiffFile): boolean {
  return file.additions + file.deletions > LOAD_OVER;
}
