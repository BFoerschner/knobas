/**
 * The ticket page's field controls, as one object the card reads (ADR-0054,
 * the 2026-11-21 ruling's sections 25 and 27).
 *
 * - **What is offered** is the source's `ticket_fields` answer, asked as the
 *   page opens: a field it lists without `readonly` is a control, one it
 *   lists `readonly` is text with the reason as its title, one it does not
 *   list is text as before. A refused credential (`refused_writes` of
 *   `edit_ticket`, and of `edit_ticket` with `field: "assignee"`) makes the
 *   fields it covers text too, with the missing permission named.
 * - **An adapter that offers no edits** (its answer is `null`: an adapter
 *   built before ticket fields, or one that edits nothing) draws its fields
 *   as text and one line: *⟨Source⟩'s adapter offers no edits here. Open the
 *   ticket in ⟨Source⟩ to change these.*
 * - **An edit** goes through `editTicket`: the value draws at once with its
 *   chip, and the page's next read settles it.
 * - **Notes** are read from the write queue as the page opens and whenever
 *   it moves (`activity:new`), so a refused edit says so after a restart too.
 */
import { untrack } from "svelte";

import { EVENTS } from "../../ipc";
import { discardWrite, pendingWrites, type QueuedWrite, type WriteOpPayload } from "../../ipc/sources";
import { fieldOptions, ticketFields, type EntityDetail, type FieldOption, type TicketField, type TicketFields } from "../../ipc/entity";
import { ipcErrorMessage } from "../../ipc";
import type { RefusedWrite } from "../../shell/refused-writes";
import { push } from "../../shell/toasts.svelte";
import { writeFollower, type WriteFollower } from "../../shell/follow-write";
import { echoesOn, settleEchoes, type Echo, type EchoListen } from "../queued.svelte";
import { askFields, askOptions, fieldsOf, optionsOf, type HeldOptions } from "../tickets/fields.svelte";
import { chipWords, editTicket, fieldKind, fieldNotes, type FieldNote, type TicketChange } from "../tickets/edits.svelte";
import { keyOf } from "./job";

/** The reads and writes the controls call; the bridge's where left out. */
export interface FieldPorts {
  ticketFields(entityId: string, behind: boolean): Promise<TicketFields | null>;
  fieldOptions(entityId: string, field: string, typed: string): Promise<{ options: FieldOption[]; complete: boolean } | null>;
  pendingWrites(): Promise<QueuedWrite[]>;
  discardWrite(id: number): Promise<void>;
}

export const bridgeFieldPorts: FieldPorts = {
  ticketFields: (entityId, behind) => ticketFields(entityId, behind),
  fieldOptions: (entityId, field, typed) => fieldOptions(entityId, field, typed),
  pendingWrites: () => pendingWrites(),
  discardWrite: (id) => discardWrite(id),
};

/** One fact's control, as the card draws it. */
export interface FactControl {
  /** The field as the source answered it; `null` where it does not list it. */
  field: TicketField | null;
  /** A press opens it. */
  can: boolean;
  /** Why it does not open, as its title; `null` where it opens or the source does not list it. */
  why: string | null;
  /** The newest echo of an edit not yet shown by the read. */
  echo: Echo | null;
  /** The chip after the value: *Assigning, queued*. */
  chip: string | null;
  /** What a refused or held edit of it says. */
  note: FieldNote | null;
}

/** What the card reads. */
export interface FieldEdits {
  /** The ticket's fields have been asked and answered (or failed), so nothing lands later. */
  readonly answered: boolean;
  /** The line under the facts of an adapter that offers no edits, or `null`. */
  readonly noEdits: string | null;
  /** The control of the field `id`. */
  control(id: string): FactControl;
  /** The labels' echoes: each name added or removed and not yet shown. */
  labelEchoes(): Echo[];
  /** What the window holds of a searched field's list. */
  options(field: TicketField): HeldOptions | null;
  /** Ask a searched field's list (empty `typed`: the first of it). */
  ask(field: TicketField, typed?: string): void;
  /** Edit: `change` is what the write sets, `text` the value as it reads, `name` the toast's words (`{key}` is the ticket's key). */
  edit(field: string, change: TicketChange, text: string, name: string): void;
  /** `×` on a note: discard its write. */
  dismiss(note: FieldNote): void;
}

export interface FieldEditsInput {
  detail: () => EntityDetail | null | undefined;
  refused: () => readonly RefusedWrite[];
  submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
  ports?: FieldPorts | undefined;
  listen?: EchoListen | undefined;
  follower?: WriteFollower | undefined;
}

/** The detail's value of field `id`, as the read holds it, for settling an echo. */
function readValue(detail: EntityDetail, id: string): string | null {
  switch (id) {
    case "title":
      return detail.row.title;
    case "assignee":
      return detail.author ?? null;
    case "priority":
      return detail.priority ?? null;
    default:
      return null;
  }
}

/**
 * The page's field edits. Call once per mount, inside a component: it asks
 * the fields when the ticket is read, and reads the queue's notes.
 */
