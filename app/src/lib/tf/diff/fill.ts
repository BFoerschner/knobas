/**
 * A range's diff read a file at a time (the compare pass, 2026-11-13): the `DiffFill` the one diff component is handed
 * where the page read the range's list of files (`entity_range_files`) rather than its whole diff. Each file's lines are
 * one page of `entity_range_file` at a time, between the two ends the list named.
 */
import type { DiffFile, RangeFileAsk, RangeFilePage, RangeFiles, RangeNames } from "../../ipc/entity";
import type { DiffFill, FilePage } from "./diff";

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

/** The first page of many files in one read (`entityRangeFilePages`'s shape). */
export type RangeFilePagesRead = (
  entityId: string,
  base: string,
  head: string,
  files: RangeFileAsk[],
  ignoreWhitespace: boolean,
  read: string,
) => Promise<(RangeFilePage | null)[]>;

/** The most files one {@link RangeFilePagesRead} asks for (`checkout::range_files::PAGES_AT_ONCE`). */
export const PAGES_AT_ONCE = 200;

/** A range's list of files (`entityRangeFiles`'s shape). */
export type RangeFilesRead = (entityId: string, base: string, head: string, ignoreWhitespace: boolean, read: string) => Promise<RangeFiles>;

/** Where a list's files' lines are read: between the two commits it settled, else by the ends it was asked by. */
export function fillEnds(list: Pick<RangeFiles, "range">, base: string, head: string): { base: string; head: string } {
  return list.range === null ? { base, head } : { base: list.range.base, head: list.range.head };
}

/**
 * **The pages read, kept for the window** (the big-compare pass, 2026-11-16): every page a fill read between two whole
 * shas, by its range and file, so a file read once draws at once wherever it is drawn again: a jump back to it, the
 * diff remounted for the counted list, and *Open as diff* of the compare it was read in. Two whole shas name content
 * that never changes, so nothing kept goes stale; ends by name are kept by the fill alone. At most {@link KEPT_LINES}
 * lines across ranges, the range used longest ago let go first.
 */
export const KEPT_LINES = 150_000;
const pagesKept = new Map<string, { pages: Map<string, FilePage>; lines: number }>();
let keptLines = 0;
const WHOLE = /^[0-9a-f]{40}([0-9a-f]{24})?$/;
const rangeKeyOf = (entityId: string, at: { base: string; head: string }, ws: boolean) =>
  WHOLE.test(at.base) && WHOLE.test(at.head) ? `${entityId}\u0000${at.base}\u0000${at.head}\u0000${ws ? 1 : 0}` : null;
const pageKeyOf = (file: Pick<DiffFile, "path" | "old_path">, from: number) => `${file.old_path ?? ""}\u0000${file.path}\u0000${from}`;
const linesOf = (page: FilePage) => page.hunks.reduce((sum, hunk) => sum + hunk.lines.length, 0);

function keepPage(range: string, key: string, page: FilePage): void {
  let kept = pagesKept.get(range);
  if (kept === undefined) kept = { pages: new Map(), lines: 0 };
  // Used now: the newest range.
  pagesKept.delete(range);
  pagesKept.set(range, kept);
  if (kept.pages.has(key)) return;
  const lines = linesOf(page);
  kept.pages.set(key, page);
  kept.lines += lines;
  keptLines += lines;
  for (const [oldest, ones] of pagesKept) {
    if (keptLines <= KEPT_LINES || oldest === range) break;
    pagesKept.delete(oldest);
    keptLines -= ones.lines;
  }
}

/** Forget every page kept, for a test. */
export function forgetKeptPages(): void {
  pagesKept.clear();
  keptLines = 0;
  listsKept.clear();
}

/**
 * The fill for a list read between `at`'s two ends in repository `entityId`; `stop` calls a read off at the backend.
 * With `readMany`, the first pages of many files come in one read (`pages`), and every page read is kept for the window
 * where the ends are whole shas (`cached`); `ahead` lets the view read ahead the files it has not drawn, where the list
 * was read at a checkout (a forge's compare is read once and kept by the backend, but a let-go one is the forge's again).
 */
