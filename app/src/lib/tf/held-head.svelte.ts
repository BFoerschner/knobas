/**
 * A header that draws once, whole (the nothing-moves pass, 2026-11-17,
 * `docs/decisions/2026-11-17-nothing-moves-once-drawn.md`; Björn: "Check if
 * any other page moves after it's drawn"). A detail page's header holds what
 * several reads answer: the title from the item, *Open in…* or *Clone* from
 * the checkout, *Refresh* or *Sync now* from the item's source, the subline's
 * facts. Drawn as each landed, the actions on the right pushed each other
 * left and the subline's sign right, under a pointer on its way to one.
 *
 * So the header is held unseen, in its room, until every read it draws from
 * has answered, and then shows at once; a read slower than {@link HEAD_HOLD_MS}
 * does not hide it longer, since a header a person can't press is worse than
 * one that moves once. Once shown for a key it stays shown until the key
 * changes: a refresh lands over what the header drew.
 */
import { untrack } from "svelte";

/** The longest a header waits unseen for its reads. */
export const HEAD_HOLD_MS = 500;

export interface HeldHead {
  /** Whether the header is drawn: its reads have answered, or it has waited long enough. */
  readonly shown: boolean;
}

/**
 * `key`: what the header is of (a repository, a pull request, a run); a new
 * one holds it again. `answered`: whether every read it draws from has
 * answered, failed or not.
 */
export function heldHead(key: () => unknown, answered: () => boolean, holdMs = HEAD_HOLD_MS): HeldHead {
  let latched = $state(false);
  $effect(() => {
    void key();
    untrack(() => (latched = answered()));
    const late = setTimeout(() => (latched = true), holdMs);
    return () => clearTimeout(late);
  });
  $effect(() => {
    if (answered()) untrack(() => (latched = true));
  });
  return {
    get shown() {
      return latched || answered();
    },
  };
}
