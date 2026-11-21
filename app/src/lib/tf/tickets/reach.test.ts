/** Where a ticket can go, kept by the window (ticket-edit decisions, section 5). */
import { afterEach, beforeEach, expect, test } from "vitest";

import { sourceRoles } from "../roles.svelte";
import { askReach, forgetReach, keyStem, LIKE_MOST, moveRefusal, noteObserved, reachOf, reachSizes, setReachClock, type ReachKey } from "./reach.svelte";

const ticket = (key: string, over: Partial<ReachKey> = {}): ReachKey => ({
  entity_id: `jira:${key}`,
  source_id: "jira",
  key,
  status: "To Do",
  issue_type: "Story",
  updated_at: "2026-11-20T10:00:00Z",
  ...over,
});

/** A port that answers from `workflow` by status, and records each ask. */
function port(workflow: Record<string, string[]> = { "To Do": ["In Progress"], Done: ["In Progress"] }) {
  const asked: { entity: string; behind: boolean }[] = [];
  const read = (entity: string, behind: boolean) => {
    asked.push({ entity, behind });
    const status = entity.endsWith("-9") ? "Done" : "To Do";
    return Promise.resolve(workflow[status] ?? []);
  };
  return { asked, read };
}

let clock = 1_000_000;
beforeEach(() => {
  forgetReach();
  setReachClock(() => clock);
  sourceRoles.take([
    { id: "jira", display_name: "Tidewater Jira", roles: [], role_misses: [], write_ops: ["transition"] },
    { id: "kuma", display_name: "Kuma", roles: [], role_misses: [], write_ops: [] },
  ]);
});
afterEach(() => sourceRoles.forget());

test("a second ask for a held key sends nothing; a changed updated_at asks again", async () => {
  const { asked, read } = port();
  await askReach(ticket("PAY-1"), "select", read);
  await askReach(ticket("PAY-1"), "rest", read);
  await askReach(ticket("PAY-1"), "press", read);
  expect(asked).toHaveLength(1);
  expect(reachOf(ticket("PAY-1"))).toEqual({ statuses: ["In Progress"], standing: "exact" });
  await askReach(ticket("PAY-1", { updated_at: "2026-11-21T10:00:00Z" }), "press", read);
  expect(asked).toHaveLength(2);
});

test("a read in flight is joined, never doubled", async () => {
  const { asked, read } = port();
  const one = askReach(ticket("PAY-1"), "press", read);
  const two = askReach(ticket("PAY-1"), "pickup", read);
  await Promise.all([one, two]);
  expect(asked).toHaveLength(1);
});

test("a pickup asks again unless an exact answer landed in the last ten seconds", async () => {
  const { asked, read } = port();
  await askReach(ticket("PAY-1"), "press", read);
  clock += 9_000;
  await askReach(ticket("PAY-1"), "pickup", read);
  expect(asked).toHaveLength(1);
  clock += 2_000;
  await askReach(ticket("PAY-1"), "pickup", read);
  expect(asked).toHaveLength(2);
});

test("a rest and a selection ask behind; a press, a pickup and a page are the person's own", async () => {
  const { asked, read } = port();
  await askReach(ticket("PAY-1"), "rest", read);
  await askReach(ticket("PAY-2"), "select", read);
  await askReach(ticket("PAY-3"), "press", read);
  await askReach(ticket("PAY-4"), "pickup", read);
  await askReach(ticket("PAY-5"), "open", read);
  expect(asked.map((one) => one.behind)).toEqual([true, true, false, false, false]);
});

test("a like answer is served for a sibling, and replaced by the sibling's own", async () => {
  const { read } = port({ "To Do": ["In Progress"] });
  await askReach(ticket("PAY-1"), "press", read);
  expect(reachOf(ticket("PAY-2"))).toEqual({ statuses: ["In Progress"], standing: "like" });
  // The like key holds the status: a Done sibling is not served a To Do answer.
  expect(reachOf(ticket("PAY-9", { status: "Done" }))).toBeNull();
  // Nor one of another project.
  expect(reachOf(ticket("OPS-2"))).toBeNull();
  await askReach(ticket("PAY-2"), "press", () => Promise.resolve(["In Review", "In Progress"]));
  expect(reachOf(ticket("PAY-2"))).toEqual({ statuses: ["In Review", "In Progress"], standing: "exact" });
});

test("two rests in flight, a third sends nothing", async () => {
  const asked: string[] = [];
  const held: (() => void)[] = [];
  const slow = (entity: string) => {
    asked.push(entity);
    return new Promise<string[]>((resolve) => held.push(() => resolve(["Done"])));
  };
  const one = askReach(ticket("PAY-1"), "rest", slow);
  const two = askReach(ticket("PAY-2"), "rest", slow);
  await askReach(ticket("PAY-3"), "rest", slow);
  await Promise.resolve();
  expect(asked).toEqual(["jira:PAY-1", "jira:PAY-2"]);
  for (const go of held.splice(0)) go();
  await Promise.all([one, two]);
  const three = askReach(ticket("PAY-3"), "rest", slow);
  await Promise.resolve();
  expect(asked).toHaveLength(3);
  for (const go of held.splice(0)) go();
  await three;
});

test("a source without transition is never asked", async () => {
  const { asked, read } = port();
  expect(await askReach(ticket("1", { entity_id: "kuma:1", source_id: "kuma" }), "press", read)).toBeNull();
  expect(asked).toHaveLength(0);
});

test("forgetReach(id) drops the exact answer and keeps the like one", async () => {
  const { read } = port();
  await askReach(ticket("PAY-1"), "press", read);
  forgetReach("jira:PAY-1");
  expect(reachOf(ticket("PAY-1"))?.standing).toBe("like");
});

test("a failed read stores nothing and is kept as the failure on what is served", async () => {
  noteObserved([{ source_id: "jira", statuses: ["To Do", "Done"] }]);
  await askReach(ticket("PAY-1"), "press", () => Promise.reject(new Error("Jira is unreachable")));
  expect(reachOf(ticket("PAY-1"))).toEqual({ statuses: ["To Do", "Done"], standing: "observed", failed: "Jira is unreachable" });
});

test("the like answers are bounded, least recently used out first", async () => {
  for (let at = 0; at <= LIKE_MOST; at++) await askReach(ticket(`P${at}X-1`), "press", () => Promise.resolve(["Done"]));
  expect(reachSizes().like).toBe(LIKE_MOST);
  expect(reachOf(ticket("P0X-2"))).toBeNull();
  expect(reachOf(ticket(`P${LIKE_MOST}X-2`))?.standing).toBe("like");
});

test("the grant for a move is asked once per ticket and kept", async () => {
  const asks: unknown[] = [];
  const grants = (sent: unknown[]) => {
    asks.push(...sent);
    return Promise.resolve([{ entity: "jira:PAY-1", op: "transition", field: null, reason: "This Jira user cannot move tickets in PAY" }]);
  };
  await askReach(ticket("PAY-1"), "press", port().read, grants as never);
  await askReach(ticket("PAY-1"), "pickup", port().read, grants as never);
  await Promise.resolve();
  expect(asks).toHaveLength(1);
  expect(moveRefusal("jira:PAY-1")).toBe("This Jira user cannot move tickets in PAY");
});

test("a key's stem drops its number", () => {
  expect(keyStem("PAY-231")).toBe("PAY");
  expect(keyStem("tidewater/payout-service#37")).toBe("tidewater/payout-service");
});
