/**
 * The ticket page's fields as controls (ADR-0054, the 2026-11-21 ruling's sections 25 and 27): each control queues the
 * `EditTicket` it should and nothing else in it, draws at once with its chip, leaves a note when refused, and stays
 * text where the source or the credential says no; an adapter that offers no edits draws its facts as text and one
 * line; `a` and `e` open the assignee and the summary; the facts do not move when the fields land late.
 */
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

// The pickers rank through the app's ranking (#2535): here a letters-in-order stand-in.
vi.mock("../../ipc/search", async (actual) => ({ ...(await actual<object>()), ...(await import("../filter/test-fuzzy")).fuzzyIpc() }));
import { ranked } from "../filter/test-fuzzy";

import type { EntityDetail, TicketFields } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { toasts } from "../../shell/toasts.svelte";
import { forgetEchoes } from "../queued.svelte";
import { createLayers } from "../layers.svelte";
import { forgetFields } from "../tickets/fields.svelte";
import type { FieldPorts } from "./field-edits.svelte";
import type { JobPorts } from "./ports";
import { chosenJob } from "./chosen.svelte";
import TfJob from "./TfJob.svelte";
import { sourceFacts } from "./source-facts.fixture";

const ID = "jira:PAY-231";
let target: HTMLElement;
let job: ReturnType<typeof mount> | null = null;
let writes: WriteOpPayload[] = [];

beforeEach(() => {
  writes = [];
  toasts.items.length = 0;
  chosenJob.choose(ID);
  target = document.createElement("div");
  document.body.appendChild(target);
});

afterEach(() => {
  forgetEchoes();
  forgetFields();
  if (job) unmount(job);
  job = null;
  target.remove();
});

