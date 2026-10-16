/**
 * The repository page's pure half (#838, R-8.3 to R-8.5, R-8.7, R-8.10):
 * what an address names, which branches the dropdown offers, which paths the
 * local checkout changed, and what the header and the lists say.
 *
 * The page reads a repository through the source's own view (`entity_view`,
 * ADR-0020): its branches, its tree and files at a branch, its commits and a
 * commit's diff, all read live, so a repository nobody cloned reads the same
 * as one on this disk. Section ids are the source's and never taken apart.
 */
import type { CheckoutView, DiffRead, EntityRow, TreeEntry, ViewRow, ViewSection } from "../../ipc/entity";
import { checkoutLine, prNumber } from "../code/code";
import { matchesAll } from "../filter/filter";

/**
 * The page's tabs, in prototype-6's order. A later ticket that builds one
 * replaces its `NOT_BUILT` entry in `TfRepo.svelte` with an arm drawing its
 * component; the tab row is this list.
 */
export const REPO_TABS = [
  { id: "files", label: "Files" },
  { id: "commits", label: "Commits" },
  { id: "branches", label: "Branches" },
  { id: "prs", label: "Pull requests" },
  { id: "local", label: "Local changes" },
] as const;

/** A tab of the row. */
export type RepoTabId = (typeof REPO_TABS)[number]["id"];

/**
 * What an address can open: a tab, or one of the two places that light a tab
 * without being it (an edit lights Files, the compare page Pull requests).
 */
export type RepoTab = RepoTabId | "edit" | "compare";

/** Which tab of the row an address lights. */
export function litTab(tab: RepoTab): RepoTabId {
  if (tab === "edit") return "files";
  if (tab === "compare") return "prs";
  return tab;
}

/** What `#/repo/<repo entity id>/...` names. */
export interface RepoRoute {
  /** The repository's entity id, or `null` for a bare `#/repo`. */
  repoId: string | null;
  tab: RepoTab;
  /** Files and edit: the file's path in the tree. */
  path: string | null;
  /** Commits: the commit's entity id, whose diff opens. */
  commit: string | null;
  /** Compare: the branch proposed (#837's `compare/<branch>`). */
  branch: string | null;
}

const TABS = new Set<string>([...REPO_TABS.map((tab) => tab.id), "edit", "compare"]);

/**
 * Read what follows `repo` in the address. The route has decoded every
 * segment, so a path's slashes split it and are joined back here; the entity
 * ids were encoded whole and arrive as one segment each.
 */
export function readRepoRoute(rest: readonly string[]): RepoRoute {
  const repoId = rest[0] || null;
  const tab = (TABS.has(rest[1] ?? "") ? rest[1] : "files") as RepoTab;
  const tail = rest.slice(2).join("/") || null;
  return {
    repoId,
    tab,
    path: tab === "files" || tab === "edit" ? tail : null,
    commit: tab === "commits" ? (rest[2] ?? null) : null,
    branch: tab === "compare" ? tail : null,
  };
}

/** The address of the repository page, a tab of it, or a place inside a tab. */
export function repoHash(repoId: string, tab?: RepoTab, ...tail: string[]): string {
  const parts = [`#/repo/${encodeURIComponent(repoId)}`];
  if (tab !== undefined) parts.push(tab, ...tail);
  return parts.join("/");
}

/** The Files tab with `path` open; each segment encoded, the slashes kept. */
export function fileHash(repoId: string, path: string): string {
  return repoHash(repoId, "files", ...path.split("/").map(encodeURIComponent));
}

/** A commit's diff under the Commits tab. */
export function commitHash(repoId: string, commitId: string): string {
  return repoHash(repoId, "commits", encodeURIComponent(commitId));
}

/** One branch the dropdown offers. */
export interface BranchOption {
  name: string;
  /**
   * The branch's entity id, which its files and commits are read at; `null`
   * only for the default branch where the mirror does not hold it, which is
   * read at the repository instead.
   */
  entityId: string | null;
  isDefault: boolean;
}

/**
 * The branches the dropdown offers, from the view's *Branches* section, in
 * its order (the default branch first). Only a branch the mirror holds has an
 * id to read at; the view cleared the others' (ADR-0020 rule 7), and the note
 * a cut list ends on has none either. The default branch is offered either
 * way: it is read at the repository.
 */
