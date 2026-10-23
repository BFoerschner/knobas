/**
 * `listEntities` over a backend that answers at most `ROW_CEILING` rows a
 * read (#1643): a limit past it is read in reads of that size, up to
 * `LIST_MOST`.
 */
import { beforeEach, expect, test, vi } from "vitest";

import { LIST_MOST, listEntities, ROW_CEILING, type EntityFilter, type EntityPage, type EntityRow } from "./entity";

let room: string[] = [];
/** How far the room moved after the first read: a later read starts this many rows early. */
let moved = 0;
const reads: { limit: number; offset: number }[] = [];

vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/core")>()),
  invoke: (command: string, args: { limit: number; offset: number }) => {
    if (command !== "list_entities") return Promise.reject(new Error(`no answer for ${command}`));
    reads.push({ limit: args.limit, offset: args.offset });
    const limit = Math.min(args.limit, ROW_CEILING);
    const start = reads.length === 1 ? args.offset : args.offset - moved;
    const rows = room.slice(start, start + limit).map((id) => ({ entity_id: id }) as EntityRow);
    return Promise.resolve({ rows, total: room.length, tally: null, groups: null } satisfies EntityPage);
  },
}));

const FILTER = { sources: [], kinds: ["repo"], order: "updated_desc" } as unknown as EntityFilter;
const ids = (n: number) => Array.from({ length: n }, (_, i) => `mock:R-${i}`);

beforeEach(() => {
  reads.length = 0;
  moved = 0;
});

test("a view that asks for 1000 rows of a bigger room gets 1000, in reads of the ceiling", async () => {
  room = ids(1200);
  const page = await listEntities(FILTER, 1000, 0);
  expect(reads).toEqual([
    { limit: 500, offset: 0 },
    { limit: 500, offset: 500 },
  ]);
  expect(page.rows.map((row) => row.entity_id)).toEqual(room.slice(0, 1000));
  expect(page.total).toBe(1200);
});

test("a miscomputed limit reads at most LIST_MOST rows", async () => {
  room = ids(5000);
  const page = await listEntities(FILTER, 2 ** 32 - 1, 0);
  expect(page.rows).toHaveLength(LIST_MOST);
  expect(reads.every((read) => read.limit <= ROW_CEILING)).toBe(true);
  expect(reads).toHaveLength(2);
});

test("a room smaller than the limit stops at its end, and a limit under the ceiling is one read", async () => {
  room = ids(700);
  expect((await listEntities(FILTER, 1000, 0)).rows).toHaveLength(700);
  expect(reads).toEqual([
    { limit: 500, offset: 0 },
    { limit: 500, offset: 500 },
  ]);

  reads.length = 0;
  expect((await listEntities(FILTER, 200, 40)).rows.map((row) => row.entity_id)).toEqual(room.slice(40, 240));
  expect(reads).toEqual([{ limit: 200, offset: 40 }]);
});

test("a row the room moved into the second read is kept once", async () => {
  room = ids(800);
  moved = 1;
  const seen = (await listEntities(FILTER, 1000, 0)).rows.map((row) => row.entity_id);
  expect(seen).toEqual(room);
  expect(reads).toHaveLength(2);
});
