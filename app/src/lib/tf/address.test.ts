import { expect, test } from "vitest";
import { entityAddress, legacyAddress } from "./address";
import { page as assetTreePage } from "./asset-tree/page";
import { page as codeRowPage } from "./code-row/page";
import { page as monitorsPage } from "./monitors/page";
import { pageFor } from "./pages";
import { parseTfHash } from "./route";

test("an entity of a known kind goes where its own page is", () => {
  expect(entityAddress("ticket", "jira:PAY-231")).toBe("#/ticket/jira%3APAY-231");
  expect(entityAddress("pr", "gitea:acme/svc#142")).toBe("#/pr/gitea%3Aacme%2Fsvc%23142");
  expect(entityAddress("page", "confluence:42")).toBe("#/page/confluence%3A42");
  expect(entityAddress("asset", "asset:9f1c")).toBe("#/asset/asset%3A9f1c");
  expect(entityAddress("route", "route:9a1b")).toBe("#/route/route%3A9a1b");
});

test("a ticket opens its ticket page by its entity id, so a key two sources hold opens the one it names (#1476, #1977, spec #2227)", () => {
  expect(entityAddress("ticket", "gitea:PAY-231")).toBe("#/ticket/gitea%3APAY-231");
  expect(entityAddress("ticket", "gitea:acme/payouts#144")).toBe("#/ticket/gitea%3Aacme%2Fpayouts%23144");
  expect(parseTfHash(entityAddress("ticket", "gitea:acme/payouts#144"))).toEqual({ view: "ticket", id: "gitea:acme/payouts#144" });
});

test("a note opens its own page, which sends the reader to Settings › Notes while notes hide (#1504, D10)", () => {
  expect(entityAddress("note", "note:0f2c1a")).toBe("#/note/note%3A0f2c1a");
  expect(parseTfHash("#/note/note%3A0f2c1a")).toEqual({ view: "note", id: "note:0f2c1a" });
});

test("a note titled by the protocol rule opens Time › Standup at its date (#1504)", () => {
  expect(entityAddress("note", "note:7a1", "Standup 2026-09-03")).toBe("#/time/standup/2026-09-03");
  expect(entityAddress("note", "note:7a1", "Standup notes")).toBe("#/note/note%3A7a1");
  // The rule is a note's: a ticket that happens to be so titled is still a ticket.
  expect(entityAddress("ticket", "jira:PAY-1", "Standup 2026-09-03")).toBe("#/ticket/jira%3APAY-1");
});

test("a Jira view opens its ticket view, by the projection's slug (#1094)", () => {
  expect(entityAddress("jira_view", "jira:board-21")).toBe("#/tickets/jira-board-21");
  expect(entityAddress("jira_view", "jira:filter-10230")).toBe("#/tickets/jira-filter-10230");
});

test("a kind with no page of its own lands on its section, never on Code (#1094)", () => {
  expect(entityAddress("build_config", "teamcity:Pay_Build")).toBe("#/builds");
});

test("a kind no surface draws, such as a custom role's, opens its item page (#1201)", () => {
  expect(entityAddress("survey", "surveys:survey-1")).toBe("#/item/surveys%3Asurvey-1");
  expect(parseTfHash("#/item/surveys%3Asurvey-1")).toEqual({ view: "item", id: "surveys:survey-1" });
});

test("every address entityAddress produces has a place in the new shell", () => {
  for (const kind of ["ticket", "pr", "repo", "branch", "commit", "build", "page", "asset", "route", "monitor", "note", "mystery"]) {
    expect(parseTfHash(entityAddress(kind, "x:1")).view).not.toBe("unknown");
  }
});

test("a branch or commit opens its own address, which the repository page's redirect draws (#1440)", () => {
  expect(entityAddress("branch", "gitea:x/pay@refs/heads/main")).toBe("#/branch/gitea%3Ax%2Fpay%40refs%2Fheads%2Fmain");
  expect(entityAddress("commit", "mock:9f2c1ab")).toBe("#/commit/mock%3A9f2c1ab");
  for (const kind of ["branch", "commit"]) expect(pageFor(parseTfHash(entityAddress(kind, "x:1"))), kind).toBe(codeRowPage);
});

test("a rooms-UI detail address is read as the entity it names", () => {
  expect(legacyAddress("#/build/teamcity%3A77")).toEqual({ go: "#/build/teamcity%3A77" });
});

test("#/ticket/<id> is the ticket page now, not a rooms address; #/start-work/<id> still opens the job by its source (#1676, spec #2227)", () => {
  expect(parseTfHash("#/ticket/gitea:PAY-231")).toEqual({ view: "ticket", id: "gitea:PAY-231" });
  expect(parseTfHash("#/ticket/gitea:acme%2Fpayouts%23144")).toEqual({ view: "ticket", id: "gitea:acme/payouts#144" });
  expect(legacyAddress("#/start-work/gitea:PAY-231")).toEqual({ go: "#/job/gitea%3APAY-231" });
});

test("#/entity/<id> has no kind on hand, so it is resolved", () => {
  expect(legacyAddress("#/entity/gitea:acme%2Fpayouts%23144")).toEqual({ resolve: "gitea:acme/payouts#144" });
});

test("a room, the sources view and the wizard land on pages that exist", () => {
  expect(legacyAddress("#/ctx/all")).toEqual({ go: "#/tickets" });
  expect(legacyAddress("#/ctx/src:jira")).toEqual({ go: "#/tickets" });
  expect(legacyAddress("#/sources")).toEqual({ go: "#/settings/sources" });
  expect(legacyAddress("#/first-run")).toEqual({ go: "#/settings/sources" });
  expect(legacyAddress("#/standup")).toEqual({ go: "#/time/standup" });
  expect(legacyAddress("#/start-work/jira:PAY-231")).toEqual({ go: "#/job/jira%3APAY-231" });
});

// #1459: the rooms UI's *Monitors* tab beside *Tree* went with #1087;
// Monitors lives at `#/monitors` (R-11.3), and the Tree keeps its addresses.
test("the Assets view's old Monitors tab lands on the Monitors page, and the Tree keeps its addresses", () => {
  const old = parseTfHash("#/assets/monitors");
  expect(old).toEqual({ view: "unknown", hash: "#/assets/monitors" });
  expect(legacyAddress("#/assets/monitors")).toEqual({ go: "#/monitors" });
  expect(pageFor(parseTfHash("#/monitors"))).toBe(monitorsPage);
  expect(pageFor(parseTfHash("#/assets"))).toBe(assetTreePage);
  expect(pageFor(parseTfHash("#/assets/tree"))).toBe(assetTreePage);
  expect(pageFor(parseTfHash("#/asset/asset:vm-pay-01"))).toBe(assetTreePage);
  expect(legacyAddress("#/assets")).toBeNull();
});

test("an address that was never the rooms UI's is left alone", () => {
  expect(legacyAddress("#/nowhere/at-all")).toBeNull();
  expect(legacyAddress("#/ticket/")).toBeNull();
  expect(legacyAddress("#/entity")).toBeNull();
});
