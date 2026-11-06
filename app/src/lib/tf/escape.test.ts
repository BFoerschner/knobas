import { afterEach, beforeEach, describe, expect, test } from "vitest";

import { escHolder, escStep, escTarget, escThrough, escThroughUntouched, openerName, windowTitle } from "./escape";
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

test("a field Esc passes through goes to the rungs below at once, and steps out only where they do nothing (F-255)", () => {
  const route = parseTfHash("#/pr/gh:pr-12");
  const field = { filter: false, text: "", through: true };
  expect(escStep({ layers: [], field, route, opener: TICKET })).toEqual({ do: "go", hash: TICKET.hash, opened: true });
  expect(escStep({ layers: [{ kind: "panel" as const }], field, route, opener: TICKET })).toEqual({ do: "close", index: 0 });
  expect(escStep({ layers: [], field, route: parseTfHash("#/code/repos"), opener: null })).toEqual({ do: "blur" });
  expect(escStep({ layers: [], field: { ...field, through: false }, route, opener: TICKET })).toEqual({ do: "blur" });
});

describe("escThroughUntouched", () => {
  let off: (() => void) | void;
  let input: HTMLInputElement;
  beforeEach(() => {
    input = document.createElement("input");
    document.body.append(input);
    off = escThroughUntouched(input);
  });
  afterEach(() => {
    off?.();
    document.body.replaceChildren();
  });

  test("the app's focus marks it", () => {
    input.focus();
    expect(escThrough(input)).toBe(true);
  });

  test("a Tab or a press into it is the reader's", () => {
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab" }));
    input.focus();
    expect(escThrough(input)).toBe(false);
    input.blur();
    input.dispatchEvent(new Event("pointerdown"));
    input.focus();
    expect(escThrough(input)).toBe(false);
  });

  test("a key in it ends the mark, and Esc doesn't", () => {
    input.focus();
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(escThrough(input)).toBe(true);
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft" }));
    expect(escThrough(input)).toBe(false);
  });

  test("words typed end it for good, so the app's focus doesn't bring it back", () => {
    input.focus();
    input.dispatchEvent(new Event("input"));
    expect(escThrough(input)).toBe(false);
    input.blur();
    input.focus();
    expect(escThrough(input)).toBe(false);
  });

  test("leaving ends it, and the app's focus brings it back", () => {
    input.focus();
    input.blur();
    expect(escThrough(input)).toBe(false);
    input.focus();
    expect(escThrough(input)).toBe(true);
  });
});

test("rung 4 ends on the field's composer, else its form or section, else the page's heading, and never the body", () => {
  document.body.innerHTML = `<main class="tf-main" tabindex="-1"><h1 id="h">Code</h1><input id="bare" />
    <section id="s"><input id="in-s" /><form id="f"><input id="in-f" /></form>
      <div class="tf-compose" id="c" tabindex="-1"><form><textarea id="in-c"></textarea></form></div></section></main>`;
  const at = (id: string) => document.getElementById(id)!;
  expect(escHolder(at("in-c"))?.id).toBe("c");
  expect(escHolder(at("in-f"))?.id).toBe("f");
  expect(escHolder(at("in-s"))?.id).toBe("s");
  expect(escHolder(at("bare"))?.id).toBe("h");
  at("h").remove();
  expect(escHolder(at("bare"))?.className).toBe("tf-main");
  expect(at("f").getAttribute("tabindex")).toBe("-1");
  document.body.replaceChildren();
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
