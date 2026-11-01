/**
 * **The chosen job** (spec #2227): the ticket the reader last made the job
 * on purpose, as its entity id, whose page draws at the bare `#/job` (home,
 * `home.ts`) and on the yellow at its own `#/ticket/<id>` (spec #2396). It is
 * set in four places, and never by reading a ticket:
 *
 * - the ticket page, once it has resolved a `#/job/<key or id>` address to
 *   its ticket (a bookmark, history, a link, the timer chip, the Tickets
 *   preview's button), which then lands on the ticket's own address;
 * - the ticket page card's foot: *Work on this*, *Resume work*, *Start work*;
 * - a timer start on a ticket (`App.svelte`, off `timer.started`);
 * - *Start work*'s run (`TfStartWork.svelte`).
 *
 * It replaces the shell's "job you were last in", which whichever job page
 * was opened last fed. The timer chip's fallback (⌘T with nothing in front)
 * and the launcher's *Link to <key>* elsewhere than on a ticket page read it
 * (`TfShell.svelte`).
 *
 * Kept in the webview's `localStorage` (`knobas.job.chosen`), as the diff
 * layout (`knobas.diff.layout`) is: a per-machine convenience, read when the
 * shell mounts, never through IPC, and not in a backup. A read that fails,
 * finds nothing or finds no entity id means no chosen job; a write that fails
 * keeps the choice for this run only.
 */
import { untrack } from "svelte";

import { isEntityId } from "./job";

export const CHOSEN_KEY = "knobas.job.chosen";

type Storage = Pick<globalThis.Storage, "getItem" | "setItem" | "removeItem">;

export interface ChosenJob {
  /** The chosen job's entity id, or `null` when none is chosen. */
  readonly id: string | null;
  /** Make `id` (a ticket's entity id) the chosen job, and keep it. */
  choose(id: string): void;
  /** Read the kept choice again: the shell's mount, as a restart does. */
  load(): void;
  /** Tests only: no chosen job, nothing kept. */
  reset(): void;
}

export function createChosenJob(storage: () => Storage | null): ChosenJob {
  const read = (): string | null => {
    try {
      const kept = storage()?.getItem(CHOSEN_KEY) ?? null;
      return kept !== null && isEntityId(kept) ? kept : null;
    } catch {
      return null;
    }
  };
  let id = $state<string | null>(read());
  return {
    get id() {
      return id;
    },
    choose(next) {
      // Untracked: an effect that chooses must not re-run, and choose again,
      // when something else changes the choice.
      if (untrack(() => id) === next) return;
      id = next;
      try {
        storage()?.setItem(CHOSEN_KEY, next);
      } catch {
        // Kept for this run only.
      }
    },
    load() {
      id = read();
    },
    reset() {
      id = null;
      try {
        storage()?.removeItem(CHOSEN_KEY);
      } catch {
        // Nothing kept to clear.
      }
    },
  };
}

/** The window's one chosen job, over this viewer's `localStorage` where there is one. */
export const chosenJob: ChosenJob = createChosenJob(() =>
  typeof window === "undefined" ? null : window.localStorage,
);
