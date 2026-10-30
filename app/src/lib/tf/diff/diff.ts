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
import type { DiffFile, DiffLine, LineThread, PipeRun, PipeStep } from "../../ipc/entity";
import type { PendingCommentStore } from "../../detail/view/pending-comments.svelte";
import { wordMarks, type Range } from "./words";

export type DiffLayout = "unified" | "split";

/**
 * The most lines one diff carries over all its files: the parser's safety
 * stop (`knobas_source::diff::MAX_LINES_PER_VIEW`, #2269), named where a
 * diff hits it.
 */
export const DIFF_CEILING = 50_000;

/** How many hidden lines one *Expand* shows. */
export const EXPAND_STEP = 20;

/** Expanded context's reader (#2267): a file's whole text on one side, at that side's commit, through the read cache; `null` where it cannot be read whole. */
export interface DiffContext {
  text(file: DiffFile, side: "old" | "new"): Promise<string | null>;
  /**
   * A side's bytes, for a rendered diff of a binary file (#2279): `null` where the file is not on
   * that side (a binary diff does not say whether it was added or deleted). Absent: none offered.
   */
  bytes?(file: DiffFile, side: "old" | "new"): Promise<Uint8Array | null>;
  /**
   * A side run through a file pipeline's `steps` (#2270, `entityFilePiped`): `null` where the file is not on
   * that side. Absent: this diff runs no pipelines.
   */
  piped?(file: DiffFile, side: "old" | "new", steps: PipeStep[]): Promise<PipeRun | null>;
}

/** *Viewed* per file: which are, and how to change one. */
export interface DiffViewed {
  paths: readonly string[];
  set(path: string, on: boolean): void;
}

/** Line comments, where the source offers a review: whose they are and where they are kept until sent. */
export interface DiffReview {
  entity: string;
  pending: PendingCommentStore;
  /** Queue a reply to `thread` on `path` (#2277); absent where the source takes none. */
  reply?: ((path: string, thread: LineThread, body: string) => Promise<boolean>) | null;
  /** Queue a resolve (`on`) or reopen of `thread` (#2277); absent where the source cannot (Forgejo). */
  resolve?: ((thread: LineThread, on: boolean) => Promise<boolean>) | null;
  /** The replies queued this session, not yet in the read, by thread id. */
  queued?: (thread: LineThread) => readonly string[];
  /** Whether `thread` reads resolved, a queued resolve or reopen counted. */
  resolved?: (thread: LineThread) => boolean;
}

/**
 * A file as the component draws it: a caller may name it by a `label` in place of its path
 * (*Local changes*: "src/retry.rs (modified, staged)", the same file twice). The label keys
 * and titles the card; the path still picks the language and places comments.
 */
export type DrawnFile = DiffFile & {
  label?: string;
  /** Worked out here (`lineDiff`) with a changed middle too long to match line by line: drawn removed whole, then added whole. */
  coarse?: true;
};

/** What tells a drawn file apart from the others: its label, else its path. */
export function keyOf(file: DrawnFile): string {
  return file.label ?? file.path;
}

/**
 * A line of the file, and its place among the file's lines. An `extra` line
 * is context the reader expanded, read from the file: not in the source's
 * diff, so `at` is -1 and it takes no comment.
 */
export interface DrawnLine {
  line: DiffLine;
  at: number;
  extra?: boolean;
}

/**
 * Before hunk `hunk`: the lines between it and the one before are not drawn,
 * and its `@@` line says where it starts. `hidden` is how many are still not
 * drawn, `null` where the headers do not say.
 */
export interface Gap {
  kind: "gap";
  header: string;
  hunk: number;
  hidden: number | null;
}

export type UnifiedRow = ({ kind: "line" } & DrawnLine) | Gap;

/** A split row: what each side draws, `null` for padding. */
export type SplitRow = { kind: "pair"; old: DrawnLine | null; new: DrawnLine | null } | Gap;

/** How much of the context before a hunk the reader expanded: `top` lines down from the hunk above, `bottom` up from this one. */
export interface Shown {
  top: number;
  bottom: number;
}

/** Expanded context: the file's lines after the change, and what is shown before each hunk (by its index). */
export interface Expanded {
  lines: readonly string[];
  shown: Readonly<Record<number, Shown>>;
}

/** Hidden lines before a hunk: the first one's number on each side, and how many. */
export interface Hidden {
  old: number;
  new: number;
  count: number;
}

const HEADER = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/;

/** A hunk's first line and the line after it, on each side; `null` for a header that says neither. */
function spanOf(header: string) {
  const m = HEADER.exec(header);
  if (m === null) return null;
  // `-5,0`: nothing of this side in the hunk, which sits after line 5.
  const side = (start: string, len: string | undefined) => {
    const [a, b] = [Number(start), len === undefined ? 1 : Number(len)];
    return b === 0 ? { first: a + 1, after: a + 1 } : { first: a, after: a + b };
  };
  return { old: side(m[1]!, m[2]), new: side(m[3]!, m[4]) };
}

