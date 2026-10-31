/**
 * The reader's pending line comments (#709, spec #701 workflow 6): what was
 * written on a commentable diff and not yet sent with a review.
 *
 * Kept per entity, for as long as the app runs, and in this viewer's browser
 * storage, so closing the detail -- or the app -- does not lose an unsent
 * comment. Storage is a convenience: missing, blocked or holding rubbish, it
 * costs the memory across restarts and never the comments in hand. An unsent
 * draft is this viewer's alone and never the mirror's (spec #701: browser
 * storage is fine for one).
 *
 * The diff writes here, the toolbar counts here, and the detail reads the
 * comments into `submit_review` and clears them once the review is queued.
 *
 * **Per pull request and head** (#2276, spec #2234 step 6d): a comment
 * carries the head it was written on, the last head the diff was drawn at
 * (`seen`). Once the diff is drawn at another head, the comments written on
 * an earlier one are kept apart (`stale`): never dropped, never sent with a
 * review, since their line numbers count on a head that is gone. The
 * review's own words (`body`) are kept beside them, until it is sent.
 *
 * Here and not in the database (the #2276 ruling): an unsent draft is the
 * viewer's own text, not a live read, so ADR-0032 does not reach it either
 * way; it was kept here since #709, and the app's webview storage lasts
 * across restarts.
 */
import type { ReviewComment, ReviewSide } from "../../ipc/sources";

/** One pending comment: where it sits and what it says, and an id to edit it by. */
export interface PendingComment {
  id: string;
  path: string;
  side: ReviewSide;
  line: number;
  body: string;
  /** The head it was written on; `null` where the diff named none, or it was written before #2276. */
  head?: string | null;
}

/** A line as a reader names it: `line 12` on the new side, `old line 12` on the old. */
export function lineName(side: ReviewSide, line: number): string {
  return side === "old" ? `old line ${line}` : `line ${line}`;
}

/** Where every viewer's pending comments are kept: one JSON object of entity id to comments. */
const KEY = "knobas.pending-comments";
/** Where each entity's last head seen and its review's words are kept: entity id to `{ head, body }`. */
const REVIEW_KEY = "knobas.pending-reviews";

interface ReviewDraft {
  head: string | null;
  body: string;
}

export interface PendingCommentStore {
  /** The pending comments on `entity` written on the head last seen, in the order written: what a review sends. */
  of(entity: string): PendingComment[];
  /** Those written on an earlier head than the one last seen, kept apart until discarded. */
  stale(entity: string): PendingComment[];
  /** The diff of `entity` is drawn at `head`: new comments are written on it. */
  seen(entity: string, head: string | null): void;
  /** The review's own words, kept until it is sent. */
  body(entity: string): string;
  setBody(entity: string, body: string): void;
  /** Keep a new comment; its id, or `null` for a blank one, which is no comment. */
  add(entity: string, draft: Omit<PendingComment, "id">): string | null;
  /** Change a comment's words. Blank discards it. */
  edit(entity: string, id: string, body: string): void;
  discard(entity: string, id: string): void;
  /**
   * Forget those of `sent` that are still as they were sent: a review took
   * them. One edited since stays pending, with its new words. The review's
   * words go too where they are still `body`.
   */
  settle(entity: string, sent: PendingComment[], body?: string): void;
  /** Forget every pending comment on `entity`: discarded, or sent. */
  clear(entity: string): void;
  /** Forget every pending comment held in hand, storage untouched: a fresh session (tests). */
  forget(): void;
}

/** `comments` as a review's line comments: where each sits and what it says. */
export function asReview(comments: PendingComment[]): ReviewComment[] {
  return comments.map(({ path, line, side, body }) => ({ path, line, side, body }));
}

function isDraft(value: unknown): value is PendingComment {
  if (typeof value !== "object" || value === null) return false;
  const d = value as Record<string, unknown>;
  return (
    typeof d.id === "string" &&
    typeof d.path === "string" &&
    (d.side === "old" || d.side === "new") &&
    typeof d.line === "number" &&
    typeof d.body === "string" &&
    (d.head === undefined || d.head === null || typeof d.head === "string")
  );
}

