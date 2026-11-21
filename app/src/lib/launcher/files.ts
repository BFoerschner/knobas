/**
 * **A repository's files in the launcher** (the everything-search decisions, 2026-11-20, D10 and D11): a file's path
 * with the letters the words cover marked, its name apart from its folders, and the lines the pane quotes with the
 * indent they share dropped. Pure.
 */
import type { Excerpt, FilesFound, FilesInRepo, Segment } from "../ipc";

/** A path's piece, marked or not, and whether it is the file's name (drawn in `--ink`). */
export interface PathPiece extends Segment {
  name: boolean;
}

/**
 * `text` with the letters `words` cover flagged, as `code_text::marks` flags them: each word from a run's start or a
 * capital inside it, `_` and `-` skipped, over as many letters as the word has.
 */
export function marksIn(text: string, words: readonly string[]): Segment[] {
  const asked = words.map((word) => word.toLowerCase().replace(/[_-]/g, "")).filter((word) => word.length >= 2);
  const out: Segment[] = [];
  const lower = text.toLowerCase();
  const starts = (i: number) => i === 0 || !/[\p{L}\p{N}]/u.test(text[i - 1]!) || (/[\p{Lu}]/u.test(text[i]!) && /[\p{Ll}\p{N}]/u.test(text[i - 1]!));
  let at = 0;
  while (at < text.length) {
    let found: [number, number] | null = null;
    for (const word of asked) {
      for (let i = at; i < text.length && (found === null || i < found[0]); i++) {
        if (!starts(i)) continue;
        let j = i;
        let k = 0;
        while (j < text.length && k < word.length) {
          if (lower[j] === "_" || lower[j] === "-") j++;
          else if (lower[j] === word[k]) (j++, k++);
          else break;
        }
        if (k === word.length) {
          found = [i, j];
          break;
        }
      }
    }
    if (found === null) break;
    if (found[0] > at) out.push({ text: text.slice(at, found[0]), hit: false });
    out.push({ text: text.slice(found[0], found[1]), hit: true });
    at = found[1];
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: false });
  return out;
}

/** The words an excerpt marks, distinct and lower-cased. */
export function markedWords(excerpt: Pick<Excerpt, "segments">): string[] {
  return [...new Set(excerpt.segments.filter((seg) => seg.hit).map((seg) => seg.text.toLowerCase()))];
}

/** A file's path as the pane draws it: the folders, then the file's name, the words marked in both. */
export function pathPieces(path: string, words: readonly string[]): PathPiece[] {
  const cut = path.lastIndexOf("/") + 1;
  const out: PathPiece[] = [];
  let at = 0;
  for (const seg of marksIn(path, words)) {
    const end = at + seg.text.length;
    if (at < cut && end > cut) {
      out.push({ text: seg.text.slice(0, cut - at), hit: seg.hit, name: false });
      out.push({ text: seg.text.slice(cut - at), hit: seg.hit, name: true });
    } else out.push({ ...seg, name: at >= cut });
    at = end;
  }
  return out;
}

/** The lines the pane quotes of a file: the one before, the line, the one after, the indent they share dropped; a blank neighbour left out. */
export function quotedLines(excerpt: Excerpt): { before: string | null; line: Segment[]; after: string | null } {
  const file = excerpt.file;
  const text = excerpt.segments.map((seg) => seg.text).join("");
  const before = file?.before && file.before.trim() !== "" ? file.before : null;
  const after = file?.after && file.after.trim() !== "" ? file.after : null;
  const indent = (line: string) => line.length - line.trimStart().length;
  const shared = Math.min(...[text, before, after].filter((line): line is string => line !== null && line.trim() !== "").map(indent));
  const drop = Number.isFinite(shared) ? shared : 0;
  // The line's segments with its first `drop` characters taken off.
  const line: Segment[] = [];
  let left = drop;
  for (const seg of excerpt.segments) {
    if (left >= seg.text.length) {
      left -= seg.text.length;
      continue;
    }
    line.push({ text: seg.text.slice(left), hit: seg.hit });
    left = 0;
  }
  return { before: before?.slice(drop) ?? null, line, after: after?.slice(drop) ?? null };
}

/** *1 file*, *4 files*. */
const counted = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/**
 * The pane's count line (D11): *In 4 files of main, 3 of them by name.*, *In 1 file of main.*, *In 37 files of main.
 * The first 8 are shown.*, *3 files of main are named by it.* `more` is the pane's own read, where it has landed.
 */
export function countLine(found: FilesFound, more: FilesInRepo | null, shown: number): string {
  const branch = found.branch || "the default branch";
  if (found.by_text === 0) return `${counted(found.by_name, "file", "files")} of ${branch} ${found.by_name === 1 ? "is" : "are"} named by it.`;
  const head = `In ${counted(found.by_text, "file", "files")} of ${branch}`;
  const named = found.by_name > 0 ? `, ${found.by_name === found.by_text && found.by_name > 1 ? "all" : found.by_name} of them by name` : "";
  const first = more?.more ? ` The first ${shown} are shown.` : "";
  return `${head}${named}.${first}`;
}
