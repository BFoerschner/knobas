/**
 * The diff of two versions of one file, worked out here rather than read from
 * a source: what an edit changes before it is committed (#845, R-8.6), drawn
 * by `TfDiffView` exactly as a source's diff is. Prototype-6's `lineDiff` and
 * `hunks`: the longest common run of lines, then one hunk per run of changes
 * with three lines of context around it.
 *
 * The table is lines × lines, so this is for a file a reader edits by hand,
 * which a source's own cap already bounds; it is not a general diff.
 */
import type { DiffFile, DiffLine } from "../../ipc/entity";

/** How many unchanged lines a hunk keeps on each side of a change. */
const CONTEXT = 3;

function split(text: string): string[] {
  return text === "" ? [] : text.split("\n");
}

/** `before` → `after` as one file of the source's diff shape; no hunk where nothing changed. */
export function lineDiff(path: string, before: string, after: string): DiffFile {
  const a = split(before);
  const b = split(after);
  const lcs = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i -= 1)
    for (let j = b.length - 1; j >= 0; j -= 1)
      lcs[i]![j] = a[i] === b[j] ? lcs[i + 1]![j + 1]! + 1 : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
  const lines: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) {
      i += 1;
      j += 1;
      lines.push({ kind: "context", old: i, new: j, text: a[i - 1]! });
    } else if (j >= b.length || (i < a.length && lcs[i + 1]![j]! >= lcs[i]![j + 1]!)) {
      i += 1;
      lines.push({ kind: "del", old: i, new: null, text: a[i - 1]! });
    } else {
      j += 1;
      lines.push({ kind: "add", old: null, new: j, text: b[j - 1]! });
    }
  }
  const near = lines.map((_, at) =>
    lines.slice(Math.max(0, at - CONTEXT), at + CONTEXT + 1).some((line) => line.kind !== "context"),
  );
  const hunks: DiffFile["hunks"] = [];
  lines.forEach((line, at) => {
    if (!near[at]) return;
    if (at === 0 || !near[at - 1]) hunks.push({ header: `@@ -${line.old ?? 0} +${line.new ?? 0} @@`, lines: [] });
    hunks[hunks.length - 1]!.lines.push(line);
  });
  return {
    path,
    old_path: null,
    change: before === "" ? "added" : "modified",
    additions: lines.filter((line) => line.kind === "add").length,
    deletions: lines.filter((line) => line.kind === "del").length,
    hunks,
    truncated: false,
    threads: [],
  };
}
