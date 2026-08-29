/**
 * The reverse direction's trigger (issue #44, stories 17 and 19).
 *
 * A merged pull request moves its linked ticket to In Review *on its own* —
 * and "on its own" has to mean something concrete: a pull request becomes
 * merged in the mirror during an ordinary sync, so **a sync run ending is the
 * trigger**, the same signal the suggestion tray listens for and for the same
 * reason — the payload is coarse by rule, and a run that just finished is the
 * only word there is that the mirror moved.
 *
 * The pass itself lives in the backend (`follow_merges`): it acts only on
 * links knobas holds, and its memory is the write queue's own rows, so calling
 * it after every sync is idempotent — a merge is followed once, however many
 * runs end. What this module adds is the *telling* (story 19): a count above
 * zero is announced as a toast, so an automatic change is visible rather than
 * mysterious. The activity stream carries the durable record; the toast is
 * the tap on the shoulder.
 *
 * A failed pass is deliberately quiet. It runs in nobody's gesture, so there
 * is no surface that owns the error — and the next sync ending retries it. A
 * pass that cannot run because the database is still coming up is the normal
 * first minute of a session, not news.
 */
import { listen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import { followMerges } from "../ipc/entity";
import type { SourceSyncStatus } from "../ipc/sources";
import { push } from "./toasts.svelte";

/**
 * Start following merges for the life of the shell.
 *
 * Returns the teardown. Passes are serialized: a run ending while a pass is in
 * flight queues exactly one more, because the dispatches a pass makes can
 * trigger a refresh whose own ending would otherwise stack passes behind one
 * another for ever.
 */
export function startFollowingMerges(): () => void {
  let dead = false;
  let inFlight = false;
  let again = false;
  let off: (() => void) | undefined;

  async function pass(): Promise<void> {
    if (inFlight) {
      again = true;
      return;
    }
    inFlight = true;
    try {
      do {
        again = false;
        const moved = await followMerges();
        if (moved > 0) {
          push({
            text:
              moved === 1
                ? "A merged pull request moved its ticket to In Review."
                : `Merged pull requests moved ${moved} tickets to In Review.`,
          });
        }
      } while (again && !dead);
    } catch {
      // Retried on the next sync ending; see the module header.
    } finally {
      inFlight = false;
    }
  }

  void listen<SourceSyncStatus>(EVENTS.syncState, (event) => {
    // `running: false` is the transition that says a run just ended and the
    // mirror may have moved — the same reading the suggestion tray makes.
    if (dead || event.payload.running) return;
    void pass();
  })
    .then((unlisten) => {
      if (dead) unlisten();
      else off = unlisten;
    })
    .catch(() => {});

  return () => {
    dead = true;
    off?.();
  };
}
