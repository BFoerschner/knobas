/** The reasons and groups the job draws for its link sets (#867). */
import { expect, test, vi } from "vitest";

import { kindRegistry } from "../../shell/kind-registry.svelte";
import { page as codeRowPage } from "../code-row/page";
import { pageFor } from "../pages";
import { parseTfHash } from "../route";
import { groupsOf, hiddenOf, sectionOf, viaLabel } from "./connections";

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

test("a run of a kind the CI adapters claim is labelled Builds, as a build is (#1282)", () => {
  const spy = vi.spyOn(kindRegistry, "frameKinds").mockReturnValue(["build", "workflow_run"]);
  try {
    expect(sectionOf("workflow_run")).toBe("Builds");
    expect(sectionOf("build")).toBe("Builds");
    expect(sectionOf("widget")).toBe("Widget");
  } finally {
    spy.mockRestore();
  }
});

test("a set's branch or commit opens its own address, which the repository page's redirect draws (#1440)", () => {
  const member = (entity_id: string, entity_kind: string) => ({ kind: "repo" as const, entity_id, entity_kind, title: entity_id, hidden: false });
  const set = {
    id: "a",
    name: "a",
    via: { kind: "manual" as const, value: "jira:PAY-1", title: null },
    members: [member("gitea:x/pay@refs/heads/main", "branch"), member("gitea:x/pay@9f2c1ab", "commit")],
  };
  const hrefs = groupsOf({ sets: [set], parent: null })[0]!.rows.map((row) => row.href!);
  expect(hrefs).toEqual(["#/branch/gitea%3Ax%2Fpay%40refs%2Fheads%2Fmain", "#/commit/gitea%3Ax%2Fpay%409f2c1ab"]);
  for (const href of hrefs) expect(pageFor(parseTfHash(href)), href).toBe(codeRowPage);
});

test("the parent's group heading opens the parent's ticket page, not its job (spec #2227)", () => {
  const parent = { ticket: "gitea:acme/payouts#144", title: null, links: [] };
  const group = groupsOf({ sets: [], parent })[0]!;
  expect(group.href).toBe("#/ticket/gitea%3Aacme%2Fpayouts%23144");
  expect(parseTfHash(group.href!)).toEqual({ view: "ticket", id: "gitea:acme/payouts#144", tab: null });
});