async function settle() {
  for (let turn = 0; turn < 10; turn += 1) await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

function ticket(): EntityDetail {
  return {
    row: { entity_id: ID, kind: "ticket", source_id: "jira", title: "Retry failed SEPA payouts", updated_at: "2026-10-15T09:00:00Z", synced_at: "2026-10-15T09:00:00Z", path: null, state: null },
    source: { id: "jira", display_name: "Jira", adapter_kind: "jira", enabled: true },
    kind_info: null,
    refreshes: true,
    body_text: "",
    author: "ben",
    author_name: "Ben Okafor",
    payload: {},
    web_url: null,
    deleted_at: null,
    links: [],
    activity: [],
    builds: [],
    status: "In Progress",
    issue_type: "Story",
    priority: "High",
    estimate_seconds: null,
    labels: ["sepa"],
    description: null,
    markup: "text",
  } as EntityDetail;
}

const FIELDS: TicketFields = {
  fields: [
    { id: "title", label: "Summary", kind: { type: "text" }, value: "Retry failed SEPA payouts", required: true },
    { id: "assignee", label: "Assignee", kind: { type: "search", scope: "PAY", person: true }, value: "ben", shown: "Ben Okafor", required: false },
    {
      id: "priority",
      label: "Priority",
      kind: { type: "choice", options: ["Highest", "High", "Low"].map((p) => ({ value: p, label: p })) },
      value: "High",
      required: false,
    },
    { id: "labels", label: "Labels", kind: { type: "labels", scope: null, create: true }, value: ["sepa"], required: false },
  ],
};

function fieldPorts(over: Partial<FieldPorts> & { answer?: TicketFields | null; late?: Promise<void>; queue?: QueuedWrite[] } = {}): FieldPorts {
  return {
    ticketFields: async () => {
      await over.late;
      return over.answer === undefined ? FIELDS : over.answer;
    },
    fieldOptions: async () => ({
      options: [
        { value: "mara", label: "Mara Lindqvist", hint: "mara" },
        { value: "ben", label: "Ben Okafor", hint: "ben" },
      ],
      complete: true,
    }),
    pendingWrites: async () => over.queue ?? [],
    discardWrite: async () => {},
    ...over,
  };
}

function ports(fields: FieldPorts, over: Partial<JobPorts> = {}): JobPorts {
  return {
    listSources: () => Promise.resolve([sourceFacts("jira")]),
    getEntity: (id) => (id === ID ? Promise.resolve(ticket()) : Promise.reject({ code: "not_found", message: id, source_id: null })),
    promoteContext: (id) => Promise.resolve({ id: "ctx:1", kind: "ticket", title: "t", anchor_id: id, created_at: "", archived_at: null }),
    listEntities: () => Promise.resolve({ rows: [], total: 0, tally: null, groups: null }),
    submitWrite: (payload) => {
      writes.push(payload);
      return Promise.resolve({ id: writes.length, state: "pending" } as QueuedWrite);
    },
    visitEntity: () => Promise.resolve(null),
    entityThread: () => Promise.resolve({ comments: [], me: ["mara"] }),
    createLink: () => Promise.resolve({} as never),
    unlink: () => Promise.resolve(),
    search: () => Promise.reject(new Error("not in this test")),
    launcherHome: () => Promise.reject(new Error("not in this test")),
    jobReach: () => Promise.resolve([]),
    jobLinkSets: () => Promise.resolve({ sets: [], parent: null }),
    hideLinkSetMember: (_t, _m, hidden) => Promise.resolve(hidden),
    getAsset: (id) => Promise.reject({ code: "not_found", message: id, source_id: null }),
    startWork: { flow: () => Promise.resolve([]), run: () => Promise.resolve([]), retry: () => Promise.resolve([]), skip: () => Promise.resolve([]), amend: () => Promise.resolve([]) },
    refusedWrites: () => Promise.resolve([]),
    listen: () => Promise.resolve(() => {}),
    fields,
    ...over,
  };
}

function open(p: JobPorts) {
  job = mount(TfJob, {
    target,
    props: { segment: "PAY-231", timer: { current: null, elapsed: null }, layers: createLayers(), ontimer: () => {}, onstarttimer: () => {}, ports: p },
  });
  flushSync();
}

const fact = (name: string) => {
  const term = [...target.querySelectorAll("dl.tf-facts > dt")].find((dt) => dt.textContent === name);
  return term?.nextElementSibling as HTMLElement | undefined;
};
const control = (name: string) => fact(name)?.querySelector<HTMLElement>(".tf-fact-edit") ?? null;
const pick = (selector: string) => document.querySelector<HTMLElement>(selector);

/** The one payload queued, as `EditTicket`. */
function edited(): Record<string, unknown> {
  expect(writes).toHaveLength(1);
  return (writes[0] as { EditTicket: Record<string, unknown> }).EditTicket;
}

/** Mutant: an assignee edit that also sends the title. */
test("the assignee is a button; a pick queues an EditTicket of the assignee alone, drawn at once with its chip", async () => {
  open(ports(fieldPorts()));
  await settle();
  const button = control("Assignee")!;
  expect(button.tagName).toBe("BUTTON");
  button.click();
  await settle();
  expect(pick(".tf-fpick")).not.toBeNull();
  const mara = [...document.querySelectorAll<HTMLElement>(".tf-fpick-opt")].find((row) => row.textContent?.includes("Mara Lindqvist"))!;
  mara.click();
  await settle();
  expect(edited()).toEqual({ entity: ID, title: null, assignee: { account: "mara" }, priority: null, labels: null, fields: {} });
  expect(fact("Assignee")?.textContent).toContain("Mara Lindqvist");
  expect(fact("Assignee")?.querySelector(".tf-fwrite")?.textContent).toBe("Assigning, queued");
});

test("a priority is a menu of the source's priorities, the current one ticked; a pick sets it", async () => {
  open(ports(fieldPorts()));
  await settle();
  control("Priority")!.click();
  await settle();
  const items = [...document.querySelectorAll<HTMLElement>("[role=menuitemradio]")];
  expect(items.map((item) => item.textContent?.replace("✓", ""))).toEqual(["Highest", "High", "Low"]);
  expect(items[1]!.getAttribute("aria-checked")).toBe("true");
  items[0]!.click();
  await settle();
  expect(edited()).toEqual({ entity: ID, title: null, assignee: null, priority: "Highest", labels: null, fields: {} });
  expect(fact("Priority")?.textContent).toContain("Highest");
});

test("each label ticked is one write of the name added or removed, and the picker stays open", async () => {
  open(ports(fieldPorts()));
  await settle();
  control("Labels")!.click();
  await settle();
  const field = document.querySelector<HTMLInputElement>(".tf-fpick input")!;
  field.value = "psp";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  const add = [...document.querySelectorAll<HTMLElement>(".tf-fpick-opt")].find((row) => row.textContent?.includes("Add “psp”"))!;
  add.click();
  await settle();
  const sepa = [...document.querySelectorAll<HTMLElement>(".tf-fpick-opt")].find((row) => row.textContent?.includes("sepa"))!;
  sepa.click();
  await settle();
  expect(writes.map((w) => (w as { EditTicket: { labels: unknown } }).EditTicket.labels)).toEqual([
    { add: ["psp"], remove: [] },
    { add: [], remove: ["sepa"] },
  ]);
  expect(pick(".tf-fpick"), "the picker stays open").not.toBeNull();
  expect([...fact("Labels")!.querySelectorAll(".tf-lab")].map((l) => l.textContent)).toEqual(["psp"]);
});

test("the labels picker has the app rank the names, keeps letters in order and marks them (#2535)", async () => {
  open(ports(fieldPorts()));
  await settle();
  control("Labels")!.click();
  await settle();
  const field = document.querySelector<HTMLInputElement>(".tf-fpick input")!;
  // No substring holds "spa"; its letters stand in order in "sepa", the label the ticket carries.
  field.value = "spa";
  field.dispatchEvent(new Event("input", { bubbles: true }));
  await ranked();
  const rows = [...document.querySelectorAll<HTMLElement>(".tf-fpick-opt .tf-fpick-name")];
  expect(rows.map((row) => row.textContent)).toEqual(["sepa", "Add “spa”"]);
  expect([...rows[0]!.querySelectorAll("mark")].map((m) => m.textContent).join("")).toBe("spa");
});

test("a read-only field has no chevron and its reason as its title; a refused credential names the permission", async () => {
  const readOnly: TicketFields = {
    fields: FIELDS.fields.map((f) => (f.id === "priority" ? { ...f, readonly: "Not on this ticket's edit screen in Jira." } : f)),
  };
  const reason = "This Jira user cannot assign tickets in PAY: it lacks the Assign Issues permission";
  open(
    ports(fieldPorts({ answer: readOnly }), {
      refusedWrites: (asks) => Promise.resolve(asks.filter((ask) => ask.field === "assignee").map((ask) => ({ ...ask, reason }))),
    }),
  );
  await settle();
  const priority = control("Priority")!;
  expect(priority.tagName).toBe("SPAN");
  expect(priority.getAttribute("aria-disabled")).toBe("true");
  expect(priority.title).toBe("Not on this ticket's edit screen in Jira.");
  const assignee = control("Assignee")!;
  expect(assignee.tagName).toBe("SPAN");
  expect(assignee.title).toBe(reason);
  priority.click();
  assignee.click();
  await settle();
  expect(writes).toEqual([]);
});

test("an adapter that offers no edits draws its facts as text and one line that says so", async () => {
  open(ports(fieldPorts({ answer: null })));
  await settle();
  await new Promise((resolve) => setTimeout(resolve, 20));
  flushSync();
  expect(control("Assignee")?.tagName).toBe("SPAN");
  expect(target.querySelector(".tf-tknoedits")?.textContent).toBe("Jira's adapter offers no edits here. Open the ticket in Jira to change these.");
});

test("a refused edit leaves a note under its field, in the source's words, with × that discards it", async () => {
  const discarded: number[] = [];
  const refused = {
    id: 7,
    source_id: "jira",
    entity_id: ID,
    op: "edit_ticket",
    payload: { EditTicket: { entity: ID, title: null, assignee: null, priority: null, labels: { add: ["bank"], remove: [] }, fields: {} } },
    state: "refused",
    detail: "Jira: Field 'labels' cannot be set. It is not on the appropriate screen, or unknown.",
    held_snapshot: null,
  } as unknown as QueuedWrite;
  open(ports(fieldPorts({ queue: [refused], discardWrite: async (id) => void discarded.push(id) })));
  await settle();
  const note = target.querySelector<HTMLElement>(".tf-fact-note")!;
  expect(note.textContent).toContain("Not labelled bank.");
  expect(note.textContent).toContain("Field 'labels' cannot be set.");
  note.querySelector<HTMLElement>(".tf-fact-x")!.click();
  await settle();
  expect(discarded).toEqual([7]);
});

test("`a` opens the assignee's picker and `e` turns the summary into a field that saves a retitle", async () => {
  open(ports(fieldPorts()));
  await settle();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "a", bubbles: true }));
  await settle();
  expect(pick(".tf-fpick")?.getAttribute("aria-label")).toBe("Assignee");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  document.querySelector<HTMLInputElement>(".tf-fpick input")?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await settle();
  (document.activeElement as HTMLElement | null)?.blur();
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", bubbles: true }));
  await settle();
  const input = target.querySelector<HTMLInputElement>(".tf-tktitleedit input")!;
  expect(input.value).toBe("Retry failed SEPA payouts");
  input.value = "Retry failed SEPA payouts three times";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  target.querySelector<HTMLFormElement>(".tf-tktitleedit")!.requestSubmit();
  await settle();
  expect(edited()).toEqual({ entity: ID, title: "Retry failed SEPA payouts three times", assignee: null, priority: null, labels: null, fields: {} });
  expect(target.querySelector(".tf-tkread h1")?.textContent).toContain("Retry failed SEPA payouts three times");
});

