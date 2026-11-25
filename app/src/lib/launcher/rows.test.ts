import { expect, test } from "vitest";

import type { ResultGroup, SearchResponse } from "../ipc";
import { flatten, ticketsFirst, twinItems } from "./rows";
import { navigationActions } from "./actions";

const group = (kind: string): ResultGroup => ({ kind, label: kind, plural: kind, monogram: "", total: 0, hits: [] });
const answer = (text: string): SearchResponse =>
  ({ interpreted: { text }, groups: [group("view"), group("set"), group("ticket"), group("pr")] }) as unknown as SearchResponse;
const order = (text: string) => ticketsFirst(answer(text)).map((one) => one.kind);

test("a query shaped like a key lists the tickets first, and the rest keep their order", () => {
  for (const query of ["PAY", "pay", "PAY-", "PAY-23", " PAY-231 "]) {
    expect(order(query), query).toEqual(["ticket", "view", "set", "pr"]);
  }
});

test("any other query keeps the engine's order", () => {
  for (const query of ["", "retry failed", "PAY 231", "feature/x"]) {
    expect(order(query), query).toEqual(["view", "set", "ticket", "pr"]);
  }
});

test("a query the engine read as a key typed with a space or none lists the tickets first (ux25)", () => {
  for (const [text, keyed] of [["pay 123", "PAY-123"], ["pay123", "PAY-123"], ["retry pay 12", "retry PAY-12"]] as const) {
    const response = answer(text);
    response.interpreted.keyed = keyed;
    expect(ticketsFirst(response).map((one) => one.kind), text).toEqual(["ticket", "view", "set", "pr"]);
  }
});

test("a ticket is a twin only where another ticket holds its key under another id (F-636)", () => {
  const item = (entity_id: string, kind = "ticket") => ({ kind, entity_id });
  const twins = twinItems([item("jira:PAY-231"), item("mock:PAY-231"), item("jira:PAY-228"), item("jira:PAY-228"), item("wiki:PAY-231", "page")]);
  expect([...twins].sort()).toEqual(["jira:PAY-231", "mock:PAY-231"]);
});

test("a pull request or repository two forges serve is a twin too, and names its source (the live run, 2026-11-09)", () => {
  const item = (entity_id: string, kind: string) => ({ kind, entity_id });
  const twins = twinItems([
    item("live-gitea:tidewater/payout-service#142", "pr"),
    item("live-forgejo:tidewater/payout-service#142", "pr"),
    item("mock:payout-service#142", "pr"),
    item("live-gitea:tidewater/payout-service", "repo"),
    item("live-forgejo:tidewater/payout-service", "repo"),
  ]);
  expect([...twins].sort()).toEqual([
    "live-forgejo:tidewater/payout-service",
    "live-forgejo:tidewater/payout-service#142",
    "live-gitea:tidewater/payout-service",
    "live-gitea:tidewater/payout-service#142",
  ]);
});

test("#b with no words offers Recent runs first; words or another filter do not (Builds' Recent, 2026-11-25)", () => {
  const rows = (raw: string, words: string) =>
    flatten({ mode: "results", response: { interpreted: { text: words, filters: { kinds: [] } }, groups: [] } as unknown as SearchResponse, home: null, actions: [], syntax: [], extras: { inbox: null, places: navigationActions() }, raw }).map((row) => row.id);
  expect(rows("#b ", "")[0]).toBe("action:builds-recent");
  expect(rows("#t ", "")).not.toContain("action:builds-recent");
  expect(rows("#b pay", "pay")).not.toContain("action:builds-recent");
});
