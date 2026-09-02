/**
 * The fixture's suggestion handlers (#237).
 *
 * The tray's refresh awaits `detect_suggestions` and then `room_suggestions`,
 * and renders a rejection rather than an empty state — so a fixture missing
 * either key put a standing red line on every room under `?fake-ipc`. These
 * tests pin the table itself, and the two properties the tray's own contract
 * rests on: a proposal is a link with `confirmed_at: null`, and a proposal
 * with no reason is not shippable.
 *
 * This file lives under `dev/` on purpose. `house-rules.test.ts` resolves
 * every importer of the dev harness and requires the set to be exactly
 * `App.svelte`, exempting only files inside the harness directory itself;
 * a test one level up would be a second door into the fixture.
 */
import { expect, test } from "vitest";

import type { SuggestionPage } from "../../ipc/entity";
import { demoHandlers } from "./fake-tauri";

test("the four commands the tray calls are answered", () => {
  const handlers = demoHandlers();
  for (const cmd of ["detect_suggestions", "room_suggestions", "accept_suggestion", "dismiss_suggestion"]) {
    expect(typeof handlers[cmd], `no handler for ${cmd}`).toBe("function");
  }
});

/** The whole room, the way the tray asks for it on `#/ctx/all`. */
const ROOM = { sources: [], ctx: null, limit: 50 };

function page(handlers: ReturnType<typeof demoHandlers>, args: Record<string, unknown> = ROOM): SuggestionPage {
  return handlers["room_suggestions"]!(args) as SuggestionPage;
}

/**
 * The two facts the tray's contract rests on, and then the two that make a
 * row worth drawing: both evidence classes are present (the badge exists to
 * tell them apart), and both ends resolve through `get_entity`, so every row
 * is openable. `total` is checked against the page's own rows *and* against a
 * narrower `limit`, because the heading shows the room's count, not the
 * window's.
 */
test("every proposal is unconfirmed, gives a reason, shows both classes and opens at both ends", () => {
  const handlers = demoHandlers();
  const answer = page(handlers);

  expect(answer.rows.length).toBeGreaterThan(0);
  expect(answer.total).toBe(answer.rows.length);
  for (const { link, from, to } of answer.rows) {
    expect(link.confirmed_at, `${link.id} is confirmed`).toBeNull();
    expect(link.origin).toBe("suggested");
    expect(link.reason, `${link.id} has no reason`).toEqual(expect.any(String));
    expect(link.rule_class, `${link.id} has no class`).not.toBeNull();
    expect(from.entity_id).toBe(link.from_id);
    expect(to.entity_id).toBe(link.to_id);
    for (const end of [from, to]) {
      expect(() => handlers["get_entity"]!({ entityId: end.entity_id }), `${end.entity_id} does not open`).not.toThrow();
    }
  }
  const classes = new Set(answer.rows.map((row) => row.link.rule_class));
  expect(classes.has("exact_key")).toBe(true);
  expect(classes.has("similarity")).toBe(true);

  const window = page(handlers, { ...ROOM, limit: 1 });
  expect(window.rows).toHaveLength(1);
  expect(window.total).toBe(answer.total);
});

/**
 * Scoped like `list_entities`: `[]` is every source, a room the mock source is
 * not in holds nothing, and a stored context's room is empty because the
 * fixture has no link graph to resolve its membership over.
 */
test("the page is scoped to the room it is read from", () => {
  const handlers = demoHandlers();
  expect(page(handlers, { ...ROOM, sources: ["mock"] }).total).toBe(page(handlers).total);
  expect(page(handlers, { ...ROOM, sources: ["gitea"] })).toEqual({ rows: [], total: 0 });
  expect(page(handlers, { ...ROOM, ctx: "ctx:fake-1" })).toEqual({ rows: [], total: 0 });
});

/**
 * Last in the file on purpose: the answer is session state, as it is for the
 * contexts handlers, and a fresh `demoHandlers()` does not reset it. Every
 * expectation here is relative to the read before it for the same reason.
 */
test("accepting or dismissing removes the row from the next read and drops the count", () => {
  const handlers = demoHandlers();
  const before = page(handlers);
  const [accepted, dismissed] = before.rows;
  expect(accepted && dismissed, "the fixture needs two rows to answer differently").toBeTruthy();

  expect(handlers["accept_suggestion"]!({ linkId: accepted!.link.id })).toBeNull();
  const afterAccept = page(handlers);
  expect(afterAccept.total).toBe(before.total - 1);
  expect(afterAccept.rows.map((row) => row.link.id)).not.toContain(accepted!.link.id);

  expect(handlers["dismiss_suggestion"]!({ linkId: dismissed!.link.id })).toBeNull();
  const afterDismiss = page(handlers);
  expect(afterDismiss.total).toBe(before.total - 2);
  expect(afterDismiss.rows.map((row) => row.link.id)).not.toContain(dismissed!.link.id);

  // Idempotent, as the real commands are: a second answer changes nothing.
  handlers["accept_suggestion"]!({ linkId: accepted!.link.id });
  expect(page(handlers).total).toBe(before.total - 2);

  // And an id no proposal carries is refused the way the real one refuses it.
  expect(() => handlers["dismiss_suggestion"]!({ linkId: "link:fake-none" })).toThrow(
    expect.objectContaining({ code: "not_found" }),
  );
});
