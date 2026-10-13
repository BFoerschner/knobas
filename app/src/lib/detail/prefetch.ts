/**
 * Read a detail ahead of the click that opens it (#753).
 *
 * A row the pointer rests on, or that keyboard focus lands on, for
 * {@link DWELL_MS} is read through {@link readAhead}: its mirror row and its
 * source view, lent to the panel that opens next and kept in the view memory
 * once they answer. The click then draws at once, from the loan or from
 * memory, where it used to wait on a live view read of a few hundred
 * milliseconds behind a placeholder.
 *
 * **The dwell is the cost control.** A pointer swept across a tile on its way
 * somewhere else passes each row in a few milliseconds and reads none of them;
 * a row read already, or being read, is not read again (`readAhead`'s own
 * rule). What a hover may cost is one read of the row the reader is looking at.
 */
import { entityView, getEntity } from "../ipc/entity";
import { readAhead } from "./view-read";

/** How long the pointer or focus rests on a row before it is read ahead. */
export const DWELL_MS = 120;

/**
 * The kinds knobas owns open views of their own, through reads of their own
 * (a note's editor, the assets Tree), so a mirror row read for them would be a
 * read nothing uses. Their namespace is their kind (`knobas_core::entity`'s
 * reserved namespaces), and a context is the fourth.
 */
const OWN_VIEWS = new Set(["note", "asset", "route", "ctx"]);

/** Whether a row's detail is the panel {@link readAhead} reads for. */
export function prefetchable(entityId: string, kind: string): boolean {
  const namespace = entityId.slice(0, entityId.indexOf(":")).toLowerCase();
  return !OWN_VIEWS.has(namespace) && !OWN_VIEWS.has(kind);
}

export interface Intent {
  /** The pointer or focus arrived on this entity's row. */
  enter(entityId: string): void;
  /** It left before the dwell, or the row went away. */
  leave(): void;
}

/** One reader's intent: at most one dwell armed at a time. */
export function createIntent(read: (entityId: string) => unknown, dwellMs = DWELL_MS): Intent {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const leave = () => {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  };
  return {
    enter(entityId) {
      leave();
      timer = setTimeout(() => {
        timer = undefined;
        void read(entityId);
      }, dwellMs);
    },
    leave,
  };
}

/** The window's: one pointer, one focus, one dwell. */
const intent = createIntent((entityId) => readAhead(entityId, getEntity, entityView));

/**
 * `use:prefetch={{ entityId, kind }}` on a row that opens a detail. Pointer
 * enter and focus arm the dwell; pointer leave, blur and unmount disarm it.
 */
export function prefetch(node: HTMLElement, target: { entityId: string; kind: string }) {
  let current = target;
  // Whether the armed dwell is this row's, so a row unmounting while another
  // is hovered does not disarm that one.
  let armed = false;
  const enter = () => {
    if (!prefetchable(current.entityId, current.kind)) return;
    armed = true;
    intent.enter(current.entityId);
  };
  const leave = () => {
    if (armed) intent.leave();
    armed = false;
  };
  node.addEventListener("pointerenter", enter);
  node.addEventListener("focus", enter);
  node.addEventListener("pointerleave", leave);
  node.addEventListener("blur", leave);
  return {
    update(next: { entityId: string; kind: string }) {
      current = next;
    },
    destroy() {
      node.removeEventListener("pointerenter", enter);
      node.removeEventListener("focus", enter);
      node.removeEventListener("pointerleave", leave);
      node.removeEventListener("blur", leave);
      leave();
    },
  };
}