/** The review drafts `storage` holds, as far as they read. */
function loadReviews(storage: Storage | null): Record<string, ReviewDraft> {
  try {
    const raw = storage?.getItem(REVIEW_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return {};
    const out: Record<string, ReviewDraft> = {};
    for (const [entity, value] of Object.entries(parsed as Record<string, unknown>)) {
      if (typeof value !== "object" || value === null) continue;
      const v = value as Record<string, unknown>;
      out[entity] = { head: typeof v.head === "string" ? v.head : null, body: typeof v.body === "string" ? v.body : "" };
    }
    return out;
  } catch {
    return {};
  }
}

/** What `storage` holds, as far as it reads; anything else is nothing. */
function load(storage: Storage | null): Record<string, PendingComment[]> {
  try {
    const raw = storage?.getItem(KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null) return {};
    const out: Record<string, PendingComment[]> = {};
    for (const [entity, list] of Object.entries(parsed)) {
      const drafts = Array.isArray(list) ? list.filter(isDraft) : [];
      if (drafts.length > 0) out[entity] = drafts;
    }
    return out;
  } catch {
    return {};
  }
}

let counter = 0;

/** The store over `storage`, read once now and written on every change. */
export function createPendingComments(storage: () => Storage | null): PendingCommentStore {
  let held: Storage | null = null;
  try {
    held = storage();
  } catch {
    held = null;
  }
  const byEntity = $state<Record<string, PendingComment[]>>(load(held));
  const reviews = $state<Record<string, ReviewDraft>>(loadReviews(held));

  function save() {
    try {
      held?.setItem(KEY, JSON.stringify(byEntity));
      held?.setItem(REVIEW_KEY, JSON.stringify(reviews));
    } catch {
      // Full or blocked: the drafts stay in hand for this run.
    }
  }

  /** The head new comments on `entity` are written on. */
  const headOf = (entity: string) => reviews[entity]?.head ?? null;
  /** Whether `d` was written on the head last seen (or names none, on either side). */
  const current = (entity: string, d: PendingComment) => {
    const head = headOf(entity);
    return head === null || (d.head ?? null) === null || d.head === head;
  };

  function review(entity: string, next: ReviewDraft) {
    if (next.head === null && next.body === "") delete reviews[entity];
    else reviews[entity] = next;
    save();
  }

  function set(entity: string, drafts: PendingComment[]) {
    if (drafts.length === 0) delete byEntity[entity];
    else byEntity[entity] = drafts;
    save();
  }

  return {
    of: (entity) => (byEntity[entity] ?? []).filter((d) => current(entity, d)),
    stale: (entity) => (byEntity[entity] ?? []).filter((d) => !current(entity, d)),
    seen(entity, head) {
      if (headOf(entity) === head) return;
      review(entity, { head, body: reviews[entity]?.body ?? "" });
    },
    body: (entity) => reviews[entity]?.body ?? "",
    setBody(entity, body) {
      review(entity, { head: headOf(entity), body });
    },
    add(entity, draft) {
      if (draft.body.trim() === "") return null;
      counter += 1;
      const id = `d${Date.now().toString(36)}-${counter}`;
      set(entity, [...(byEntity[entity] ?? []), { ...draft, head: headOf(entity), id }]);
      return id;
    },
    edit(entity, id, body) {
      const drafts = byEntity[entity] ?? [];
      if (body.trim() === "") {
        set(entity, drafts.filter((d) => d.id !== id));
        return;
      }
      set(entity, drafts.map((d) => (d.id === id ? { ...d, body } : d)));
    },
    discard(entity, id) {
      set(entity, (byEntity[entity] ?? []).filter((d) => d.id !== id));
    },
    settle(entity, sent, body) {
      const gone = new Set(sent.map((d) => `${d.id}\n${d.body}`));
      set(entity, (byEntity[entity] ?? []).filter((d) => !gone.has(`${d.id}\n${d.body}`)));
      if (body !== undefined && reviews[entity]?.body === body) review(entity, { head: headOf(entity), body: "" });
    },
    clear(entity) {
      set(entity, []);
      if (reviews[entity]) review(entity, { head: headOf(entity), body: "" });
    },
    forget() {
      for (const entity of Object.keys(byEntity)) delete byEntity[entity];
      for (const entity of Object.keys(reviews)) delete reviews[entity];
    },
  };
}

/** The app's one store, over this viewer's `localStorage` where there is one. */
export const pendingComments: PendingCommentStore = createPendingComments(() =>
  typeof window === "undefined" ? null : (window.localStorage ?? null),
);
