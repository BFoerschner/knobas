/**
 * What the detail remembers between openings, so it does not jump (#693).
 *
 * Two memories, both for the session and neither stored (ADR-0020 rule 1: a
 * view is live; this is the reader's back button, not a mirror):
 *
 * * **the last entities seen**, each with the row and the view it was drawn
 *   with, so going back to one draws it at once and re-reads it behind;
 * * **which source and kind answered `None`**, so a source that draws no view
 *   for a kind (every Jira ticket today) never makes its next item wait for
 *   the read. This is the cheapest honest signal of whether a view is coming:
 *   learned from the adapter's own answer, never guessed from its name (§3a).
 *   A view arriving later for that pair forgets it again.
 */
import type { EntityDetail, SourceView } from "../ipc/entity";

/** How long the panel waits for a view before drawing the mirror's body. */
export const VIEW_PATIENCE_MS = 1500;

/** How many entities are remembered for back and forward. */
export const REMEMBERED = 20;

export interface Recalled {
  detail: EntityDetail;
  /** The view it was drawn with; `null` for none, `undefined` if never answered. */
  view: SourceView | null | undefined;
}

export interface ViewMemory {
  /** The entity as last drawn, or `null`. Counts as a use. */
  recall(entityId: string): Recalled | null;
  /** Remember the entity as it is drawn now. */
  keep(entityId: string, held: Recalled): void;
  /** Whether this source answered `None` for this kind this session. */
  drawsNothing(sourceId: string, kind: string): boolean;
  /** Clear both memories. Tests start from an empty session. */
  clear(): void;
}

export function createViewMemory(limit = REMEMBERED): ViewMemory {
  const seen = new Map<string, Recalled>();
  const nothing = new Set<string>();
  const pair = (sourceId: string, kind: string) => `${sourceId}\u0000${kind}`;
  return {
    recall(entityId) {
      const held = seen.get(entityId);
      if (held === undefined) return null;
      // Map order is insertion order: re-inserting makes it the newest.
      seen.delete(entityId);
      seen.set(entityId, held);
      return held;
    },
    keep(entityId, held) {
      seen.delete(entityId);
      seen.set(entityId, held);
      while (seen.size > limit) seen.delete(seen.keys().next().value!);
      if (held.view === null) nothing.add(pair(held.detail.source.id, held.detail.row.kind));
      else if (held.view !== undefined) nothing.delete(pair(held.detail.source.id, held.detail.row.kind));
    },
    drawsNothing(sourceId, kind) {
      return nothing.has(pair(sourceId, kind));
    },
    clear() {
      seen.clear();
      nothing.clear();
    },
  };
}

/** The one the window uses. */
export const viewMemory = createViewMemory();

/**
 * Whether two answers say the same thing. A re-read that answers what is
 * already drawn must not be assigned, or the open tab and scroll would reset
 * for nothing. Both sides are plain IPC data, so their JSON is their content.
 */
export function sameAnswer(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
