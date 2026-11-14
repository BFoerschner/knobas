/**
 * The checkout's reads for the Files tab's tests (the big-folder pass, 2026-11-13): `localDir` and `localPaths`
 * answered from a whole tree a test holds, as the backend answers them from the tree it keeps. `read` may answer late
 * (a promise) or not at all (`null`, no checkout's copy); every folder read waits for it, as the backend's would.
 */
import type { LocalDir, LocalPaths, LocalTree } from "../../ipc/entity";

const parentOf = (full: string) => (full.includes("/") ? full.slice(0, full.lastIndexOf("/")) : "");

export interface CheckoutPorts {
  localDir: (at: string, commit: string | null, dir: string) => Promise<LocalDir | null>;
  localPaths: (at: string, commit: string) => Promise<LocalPaths | null>;
  /** Every folder read, `commit ?? "tip"` and the folder, in order. */
  dirReads: string[];
}

export function checkoutPorts(read: () => LocalTree | null | Promise<LocalTree | null>): CheckoutPorts {
  const dirReads: string[] = [];
  return {
    dirReads,
    async localDir(_at, commit, dir) {
      dirReads.push(`${commit ?? "tip"}:${dir}`);
      const tree = await read();
      if (tree === null || (commit !== null && commit !== tree.commit)) return null;
      if (dir !== "" && !tree.entries.some((entry) => entry.path === dir && entry.kind === "dir")) return null;
      const entries = tree.entries.filter((entry) => parentOf(entry.path) === dir);
      return { commit: tree.commit, as_of: tree.as_of, dir, entries, complete: true };
    },
    async localPaths(_at, commit) {
      const tree = await read();
      if (tree === null || commit !== tree.commit) return null;
      const entries = tree.entries.filter((entry) => entry.kind === "file" || entry.kind === "symlink");
      return { commit: tree.commit, entries, truncated: tree.truncated };
    },
  };
}
