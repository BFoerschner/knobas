/**
 * Expanded context's reader for a commit's diff (#2267): the file whole at
 * the commit (the new side) and at its first parent (the old side), each a
 * `file:<path>` read of that commit's entity. A commit at a full sha is
 * `Standing::Fixed`, so the app's read cache answers the second read of the
 * same file with no source request (ADR-0032, `notes/read-cache.md`).
 *
 * The parent is the overview's *Parent* row, where the view gives it an
 * address (ADR-0020 rule 7: only where the mirror holds that commit); the
 * shell never derives one id from another. Without it the old side is not
 * read, which costs only its highlighting beyond the drawn lines: an
 * expanded line is context, the same text on both sides.
 */
import { isIpcError } from "../../ipc";
import { archiveOpen, type ArchiveIndex, type DiffFile, type DiffRange, type PipeRun, type PipeStep, type SourceView } from "../../ipc/entity";
import { INDEX, isWorkingTree, WORKING_TREE } from "./address";
import type { DiffContext, DrawnFile } from "./diff";

type Read = (entityId: string, section: string | null) => Promise<SourceView | null>;
/** The byte read (#2256): a file's bytes at an entity; `commit` `null` is a commit entity's own sha. */
type ReadBytes = (entityId: string, path: string, commit: string | null) => Promise<Uint8Array>;
/** A file pipeline over a file's bytes at an entity (#2270, `entityFilePiped`). */
type ReadPiped = (entityId: string, path: string, commit: string | null, steps: PipeStep[]) => Promise<PipeRun>;
/** An archive at an entity listed for the JAR view (`archive_open`); `commit` as the byte read takes it. */
type ReadArchive = (entityId: string, path: string, commit: string | null) => Promise<ArchiveIndex>;

/** The text of a `file:` read, where it holds the whole file: not cut, not binary. */
export function wholeFileText(view: SourceView | null): string | null {
  for (const section of view?.sections ?? [])
    for (const block of section.blocks) if (block.type === "file") return block.truncated || block.binary ? null : block.text;
  return null;
}

/** The first parent's address on a commit's overview, where it has one. */
export function parentOf(view: SourceView | null): string | null {
  for (const section of view?.sections ?? [])
    for (const block of section.blocks)
      if (block.type === "entities" && (block.title === "Parent" || block.title === "Parents")) return block.rows[0]?.entity_id ?? null;
  return null;
}

/**
 * `bytes` (#2279's rendered image diff), where given: a side's bytes at its commit. A binary diff
 * does not say whether the file was added or deleted, so a side the file is not on (`not_found`)
 * is `null`; without a parent address the old side is a refusal, not an absence. `piped` (#2270) runs a
 * file pipeline on a side the same way.
 */
export function commitContext(read: Read, commitId: string, parentId: string | null, bytes?: ReadBytes, piped?: ReadPiped, archive: ReadArchive = archiveOpen): DiffContext {
  /** `ask` at a side's commit and path; a side the file is not on (`not_found`) is `null`. */
  async function atSide<T>(file: DiffFile, side: "old" | "new", ask: (id: string, path: string) => Promise<T>): Promise<T | null> {
    const id = side === "new" ? commitId : parentId;
    if (id === null) throw new Error("the parent commit has no address here, so the file before the change is not read");
    try {
      return await ask(id, side === "old" ? (file.old_path ?? file.path) : file.path);
    } catch (cause) {
      if (isIpcError(cause) && cause.code === "not_found") return null;
      throw cause;
    }
  }
  return {
    ...(bytes ? { bytes: (file: DiffFile, side: "old" | "new") => atSide(file, side, (id, path) => bytes(id, path, null)) } : {}),
    ...(piped ? { piped: (file: DiffFile, side: "old" | "new", steps: PipeStep[]) => atSide(file, side, (id, path) => piped(id, path, null, steps)) } : {}),
    archive: (file, side) => atSide(file, side, (id, path) => archive(id, path, null)),
    async text(file, side) {
      const id = side === "new" ? commitId : parentId;
      if (id === null) return null;
      return wholeFileText(await read(id, `file:${side === "old" ? (file.old_path ?? file.path) : file.path}`));
    },
  };
}

