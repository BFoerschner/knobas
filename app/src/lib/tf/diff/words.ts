/**
 * Word-level changes (#2267, spec #2234 story 61): inside a changed line pair
 * (a removed line and the added line it is drawn beside), the words that
 * differ, so a one-character change is visible.
 *
 * - A line is cut into words (`\w` runs), runs of space, and single other
 *   characters; the two lines' words are lined up by their longest common
 *   subsequence, and what is left over on each side is marked. Neighbouring
 *   leftovers are one mark.
 * - Not marked word by word: a pair with no word in common (the whole line
 *   changed, which the line's own colour says already), and a pair where
 *   either line is longer than {@link LONG} characters (the lineup costs the
 *   product of the two lengths).
 */
import type { LineTokens } from "../../render/port";

/** Lines longer than this are not marked word by word. */
export const LONG = 1_000;

/** `[from, to)` character offsets into a line. */
export type Range = [number, number];

const WORD = /\w+|\s+|[^\w\s]/gu;

/** The changed words of `old` and of `nu`, as ranges into each. */
export function wordMarks(old: string, nu: string): [Range[], Range[]] {
  if (old.length > LONG || nu.length > LONG || old === nu) return [[], []];
  const a = old.match(WORD) ?? [];
  const b = nu.match(WORD) ?? [];
  // lcs[i][j]: the common subsequence of a[i..] and b[j..].
  const lcs = Array.from({ length: a.length + 1 }, () => new Uint16Array(b.length + 1));
  for (let i = a.length - 1; i >= 0; i -= 1)
    for (let j = b.length - 1; j >= 0; j -= 1)
      lcs[i]![j] = a[i] === b[j] ? lcs[i + 1]![j + 1]! + 1 : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
  const keptA = new Array<boolean>(a.length).fill(false);
  const keptB = new Array<boolean>(b.length).fill(false);
  let shared = false;
  for (let i = 0, j = 0; i < a.length && j < b.length; ) {
    if (a[i] === b[j]) {
      keptA[i] = keptB[j] = true;
      if (a[i]!.trim() !== "") shared = true;
      i += 1;
      j += 1;
    } else if (lcs[i + 1]![j]! >= lcs[i]![j + 1]!) i += 1;
    else j += 1;
  }
  if (!shared) return [[], []];
  return [ranges(a, keptA), ranges(b, keptB)];
}

function ranges(words: readonly string[], kept: readonly boolean[]): Range[] {
  const out: Range[] = [];
  let at = 0;
  words.forEach((word, n) => {
    const end = at + word.length;
    if (!kept[n]) {
      const last = out[out.length - 1];
      if (last && last[1] === at) last[1] = end;
      else out.push([at, end]);
    }
    at = end;
  });
  return out;
}

/** A piece of a drawn line: its text, its highlight kind (`""` for none), and whether it is a changed word. */
export interface Marked {
  text: string;
  kind: string;
  mark: boolean;
}

/** Line `text` cut at its tokens' edges and its marks' edges. Tokens that do not add up to the text are ignored. */
export function marked(text: string, tokens: LineTokens | undefined, marks: readonly Range[]): Marked[] {
  const kinds: LineTokens = tokens && tokens.map(([part]) => part).join("") === text ? tokens : [[text, ""]];
  const out: Marked[] = [];
  let at = 0;
  for (const [part, kind] of kinds) {
    const end = at + part.length;
    // The cuts inside this token: its edges and every mark edge between them.
    const cuts = [at, ...marks.flat().filter((edge) => edge > at && edge < end), end];
    for (let n = 0; n + 1 < cuts.length; n += 1) {
      const [from, to] = [cuts[n]!, cuts[n + 1]!];
      if (from === to) continue;
      const mark = marks.some(([m, e]) => m <= from && to <= e);
      out.push({ text: text.slice(from, to), kind, mark });
    }
    at = end;
  }
  return out;
}
