import { expect, test } from "vitest";

import type { ResultGroup, SearchResponse } from "../ipc";
import { ticketsFirst } from "./rows";

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
