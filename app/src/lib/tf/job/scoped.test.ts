import { afterEach, expect, test } from "vitest";

import { sessionFilter } from "../filter/filter-state.svelte";
import { scopedTicket } from "./scoped.svelte";

afterEach(() => scopedTicket.clear());

test("a card's Builds row turns This job on for the rail and the runs alike", () => {
  sessionFilter("builds-pipelines").scope = "all";
  sessionFilter("builds-runs").scope = "all";
  scopedTicket.open("builds", "mock:PAY-231");
  expect(sessionFilter("builds-pipelines").scope).toBe("job");
  expect(sessionFilter("builds-runs").scope).toBe("job");
});

test("leaving ends the narrowing: each filter bar is back on the scope it had", () => {
  sessionFilter("code-repos").scope = "mine";
  sessionFilter("code-prs").scope = "all";
  scopedTicket.open("code", "mock:PAY-231");
  expect(sessionFilter("code-repos").scope).toBe("job");
  scopedTicket.clear();
  expect(sessionFilter("code-repos").scope).toBe("mine");
  expect(sessionFilter("code-prs").scope).toBe("all");
});

test("a bar a row turned on before its view first drew comes back on All", () => {
  sessionFilter("docs").scope = null;
  scopedTicket.open("docs", "mock:PAY-231");
  scopedTicket.clear();
  expect(sessionFilter("docs").scope).toBe("all");
});

test("a second row replaces the first and restores the first one's bars", () => {
  sessionFilter("monitors").scope = "all";
  scopedTicket.open("monitors", "mock:PAY-231");
  scopedTicket.openAssets("mock:PAY-231");
  expect(sessionFilter("monitors").scope).toBe("all");
  expect(scopedTicket.assetsOf).toBe("mock:PAY-231");
  expect(scopedTicket.viewJob("assets")).toBeNull();
});
