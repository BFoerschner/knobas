/**
 * What the echo suites speak for (#2102–#2105): the bridge's `listen`, which
 * the test raises `sync:state` and `item:refreshed` through as the backend
 * reports a run's end or a refresh, and a write follower the test says
 * *sent* through as the queue's next read stops listing the write.
 */
import { flushSync } from "svelte";

import type { Hear, WriteFollower } from "../shell/follow-write";
import type { EchoListen } from "./queued.svelte";

/** A `listen` port the test raises events through; each raise waits `settle` for the re-read. */
export function echoEvents(settle: () => Promise<void>) {
  const heard: { event: string; handler: (event: { payload: unknown }) => void }[] = [];
  const raise = async (event: string, payload: unknown) => {
    for (const entry of [...heard]) if (entry.event === event) entry.handler({ payload });
    await settle();
  };
  const listen: EchoListen = async (event, handler) => {
    const entry = { event, handler };
    heard.push(entry);
    return () => void heard.splice(heard.indexOf(entry), 1);
  };
  return {
    listen,
    /** `source`'s run ends, as `sync:state` says it. */
    ended: (source: string) => raise("sync:state", { source_id: source, running: false }),
    /** A refresh of `entity` after write `write` ended (#2095). */
    refreshed: (entity: string, write: number | null) => raise("item:refreshed", { entity_id: entity, write_id: write, ended: "refreshed" }),
  };
}

/** A follower the test speaks for: `sent(id)` is the queue's next read no longer listing write `id`. */
export function echoFollower() {
  const hears = new Map<number, Hear>();
  const follower: WriteFollower = {
    follow(id, hear) {
      hears.set(id, hear);
      return () => hears.delete(id);
    },
  };
  const sent = (id: number) => {
    hears.get(id)!(undefined);
    flushSync();
  };
  return { follower, sent, hear: (id: number, row: unknown) => hears.get(id)?.(row as never) };
}
