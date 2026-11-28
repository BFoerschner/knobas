import { expect, test } from "vitest";

import { isTopPage, itemOf, jobHash, NAV, parentOf, parseTfHash, reopens, SECTION_NAME, sectionOf, ticketHash } from "./route";

test("the empty address is Tickets, as in prototype-6", () => {
  expect(parseTfHash("")).toEqual({ view: "tickets", rest: [] });
  expect(parseTfHash("#/")).toEqual({ view: "tickets", rest: [] });
  expect(parseTfHash("#/tickets/v-mine")).toEqual({ view: "tickets", rest: ["v-mine"] });
});

test("a job carries its key and its tab", () => {
  expect(parseTfHash("#/job/PAY-231/conv")).toEqual({ view: "job", key: "PAY-231", tab: "conv" });
  expect(parseTfHash("#/job/PAY-231")).toEqual({ view: "job", key: "PAY-231", tab: null });
  expect(parseTfHash("#/job")).toEqual({ view: "job", key: null, tab: null });
});

test("a deep address lights the section it belongs to", () => {
  expect(sectionOf(parseTfHash("#/pr/139"))).toBe("code");
  expect(sectionOf(parseTfHash("#/build/payout/1187"))).toBe("builds");
  expect(sectionOf(parseTfHash("#/page/4711"))).toBe("docs");
  expect(sectionOf(parseTfHash("#/settings/source/jira"))).toBe("settings");
});

test("a space's address keeps its segment whole, with or without a source (#1455)", () => {
  const docs = { view: "full", section: "docs", head: "docs" };
  expect(parseTfHash("#/docs/space/ENG")).toEqual({ ...docs, rest: ["space", "ENG"] });
  expect(parseTfHash("#/docs/space/wiki%3AENG")).toEqual({ ...docs, rest: ["space", "wiki:ENG"] });
});

test("Assets is a section of its own with its nav entry after Monitors, and an asset's parent is its top page (2026-11-19)", () => {
  for (const hash of ["#/assets", "#/assets/tree", "#/asset/vm-pay-01", "#/route/route:payouts"]) {
    expect(sectionOf(parseTfHash(hash)), hash).toBe("assets");
  }
  expect(parentOf(parseTfHash("#/asset/vm-pay-01"))).toBe("#/assets");
  expect(parentOf(parseTfHash("#/route/route:payouts"))).toBe("#/assets");
  expect(NAV.indexOf("assets")).toBe(NAV.indexOf("monitors") + 1);
  expect(SECTION_NAME.assets).toBe("Assets");
});

test("every address of the tree with nothing selected is its top page, whatever tab name it still carries", () => {
  for (const hash of ["#/assets", "#/assets/tree"]) {
    expect(isTopPage(parseTfHash(hash)), hash).toBe(true);
    expect(parentOf(parseTfHash(hash)), hash).toBeNull();
  }
});

test("Monitors keeps its own two addresses, and no asset address is one of them", () => {
  expect(sectionOf(parseTfHash("#/monitors"))).toBe("monitors");
  expect(sectionOf(parseTfHash("#/monitors/unmonitored"))).toBe("monitors");
  expect(sectionOf(parseTfHash("#/asset/vm-pay-01"))).not.toBe("monitors");
});

test("Dependencies and Proposed links have no address and no nav entry (2026-11-05)", () => {
  for (const hash of ["#/deps", "#/deps/job/PAY-231", "#/map", "#/review", "#/review/link%3A1"]) {
    expect(parseTfHash(hash), hash).toEqual({ view: "unknown", hash });
    expect(sectionOf(parseTfHash(hash)), hash).toBeNull();
  }
  expect(NAV).toEqual(["tickets", "inbox", "code", "builds", "docs", "secrets", "monitors", "assets", "sets", "time"]);
  expect(SECTION_NAME.secrets).toBe("Secrets");
  expect(parseTfHash("#/secrets/entry/pw%3Ae1")).toEqual({ view: "full", section: "secrets", head: "secrets", rest: ["entry", "pw:e1"] });
  expect(sectionOf(parseTfHash("#/secrets/in/pw%3Af1"))).toBe("secrets");
  expect(parseTfHash("#/passwords")).toEqual({ view: "unknown", hash: "#/passwords" });
  expect(parseTfHash("#/passwords/entry/pw%3Ae1")).toEqual({ view: "unknown", hash: "#/passwords/entry/pw%3Ae1" });
});

test("a rooms-UI address is unknown here, not Tickets", () => {
  expect(parseTfHash("#/ctx/all")).toEqual({ view: "unknown", hash: "#/ctx/all" });
  expect(sectionOf(parseTfHash("#/ctx/all"))).toBeNull();
});

