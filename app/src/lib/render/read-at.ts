/**
 * Where a render on the Files tab reads (ux29; Björn: "In the folder rendering, when I have the working tree selected,
 * it tries to render the folder from the head and not from my local changes"). The one place that decides it: a folder
 * renderer, *Run again*, the tree's marks, a file renderer, *Render as* and a pipeline on a file all take it.
 *
 * - **On the working tree** every render reads the working tree on disk (ADR-0049's `~working-tree`): a folder is the
 *   copy Compare's working side makes, uncommitted and untracked files included and ignored ones never
 *   (`folder_render::resolve::working_folder`), keyed by the changed files' bytes, so a change on disk is a new state;
 *   a file is read from the checkout as it is on disk. A folder runs in the working tree's lane (`cmp:~working-tree`),
 *   the one Compare's working side runs in, so a working-tree drawing is never the Files tab's last drawing of a commit,
 *   and both read the one run.
 * - **On a branch** it reads the clone's commit, `null` for the mirror's tip, as before.
 */
import type { WorkingMark } from "../tf/repo/repo";
import { WORKING_TREE } from "../tf/diff/address";
import { laneOf } from "./folder-compare.svelte";

/** Where a folder render of the Files tab is asked. */
export interface RenderFrom {
  /** The entity asked: the repository on the working tree (a checkout is the repository's), else the tab's. */
  entityId: string;
  /** `~working-tree`, the clone's commit, or `null` for the mirror's tip. */
  commit: string | null;
  /** The section the source lists every path in, for a read of the source; never on the working tree. */
  paths: string | null;
  /** The working tree's lane; absent for the Files tab's own. */
  lane?: string;
  /** Moves when the working tree changes on disk (git's list's stamp), so the render asks again; `""` at a commit. */
  stamp: string;
}

/** What the Files tab knows of the working tree: `null` on a branch. */
export interface WorkingDisk {
  /** The checkout's `HEAD`. */
  head: string;
  stamp: string;
  marks: ReadonlyMap<string, WorkingMark>;
}

/** The working tree's lane (`folder-compare`'s `laneOf`): Files and Compare's working side share it. */
export const WORKING_LANE = laneOf(WORKING_TREE);

/** Where a folder render reads: the working tree where it is chosen, else the clone's commit (`clone`) or the tip. */
export function renderFrom(at: string, repoId: string, disk: WorkingDisk | null, clone: string | null, paths: string | null): RenderFrom {
  if (disk !== null) return { entityId: repoId, commit: WORKING_TREE, paths: null, lane: WORKING_LANE, stamp: disk.stamp };
  return { entityId: at, commit: clone, paths, stamp: "" };
}

/**
 * Where a file's bytes are read for a renderer, a pipeline, a JAR, a Markdown image or the whole text of a cut file. On
 * the working tree: a file git lists changed or added is read as it is on disk; any other is `HEAD`'s blob, as the
 * working folder's copy takes it (a deleted one has nothing on disk). On a branch: the commit the file was read at.
 */
export function fileReadAt(disk: WorkingDisk | null, path: string, commit: string | null): string | null {
  if (disk === null) return commit;
  const mark = disk.marks.get(path);
  return mark === "M" || mark === "A" ? WORKING_TREE : (commit ?? disk.head);
}

/**
 * The key a file's byte read is kept by: `[entity, path, where]`, and on the working tree git's list's stamp too, so a
 * file changed again on disk is read again.
 */
export function bytesKey(at: string, path: string, where: string | null, disk: WorkingDisk | null): string {
  return JSON.stringify(where === WORKING_TREE && disk !== null ? [at, path, where, disk.stamp] : [at, path, where]);
}

/** The files git lists changed, added or deleted under `folder` (`""` the top), for the made bar's words. */
export function changedUnder(marks: ReadonlyMap<string, WorkingMark>, folder: string): number {
  let count = 0;
  for (const path of marks.keys()) if (folder === "" || path.startsWith(`${folder}/`)) count += 1;
  return count;
}
