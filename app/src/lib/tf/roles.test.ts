/**
 * What the shell still knows of the source list's role claims (ADR-0029 as
 * amended on 2026-10-19, #1300): which source answers an optional read of a
 * role, and nothing a page could drop a row by.
 */
import { expect, test } from "vitest";

import { claims, createSourceRoles, fills } from "./roles.svelte";

test("the role module offers no row filter: a surface draws what its read answered (#1300)", () => {
  // `nameOf` names a source (#1476); it filters nothing.
  expect(Object.keys(createSourceRoles()).sort()).toEqual(["answers", "forget", "nameOf", "take"]);
});

test("a source claims a role its list entry names, and fills it while its records missed none of the role's paths (#1199)", () => {
  const miss = { role: "ticket-system", kind: "ticket", field: "priority", path: "fields.priority.name" };
  const forge = {
    id: "forge",
    roles: [
      { role: "code-host", reads: [] },
      { role: "ticket-system", reads: [] },
    ],
    role_misses: [miss],
  };
  expect(claims(forge, "ticket-system")).toBe(true);
  expect(fills(forge, "ticket-system")).toBe(false);
  expect(fills(forge, "code-host")).toBe(true);
  expect(claims({ roles: [] }, "code-host")).toBe(false);
});

test("an optional read is answered only by a listed source whose claim names it (#1208)", () => {
  const roles = createSourceRoles();
  expect(roles.answers("ticket-system", "jira", "translate_query")).toBe(false);
  roles.take([
    { id: "jira", roles: [{ role: "ticket-system", reads: ["reachable_transitions", "translate_query"] }], role_misses: [] },
    { id: "gitea", roles: [{ role: "ticket-system", reads: ["reachable_transitions"] }], role_misses: [] },
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

  roles.forget();
  expect(roles.answers("ticket-system", "jira", "translate_query")).toBe(false);
});

test("a source is named by its display name, or its id when the list never said (#1476)", () => {
  const roles = createSourceRoles();
  expect(roles.nameOf("jira")).toBe("jira");
  roles.take([
    { id: "jira", display_name: "Tidewater Jira", roles: [], role_misses: [] },
    { id: "mock", roles: [], role_misses: [] },
  ]);
  expect([roles.nameOf("jira"), roles.nameOf("mock"), roles.nameOf("gone")]).toEqual(["Tidewater Jira", "mock", "gone"]);
});
