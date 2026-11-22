/**
 * A ticket's fields in the fixture (ADR-0054): `ticket_fields` and
 * `field_options` for the fixture's Jira-like tickets, and `EditTicket`
 * changing the ticket the fixture serves, so the ticket page's controls can
 * be walked, pressed and screenshotted without a source.
 *
 * The real mock adapter offers no edits (it answers `ticket_fields` with
 * nothing); the fixture answers like Jira does, the way it answers
 * `reachable_transitions`. Switches:
 *
 * - `?fake-noedits`: every ticket answers `null`, an adapter built before
 *   ticket fields: the facts as text and the one line under them.
 * - `?fake-readonly`: the credential lacks *Edit Issues* and *Assign
 *   Issues*: `refused_writes` refuses each edit ask with Jira's reason.
 * - `?fake-refuse-edits`: every edit is refused by the source with Jira's
 *   sentence, so the note under its field can be drawn.
 *
 * Each ticket also answers six fields of its own (step 10 of the ruling), one
 * of each kind but labels, as a Jira screen with custom fields would: a due
 * date, story points, a customer reference, an environment, a reviewer and
 * whether it needs a release note. Their values are kept on the ticket's
 * payload under `own`, so an edit changes what the next read answers.
 */
import type { QueuedWrite, WriteOpPayload } from "../../../ipc/sources";
import type { FieldOptions, TicketFields } from "../../../ipc/entity";
import { CORPUS, fakePersonName } from "./corpus";
import { keyOf } from "../../../time/format";
import { FIXTURE_QUEUE } from "./write-queue";
import { onFakeReset } from "../fake-seed";

type Handler = (args: Record<string, unknown>) => unknown;

let noEdits = false;
let readOnly = false;
let refuseEdits = false;
onFakeReset(() => {
  noEdits = false;
  readOnly = false;
  refuseEdits = false;
});

/** The accounts a fixture ticket can be assigned to, `(account, name)`, as Jira's assignable search answers them. */
const ASSIGNABLE: [string, string][] = [
  ["mara", "Mara Lindqvist"],
  ["jonas", "Jonas Becker"],
  ["priya", "Priya Nair"],
  ["tomasz", "Tomasz Wójcik"],
  ["lena", "Lena Hoffmann"],
  ["aleks", "Aleksander Nowak"],
  ["jbertrand", "Julie Bertrand"],
];

const PRIORITIES = ["Highest", "High", "Medium", "Low", "Lowest"];

/** A fixture ticket's own fields as a Jira screen lists them, with what they hold before any edit. */
const OWN: { id: string; label: string; kind: TicketFields["fields"][number]["kind"]; start: unknown }[] = [
  { id: "duedate", label: "Due date", kind: { type: "date" }, start: "2026-12-04" },
  { id: "customfield_10106", label: "Story Points", kind: { type: "number" }, start: 5 },
  { id: "customfield_10100", label: "Customer ref", kind: { type: "text" }, start: "ACME-7" },
  {
    id: "customfield_10200",
    label: "Environment",
    kind: { type: "choice", options: ["Staging", "Production"].map((value) => ({ value, label: value })) },
    start: "Staging",
  },
  { id: "customfield_10300", label: "Reviewer", kind: { type: "search", scope: "people", person: true }, start: null },
  { id: "customfield_10400", label: "Release note", kind: { type: "flag" }, start: false },
];

/** The ticket's own fields' values, kept on its payload. */
function ownOf(payload: Record<string, unknown>): Record<string, unknown> {
  const held = payload["own"];
  if (typeof held === "object" && held !== null) return held as Record<string, unknown>;
  const fresh = Object.fromEntries(OWN.map((field) => [field.id, field.start]));
  payload["own"] = fresh;
  return fresh;
}

function ticketOf(entityId: string) {
  return CORPUS.find((entry) => entry.entity_id === entityId && entry.kind === "ticket");
}

function projectOf(entityId: string): string {
  return keyOf(entityId).replace(/-\d+$/, "");
}

function str(value: unknown): string | null {
  return typeof value === "string" && value.trim() !== "" ? value : null;
}

/** `ticket_fields`: a fixture ticket's summary, assignee, priority and labels, as Jira's `editmeta` reads. */
export function fakeTicketFields(entityId: string): TicketFields | null {
  const entry = ticketOf(entityId);
  if (entry === undefined || noEdits) return null;
  const assignee = str(entry.payload["assignee"]);
  const priority = str(entry.payload["priority"]);
  const labels = Array.isArray(entry.payload["labels"]) ? (entry.payload["labels"] as string[]) : [];
  return {
    fields: [
      { id: "title", label: "Summary", kind: { type: "text" }, value: entry.title, required: true },
      {
        id: "assignee",
        label: "Assignee",
        kind: { type: "search", scope: projectOf(entityId), person: true },
        value: assignee,
        ...(assignee !== null ? { shown: fakePersonName(assignee) ?? assignee } : {}),
        required: false,
      },
      {
        id: "priority",
        label: "Priority",
        kind: { type: "choice", options: PRIORITIES.map((name) => ({ value: name, label: name })) },
        value: priority,
        required: false,
      },
      { id: "labels", label: "Labels", kind: { type: "labels", scope: null, create: true }, value: labels, required: false },
      ...OWN.map((field) => {
        const value = ownOf(entry.payload)[field.id] ?? null;
        const shown = field.kind.type === "search" && typeof value === "string" ? { shown: fakePersonName(value) ?? ASSIGNABLE.find(([a]) => a === value)?.[1] ?? value } : {};
        return { id: field.id, label: field.label, kind: field.kind, value, ...shown, required: false };
      }),
    ],
  };
}

