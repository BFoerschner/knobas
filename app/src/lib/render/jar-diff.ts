/**
 * Two archives compared (2026-11-22, `docs/decisions/2026-11-22-a-jar-is-compared-by-what-it-holds.md`): the entries
 * that differ between an archive's two sides in a comparison, told by name, CRC-32 and size from each side's listing,
 * with no entry read. Pure: the stage's view (`tf/diff/TfJarDiff.svelte`) holds the reads.
 *
 * - **Class families.** A top-level class and its nested classes (`a/Foo.class`, `a/Foo$Bar.class`) are one row, by
 *   the top-level class (`topClass`): changed where any member differs. It is also what is decompiled.
 * - **Libraries.** A nested archive removed and one added in the same folder under the same library name
 *   (`spring-core-6.1.2.jar`, `spring-core-6.1.3.jar`) are one changed row.
 * - **A duplicated name** is compared by its first entry, as the reader opens it.
 */

import type { ArchiveIndex, ClassOutline, Hunk } from "../ipc/entity";
import { declaration, head, kindOf, memberLine, supers, topClass, type EntryKind } from "./jar";

export type JarChange = "added" | "removed" | "changed";

/** One entry, or one class family, as a side holds it. */
export interface JarSide {
  /** The entry's name, the top-level class's for a family. */
  path: string;
  /** Unpacked size, summed over a family. */
  size: number;
  /** A family's members, the top-level class first where it is there; one entry's own name. */
  members: string[];
}

export interface JarRow {
  /** Unique in the list: the after side's path, else the before side's. */
  key: string;
  change: JarChange;
  kind: EntryKind;
  old: JarSide | null;
  new: JarSide | null;
  /** A library paired across a version: its name and both versions. */
  lib: { name: string; from: string | null; to: string | null } | null;
}

export interface JarCompare {
  rows: JarRow[];
  /** Rows the same on both sides, counted as the rows are (a family one). */
  same: number;
  changed: number;
  added: number;
  removed: number;
}

type Entry = { size: number; crc: number };

/** Each name's first entry, in name order. */
function entries(index: ArchiveIndex): Map<string, Entry> {
  const out = new Map<string, Entry>();
  if (index.names === "") return out;
  const names = index.names.split("\n");
  for (let i = 0; i < names.length; i += 1) {
    const name = names[i]!;
    // A second entry of a name (flagged a duplicate) is passed over: the first is the one read.
    if (out.has(name)) continue;
    out.set(name, { size: index.sizes[i] ?? 0, crc: index.crcs[i] ?? 0 });
  }
  return out;
}

/** The unit an entry is compared in: its top-level class for a class, else itself. */
const unitOf = (name: string) => (name.toLowerCase().endsWith(".class") ? topClass(name) : name);

/** Each unit's entries, by unit, in name order. */
function units(all: Map<string, Entry>): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const name of all.keys()) {
    const unit = unitOf(name);
    const list = out.get(unit);
    if (list === undefined) out.set(unit, [name]);
    else list.push(name);
  }
  // The top-level class first, then its nested ones in name order.
  for (const [unit, list] of out) if (list.length > 1 && list.includes(unit)) out.set(unit, [unit, ...list.filter((n) => n !== unit)]);
  return out;
}

function sideOf(unit: string, members: string[], all: Map<string, Entry>): JarSide {
  return { path: unit, size: members.reduce((sum, m) => sum + (all.get(m)?.size ?? 0), 0), members };
}

function sameUnit(a: string[], b: string[], before: Map<string, Entry>, after: Map<string, Entry>): boolean {
  if (a.length !== b.length) return false;
  const bs = new Set(b);
  return a.every((m) => {
    if (!bs.has(m)) return false;
    const x = before.get(m)!;
    const y = after.get(m)!;
    return x.crc === y.crc && x.size === y.size;
  });
}

/** A library's name and version from its file name, as `index.libs` has it where it does. */
function libOf(index: ArchiveIndex | null, path: string): { name: string; version: string | null } {
  const lib = index?.libs.find((l) => l.entry === path);
  if (lib !== undefined) return { name: lib.name, version: lib.version };
  const file = path.slice(path.lastIndexOf("/") + 1).replace(/\.(jar|war|ear|aar)$/i, "");
  const dash = file.search(/-\d/);
  return dash > 0 ? { name: file.slice(0, dash), version: file.slice(dash + 1) } : { name: file, version: null };
}

const folderOf = (path: string) => path.slice(0, path.lastIndexOf("/") + 1);

