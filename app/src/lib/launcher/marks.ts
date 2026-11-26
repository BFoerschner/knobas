/**
 * The matched letters of a launcher row's title (the fuzzy-search decisions, 2026-11-19, D9): the backend says where,
 * as pairs of start and end in UTF-16 code units of the hit's title (`SearchHit.marks`), and the row paints them with
 * the CSS Custom Highlight API, as the find bar paints its matches (`tf/find-text.ts`). The title stays one text node,
 * so a row whose marks change is not built again: a highlight is paint only, and nothing moves once drawn.
 *
 * Where a webview has no Highlight API (`canMark()`), the rows draw without marks.
 */
import { canMark } from "../tf/find-text";

/** The one highlight every row's ranges join, registered on first use. */
let shared: Highlight | null = null;

function highlight(): Highlight {
  if (shared === null || CSS.highlights.get("tf-pick") !== shared) {
    shared = new Highlight();
    CSS.highlights.set("tf-pick", shared);
  }
  return shared;
}

/**
 * `marks` of the hit's title `full`, moved into the title the row draws, `drawn`: the same text, the text with a
 * leading part moved out (a repository's owner goes to the key cell), or the text with a part put before it (a pull
 * request's number). A pair outside what is drawn is cut to it or dropped.
 */
export function marksIn(marks: readonly number[] | undefined, full: string, drawn: string): number[] {
  if (!marks || marks.length < 2) return [];
  let shift: number;
  if (drawn === full) shift = 0;
  else if (full.endsWith(drawn)) shift = drawn.length - full.length;
  else if (drawn.endsWith(full)) shift = drawn.length - full.length;
  else return [];
  const out: number[] = [];
  for (let at = 0; at + 1 < marks.length; at += 2) {
    const start = Math.max(0, marks[at]! + shift);
    const end = Math.min(drawn.length, marks[at + 1]! + shift);
    if (end > start) out.push(start, end);
  }
  return out;
}

/**
 * An attachment for the title's span: one `Range` a pair over its text node, added to the `tf-pick` highlight and
 * taken out again when the marks or the title change or the row goes. Reads `read` reactively, so a new answer moves
 * the ranges and leaves the span and its text node as they were.
 */
export function markTitle(read: () => { title: string; marks: readonly number[] }) {
  return (span: HTMLElement) => {
    const { title, marks } = read();
    if (marks.length < 2 || !canMark()) return;
    const text = span.firstChild;
    if (!(text instanceof Text) || title.length === 0) return;
    const ranges: Range[] = [];
    for (let at = 0; at + 1 < marks.length; at += 2) {
      const start = marks[at]!;
      const end = marks[at + 1]!;
      if (end > text.length || end <= start) continue;
      const range = new Range();
      range.setStart(text, start);
      range.setEnd(text, end);
      ranges.push(range);
    }
    const into = highlight();
    for (const range of ranges) into.add(range);
    return () => {
      for (const range of ranges) into.delete(range);
    };
  };
}
