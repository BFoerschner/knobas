/**
 * What a room's Assets tile reads — the per-room rule, on its own and pure.
 *
 * Spec #427 gives the tile a different read in each of the four kinds of room,
 * and the *room's own filter* is what decides which (§"The Kuma room and
 * tiles"): a **stored** room lists its member assets; *All work* lists the
 * estate's top level; a **source** room lists the assets that source's
 * monitors attach to; a **project** room lists nothing, because a project is a
 * source's grouping of its own items and an estate has no such grouping to
 * guess at.
 *
 * Only the stored room's read is built (#434). The other three are **#435**'s,
 * and until then this answers `none` for them, which is what the tile draws
 * nothing for. That is why the answer is a tagged union of one useful member
 * rather than a boolean: #435 adds members here and every caller keeps
 * compiling until it has handled them.
 *
 * A module of its own, and pure, because it is the one part of the tile that
 * can be wrong without a database or a window — the rule spec #427 names as a
 * frontend test target in as many words ("the tile's per-room rule").
 */
import type { RoomContext } from "./contexts";

/**
 * The room's narrowing, which is all this rule reads.
 *
 * Taken off `RoomContext` rather than spelled as its own
 * `Pick<EntityFilter, …>`: this is *the room's filter*, and a second
 * hand-written copy of that shape could drift from the one the switcher
 * actually produces without either side failing to compile.
 */
export type RoomFilter = RoomContext["filter"];

/**
 * What the tile reads for this room.
 *
 * `none` is *this room has no Assets tile*, not *this room's tile is empty*:
 * the room draws no tile at all, the way it draws no Docs tile for a corpus
 * with no pages.
 */
export type AssetsTileRead =
  | {
      /** A stored room: the context's member assets, ancestors included. */
      kind: "members";
      /** The context whose membership scopes the read. */
      ctx: string;
    }
  | { kind: "none" };

/**
 * The rule.
 *
 * A stored room is the one that carries a `context`; every derived room leaves
 * it null and narrows by `sources`, `project`, or nothing (ADR-0010). So the
 * one field decides, and a project room — which sets `sources` *and*
 * `project` and no context — falls to `none` by that same reading rather than
 * by a case of its own.
 */
export function assetsTileRead(filter: RoomFilter): AssetsTileRead {
  const ctx = filter.context;
  if (ctx !== null && ctx !== undefined && ctx !== "") {
    return { kind: "members", ctx };
  }
  return { kind: "none" };
}

/**
 * Whether this room draws an Assets tile at all.
 *
 * A second question rather than the room reading `.kind !== "none"` itself:
 * the room asks *do I draw one*, which is a boolean and always will be, and
 * the tile asks *what do I read*, which #435 grows. Keeping them apart is what
 * lets the union grow without `Room.svelte` learning its members.
 */
export function roomDrawsAssets(filter: RoomFilter): boolean {
  return assetsTileRead(filter).kind !== "none";
}

/** The tile's id in the room's grid — the key the maximise gesture uses. */
export const ASSETS_TILE = "assets";
