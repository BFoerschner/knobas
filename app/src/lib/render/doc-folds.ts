/**
 * Folding a document by its lines (the XML view's design, 2026-11-23, sections 7 and 8). Pure, and on the page: the
 * worker hands over a {@link FoldTable} once, a few columns of the outline, and every fold after that is worked out
 * here, 7 to 14 ms at 800,000 lines.
 *
 * - **What folds**: an element that ends on a later line than it starts, a comment of several lines, and in JSON an
 *   object or an array. Where two start on one line, the outer one folds.
 * - **A fold is named by its first line** here; across versions of the file it is named by its element
 *   (`foldKey`, in the worker), since lines move.
 * - **Levels** count from the root, which is level 1. *n levels open* means every fold at level n + 1 and deeper is
 *   shut. `−` folds the deepest level that is open everywhere and `+` opens the shallowest level that is folded: only
 *   that one level's folds change, so a fold made by hand deeper down stays as it was.
 */

/** What can fold in a document, as the worker hands it over. Folds are in order of their first line. */
export interface FoldTable {
  /** How many lines the file holds. */
  readonly lines: number;
  /** For each line (from 1; index 0 unused), the fold that starts on it, or -1. */
  readonly byLine: Int32Array;
  /** Each fold's first and last line. */
  readonly line: Uint32Array;
  readonly end: Uint32Array;
  /** Each fold's level: the root 1. */
  readonly depth: Uint16Array;
  /** What each fold hides, counted, and in what: see {@link UNITS}. */
  readonly count: Uint32Array;
  readonly unit: Uint8Array;
  /** Each fold's closing tag (`</ORDER_ITEM>`, `-->`, `]`, `}`): an index into `closers`. */
  readonly closer: Uint32Array;
  readonly closers: readonly string[];
}

/** What a fold's count counts. */
export const UNITS = [
  ["element", "elements"],
  ["character", "characters"],
  ["line", "lines"],
  ["item", "items"],
  ["member", "members"],
] as const;
export const ELEMENTS = 0;
export const CHARACTERS = 1;
export const LINES = 2;
export const ITEMS = 3;
export const MEMBERS = 4;

/** The folds shut, by their first line. */
export type Folded = ReadonlySet<number>;

/** A table with nothing in it: a document before its outline lands. */
export function noFolds(lines: number): FoldTable {
  return {
    lines,
    byLine: new Int32Array(lines + 2).fill(-1),
    line: new Uint32Array(0),
    end: new Uint32Array(0),
    depth: new Uint16Array(0),
    count: new Uint32Array(0),
    unit: new Uint8Array(0),
    closer: new Uint32Array(0),
    closers: [],
  };
}

/** The lines left on screen with `folded` shut: a fold's own line stays, the rest of it goes. */
export function visibleLines(t: FoldTable, folded: Folded): Uint32Array {
  const out = new Uint32Array(t.lines);
  if (folded.size === 0) {
    for (let l = 1; l <= t.lines; l += 1) out[l - 1] = l;
    return out;
  }
  let n = 0;
  for (let l = 1; l <= t.lines; l += 1) {
    out[n++] = l;
    const f = t.byLine[l]!;
    if (f !== -1 && folded.has(l)) l = t.end[f]!;
  }
  return out.subarray(0, n);
}

/** The row a line stands in among `visible`: the line itself, or the fold that hides it. */
export function rowOfLine(visible: Uint32Array, line: number): number {
  let lo = 0;
  let hi = visible.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (visible[mid]! <= line) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** The deepest level at which anything folds (the root is level 1): how far *Levels* goes in this document. */
export function foldDepth(t: FoldTable): number {
  let most = 0;
  for (let f = 0; f < t.depth.length; f += 1) if (t.depth[f]! > most) most = t.depth[f]!;
  return most;
}

/** Every fold at level `levels + 1` or deeper: exactly `levels` levels open (the digit keys). */
export function foldedTo(t: FoldTable, levels: number): Set<number> {
  const out = new Set<number>();
  if (levels <= 0) return out;
  for (let f = 0; f < t.depth.length; f += 1) if (t.depth[f]! >= levels + 1) out.add(t.line[f]!);
  return out;
}

/** How many levels are open everywhere: one less than the shallowest fold shut, or every level where none is. */
export function levelsOpen(t: FoldTable, folded: Folded): number {
  let least = Infinity;
  for (const l of folded) {
    const f = t.byLine[l] ?? -1;
    if (f !== -1 && t.depth[f]! < least) least = t.depth[f]!;
  }
  return least === Infinity ? foldDepth(t) : least - 1;
}

/**
 * One level more (`by` 1) or one fewer (-1). More opens the shallowest level that is folded; fewer folds the deepest
 * level that is open everywhere, and stops at 1, the root with its children folded. Only that level's folds change.
 */
export function stepLevels(t: FoldTable, folded: Folded, by: 1 | -1): Set<number> {
  const n = levelsOpen(t, folded);
  const out = new Set(folded);
  if (by > 0) {
    for (const l of folded) {
      const f = t.byLine[l] ?? -1;
      if (f !== -1 && t.depth[f] === n + 1) out.delete(l);
    }
  } else if (n > 1) {
    for (let f = 0; f < t.depth.length; f += 1) if (t.depth[f] === n) out.add(t.line[f]!);
  }
  return out;
}

/** Every fold from `line` to its end, its own included: what `Shift ←`, `Shift →` and Alt on a caret change. */
export function foldsUnder(t: FoldTable, line: number): number[] {
  const f = t.byLine[line] ?? -1;
  if (f === -1) return [];
  const out: number[] = [];
  for (let l = line; l <= t.end[f]!; l += 1) if (t.byLine[l] !== -1) out.push(l);
  return out;
}

/** The line that opens what holds `line`: the innermost fold that starts above it and reaches it; 0 for none. */
export function holderOf(t: FoldTable, line: number): number {
  for (let l = line - 1; l >= 1; l -= 1) {
    const f = t.byLine[l]!;
    if (f !== -1 && t.end[f]! >= line) return l;
  }
  return 0;
}

/** `folded` less every fold that hides `line`: a step to a line inside a fold opens what hides it. */
export function opening(t: FoldTable, folded: Folded, line: number): Folded {
  let out: Set<number> | null = null;
  for (const l of folded) {
    const f = t.byLine[l] ?? -1;
    if (f !== -1 && l < line && t.end[f]! >= line) {
      out ??= new Set(folded);
      out.delete(l);
    }
  }
  return out ?? folded;
}

/** Whether a line is hidden inside a fold that is shut. */
export function hidden(t: FoldTable, folded: Folded, line: number): boolean {
  return opening(t, folded, line) !== folded;
}

/** How many of `lines` (sorted) lie inside fold `f` below its first line: what a shut fold counts of matches. */
export function countInside(t: FoldTable, f: number, lines: Uint32Array | readonly number[]): number {
  const from = t.line[f]!;
  const to = t.end[f]!;
  let lo = 0;
  let hi = lines.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid]! <= from) lo = mid + 1;
    else hi = mid;
  }
  let n = 0;
  for (let i = lo; i < lines.length && lines[i]! <= to; i += 1) n += 1;
  return n;
}
