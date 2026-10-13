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
 */
import type { ReviewComment, ReviewSide } from "../../ipc/sources";

/** One pending comment: where it sits and what it says, and an id to edit it by. */
export interface PendingComment {
  id: string;
  path: string;
  side: ReviewSide;
  line: number;
  body: string;
}

/** A line as a reader names it: `line 12` on the new side, `old line 12` on the old. */
export function lineName(side: ReviewSide, line: number): string {
  return side === "old" ? `old line ${line}` : `line ${line}`;
}

/** Where every viewer's pending comments are kept: one JSON object of entity id to comments. */
const KEY = "knobas.pending-comments";

export interface PendingCommentStore {
  /** The pending comments on `entity`, in the order written. */
  of(entity: string): PendingComment[];
  /** Keep a new comment; its id, or `null` for a blank one, which is no comment. */
  add(entity: string, draft: Omit<PendingComment, "id">): string | null;
  /** Change a comment's words. Blank discards it. */
  edit(entity: string, id: string, body: string): void;
  discard(entity: string, id: string): void;
  /**
   * Forget those of `sent` that are still as they were sent: a review took
   * them. One edited since stays pending, with its new words.
   */
  settle(entity: string, sent: PendingComment[]): void;
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
    typeof d.body === "string"
  );
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

  function save() {
    try {
      held?.setItem(KEY, JSON.stringify(byEntity));
    } catch {
      // Full or blocked: the drafts stay in hand for this run.
    }
  }

  function set(entity: string, drafts: PendingComment[]) {
    if (drafts.length === 0) delete byEntity[entity];
    else byEntity[entity] = drafts;
    save();
  }

  return {
    of: (entity) => byEntity[entity] ?? [],
    add(entity, draft) {
      if (draft.body.trim() === "") return null;
      counter += 1;
      const id = `d${Date.now().toString(36)}-${counter}`;
      set(entity, [...(byEntity[entity] ?? []), { ...draft, id }]);
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
    settle(entity, sent) {
      const gone = new Set(sent.map((d) => `${d.id}\n${d.body}`));
      set(entity, (byEntity[entity] ?? []).filter((d) => !gone.has(`${d.id}\n${d.body}`)));
    },
    clear(entity) {
      set(entity, []);
    },
    forget() {
      for (const entity of Object.keys(byEntity)) delete byEntity[entity];
    },
  };
}

/** The app's one store, over this viewer's `localStorage` where there is one. */
export const pendingComments: PendingCommentStore = createPendingComments(() =>
  typeof window === "undefined" ? null : (window.localStorage ?? null),
);
