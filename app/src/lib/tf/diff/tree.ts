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
  | { kind: "dir"; path: string }
  | { kind: "file"; file: DrawnFile; name: string; depth: 0 | 1; made?: true }
  /** A grouped folder's row, where its directory header would be, then its band, then its generated files (ux26). */
  | { kind: "gdir"; group: TreeGroup }
  | { kind: "gband"; group: TreeGroup };

const dirOf = (path: string) => path.slice(0, Math.max(0, path.lastIndexOf("/")));

/**
 * The rail's rows: each directory's files under it, root files first, directories and files by name; a grouped folder
 * (`groups`) sorts with the directories by its path, as its row, its band and its generated files.
 */
export function treeOf(files: readonly DrawnFile[], groups: readonly TreeGroup[] = []): TreeRow[] {
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
      rows.push({ kind: "gdir", group }, { kind: "gband", group });
      for (const file of group.files) rows.push({ kind: "file", file, name: file.path, depth: 1, made: true });
      continue;
    }
    const dir = key;
    if (dir !== "") rows.push({ kind: "dir", path: dir });
    const named = byDir.get(dir)!.map((file) => ({ file, name: keyOf(file).slice(dir === "" ? 0 : dir.length + 1) }));
    for (const { file, name } of named.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0)))
      rows.push({ kind: "file", file, name, depth: dir === "" ? 0 : 1 });
  }
  return rows;
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
