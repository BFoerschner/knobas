/**
 * What a ticket lets a person change, and a field's choices, kept by the
 * window (ADR-0054, the 2026-11-21 ruling's sections 17, 18 and 28).
 *
 * - **Fields** (`ticket_fields`) are asked when a ticket page opens and
 *   when a surface is about to offer an edit, and kept by the ticket as it
 *   stood (its id and `updated_at`), so a ticket the mirror shows changed is
 *   asked again. Never written anywhere; a landed write forgets them.
 * - **Options** (`field_options`) are kept for the session by the source,
 *   the field and the field's `scope`: a project's people are read once and
 *   every later ticket of that project opens its picker from memory. A list
 *   the source cut short (`complete: false`) grows by what a typed search
 *   adds, appended so nothing drawn moves.
 * - **Read ahead**: when a ticket's fields land and name a scope the window
 *   does not hold, its list is asked behind, so the first picker of a
 *   session opens full as well.
 *
 * Nothing here awaits on a press: `fieldsOf` and `optionsOf` are map reads.
 */
import { SvelteMap } from "svelte/reactivity";

import { fieldOptions, ticketFields } from "../../ipc/entity";
import { ipcErrorMessage } from "../../ipc";
import type { FieldOption, FieldOptions, TicketField, TicketFields } from "../../ipc/entity";

/** The two reads, as a test replaces them. */
export interface FieldsPort {
  ticketFields(entityId: string, behind: boolean): Promise<TicketFields | null>;
  fieldOptions(entityId: string, field: string, typed: string): Promise<FieldOptions | null>;
}

const live: FieldsPort = {
  ticketFields: (entityId, behind) => ticketFields(entityId, behind),
  fieldOptions: (entityId, field, typed) => fieldOptions(entityId, field, typed),
};

/** A ticket as the fields are keyed by: its id and when the mirror last saw it change. */
export interface FieldsKey {
  entity_id: string;
  updated_at?: string | null;
}

/** What the window holds of one ticket's fields. */
export type Held =
  /** The source's answer: `null` is a source that offers no edits on it. */
  | { state: "ready"; fields: TicketFields | null }
  | { state: "asking" }
  | { state: "failed"; message: string };

/** One field's choices as the window holds them. */
export interface HeldOptions {
  options: FieldOption[];
  complete: boolean;
  /** The last read's failure, where it failed. */
  failed?: string;
  /** A read is out. */
  asking: boolean;
}

/** The most tickets whose fields are kept, least recently used out first. */
export const FIELDS_HELD = 500;

const held = new SvelteMap<string, Held>();
const inFlight = new Map<string, Promise<TicketFields | null>>();
const options = new SvelteMap<string, HeldOptions>();
const optionsInFlight = new Map<string, Promise<HeldOptions>>();

function keyOf(ticket: FieldsKey): string {
  return `${ticket.entity_id}\u0000${ticket.updated_at ?? ""}`;
}

/** The scope a field's list is kept under. `null` for a field with no list. */
export function scopeOf(field: TicketField): string | null {
  const kind = field.kind;
  if (kind.type === "search") return kind.scope;
  if (kind.type === "labels" && kind.scope !== null) return kind.scope;
  return null;
}

function optionsKey(sourceId: string, fieldId: string, scope: string): string {
  return `${sourceId}\u0000${fieldId}\u0000${scope}`;
}

function sourceOf(entityId: string): string {
  const colon = entityId.indexOf(":");
  return colon === -1 ? entityId : entityId.slice(0, colon);
}

/** What the window holds of `ticket`'s fields now (reactive), `null` when never asked. */
export function fieldsOf(ticket: FieldsKey): Held | null {
  return held.get(keyOf(ticket)) ?? null;
}

/** The field `id` of what the window holds of `ticket`, or `null`. */
export function fieldOf(ticket: FieldsKey, id: string): TicketField | null {
  const answer = fieldsOf(ticket);
  if (answer?.state !== "ready" || answer.fields === null) return null;
  return answer.fields.fields.find((field) => field.id === id) ?? null;
}

function remember(key: string, value: Held): void {
  held.delete(key);
  held.set(key, value);
  while (held.size > FIELDS_HELD) {
    const oldest = held.keys().next().value;
    if (oldest === undefined) break;
    held.delete(oldest);
  }
}

/**
 * Ask `ticket`'s source what it lets a person change, unless an answer for
 * the ticket as it stands is held or out. `when` says whose read it is: a
 * page opening or a menu about to offer an edit is a person's; `ahead` (a
 * lane pickup, a rested card) goes in the background lane. Answers the
 * fields, or `null` for no edits; a failure is kept as `failed`.
 */
