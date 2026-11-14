/**
 * **What Docs has read of its tree** (the instant-open pass of PR #2479,
 * 2026-11-12, `docs/decisions/2026-11-12-opening-a-page-draws-what-the-mirror-holds.md`).
 *
 * At Björn's size a wiki holds 5,032 pages, and Docs read all of them, with
 * every page's parent walked out of its payload, before it drew its home:
 * 612 KB and 780 to 820 ms on every first open. Now it reads what it draws:
 *
 * - the home's first screen is `docs_outline`: every space with its count,
 *   the top of its tree and the pages its tile lists, the page read last and
 *   *For you*, each page with every page above it and how many are under it;
 * - a fold opened reads the pages under it, a page opened the way to it, a
 *   space's page the whole space (`docs_nodes`);
 * - only the job's pages (*This job*, the home's *Pages on …*), the new-page
 *   dialog and a queued page's settling read the whole tree, as before.
 *
 * What was read is kept here, outside any component, so Docs drawn again (a
 * return, or a page the kept pages dropped) draws it at once and reads again
 * behind it. Every part read is a cut of one tree, in its order (`ord`), and
 * holds every page above each page it holds, so the helpers in `docs.ts` that
 * walk up a page's parents work on a part as on the whole.
 */
import type { DocNode, DocSpace, DocsNodesAsked, DocsTree } from "../../ipc/entity";
import { pageCount, spaceId } from "./docs";

export { pageCount };

/** The reads the tree is made of. */
export interface TreeReads {
  outline(): Promise<DocsTree>;
  nodes(asked: DocsNodesAsked): Promise<DocSpace[]>;
  whole(): Promise<DocsTree>;
}

/** What Docs holds of the tree, and what it asked for beyond the outline, which a read again asks for again. */
export interface HeldTree {
  tree: DocsTree | null;
  /** Read whole (`docs_tree`), so no part is missing. */
  complete: boolean;
  pages: Set<string>;
  folds: Set<string>;
  spaces: Set<string>;
}

/** The window's one, across every mount of Docs. */
export const heldTree: HeldTree = { tree: null, complete: false, pages: new Set(), folds: new Set(), spaces: new Set() };

/** Back to nothing read, for a test. */
export function forgetTree(): void {
  heldTree.tree = null;
  heldTree.complete = false;
  heldTree.pages.clear();
  heldTree.folds.clear();
  heldTree.spaces.clear();
}

/** Whether node `node` of `space` has pages under it the tree draws as a fold, read or not. */
export function hasChildren(node: DocNode, under: ReadonlyMap<string | null, unknown>): boolean {
  return under.has(node.entity_id) || (node.children ?? 0) > 0;
}

/** Whether the pages under `node` are still to be read: it says it has some, and none is held. */
export function childrenMissing(node: DocNode, under: ReadonlyMap<string | null, unknown>): boolean {
  return (node.children ?? 0) > 0 && !under.has(node.entity_id);
}

/** `tree` with `parts` merged in: each part's pages put in their space, in the space's order. */
export function mergeParts(tree: DocsTree, parts: readonly DocSpace[]): DocsTree {
  if (parts.length === 0) return tree;
  const spaces = tree.spaces.map((space) => {
    const part = parts.find((p) => spaceId(p) === spaceId(space));
    if (part === undefined) return space;
    const byId = new Map(space.pages.map((page) => [page.entity_id, page]));
    for (const page of part.pages) byId.set(page.entity_id, page);
    const pages = [...byId.values()].sort((a, b) => (a.ord ?? 0) - (b.ord ?? 0));
    const count = part.page_count ?? space.page_count;
    return count === undefined ? { ...space, pages } : { ...space, page_count: count, pages };
  });
  return { ...tree, spaces };
}

/** Whether a space is held whole: read whole, or holding as many pages as it has. */
export function spaceWhole(space: DocSpace): boolean {
  return heldTree.complete || space.page_count === undefined || space.pages.length >= space.page_count;
}

/** Whether a page is held, with the way to it. */
export function pageHeld(tree: DocsTree, id: string): boolean {
  return tree.spaces.some((space) => space.pages.some((page) => page.entity_id === id));
}

/**
 * The tree read again: whole when `whole` asks, else the outline and every
 * part asked for so far, together, so what is drawn is replaced in one step
 * and nothing it held goes missing in between.
 */
export async function readHeld(reads: TreeReads, whole: boolean): Promise<DocsTree> {
  if (whole) {
    const read = await reads.whole();
    heldTree.tree = read;
    heldTree.complete = true;
    return read;
  }
  const asked = { pages: [...heldTree.pages], folds: [...heldTree.folds], spaces: [...heldTree.spaces] };
  const more = asked.pages.length + asked.folds.length + asked.spaces.length > 0;
  const [outline, parts] = await Promise.all([reads.outline(), more ? reads.nodes(asked) : Promise.resolve([])]);
  let read = mergeParts(outline, parts);
  // A part asked for while this read was out (`readParts`) is read too, or the tree drawn would lose it.
  const since = {
    pages: [...heldTree.pages].filter((id) => !asked.pages.includes(id)),
    folds: [...heldTree.folds].filter((id) => !asked.folds.includes(id)),
    spaces: [...heldTree.spaces].filter((id) => !asked.spaces.includes(id)),
  };
  if (since.pages.length + since.folds.length + since.spaces.length > 0) read = mergeParts(read, await reads.nodes(since));
  heldTree.tree = read;
  heldTree.complete = false;
  return read;
}

/**
 * Read the parts `asked` names that the held tree lacks, merged into it.
 * `null` when nothing was missing or no tree is held yet.
 */
export async function readParts(reads: TreeReads, asked: Partial<DocsNodesAsked>): Promise<DocsTree | null> {
  const tree = heldTree.tree;
  if (tree === null || heldTree.complete) return null;
  const under = new Map<string, DocNode>();
  for (const space of tree.spaces) for (const page of space.pages) under.set(page.entity_id, page);
  const hasUnder = new Set(tree.spaces.flatMap((s) => s.pages.map((p) => p.parent)));
  const pages = (asked.pages ?? []).filter((id) => !heldTree.pages.has(id));
  const folds = (asked.folds ?? []).filter((id) => !heldTree.folds.has(id) && !(under.has(id) && (hasUnder.has(id) || (under.get(id)!.children ?? 0) === 0)));
  const spaces = (asked.spaces ?? []).filter((id) => !heldTree.spaces.has(id));
  for (const id of pages) heldTree.pages.add(id);
  for (const id of folds) heldTree.folds.add(id);
  for (const id of spaces) heldTree.spaces.add(id);
  if (pages.length + folds.length + spaces.length === 0) return null;
  let parts: DocSpace[];
  try {
    parts = await reads.nodes({ pages, folds, spaces });
  } catch (cause) {
    // Not read: asked again next time.
    for (const id of pages) heldTree.pages.delete(id);
    for (const id of folds) heldTree.folds.delete(id);
    for (const id of spaces) heldTree.spaces.delete(id);
    throw cause;
  }
  // A read again (`readHeld`) may have replaced the tree meanwhile: merge into what is held now.
  if (heldTree.tree === null) return null;
  heldTree.tree = mergeParts(heldTree.tree, parts);
  return heldTree.tree;
}
