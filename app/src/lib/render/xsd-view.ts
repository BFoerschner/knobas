/**
 * The schema explorer's rail, pure (the XSD design, 2026-11-23, sections 7 and 8): the rows each mode lists, and how
 * often a thing may stand in its token and in words.
 */
import type { DeclRow, Occurs, SetIndex, TreeRow } from "./xsd-answer";

export type RailMode = "tree" | "elements" | "types";

export type RailRow =
  | { readonly kind: "overview"; readonly k: "overview"; readonly d: 0 }
  | { readonly kind: "tree"; readonly k: string; readonly d: number; readonly row: TreeRow; readonly open: boolean }
  | { readonly kind: "file"; readonly k: string; readonly d: 0; readonly path: string; readonly count: number; readonly open: boolean; readonly reading: boolean }
  | { readonly kind: "decl"; readonly k: string; readonly d: number; readonly decl: DeclRow; readonly marks: readonly number[] | null };

/** The token of how often: `1`, `0..1`, `1..∞`, `2..4`, `0` for never. */
export function occText(o: Occurs): string {
  if (o.max === 0) return "0";
  const max = o.max === -1 ? "∞" : String(o.max);
  return o.min === o.max ? String(o.min) : `${o.min}..${max}`;
}

/** How often, in words: the token's `title`. */
export function occWords(o: Occurs): string {
  const { min, max } = o;
  if (max === 0) return "Never: the schema forbids it here";
  if (min === 1 && max === 1) return "Exactly once";
  if (min === 0 && max === 1) return "Once, or not at all";
  if (max === -1) return min === 0 ? "Any number of times, or not at all" : min === 1 ? "Once or more" : `${min} times or more`;
  if (min === max) return `Exactly ${min} times`;
  return `${min} to ${max} times`;
}

const TYPES = new Set(["ct", "st", "gr", "ag", "at"]);

/** Which declarations a mode lists: *Elements* the elements, *Types* the rest. */
export const inMode = (mode: RailMode, d: DeclRow): boolean => (mode === "types" ? TYPES.has(d.kind) : d.kind === "el");

export interface RailInput {
  readonly mode: RailMode;
  /** The folds open: tree row ids, and `F:<path>` for a file's fold. */
  readonly open: ReadonlySet<string>;
  /** The tree's children read so far, by the row that holds them (`""` for the roots). */
  readonly tree: ReadonlyMap<string, readonly TreeRow[]>;
  /** The declarations the filter keeps, best first; `null` with no words typed. */
  readonly ranked: readonly { decl: DeclRow; marks: readonly number[] | null }[] | null;
}

/** The rail's rows (section 7): the overview first, then the tree, or each file's fold with its declarations. */
export function railRows(index: SetIndex, input: RailInput): RailRow[] {
  const rows: RailRow[] = [];
  if (input.ranked !== null) {
    for (const r of input.ranked) rows.push({ kind: "decl", k: `D${r.decl.key}`, d: 0, decl: r.decl, marks: r.marks });
    return rows;
  }
  rows.push({ kind: "overview", k: "overview", d: 0 });
  if (input.mode === "tree") {
    const walk = (under: string, d: number) => {
      for (const row of input.tree.get(under) ?? []) {
        const open = row.kids && input.open.has(row.id);
        rows.push({ kind: "tree", k: row.id, d, row, open });
        if (open) walk(row.id, d + 1);
      }
    };
    walk("", 0);
    return rows;
  }
  const byFile = new Map<string, DeclRow[]>();
  for (const d of index.decls) {
    if (!inMode(input.mode, d)) continue;
    let list = byFile.get(d.file);
    if (list === undefined) byFile.set(d.file, (list = []));
    list.push(d);
  }
  for (const f of index.files) {
    const list = byFile.get(f.path) ?? [];
    const k = `F:${f.path}`;
    // The opened file's fold stands open until it is shut; the others shut until opened.
    const open = f.depth === 0 ? !input.open.has(`${k}:shut`) : input.open.has(k);
    rows.push({ kind: "file", k, d: 0, path: f.path, count: list.length, open, reading: f.reading });
    if (open) for (const d of [...list].sort((a, b) => a.line - b.line)) rows.push({ kind: "decl", k: `D${d.key}`, d: 1, decl: d, marks: null });
  }
  return rows;
}

/** What the filter reads of a declaration: its prefixed name, its kind, and its documentation's first sentence. */
export const KIND_WORD: Record<DeclRow["kind"], string> = { el: "element", ct: "complex type", st: "simple type", gr: "group", ag: "attribute group", at: "attribute" };
export const filterText = (d: DeclRow): string => `${d.prefix}${d.local} ${KIND_WORD[d.kind]} ${d.words}`;
