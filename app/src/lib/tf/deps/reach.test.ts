/**
 * The job's Map (#874, R-11.10): the top-bar read, kept to what the ticket
 * reaches through the job-reach read and the job's pipelines.
 */
import { expect, test } from "vitest";

import type { Dependencies, DepItem } from "../../ipc/assets";
import type { JobPipeline, Reached } from "../../ipc/entity";
import { reachedPart } from "./reach";

function item(id: string, kind: string, name = id): DepItem {
  return { id, kind, name, health: null, state: null, monitors: 0, environment: null, owner: null };
}

const READ: Dependencies = {
  layers: [{ id: "work", name: "Work", kinds: ["ticket", "pr"] }],
  kinds: [{ id: "ticket", label: "Ticket", monogram: "TK" }],
  items: [
    item("jira:PAY-231", "ticket", "PAY-231"),
    item("jira:PAY-228", "ticket", "PAY-228"),
    item("gitea:p#142", "pr"),
    item("gitea:p#150", "pr"),
    item("tc:buildType:Payout_Build", "pipeline"),
    item("tc:buildType:Ledger_Build", "pipeline"),
    item("asset:payouts", "service"),
    item("asset:ledger", "service"),
    item("asset:vm", "vm"),
  ],
  relations: [
    { from: "jira:PAY-231", to: "asset:payouts", relation: "deployed-to" },
    { from: "jira:PAY-228", to: "asset:ledger", relation: "deployed-to" },
    { from: "asset:payouts", to: "asset:vm", relation: "inside" },
    { from: "asset:ledger", to: "asset:vm", relation: "inside" },
    { from: "gitea:p#150", to: "asset:ledger", relation: "related" },
  ],
};

const reached = (entity_id: string, kind: string): Reached => ({ entity_id, kind, step: entity_id, origin: { kind: "own" } });

const REACH: Reached[] = [
  reached("gitea:p#142", "pr"),
  reached("tc:build:Payout_Build-1188", "build"),
  reached("asset:payouts", "asset"),
  reached("asset:vm", "asset"),
  reached("uk:payouts-health", "monitor"),
];

const PIPELINES: JobPipeline[] = [{ pipeline_id: "tc:buildType:Payout_Build", reasons: [{ kind: "pr_check", pr: "gitea:p#142" }] }];

test("keeps the ticket, what it reaches and its pipelines, and nothing else", () => {
  const part = reachedPart(READ, { id: "jira:PAY-231", key: "PAY-231" }, REACH, PIPELINES);
  expect(part.items.map((i) => i.id)).toEqual([
    "jira:PAY-231",
    "gitea:p#142",
    "tc:buildType:Payout_Build",
    "asset:payouts",
    "asset:vm",
  ]);
  expect(part.layers).toBe(READ.layers);
  expect(part.kinds).toBe(READ.kinds);
});

test("keeps only the relations whose two ends are kept", () => {
  const part = reachedPart(READ, { id: "jira:PAY-231", key: "PAY-231" }, REACH, PIPELINES);
  expect(part.relations).toEqual([
    { from: "jira:PAY-231", to: "asset:payouts", relation: "deployed-to" },
    { from: "asset:payouts", to: "asset:vm", relation: "inside" },
  ]);
});

test("a ticket the read does not hold is drawn anyway, by its key", () => {
  const part = reachedPart(READ, { id: "jira:PAY-240", key: "PAY-240" }, [], []);
  expect(part.items).toEqual([item("jira:PAY-240", "ticket", "PAY-240")]);
  expect(part.relations).toEqual([]);
});
