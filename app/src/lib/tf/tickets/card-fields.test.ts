/** Whether another lane's value is one a ticket's field takes (ticket-edit decisions, section 26), and what the menu offers. */
import { afterEach, beforeEach, expect, test } from "vitest";

import type { MiniBoardCard, TicketField, TicketFields } from "../../ipc/entity";
import { sourceRoles } from "../roles.svelte";
import { createCardFields, type CardFields } from "./card-fields.svelte";
import { askFields, forgetFields } from "./fields.svelte";

const CARD: MiniBoardCard = {
  entity_id: "jira:PAY-3",
  source_id: "jira",
  key: "PAY-3",
  title: "Settle fees",
  status: "To Do",
  priority: "High",
  issue_type: "Story",
  assignee: "mara",
  updated_at: "2026-11-20T10:00:00Z",
  path: null,
  parent_title: null,
  mine: false,
  labels: [],
  comment_count: null,
  signals: [],
};

const ASSIGNEE: TicketField = { id: "assignee", label: "Assignee", kind: { type: "search", scope: "PAY", person: true }, value: "mara", required: false };
const PRIORITY: TicketField = { id: "priority", label: "Priority", kind: { type: "choice", options: [{ value: "High", label: "High" }, { value: "Low", label: "Low" }] }, value: "High", required: true };

let edits = true;
function fields(answer: TicketFields | null): CardFields {
  const port = { ticketFields: () => Promise.resolve(answer), fieldOptions: () => Promise.resolve({ options: [], complete: true }) };
  void askFields(CARD, "menu", port);
  return createCardFields({
    cardOf: (id) => (id === CARD.entity_id ? CARD : null),
    sourceName: () => "Tidewater Jira",
    edits: () => edits,
    openPicker: () => {},
    fields: port,
    refused: () => Promise.resolve([]),
  });
}
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  edits = true;
  forgetFields();
});
afterEach(() => sourceRoles.forget());

test("another assignee lane: yes once its fields say so, ask until they land; nobody's lane takes it where the field may be empty", async () => {
  const card = fields({ fields: [ASSIGNEE, PRIORITY] });
  expect(card.lane(CARD, "assignee", "jonas")).toEqual({ kind: "ask" });
  await flush();
  expect(card.lane(CARD, "assignee", "jonas")).toEqual({ kind: "yes" });
  expect(card.lane(CARD, "assignee", "")).toEqual({ kind: "yes" });
});

test("a read-only assignee refuses another lane with the source's reason; a required one refuses Unassigned", async () => {
  const card = fields({ fields: [{ ...ASSIGNEE, readonly: "You may not assign tickets in Tidewater Jira (Assign Issues)." }] });
  await flush();
  expect(card.lane(CARD, "assignee", "jonas")).toEqual({ kind: "no", why: "You may not assign tickets in Tidewater Jira (Assign Issues)." });
  expect(card.offer(CARD.entity_id)).toEqual({ assign: { disabled: "You may not assign tickets in Tidewater Jira (Assign Issues)." }, priority: null });
  forgetFields();
  const required = fields({ fields: [{ ...ASSIGNEE, required: true }] });
  await flush();
  expect(required.lane(CARD, "assignee", "")).toEqual({ kind: "no", why: "PAY-3 must have an assignee." });
});

test("priority lanes: one of the source's priorities takes it, another name and No priority do not", async () => {
  const card = fields({ fields: [ASSIGNEE, PRIORITY] });
  await flush();
  expect(card.lane(CARD, "priority", "low")).toEqual({ kind: "yes" });
  expect(card.lane(CARD, "priority", "Blocker")).toEqual({ kind: "no", why: "Tidewater Jira has no priority Blocker." });
  expect(card.lane(CARD, "priority", "")).toEqual({ kind: "no", why: "knobas does not take a ticket's priority away. Drop it in a priority's lane." });
});

test("an epic, type or label lane takes no drop; a source with no edits or no such field says so", async () => {
  const card = fields({ fields: [ASSIGNEE] });
  await flush();
  expect(card.lane(CARD, "epic", "PAY-200")).toEqual({ kind: "no", why: "knobas does not change a ticket's epic. Drop it in its own lane." });
  expect(card.lane(CARD, "label", "sepa")).toEqual({ kind: "no", why: "knobas does not change a ticket's labels. Drop it in its own lane." });
  expect(card.lane(CARD, "priority", "Low")).toEqual({ kind: "no", why: "Tidewater Jira has no priority on PAY-3. Drop it in its own lane." });
  edits = false;
  expect(card.lane(CARD, "assignee", "jonas")).toEqual({ kind: "no", why: "Tidewater Jira's adapter offers no edits. Drop it in its own lane." });
  expect(card.offer(CARD.entity_id)).toBeNull();
});
