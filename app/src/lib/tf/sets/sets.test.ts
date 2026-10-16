/** Link sets' words (#866): a rule's line in prototype-6's spelling, and the per-kind contents. */
import { expect, test } from "vitest";

import { contentLines, ruleLabel } from "./sets";

test("a rule reads as prototype-6 writes it, the epic's title only when it is not its key", () => {
  expect(ruleLabel({ kind: "epic", value: "jira:PAY-200", title: "Payout reliability" })).toBe(
    "Every ticket under epic PAY-200 Payout reliability",
  );
  expect(ruleLabel({ kind: "epic", value: "jira:PAY-200", title: "PAY-200" })).toBe("Every ticket under epic PAY-200");
  expect(ruleLabel({ kind: "parent", value: "jira:PAY-231", title: null })).toBe("Every sub-task of PAY-231");
  expect(ruleLabel({ kind: "project", value: "jira:OPS", title: "Operations" })).toBe("Every ticket in project OPS");
  expect(ruleLabel({ kind: "manual", value: "jira:PAY-240", title: null })).toBe("PAY-240, applied by hand");
});

test("what is in a set is one line per kind that holds anything, in panel order", () => {
  expect(
    contentLines({
      members: [
        { kind: "monitor", entity_id: "kuma:1", title: null },
        { kind: "repo", entity_id: "gitea:a", title: null },
        { kind: "repo", entity_id: "gitea:b", title: null },
      ],
    }),
  ).toEqual(["2 repositories", "1 monitors"]);
});
