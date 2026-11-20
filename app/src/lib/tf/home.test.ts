import { expect, test } from "vitest";

import { JOB_HOME, TICKETS_HOME, homeIfBlank, homeInstead, homeOf } from "./home";
import { parseTfHash } from "./route";

function at(hash: string) {
  const replaced: string[] = [];
  return {
    hash,
    replaced,
    replace(url: string) {
      replaced.push(url);
    },
  };
}

const CHOSEN = "jira:10231";

test("a window opened on no address with no chosen job lands on All tickets, at #/tickets (2026-11-20)", () => {
  for (const blank of ["", "#", "#/"]) {
    const place = at(blank);
    homeIfBlank(place, null);
    expect(place.replaced).toEqual([TICKETS_HOME]);
  }
  expect(TICKETS_HOME).toBe("#/tickets");
});

test("a window opened on no address with a chosen job lands on home, the bare #/job that draws it (spec #2227)", () => {
  for (const blank of ["", "#", "#/"]) {
    const place = at(blank);
    homeIfBlank(place, CHOSEN);
    expect(place.replaced).toEqual([JOB_HOME]);
  }
  expect(JOB_HOME).toBe("#/job");
  expect(homeOf(CHOSEN)).toBe("#/job");
  expect(homeOf(null)).toBe("#/tickets");
});

test("an address the window was opened on is kept, chosen job or not", () => {
  for (const hash of ["#/job/PAY-231", "#/settings/backup", "#/tickets/mine", "#/job"]) {
    for (const chosen of [null, CHOSEN]) {
      const place = at(hash);
      homeIfBlank(place, chosen);
      expect(place.replaced, `${hash} with ${chosen ?? "none"} chosen`).toEqual([]);
    }
  }
});

test("a bare #/job with no chosen job goes to All tickets; with one, or a job's own address, it stays", () => {
  const instead = (hash: string, chosen: string | null) => homeInstead(parseTfHash(hash), chosen);
  expect(instead("#/job", null)).toBe("#/tickets");
  expect(instead("#/job/", null)).toBe("#/tickets");
  expect(instead("#/job", CHOSEN)).toBeNull();
  expect(instead("#/job/PAY-231", null)).toBeNull();
  expect(instead("#/tickets", null)).toBeNull();
  expect(instead("#/ticket/jira%3A10231", null)).toBeNull();
});
