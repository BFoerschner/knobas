/**
 * What a source's credential cannot write (#2040, ADR-0020 rule 4), for a
 * surface that draws a write from what the adapter declares: the inbox's
 * *Re-run*, the Builds frame's *Run…*, *Re-run* and parameters,
 * Confluence's writes (#2053): the Docs reader's *Comment*, *Edit section*
 * and *New page*, and stand-up's publish and its target picker, and Jira's
 * writes (#2052): the job's move and *Comment*, the worklog draft, the Day's
 * *Log…*, *Log all* and the create-ticket dialog. Gitea's comment, move and
 * new ticket are drawn by the same surfaces (#2054).
 *
 * A surface draws its writes as before, then asks `refused_writes` about
 * them and draws each refused one disabled with the reason (`aria-disabled`
 * and the reason as its title, as the repository page draws a refused
 * *New branch…*). A press on it queues nothing. Anything that is not an
 * answer (a rejection, an ask left out) is no verdict, and the write stays
 * as it was drawn.
 */
import { refusedWrites, type RefusedWrite, type WriteAsk } from "../ipc/entity";

export type { RefusedWrite, WriteAsk };

/** The read, injectable so a component test needs no Tauri bridge. */
export type RefusedWritesPort = (asks: WriteAsk[]) => Promise<RefusedWrite[]>;

/**
 * `asks` sent once each, and the answer; a rejection answers nothing. No
 * ask sends nothing.
 */
export async function askRefused(asks: readonly WriteAsk[], read?: RefusedWritesPort): Promise<RefusedWrite[]> {
  const once = asks.filter(
    (ask, i) => asks.findIndex((other) => other.entity === ask.entity && other.op === ask.op) === i,
  );
  if (once.length === 0) return [];
  try {
    // The bridge is reached inside the `try`, so a shell without it answers nothing too.
    return await (read ?? refusedWrites)(once);
  } catch {
    return [];
  }
}

/** Why `op` on `entity` cannot be made, by `answer`, or `null`. */
export function refusalOf(answer: readonly RefusedWrite[], entity: string | null, op: string): string | null {
  if (entity === null) return null;
  return answer.find((refused) => refused.entity === entity && refused.op === op)?.reason ?? null;
}

/**
 * {@link askRefused} for an `$effect`: hands the answer to `land` unless the
 * effect ran again or its component went first, and returns the cleanup that
 * says so: `$effect(() => askInto(asks, port, (answer) => (refused = answer)))`.
 */
export function askInto(
  asks: readonly WriteAsk[],
  read: RefusedWritesPort | undefined,
  land: (answer: RefusedWrite[]) => void,
): () => void {
  let live = true;
  void askRefused(asks, read).then((answer) => {
    if (live) land(answer);
  });
  return () => {
    live = false;
  };
}

/** How long a typed-for set stands still before {@link createAsker} asks about it (#2063). */
export const SETTLE_MS = 250;

const keyOf = (ask: WriteAsk) => JSON.stringify([ask.entity, ask.op]);

/**
 * Asks for a surface whose drawn set changes as the reader types, such as
 * the publish-target picker's hits (#2063). {@link askInto} sends one round
 * per set, and a round is one source read per entity, so a set that changes
 * per keystroke would ask per keystroke. Here each ask is sent once for the
 * asker's life, and every answer is handed to `land`, however late, since
 * it stays true for the entities it names. One that is rejected is
 * forgotten, so a later `ask` sends it again.
 *
 * `ask(now, settled, read)` sends `now` at once and `settled` once
 * {@link SETTLE_MS} pass without another `ask`, and returns the cleanup that
 * cancels the wait, for an `$effect`. `stop()`, for the surface's teardown,
 * lands nothing more.
 */
export function createAsker(land: (answer: RefusedWrite[]) => void) {
  const asked = new Set<string>();
  let stopped = false;
  const send = (asks: readonly WriteAsk[], read: RefusedWritesPort | undefined) => {
    const fresh = asks.filter((ask) => {
      const key = keyOf(ask);
      if (asked.has(key)) return false;
      asked.add(key);
      return true;
    });
    if (fresh.length === 0) return;
    // `then` reaches the bridge, so a shell without it is a rejection too.
    void Promise.resolve()
      .then(() => (read ?? refusedWrites)(fresh))
      .then(
        (answer) => {
          if (!stopped) land(answer);
        },
        () => {
          for (const ask of fresh) asked.delete(keyOf(ask));
        },
      );
  };
  return {
    ask(now: readonly WriteAsk[], settled: readonly WriteAsk[], read: RefusedWritesPort | undefined): () => void {
      send(now, read);
      if (settled.every((ask) => asked.has(keyOf(ask)))) return () => {};
      const wait = setTimeout(() => send(settled, read), SETTLE_MS);
      return () => clearTimeout(wait);
    },
    stop() {
      stopped = true;
    },
  };
}

/** A refused write's control's attributes (a button, a select): `aria-disabled` and the reason as its title; none where `reason` is `null`. */
export function refusedAttrs(reason: string | null): { "aria-disabled"?: "true"; title?: string } {
  return reason === null ? {} : { "aria-disabled": "true", title: reason };
}
