/**
 * A range's diff read a file at a time (the compare pass, 2026-11-13): the `DiffFill` the one diff component is handed
 * where the page read the range's list of files (`entity_range_files`) rather than its whole diff. Each file's lines are
 * one page of `entity_range_file` at a time, between the two ends the list named.
 */
import type { RangeFilePage, RangeFiles } from "../../ipc/entity";
import type { DiffFill } from "./diff";

/** One page of one file's lines (`entityRangeFile`'s shape). */
export type RangeFileRead = (
  entityId: string,
  base: string,
  head: string,
  path: string,
  oldPath: string | null,
  ignoreWhitespace: boolean,
  from: number,
  read: string,
) => Promise<RangeFilePage>;

/** A range's list of files (`entityRangeFiles`'s shape). */
export type RangeFilesRead = (entityId: string, base: string, head: string, ignoreWhitespace: boolean, read: string) => Promise<RangeFiles>;

/** Where a list's files' lines are read: between the two commits it settled, else by the ends it was asked by. */
export function fillEnds(list: Pick<RangeFiles, "range">, base: string, head: string): { base: string; head: string } {
  return list.range === null ? { base, head } : { base: list.range.base, head: list.range.head };
}

/** The fill for a list read between `at`'s two ends in repository `entityId`; `stop` calls a read off at the backend. */
export function rangeFill(read: RangeFileRead, entityId: string, at: { base: string; head: string }, ignoreWhitespace: boolean, stop?: (read: string) => void): DiffFill {
  return {
    page: (file, from, name) =>
      read(entityId, at.base, at.head, file.path, file.old_path ?? null, ignoreWhitespace, from, name).then((page) => ({
        hunks: page.file.hunks,
        continues: page.continues,
        next: page.next,
        truncated: page.file.truncated,
      })),
    ...(stop ? { stop } : {}),
  };
}
