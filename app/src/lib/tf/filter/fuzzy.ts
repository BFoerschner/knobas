/**
 * The fuzzy finder for a list of options the browser already holds (the view
 * editor's condition values): typed letters keep an option when they appear in
 * it in order, not only side by side.
 *
 * Pure. Not the filter bar's rule ({@link matchesAll}, whole words in any
 * order), which filters rows of prose, where letters scattered over a
 * sentence would keep nearly everything.
 */
import type { Mark } from "./filter";

/** One option the typed letters kept, and its text cut into runs, the matched letters marked. */
export interface FuzzyHit<T> {
  option: T;
  runs: Mark[];
}

/** What a matched letter is worth, and what its place adds or costs. */
const LETTER = 16;
/** Beside the letter matched before it. */
const BESIDE = 16;
/** At the start of a word. */
const WORD_START = 12;
/** At the start of the text, over a word's start. */
const TEXT_START = 4;
/** Per letter skipped between two matched ones. */
const SKIPPED = 2;

/** Whether a word starts at `at`: the text's start, after a space or a sign, or a capital after a small letter. */
function startsWord(text: string, at: number): boolean {
  if (at === 0) return true;
  const before = text[at - 1]!;
  if (!/[\p{L}\p{N}]/u.test(before)) return true;
  return /\p{Ll}/u.test(before) && /\p{Lu}/u.test(text[at]!);
}

/**
 * The best place for `word` in the text, its letters in order, or `null` when
 * they are not all there. Of every way the letters fit, the one that scores
 * highest: letters side by side, at a word's start and at the text's start
 * count for more, letters skipped between them for less. How far into the
 * text a match starts counts for nothing.
 */
function place(text: string, lower: string, word: string): { score: number; at: number[] } | null {
  const length = lower.length;
  if (word.length > length) return null;
  // `best[at]`: the score of the letters so far with the last of them at `at`.
  let best = new Float64Array(length).fill(-Infinity);
  const cameFrom: Int32Array[] = [];
  for (let letter = 0; letter < word.length; letter += 1) {
    const row = new Float64Array(length).fill(-Infinity);
    const back = new Int32Array(length).fill(-1);
    // The best earlier place that leaves a gap, kept as the loop passes it.
    let apart = -Infinity;
    let apartAt = -1;
    for (let at = letter; at < length; at += 1) {
      if (letter > 0 && at >= 2 && best[at - 2]! + SKIPPED * (at - 2) > apart) {
        apart = best[at - 2]! + SKIPPED * (at - 2);
        apartAt = at - 2;
      }
      if (lower[at] !== word[letter]) continue;
      const own = LETTER + (startsWord(text, at) ? WORD_START : 0);
      if (letter === 0) {
        row[at] = own + (at === 0 ? TEXT_START : 0);
        continue;
      }
      const beside = best[at - 1]! + BESIDE;
      const gapped = apart - SKIPPED * (at - 1);
      if (beside >= gapped) {
        if (beside === -Infinity) continue;
        row[at] = own + beside;
        back[at] = at - 1;
      } else {
        row[at] = own + gapped;
        back[at] = apartAt;
      }
    }
    cameFrom.push(back);
    best = row;
  }
  let end = -1;
  for (let at = 0; at < length; at += 1) {
    if (best[at]! > (end === -1 ? -Infinity : best[end]!)) end = at;
  }
  if (end === -1) return null;
  const at: number[] = [];
  for (let letter = word.length - 1, here = end; letter >= 0; here = cameFrom[letter]![here]!, letter -= 1) at.push(here);
  return { score: best[end]!, at };
}

/** `text` cut into runs, the letters at `at` marked. */
function runsOf(text: string, at: ReadonlySet<number>): Mark[] {
  const runs: Mark[] = [];
  for (let index = 0; index < text.length; index += 1) {
    const hit = at.has(index);
    const last = runs[runs.length - 1];
    if (last && last.hit === hit) last.text += text[index];
    else runs.push({ text: text[index]!, hit });
  }
  return runs.length === 0 ? [{ text, hit: false }] : runs;
}

/**
 * The options `typed` keeps, the closest first. Each word typed has to sit in
 * the option's text, its letters in order, ignoring case; options equally
 * close keep the order given, and nothing typed keeps them all as they came.
 */
export function fuzzyRank<T>(options: readonly T[], typed: string, textOf: (option: T) => string): FuzzyHit<T>[] {
  const words = typed.toLowerCase().split(/\s+/).filter(Boolean);
  const hits: (FuzzyHit<T> & { score: number })[] = [];
  for (const option of options) {
    const text = textOf(option);
    const lower = text.toLowerCase();
    // A letter whose small form is longer than itself would put every mark after it one place off.
    const aligned = lower.length === text.length;
    const at = new Set<number>();
    let score = 0;
    let kept = true;
    for (const word of words) {
      const found = place(aligned ? text : lower, lower, word);
      if (found === null) {
        kept = false;
        break;
      }
      score += found.score;
      if (aligned) for (const index of found.at) at.add(index);
    }
    if (kept) hits.push({ option, runs: runsOf(text, at), score });
  }
  if (words.length > 0) hits.sort((a, b) => b.score - a.score);
  return hits.map(({ option, runs }) => ({ option, runs }));
}
