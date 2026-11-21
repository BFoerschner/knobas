/**
 * A ticket's field edit (ADR-0054, the 2026-11-21 ruling's sections 16, 24
 * and 25): one `EditTicket` write carrying only what changed, drawn at once
 * as an echo, and the note a refused or held edit leaves under its field.
 *
 * - **`editTicket`** is what every surface calls (the ticket page's controls,
 *   a card's *Assign to…*): the echo first, synchronously, so the caller's
 *   next frame draws the new value; then `submit_write`; then the echo again
 *   with the write's id, which starts the follow; then the toast. A
 *   rejection of `submit_write` itself drops the echo and toasts why.
 * - **Echoes** are kind `field:<id>` on the ticket, their text the value as
 *   it reads (*Mara Lindqvist*, *High*, `+sepa`). The page settles each when
 *   its read shows the value.
 * - **Notes** come from the write queue, not from the echo: a ticket's field
 *   has a note while the queue holds a refused or held (not unconfirmed)
 *   `EditTicket` that sets it, so a note survives a restart and goes when
 *   that row is discarded, applied anyway or sent.
 */
import type { Assignee, LabelChange, QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { ipcErrorMessage } from "../../ipc";
import { mayHaveLanded, writeFollower, type WriteFollower } from "../../shell/follow-write";
import { toastQueued, type WriteSource } from "../../shell/queued-toast";
import { push } from "../../shell/toasts.svelte";
import { echoWrite, settleEchoes } from "../queued.svelte";

/** What an edit changes: the fields of `EditTicket` it sets, each `undefined` left alone. */
export interface TicketChange {
  title?: string;
  assignee?: Assignee;
  priority?: string;
  labels?: LabelChange;
  /** A source's own fields by the id `ticket_fields` named. */
  fields?: Record<string, unknown>;
}

/** The ticket an edit is made on. */
export interface EditedTicket {
  entity_id: string;
  /** Its key as a person reads it (*PAY-231*). */
  key: string;
  source: WriteSource;
}

/** How the edit is drawn and said. */
export interface EditWords {
  /** The field id the echo is drawn on (`assignee`, `labels`, `due`). */
  field: string;
  /** The value as it reads once set (*Mara Lindqvist*; *Unassigned*; for labels `+sepa` or `-triage`). */
  text: string;
  /** What the field read as when the edit was made. */
  base: string | null;
  /** What the toast and a refusal call it: *Assign PAY-231 to Mara Lindqvist*. */
  name: string;
}

export interface EditPorts {
  submitWrite(payload: WriteOpPayload): Promise<QueuedWrite>;
  follower?: WriteFollower;
}

/** The surface the ticket page's echoes are tagged with, which re-reads it (`watchEchoes`). */
export const JOB_SURFACE = "job";

/** The echo kind of a field. */
export function fieldKind(field: string): string {
  return `field:${field}`;
}

/** `change` as the `EditTicket` payload: every named field spelt, `null` where it is not set. */
export function editPayload(entity: string, change: TicketChange): WriteOpPayload {
  return {
    EditTicket: {
      entity,
      title: change.title ?? null,
      assignee: change.assignee ?? null,
      priority: change.priority ?? null,
      labels: change.labels ?? null,
      fields: change.fields ?? {},
    },
  } as WriteOpPayload;
}

/**
 * Edit `ticket`: drawn at once as an echo, written behind. Answers the queued
 * write, or `null` when it could not be queued.
 */
export async function editTicket(
  ticket: EditedTicket,
  change: TicketChange,
  words: EditWords,
  ports: EditPorts,
  surface = JOB_SURFACE,
): Promise<QueuedWrite | null> {
  const id = ticket.entity_id;
  const kind = fieldKind(words.field);
  const many = words.field === "labels";
  echoWrite(id, kind, words.text, { base: words.base, surface, source: ticket.source.id, many });
  let write: QueuedWrite;
  try {
    write = await ports.submitWrite(editPayload(id, change));
  } catch (cause) {
    settleEchoes(id, (echo) => echo.kind === kind && echo.write === null && echo.text === words.text);
    push({ text: `Could not queue the edit: ${ipcErrorMessage(cause)}`, tone: "err" });
    return null;
  }
  // The first echo, without a write, gives way to the followed one.
  settleEchoes(id, (echo) => echo.kind === kind && echo.write === null && echo.text === words.text);
  echoWrite(id, kind, words.text, {
    write,
    base: words.base,
    surface,
    source: ticket.source.id,
    many,
    following: { name: words.name },
    follower: ports.follower ?? writeFollower(),
  });
  toastQueued(words.name.charAt(0).toLowerCase() + words.name.slice(1), ticket.source, { write, subject: `edit:${id}:${words.field}` });
  return write;
}

/** The chip a field's echo draws after its value, in the fixed words. */
export function chipWords(field: string, text: string, state: "queued" | "sent"): string {
  const sent = state === "sent";
  if (field === "labels") {
    const name = text.slice(1);
    const adding = text.startsWith("+");
    if (sent) return `${adding ? "Added" : "Removed"} ${name}, sent`;
    return `${adding ? "Adding" : "Removing"} ${name}, queued`;
  }
  if (field === "assignee") return sent ? "Assigned, sent" : "Assigning, queued";
  if (field === "title") return sent ? "Renamed, sent" : "Renaming, queued";
  return sent ? "Set, sent" : "Setting, queued";
}

/** What a refused or held edit says under its field. */
export interface FieldNote {
  /** The queued write, which `×` discards. */
  write: number;
  tone: "err" | "warn";
  /** knobas' words: *Not assigned to mara.* */
  lead: string;
  /** The source's sentence, or why knobas holds it. */
  words: string;
}

/** The fields an `EditTicket` payload sets, with the lead a refusal of each says. */
function setFields(payload: WriteOpPayload): { field: string; lead: string }[] {
  const op = (payload as { EditTicket?: Record<string, unknown> }).EditTicket;
  if (op === undefined) return [];
  const out: { field: string; lead: string }[] = [];
  if (typeof op["title"] === "string") out.push({ field: "title", lead: "Not renamed." });
  const assignee = op["assignee"];
  if (assignee === "nobody") out.push({ field: "assignee", lead: "Not unassigned." });
  else if (typeof assignee === "object" && assignee !== null) {
    const account = (assignee as { account?: unknown }).account;
    out.push({ field: "assignee", lead: `Not assigned to ${String(account)}.` });
  }
  if (typeof op["priority"] === "string") out.push({ field: "priority", lead: `Not set to ${op["priority"]}.` });
  const labels = op["labels"] as { add?: string[]; remove?: string[] } | null | undefined;
  if (labels) {
    const add = labels.add ?? [];
    const remove = labels.remove ?? [];
    const lead = add.length > 0 ? `Not labelled ${add.join(", ")}.` : `${remove.join(", ")} not removed.`;
    out.push({ field: "labels", lead });
  }
  const own = op["fields"];
  if (typeof own === "object" && own !== null) for (const field of Object.keys(own)) out.push({ field, lead: "Not set." });
  return out;
}

/**
 * The notes of the queue's open rows, by entity and field: each refused row,
 * and each held one but an unconfirmed send, of an `EditTicket`. The newest
 * row of a field wins.
 */
export function fieldNotes(rows: readonly QueuedWrite[], keyOf: (entityId: string) => string, sourceName: (sourceId: string) => string): Record<string, Record<string, FieldNote>> {
  const notes: Record<string, Record<string, FieldNote>> = {};
  const ordered = [...rows].sort((a, b) => a.id - b.id);
  for (const row of ordered) {
    if (row.state !== "refused" && !(row.state === "held" && !mayHaveLanded(row))) continue;
    for (const { field, lead } of setFields(row.payload)) {
      const tone = row.state === "refused" ? "err" : "warn";
      const words =
        row.state === "refused"
          ? (row.detail ?? `${sourceName(row.source_id)} refused it.`)
          : `${keyOf(row.entity_id)} changed in ${sourceName(row.source_id)} since you edited it.`;
      (notes[row.entity_id] ??= {})[field] = { write: row.id, tone, lead, words };
    }
  }
  return notes;
}
