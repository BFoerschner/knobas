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
 * ## One reader for every follow (#779)
 *
 * Every follow goes through one {@link WriteFollower}: one listener on
 * `activity:new`, which follows every queue transition, and **one**
 * `pending_writes` read per event however many writes are followed (*Log
 * all* of twenty is twenty follows). Events that land while a read is out
 * are coalesced into one more read after it. Each read is fanned out to the
 * subscribers by write id. A follow also asks once at once: a send quick
 * enough to settle before the follow began announced itself to nobody.
 *
 * A subscriber leaves when its write settles (sent, withdrawn in the queue,
 * or told as held or refused) or when its surface stops it -- `followWrite`
 * answers the stop. The last one out stops the listener.
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

/** What a subscriber hears: its write's open row, or `undefined` once it left the queue's open rows. */
export type Hear = (row: QueuedWrite | undefined) => void;

export interface WriteFollower {
  /** Hear write `id`'s row after every queue read, until the answered stop is called. */
  follow(id: number, hear: Hear): () => void;
}

/** A follower over `ports`: one listener and one read per event, shared by every follow. */
export function createFollower(ports: FollowPorts): WriteFollower & { reset(): void } {
  const subscribers = new Set<{ id: number; hear: Hear }>();
  let reading = false;
  let again = false;
  let unlisten: (() => void) | null = null;
  let listening = false;
  /** Settles once the listener in flight is registered (or failed); null when none is. */
  let registering: Promise<void> | null = null;

  async function read() {
    if (reading) {
      again = true;
      return;
    }
    reading = true;
    try {
      do {
        again = false;
        let open: QueuedWrite[];
        try {
          open = await ports.pendingWrites();
        } catch {
          // Not known; the next queue movement asks again.
          continue;
        }
        const byId = new Map(open.map((w) => [w.id, w]));
        for (const subscriber of [...subscribers]) {
          if (subscribers.has(subscriber)) subscriber.hear(byId.get(subscriber.id));
        }
      } while (again && subscribers.size > 0);
    } finally {
      reading = false;
    }
  }

  function listenIfNeeded() {
    if (listening) return;
    listening = true;
    const pending: Promise<void> = ports
      .listen(EVENTS.activityNew, () => void read())
      .then((off) => {
        if (listening && unlisten === null) unlisten = off;
        else off();
      })
      .catch(() => {
        // No event channel: the queue panel still says where the write is.
        listening = false;
      })
      .finally(() => {
        if (registering === pending) registering = null;
      });
    registering = pending;
  }

  function stopIfIdle() {
    if (subscribers.size > 0 || !listening) return;
    listening = false;
    unlisten?.();
    unlisten = null;
  }

  return {
    /** Tests only: drop every subscriber and the listener (#1743). */
    reset() {
      subscribers.clear();
      again = false;
      registering = null;
      if (listening) {
        listening = false;
        unlisten?.();
        unlisten = null;
      }
    },
    follow(id, hear) {
      const subscriber = { id, hear };
      subscribers.add(subscriber);
      listenIfNeeded();
      // Read only once the listener is in place: a transition between an
      // earlier read's answer and the registration would be heard by neither
      // (FE-12). With a listener already up, read at once.
      if (registering) void registering.then(read);
      else void read();
      return () => {
        if (!subscribers.delete(subscriber)) return;
        stopIfIdle();
      };
    },
  };
}

let shared: ReturnType<typeof createFollower> | null = null;

/** Tests only: stop the shell's follower and make the next use start a fresh one (#1743). */
export function resetWriteFollower(): void {
  shared?.reset();
  shared = null;
}

/** The shell's one follower, made on first use. */
export function writeFollower(): WriteFollower {
  shared ??= createFollower(REAL);
  return shared;
}

export interface Following {
  /** What the write is, in the toast's words: "Comment", "Worklog". */
  name: string;
  /** Reopen the surface with what was typed; absent where it cannot. */
  resend?: () => void;
  /** What the surface takes back as the write is told held or refused, before the toast: its echo (#1424). */
  onstop?: () => void;
  /** The write left the queue's open rows: sent, or withdrawn in the queue. For a surface that asks which (#1959). */
  onleave?: () => void;
}

/** Whether an open row is one that will not go on its own: held or refused. */
export function willNotGo(row: QueuedWrite | undefined): row is QueuedWrite {
  return row?.state === "held" || row?.state === "refused";
}

/** The toast for a write that will not go on its own. */
export function toastStopped(row: QueuedWrite, following: Following): void {
  push({
    text: `${following.name} ${row.state === "held" ? "was held" : "was refused"}${row.detail ? `: ${row.detail}` : ""}`,
    tone: "err",
    ms: 20_000,
    action: { label: "Open write queue", run: openWriteQueue },
    more: following.resend ? [{ label: "Edit and resend", run: following.resend }] : [],
  });
}

/**
 * Watch `write` until it settles; say so only if it is held or refused.
 * Answers the stop, for a surface that unmounts first.
 */
export function followWrite(
  write: Pick<QueuedWrite, "id">,
  following: Following,
  follower: WriteFollower = writeFollower(),
): () => void {
  let stop: () => void = () => {};
  let done = false;
  const leave = () => {
    done = true;
    stop();
  };
  stop = follower.follow(write.id, (row) => {
    if (done) return;
    if (row === undefined) {
      leave();
      following.onleave?.();
      return;
    }
    if (!willNotGo(row)) return;
    leave();
    following.onstop?.();
    toastStopped(row, following);
  });
  if (done) stop();
  return leave;
}
