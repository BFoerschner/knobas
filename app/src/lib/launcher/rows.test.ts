import { expect, test } from "vitest";

import type { ResultGroup, SearchResponse } from "../ipc";
import { ticketsFirst, twinTickets } from "./rows";

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

test("a ticket is a twin only where another ticket holds its key under another id (F-636)", () => {
  const item = (entity_id: string, kind = "ticket") => ({ kind, entity_id });
  const twins = twinTickets([item("jira:PAY-231"), item("mock:PAY-231"), item("jira:PAY-228"), item("jira:PAY-228"), item("wiki:PAY-231", "page")]);
  expect([...twins].sort()).toEqual(["jira:PAY-231", "mock:PAY-231"]);
});