/** For each hunk, the context hidden before it (from the top of the file, for the first); `null` where nothing is or the headers do not say. */
export function hiddenBefore(file: DiffFile): (Hidden | null)[] {
  const spans = file.hunks.map((hunk) => spanOf(hunk.header));
  return spans.map((span, h) => {
    const prev = h === 0 ? { old: { after: 1 }, new: { after: 1 } } : spans[h - 1];
    if (!span || !prev) return null;
    const count = span.new.first - prev.new.after;
    return count > 0 ? { old: prev.old.after, new: prev.new.after, count } : null;
  });
}

/** The rows hunk by hunk: each gap, then the hunk's lines, with the context the reader expanded around the gap. */
function sectionsOf(file: DiffFile, expanded?: Expanded): (Gap | DrawnLine)[] {
  const hidden = hiddenBefore(file);
  const out: (Gap | DrawnLine)[] = [];
  let at = 0;
  file.hunks.forEach((hunk, h) => {
    const gap = hidden[h] ?? null;
    const shown = expanded?.shown[h] ?? { top: 0, bottom: 0 };
    const top = gap ? Math.min(shown.top, gap.count) : 0;
    const bottom = gap ? Math.min(shown.bottom, gap.count - top) : 0;
    const extra = (from: number, n: number) => {
      for (let k = 0; k < n; k += 1) {
        const nu = gap!.new + from + k;
        const line: DiffLine = { kind: "context", old: gap!.old + from + k, new: nu, text: expanded?.lines[nu - 1] ?? "" };
        out.push({ line, at: -1, extra: true });
      }
    };
    extra(0, top);
    const left = gap === null ? null : gap.count - top - bottom;
    // A gap is drawn before every hunk but the first, and before the first where it can be expanded; never once expanded away.
    if ((h > 0 || (expanded && left !== null && left > 0)) && !(expanded && left === 0 && (top > 0 || bottom > 0)))
      out.push({ kind: "gap", header: hunk.header, hunk: h, hidden: left });
    if (gap) extra(gap.count - bottom, bottom);
    for (const line of hunk.lines) out.push({ line, at: at++ });
  });
  return out;
}

const isGap = (item: Gap | DrawnLine): item is Gap => "kind" in item;

export function unifiedRows(file: DiffFile, expanded?: Expanded): UnifiedRow[] {
  return sectionsOf(file, expanded).map((item) => (isGap(item) ? item : { kind: "line", ...item }));
}

export function splitRows(file: DiffFile, expanded?: Expanded): SplitRow[] {
  const rows: SplitRow[] = [];
  let dels: DrawnLine[] = [];
  let adds: DrawnLine[] = [];
  const flush = () => {
    for (let n = 0; n < Math.max(dels.length, adds.length); n += 1) {
      rows.push({ kind: "pair", old: dels[n] ?? null, new: adds[n] ?? null });
    }
    dels = [];
    adds = [];
  };
  for (const item of sectionsOf(file, expanded)) {
    if (isGap(item)) {
      flush();
      rows.push(item);
    } else if (item.line.kind === "context") {
      flush();
      rows.push({ kind: "pair", old: item, new: item });
    } else if (item.line.kind === "del") {
      // A removed line after added ones starts the next pair of runs.
      if (adds.length > 0) flush();
      dels.push(item);
    } else {
      adds.push(item);
    }
  }
  flush();
  return rows;
}

/** The changed words of each removed line and the added line it pairs with (as split draws them), by `at`. */
export function changedWords(file: DiffFile): Map<number, Range[]> {
  const marks = new Map<number, Range[]>();
  for (const row of splitRows(file)) {
    if (row.kind !== "pair" || row.old === null || row.new === null || row.old === row.new) continue;
    const [old, nu] = wordMarks(row.old.line.text, row.new.line.text);
    if (old.length > 0) marks.set(row.old.at, old);
    if (nu.length > 0) marks.set(row.new.at, nu);
  }
  return marks;
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
  /** A side that is its whole file: its rows are its line numbers less one, expanded lines included. */
  whole: { old: boolean; new: boolean };
}

/** The whole file on each side, where it was read for expanded context. */
export interface WholeTexts {
  old: string | null;
  new: string | null;
}

export function sidesOf(file: DiffFile, whole?: WholeTexts): Sides {
  const old: string[] = [];
  const nu: string[] = [];
  const index: Sides["index"] = [];
  const hasOld = typeof whole?.old === "string";
  const hasNew = typeof whole?.new === "string";
  for (const line of file.hunks.flatMap((hunk) => hunk.lines)) {
    const o = line.kind === "add" ? null : hasOld ? line.old! - 1 : old.push(line.text) - 1;
    const n = line.kind === "del" ? null : hasNew ? line.new! - 1 : nu.push(line.text) - 1;
    index.push({ old: o, new: n });
  }
  return {
    old: { path: file.old_path ?? file.path, text: hasOld ? whole!.old! : old.join("\n") },
    new: { path: file.path, text: hasNew ? whole!.new! : nu.join("\n") },
    index,
    whole: { old: hasOld, new: hasNew },
  };
}

/** What the window draws: split only where it was chosen and there is room for it. */
export function shownLayout(chosen: DiffLayout, narrow: boolean): DiffLayout {
  return narrow ? "unified" : chosen;
}
