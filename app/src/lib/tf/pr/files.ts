/**
 * *Files changed*'s pure half (#842, R-8.17): the diff a pull request's
 * *Files* section answers (Gitea's `view/pr.rs` reads it only when asked,
 * ADR-0020 rule 3), where each of its threads sits, and where a reader's new
 * comment goes.
 *
 * A thread sits under the line it was written on: a removed line by its old
 * number on the old side, any other line by its new number on the new side,
 * and a context line by either. An outdated thread (the source's answer), or
 * one whose line this diff does not draw, is kept apart at the end of its
 * file, never dropped (the old shell's `DiffBlock` reads them the same way).
 */
import type { Commenting, DiffFile, DiffLine, DiffRange, FileThread, LineThread, SourceView } from "../../ipc/entity";
import type { ReviewSide } from "../../ipc/sources";

/** What *Files changed* draws: the files, and whether and how they take comments. */
export interface PrDiff {
  files: DiffFile[];
  /** The source's cap cut the diff. */
  truncated: boolean;
  commentable: Commenting | null;
  /** Threads on files the diff no longer holds. */
  gone: FileThread[];
  /** What the diff is between, where the source names it (#2273, #2310). */
  range: DiffRange | null;
}

/** The *Files* section's diff, or `null` while it is the deferred stub. */
export function diffOf(view: SourceView | null): PrDiff | null {
  const files = view?.sections.find((section) => section.id === "files");
  for (const block of files?.blocks ?? []) {
    if (block.type === "diff") {
      return { files: block.files, truncated: block.truncated, commentable: block.commentable, gone: block.gone, range: block.range ?? null };
    }
  }
  return null;
}

/** Where a new comment on `line` goes: a removed line's old number, any other line's new one. */
export function targetOf(line: DiffLine): { side: ReviewSide; line: number } {
  return line.kind === "del" ? { side: "old", line: line.old ?? 0 } : { side: "new", line: line.new ?? 0 };
}

/** The places a drawn line answers to: `old:7`, `new:9`. */
function placesOf(line: DiffLine): string[] {
  if (line.kind === "del") return [`old:${line.old}`];
  if (line.kind === "add") return [`new:${line.new}`];
  return [`new:${line.new}`, `old:${line.old}`];
}

/** Each drawn line's threads, in the order the file draws its lines, and the threads kept apart. */
export interface PlacedThreads {
  /** One list per line, hunk by hunk. */
  under: LineThread[][];
  apart: LineThread[];
}

/** Where each of `file`'s threads is drawn: under the first line it answers to, or apart. */
export function placeThreads(file: DiffFile): PlacedThreads {
  const lines = file.hunks.flatMap((hunk) => hunk.lines);
  const under: LineThread[][] = lines.map(() => []);
  const apart: LineThread[] = [];
  for (const thread of file.threads) {
    const place = `${thread.side}:${thread.line}`;
    const at = thread.outdated ? -1 : lines.findIndex((line) => placesOf(line).includes(place));
    if (at < 0) apart.push(thread);
    else under[at]!.push(thread);
  }
  return { under, apart };
}

/** Whether a new comment's target and a pending comment's place are one line. */
export function sameTarget(a: { path: string; side: ReviewSide; line: number }, b: { path: string; side: ReviewSide; line: number }): boolean {
  return a.path === b.path && a.side === b.side && a.line === b.line;
}

/** Whether `path` matches the file filter: any part of it, in any case. */
export function matchesPath(path: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  return q === "" || path.toLowerCase().includes(q);
}
