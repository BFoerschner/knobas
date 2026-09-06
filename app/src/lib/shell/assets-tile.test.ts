/**
 * The Assets tile's per-room rule (spec #427, "The Kuma room and tiles").
 *
 * Four kinds of room, one rule, and the room's own filter is its whole input —
 * so this is a table rather than four tests. #434 built the stored room's
 * case; #435 built the other three.
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
 * The three derived rooms, each with the read spec #427 gives it (stories 43,
 * 44 and 45).
 *
 * The project room is the one that could pass by accident: it is the only room
 * that sets two filter fields, so a rule asking about `sources` before
 * `project` gives it a source room's read — the tile would then list the
 * assets that source's monitors watch under a heading claiming to be about one
 * project, which is precisely the guess story 45 refuses.
 *
 * *All work* is the mirror-image trap: its filter is **empty**, not "every
 * source", so a rule reading "no sources named" as "nothing to show" would
 * leave the widest room blank.
 */
test.each([
  ["all", "All work", { kind: "roots" }, true],
  ["src:kuma", "a source room", { kind: "source", source: "kuma" }, true],
  ["proj:kuma:PAY", "a project room", { kind: "none" }, false],
])("%s (%s) reads what its kind of room reads", (id, _what, read, draws) => {
  const filter = room(id).filter;
  expect(assetsTileRead(filter)).toEqual(read);
  expect(roomDrawsAssets(filter)).toBe(draws);
});

/**
 * A filter naming two sources is nobody's room, and the honest answer is no
 * tile.
 *
 * `EntityFilter.sources` is a list and the switcher only ever puts one id in
 * it, so this is a shape the type allows and no room produces — which is
 * exactly when a rule quietly picks the first element and shows one source's
 * monitors under a heading that claims both.
 */
test("a filter over several sources is no source room", () => {
  const filter = { sources: ["kuma", "jira"], context: null, project: null };
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
  expect(assetsTileRead({ sources: [], context: "", project: null })).toEqual({ kind: "roots" });
});

/**
 * And a blank project is no project, for the same reason read the other way:
 * a room narrowing by nothing is *All work*, however many of its fields
 * arrived as empty strings rather than as nulls.
 */
test("a blank project in a source room is no project", () => {
  expect(assetsTileRead({ sources: ["kuma"], context: null, project: "" })).toEqual({
    kind: "source",
    source: "kuma",
  });
});

/** The grid keys the maximise gesture on this id, so it may not collide with
 * a bucket id from `kinds.ts`. */
test("the tile's id is its own", () => {
  expect(ASSETS_TILE).toBe("assets");
  expect(["tickets", "code", "builds", "docs", "notes"]).not.toContain(ASSETS_TILE);
});
