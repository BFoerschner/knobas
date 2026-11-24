/**
 * Where a file a schema names is found (the XSD design, 2026-11-23, section 14; the XML view's design, section 10).
 * Pure: the worker hands what it knows, and an answer that needs more says what to read.
 *
 * In order, the first that fits:
 *   1. what the person pointed to for this namespace and location, or an address they had knobas download;
 *   2. the location read from the folder of the file that names it, inside the repository;
 *   3. an OASIS XML catalog in the repository;
 *   4. a file of that name elsewhere in the repository;
 *   5. any `.xsd` in the repository that declares the namespace (imports only);
 *   6. another synced repository, where exactly one declares the namespace (imports only);
 *   7. a schema that comes with knobas.
 * A file fits only if it declares the namespace asked for: for an import the import's, for an include, a redefine or
 * an override the includer's or none. An address is never fetched by itself, and never joined as a path.
 *
 * Paths: a file of the schema's own repository is its path. A file read from elsewhere has a path of its own kind, so
 * the set keeps one map of texts: `@repo/<entity>/<path>`, `@download/<sha256>`, `@knobas/<name>`.
 */
import { XS } from "./xsd-xml";

export type RefKind = "import" | "include" | "redefine" | "override";
export type How = "pointed" | "downloaded" | "beside" | "catalog" | "name" | "namespace" | "repo" | "bundled" | "builtin";

/** What a person told knobas for a namespace and a location (`xsd.locations`). */
export interface Pointed {
  readonly namespace: string;
  readonly location: string;
  readonly to: { readonly repo: string; readonly path: string; readonly name?: string } | { readonly url: string; readonly sha256: string; readonly at: string };
}

/** A catalog's entries, read once (`xsd-index.ts`). */
export interface CatalogEntry {
  readonly kind: "exact" | "prefix" | "suffix";
  readonly match: string;
  /** A path in the repository (an address is dropped when the catalog is read). */
  readonly to: string;
}
export interface Catalog {
  readonly path: string;
  readonly entries: readonly CatalogEntry[];
}

/** The repository's schemas, read only when a reference is not found beside its file (section 15). */
export interface SchemaIndex {
  /** Every `.xsd` read, with what it declares (`undefined`: not a schema). */
  readonly declares: ReadonlyMap<string, string | undefined>;
  readonly catalogs: readonly Catalog[];
  /** How many `.xsd` the repository holds, and how many were read (the bound). */
  readonly total: number;
  readonly read: number;
}

/** Other synced repositories' `.xsd` files that declare a namespace (`files_declaring`, step S3b). */
export interface Elsewhere {
  readonly repo: string;
  /** Its name, `tidewater/catalog-tools`, and the branch read. */
  readonly name: string;
  readonly branch: string;
  readonly path: string;
}

export interface LocateIn {
  /** Every path of the repository at the commit shown; `null` until they are read (they come with the index). */
  readonly paths: ReadonlySet<string> | null;
  /** What a file declares, where its text was read: `undefined` not a schema, `null` not read yet. */
  readonly declares: (path: string) => string | undefined | null;
  /** Whether a file was asked for and is not there (its read answered nothing). */
  readonly gone?: (path: string) => boolean;
  readonly pointed: readonly Pointed[];
  /** The repository's index, once built; `null` before. */
  readonly index: SchemaIndex | null;
  /** Other repositories' files for a namespace, once asked; `null` before. */
  readonly elsewhere: ((namespace: string) => readonly Elsewhere[] | null) | null;
  /** The schemas that come with knobas, by namespace: their paths. */
  readonly bundled: ReadonlyMap<string, string>;
}

export type Located =
  | { readonly to: string; readonly how: How; readonly others: readonly string[]; readonly via?: string; readonly from?: Elsewhere }
  | { readonly to: null; readonly why: string; readonly address: string | null; readonly mismatch: { path: string; declares: string } | null; readonly two?: readonly Elsewhere[] }
  /** Needs more read first: paths, `?index`, or `?elsewhere:<namespace>`. */
  | { readonly need: readonly string[] };

export const dirOf = (p: string): string => (p.lastIndexOf("/") === -1 ? "" : p.slice(0, p.lastIndexOf("/")));
export const baseOf = (p: string): string => p.slice(p.lastIndexOf("/") + 1);

/** An address: any scheme but none and `file:`. It is never joined as a path. */
export const isRemote = (loc: string): boolean => /^[a-z][a-z0-9+.-]*:/i.test(loc) && !/^file:/i.test(loc);

/** `rel` read from the folder of `from`, inside the repository: `null` where it climbs out of it. */
export function joinPath(from: string, rel: string): string | null {
  if (isRemote(rel)) return null;
  const clean = rel.replace(/^file:(\/\/)?/i, "");
  // A path of another kind (`@repo/<id>/…`) keeps its head: `..` never climbs past it.
  const kind = /^@(repo\/[^/]+|download|knobas)\//.exec(from);
  const head = kind === null ? "" : kind[0];
  const parts = dirOf(from.slice(head.length)) === "" ? [] : dirOf(from.slice(head.length)).split("/");
  for (const seg of clean.split("/")) {
    if (seg === "" || seg === ".") continue;
    if (seg === "..") {
      if (parts.length === 0) return null;
      parts.pop();
    } else parts.push(seg);
  }
  return head + parts.join("/");
}

/** How many leading folders two paths share: the nearer file wins a tie. */
export function shared(a: string, b: string): number {
  const x = dirOf(a).split("/");
  const y = dirOf(b).split("/");
  let k = 0;
  while (k < x.length && k < y.length && x[k] === y[k]) k += 1;
  return k;
}

