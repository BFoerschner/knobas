import { expect, test } from "vitest";

import { escStep, escTarget, openerName, windowTitle } from "./escape";
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
  expect(escStep({ ...state, visiting: true })).toEqual({ do: "go", hash: TICKET.hash, opened: true });
});

test("an inline panel closes before Esc leaves the page, after a field has let go and under any dialog", () => {
  const route = parseTfHash("#/pr/gh:pr-12");
  const state = { layers: [{ kind: "panel" as const }], field: null, route, opener: TICKET };
  expect(escStep(state)).toEqual({ do: "close", index: 0 });
  expect(escStep({ ...state, field: { filter: false, text: "" } })).toEqual({ do: "blur" });
  expect(escStep({ ...state, layers: [{ kind: "panel" as const }, { kind: "dialog" as const }] })).toEqual({ do: "close", index: 1 });
});

test("an empty filter falls through to what Esc would do without it, and blurs only where that is nothing", () => {
  const route = parseTfHash("#/pr/gh:pr-12");
  const field = { filter: true, text: "" };
  expect(escStep({ layers: [], field: { filter: true, text: "pay" }, route, opener: TICKET })).toEqual({ do: "clear-filter" });
  expect(escStep({ layers: [], field, route, opener: TICKET })).toEqual({ do: "go", hash: TICKET.hash, opened: true });
  expect(escStep({ layers: [{ kind: "panel" as const }], field, route, opener: TICKET })).toEqual({ do: "close", index: 0 });
  expect(escStep({ layers: [], field, route: parseTfHash("#/code/repos"), opener: null })).toEqual({ do: "blur" });
});

test("a hint names what Esc lands on: a section's deeper address by the name the page gave itself", () => {
  expect(openerName(parseTfHash("#/builds/teamcity:Payout_Lint"), "Payout_Lint")).toBe("Payout_Lint");
  expect(openerName(parseTfHash("#/docs/space/ENG"), "Engineering")).toBe("Engineering");
  expect(openerName(parseTfHash("#/sets/payout-stack"), "Payout stack")).toBe("Payout stack");
  // A page that has not named itself yet is called by the section.
  expect(openerName(parseTfHash("#/sets/payout-stack"), null)).toBe("Link sets");
  expect(openerName(parseTfHash("#/build/p/4811"), "#4811")).toBe("Run #4811");
});

test("the asset tree's bare root goes back to what opened the tree; with nothing, it stays", () => {
  const root = parseTfHash("#/assets/tree");
  expect(escTarget(root, TICKET)).toEqual({ ...TICKET, opened: true });
  expect(escTarget(root, null)).toBeNull();
});

test("a marked monitor with no opener goes to Monitors, with one to what opened it", () => {
  const marked = parseTfHash("#/monitors/at/kuma%3A3");
  expect(escTarget(marked, null)).toEqual({ hash: "#/monitors", name: "Monitors", opened: false });
  const inbox = { hash: "#/inbox", name: "Inbox" };
  expect(escTarget(marked, inbox)).toEqual({ ...inbox, opened: true });
});

test("the window is named for the page, by what the page calls itself (F-015)", () => {
  expect(windowTitle(parseTfHash("#/code"), null)).toBe("Code · knobas");
  expect(windowTitle(parseTfHash("#/settings/appearance"), "Appearance")).toBe("Appearance · knobas");
  expect(windowTitle(parseTfHash("#/ticket/jira%3APAY-231"), "PAY-231")).toBe("PAY-231 · knobas");
});
