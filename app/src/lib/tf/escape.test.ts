import { expect, test } from "vitest";

import { escStep, escTarget } from "./escape";
import { parseTfHash } from "./route";

const TICKET = { hash: "#/ticket/mock%3APAY-231", name: "PAY-231" };

test("a section's top page goes nowhere, unless a ticket card's row opened it (spec #2401)", () => {
  const top = parseTfHash("#/monitors");
  // The nav's own click leaves an opener too, and Esc must not undo it.
  expect(escTarget(top, TICKET)).toBeNull();
  expect(escTarget(top, TICKET, true)).toEqual({ ...TICKET, opened: true });
  // Held but with no opener, as after a reload: nowhere to go.
  expect(escTarget(top, null, true)).toBeNull();
});

test("Esc's step takes the same answer as the breadcrumb's hint", () => {
  const route = parseTfHash("#/monitors");
  const state = { layers: [], field: null, route, opener: TICKET };
  expect(escStep(state)).toEqual({ do: "nothing" });
  expect(escStep({ ...state, fromTicketRow: true })).toEqual({ do: "go", hash: TICKET.hash, opened: true });
});