/** What differs between `before` and `after`; a side `null` has no archive (an added or deleted file). */
export function compareJars(before: ArchiveIndex | null, after: ArchiveIndex | null): JarCompare {
  const a = before === null ? new Map<string, Entry>() : entries(before);
  const b = after === null ? new Map<string, Entry>() : entries(after);
  const ua = units(a);
  const ub = units(b);
  let rows: JarRow[] = [];
  let same = 0;
  for (const [unit, members] of ua) {
    const other = ub.get(unit);
    if (other === undefined) rows.push({ key: unit, change: "removed", kind: kindOf(unit), old: sideOf(unit, members, a), new: null, lib: null });
    else if (sameUnit(members, other, a, b)) same += 1;
    else rows.push({ key: unit, change: "changed", kind: kindOf(unit), old: sideOf(unit, members, a), new: sideOf(unit, other, b), lib: null });
  }
  for (const [unit, members] of ub) {
    if (!ua.has(unit)) rows.push({ key: unit, change: "added", kind: kindOf(unit), old: null, new: sideOf(unit, members, b), lib: null });
  }
  // Libraries across a version: one removed and one added archive in one folder under one name become a changed row.
  const gone = rows.filter((r) => r.change === "removed" && r.kind === "archive");
  const came = rows.filter((r) => r.change === "added" && r.kind === "archive");
  if (gone.length > 0 && came.length > 0) {
    const used = new Set<JarRow>();
    const paired: JarRow[] = [];
    for (const g of gone) {
      const from = libOf(before, g.key);
      const match = came.filter((c) => !used.has(c) && folderOf(c.key) === folderOf(g.key) && libOf(after, c.key).name === from.name);
      // Only an unambiguous pair: two candidates of one name are left as they are.
      if (match.length !== 1 || gone.filter((x) => folderOf(x.key) === folderOf(g.key) && libOf(before, x.key).name === from.name).length !== 1) continue;
      const c = match[0]!;
      used.add(g);
      used.add(c);
      paired.push({ key: c.key, change: "changed", kind: "archive", old: g.old, new: c.new, lib: { name: from.name, from: from.version, to: libOf(after, c.key).version } });
    }
    if (paired.length > 0) rows = [...rows.filter((r) => !used.has(r)), ...paired];
  }
  rows.sort((x, y) => (x.key < y.key ? -1 : x.key > y.key ? 1 : 0));
  const count = (c: JarChange) => rows.filter((r) => r.change === c).length;
  return { rows, same, changed: count("changed"), added: count("added"), removed: count("removed") };
}

/** The foot's words: `3 changed, 2 added, 1 removed; 1,198 the same`; one side's `1,204 entries, all added`. */
export function compareWords(c: JarCompare, oneSide: "added" | "removed" | null): string {
  const n = (x: number) => x.toLocaleString("en");
  if (oneSide !== null) {
    const total = c.rows.length;
    return `${n(total)} ${total === 1 ? "entry" : "entries"}, all ${oneSide}`;
  }
  const parts = [
    [c.changed, "changed"],
    [c.added, "added"],
    [c.removed, "removed"],
  ]
    .filter(([x]) => (x as number) > 0)
    .map(([x, w]) => `${n(x as number)} ${w as string}`);
  if (parts.length === 0) return `${n(c.same)} the same`;
  return c.same > 0 ? `${parts.join(", ")}; ${n(c.same)} the same` : parts.join(", ");
}

/** A value on both sides, with an arrow only where it differs. */
function both(a: string | null, b: string | null): string | null {
  if (a === null && b === null) return null;
  if (a === b || b === null) return a;
  if (a === null) return b;
  return `${a} → ${b}`;
}

/** The head line: `ledger-service 1.4.0 → 1.5.0 · Spring Boot JAR · Java 17 → 21`, an arrow only where a value differs. */
export function headLine(before: ArchiveIndex | null, after: ArchiveIndex | null, fileName: string): string {
  const h = [before, after].map((x) => (x === null ? null : head(x, fileName)));
  const [ha, hb] = [h[0] ?? null, h[1] ?? null];
  const title = both(ha?.title ?? null, hb?.title ?? null);
  const version = both(ha?.version ?? null, hb?.version ?? null);
  const kind = both(ha?.kind ?? null, hb?.kind ?? null);
  const release = both(before?.release?.replace(/^Java /, "") ?? null, after?.release?.replace(/^Java /, "") ?? null);
  const parts = [[title, version].filter((x) => x !== null).join(" "), kind, release === null ? null : `Java ${release}`];
  return parts.filter((p): p is string => p !== null && p !== "").join(" · ");
}

/** A class's outline as lines, the declaration first: what an *Outline* diff compares. */
export function outlineText(o: ClassOutline): string {
  const lines = [...o.annotations, `${declaration(o)}${supers(o).map((s) => ` ${s.word} ${s.names.join(", ")}`).join("")}`];
  for (const f of o.fields) lines.push(`  ${[...f.annotations, memberLine(f)].join(" ")}`);
  for (const m of o.methods) lines.push(`  ${[...m.annotations, memberLine(m)].join(" ")}`);
  return lines.join("\n");
}

/** The first `max` lines of `hunks` and how many more there are: a long diff draws its start, then *Show the other N lines*. */
export function firstLines(hunks: readonly Hunk[], max: number): { hunks: Hunk[]; more: number } {
  const out: Hunk[] = [];
  let left = max;
  let more = 0;
  for (const h of hunks) {
    if (left <= 0) {
      more += h.lines.length;
      continue;
    }
    if (h.lines.length <= left) out.push(h);
    else {
      out.push({ ...h, lines: h.lines.slice(0, left) });
      more += h.lines.length - left;
    }
    left -= Math.min(left, h.lines.length);
  }
  return { hunks: out, more };
}
