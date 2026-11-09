/**
 * A source view the app answered from memory, or from what it held since the
 * last session (ADR-0042), was read again behind its answer (#2242,
 * `view:fresh`): the answer moved, or an answer older than
 * {@link HELD_AFTER_MS} was confirmed, so the page showing it reads it again,
 * now the fresh answer, over what it drew. Or the read behind such an answer
 * failed, and the page says so over what it drew and reads nothing again.
 * One ear per section a page draws; the branch list (`TfRepo`) and the top
 * of *Files* (`TfRepoFiles`) keep their own, older ones.
 */
import { untrack } from "svelte";
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../../ipc";
import type { ViewFresh } from "../../ipc/entity";
import { ago } from "../../shell/time";
import type { EchoListen } from "../queued.svelte";

/** How old a held answer is before its page says from when it is: the backend's `HELD_AFTER`. */
export const HELD_AFTER_MS = 30_000;

/**
 * The line over a view drawn from what knobas held (`held_at`), or `null` for
 * a read made now or a moment ago: from when it is, then that `source` is
 * read behind it, or why it did not answer.
 */
export function heldLine(heldAt: string | null | undefined, source: string, failed: string | null, now: Date = new Date()): string | null {
  if (heldAt === null || heldAt === undefined) return null;
  const at = new Date(heldAt).getTime();
  if (Number.isNaN(at) || now.getTime() - at < HELD_AFTER_MS) return null;
  const when = ago(heldAt, now);
  const read = `As knobas read it ${when === "just now" ? "a moment ago" : when}.`;
  return failed === null ? `${read} Checking ${source}…` : `${read} ${source} did not answer: ${failed}`;
}

/**
 * Call `again` each time `section` (`null`: the whole view) of the entity
 * `at()` names was read again behind and moved or was confirmed, and `failed`
 * with the source's reason when that read failed. Called while a component
 * initialises: the ear goes with it.
 */
export function onViewFresh(
  at: () => string | null,
  section: string | null,
  again: () => void,
  listen?: EchoListen,
  failed?: (why: string) => void,
): void {
  $effect(() => {
    const where = at();
    if (where === null) return;
    const hear: EchoListen = listen ?? ((event, handler) => tauriListen(event, handler));
    let live = true;
    let stop: (() => void) | null = null;
    void Promise.resolve()
      .then(() =>
        hear(EVENTS.viewFresh, ({ payload }) => {
          const fresh = payload as ViewFresh;
          if (!live || fresh.entity_id !== where || (fresh.section ?? null) !== section) return;
          if (fresh.failed !== undefined && fresh.failed !== null) {
            if (failed) untrack(() => failed(fresh.failed!));
          } else untrack(again);
        }),
      )
      .then(
        (unlisten) => {
          if (live) stop = unlisten;
          else unlisten();
        },
        // No bridge to hear it on (a test without `listen`): nothing is told.
        () => {},
      );
    return () => {
      live = false;
      stop?.();
    };
  });
}
