/**
 * The Assets tile's per-room rule (spec #427, "The Kuma room and tiles").
 *
 * Four kinds of room, one rule, and the room's own filter is its whole input —
 * so this is a table rather than four tests, and the table is the thing #435
 * grows when the derived rooms get their reads.
 *
 * `builtinContexts` builds the derived rooms rather than the filters being
 * hand-written here: what is under test is the rule against the filters the
 * switcher actually produces, and a hand-written `{ sources: [], context: null }`
 * would keep passing after the switcher stopped making anything of that shape.
 */
import { expect, test } from "vitest";

import { builtinContexts, storedContext } from "./contexts";
import { ASSETS_TILE, assetsTileRead, roomDrawsAssets } from "./assets-tile";
import type { ContextRow } from "../ipc/entity";

const ROOMS = builtinContexts(
  [{ id: "kuma", label: "Uptime Kuma" }],
  [{ source_id: "kuma", key: "PAY", name: "Payments" }],
);

/** The room `id` names, so a failure says which room it was about. */
function room(id: string) {
  const found = ROOMS.find((candidate) => candidate.id === id);
  if (!found) throw new Error(`no room ${id} in ${ROOMS.map((r) => r.id).join(", ")}`);
  return found;
}

const STORED: ContextRow = {
  id: "ctx:pay",
  kind: "adhoc",
  title: "payments stack",
  anchor_id: null,
  created_at: "2026-09-06T09:00:00Z",
  archived_at: null,
};

test("a stored room reads its member assets", () => {
  const filter = storedContext(STORED).filter;
  expect(assetsTileRead(filter)).toEqual({ kind: "members", ctx: "ctx:pay" });
  expect(roomDrawsAssets(filter)).toBe(true);
});

/**
 * The three derived rooms, which #434 does not build a read for.
 *
 * *All work*'s top level, a source room's monitored assets and a project
 * room's nothing are #435's; until then each answers `none`, and the room
 * draws no tile at all — which is a different thing from a tile that is empty,
 * and the difference is what this asserts.
 *
 * The project room is the one that could pass by accident: it is the only
 * derived room that sets two filter fields, so a rule reading "no sources" or
 * "no project" rather than "no context" would answer differently for it than
 * for *All work*.
 */
test.each([
  ["all", "All work"],
  ["src:kuma", "a source room"],
  ["proj:kuma:PAY", "a project room"],
])("%s (%s) draws no Assets tile yet", (id) => {
  const filter = room(id).filter;
  expect(assetsTileRead(filter)).toEqual({ kind: "none" });
  expect(roomDrawsAssets(filter)).toBe(false);
});

/**
 * A context of the empty string is not a context.
 *
 * The address `#/ctx/` cannot produce one, but `context` crosses the bridge as
 * a nullable string and a blank would read as "a stored room" under a plain
 * null check — and then the tile would ask the backend for the members of a
 * context with no id.
 */
test("a blank context is no context", () => {
  expect(assetsTileRead({ sources: [], context: "", project: null })).toEqual({ kind: "none" });
});

/** The grid keys the maximise gesture on this id, so it may not collide with
 * a bucket id from `kinds.ts`. */
test("the tile's id is its own", () => {
  expect(ASSETS_TILE).toBe("assets");
  expect(["tickets", "code", "builds", "docs", "notes"]).not.toContain(ASSETS_TILE);
});
