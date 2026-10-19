/** The reasons and groups the job draws for its link sets (#867). */
import { expect, test, vi } from "vitest";

import { kindRegistry } from "../../shell/kind-registry.svelte";
import { groupsOf, hiddenOf, viaLabel } from "./connections";

test("a set's reason names its rule in prototype-6's words", () => {
  expect(viaLabel({ kind: "epic", value: "jira:PAY-200" })).toBe("via epic PAY-200");
  expect(viaLabel({ kind: "project", value: "jira:PAY" })).toBe("via project PAY");
  expect(viaLabel({ kind: "parent", value: "jira:PAY-231" })).toBe("via parent PAY-231");
  expect(viaLabel({ kind: "manual", value: "jira:PAY-231" })).toBe("applied by you");
});

test("a member hidden here shows once under Hidden on KEY, however many sets hold it", () => {
  const member = { kind: "repo" as const, entity_id: "gitea:x/psp-client", entity_kind: "repo", title: "psp-client", hidden: true };
  const set = (id: string) => ({ id, name: id, via: { kind: "manual" as const, value: "jira:PAY-1", title: null }, members: [member] });
  expect(hiddenOf({ sets: [set("a"), set("b")], parent: null })).toEqual([{ id: "gitea:x/psp-client", title: "psp-client" }]);
  expect(hiddenOf(null)).toEqual([]);
});

test("a run of a kind the CI adapters claim opens Builds' run page (#1278)", () => {
  const spy = vi.spyOn(kindRegistry, "frameKinds").mockReturnValue(["build", "workflow_run"]);
  try {
    const member = { kind: "repo" as const, entity_id: "forge:run#77", entity_kind: "workflow_run", title: "ci.yml #77", hidden: false };
    const set = { id: "a", name: "a", via: { kind: "manual" as const, value: "jira:PAY-1", title: null }, members: [member] };
    expect(groupsOf({ sets: [set], parent: null })[0]!.rows[0]!.href).toBe("#/build/forge%3Arun%2377");
  } finally {
    spy.mockRestore();
  }
});
