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
 * All four are built: #434 the stored room's, #435 the rest. The answer is a
 * tagged union rather than a boolean because each of the three that reads
 * anything reads something *different* — a context's members, the estate's
 * top level, one source's monitored assets — and a caller told only "yes"
 * would have to work out which from the filter a second time.
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
  | {
      /**
       * *All work*: the estate's **top level**, with its problem counts
       * (story 43) — the first Miller column, in the widest room.
       *
       * The top level and not every asset: the widest room gives the widest
       * *view*, and a flat list of four hundred containers is the opposite of
       * one. What each root carries about its subtree is `problems_inside`,
       * which every row has already.
       */
      kind: "roots";
    }
  | {
      /**
       * A source room: the assets this source's monitors are attached to
       * (story 44), by a `monitored-by` link.
       */
      kind: "source";
      /** The source whose monitors scope the read. */
      source: string;
    }
  | { kind: "none" };

/**
 * The rule.
 *
 * Three questions in the order that makes each answer unambiguous, over the
 * three filter fields (ADR-0010):
 *
 * 1. **A `context`** is a stored room, which no derived room ever sets.
 * 2. **A `project`** is a project room — the one room that narrows by two
 *    fields at once, because a project key is unique only inside its own
 *    source. It reads nothing, and that is story 45: *"show nothing rather
 *    than something guessed"*, since a project is a source's grouping of its
 *    own items and an estate has no such grouping to guess at.
 * 3. **A `source`** is that source's room.
 *
 * What is left narrows by nothing at all, and that is *All work*.
 *
 * The order is load-bearing at step 2: a project room sets `sources` too, so
 * asking about sources first would give it a source room's read.
 *
 * A filter naming **several** sources reads nothing. No room produces one —
 * every source room names exactly its own — and a tile that picked the first
 * of them would be showing one source's monitors under a heading that claims
 * more.
 */
export function assetsTileRead(filter: RoomFilter): AssetsTileRead {
  const ctx = filter.context;
  if (named(ctx)) return { kind: "members", ctx };
  if (named(filter.project)) return { kind: "none" };
  const sources = filter.sources;
  if (sources.length === 1) return { kind: "source", source: sources[0]! };
  if (sources.length > 1) return { kind: "none" };
  return { kind: "roots" };
}

/**
 * Whether a narrowing field names anything.
 *
 * Both nullable fields cross the bridge as `string | null`, and both can also
 * arrive as `""` — the address `#/ctx/` cannot make one, but the shape allows
 * it, and a blank read as a value would send the tile after the members of a
 * context with no id. One predicate rather than the same three comparisons
 * written twice.
 */
function named(value: string | null | undefined): value is string {
  return value !== null && value !== undefined && value !== "";
}

/**
 * Whether this room draws an Assets tile at all.
 *
 * A second question rather than the room reading `.kind !== "none"` itself:
 * the room asks *do I draw one*, which is a boolean and always will be, and
 * the tile asks *what do I read*, which is a union of four. Keeping them apart
 * is what let #435 add three members without `Room.svelte` learning any of
 * them.
 */
export function roomDrawsAssets(filter: RoomFilter): boolean {
  return assetsTileRead(filter).kind !== "none";
}

/** The tile's id in the room's grid — the key the maximise gesture uses. */
export const ASSETS_TILE = "assets";