/** The section of a range's repository that reads `path` whole at the commit `sha`. */
export function fileAtSection(sha: string, path: string): string {
  return `file@${sha}:${path}`;
}

/**
 * Expanded context's reader for a diff that names its range (#2310): a pull request's, a compare's,
 * a diff address's. Each side is read in the range's repository at its own whole sha (`base` the
 * old side, by the file's old path; `head` the new), which stands at a fixed commit, so the app's
 * read cache answers the second read of a file with no source request (ADR-0032). `bytes`, where
 * given, reads a side's bytes at that sha for a rendered image diff (#2279), and `piped` runs a file
 * pipeline on a side there (#2270); `not_found` is no file on that side.
 */
export function rangeContext(read: Read, range: DiffRange, bytes?: ReadBytes, piped?: ReadPiped, archive: ReadArchive = archiveOpen): DiffContext {
  const at = (file: DiffFile, side: "old" | "new") =>
    side === "new" ? { sha: range.head, path: file.path } : { sha: range.base, path: file.old_path ?? file.path };
  /** `ask` at a side's sha and path; a side the file is not on (`not_found`) is `null`. */
  async function atSide<T>(file: DiffFile, side: "old" | "new", ask: (sha: string, path: string) => Promise<T>): Promise<T | null> {
    const { sha, path } = at(file, side);
    try {
      return await ask(sha, path);
    } catch (cause) {
      if (isIpcError(cause) && cause.code === "not_found") return null;
      throw cause;
    }
  }
  return {
    ...(bytes ? { bytes: (file: DiffFile, side: "old" | "new") => atSide(file, side, (sha, path) => bytes(range.repository, path, sha)) } : {}),
    ...(piped
      ? { piped: (file: DiffFile, side: "old" | "new", steps: PipeStep[]) => atSide(file, side, (sha, path) => piped(range.repository, path, sha, steps)) }
      : {}),
    archive: (file, side) => atSide(file, side, (sha, path) => archive(range.repository, path, sha)),
    async text(file, side) {
      const { sha, path } = at(file, side);
      // No source holds the working tree: its side is its bytes, decoded (ux26).
      if (isWorkingTree(sha) && bytes) {
        const got = await bytes(range.repository, path, sha).catch((cause) => {
          if (isIpcError(cause) && cause.code === "not_found") return null;
          throw cause;
        });
        return got === null ? null : new TextDecoder().decode(got);
      }
      return wholeFileText(await read(range.repository, fileAtSection(sha, path)));
    },
  };
}

/**
 * The reader for *Local changes*' two halves (ux26): a file not staged is the index against the working tree, a staged
 * one `HEAD` (`head`, its whole sha) against the index; each side read at the checkout (`entity_file_bytes` with the
 * end's token), never the source. Its text is its bytes, decoded. `head` `null` (no commit yet): a staged file has no
 * side before.
 */
export function localContext(bytes: ReadBytes, repoId: string, head: string | null, piped?: ReadPiped, archive: ReadArchive = archiveOpen): DiffContext {
  const at = (file: DrawnFile, side: "old" | "new") => {
    const staged = file.half === "staged";
    const end = side === "new" ? (staged ? INDEX : WORKING_TREE) : staged ? head : INDEX;
    return { end, path: side === "old" ? (file.old_path ?? file.path) : file.path };
  };
  async function atSide<T>(file: DrawnFile, side: "old" | "new", ask: (path: string, end: string) => Promise<T>): Promise<T | null> {
    const { end, path } = at(file, side);
    if (end === null) return null;
    try {
      return await ask(path, end);
    } catch (cause) {
      if (isIpcError(cause) && cause.code === "not_found") return null;
      throw cause;
    }
  }
  const read = (file: DrawnFile, side: "old" | "new") => atSide(file, side, (path, end) => bytes(repoId, path, end));
  return {
    bytes: read,
    archive: (file, side) => atSide(file, side, (path, end) => archive(repoId, path, end)),
    ...(piped ? { piped: (file: DiffFile, side: "old" | "new", steps: PipeStep[]) => atSide(file, side, (path, end) => piped(repoId, path, end, steps)) } : {}),
    async text(file, side) {
      const got = await read(file, side);
      return got === null ? null : new TextDecoder().decode(got);
    },
  };
}
