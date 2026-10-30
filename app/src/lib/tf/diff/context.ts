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
import type { SourceView } from "../../ipc/entity";
import type { DiffContext } from "./diff";

type Read = (entityId: string, section: string | null) => Promise<SourceView | null>;

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

export function commitContext(read: Read, commitId: string, parentId: string | null): DiffContext {
  return {
    async text(file, side) {
      const id = side === "new" ? commitId : parentId;
      if (id === null) return null;
      return wholeFileText(await read(id, `file:${side === "old" ? (file.old_path ?? file.path) : file.path}`));
    },
  };
}
