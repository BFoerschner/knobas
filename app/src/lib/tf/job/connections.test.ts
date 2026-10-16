/** The reasons and groups the job draws for its link sets (#867). */
import { expect, test } from "vitest";

import { hiddenOf, viaLabel } from "./connections";

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
