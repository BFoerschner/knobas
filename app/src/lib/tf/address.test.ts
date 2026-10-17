import { expect, test } from "vitest";
import { entityAddress, legacyAddress } from "./address";
import { parseTfHash } from "./route";

test("an entity of a known kind goes where its own page is", () => {
  expect(entityAddress("ticket", "jira:PAY-231")).toBe("#/job/PAY-231");
  expect(entityAddress("pr", "gitea:acme/svc#142")).toBe("#/pr/gitea%3Aacme%2Fsvc%23142");
  expect(entityAddress("page", "confluence:42")).toBe("#/page/confluence%3A42");
  expect(entityAddress("asset", "asset:9f1c")).toBe("#/asset/asset%3A9f1c");
  expect(entityAddress("route", "route:9a1b")).toBe("#/route/route%3A9a1b");
});

test("a note opens Settings › Notes and capture, the one place notes are drawn (D10)", () => {
  expect(entityAddress("note", "note:0f2c1a")).toBe("#/settings/notes");
});

test("every address entityAddress produces has a place in the new shell", () => {
  for (const kind of ["ticket", "pr", "repo", "branch", "commit", "build", "page", "asset", "route", "monitor", "note", "mystery"]) {
    expect(parseTfHash(entityAddress(kind, "x:1")).view).not.toBe("unknown");
  }
});

test("a rooms-UI detail address is read as the entity it names", () => {
  expect(legacyAddress("#/ticket/jira:PAY-231")).toEqual({ go: "#/job/PAY-231" });
  expect(legacyAddress("#/note/note:0f2c1a")).toEqual({ go: "#/settings/notes" });
  expect(legacyAddress("#/build/teamcity%3A77")).toEqual({ go: "#/build/teamcity%3A77" });
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

test("an address that was never the rooms UI's is left alone", () => {
  expect(legacyAddress("#/nowhere/at-all")).toBeNull();
  expect(legacyAddress("#/ticket/")).toBeNull();
  expect(legacyAddress("#/entity")).toBeNull();
});
