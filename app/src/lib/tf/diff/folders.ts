/**
 * A folder renderer in a comparison (ux26, `docs/decisions/2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-
 * working-tree.md` rulings 7 and 8; the decision list's section 8), the pure half: which changed folders a folder
 * renderer takes, which one a changed file is grouped under, and the list of real files and groups the view draws.
 *
 * - A folder is **taken** when a saved renderer takes it on either side (both sides' marks, merged by the caller).
 * - Walking down a changed file's path, the **outermost** taken folder is the one grouped; nothing inside it is offered.
 * - A renderer with no glob in either list takes any folder, so it is **never applied by the band's toggle** (it would
 *   turn the repository's top into a run); it can still be chosen for a folder by hand, at the folder's glyph.
 * - A folder with no changed file under it is never grouped: the comparison's own file list is the proof that its two
 *   sides are the same.
 */
import type { FolderRenderer } from "../../ipc/app";
import { renderersFor } from "../../render/folder";
import type { DrawnFile } from "./diff";

/** A grouped folder: its renderer, and the changed real files it stands in for while grouped. */
export interface FolderGroup {
  folder: string;
  renderer: FolderRenderer;
  real: DrawnFile[];
}

/** What the file list holds: a real file, or a grouped folder in its files' place. */
export type Listed = { kind: "file"; file: DrawnFile } | { kind: "group"; group: FolderGroup };

/** The folders above `path`, from the top (`""`) down, the file's own folder last; the file itself is not one. */
export function foldersOf(path: string): string[] {
  const out = [""];
  for (let at = path.indexOf("/"); at >= 0; at = path.indexOf("/", at + 1)) out.push(path.slice(0, at));
  return out;
}

/** The outermost folder above `path` that `taken` holds, or `null`. */
export function takenFolder(path: string, taken: ReadonlyMap<string, unknown>): string | null {
  for (const folder of foldersOf(path)) if (taken.has(folder)) return folder;
  return null;
}

/** Whether a renderer says which folders it takes (a glob in either list): only those are applied by the toggle. */
export const hasGlob = (r: FolderRenderer) => r.holds.length > 0 || r.folders.length > 0;

/**
 * The folder each renderer takes, among the folders above `paths`, that the band's toggle groups: each distinct folder
 * asked once. The renderer is the folder's pick where it still takes the folder, else the first saved renderer with a
 * glob that takes it.
 */
export function takenFolders(
  paths: readonly string[],
  saved: readonly FolderRenderer[],
  marks: ReadonlyMap<string, ReadonlySet<string>>,
  pickOf: (folder: string) => string | null,
): Map<string, FolderRenderer> {
  const taken = new Map<string, FolderRenderer>();
  const globbed = saved.filter(hasGlob);
  if (globbed.length === 0) return taken;
  const asked = new Set<string>();
  for (const path of paths)
    for (const folder of foldersOf(path)) {
      if (asked.has(folder)) continue;
      asked.add(folder);
      const by = rendererOf(folder, globbed, saved, marks, pickOf(folder));
      if (by !== undefined) taken.set(folder, by);
    }
  return taken;
}

/** The renderer `folder` gets among `taking`'s: its pick where any saved renderer by that id takes it, else the first. */
function rendererOf(
  folder: string,
  taking: readonly FolderRenderer[],
  saved: readonly FolderRenderer[],
  marks: ReadonlyMap<string, ReadonlySet<string>>,
  pick: string | null,
): FolderRenderer | undefined {
  const first = renderersFor(folder, taking, marks)[0];
  if (first === undefined) return undefined;
  if (pick === null) return first;
  return renderersFor(folder, saved, marks).find((r) => r.id === pick) ?? first;
}

/**
 * The folder a directory row's glyph groups, and the renderers that take it: the outermost folder above or at `dir`
 * the toggle would group, else `dir` itself where any saved renderer takes it (one with no glob too: it is chosen by
 * hand here). `null`: no glyph.
 */
export function glyphFolder(
  dir: string,
  taken: ReadonlyMap<string, FolderRenderer>,
  saved: readonly FolderRenderer[],
  marks: ReadonlyMap<string, ReadonlySet<string>>,
): { folder: string; taking: FolderRenderer[] } | null {
  for (const folder of [...foldersOf(dir), dir]) {
    if (taken.has(folder)) return { folder, taking: renderersFor(folder, saved, marks) };
  }
  const taking = renderersFor(dir, saved, marks);
  return taking.length === 0 ? null : { folder: dir, taking };
}

/**
 * The comparison's files as the view lists them: the files under a grouped folder leave the list, and the folder's
 * group stands in the place of the first of them. A folder is grouped where the toggle is `on` and `taken` holds it, or
 * where the person chose it at its glyph (`chosen`, with the renderer chosen); never one in `real` (*Real files*).
 */
export function groupFolders(
  files: readonly DrawnFile[],
  how: { taken: ReadonlyMap<string, FolderRenderer>; chosen: ReadonlyMap<string, FolderRenderer>; real: ReadonlySet<string>; on: boolean },
): Listed[] {
  const active = new Map<string, FolderRenderer>();
  if (how.on) for (const [folder, by] of how.taken) if (!how.real.has(folder)) active.set(folder, by);
  for (const [folder, by] of how.chosen) if (!how.real.has(folder)) active.set(folder, by);
  if (active.size === 0) return files.map((file) => ({ kind: "file", file }));
  const out: Listed[] = [];
  const groups = new Map<string, FolderGroup>();
  for (const file of files) {
    const folder = takenFolder(file.path, active);
    if (folder === null) {
      out.push({ kind: "file", file });
      continue;
    }
    const had = groups.get(folder);
    if (had !== undefined) {
      had.real.push(file);
      continue;
    }
    const group: FolderGroup = { folder, renderer: active.get(folder)!, real: [file] };
    groups.set(folder, group);
    out.push({ kind: "group", group });
  }
  return out;
}
