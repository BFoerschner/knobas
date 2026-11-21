/**
 * The JAR view's model (ux19, `docs/decisions/2026-11-18-a-jar-is-drawn-as-its-contents.md`): an archive's listing as
 * a tree, the rows a windowed list draws for what is open, the filter, and what the head says. Pure: the view
 * (`tf/render/TfJar.svelte`) holds the state and the reads.
 */

import type { ArchiveIndex, ClassOutline, Member } from "../ipc/entity";

/** `ArchiveIndex.flags`' bits, as `knobas_jar::flag` sets them. */
export const UNSAFE = 1;
export const UNREADABLE = 2;
export const DUPLICATE = 4;

/** One archive's entries, unpacked from the index's columns once. */
export interface Listing {
  names: string[];
  sizes: number[];
  packed: number[];
  flags: number[];
  tree: Dir;
}

/** A folder: its subfolders and its files' entry numbers, both in name order; its size summed. */
export interface Dir {
  name: string;
  /** From the archive's top, with a trailing `/`; `""` for the top. */
  path: string;
  dirs: Dir[];
  files: number[];
  size: number;
  /** Files below it, at any depth. */
  count: number;
}

/** The listing of `index`, its tree built in one pass over the sorted names. */
export function listing(index: ArchiveIndex): Listing {
  const names = index.names === "" ? [] : index.names.split("\n");
  const top: Dir = { name: "", path: "", dirs: [], files: [], size: 0, count: 0 };
  const byPath = new Map<string, Dir>([["", top]]);
  for (let i = 0; i < names.length; i += 1) {
    const name = names[i]!;
    const size = index.sizes[i] ?? 0;
    let at = top;
    at.size += size;
    at.count += 1;
    let from = 0;
    for (let slash = name.indexOf("/", from); slash !== -1; slash = name.indexOf("/", from)) {
      const path = name.slice(0, slash + 1);
      let dir = byPath.get(path);
      if (dir === undefined) {
        dir = { name: name.slice(from, slash), path, dirs: [], files: [], size: 0, count: 0 };
        byPath.set(path, dir);
        at.dirs.push(dir);
      }
      dir.size += size;
      dir.count += 1;
      at = dir;
      from = slash + 1;
    }
    at.files.push(i);
  }
  // The names come sorted, so files are in order; folders were made in order of their first file, which is name order
  // only when no file sorts between: sort them.
  const sortDirs = (d: Dir) => {
    d.dirs.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    for (const sub of d.dirs) sortDirs(sub);
  };
  sortDirs(top);
  return { names, sizes: index.sizes, packed: index.packed_sizes, flags: index.flags, tree: top };
}

/** A folder with no files and one folder below it is drawn as one row with that folder: `com/example/demo`. */
export function chain(dir: Dir): { label: string; end: Dir } {
  let end = dir;
  let label = dir.name;
  while (end.files.length === 0 && end.dirs.length === 1) {
    end = end.dirs[0]!;
    label = `${label}/${end.name}`;
  }
  return { label, end };
}

/** What an entry is, by its name: how it opens. */
export type EntryKind = "class" | "archive" | "image" | "pdf" | "markdown" | "file";

export function kindOf(name: string): EntryKind {
  const lower = name.toLowerCase();
  if (lower.endsWith(".class")) return "class";
  if (/\.(jar|war|ear|aar)$/.test(lower)) return "archive";
  if (/\.(png|jpe?g|gif|webp|avif|bmp|ico|svg)$/.test(lower)) return "image";
  if (lower.endsWith(".pdf")) return "pdf";
  if (/\.(md|markdown)$/.test(lower)) return "markdown";
  return "file";
}

