/**
 * The Proposed links view's decisions (#835): held for the undo window, then
 * written over `accept_suggestion`, `dismiss_suggestion` or, for a relation
 * the reader changed, `create_link`.
 */
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { LinkRow, SuggestionEntry } from "../../ipc/entity";
import { createDecisions, type DecisionPorts } from "./decisions.svelte";

const ENTRY: SuggestionEntry = {
  link: {
    id: "link-1",
    from_id: "gitea:9f2c1ab",
    to_id: "jira:PAY-231",
    relation: "related",
    origin: "suggested",
    note: null,
    created_by: "knobas",
    created_at: "2026-08-22T10:00:00Z",
    confirmed_at: null,
    rule: "commit_message_key",
    rule_class: "exact_key",
    reason: "the commit message mentions PAY-231",
  },
  from: { entity_id: "gitea:9f2c1ab", kind: "commit", title: "key the retry", deleted_at: null },
  to: { entity_id: "jira:PAY-231", kind: "ticket", title: "Retry failed SEPA payouts", deleted_at: null },
};

function ports(over: Partial<DecisionPorts> = {}) {
  const calls: string[] = [];
  const made: DecisionPorts = {
    acceptSuggestion: async (id) => void calls.push(`accept ${id}`),
    dismissSuggestion: async (id) => void calls.push(`dismiss ${id}`),
    createLink: async (from, to, relation) => {
      calls.push(`create ${from} ${to} ${relation}`);
      return { ...ENTRY.link, id: "link-made", relation: relation ?? "related" } as LinkRow;
    },
    ...over,
  };
  return { calls, ports: made };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

test("Link them is written once the undo window closes, not before", async () => {
  const { calls, ports: p } = ports();
  const decisions = createDecisions(p, 1000);
  decisions.decide(ENTRY, "yes", "related", () => {});
  expect(decisions.verdicts["link-1"]).toBe("yes");
  await vi.advanceTimersByTimeAsync(999);
  expect(calls).toEqual([]);
  await vi.advanceTimersByTimeAsync(1);
  expect(calls).toEqual(["accept link-1"]);
});

test("Not related is written as a dismissal", async () => {
  const { calls, ports: p } = ports();
  const decisions = createDecisions(p, 1000);
  decisions.decide(ENTRY, "no", "related", () => {});
  await vi.advanceTimersByTimeAsync(1000);
  expect(calls).toEqual(["dismiss link-1"]);
});

test("Decide later writes nothing and is remembered for the session", async () => {
  const { calls, ports: p } = ports();
  const decisions = createDecisions(p, 1000);
  decisions.decide(ENTRY, "skip", "related", () => {});
  await vi.advanceTimersByTimeAsync(5000);
  expect(calls).toEqual([]);
  expect(decisions.verdicts["link-1"]).toBe("skip");
});

test("Undo inside the window writes nothing and the proposal is undecided again", async () => {
  const { calls, ports: p } = ports();
  const decisions = createDecisions(p, 1000);
  decisions.decide(ENTRY, "yes", "related", () => {});
  decisions.undo("link-1");
  await vi.advanceTimersByTimeAsync(5000);
  expect(calls).toEqual([]);
  expect(decisions.verdicts["link-1"]).toBeUndefined();
});

test("a relation the reader changed is drawn as that link, and the proposal is dismissed", async () => {
  const { calls, ports: p } = ports();
  const decisions = createDecisions(p, 1000);
  decisions.decide(ENTRY, "yes", "implements", () => {});
  await vi.advanceTimersByTimeAsync(1000);
  expect(calls).toEqual(["create gitea:9f2c1ab jira:PAY-231 implements", "dismiss link-1"]);
});

test("a write that fails undoes the decision and says why", async () => {
  const { ports: p } = ports({
    acceptSuggestion: async () => {
      throw { code: "not_found", message: "link-1 is not a link", source_id: null };
    },
  });
  const decisions = createDecisions(p, 1000);
  const failures: string[] = [];
  decisions.decide(ENTRY, "yes", "related", (message) => failures.push(message));
  await vi.advanceTimersByTimeAsync(1000);
  expect(decisions.verdicts["link-1"]).toBeUndefined();
  expect(failures).toEqual(["Could not link it: link-1 is not a link"]);
});

test("the entry stays known after its write, so Decided can still list it", async () => {
  const { ports: p } = ports();
  const decisions = createDecisions(p, 1000);
  decisions.decide(ENTRY, "no", "related", () => {});
  await vi.advanceTimersByTimeAsync(1000);
  expect(decisions.entries.map((entry) => entry.link.id)).toEqual(["link-1"]);
});
