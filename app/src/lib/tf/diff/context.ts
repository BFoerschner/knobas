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
import type { DiffFile, SourceView } from "../../ipc/entity";
import type { DiffContext } from "./diff";

type Read = (entityId: string, section: string | null) => Promise<SourceView | null>;
/** The byte read (#2256): a file's bytes at an entity; `commit` `null` is a commit entity's own sha. */
type ReadBytes = (entityId: string, path: string, commit: string | null) => Promise<Uint8Array>;

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
 * is `null`; without a parent address the old side is a refusal, not an absence.
 */
export function commitContext(read: Read, commitId: string, parentId: string | null, bytes?: ReadBytes): DiffContext {
  const byteRead = bytes
    ? {
        async bytes(file: DiffFile, side: "old" | "new") {
          const id = side === "new" ? commitId : parentId;
          if (id === null) throw new Error("the parent commit has no address here, so the file before the change is not read");
          try {
            return await bytes(id, side === "old" ? (file.old_path ?? file.path) : file.path, null);
          } catch (cause) {
            if (isIpcError(cause) && cause.code === "not_found") return null;
            throw cause;
          }
        },
      }
    : {};
  return {
    ...byteRead,
    async text(file, side) {
      const id = side === "new" ? commitId : parentId;
      if (id === null) return null;
      return wholeFileText(await read(id, `file:${side === "old" ? (file.old_path ?? file.path) : file.path}`));
    },
  };
}