/** Bytes as text, where they are: no NUL in the first 8 KB and valid UTF-8 throughout. */
export function textOf(bytes: Uint8Array): string | null {
  const head = bytes.subarray(0, 8192);
  if (head.includes(0)) return null;
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

/** A size as the file view's header says one: 1,024 bytes a KB. */
export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

/** What a scope's key is made of: the nested archives' entry names from the top, `!` between. */
export function scopeOf(parent: string, entry: string): string {
  return parent === "" ? entry : `${parent}!${entry}`;
}

/** One row the list draws. `k` is unique across scopes and kinds. */
export type Row =
  | { k: string; kind: "dir"; depth: number; scope: string; label: string; dir: Dir; open: boolean }
  | { k: string; kind: "file"; depth: number; scope: string; i: number; label: string; path: string; open: boolean }
  | { k: string; kind: "detail"; depth: number; scope: string; path: string }
  | { k: string; kind: "lib"; depth: number; scope: string; path: string; name: string; version: string | null; size: number; open: boolean }
  | { k: string; kind: "note"; depth: number; scope: string; text: string };

/** What the view has open, as plain sets read once a pass. */
export interface Open {
  /** `scope|folder path`. */
  dirs: ReadonlySet<string>;
  /** `scope|entry`: an entry opened in place, or a nested archive opened as its tree. */
  files: ReadonlySet<string>;
}

/** A nested archive's state: listed, being read, or refused. */
export type Nested = { ok: true; listing: Listing } | { ok: false; error: string } | "reading";

const NO_WORDS: readonly string[] = [];

/** The filter's words: lowercase, every one must match. */
export function terms(typed: string): readonly string[] {
  const words = typed.toLowerCase().split(/\s+/).filter((w) => w !== "");
  return words.length === 0 ? NO_WORDS : words;
}

export function matches(hay: string, words: readonly string[]): boolean {
  for (const w of words) if (!hay.includes(w)) return false;
  return true;
}

/**
 * The rows of the archive at `scope` (its listing `l`) below `depth`, for what `open` holds. A nested archive opened
 * draws its own rows below its entry, one level in, read from `nested`.
 */
export function treeRows(l: Listing, scope: string, depth: number, open: Open, nested: ReadonlyMap<string, Nested>, out: Row[] = []): Row[] {
  const walk = (dir: Dir, d: number) => {
    for (const sub of dir.dirs) {
      const { label, end } = chain(sub);
      const isOpen = open.dirs.has(`${scope}|${end.path}`);
      out.push({ k: `g${scope}|${end.path}`, kind: "dir", depth: d, scope, label, dir: end, open: isOpen });
      if (isOpen) walk(end, d + 1);
    }
    for (const i of dir.files) fileRow(l, scope, i, d, open, nested, out, l.names[i]!.slice(dir.path.length));
  };
  walk(l.tree, depth);
  return out;
}

function fileRow(l: Listing, scope: string, i: number, depth: number, open: Open, nested: ReadonlyMap<string, Nested>, out: Row[], label: string) {
  const path = l.names[i]!;
  const key = `${scope}|${path}`;
  const isOpen = open.files.has(key);
  out.push({ k: `f${key}#${i}`, kind: "file", depth, scope, i, label, path, open: isOpen });
  if (!isOpen) return;
  if (kindOf(path) === "archive") nestedRows(scopeOf(scope, path), depth + 1, open, nested, out);
  else out.push({ k: `d${key}#${i}`, kind: "detail", depth, scope, path });
}

function nestedRows(scope: string, depth: number, open: Open, nested: ReadonlyMap<string, Nested>, out: Row[]) {
  const got = nested.get(scope);
  if (got === undefined || got === "reading") return;
  if (!got.ok) {
    out.push({ k: `n${scope}`, kind: "note", depth, scope, text: got.error });
    return;
  }
  if (got.listing.names.length === 0) out.push({ k: `n${scope}`, kind: "note", depth, scope, text: "An empty archive." });
  treeRows(got.listing, scope, depth, open, nested, out);
}

/** The filter's rows: every entry of the top archive and of each nested one listed, matching every word, by path. */
export function filteredRows(top: Listing, words: readonly string[], open: Open, nested: ReadonlyMap<string, Nested>): { rows: Row[]; hits: number } {
  const out: Row[] = [];
  let hits = 0;
  const scan = (l: Listing, scope: string) => {
    for (let i = 0; i < l.names.length; i += 1) {
      const name = l.names[i]!;
      if (!matches(name.toLowerCase(), words)) continue;
      if (scope === "") hits += 1;
      fileRow(l, scope, i, 0, { dirs: open.dirs, files: kindOf(name) === "archive" ? new Set() : open.files }, nested, out, scope === "" ? name : `${scope}!${name}`);
    }
  };
  scan(top, "");
  for (const [scope, got] of nested) if (typeof got === "object" && got.ok) scan(got.listing, scope);
  return { rows: out, hits };
}

/** The dependency list: the archive's nested JARs, each opening to its tree in place. */
export function libRows(index: ArchiveIndex, words: readonly string[], open: Open, nested: ReadonlyMap<string, Nested>): Row[] {
  const out: Row[] = [];
  for (const lib of index.libs) {
    if (words.length > 0 && !matches(`${lib.name} ${lib.version ?? ""} ${lib.entry}`.toLowerCase(), words)) continue;
    const key = `|${lib.entry}`;
    const isOpen = open.files.has(key);
    out.push({ k: `l${key}`, kind: "lib", depth: 0, scope: "", path: lib.entry, name: lib.name, version: lib.version, size: lib.size, open: isOpen });
    if (isOpen) nestedRows(scopeOf("", lib.entry), 1, open, nested, out);
  }
  return out;
}

/** What the head says about an archive. */
export interface Head {
  title: string;
  version: string | null;
  /** `group:artifact`, where Maven's files say. */
  coords: string | null;
  kind: string;
  facts: { label: string; value: string; code: boolean }[];
  /** The manifest's other fields, for the fold. */
  rest: [string, string][];
  classPath: string[];
}

const KINDS: Record<string, string> = {
  jar: "JAR",
  "boot-jar": "Spring Boot JAR",
  war: "WAR",
  "boot-war": "Spring Boot WAR",
  ear: "EAR",
  aar: "Android library",
};

/** The manifest fields the head names, in its order, and how; the rest go in the fold. */
const SHOWN: { keys: string[]; label: string; code: boolean }[] = [
  { keys: ["Start-Class"], label: "Application class", code: true },
  { keys: ["Main-Class"], label: "Main class", code: true },
  { keys: ["Automatic-Module-Name"], label: "Module", code: true },
  { keys: ["Implementation-Title"], label: "Title", code: false },
  { keys: ["Implementation-Vendor"], label: "Vendor", code: false },
  { keys: ["Built-By"], label: "Built by", code: false },
  { keys: ["Build-Jdk-Spec", "Build-Jdk"], label: "Built with JDK", code: false },
  { keys: ["Created-By"], label: "Created by", code: false },
  { keys: ["Spring-Boot-Version"], label: "Spring Boot", code: false },
  { keys: ["Bundle-SymbolicName"], label: "OSGi bundle", code: true },
];

export function head(index: ArchiveIndex, fileName: string): Head {
  const field = (k: string) => index.manifest.find(([n]) => n === k)?.[1] ?? null;
  const own = index.maven[0] ?? null;
  const stem = fileName.replace(/\.(jar|war|ear|aar)$/i, "");
  const dash = stem.search(/-\d/);
  const fromName = dash > 0 ? { name: stem.slice(0, dash), version: stem.slice(dash + 1) } : { name: stem, version: null };
  const title = own?.artifact ?? field("Implementation-Title") ?? fromName.name;
  const version = (own !== null && own.version !== "" ? own.version : null) ?? field("Implementation-Version") ?? field("Bundle-Version") ?? fromName.version;
  const facts: Head["facts"] = [];
  const used = new Set<string>(["Manifest-Version", "Class-Path", "Implementation-Version"]);
  if (index.release !== null) {
    const multi = index.releases.length > 0 ? `, and ${index.releases.map((r) => `Java ${r}`).join(", ")} where it runs on it` : "";
    facts.push({ label: "Compiled for", value: `${index.release}${multi}`, code: false });
  }
  for (const s of SHOWN) {
    const key = s.keys.find((k) => field(k) !== null);
    if (key === undefined) continue;
    for (const k of s.keys) used.add(k);
    // Spring Boot's own launcher is the main class of every fat JAR: the application's class says more.
    if (key === "Main-Class" && field("Start-Class") !== null) {
      facts.push({ label: "Launcher", value: field(key)!, code: true });
      continue;
    }
    if (key === "Implementation-Title" && field(key) === title) continue;
    facts.push({ label: s.label, value: field(key)!, code: s.code });
  }
  if (own?.description) facts.push({ label: "Description", value: own.description, code: false });
  const classPath = (field("Class-Path") ?? "").split(/\s+/).filter((p) => p !== "");
  return {
    title,
    version,
    coords: own === null ? null : `${own.group}:${own.artifact}`,
    kind: KINDS[index.kind] ?? "Archive",
    facts,
    rest: index.manifest.filter(([k]) => !used.has(k)),
    classPath,
  };
}

/** A class outline's declaration line: `public abstract class Shelf<T extends Comparable<T>>`. */
export function declaration(o: ClassOutline): string {
  const simple = o.name.slice(o.name.lastIndexOf(".") + 1).replace(/\$/g, ".");
  const mods = o.modifiers === "" ? "" : `${o.modifiers} `;
  if (o.kind === "module") return `module ${o.name}`;
  const comps = o.kind === "record" ? `(${o.components.join(", ")})` : "";
  return `${mods}${o.kind} ${simple}${o.type_params}${comps}`;
}

/** What follows the declaration: `extends`, `implements` (an interface's `extends`), `permits`. */
export function supers(o: ClassOutline): { word: string; names: string[] }[] {
  const out: { word: string; names: string[] }[] = [];
  if (o.extends !== null) out.push({ word: "extends", names: [o.extends] });
  if (o.implements.length > 0) out.push({ word: o.kind === "interface" || o.kind === "@interface" ? "extends" : "implements", names: o.implements });
  if (o.permits.length > 0) out.push({ word: "permits", names: o.permits });
  return out;
}

/** A member as source declares it, less its annotations: `public static final int CAPACITY = 42`. */
export function memberLine(m: Member): string {
  if (m.constant) return m.name;
  const mods = m.modifiers === "" ? "" : `${m.modifiers} `;
  const tp = m.type_params === "" ? "" : `${m.type_params} `;
  const ty = m.ty === "" ? "" : `${m.ty} `;
  if (m.params === null) return `${mods}${ty}${m.name}${m.value === null ? "" : ` = ${m.value}`}`;
  const throws = m.throws.length > 0 ? ` throws ${m.throws.join(", ")}` : "";
  return `${mods}${tp}${ty}${m.name}(${m.params})${throws}`;
}

/** The top-level class an entry decompiles as: `a/Foo$Bar.class` is `a/Foo.class`. */
export function topClass(path: string): string {
  const slash = path.lastIndexOf("/") + 1;
  const file = path.slice(slash);
  const dollar = file.indexOf("$");
  return dollar <= 0 ? path : `${path.slice(0, slash)}${file.slice(0, dollar)}.class`;
}