test("a ticket page is its entity id under Tickets, and a bare #/ticket is Tickets (spec #2227)", () => {
  expect(parseTfHash("#/ticket/jira%3APAY-231")).toEqual({ view: "ticket", id: "jira:PAY-231", tab: null });
  expect(parseTfHash("#/ticket/jira:PAY-231")).toEqual({ view: "ticket", id: "jira:PAY-231", tab: null });
  expect(sectionOf(parseTfHash("#/ticket/jira:PAY-231"))).toBe("tickets");
  expect(parseTfHash("#/ticket")).toEqual({ view: "tickets", rest: [] });
  expect(parseTfHash("#/ticket/")).toEqual({ view: "tickets", rest: [] });
});

test("a job addressed by its source round-trips, its tab too, and a bare key is still a key (#1476)", () => {
  expect(jobHash("jira:PAY-231")).toBe("#/job/jira%3APAY-231");
  expect(parseTfHash(jobHash("jira:PAY-231"))).toEqual({ view: "job", key: "jira:PAY-231", tab: null });
  expect(parseTfHash(`${jobHash("jira:PAY-231")}/map`)).toEqual({ view: "job", key: "jira:PAY-231", tab: "map" });
  // Typed by hand, unencoded, it reads the same.
  expect(parseTfHash("#/job/jira:PAY-231")).toEqual({ view: "job", key: "jira:PAY-231", tab: null });
  expect(parseTfHash(jobHash("PAY-231"))).toEqual({ view: "job", key: "PAY-231", tab: null });
});

test("a ticket page carries its tab, round-trips by its entity id, and every ticket page lights Tickets (spec #2396)", () => {
  expect(ticketHash("gitea:acme/payouts#144")).toBe("#/ticket/gitea%3Aacme%2Fpayouts%23144");
  expect(parseTfHash(ticketHash("gitea:acme/payouts#144", "map"))).toEqual({ view: "ticket", id: "gitea:acme/payouts#144", tab: "map" });
  expect(parseTfHash(ticketHash("jira:PAY-231", null))).toEqual({ view: "ticket", id: "jira:PAY-231", tab: null });
  expect(sectionOf(parseTfHash("#/ticket/jira:PAY-231/conversation"))).toBe("tickets");
  expect(sectionOf(parseTfHash("#/job"))).toBe("tickets");
  expect(sectionOf(parseTfHash("#/job/jira:PAY-231"))).toBe("tickets");
});

test("the nav has no Job entry (spec #2396)", () => {
  expect(NAV).not.toContain("job");
  expect(NAV[0]).toBe("tickets");
});

test("a nav entry comes back to a section's tab or saved view, and to no deeper place", async () => {
  const { sectionTabOf } = await import("./route");
  expect(sectionTabOf(parseTfHash("#/code/prs"))).toBe("code");
  expect(sectionTabOf(parseTfHash("#/time/week"))).toBe("time");
  expect(sectionTabOf(parseTfHash("#/monitors/unmonitored"))).toBe("monitors");
  expect(sectionTabOf(parseTfHash("#/tickets/view-1"))).toBe("tickets");
  expect(sectionTabOf(parseTfHash("#/builds/pipe-1"))).toBe("builds");
  for (const place of ["#/monitors/at/kuma%3A3", "#/sets/payout-stack", "#/docs/space/ENG", "#/pr/gh:1", "#/build/pipe-1/run-2", "#/ticket/jira:PAY-1"]) {
    expect(sectionTabOf(parseTfHash(place)), place).toBeNull();
  }
});

test("Docs comes back to its home and a search, and Settings to a section, and neither to a place inside them", async () => {
  const { sectionTabOf } = await import("./route");
  for (const own of ["#/docs", "#/docs/search/retry"]) expect(sectionTabOf(parseTfHash(own)), own).toBe("docs");
  for (const own of ["#/settings", "#/settings/appearance", "#/settings/sources"]) expect(sectionTabOf(parseTfHash(own)), own).toBe("settings");
  for (const place of ["#/docs/space/ENG", "#/page/4711", "#/settings/source/jira", "#/settings/source/jira/reenter"]) {
    expect(sectionTabOf(parseTfHash(place)), place).toBeNull();
  }
});

test("a run, a pull request and a repository are items, on any of their tabs, and nothing else is", () => {
  expect(itemOf(parseTfHash("#/build/pipe%3A1/run%3A2/log"))).toBe("#/build/pipe%3A1/run%3A2");
  expect(itemOf(parseTfHash("#/build/pipe%3A1/run%3A2"))).toBe("#/build/pipe%3A1/run%3A2");
  expect(itemOf(parseTfHash("#/pr/gh%3A1/diff"))).toBe("#/pr/gh%3A1");
  expect(itemOf(parseTfHash("#/repo/gh%3Aa%2Fb/files/src/a.rs"))).toBe("#/repo/gh%3Aa%2Fb");
  // A run's address that names no pipeline is replaced as soon as its pipeline is known.
  for (const other of ["#/build/run-2", "#/builds/pipe-1", "#/code/prs", "#/ticket/jira:PAY-1", "#/page/4711", "#/docs/search/retry", "#/settings/appearance", "#/branch/x", "#/commit/x", "#/inbox"]) {
    expect(itemOf(parseTfHash(other)), other).toBeNull();
  }
});