test("the facts' boxes do not move when the fields land late", async () => {
  let land: () => void = () => {};
  const late = new Promise<void>((resolve) => (land = resolve));
  open(ports(fieldPorts({ late })));
  await settle();
  const before = control("Assignee")!.getBoundingClientRect();
  const rowsBefore = [...target.querySelectorAll("dl.tf-facts > dt")].map((dt) => dt.textContent);
  land();
  await settle();
  const after = control("Assignee")!.getBoundingClientRect();
  expect([after.left, after.top, after.width]).toEqual([before.left, before.top, before.width]);
  expect([...target.querySelectorAll("dl.tf-facts > dt")].map((dt) => dt.textContent).slice(0, rowsBefore.length)).toEqual(rowsBefore);
});

// ---- a source's own fields (ADR-0054, step 10) ----

const OWN: TicketFields = {
  fields: [
    ...FIELDS.fields,
    { id: "duedate", label: "Due date", kind: { type: "date" }, value: "2026-12-04", required: false },
    { id: "customfield_10106", label: "Story Points", kind: { type: "number" }, value: 5, required: false },
    { id: "customfield_10200", label: "Environment", kind: { type: "choice", options: ["Staging", "Production"].map((value) => ({ value, label: value })) }, value: "Staging", required: false },
    { id: "customfield_10300", label: "Reviewer", kind: { type: "search", scope: "PAY", person: true }, value: null, required: false },
    { id: "customfield_10400", label: "Release note", kind: { type: "flag" }, value: false, required: false },
    { id: "customfield_10500", label: "Customer ref", kind: { type: "text" }, value: "ACME-7", required: false, readonly: "Not on this ticket's edit screen in Jira." },
  ],
};

