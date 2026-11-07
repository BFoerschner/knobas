/** A view's rows read in the one date form, whichever list draws them (F-049, F-221). */
import { expect, test, vi } from "vitest";

import { entityView, type SourceView } from "./entity";

let answer: unknown = null;

vi.mock("@tauri-apps/api/core", async (original) => ({
  ...(await original<typeof import("@tauri-apps/api/core")>()),
  invoke: () => Promise.resolve(answer),
}));

const row = (meta: string | null) => ({ entity_id: null, title: "x", meta, tone: null, chip: null, url: null });

test("a row's ISO date reads as the date form, and everything else in the view is left alone", async () => {
  const year = new Date().getUTCFullYear();
  const text = { type: "text", text: `${year}-08-21`, markdown: false };
  answer = { sections: [{ id: "commits", blocks: [text, { type: "entities", title: null, rows: [row(`mara · ${year}-08-21`), row("5 h ago"), row(null)] }] }] };
  const view = (await entityView("mock:repo")) as SourceView;
  const blocks = view.sections[0]!.blocks;
  expect(blocks[0]).toEqual(text);
  expect(blocks[1]!.type === "entities" && blocks[1]!.rows.map((r) => r.meta)).toEqual(["mara · 21 Aug", "5 h ago", null]);
});

test("no view is no view", async () => {
  answer = null;
  expect(await entityView("mock:repo")).toBeNull();
});
