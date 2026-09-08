/**
 * The one-keystroke link, and what it says afterwards.
 *
 * This is the launcher chain's whole payload: the launcher hands over a result
 * id and the shell calls this. What matters is that a link is drawn with the
 * default relation, that the reader is told, and that the one outcome which is
 * *not* a fault — "already linked" — reads as a sentence rather than as a
 * constraint name.
 */
import { beforeEach, expect, test, vi } from "vitest";

/** Every `create_link` this module issued. */
const writes: unknown[][] = [];
let refusal: unknown = null;

vi.mock("../ipc", async (importOriginal) => {
  const real = await importOriginal<typeof import("../ipc")>();
  return {
    ...real,
    createLink: async (...args: unknown[]) => {
      writes.push(args);
      if (refusal) throw refusal;
      return {};
    },
  };
});

const { linkChanges, linkFailureMessage, linkTo } = await import("./links.svelte");
const { toasts } = await import("../shell/toasts.svelte");

beforeEach(() => {
  writes.length = 0;
  refusal = null;
  toasts.items = [];
});

function said(): string {
  return toasts.items.map((toast) => toast.text).join(" | ");
}

/**
 * Story 17: find, Tab, link. No relation, no note — the command's own default
 * is what makes the link `related`, so nothing is sent for it.
 */
test("linking from the chain writes the pair and nothing else", async () => {
  const before = linkChanges.count;

  await linkTo("mock:PAY-231", "mock:ENG-SEPA", "SEPA retry runbook");

  expect(writes).toEqual([["mock:PAY-231", "mock:ENG-SEPA"]]);
  expect(said()).toContain("Linked SEPA retry runbook");
  expect(toasts.items[0]?.tone, "a success is not an error").not.toBe("err");
  expect(
    linkChanges.count,
    "an open detail has to hear about a link drawn from outside it",
  ).toBe(before + 1);
});

/** Story 14, from the launcher's side: a state, in words, and not a fault. */
test("a duplicate reports already linked, in words, and nothing changes", async () => {
  refusal = {
    code: "conflict",
    message: 'duplicate key value violates unique constraint "link_active_idx"',
    source_id: null,
  };
  const before = linkChanges.count;

  await linkTo("mock:PAY-231", "mock:ENG-SEPA", "SEPA retry runbook");

  expect(said()).toContain("Already linked");
  expect(said(), "the constraint name is not a sentence").not.toContain("link_active_idx");
  expect(toasts.items[0]?.tone).toBe("err");
  expect(linkChanges.count, "nothing was written, so nothing re-reads").toBe(before);
});

/** Any other refusal keeps its own words: `not_found` names what has not synced. */
test("a refusal that is not a conflict keeps the backend's own message", async () => {
  refusal = { code: "not_found", message: "mock:GONE-1 is not in the mirror", source_id: null };

  await linkTo("mock:PAY-231", "mock:GONE-1", "Gone");

  expect(said()).toContain("mock:GONE-1 is not in the mirror");
  expect(said()).not.toContain("Already linked");
});

/** A rejection that is not an `IpcError` at all still produces a sentence. */
test("a rejection that is not an IpcError is still reported", async () => {
  refusal = new Error("the bridge is not there");

  await linkTo("mock:PAY-231", "mock:ENG-SEPA", "Runbook");

  expect(said()).toContain("the bridge is not there");
});

/** The mapping on its own, since both surfaces show it. */
test("only conflict is rewritten", () => {
  expect(linkFailureMessage({ code: "conflict", message: "whatever", source_id: null })).toBe(
    "Already linked — this pair already carries that relation.",
  );
  expect(linkFailureMessage({ code: "invalid", message: "not an entity id", source_id: null })).toBe(
    "not an entity id",
  );
});
