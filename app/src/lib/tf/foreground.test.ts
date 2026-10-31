import { expect, test } from "vitest";

import { foregroundOf } from "./foreground";
import { parseTfHash } from "./route";

const JOB = "jira:PAY-231";

function at(hash: string, job: string | null = null) {
  return foregroundOf(parseTfHash(hash), job)?.entity_id ?? null;
}

test("an entity page puts its entity in front, before the job", () => {
  expect(at("#/pr/gitea%3Aacme%2Fpayouts%23144", JOB)).toBe("gitea:acme/payouts#144");
  expect(at("#/repo/gitea%3Aacme%2Fpayouts", JOB)).toBe("gitea:acme/payouts");
  expect(at("#/page/confluence%3AENG%3ASEPA%20design", JOB)).toBe("confluence:ENG:SEPA design");
  expect(at("#/asset/asset%3Apayouts-api", JOB)).toBe("asset:payouts-api");
});

test("a ticket page puts its ticket in front, before the chosen job (spec #2227)", () => {
  expect(at("#/ticket/jira%3APAY-240", JOB)).toBe("jira:PAY-240");
  expect(at("#/ticket/jira%3APAY-240")).toBe("jira:PAY-240");
});

test("a branch or a commit is in front while its page sends the reader to its repository (#1440)", () => {
  expect(at("#/branch/gitea%3Aacme%2Fpayouts%40main", JOB)).toBe("gitea:acme/payouts@main");
  expect(at("#/commit/gitea%3Aabc123", JOB)).toBe("gitea:abc123");
});

test("a run is the run, whether the address names its pipeline yet or not", () => {
  expect(at("#/build/teamcity%3Arun-42")).toBe("teamcity:run-42");
  expect(at("#/build/teamcity%3APayouts_Build/teamcity%3Arun-42")).toBe("teamcity:run-42");
  expect(at("#/build/teamcity%3APayouts_Build/teamcity%3Arun-42/tests")).toBe("teamcity:run-42");
});

test("moving between the tabs of one entity keeps one foreground", () => {
  const pr = "gitea:acme/payouts#144";
  for (const tab of ["", "/files", "/checks", "/nonsense"]) expect(at(`#/pr/gitea%3Aacme%2Fpayouts%23144${tab}`)).toBe(pr);
  const repo = "gitea:acme/payouts";
  for (const tab of ["", "/files/src/main.rs", "/commits/abc", "/branches", "/compare/feature/x"])
    expect(at(`#/repo/gitea%3Aacme%2Fpayouts${tab}`)).toBe(repo);
  expect(at("#/page/confluence%3AENG%3A42/3")).toBe("confluence:ENG:42");
  for (const tab of ["", "/conv", "/map", "/code"]) expect(at(`#/job/PAY-231${tab}`, JOB)).toBe(JOB);
});

test("with no entity page, the open job's ticket is in front", () => {
  expect(at("#/job/PAY-231", JOB)).toBe(JOB);
});

test("a list, a space or the tree with nothing selected is nothing, never the last job", () => {
  for (const hash of [
    "#/tickets",
    "#/time/2026-09-03",
    "#/builds",
    "#/builds/teamcity%3APayouts_Build",
    "#/docs",
    "#/docs/space/ENG",
    "#/docs/space/wiki%3AENG",
    "#/assets",
    "#/route/route%3Aget-payouts",
    "#/deps",
    "#/code",
    "#/settings/notes",
    "#/nowhere/at-all",
  ])
    expect(at(hash), hash).toBeNull();
});

test("an address that names no entity id is nothing", () => {
  expect(at("#/pr", JOB)).toBe(JOB);
  expect(at("#/pr/not-an-id")).toBeNull();
  expect(at("#/build/ctx%3Aroom")).toBeNull();
});