export function createFieldEdits(input: FieldEditsInput): FieldEdits {
  const ports = input.ports ?? bridgeFieldPorts;
  const fieldsPort = { ticketFields: ports.ticketFields, fieldOptions: ports.fieldOptions };
  let notes = $state<Record<string, Record<string, FieldNote>>>({});

  const key = () => {
    const detail = input.detail();
    return detail ? { entity_id: detail.row.entity_id, updated_at: detail.row.updated_at ?? null } : null;
  };

  async function readNotes() {
    try {
      const rows = await ports.pendingWrites();
      notes = fieldNotes(rows, keyOf, (sourceId) => {
        const detail = input.detail();
        return detail && detail.source.id === sourceId ? detail.source.display_name : sourceId;
      });
    } catch {
      // Not known; the queue's next movement asks again.
    }
  }

  // The fields, as the page reads its ticket.
  $effect(() => {
    const k = key();
    if (k === null) return;
    untrack(() => void askFields(k, "open", fieldsPort));
  });

  // Each read of the ticket settles an echo whose value it shows.
  $effect(() => {
    const detail = input.detail();
    if (!detail) return;
    const id = detail.row.entity_id;
    const labels = detail.labels ?? [];
    const answer = fieldsOf({ entity_id: id, updated_at: detail.row.updated_at ?? null });
    untrack(() =>
      settleEchoes(id, (echo) => {
        if (!echo.kind.startsWith("field:")) return false;
        const field = echo.kind.slice("field:".length);
        if (field === "labels") {
          const name = echo.text.slice(1);
          return echo.text.startsWith("+") ? labels.includes(name) : !labels.includes(name);
        }
        const read = readValue(detail, field);
        if (read !== null || field === "assignee") return field === "assignee" ? (echo.base !== read) : read === echo.text;
        // A field the mirror does not hold: the fields' read shows it.
        if (answer?.state !== "ready" || answer.fields === null) return false;
        const now = answer.fields.fields.find((f) => f.id === field);
        return now !== undefined && (now.shown ?? String(now.value ?? "")) === echo.text;
      }),
    );
  });

  // The queue's notes, now and whenever it moves.
  $effect(() => {
    untrack(() => void readNotes());
    const listen = input.listen;
    if (listen === undefined) return;
    let off: (() => void) | null = null;
    let live = true;
    void listen(EVENTS.activityNew, () => void readNotes())
      .then((stop) => {
        if (live) off = stop;
        else stop();
      })
      .catch(() => {});
    return () => {
      live = false;
      off?.();
    };
  });

  function answer(): TicketFields | null | undefined {
    const k = key();
    if (k === null) return undefined;
    const held = fieldsOf(k);
    if (held === null || held.state === "asking") return undefined;
    return held.state === "ready" ? held.fields : null;
  }

  function credential(field: string): string | null {
    const detail = input.detail();
    if (!detail) return null;
    const entity = detail.row.entity_id;
    const refused = input.refused();
    const named = refused.find((r) => r.entity === entity && r.op === "edit_ticket" && r.field === field);
    if (named) return named.reason;
    // The assignee has its own permission; any other field goes with the edit as a whole.
    if (field === "assignee") return null;
    return refused.find((r) => r.entity === entity && r.op === "edit_ticket" && (r.field ?? null) === null)?.reason ?? null;
  }

  return {
    get answered() {
      const k = key();
      if (k === null) return false;
      const held = fieldsOf(k);
      return held !== null && held.state !== "asking";
    },
    get noEdits() {
      const detail = input.detail();
      if (!detail || answer() !== null) return null;
      const k = key();
      if (k === null || fieldsOf(k)?.state !== "ready") return null;
      const name = detail.source.display_name || detail.source.id;
      return `${name}'s adapter offers no edits here. Open the ticket in ${name} to change these.`;
    },
    control(id) {
      const detail = input.detail();
      const fields = answer();
      const field = fields?.fields.find((f) => f.id === id) ?? null;
      const refusal = field === null ? null : credential(id);
      const echo = detail ? (echoesOn(detail.row.entity_id, fieldKind(id)).at(-1) ?? null) : null;
      const note = detail ? (notes[detail.row.entity_id]?.[id] ?? null) : null;
      return {
        field,
        can: field !== null && !field.readonly && refusal === null,
        why: field?.readonly ?? refusal,
        echo: id === "labels" ? null : echo,
        chip: echo === null || id === "labels" ? null : chipWords(id, echo.text, echo.state),
        note,
      };
    },
    labelEchoes() {
      const detail = input.detail();
      return detail ? echoesOn(detail.row.entity_id, fieldKind("labels")) : [];
    },
    options(field) {
      const detail = input.detail();
      return detail ? optionsOf(detail.row.entity_id, field) : null;
    },
    ask(field, typed = "") {
      const detail = input.detail();
      if (detail) void askOptions(detail.row.entity_id, field, typed, fieldsPort);
    },
    edit(field, change, text, name) {
      const detail = input.detail();
      if (!detail) return;
      const base = field === "labels" ? null : (readValue(detail, field) ?? null);
      void editTicket(
        { entity_id: detail.row.entity_id, key: keyOf(detail.row.entity_id), source: detail.source },
        change,
        { field, text, base, name: name.replaceAll("{key}", keyOf(detail.row.entity_id)) },
        { submitWrite: input.submitWrite, follower: input.follower ?? writeFollower() },
      ).then(() => readNotes());
    },
    dismiss(note) {
      ports
        .discardWrite(note.write)
        .then(() => readNotes())
        .catch((cause: unknown) => push({ text: `Could not discard it: ${ipcErrorMessage(cause)}`, tone: "err" }));
    },
  };
}
