import { expect, test } from "vitest";

import { isTopPage, jobHash, NAV, parentOf, parseTfHash, sectionOf, ticketHash } from "./route";

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

test("the asset tree is a section of its own, with no nav entry, and an asset's parent is its top page (2026-11-05)", () => {
  for (const hash of ["#/assets", "#/assets/tree", "#/asset/vm-pay-01", "#/route/route:payouts"]) {
    expect(sectionOf(parseTfHash(hash)), hash).toBe("assets");
  }
  expect(parentOf(parseTfHash("#/asset/vm-pay-01"))).toBe("#/assets");
  expect(parentOf(parseTfHash("#/route/route:payouts"))).toBe("#/assets");
  expect(NAV).not.toContain("assets");
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
  expect(NAV).toEqual(["tickets", "inbox", "code", "builds", "docs", "monitors", "sets", "time"]);
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
