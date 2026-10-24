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

/**
 * How long a read is waited on in silence before the panel says it is
 * reading (#703). Under it the body keeps its place, empty, and a view that
 * lands is drawn with no line before it: a line drawn for the 150 ms a local
 * Gitea takes and then swapped for the tabs was the flicker on opening a
 * repo. Over it, the line is honest news.
 */
export const PLACEHOLDER_MS = 400;

/** How many entities are remembered for back and forward. */
export const REMEMBERED = 20;

export interface Recalled {
  detail: EntityDetail;
  /** The view it was drawn with; `null` for none, `undefined` if never answered. */
  view: SourceView | null | undefined;
}

/**
 * An entity's two reads, started ahead of its panel by {@link readAhead} and
 * lent to the panel that mounts for it, so nothing is read twice (#703).
 */
export interface Loan {
  row: Promise<EntityDetail>;
  view: Promise<SourceView | null>;
  /** When the reads started (`Date.now()`): the panel's waits count from here. */
  since: number;
}

/**
 * How long reads made ahead are lent. A panel mounts within
 * {@link HOLD_MS} (the placeholder's threshold) of them, so twice that is
 * room to spare; one that was never mounted (the reader moved on) leaves a
 * loan nobody may take later, or a later opening would draw an answer it
 * never asked for and read nothing.
 */
export const LOAN_MS = 2 * PLACEHOLDER_MS;

export interface ViewMemory {
  /** The entity as last drawn, or `null`. Counts as a use. */
  recall(entityId: string): Recalled | null;
  /** Remember the entity as it is drawn now. */
  keep(entityId: string, held: Recalled): void;
  /** Whether this source answered `None` for this kind this session. */
  drawsNothing(sourceId: string, kind: string): boolean;
  /** Lend an entity's reads made ahead to the panel that opens it next. */
  lend(entityId: string, loan: Loan): void;
  /** Take the reads made ahead for this entity, if they are fresh; once. */
  borrow(entityId: string): Loan | null;
  /** Clear every memory, and the reads ahead still in flight. Tests start from an empty session. */
  clear(): void;
}

export function createViewMemory(limit = REMEMBERED): ViewMemory {
  const seen = new Map<string, Recalled>();
  const nothing = new Set<string>();
  const loans = new Map<string, Loan>();
  const pair = (sourceId: string, kind: string) => `${sourceId}\u0000${kind}`;
  const memory: ViewMemory = {
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
    lend(entityId, loan) {
      loans.set(entityId, loan);
    },
    borrow(entityId) {
      const loan = loans.get(entityId);
      loans.delete(entityId);
      if (loan === undefined || Date.now() - loan.since > LOAN_MS) return null;
      return loan;
    },
    clear() {
      seen.clear();
      nothing.clear();
      loans.clear();
      flights.delete(memory);
    },
  };
  return memory;
}

/** The one the window uses. */
export const viewMemory = createViewMemory();

/**
 * How long a detail opened from another waits for its row and view before it
 * is mounted anyway (#693, #703). Under it, the panel being left stays drawn
 * and the next one arrives whole; over it, the next one is mounted and says
 * it is reading, because a frozen panel is worse than an honest one. It is
 * the placeholder's threshold: holding longer would only delay a line the
 * next panel then draws at once.
 */
export const HOLD_MS = PLACEHOLDER_MS;

/**
 * Read an entity's row and view ahead of mounting its panel (#693, #703):
 * both are lent to the panel, which draws from them rather than reading
 * again, and kept in the memory it recalls from once they answer. Settles
 * when the row has answered and, unless its source is remembered as drawing
 * nothing for its kind, the view too; either way, since a failed read is the
 * panel's own to say. An entity drawn moments ago settles at once: the panel
 * draws it from memory and reads it again itself.
 */
export function readAhead(
  entityId: string,
  read: (entityId: string) => Promise<EntityDetail>,
  readView: (entityId: string) => Promise<SourceView | null>,
  memory: ViewMemory = viewMemory,
): Promise<void> {
  // One read ahead per entity at a time (#753): a hover and then the swap
  // that opens it, or two hovers, wait on the first rather than reading twice.
  const reading = inFlight(memory);
  const already = reading.get(entityId);
  if (already !== undefined) return already;
  const settled = readAheadOnce(entityId, read, readView, memory).finally(() =>
    reading.delete(entityId),
  );
  reading.set(entityId, settled);
  return settled;
}

const flights = new WeakMap<ViewMemory, Map<string, Promise<void>>>();
function inFlight(memory: ViewMemory): Map<string, Promise<void>> {
  let reading = flights.get(memory);
  if (reading === undefined) {
    reading = new Map();
    flights.set(memory, reading);
  }
  return reading;
}

async function readAheadOnce(
  entityId: string,
  read: (entityId: string) => Promise<EntityDetail>,
  readView: (entityId: string) => Promise<SourceView | null>,
  memory: ViewMemory,
): Promise<void> {
  if (memory.recall(entityId) !== null) return;
  const loan: Loan = { row: read(entityId), view: readView(entityId), since: Date.now() };
  // The panel attaches its own handlers; these keep a rejection nobody else
  // awaited from going unhandled.
  loan.row.catch(() => {});
  loan.view.catch(() => {});
  memory.lend(entityId, loan);
  let detail: EntityDetail;
  try {
    detail = await loan.row;
  } catch {
    return;
  }
  let view: SourceView | null | undefined = undefined;
  if (!memory.drawsNothing(detail.source.id, detail.row.kind)) {
    try {
      view = await loan.view;
    } catch {
      // Drawn as the row alone; the panel says why the view is not there.
    }
  }
  if (memory.recall(entityId) === null) memory.keep(entityId, { detail, view });
}

/**
 * Whether two answers say the same thing. A re-read that answers what is
 * already drawn must not be assigned, or the open tab and scroll would reset
 * for nothing. Both sides are plain IPC data, so their JSON is their content.
 */
export function sameAnswer(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
