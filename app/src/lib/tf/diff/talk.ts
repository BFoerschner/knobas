/**
 * What a diff card needs of the view's line comments (#2266): one comment box
 * open across the whole diff, its draft, and the reader's pending comments.
 * `TfDiffView` keeps it; each `TfDiffCard` reads it.
 */
import type { DiffLine } from "../../ipc/entity";
import type { PendingComment } from "../../detail/view/pending-comments.svelte";
import type { DiffReview } from "./diff";

export interface DiffTalk {
  readonly review: DiffReview | null;
  /** The reader's pending comments on this diff's entity. */
  readonly drafts: readonly PendingComment[];
  draftsAt(path: string, line: DiffLine): PendingComment[];
  isOpen(path: string, line: DiffLine): boolean;
  /** Opens the comment box under `line` of `path`, with an empty draft. */
  openAt(path: string, line: DiffLine): void;
  draft: string;
  cancel(): void;
  /** Keeps the draft pending in the reader's store under its line. */
  add(): void;
}
