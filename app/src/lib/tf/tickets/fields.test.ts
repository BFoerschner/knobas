import { afterEach, expect, test, vi } from "vitest";

import type { FieldOptions, TicketField, TicketFields } from "../../ipc/entity";
import { askFields, askOptions, fieldOf, fieldsOf, forgetFields, optionsOf, type FieldsPort } from "./fields.svelte";

afterEach(() => forgetFields());

const ASSIGNEE: TicketField = {
  id: "assignee",
  label: "Assignee",
  kind: { type: "search", scope: "PAY", person: true },
  value: "mara",
  shown: "Mara Lindqvist",
  required: false,
};
const LABELS: TicketField = {
  id: "labels",
  label: "Labels",
  kind: { type: "labels", scope: null, create: true },
  value: [],
  required: false,
};
const FIELDS: TicketFields = { fields: [ASSIGNEE, LABELS] };

const PEOPLE: FieldOptions = {
  options: [
    { value: "mara", label: "Mara Lindqvist", hint: "mara" },
    { value: "ben", label: "Ben Okafor", hint: "ben" },
  ],
  complete: true,
};

/** A port recording what it was asked, answering from `fields` and `people`. */
function port(fields: TicketFields | null = FIELDS, people: (typed: string) => FieldOptions | null = () => PEOPLE) {
  const asked: string[] = [];
  const recorded: FieldsPort = {
    ticketFields: vi.fn(async (entityId: string, behind: boolean) => {
      asked.push(`fields ${entityId}${behind ? " behind" : ""}`);
      return fields;
    }),
    fieldOptions: vi.fn(async (entityId: string, field: string, typed: string) => {
      asked.push(`options ${entityId} ${field} ${typed}`);
      return people(typed);
    }),
  };
  return { port: recorded, asked };
}

const PAY_231 = { entity_id: "jira:PAY-231", updated_at: "2026-11-21T09:00:00Z" };
const PAY_232 = { entity_id: "jira:PAY-232", updated_at: "2026-11-21T09:00:00Z" };

test("a ticket's fields are asked once as it stands, and again once it changed", async () => {
  const { port: p, asked } = port();
  expect(await askFields(PAY_231, "open", p)).toEqual(FIELDS);
  await askFields(PAY_231, "open", p);
  expect(asked.filter((a) => a.startsWith("fields"))).toEqual(["fields jira:PAY-231"]);
  expect(fieldOf(PAY_231, "assignee")?.value).toBe("mara");
  await askFields({ ...PAY_231, updated_at: "2026-11-21T10:00:00Z" }, "open", p);
  expect(asked.filter((a) => a.startsWith("fields"))).toHaveLength(2);
});

test("a read asked ahead goes in the background lane; a person's does not", async () => {
  const { port: p, asked } = port();
  await askFields(PAY_231, "ahead", p);
  await askFields(PAY_232, "menu", p);
  expect(asked.filter((a) => a.startsWith("fields"))).toEqual(["fields jira:PAY-231 behind", "fields jira:PAY-232"]);
});

test("two asks at once send one read", async () => {
  const { port: p, asked } = port();
  await Promise.all([askFields(PAY_231, "open", p), askFields(PAY_231, "open", p)]);
  expect(asked.filter((a) => a.startsWith("fields"))).toHaveLength(1);
});

test("a failed read is kept as failed, and a landed write forgets the ticket", async () => {
  const failing: FieldsPort = {
    ticketFields: async () => {
      throw { code: "unreachable", message: "Jira did not answer", source_id: "jira" };
    },
    fieldOptions: async () => null,
  };
  expect(await askFields(PAY_231, "open", failing)).toBeNull();
  expect(fieldsOf(PAY_231)).toEqual({ state: "failed", message: expect.stringContaining("Jira did not answer") });
  forgetFields("jira:PAY-231");
  expect(fieldsOf(PAY_231)).toBeNull();
});

/** Mutant: the list kept by ticket, not by scope (the second picker asks). */
test("the second ticket of a scope opens its picker from memory, read ahead with the fields", async () => {
  const { port: p, asked } = port();
  await askFields(PAY_231, "open", p);
  await vi.waitFor(() => expect(optionsOf("jira:PAY-231", ASSIGNEE)?.options).toHaveLength(2));
  expect(asked).toContain("options jira:PAY-231 assignee ");
  expect(optionsOf("jira:PAY-231", LABELS), "labels with no scope have no list").toBeNull();
  const before = asked.length;
  const second = await askOptions("jira:PAY-232", ASSIGNEE, "", p);
  expect(second.options.map((o) => o.value)).toEqual(["mara", "ben"]);
  expect(asked.length, "nothing sent for the second ticket").toBe(before);
});

/** Mutant: the merged options sorted into the drawn ones (a drawn row's index changes). */
test("a typed search adds what the list lacked at its end, and moves nothing drawn", async () => {
  const { port: p } = port(FIELDS, (typed) =>
    typed === ""
      ? { options: PEOPLE.options, complete: false }
      : { options: [{ value: "aleks", label: "Aleksander Nowak", hint: "anowak" }, PEOPLE.options[0]!], complete: true },
  );
  const first = await askOptions("jira:PAY-231", ASSIGNEE, "", p);
  expect(first.complete).toBe(false);
  const after = await askOptions("jira:PAY-231", ASSIGNEE, "ak", p);
  expect(after.options.map((o) => o.value)).toEqual(["mara", "ben", "aleks"]);
  expect(after.complete, "a typed answer says nothing of the whole list").toBe(false);
});

test("a failed list keeps what it held and says why", async () => {
  const { port: ok } = port();
  await askOptions("jira:PAY-231", ASSIGNEE, "", ok);
  const failing: FieldsPort = {
    ticketFields: async () => null,
    fieldOptions: async () => {
      throw { code: "unreachable", message: "Jira did not answer", source_id: "jira" };
    },
  };
  const after = await askOptions("jira:PAY-231", ASSIGNEE, "zz", failing);
  expect(after.options).toHaveLength(2);
  expect(after.failed).toContain("Jira did not answer");
});

test("a source that offers no edits answers null, and nothing is read ahead", async () => {
  const { port: p, asked } = port(null);
  expect(await askFields(PAY_231, "open", p)).toBeNull();
  expect(fieldsOf(PAY_231)).toEqual({ state: "ready", fields: null });
  expect(asked).toEqual(["fields jira:PAY-231"]);
});

/** A ticket asked again once it changed draws its last answer meanwhile: an open control does not close. */
test("while a changed ticket is asked again, its last answer stands", async () => {
  let land: (fields: TicketFields) => void = () => {};
  const slow: FieldsPort = {
    ticketFields: () => new Promise((resolve) => (land = resolve)),
    fieldOptions: async () => null,
  };
  const { port: p } = port();
  await askFields(PAY_231, "open", p);
  const moved = { ...PAY_231, updated_at: "2026-11-21T10:00:00Z" };
  const asking = askFields(moved, "open", slow);
  expect(fieldsOf(moved)).toEqual({ state: "ready", fields: FIELDS });
  land({ fields: [ASSIGNEE] });
  await asking;
  expect(fieldsOf(moved)).toEqual({ state: "ready", fields: { fields: [ASSIGNEE] } });
});
