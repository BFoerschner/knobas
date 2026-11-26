/**
 * A line of a Docs code block cut by the highlighter's tokens and by the find bar's marks (ADR-0051): each run its
 * text, its role (`""` plain) and whether it is marked. Tokens that do not add up to the line (an answer for other
 * text) are left out, so the line draws plain with its marks.
 */
import type { LineTokens } from "../../render/port";
import { markText } from "./docs";

export interface CodeRun {
  text: string;
  kind: string;
  hit: boolean;
}

export function codeRuns(line: string, tokens: LineTokens | undefined, stems: string[]): CodeRun[] {
  if (line === "") return [];
  const marks = markText(line, stems);
  const usable = tokens !== undefined && tokens.map(([t]) => t).join("") === line ? tokens : [[line, ""] as const];
  const out: CodeRun[] = [];
  let ti = 0;
  let tOff = 0;
  let mi = 0;
  let mOff = 0;
  while (ti < usable.length && mi < marks.length) {
    const [text, kind] = usable[ti]!;
    const mark = marks[mi]!;
    const take = Math.min(text.length - tOff, mark.text.length - mOff);
    if (take > 0) {
      const last = out.at(-1);
      const piece = text.slice(tOff, tOff + take);
      if (last && last.kind === kind && last.hit === mark.hit) last.text += piece;
      else out.push({ text: piece, kind, hit: mark.hit });
    }
    tOff += take;
    mOff += take;
    if (tOff >= text.length) (ti += 1), (tOff = 0);
    if (mOff >= mark.text.length) (mi += 1), (mOff = 0);
  }
  return out;
}