export function rangeFill(
  read: RangeFileRead,
  entityId: string,
  at: { base: string; head: string },
  ignoreWhitespace: boolean,
  stop?: (read: string) => void,
  readMany?: RangeFilePagesRead,
  ahead = false,
): DiffFill {
  const range = rangeKeyOf(entityId, at, ignoreWhitespace);
  const pageOf = (page: RangeFilePage): FilePage => ({ hunks: page.file.hunks, continues: page.continues, next: page.next, truncated: page.file.truncated });
  const kept = (file: DiffFile, from: number, page: FilePage) => {
    if (range !== null) keepPage(range, pageKeyOf(file, from), page);
    return page;
  };
  return {
    page: (file, from, name) =>
      read(entityId, at.base, at.head, file.path, file.old_path ?? null, ignoreWhitespace, from, name).then((page) => kept(file, from, pageOf(page))),
    ...(readMany
      ? {
          pages: (files, name) =>
            readMany(entityId, at.base, at.head, files.map((file) => ({ path: file.path, old_path: file.old_path ?? null })), ignoreWhitespace, name).then((pages) =>
              files.map((file, n) => {
                const page = pages[n];
                return page === null || page === undefined ? null : kept(file, 0, pageOf(page));
              }),
            ),
        }
      : {}),
    cached: (file, from) => (range === null ? undefined : pagesKept.get(range)?.pages.get(pageKeyOf(file, from))),
    ...(readMany && ahead && range !== null ? { ahead: true } : {}),
    ...(stop ? { stop } : {}),
  };
}

/**
 * **The lists drawn, kept for the window** (the big-compare pass): a range's counted list as Compare drew it, by the two
 * whole shas it settled, so *Open as diff* of that compare draws it at once and reads it no second time. The last few.
 */
const listsKept = new Map<string, RangeFiles>();
const LISTS_KEPT = 8;

/** Keeps `list`, the counted list between the two commits it settled, for {@link keptList}. */
export function keepList(entityId: string, list: RangeFiles): void {
  if (list.range === null || list.truncated) return;
  const key = rangeKeyOf(entityId, list.range, false);
  if (key === null) return;
  listsKept.delete(key);
  listsKept.set(key, list);
  while (listsKept.size > LISTS_KEPT) listsKept.delete(listsKept.keys().next().value!);
}

/** The counted list kept between two whole shas, if a page drew it this session. */
export function keptList(entityId: string, base: string, head: string): RangeFiles | undefined {
  const key = rangeKeyOf(entityId, { base, head }, false);
  return key === null ? undefined : listsKept.get(key);
}

/** Each change letter's change; a letter the backend added later reads as a modification. */
const CHANGES: Readonly<Record<string, DiffFile["change"] | undefined>> = { A: "added", D: "deleted", R: "renamed", M: "modified" };

/**
 * A compare's names (`entity_range_names`, the stored-compare pass, 2026-11-14) as the list the page draws: each file as
 * the list with counts names it, `additions` and `deletions` `0`, no file `binary` and no blob, between the two commits
 * the names were read between.
 */
export function namesList(names: RangeNames): RangeFiles {
  const moved = new Map(names.renamed.map(([at, old, similarity]) => [at, { old, similarity }] as const));
  const modes = new Map(names.modes.map(([at, old, nu]) => [at, { old, nu }] as const));
  const files = names.paths.map((path, at): DiffFile => {
    const rename = moved.get(at);
    const mode = modes.get(at);
    return {
      path,
      old_path: rename?.old ?? null,
      change: CHANGES[names.changes.charAt(at)] ?? "modified",
      additions: 0,
      deletions: 0,
      hunks: [],
      truncated: false,
      ...(rename?.similarity != null ? { similarity: rename.similarity } : {}),
      ...(mode ? { old_mode: mode.old, new_mode: mode.nu } : {}),
      threads: [],
    };
  });
  return { files, truncated: names.truncated, range: names.range, read_at: "checkout", predicted_at: names.predicted_at };
}