/** `field_options`: the people of the fixture, those whose name or account holds `typed`. */
export function fakeFieldOptions(entityId: string, field: string, typed: string): FieldOptions | null {
  if (ticketOf(entityId) === undefined || (field !== "assignee" && field !== "customfield_10300")) return null;
  const words = typed.trim().toLowerCase();
  const options = ASSIGNABLE.filter(([account, name]) => words === "" || account.includes(words) || name.toLowerCase().includes(words)).map(
    ([account, name]) => ({ value: account, label: name, hint: account }),
  );
  return { options, complete: true };
}

/** The edit's change made to the fixture's ticket, so the page's next read shows it. */
function apply(op: Record<string, unknown>): void {
  const entry = ticketOf(String(op["entity"] ?? ""));
  if (entry === undefined) return;
  if (typeof op["title"] === "string") entry.title = op["title"];
  const assignee = op["assignee"];
  if (assignee === "nobody") entry.payload["assignee"] = null;
  else if (typeof assignee === "object" && assignee !== null) entry.payload["assignee"] = (assignee as { account: string }).account;
  if (typeof entry.payload["assignee"] !== "undefined") entry.author = str(entry.payload["assignee"]);
  if (typeof op["priority"] === "string") entry.payload["priority"] = op["priority"];
  const change = op["labels"] as { add?: string[]; remove?: string[] } | null | undefined;
  if (change) {
    const now = Array.isArray(entry.payload["labels"]) ? [...(entry.payload["labels"] as string[])] : [];
    for (const name of change.add ?? []) if (!now.includes(name)) now.push(name);
    entry.payload["labels"] = now.filter((name) => !(change.remove ?? []).includes(name));
  }
  const own = op["fields"];
  if (typeof own === "object" && own !== null) Object.assign(ownOf(entry.payload), own);
  entry.updated_at = new Date().toISOString();
}

/**
 * An `EditTicket` the fixture's `submit_write` took: applied to the ticket, or under `?fake-refuse-edits` refused with
 * Jira's sentence and kept open in the queue. Answers the row's state and detail.
 */
export function fakeEdit(payload: WriteOpPayload): Pick<QueuedWrite, "state" | "detail"> {
  const op = (payload as { EditTicket?: Record<string, unknown> }).EditTicket;
  if (op === undefined) return { state: "sent", detail: null };
  if (refuseEdits) return { state: "refused", detail: "Jira: Field 'labels' cannot be set. It is not on the appropriate screen, or unknown." };
  apply(op);
  return { state: "sent", detail: null };
}

/** A refused edit's row, kept open in the queue the notes are read from. */
export function keepRefused(row: QueuedWrite): void {
  if (row.state === "refused") (FIXTURE_QUEUE as QueuedWrite[]).push(row);
}

/** `refused_writes`' answers for the edit asks under `?fake-readonly`, Jira's reasons; none otherwise. */
export function fakeEditRefusals(asks: readonly { entity: string; op: string; field?: string }[]): { entity: string; op: string; field?: string; reason: string }[] {
  if (!readOnly) return [];
  return asks
    .filter((ask) => ask.op === "edit_ticket" || ask.op === "transition")
    .map((ask) => {
      const project = projectOf(ask.entity);
      const reason =
        ask.op === "transition"
          ? `This Jira user cannot move tickets in ${project}: it lacks the Transition Issues permission`
          : ask.field === "assignee"
            ? `This Jira user cannot assign tickets in ${project}: it lacks the Assign Issues permission`
            : `This Jira user cannot edit tickets in ${project}: it lacks the Edit Issues permission`;
      return { ...ask, reason };
    });
}

export function seed(params: URLSearchParams): void {
  noEdits = params.has("fake-noedits");
  readOnly = params.has("fake-readonly");
  refuseEdits = params.has("fake-refuse-edits");
}

export function handlers(): Record<string, Handler> {
  return {
    ticket_fields: (args) => fakeTicketFields(String(args["entityId"] ?? "")),
    field_options: (args) => fakeFieldOptions(String(args["entityId"] ?? ""), String(args["field"] ?? ""), String(args["typed"] ?? "")),
  };
}
