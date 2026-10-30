/**
 * What a file viewer line is drawn from (#2254, #2299): its text cut by the
 * highlighter's tokens and by *Find in file*'s hits, and where the hits are.
 * Hits are counted as the run log's *Find in log* counts them (`marks`): a
 * run of matched characters is one hit.
 */
import type { LineTokens } from "../../render/port";
import { markNeedles, words } from "../filter/filter";

/** A piece of a line: its text, its token kind (`""` plain), and which of the line's hits it is part of. */
export interface Segment {
  text: string;
  kind: string;
  /** The hit's number within the line, from 0; `null` outside any hit. */
  hit: number | null;
}

/** Find's needles for what was typed (words of two characters and more). */
export function needles(typed: string): string[] {
  return words(typed).filter((word) => word.length >= 2);
}

/**
 * Line `text` as segments, or `null` when it has neither tokens nor hits (so
 * the viewer draws its text as it is). Tokens that do not add up to the text
 * (a stale answer) are ignored.
 */
export function segments(text: string, tokens: LineTokens | undefined, found: readonly string[]): Segment[] | null {
  if (text === "") return null;
  const runs = found.length === 0 || !found.some((n) => text.toLowerCase().includes(n)) ? null : markNeedles(text, found);
  const usable = tokens !== undefined && tokens.map(([t]) => t).join("") === text ? tokens : undefined;
  if (runs === null && usable === undefined) return null;
  // Where each cut falls: token edges and hit edges, merged.
  const kinds = usable ?? [[text, ""] as const];
  const hits = runs ?? [{ text, hit: false }];
  const out: Segment[] = [];
  let ti = 0, tOff = 0, hi = 0, hOff = 0, hitNo = -1;
  if (hits[0]?.hit) hitNo = 0;
  while (ti < kinds.length && hi < hits.length) {
    const [tText, kind] = kinds[ti]!;
    const h = hits[hi]!;
    const take = Math.min(tText.length - tOff, h.text.length - hOff);
    if (take > 0) out.push({ text: tText.slice(tOff, tOff + take), kind, hit: h.hit ? hitNo : null });
    tOff += take;
    hOff += take;
    if (tOff >= tText.length) (ti += 1), (tOff = 0);
    if (hOff >= h.text.length) {
      hi += 1;
      hOff = 0;
      if (hits[hi]?.hit) hitNo += 1;
    }
  }
  return out;
}

/** Every hit in a file: the lines that hold one, in order, and the number of each line's first hit. */
export interface FileHits {
  total: number;
  /** Line indexes (from 0) holding a hit, ascending. */
  lines: number[];
  /** `starts[i]`: the number (from 0) of `lines[i]`'s first hit. */
  starts: number[];
}

export function fileHits(lines: readonly string[], found: readonly string[]): FileHits {
  const hits: FileHits = { total: 0, lines: [], starts: [] };
  if (found.length === 0) return hits;
  for (let i = 0; i < lines.length; i += 1) {
    const text = lines[i]!;
    const lower = text.toLowerCase();
    if (!found.some((n) => lower.includes(n))) continue;
    const count = markNeedles(text, found).filter((run) => run.hit).length;
    hits.lines.push(i);
    hits.starts.push(hits.total);
    hits.total += count;
  }
  return hits;
}

/** The line index holding hit `n`. */
export function lineOfHit(hits: FileHits, n: number): number {
  let lo = 0, hi = hits.lines.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (hits.starts[mid]! <= n) lo = mid;
    else hi = mid - 1;
  }
  return hits.lines[lo] ?? -1;
}

/** The number of line `line`'s first hit, or `undefined` for a line with none. */
export function firstHitOn(hits: FileHits, line: number): number | undefined {
  let lo = 0, hi = hits.lines.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const at = hits.lines[mid]!;
    if (at === line) return hits.starts[mid];
    if (at < line) lo = mid + 1;
    else hi = mid - 1;
  }
  return undefined;
}
