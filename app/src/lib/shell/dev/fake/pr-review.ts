/**
 * `?fake-ipc`: *Viewed* on a pull request's files (#842): `pr_viewed_files`
 * and `set_pr_file_viewed`, held for the session by pull request. The fake's
 * heads never move, so nothing resets them; PR #142's design note starts
 * viewed, so *Files changed* draws one collapsed file and its tick.
 */
import type { Handlers } from "../fake-tauri";

export function handlers(): Handlers {
  const viewed = new Map<string, Set<string>>([["mock:payout-service#142", new Set(["docs/backoff.md"])]]);
  const sorted = (id: string) => [...(viewed.get(id) ?? [])].sort();
  return {
    // The fake keeps by path alone: its diffs never change (the app keeps by blob, #2276).
    pr_viewed_files: (args) => sorted(String(args["prId"] ?? "")),
    set_pr_file_viewed: (args) => {
      const id = String(args["prId"] ?? "");
      const files = viewed.get(id) ?? new Set<string>();
      const path = String(args["path"] ?? "");
      if (args["viewed"] === true) files.add(path);
      else files.delete(path);
      viewed.set(id, files);
      return sorted(id);
    },
  };
}