/** Nearest first: the most leading folders shared with `from`, then the shortest path, then by name. */
export function nearest(list: readonly string[], from: string): string[] {
  return [...list].sort((a, b) => shared(b, from) - shared(a, from) || a.length - b.length || (a < b ? -1 : a > b ? 1 : 0));
}

/** A catalog entry's answer for `key`, or `null`. */
function catalogHit(e: CatalogEntry, key: string): string | null {
  if (e.kind === "exact") return key === e.match ? e.to : null;
  if (e.kind === "prefix") return key.startsWith(e.match) ? e.to + key.slice(e.match.length) : null;
  return key.endsWith(e.match) ? e.to : null;
}

/** Where the file a reference names is (section 14). */
export function locate(inn: LocateIn, from: string, fromNamespace: string, kind: RefKind, namespace: string | null, location: string | null): Located {
  if (kind === "import" && namespace === XS) return { to: XS, how: "builtin", others: [] };
  const want = kind === "import" ? (namespace ?? "") : fromNamespace;
  const need: string[] = [];
  /** Whether `path` declares what is wanted; `null` while its text is not read. */
  const fits = (path: string): boolean | null => {
    const n = inn.declares(path);
    if (n === null) return null;
    if (n === undefined) return false;
    return kind === "import" ? n === want : n === want || n === "";
  };

  // 1. What the person pointed to, or downloaded.
  for (const p of inn.pointed) {
    if ((p.namespace || "") !== (namespace ?? "") || (p.location || "") !== (location ?? "")) continue;
    if ("url" in p.to) return { to: `@download/${p.to.sha256}`, how: "downloaded", others: [] };
    return { to: `@repo/${p.to.repo}/${p.to.path}`, how: "pointed", others: [] };
  }

  // 2. Beside the file that names it.
  let mismatch: { path: string; declares: string } | null = null;
  if (location !== null && !isRemote(location)) {
    const beside = joinPath(from, location);
    // Before the paths are known, the file beside it is tried by reading it: a read that answers nothing is no file.
    if (beside !== null && !(inn.gone?.(beside) ?? false) && (inn.paths === null || inn.paths.has(beside) || beside.startsWith("@"))) {
      const ok = fits(beside);
      if (ok === null) return { need: [beside] };
      if (ok) return { to: beside, how: "beside", others: [] };
      const d = inn.declares(beside);
      mismatch = { path: beside, declares: d ?? "" };
    }
  }

  // 3 to 5 need the repository's index: built the first time a reference is not found beside its file.
  if (inn.index === null || inn.paths === null) return { need: ["?index"] };
  const index = inn.index;
  const paths = inn.paths;
  const keys = [location, namespace].filter((k): k is string => k !== null && k !== "");
  for (const cat of index.catalogs) {
    for (const e of cat.entries) {
      for (const k of keys) {
        const hit = catalogHit(e, k);
        if (hit === null || !paths.has(hit)) continue;
        const ok = fits(hit);
        if (ok === null) need.push(hit);
        else if (ok) return { to: hit, how: "catalog", others: [], via: cat.path };
      }
    }
  }
  if (need.length > 0) return { need };
  if (location !== null) {
    const name = baseOf(location.replace(/[?#].*$/, "")).toLowerCase();
    const named = [...index.declares.keys()].filter((p) => p !== from && baseOf(p).toLowerCase() === name && fits(p) === true);
    if (named.length > 0) {
      const order = nearest(named, from);
      return { to: order[0]!, how: "name", others: order.slice(1) };
    }
    if (mismatch === null) {
      const wrong = [...index.declares.keys()].find((p) => p !== from && baseOf(p).toLowerCase() === name && inn.declares(p) !== undefined);
      if (wrong !== undefined) mismatch = { path: wrong, declares: inn.declares(wrong) ?? "" };
    }
  }
  if (kind === "import" && want !== "") {
    const declared = [...index.declares.entries()].filter(([p, n]) => p !== from && n === want).map(([p]) => p);
    if (declared.length > 0) {
      const order = nearest(declared, from);
      return { to: order[0]!, how: "namespace", others: order.slice(1) };
    }
  }

  // 6. Another synced repository, where exactly one declares the namespace.
  if (kind === "import" && want !== "" && inn.elsewhere !== null) {
    const there = inn.elsewhere(want);
    if (there === null) return { need: [`?elsewhere:${want}`] };
    const repos = new Set(there.map((e) => e.repo));
    if (repos.size === 1) {
      const name = location === null ? "" : baseOf(location).toLowerCase();
      const pick = there.find((e) => baseOf(e.path).toLowerCase() === name) ?? there[0]!;
      return { to: `@repo/${pick.repo}/${pick.path}`, how: "repo", others: [], from: pick };
    }
    if (repos.size > 1) return { to: null, why: "Two repositories declare it.", address: null, mismatch: null, two: there };
  }

  // 7. What comes with knobas.
  const shipped = kind === "import" ? inn.bundled.get(want) : undefined;
  if (shipped !== undefined) return { to: shipped, how: "bundled", others: [] };

  const address = location !== null && isRemote(location) ? location : null;
  const why =
    location === null
      ? "The import names no file."
      : address !== null
        ? "It is an address. knobas fetches none by itself."
        : mismatch !== null
          ? `${baseOf(mismatch.path)} is there, but it declares ${mismatch.declares === "" ? "no namespace" : mismatch.declares}.`
          : "No file of that name is here.";
  return { to: null, why, address, mismatch };
}
