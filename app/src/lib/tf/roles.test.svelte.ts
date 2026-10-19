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

test("a source whose records missed a path of a role it claims is not drawn for that role (#1199)", () => {
  const roles = createSourceRoles();
  roles.take([
    {
      id: "forge",
      roles: [
        { role: "code-host", reads: [] },
        { role: "ticket-system", reads: [] },
      ],
      role_misses: [{ role: "ticket-system", kind: "ticket", field: "priority", path: "fields.priority.name" }],
    },
  ]);
  expect(roles.draws("ticket-system", "forge")).toBe(false);
  expect(roles.draws("code-host", "forge")).toBe(true);
  // Tickets' server-narrowed read (#1200) asks for fillers only.
  expect(roles.scope("ticket-system")).toEqual([]);

  // The next list read, after a sync whose records resolve the path, draws it again.
  roles.take([{ id: "forge", roles: [{ role: "ticket-system", reads: [] }], role_misses: [] }]);
  expect(roles.draws("ticket-system", "forge")).toBe(true);
  expect(roles.scope("ticket-system")).toBeNull();
});

test("an optional read is answered only by a listed source whose claim names it (#1208)", () => {
  const roles = createSourceRoles();
  expect(roles.answers("ticket-system", "jira", "translate_query")).toBe(false);
  roles.take([
    { id: "jira", roles: [{ role: "ticket-system", reads: ["reachable_transitions", "translate_query"] }] },
    { id: "gitea", roles: [{ role: "ticket-system", reads: ["reachable_transitions"] }] },
    {
      id: "missed",
      roles: [{ role: "ticket-system", reads: ["translate_query"] }],
      role_misses: [{ role: "ticket-system", kind: "ticket", field: "status", path: "fields.status.name" }],
    },
  ]);
  expect(roles.answers("ticket-system", "jira", "translate_query")).toBe(true);
  expect(roles.answers("ci", "jira", "translate_query")).toBe(false);
  expect(roles.answers("ticket-system", "gitea", "translate_query")).toBe(false);
  expect(roles.answers("ticket-system", "missed", "translate_query")).toBe(false);
  expect(roles.answers("ticket-system", "unlisted", "translate_query")).toBe(false);
});
