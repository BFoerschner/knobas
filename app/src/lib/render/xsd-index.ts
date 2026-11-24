/**
 * The repository's schemas, indexed only when a reference needs it (the XSD design, 2026-11-23, section 15). Steps 3
 * to 5 of section 14 need every `.xsd` of the repository and the namespace each declares, and its XML catalogs. That
 * is read the first time a reference is not found beside its file, never before: openTRANS, UBL and CII never build it.
 *
 * - **Bounds**: {@link INDEX_FILES} files and {@link INDEX_BYTES}, {@link INDEX_AT_ONCE} reads at once, nearest
 *   folders first. Past a bound the index says how many it read of how many.
 * - **Kept by content**: each file's sniff is kept by its path and its text, so at another commit a file whose content
 *   is the same is not sniffed again, and a path gone is dropped. The last four repositories' are kept, in memory only.
 */
import type { Catalog, CatalogEntry } from "./xsd-locate";
import { isRemote, joinPath, nearest } from "./xsd-locate";
import { sniffNamespace } from "./xsd-sniff";
import { attr, readSchemaXml, type XNode } from "./xsd-xml";
import type { SchemaIndexWire } from "./xsd-answer";

export const INDEX_FILES = 300;
export const INDEX_BYTES = 32 * 1024 * 1024;
export const INDEX_AT_ONCE = 6;

export const CATALOG_NS = "urn:oasis:names:tc:entity:xmlns:xml:catalog";
const CATALOG_NAME = /(^|\/)(catalog[^/]*\.xml|[^/]*-catalog\.xml|[^/]*\.cat(alog)?)$/i;

/** Each file's sniff by its text, per repository; the last four repositories. */
const kept = new Map<string, Map<string, { text: string; ns: string | undefined }>>();

/** An OASIS catalog's entries, its targets read from its own folder (and `xml:base`); an address is dropped. */
export function readCatalog(path: string, text: string): Catalog | null {
  if (text.indexOf(CATALOG_NS) === -1) return null;
  let root: XNode;
  try {
    root = readSchemaXml(text).root;
  } catch {
    return null;
  }
  if (root.l !== "catalog" || root.ns !== CATALOG_NS) return null;
  const entries: CatalogEntry[] = [];
  const walk = (node: XNode, base: string) => {
    for (const c of node.c) {
      if (c.ns !== CATALOG_NS) continue;
      const b = attr(c, "xml:base") ?? base;
      const to = (v: string | null): string | null => (v === null || isRemote(v) ? null : joinPath(path, (b !== "" ? b.replace(/\/?$/, "/") : "") + v));
      const add = (kind: CatalogEntry["kind"], match: string | null, target: string | null) => {
        const t = to(target);
        if (match !== null && t !== null) entries.push({ kind, match, to: t });
      };
      if (c.l === "group") walk(c, b);
      else if (c.l === "uri") add("exact", attr(c, "name"), attr(c, "uri"));
      else if (c.l === "system") add("exact", attr(c, "systemId"), attr(c, "uri"));
      else if (c.l === "rewriteURI") add("prefix", attr(c, "uriStartString"), attr(c, "rewritePrefix"));
      else if (c.l === "rewriteSystem") add("prefix", attr(c, "systemIdStartString"), attr(c, "rewritePrefix"));
      else if (c.l === "uriSuffix") add("suffix", attr(c, "uriSuffix"), attr(c, "uri"));
      else if (c.l === "systemSuffix") add("suffix", attr(c, "systemIdSuffix"), attr(c, "uri"));
    }
  };
  walk(root, "");
  return { path, entries };
}

/**
 * Builds the index of a repository's schemas at the commit shown: every `.xsd` sniffed, every catalog read. `from` is
 * the file that asked: the nearest folders are read first when the bound is met. `read` is the file view's read at
 * that commit.
 */
export async function buildIndex(repo: string, paths: readonly string[], from: string, read: (path: string) => Promise<string | null>): Promise<SchemaIndexWire> {
  const xsds = nearest(
    paths.filter((p) => /\.xsd$/i.test(p)),
    from,
  );
  const cats = paths.filter((p) => CATALOG_NAME.test(p));
  let mine = kept.get(repo);
  if (mine === undefined) {
    if (kept.size >= 4) kept.delete(kept.keys().next().value!);
    kept.set(repo, (mine = new Map()));
  }
  const declares: [string, string | null][] = [];
  const catalogs: Catalog[] = [];
  let bytes = 0;
  let readCount = 0;
  const queue = xsds.slice(0, INDEX_FILES);
  const work = async () => {
    for (;;) {
      const p = queue.shift();
      if (p === undefined) return;
      if (bytes >= INDEX_BYTES) return;
      const text = await read(p);
      if (text === null) continue;
      bytes += text.length;
      readCount += 1;
      const had = mine.get(p);
      const ns = had !== undefined && had.text === text ? had.ns : sniffNamespace(text);
      mine.set(p, { text, ns });
      declares.push([p, ns ?? null]);
    }
  };
  await Promise.all(Array.from({ length: INDEX_AT_ONCE }, work));
  for (const p of cats) {
    const text = await read(p);
    if (text === null) continue;
    const cat = readCatalog(p, text);
    if (cat !== null) catalogs.push(cat);
  }
  // A path gone from this commit is dropped from what is kept.
  const here = new Set(paths);
  for (const p of [...mine.keys()]) if (!here.has(p)) mine.delete(p);
  return { declares: declares.filter(([, n]) => n !== null), catalogs, total: xsds.length, read: readCount };
}

/**
 * Each listing's index, read once, by the repository and the folder asked from (the bound reads the nearest folders
 * first). The page hands the same list of paths for the same commit and a new one for another, so the set's own need
 * of the index (a reference not found beside its file) and *Point to a file…* after it share one read: the picker's
 * press was 168 ms at 6x on 1440 x 868 reading every `.xsd` again, against 150. Held weakly by the list.
 */
const held = new WeakMap<readonly string[], Map<string, Promise<SchemaIndexWire>>>();
export function heldIndex(repo: string, paths: readonly string[], from: string, read: (path: string) => Promise<string | null>): Promise<SchemaIndexWire> {
  let byKey = held.get(paths);
  if (byKey === undefined) held.set(paths, (byKey = new Map()));
  const key = `${repo}\n${from.slice(0, from.lastIndexOf("/") + 1)}`;
  let index = byKey.get(key);
  if (index === undefined) {
    const mine = byKey;
    index = buildIndex(repo, paths, from, read);
    index.catch(() => mine.delete(key));
    byKey.set(key, index);
  }
  return index;
}