test("only the item's bare address reopens what was left: not a tab's, and not another item's", () => {
  expect(reopens("#/build/p/1/log", "#/build/p/1")).toBe(true);
  expect(reopens("#/build/p/1", "#/build/p/1")).toBe(true);
  expect(reopens("#/pr/gh%3A1/diff", "#/pr/gh%3A1")).toBe(true);
  expect(reopens("#/repo/x/files/a/b.rs", "#/repo/x")).toBe(true);
  expect(reopens("#/build/p/1/log", "#/build/p/1/tests")).toBe(false);
  expect(reopens("#/build/p/1/log", "#/build/p/2")).toBe(false);
  expect(reopens("#/pr/1/diff", "#/repo/1")).toBe(false);
  expect(reopens("#/repo/x/files/a", "#/repo/x/files/a")).toBe(false);
});

test("Add a source's address is a place in Settings: no section tab to come back to, and Sources above it (ux31)", async () => {
  const { parentOf, sectionTabOf } = await import("./route");
  expect(sectionTabOf(parseTfHash("#/settings/sources/add"))).toBeNull();
  expect(sectionTabOf(parseTfHash("#/settings/sources"))).toBe("settings");
  expect(parentOf(parseTfHash("#/settings/sources/add"))).toBe("#/settings/sources");
});

// #2559: every section's chain, as `parentOf`'s comment writes it down. `null` is "Esc goes nowhere".
test.each([
  ["tickets", [["#/ticket/jira%3APAY-231", "#/tickets"], ["#/tickets/mine", null], ["#/job", null]]],
  ["code", [
    ["#/repo/gh%3Ar1/files/src/a.ts", "#/repo/gh%3Ar1"],
    ["#/repo/gh%3Ar1/line/12/src/a.ts", "#/repo/gh%3Ar1"],
    ["#/repo/gh%3Ar1/edit/src/a.ts", "#/repo/gh%3Ar1/files/src/a.ts"],
    ["#/repo/gh%3Ar1/commits/gh%3Ac9", "#/repo/gh%3Ar1/commits"],
    ["#/repo/gh%3Ar1/commits", "#/code"],
    ["#/repo/gh%3Ar1/compare/main...feat", "#/code"],
    ["#/repo/gh%3Ar1", "#/code"],
    ["#/pr/gh%3Apr-12", "#/code"],
    ["#/code/prs", null],
  ]],
  // A pipeline and a folder sit under All pipelines, Builds' own page; a bare Builds draws the last place stood on, so
  // going up to it would redraw the page just left (#2570, Ruling 12). All pipelines itself goes nowhere.
  ["builds", [
    ["#/build/tc%3AP/4811", "#/builds/tc%3AP"],
    ["#/builds/tc%3AP", "#/builds/recent"],
    ["#/builds/tc%3AP/settings", "#/builds/recent"],
    ["#/builds/folder/tc/Payments/Payouts", "#/builds/recent"],
    ["#/builds/folder/~repo/gh%3Arepo-1", "#/builds/recent"],
    ["#/builds/recent", null],
    ["#/builds", null],
  ]],
  ["docs", [["#/page/confluence%3A9", "#/docs"], ["#/docs/space/ENG", "#/docs"], ["#/docs", null]]],
  ["secrets", [["#/secrets/entry/e1", "#/secrets"], ["#/secrets/in/f1", "#/secrets"]]],
  ["monitors", [["#/monitors/at/kuma%3A3", "#/monitors"], ["#/monitors/unmonitored", null]]],
  ["assets", [["#/asset/vm-1", "#/assets"], ["#/route/route%3Ap", "#/assets"]]],
  ["sets", [["#/sets/payout-stack", "#/sets"]]],
  ["time", [["#/time/week", null], ["#/time/week/2026-11-23", null]]],
  ["inbox", [["#/inbox/mentions", null]]],
  ["settings", [
    ["#/settings/source/jira", "#/settings/sources"],
    ["#/settings/sources/add", "#/settings/sources"],
    ["#/settings/checkouts/app/x", "#/settings/checkouts"],
    ["#/settings/commands/new", "#/settings/commands"],
    ["#/settings/renderers/folder/x", "#/settings/renderers"],
    ["#/settings/appearance/fonts", "#/settings/appearance"],
    ["#/settings/notes", null],
    ["#/settings", null],
  ]],
  ["no section", [["#/item/x%3A1", null], ["#/note/n1", null], ["#/site/wiki", null], ["#/nowhere", null]]],
] as const)("%s's chain goes one level up its own section (#2559)", (_, pairs) => {
  for (const [hash, up] of pairs) expect(parentOf(parseTfHash(hash)), hash).toBe(up);
});