const more = () => target.querySelector<HTMLDetailsElement>("details.tf-tkmore");
const ownFact = (name: string) => {
  const term = [...(more()?.querySelectorAll("dl.tf-facts > dt") ?? [])].find((dt) => dt.textContent === name);
  return term?.nextElementSibling as HTMLElement | undefined;
};

test("a source's own fields stand in a closed fold under the facts with their count; the four named fields are not in it", async () => {
  open(ports(fieldPorts({ answer: OWN })));
  await settle();
  const fold = more()!;
  expect(fold.open).toBe(false);
  expect(fold.querySelector("summary")!.textContent).toBe("More fields 6");
  const names = [...fold.querySelectorAll("dl.tf-facts > dt")].map((dt) => dt.textContent);
  expect(names).toEqual(["Due date", "Story Points", "Environment", "Reviewer", "Release note", "Customer ref"]);
  // Read only: text with the reason as its title.
  const ref = ownFact("Customer ref")!.querySelector<HTMLElement>(".tf-fact-edit")!;
  expect(ref.getAttribute("aria-disabled")).toBe("true");
  expect(ref.title).toBe("Not on this ticket's edit screen in Jira.");
  open(ports(fieldPorts()));
});

test("no own fields, no fold", async () => {
  open(ports(fieldPorts()));
  await settle();
  expect(more()).toBeNull();
});

