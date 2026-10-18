/**
 * Which sources a surface draws (#1196): the ones the source list says claim
 * its role, and every one while the list is unread or does not name it.
 */
import { expect, test } from "vitest";

import { createSourceRoles } from "./roles.svelte";

const rows = [{ source_id: "forge" }, { source_id: "tracker" }, { source_id: "gone" }];

test("before the source list is read, a surface draws every row it read", () => {
  const roles = createSourceRoles();
  expect(roles.keep("code-host", rows)).toBe(rows);
});

test("once read, a surface draws the claimers and the sources the list does not name", () => {
  const roles = createSourceRoles();
  roles.take([
    { id: "forge", roles: [{ role: "code-host", reads: [] }] },
    { id: "tracker", roles: [{ role: "ticket-system", reads: [] }] },
  ]);

  expect(roles.keep("code-host", rows).map((row) => row.source_id)).toEqual(["forge", "gone"]);
  expect(roles.keep("ticket-system", rows).map((row) => row.source_id)).toEqual(["tracker", "gone"]);

  roles.forget();
  expect(roles.keep("code-host", rows)).toBe(rows);
});

test("a listed source that names no claims claims nothing", () => {
  const roles = createSourceRoles();
  roles.take([{ id: "forge" }]);
  expect(roles.draws("code-host", "forge")).toBe(false);
});

test("a read the server narrows asks for the claimers only once a listed source lacks the claim (#1200)", () => {
  const roles = createSourceRoles();
  // Unread: nothing to narrow.
  expect(roles.scope("ticket-system")).toBeNull();

  // Every listed source claims it: nothing to narrow, so a source the list does not name still draws.
  roles.take([{ id: "jira", roles: [{ role: "ticket-system", reads: [] }] }]);
  expect(roles.scope("ticket-system")).toBeNull();

  roles.take([
    { id: "jira", roles: [{ role: "ticket-system", reads: [] }] },
    { id: "forge", roles: [{ role: "code-host", reads: [] }] },
    { id: "tracker", roles: [{ role: "ticket-system", reads: [] }] },
  ]);
  expect(roles.scope("ticket-system")).toEqual(["jira", "tracker"]);

  // No listed source claims it: a read of nothing.
  roles.take([{ id: "forge", roles: [{ role: "code-host", reads: [] }] }]);
  expect(roles.scope("ticket-system")).toEqual([]);
});
