import { afterEach, beforeEach, describe, expect, test } from "vitest";

import { escHolder, escStep, escTarget, escThrough, escThroughUntouched, openerName, windowTitle } from "./escape";
import { parseTfHash } from "./route";

const TICKET = { hash: "#/ticket/mock%3APAY-231", name: "PAY-231" };

test("a section's own page goes nowhere, whatever opened it: a ticket card's row, the launcher, Settings' cog (#2559)", () => {
  for (const hash of ["#/monitors", "#/sets", "#/time", "#/builds", "#/settings", "#/settings/notes", "#/code/prs"]) {
    expect(escTarget(parseTfHash(hash), TICKET), hash).toBeNull();
    expect(escTarget(parseTfHash(hash), null), hash).toBeNull();
  }
});

test("Esc goes up the page's own section, never back to the opener in another (#2559)", () => {
  // A repository opened from a ticket's page goes to Code, not back to the ticket.
  expect(escTarget(parseTfHash("#/repo/gh%3Arepo-1/commits"), TICKET)).toEqual({ hash: "#/code", name: "Code", opened: false });
  // What the page says it sits under wins over the opener too.
  const pipeline = { hash: "#/builds/teamcity%3APayout_Lint", name: "Payout_Lint" };
  expect(escTarget(parseTfHash("#/build/teamcity%3APayout_Lint/4811"), TICKET, pipeline)).toEqual({ ...pipeline, opened: false });
  // An item's page and a note's page have no section, so nothing, opener or not.
  expect(escTarget(parseTfHash("#/item/x%3A1"), TICKET)).toBeNull();
  expect(escTarget(parseTfHash("#/note/n1"), TICKET)).toBeNull();
});

test("the opener says only how: the parent it is, the shell steps back to, by the name the page gave itself", () => {
  const pipeline = { hash: "#/builds/teamcity%3APayout_Lint", name: "Payout_Lint" };
  const run = parseTfHash("#/build/teamcity%3APayout_Lint/4811");
  expect(escTarget(run, pipeline, { ...pipeline, name: "Payout lint" })).toEqual({ ...pipeline, opened: true });
  const tickets = { hash: "#/tickets", name: "Tickets" };
  expect(escTarget(parseTfHash("#/ticket/mock%3APAY-231"), tickets)).toEqual({ ...tickets, opened: true });
});

test("Esc's step takes the same answer as the breadcrumb's hint", () => {
  const route = parseTfHash("#/monitors/at/kuma%3A3");
  const state = { layers: [], field: null, route, opener: TICKET };
  expect(escStep(state)).toEqual({ do: "go", hash: "#/monitors", opened: false });
  expect(escTarget(route, TICKET)).toEqual({ hash: "#/monitors", name: "Monitors", opened: false });
  expect(escStep({ ...state, route: parseTfHash("#/monitors") })).toEqual({ do: "nothing" });
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
  expect(escStep({ layers: [], field, route, opener: TICKET })).toEqual({ do: "go", hash: "#/code", opened: false });
  expect(escStep({ layers: [{ kind: "panel" as const }], field, route, opener: TICKET })).toEqual({ do: "close", index: 0 });
  expect(escStep({ layers: [], field, route: parseTfHash("#/code/repos"), opener: null })).toEqual({ do: "blur" });
});

