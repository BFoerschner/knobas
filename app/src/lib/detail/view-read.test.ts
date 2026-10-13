/**
 * Reads made ahead are lent to the panel that opens next, once, and only
 * while fresh (#703): a loan the reader walked away from must not answer a
 * later opening, which would then draw what it never asked for.
 */
import { afterEach, expect, test, vi } from "vitest";

import type { EntityDetail } from "../ipc/entity";
import { createViewMemory, LOAN_MS } from "./view-read";

const loan = (since: number) => ({
  row: Promise.resolve({} as EntityDetail),
  view: Promise.resolve(null),
  since,
});

afterEach(() => vi.useRealTimers());

test("a loan is borrowed once", () => {
  const memory = createViewMemory();
  const lent = loan(Date.now());
  memory.lend("gitea:a/b", lent);
  expect(memory.borrow("gitea:a/b")).toBe(lent);
  expect(memory.borrow("gitea:a/b"), "the next opening reads for itself").toBeNull();
});

test("a loan older than LOAN_MS is not borrowed", () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  const memory = createViewMemory();
  memory.lend("gitea:a/b", loan(Date.now()));
  vi.advanceTimersByTime(LOAN_MS + 1);
  expect(memory.borrow("gitea:a/b")).toBeNull();
});

test("clearing the memory drops its loans", () => {
  const memory = createViewMemory();
  memory.lend("gitea:a/b", loan(Date.now()));
  memory.clear();
  expect(memory.borrow("gitea:a/b")).toBeNull();
});
