/**
 * What a diff card needs of the view's line comments (#2266): one comment box
 * open across the whole diff, its draft, and the reader's pending comments.
 * `TfDiffView` keeps it; each `TfDiffCard` reads it.
 */
import type { DiffLine, LineThread } from "../../ipc/entity";
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
  /** Whether a reply can be written on `thread` (#2277): the source takes one there. */
  canReply(thread: LineThread): boolean;
  /** Whether `thread` can be resolved or reopened (#2277). */
  canResolve(thread: LineThread): boolean;
  /** Whether `thread` reads resolved, a queued resolve counted. */
  isResolved(thread: LineThread): boolean;
  /** Whether a resolved `thread` is shown opened. */
  isShown(thread: LineThread): boolean;
  show(thread: LineThread, on: boolean): void;
  /** Whether the reply box under `thread` is open. */
  isReplying(thread: LineThread): boolean;
  openReply(thread: LineThread): void;
  replyDraft: string;
  cancelReply(): void;
  sendReply(path: string, thread: LineThread): void;
  resolve(thread: LineThread, on: boolean): void;
  /** The replies queued on `thread` this session that the read does not hold yet. */
  queuedOn(thread: LineThread): readonly string[];
}