test("a field Esc passes through goes to the rungs below at once, and steps out only where they do nothing (F-255)", () => {
  const route = parseTfHash("#/pr/gh:pr-12");
  const field = { filter: false, text: "", through: true };
  expect(escStep({ layers: [], field, route, opener: TICKET })).toEqual({ do: "go", hash: "#/code", opened: false });
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

test("Assets' own page is a top page like Monitors': Esc there goes nowhere (2026-11-19, #2559)", () => {
  const root = parseTfHash("#/assets/tree");
  expect(escTarget(root, TICKET)).toBeNull();
  expect(escTarget(root, null)).toBeNull();
  // An asset in it goes to Assets, or the holder its page names, not to what opened it.
  expect(escTarget(parseTfHash("#/asset/vm-pay-01"), TICKET)).toEqual({ hash: "#/assets", name: "Assets", opened: false });
});

test("a marked monitor goes to Monitors, whatever opened it", () => {
  const marked = parseTfHash("#/monitors/at/kuma%3A3");
  expect(escTarget(marked, null)).toEqual({ hash: "#/monitors", name: "Monitors", opened: false });
  const inbox = { hash: "#/inbox", name: "Inbox" };
  expect(escTarget(marked, inbox)).toEqual({ hash: "#/monitors", name: "Monitors", opened: false });
});

test("the window is named for the page, by what the page calls itself (F-015)", () => {
  expect(windowTitle(parseTfHash("#/code"), null)).toBe("Code · knobas");
  expect(windowTitle(parseTfHash("#/settings/appearance"), "Appearance")).toBe("Appearance · knobas");
  // A child of a section is named by its own crumb (the subtrees ruling, 2026-11-21).
  expect(windowTitle(parseTfHash("#/settings/appearance/code-colours"), "Code colours")).toBe("Code colours · knobas");
  expect(windowTitle(parseTfHash("#/settings/appearance/fonts"), "Fonts")).toBe("Fonts · knobas");
  expect(windowTitle(parseTfHash("#/ticket/jira%3APAY-231"), "PAY-231")).toBe("PAY-231 · knobas");
});

test("Esc in a field whose only holder is a pane's scroller lands on the heading, which draws no ring (F-649)", () => {
  document.body.innerHTML = `<div class="tf-main"><h1>Monitoring</h1><section class="tf-scroller"><input /></section></div>`;
  const heading = document.querySelector("h1");
  expect(escHolder(document.querySelector("input")!)).toBe(heading);
  expect(document.querySelector(".tf-scroller")!.hasAttribute("tabindex")).toBe(false);
  document.body.replaceChildren();
});

test("Esc in a field in a section inside the scroller still lands on that section", () => {
  document.body.innerHTML = `<div class="tf-main"><h1>Monitoring</h1><section class="tf-scroller"><section class="inner"><input /></section></section></div>`;
  expect(escHolder(document.querySelector("input")!)).toBe(document.querySelector(".inner"));
  document.body.replaceChildren();
});

describe("a site's page (ADR-0050, decisions §9)", () => {
  const site = { view: "site", source: "wiki" } as const;
  const opener = { hash: "#/ticket/jira%3APAY-231", name: "PAY-231" };

  const page = { hash: "#/page/confluence%3A2", name: "On-call handbook" };

  test("the first Esc arms, the second goes to the page knobas holds for the site, never to the opener (#2559)", () => {
    expect(escStep({ layers: [], field: null, route: site, opener, landing: page })).toEqual({ do: "arm" });
    expect(escStep({ layers: [], field: null, route: site, opener, landing: page, armed: true })).toEqual({ do: "go", hash: page.hash, opened: false });
  });

  test("where knobas holds no page for the site, Esc is the site's alone: nothing, armed or not, opener or not", () => {
    for (const from of [opener, null]) {
      expect(escStep({ layers: [], field: null, route: site, opener: from })).toEqual({ do: "nothing" });
      expect(escStep({ layers: [], field: null, route: site, opener: from, armed: true })).toEqual({ do: "nothing" });
    }
  });

  test("a layer closes first and arms nothing", () => {
    expect(escStep({ layers: [{ kind: "dialog" }], field: null, route: site, opener, landing: page })).toEqual({ do: "close", index: 0 });
  });

  // Björn, 2026-11-25: "exit to the current page of the web version if I browsed inside knobas then go back with double esc".
  test("browsed to another page knobas holds, the second Esc lands on it, in place of the site's entry", () => {
    const landing = { hash: "#/page/confluence%3A9", name: "Retry budget" };
    expect(escStep({ layers: [], field: null, route: site, opener, landing })).toEqual({ do: "arm" });
    expect(escStep({ layers: [], field: null, route: site, opener, armed: true, landing })).toEqual({ do: "go", hash: landing.hash, opened: false });
    expect(escTarget(site, opener, null, landing)).toEqual({ ...landing, opened: false, lands: true });
  });

  test("a landing on the opener's own page is the opener's step back, and with no opener the landing is still where Esc goes", () => {
    expect(escTarget(site, page, null, { hash: "#/page/confluence%3A2/3", name: "On-call handbook" })).toEqual({ ...page, opened: true });
    expect(escTarget(site, null, null, page)).toEqual({ ...page, opened: false, lands: true });
    expect(escTarget(site, opener, null, null)).toBeNull();
  });

  test("a site's page has no parent and lights no nav entry", () => {
    expect(escTarget(site, null)).toBeNull();
    expect(parseTfHash("#/site/wiki")).toEqual(site);
    expect(parseTfHash("#/site")).toEqual({ view: "unknown", hash: "#/site" });
  });
});

test("the opener stands in for the section's own page in the form the reader left it, and only that (#2559)", () => {
  const ticket = parseTfHash("#/ticket/mock%3APAY-231");
  const mine = { hash: "#/tickets/mine", name: "Mine" };
  expect(escTarget(ticket, mine)).toEqual({ ...mine, opened: true });
  const prs = { hash: "#/code/prs", name: "Code" };
  expect(escTarget(parseTfHash("#/pr/gh%3Apr-12/files"), prs)).toEqual({ ...prs, opened: true });
  // Home lights Tickets but is not its page; another pipeline is not this run's.
  expect(escTarget(ticket, { hash: "#/job", name: "Job" })).toEqual({ hash: "#/tickets", name: "Tickets", opened: false });
  const other = { hash: "#/builds/tc%3AQ", name: "Q" };
  expect(escTarget(parseTfHash("#/build/tc%3AP/4811"), other)).toEqual({ hash: "#/builds/tc%3AP", name: "Builds", opened: false });
});