export function branchOptions(section: ViewSection | null): BranchOption[] {
  const options: BranchOption[] = [];
  for (const block of section?.blocks ?? []) {
    if (block.type !== "entities") continue;
    for (const row of block.rows) {
      const isDefault = row.chip === "default";
      if (row.entity_id === null && !isDefault) continue;
      options.push({ name: row.title, entityId: row.entity_id, isDefault });
    }
  }
  return options;
}

/** The view's rows of its first entities block. */
export function viewRows(section: ViewSection | null | undefined): ViewRow[] {
  for (const block of section?.blocks ?? []) {
    if (block.type === "entities") return block.rows;
  }
  return [];
}

/** The rows that hold every word typed, across sha, title and meta. */
export function visibleViewRows(rows: readonly ViewRow[], text: string): ViewRow[] {
  return rows.filter((row) => matchesAll(`${row.chip ?? ""} ${row.title} ${row.meta ?? ""}`, text));
}

/**
 * The paths the local checkout changed: unstaged, staged (a rename under its
 * new path) and untracked files. An untracked directory (`scratch/`) marks no
 * file, since the tree does not list what is inside it as changed.
 */
export function changedPaths(read: DiffRead | null): Set<string> {
  const paths = new Set<string>();
  if (read === null || read.status !== "read") return paths;
  for (const file of [...read.unstaged.files, ...read.staged.files]) paths.add(file.path);
  for (const path of read.untracked) if (!path.endsWith("/")) paths.add(path);
  return paths;
}

/** A file's lines, without the empty one a final newline would add. */
export function fileLines(text: string): string[] {
  const lines = text.split("\n");
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}

/** The directories above a path, outermost first: `src`, `src/sepa`. */
export function ancestors(path: string): string[] {
  const parts = path.split("/").slice(0, -1);
  return parts.map((_, at) => parts.slice(0, at + 1).join("/"));
}

/**
 * The file that opens when the address names none: the top directory's
 * README, a Markdown one first, as Gitea's own page prefers it.
 */
export function pickDefaultFile(entries: readonly TreeEntry[]): string | null {
  let best: string | null = null;
  for (const entry of entries) {
    if (entry.kind !== "file") continue;
    const lower = entry.name.toLowerCase();
    if (!/^readme(\.[a-z]+)?$/.test(lower)) continue;
    const markdown = /\.(md|markdown|mdown|mkd)$/.test(lower);
    if (best === null || (markdown && !/\.(md|markdown|mdown|mkd)$/.test(best.toLowerCase()))) best = entry.name;
  }
  return best;
}

/**
 * The header's checkout: where it is and how it stands (Code's *Local
 * checkout* words), both `null` with no clone, `null` itself while
 * `entity_checkout` has not answered.
 */
export function checkoutSentence(view: CheckoutView | undefined): { path: string | null; state: string | null } | null {
  if (view === undefined) return null;
  if (view.path === null) return { path: null, state: null };
  const line = checkoutLine(view);
  return { path: view.path, state: line !== null && line.text !== view.path ? line.text : null };
}

/** A commit's short sha from its id's `@<sha>`, the way the source shortens it; `null` without one. */
export function shaOf(commitId: string): string | null {
  const found = /@([0-9a-f]{4,64})$/i.exec(commitId);
  return found ? found[1]!.slice(0, 10) : null;
}

/** The pull requests whose declared parent is this repository, in its source. */
export function repoPrs(rows: readonly EntityRow[], repo: Pick<EntityRow, "source_id" | "title">): EntityRow[] {
  return rows.filter((row) => row.kind === "pr" && row.source_id === repo.source_id && row.path === repo.title);
}

/** The pull requests the *Pull requests* tab's filter keeps. */
export function visibleRepoPrs(rows: readonly EntityRow[], states: readonly string[], text: string): EntityRow[] {
  const wanted = states.map((state) => state.toLowerCase());
  return rows.filter((row) => {
    if (wanted.length > 0 && !wanted.includes(row.state?.text.toLowerCase() ?? "")) return false;
    return matchesAll(`${prNumber(row.entity_id) ?? ""} ${row.title}`, text);
  });
}
