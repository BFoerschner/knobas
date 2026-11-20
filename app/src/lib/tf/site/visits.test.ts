/** After a visit (ADR-0048, decisions §15). */
import { expect, test } from "vitest";

import { createVisits } from "./visits";

function world(names: Record<string, string>, failing: string[] = []) {
  const refreshed: string[] = [];
  const asked: string[] = [];
  const visits = createVisits({
    resolve: (url) => {
      asked.push(url);
      const id = names[url];
      return Promise.resolve(id === undefined ? null : { entity_id: id, kind: "ticket" });
    },
    refresh: (id) => {
      refreshed.push(id);
      return failing.includes(id) ? Promise.reject(new Error("no")) : Promise.resolve();
    },
  });
  return { visits, refreshed, asked };
}

test("three addresses of which two resolve: leaving refreshes those two, in the order visited", async () => {
  const w = world({ "https://j/1": "jira:A-1", "https://j/3": "jira:A-3" });
  await w.visits.see("jira", "https://j/1");
  await w.visits.see("jira", "https://j/2");
  await w.visits.see("jira", "https://j/3");
  await w.visits.leave("jira");
  expect(w.refreshed).toEqual(["jira:A-1", "jira:A-3"]);
  await w.visits.leave("jira");
  expect(w.refreshed).toHaveLength(2);
});

test("seven resolved: the last five", async () => {
  const names = Object.fromEntries([1, 2, 3, 4, 5, 6, 7].map((n) => [`https://j/${n}`, `jira:A-${n}`]));
  const w = world(names);
  for (const n of [1, 2, 3, 4, 5, 6, 7]) await w.visits.see("jira", `https://j/${n}`);
  await w.visits.leave("jira");
  expect(w.refreshed).toEqual(["jira:A-3", "jira:A-4", "jira:A-5", "jira:A-6", "jira:A-7"]);
});

test("an address visited twice is one item, at its later place, and is asked of the mirror once", async () => {
  const w = world({ "https://j/1": "jira:A-1", "https://j/2": "jira:A-2" });
  await w.visits.see("jira", "https://j/1");
  await w.visits.see("jira", "https://j/2");
  await w.visits.see("jira", "https://j/1");
  await w.visits.leave("jira");
  expect(w.refreshed).toEqual(["jira:A-2", "jira:A-1"]);
  expect(w.asked).toEqual(["https://j/1", "https://j/2"]);
});

test("leaving with nothing resolved refreshes nothing", async () => {
  const w = world({});
  await w.visits.see("jira", "https://j/9");
  await w.visits.leave("jira");
  expect(w.refreshed).toEqual([]);
});

test("a refresh that rejects does not stop the next", async () => {
  const w = world({ "https://j/1": "jira:A-1", "https://j/2": "jira:A-2" }, ["jira:A-1"]);
  await w.visits.see("jira", "https://j/1");
  await w.visits.see("jira", "https://j/2");
  await w.visits.leave("jira");
  expect(w.refreshed).toEqual(["jira:A-1", "jira:A-2"]);
});

test("the answer is unknown while the read is pending, and nothing for an address that names nothing", async () => {
  const w = world({ "https://j/1": "jira:A-1" });
  const asking = w.visits.see("jira", "https://j/1");
  expect(w.visits.answer("https://j/1")).toBeUndefined();
  await asking;
  expect(w.visits.answer("https://j/1")).toEqual({ entity_id: "jira:A-1", kind: "ticket" });
  await w.visits.see("jira", "https://j/2");
  expect(w.visits.answer("https://j/2")).toBeNull();
});
