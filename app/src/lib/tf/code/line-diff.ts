/**
 * The diff of two versions of one file, worked out here rather than read from
 * a source: what an edit changes before it is committed (#845, R-8.6), drawn
 * by `TfDiffView` exactly as a source's diff is. Prototype-6's `lineDiff` and
 * `hunks`: the longest common run of lines, then one hunk per run of changes
 * with three lines of context around it.
 *
 * The lines both versions start and end with are matched first; the table is
 * lines × lines of the changed middle only, and a middle past
 * `TABLE_BOUND` cells is drawn removed whole, then added whole (#2270: a
 * pipeline's two outputs are diffed here, and can be long). Such a diff is
 * marked `coarse`, and the card says so.
 */
import type { DiffFile, DiffLine } from "../../ipc/entity";

/** How many unchanged lines a hunk keeps on each side of a change. */
const CONTEXT = 3;
/** The most cells the table of the changed middle may hold (#2270: a pipeline's output can be long). */
const TABLE_BOUND = 4_000_000;

function split(text: string): string[] {
  return text === "" ? [] : text.split("\n");
}

/** `before` → `after` as one file of the source's diff shape; no hunk where nothing changed. */
export function lineDiff(path: string, before: string, after: string): DiffFile & { coarse?: true } {
  const a = split(before);
  const b = split(after);
  const lines: DiffLine[] = [];
  let coarse = false;
  // The lines both sides open and close with need no table: only the changed middle does.
  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head += 1;
  let tail = 0;
  while (tail < a.length - head && tail < b.length - head && a[a.length - 1 - tail] === b[b.length - 1 - tail]) tail += 1;
  for (let k = 0; k < head; k += 1) lines.push({ kind: "context", old: k + 1, new: k + 1, text: a[k]! });
  const midA = a.slice(head, a.length - tail);
  const midB = b.slice(head, b.length - tail);
  if ((midA.length + 1) * (midB.length + 1) > TABLE_BOUND) {
    // Too wide a middle for the table: removed whole, then added whole. A true diff, if not the least one.
    coarse = true;
    midA.forEach((text, k) => lines.push({ kind: "del", old: head + k + 1, new: null, text }));
    midB.forEach((text, k) => lines.push({ kind: "add", old: null, new: head + k + 1, text }));
  } else {
    const lcs = Array.from({ length: midA.length + 1 }, () => new Uint32Array(midB.length + 1));
    for (let i = midA.length - 1; i >= 0; i -= 1)
      for (let j = midB.length - 1; j >= 0; j -= 1)
        lcs[i]![j] = midA[i] === midB[j] ? lcs[i + 1]![j + 1]! + 1 : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!);
    let i = 0;
    let j = 0;
    while (i < midA.length || j < midB.length) {
      if (i < midA.length && j < midB.length && midA[i] === midB[j]) {
        i += 1;
        j += 1;
        lines.push({ kind: "context", old: head + i, new: head + j, text: midA[i - 1]! });
      } else if (j >= midB.length || (i < midA.length && lcs[i + 1]![j]! >= lcs[i]![j + 1]!)) {
        i += 1;
        lines.push({ kind: "del", old: head + i, new: null, text: midA[i - 1]! });
      } else {
        j += 1;
        lines.push({ kind: "add", old: null, new: head + j, text: midB[j - 1]! });
      }
    }
  }
  for (let k = tail; k > 0; k -= 1)
    lines.push({ kind: "context", old: a.length - k + 1, new: b.length - k + 1, text: a[a.length - k]! });
  const near = lines.map((_, at) =>
    lines.slice(Math.max(0, at - CONTEXT), at + CONTEXT + 1).some((line) => line.kind !== "context"),
  );
  const runs: { lines: DiffLine[]; old: number; new: number }[] = [];
  // The last line of each side before the line at hand: where a hunk with none of that side sits.
  let oldBefore = 0;
  let newBefore = 0;
  lines.forEach((line, at) => {
    if (near[at]) {
      if (at === 0 || !near[at - 1]) runs.push({ lines: [], old: oldBefore, new: newBefore });
      runs[runs.length - 1]!.lines.push(line);
    }
    oldBefore = line.old ?? oldBefore;
    newBefore = line.new ?? newBefore;
  });
  // `@@ -a,b +c,d @@` as git prints it: a side with no lines in the hunk starts after the line before it.
  const range = (numbers: number[], before: number) => `${numbers.length === 0 ? before : numbers[0]},${numbers.length}`;
  const hunks: DiffFile["hunks"] = runs.map((run) => ({
    header: `@@ -${range(run.lines.flatMap((l) => (l.old === null ? [] : [l.old])), run.old)} +${range(run.lines.flatMap((l) => (l.new === null ? [] : [l.new])), run.new)} @@`,
    lines: run.lines,
  }));
  return {
    path,
    old_path: null,
    change: before === "" ? "added" : "modified",
    additions: lines.filter((line) => line.kind === "add").length,
    deletions: lines.filter((line) => line.kind === "del").length,
    hunks,
    truncated: false,
    threads: [],
    ...(coarse ? { coarse: true as const } : {}),
  };
}
