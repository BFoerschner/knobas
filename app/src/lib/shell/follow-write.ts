/**
 * Follow a write queued from a surface that draws nothing of it (#775).
 *
 * `submit_write` answers once the write is queued, so the inbox's comment
 * box, the worklog draft and the monitor rows close on that answer and say
 * *queued*. What they make has no natural place on those surfaces (the
 * detail draws its own, `view/sending.ts`), so the toast is the whole of the
 * feedback -- until the queue says the write will not go on its own. Then the
 * reader is told why, in the queue's words, with the way to the queue and,
 * where the surface can open again with what was typed, *Edit and resend*:
 * the detail's own failure path (#752), for a surface without a panel.
 *
 * Listens on `activity:new`, which follows every queue transition, and asks
 * `pending_writes` once at once as well: a send quick enough to settle before
 * the listener was registered announced itself to nobody. Stops when the
 * write has left the open rows (sent, or withdrawn in the queue) or once it
 * has been told as held or refused.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS } from "../ipc";
import { pendingWrites as realPendingWrites, type QueuedWrite } from "../ipc/sources";
import { push } from "./toasts.svelte";
import { openWriteQueue } from "./write-queue.svelte";

export interface FollowPorts {
  listen: (event: string, handler: () => void) => Promise<() => void>;
  pendingWrites: () => Promise<QueuedWrite[]>;
}

const REAL: FollowPorts = {
  listen: (event, handler) => tauriListen(event, handler),
  pendingWrites: () => realPendingWrites(),
};

export interface Following {
  /** What the write is, in the toast's words: "Comment", "Worklog". */
  name: string;
  /** Reopen the surface with what was typed; absent where it cannot. */
  resend?: () => void;
}

/** Watch `write` until it settles; say so only if it is held or refused. */
export function followWrite(write: Pick<QueuedWrite, "id">, following: Following, ports: FollowPorts = REAL): void {
  let done = false;
  let unlisten: (() => void) | null = null;
  const stop = () => {
    done = true;
    unlisten?.();
    unlisten = null;
  };
  const ask = async () => {
    if (done) return;
    let open: QueuedWrite[];
    try {
      open = await ports.pendingWrites();
    } catch {
      // Not known; the next queue movement asks again.
      return;
    }
    if (done) return;
    const row = open.find((w) => w.id === write.id);
    if (row === undefined) return stop();
    if (row.state !== "held" && row.state !== "refused") return;
    stop();
    push({
      text: `${following.name} ${row.state === "held" ? "was held" : "was refused"}${row.detail ? `: ${row.detail}` : ""}`,
      tone: "err",
      ms: 20_000,
      action: { label: "Open write queue", run: openWriteQueue },
      more: following.resend ? [{ label: "Edit and resend", run: following.resend }] : [],
    });
  };
  ports
    .listen(EVENTS.activityNew, () => void ask())
    .then((off) => {
      if (done) off();
      else unlisten = off;
    })
    .catch(() => {
      // No event channel: the queue panel still says where the write is.
    });
  void ask();
}
