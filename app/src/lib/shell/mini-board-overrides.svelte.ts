/**
 * The reader's mini board layout overrides, per room, for the session (#245).
 *
 * The room chooses the **default** (`RoomContext.miniBoardLayout`, #210); a
 * reader may **override** it from the Tickets tile, and this is where that
 * choice lives — above the room view, keyed by room id, so it survives a walk
 * to another room and back. It holds departures from the default only: choosing
 * the room's own layout again clears the entry rather than recording it, so
 * there is no third "auto" state to reason about.
 *
 * The same shape as `contexts.svelte.ts` and `projects.svelte.ts` — one fact,
 * one home — minus their IPC: nothing here is persisted, so a restart forgets
 * every override. Durable storage is a later step on the same key (the
 * `knobas.setting` table exists for it), and it needs a command and a §10.8
 * entry of its own.
 */
import { SvelteMap } from "svelte/reactivity";

import type { MiniBoardLayout, RoomContext } from "./contexts";

export interface MiniBoardOverrides {
  /** The override for `roomId`, or `undefined` where the room draws its default. */
  overrideFor(roomId: string): MiniBoardLayout | undefined;
  /**
   * Choose `layout` for `room`. The room's own default clears the override;
   * anything else records it.
   */
  choose(room: RoomContext, layout: MiniBoardLayout): void;
}

export function createMiniBoardOverrides(): MiniBoardOverrides {
  const byRoom = new SvelteMap<string, MiniBoardLayout>();

  return {
    overrideFor(roomId) {
      return byRoom.get(roomId);
    },
    choose(room, layout) {
      if (layout === room.miniBoardLayout) byRoom.delete(room.id);
      else byRoom.set(room.id, layout);
    },
  };
}

/** The one the window uses. Tests build their own with {@link createMiniBoardOverrides}. */
export const miniBoardOverrides = createMiniBoardOverrides();