export function askFields(
  ticket: FieldsKey,
  when: "open" | "menu" | "ahead" = "open",
  port: FieldsPort = live,
): Promise<TicketFields | null> {
  const key = keyOf(ticket);
  const now = held.get(key);
  if (now?.state === "ready") return Promise.resolve(now.fields);
  const out = inFlight.get(key);
  if (out !== undefined) return out;
  remember(key, { state: "asking" });
  const read = port
    .ticketFields(ticket.entity_id, when === "ahead")
    .then(
      (fields) => {
        remember(key, { state: "ready", fields });
        if (fields !== null) readAhead(ticket.entity_id, fields, port);
        return fields;
      },
      (error: unknown) => {
        remember(key, { state: "failed", message: ipcErrorMessage(error) });
        return null;
      },
    )
    .finally(() => inFlight.delete(key));
  inFlight.set(key, read);
  return read;
}

/** Each list `fields` names that the window does not hold, asked behind. */
function readAhead(entityId: string, fields: TicketFields, port: FieldsPort): void {
  const source = sourceOf(entityId);
  for (const field of fields.fields) {
    const scope = scopeOf(field);
    if (scope === null || field.readonly) continue;
    if (options.has(optionsKey(source, field.id, scope))) continue;
    void askOptions(entityId, field, "", port);
  }
}

/** What the window holds of `field`'s choices for tickets of `entityId`'s source (reactive). */
export function optionsOf(entityId: string, field: TicketField): HeldOptions | null {
  const scope = scopeOf(field);
  if (scope === null) return null;
  return options.get(optionsKey(sourceOf(entityId), field.id, scope)) ?? null;
}

/**
 * Ask `field`'s choices matching `typed` (empty: the first of them). With
 * nothing typed, a list held for the field's scope is answered from memory
 * and nothing is sent. With words typed, the source's answer is merged into
 * what is held: options not there yet are appended in the order the source
 * gave them, and none drawn moves.
 */
export function askOptions(
  entityId: string,
  field: TicketField,
  typed = "",
  port: FieldsPort = live,
): Promise<HeldOptions> {
  const scope = scopeOf(field);
  if (scope === null) return Promise.resolve({ options: [], complete: true, asking: false });
  const key = optionsKey(sourceOf(entityId), field.id, scope);
  const now = options.get(key);
  if (typed === "" && now !== undefined && !now.failed) return Promise.resolve(now);
  const readKey = `${key}\u0000${typed}`;
  const out = optionsInFlight.get(readKey);
  if (out !== undefined) return out;
  options.set(key, { options: now?.options ?? [], complete: now?.complete ?? false, asking: true });
  const read = port
    .fieldOptions(entityId, field.id, typed)
    .then(
      (answer) => merge(key, answer?.options ?? [], typed === "" ? (answer?.complete ?? true) : null),
      (error: unknown) => {
        const was = options.get(key);
        const failed: HeldOptions = {
          options: was?.options ?? [],
          complete: was?.complete ?? false,
          failed: ipcErrorMessage(error),
          asking: false,
        };
        options.set(key, failed);
        return failed;
      },
    )
    .finally(() => optionsInFlight.delete(readKey));
  optionsInFlight.set(readKey, read);
  return read;
}

/** `found` added to the list at `key`, at its end, each value once. `complete` replaces the flag where given. */
function merge(key: string, found: readonly FieldOption[], complete: boolean | null): HeldOptions {
  const was = options.get(key);
  const list = [...(was?.options ?? [])];
  const seen = new Set(list.map((option) => option.value));
  for (const option of found) {
    if (seen.has(option.value)) continue;
    seen.add(option.value);
    list.push(option);
  }
  const next: HeldOptions = { options: list, complete: complete ?? was?.complete ?? false, asking: false };
  options.set(key, next);
  return next;
}

/** Forget what is held of one ticket's fields (a landed write), or of everything (a test). */
export function forgetFields(entityId?: string): void {
  if (entityId === undefined) {
    held.clear();
    inFlight.clear();
    options.clear();
    optionsInFlight.clear();
    return;
  }
  for (const key of [...held.keys()]) if (key.startsWith(`${entityId}\u0000`)) held.delete(key);
  for (const key of [...inFlight.keys()]) if (key.startsWith(`${entityId}\u0000`)) inFlight.delete(key);
}
