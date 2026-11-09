/**
 * A source view the app answered from memory was read again behind its answer,
 * and the answer moved (#2242, `view:fresh`): the page showing it reads it
 * again, now the fresh answer, over what it drew. One ear per section a page
 * draws; the branch list (`TfRepo`) and the top of *Files* (`TfRepoFiles`)
 * keep their own, older ones.
 */
import { untrack } from "svelte";
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../../ipc";
import type { ViewFresh } from "../../ipc/entity";
import type { EchoListen } from "../queued.svelte";

/**
 * Call `again` each time `section` of the entity `at()` names was read again
 * behind and moved. Called while a component initialises: the ear goes with it.
 */
export function onViewFresh(at: () => string, section: string, again: () => void, listen?: EchoListen): void {
  $effect(() => {
    const where = at();
    const hear: EchoListen = listen ?? ((event, handler) => tauriListen(event, handler));
    let live = true;
    let stop: (() => void) | null = null;
    void Promise.resolve()
      .then(() =>
        hear(EVENTS.viewFresh, ({ payload }) => {
          const fresh = payload as ViewFresh;
          if (live && fresh.entity_id === where && fresh.section === section) untrack(again);
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
