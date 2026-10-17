/** Link sets' words (#866): a rule's line in prototype-6's spelling, and the per-kind contents. */
import { expect, test } from "vitest";

import { reasonLabel } from "../builds/builds";
import { originLabel } from "../job/job";
import type { LinkSetMemberKind } from "../../ipc/entity";
import { contentLines, hintTickets, patternQuery, ruleLabel } from "./sets";

test("a rule reads as prototype-6 writes it, the epic's title only when it is not its key", () => {
  expect(ruleLabel({ kind: "epic", value: "jira:PAY-200", title: "Payout reliability" })).toBe(
    "Every ticket under epic PAY-200 Payout reliability",
  );
  expect(ruleLabel({ kind: "epic", value: "jira:PAY-200", title: "PAY-200" })).toBe("Every ticket under epic PAY-200");
  expect(ruleLabel({ kind: "parent", value: "jira:PAY-231", title: null })).toBe("Every sub-task of PAY-231");
  expect(ruleLabel({ kind: "project", value: "jira:OPS", title: "Operations" })).toBe("Every ticket in project OPS");
  expect(ruleLabel({ kind: "manual", value: "jira:PAY-240", title: null })).toBe("PAY-240, applied by hand");
});

const row = (kind: LinkSetMemberKind, entity_id: string) => ({ kind, entity_id, title: null, line: null, state: null });

test("what is in a set is one line per kind that holds anything, in panel order, a pattern's matches counted once", () => {
  expect(
    contentLines({
      members: [row("monitor", "kuma:1"), row("repo", "gitea:a"), row("repo", "gitea:b"), row("pipeline", "tc:A")],
      patterns: [
        { kind: "pipeline", query: "name:*", count: 2, matches: [row("pipeline", "tc:A"), row("pipeline", "tc:B")] },
      ],
    }),
  ).toEqual(["2 repositories", "2 pipelines", "1 monitors"]);
});

test("the pattern dialog's fields are written in the search grammar the backend reads", () => {
  expect(patternQuery("pipeline", "Payouts", "Payout_*")).toBe("project:Payouts name:Payout_*");
  expect(patternQuery("pipeline", "", "")).toBe("name:*");
  expect(patternQuery("pipeline", "PSP client", "Psp*")).toBe('project:"PSP client" name:Psp*');
  expect(patternQuery("page", "OPS", "")).toBe("project:OPS");
  expect(patternQuery("page", "OPS", "On-call handbook")).toBe('project:OPS under:"On-call handbook"');
});

test("a row a link set brought is labelled with the set's name, on the job's sections and on Builds", () => {
  const origin = { kind: "link_set" as const, set: "payout-stack", name: "Payout stack" };
  expect(originLabel({ origin })).toBe("Payout stack");
  expect(originLabel({ origin: { kind: "parent", ticket: "jira:PAY-231" } })).toBe("Parent PAY-231");
  expect(originLabel({ origin: { kind: "own" } })).toBeNull();
  expect(reasonLabel({ kind: "linked", step: "teamcity:buildType:Payout_Build", origin }, "PAY-240")).toBe("Payout stack");
});

test("the hint names the first ticket, how many more, and the epic, as prototype-6 writes it (#870)", () => {
  const hint = {
    set_id: "payout-stack",
    set_name: "Payout stack",
    epic: "jira:PAY-200",
    member: { kind: "pipeline" as const, entity_id: "teamcity:Dashboard_Build" },
    title: "Dashboard_Build",
    tickets: ["jira:PAY-240", "jira:PAY-241", "jira:PAY-245"],
  };
  expect(hintTickets(hint)).toBe("PAY-240 and 2 more tickets under PAY-200");
  expect(hintTickets({ ...hint, tickets: ["jira:PAY-240", "jira:PAY-241"] })).toBe(
    "PAY-240 and 1 more ticket under PAY-200",
  );
});