/** Mutant: an own field's edit that also sends a named field. */
test("a date, a number, a choice, a person and a flag each queue EditTicket with that field alone, drawn at once", async () => {
  open(ports(fieldPorts({ answer: OWN })));
  await settle();
  more()!.open = true;
  flushSync();
  const only = (fields: Record<string, unknown>) => ({ entity: ID, title: null, assignee: null, priority: null, labels: null, fields });

  const date = ownFact("Due date")!.querySelector<HTMLInputElement>("input[type=date]")!;
  date.value = "2026-12-24";
  date.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
  expect(edited()).toEqual(only({ duedate: "2026-12-24" }));
  expect(ownFact("Due date")!.querySelector(".tf-fwrite")?.textContent).toBe("Setting, queued");
  writes.length = 0;

  ownFact("Story Points")!.querySelector<HTMLElement>(".tf-fact-edit")!.click();
  await settle();
  const number = ownFact("Story Points")!.querySelector<HTMLInputElement>("input[type=number]")!;
  number.value = "8";
  number.dispatchEvent(new Event("input", { bubbles: true }));
  number.form!.requestSubmit();
  await settle();
  expect(edited()).toEqual(only({ customfield_10106: 8 }));
  expect(ownFact("Story Points")!.textContent).toContain("8");
  writes.length = 0;

  ownFact("Environment")!.querySelector<HTMLElement>(".tf-fact-edit")!.click();
  await settle();
  const items = [...document.querySelectorAll<HTMLElement>("[role=menuitemradio]")];
  expect(items.map((item) => item.getAttribute("aria-checked"))).toEqual(["true", "false"]);
  items[1]!.click();
  await settle();
  expect(edited()).toEqual(only({ customfield_10200: "Production" }));
  writes.length = 0;

  ownFact("Reviewer")!.querySelector<HTMLElement>(".tf-fact-edit")!.click();
  await settle();
  [...document.querySelectorAll<HTMLElement>(".tf-fpick-opt")].find((row) => row.textContent?.includes("Mara Lindqvist"))!.click();
  await settle();
  expect(edited()).toEqual(only({ customfield_10300: "mara" }));
  expect(ownFact("Reviewer")!.textContent).toContain("Mara Lindqvist");
  writes.length = 0;

  const flag = ownFact("Release note")!.querySelector<HTMLInputElement>("input[type=checkbox]")!;
  flag.click();
  await settle();
  expect(edited()).toEqual(only({ customfield_10400: true }));
  expect(ownFact("Release note")!.textContent).toContain("Yes");
});
