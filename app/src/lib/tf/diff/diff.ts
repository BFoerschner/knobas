/**
 * The one diff component's pure half (#2266, spec #2234 step 6a): what
 * `TfDiffView` draws of a source's diff (`DiffFile`), in either layout, and
 * the two texts it highlights.
 *
 * - **Unified** draws the hunks' lines in order, a gap (`⋯`) before every
 *   hunk but the first.
 * - **Split** draws one row per pair: a context line on both sides; a run of
 *   removed lines beside the run of added lines that follows it, the shorter
 *   side padded.
 * - Every line carries `at`, its place among the file's lines (hunk by hunk),
 *   the index `placeThreads` (`pr/files.ts`) files threads under.
 * - **Sides**: each side's lines joined into one text (the old side its
 *   context and removed lines, the new side its context and added ones), so
 *   the highlighter carries its state from one hunk into the next and a
 *   string or comment that spans hunks stays one. The old side is named by
 *   the file's old path, so a rename that changes the language highlights
 *   each side in its own.
 */
import type { DiffFile, DiffLine } from "../../ipc/entity";
import type { PendingCommentStore } from "../../detail/view/pending-comments.svelte";

export type DiffLayout = "unified" | "split";

/** *Viewed* per file: which are, and how to change one. */
export interface DiffViewed {
  paths: readonly string[];
  set(path: string, on: boolean): void;
}

/** Line comments, where the source offers a review: whose they are and where they are kept until sent. */
export interface DiffReview {
  entity: string;
  pending: PendingCommentStore;
}

/**
 * A file as the component draws it: a caller may name it by a `label` in place of its path
 * (*Local changes*: "src/retry.rs (modified, staged)", the same file twice). The label keys
 * and titles the card; the path still picks the language and places comments.
 */
export type DrawnFile = DiffFile & { label?: string };

/** What tells a drawn file apart from the others: its label, else its path. */
export function keyOf(file: DrawnFile): string {
  return file.label ?? file.path;
}

/** A line of the file, and its place among the file's lines. */
export interface DrawnLine {
  line: DiffLine;
  at: number;
}

/** Before every hunk but the first: the lines between are not drawn, and the hunk's `@@` line says where it starts. */
export interface Gap {
  kind: "gap";
  header: string;
}

export type UnifiedRow = ({ kind: "line" } & DrawnLine) | Gap;

/** A split row: what each side draws, `null` for padding. */
export type SplitRow = { kind: "pair"; old: DrawnLine | null; new: DrawnLine | null } | Gap;

function hunksOf(file: DiffFile): DrawnLine[][] {
  let at = 0;
  return file.hunks.map((hunk) => hunk.lines.map((line) => ({ line, at: at++ })));
}

export function unifiedRows(file: DiffFile): UnifiedRow[] {
  return hunksOf(file).flatMap((lines, h) => [
    ...(h > 0 ? [{ kind: "gap", header: file.hunks[h]!.header } as const] : []),
    ...lines.map((drawn) => ({ kind: "line", ...drawn }) as const),
  ]);
}

export function splitRows(file: DiffFile): SplitRow[] {
  const rows: SplitRow[] = [];
  hunksOf(file).forEach((lines, h) => {
    if (h > 0) rows.push({ kind: "gap", header: file.hunks[h]!.header });
    let dels: DrawnLine[] = [];
    let adds: DrawnLine[] = [];
    const flush = () => {
      for (let n = 0; n < Math.max(dels.length, adds.length); n += 1) {
        rows.push({ kind: "pair", old: dels[n] ?? null, new: adds[n] ?? null });
      }
      dels = [];
      adds = [];
    };
    for (const drawn of lines) {
      if (drawn.line.kind === "context") {
        flush();
        rows.push({ kind: "pair", old: drawn, new: drawn });
      } else if (drawn.line.kind === "del") {
        // A removed line after added ones starts the next pair of runs.
        if (adds.length > 0) flush();
        dels.push(drawn);
      } else {
        adds.push(drawn);
      }
    }
    flush();
  });
  return rows;
}

/** One side's text to highlight, and the file name its language comes from. */
export interface SideText {
  path: string;
  text: string;
}

export interface Sides {
  old: SideText;
  new: SideText;
  /** For each of the file's lines (by `at`), its row in each side's text, `null` where the side lacks it. */
  index: { old: number | null; new: number | null }[];
}

export function sidesOf(file: DiffFile): Sides {
  const old: string[] = [];
  const nu: string[] = [];
  const index: Sides["index"] = [];
  for (const line of file.hunks.flatMap((hunk) => hunk.lines)) {
    const o = line.kind === "add" ? null : old.push(line.text) - 1;
    const n = line.kind === "del" ? null : nu.push(line.text) - 1;
    index.push({ old: o, new: n });
  }
  return {
    old: { path: file.old_path ?? file.path, text: old.join("\n") },
    new: { path: file.path, text: nu.join("\n") },
    index,
  };
}

/** What the window draws: split only where it was chosen and there is room for it. */
export function shownLayout(chosen: DiffLayout, narrow: boolean): DiffLayout {
  return narrow ? "unified" : chosen;
}
