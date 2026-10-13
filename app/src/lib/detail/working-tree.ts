/**
 * What the checkout panel and a branch view say about a working tree (#704,
 * ADR-0021): `entity_checkout`'s `tree`, as one line and a tone.
 *
 * Tones follow the Signal rule, colour only where something needs a hand:
 * a clean tree and commits waiting to be pushed are `muted`, uncommitted or
 * untracked paths are `amber`, and a conflict is `fail`.
 */
import type { TreeRead, WorkingTreeState } from "../ipc/entity";

export type TreeTone = "muted" | "amber" | "fail";

export interface TreeLine {
  text: string;
  tone: TreeTone;
}

/** The counts that are not zero, worst first. */
function counts(state: WorkingTreeState): string[] {
  const parts: string[] = [];
  if (state.conflicted > 0) parts.push(`${state.conflicted} conflicted`);
  if (state.changed > 0) parts.push(`${state.changed} changed`);
  if (state.untracked > 0) parts.push(`${state.untracked} untracked`);
  if (state.ahead > 0) parts.push(`${state.ahead} to push`);
  if (state.behind > 0) parts.push(`${state.behind} to pull`);
  return parts;
}

function tone(state: WorkingTreeState): TreeTone {
  if (state.conflicted > 0) return "fail";
  if (state.changed > 0 || state.untracked > 0) return "amber";
  return "muted";
}

/** The checkout panel's line under the path. */
export function treeLine(tree: TreeRead): TreeLine {
  switch (tree.status) {
    case "no_git":
      return { text: "git not found on PATH", tone: "muted" };
    case "not_a_repo":
      return { text: "not a git working tree", tone: "amber" };
    case "failed":
      return {
        text: `could not read the working tree: ${tree.reason}`,
        tone: "fail",
      };
    case "read": {
      const where =
        tree.branch !== null
          ? `on ${tree.branch}`
          : tree.head !== null
            ? `detached at ${tree.head.slice(0, 7)}`
            : "detached";
      const found = counts(tree);
      const parts = [where, ...(found.length > 0 ? found : ["clean"])];
      if (tree.branch !== null && tree.upstream === null) parts.push("no upstream");
      return { text: parts.join(" · "), tone: tone(tree) };
    }
  }
}

/**
 * The branch view's note, or `null` when the checkout is not on `branch`.
 *
 * `branch` is the mirrored branch's name. It is compared with what git
 * reported and never handed to git (ADR-0021).
 */
export function branchNote(tree: TreeRead | null, branch: string): TreeLine | null {
  if (tree?.status !== "read" || tree.branch !== branch) return null;
  return {
    text: ["Checked out locally", ...counts(tree)].join(" · "),
    tone: tone(tree),
  };
}
