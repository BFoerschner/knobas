/**
 * A ticket's fields from its card (ADR-0054, the 2026-11-21 ruling's section 26): the page drawing cards offers
 * *Assign to…* and *Priority ▸* in a ticket's right-click menu while it is shown, as it offers *Move to ▸*
 * (`moves.svelte.ts`). The newest page offering answers; the menu reads it reactively, so an answer of
 * `ticket_fields` that lands while the menu stands redraws it.
 */
import { SvelteMap } from "svelte/reactivity";

import type { FieldOption, MiniBoardCard, TicketField } from "../../ipc/entity";
import type { QueuedWrite, WriteOpPayload } from "../../ipc/sources";
import { submitWrite as realSubmitWrite } from "../../ipc/entity";
import { askRefused, type RefusedWritesPort } from "../../shell/refused-writes";
import type { WriteFollower } from "../../shell/follow-write";
import type { FieldOffer } from "../context/items";
import { echoesOn, settleEchoes } from "../queued.svelte";
import type { FieldEchoed, LanedBy } from "./echo-board";
import { sameStatus, type LaneTakes } from "./drop";
import { chipWords, editTicket, fieldKind, type TicketChange } from "./edits.svelte";
import { askFields, askOptions, fieldOf, fieldsOf, optionsOf, type FieldsPort, type HeldOptions } from "./fields.svelte";

export interface FieldProvider {
  /** What the menu offers on this ticket, or `null` for one the page does not hold. */
  offer(entityId: string): FieldOffer | null;
  /** The menu opened on it: ask its fields. */
  ask(entityId: string): void;
  /** *Assign to…*: the people picker, standing at the card. */
  assign(entityId: string): void;
  /** *Priority ▸ ⟨label⟩*. */
  priority(entityId: string, value: string, label: string): void;
}

const providers: FieldProvider[] = [];
/** Moves when a provider comes or goes, so a reader of {@link fieldProvider} is reactive. */
let offered = $state(0);

/** A page offers a card's field edits while it is shown; answers the stop. */
export function offerFields(provider: FieldProvider): () => void {
  providers.push(provider);
  offered += 1;
  return () => {
    const at = providers.indexOf(provider);
    if (at >= 0) providers.splice(at, 1);
    offered += 1;
  };
}

/** The page offering field edits now, or `null`. */
export function fieldProvider(): FieldProvider | null {
  void offered;
  return providers.at(-1) ?? null;
}

/** Forget every provider: a new session, which is what each test is. */
export function forgetFieldProviders(): void {
  providers.length = 0;
}

// -- the page's side: what a card edits, its echoes and its lanes --

/** The surface a card's edit echoes are tagged with: the Tickets page reads its board again for them. */
export const TICKETS_SURFACE = "tickets";

export interface CardFieldsHost {
  /** The drawn card of an entity, or `null`. */
  cardOf(entityId: string): MiniBoardCard | null;
  sourceName(sourceId: string): string;
  /** Whether the source's adapter declares `edit_ticket`. */
  edits(sourceId: string): boolean;
  /** *Assign to…* or `a`: open the people picker at the card. */
  openPicker(card: MiniBoardCard): void;
  /** The bridge's where left out. */
  submitWrite?: ((payload: WriteOpPayload) => Promise<QueuedWrite>) | undefined;
  refused?: RefusedWritesPort | undefined;
  fields?: FieldsPort | undefined;
  follower?: WriteFollower | undefined;
}

export interface CardFields {
  offer(entityId: string): FieldOffer | null;
  /** Ask its fields: a menu about to offer an edit, or `ahead` (a lane pickup) in the background lane. */
  ask(entityId: string, when?: "menu" | "ahead"): void;
  /** The ticket's assignee field, where its source lists one. */
  assignee(card: MiniBoardCard): TicketField | null;
  /** Why its field `id` cannot be changed, or `null`. */
  why(card: MiniBoardCard, id: string): string | null;
  options(card: MiniBoardCard, field: TicketField): HeldOptions | null;
  search(card: MiniBoardCard, field: TicketField, typed: string): void;
  assign(card: MiniBoardCard, account: string | null, shown: string): void;
  priority(card: MiniBoardCard, value: string, label: string): void;
  /** Its edits not yet shown by the read, as the board draws them. */
  echoed(entityId: string): FieldEchoed | null;
  /** The chip of its newest field edit, for the head line. */
  chip(entityId: string): { words: string; other: string; tone: "info" | "muted" } | null;
  /** Each read's cards: an echo the read shows goes. */
  settle(cards: readonly MiniBoardCard[]): void;
  /** Whether another lane's value is one its field takes (`drop.ts`'s rule 2). */
  lane(card: MiniBoardCard, by: LanedBy, key: string): LaneTakes;
  /** A drop in another lane: the lane's value queued. */
  relane(card: MiniBoardCard, by: LanedBy, key: string): string | null;
}

/** The words a lane's key reads as: *Unassigned*, *No priority*, or the key. */
function laneWords(by: LanedBy, key: string): string {
  if (key !== "") return key;
  return by === "assignee" ? "Unassigned" : "No priority";
}

export function createCardFields(host: CardFieldsHost): CardFields {
  /** `refused_writes` for `edit_ticket` on a ticket: the assignee's own, and the edit's as a whole. */
  const refusals = new SvelteMap<string, { assignee: string | null; edit: string | null }>();
  const port = (): FieldsPort | undefined => host.fields;

  function why(card: MiniBoardCard, id: string): string | null {
    const field = fieldOf(card, id);
    if (field?.readonly) return field.readonly;
    const refused = refusals.get(card.entity_id);
    if (refused === undefined) return null;
    return id === "assignee" ? refused.assignee : refused.edit;
  }

  function edit(card: MiniBoardCard, field: string, change: TicketChange, text: string, base: string | null, name: string, into?: string) {
    void editTicket(
      { entity_id: card.entity_id, key: card.key, source: { id: card.source_id, display_name: host.sourceName(card.source_id) } },
      change,
      { field, text, base, name: name.replaceAll("{key}", card.key), ...(into === undefined ? {} : { into }) },
      { submitWrite: host.submitWrite ?? realSubmitWrite, ...(host.follower ? { follower: host.follower } : {}) },
      TICKETS_SURFACE,
    );
  }

  function assign(card: MiniBoardCard, account: string | null, shown: string) {
    if ((card.assignee ?? null) === account) return;
    const text = account === null ? "Unassigned" : shown;
    edit(card, "assignee", { assignee: account === null ? "nobody" : { account } }, text, card.assignee_name ?? card.assignee ?? null, account === null ? "Unassign {key}" : `Assign {key} to ${shown}`, account ?? "");
  }

  function priority(card: MiniBoardCard, value: string, label: string) {
    if (sameStatus(card.priority, value)) return;
    edit(card, "priority", { priority: value }, label, card.priority, `Set {key}’s priority to ${label}`);
  }

  function lane(card: MiniBoardCard, by: LanedBy, key: string): LaneTakes {
    if (by !== "assignee" && by !== "priority") {
      const what = by === "epic" ? "epic" : by === "issuetype" ? "issue type" : "labels";
      return { kind: "no", why: `knobas does not change a ticket's ${what}. Drop it in its own lane.` };
    }
    const source = host.sourceName(card.source_id);
    if (!host.edits(card.source_id)) return { kind: "no", why: `${source}'s adapter offers no edits. Drop it in its own lane.` };
    const held = fieldsOf(card);
    if (held === null || held.state !== "ready") return { kind: "ask" };
    const field = held.fields?.fields.find((one) => one.id === by) ?? null;
    if (field === null) return { kind: "no", why: `${source} has no ${by} on ${card.key}. Drop it in its own lane.` };
    const refused = why(card, by);
    if (refused !== null) return { kind: "no", why: refused };
    if (key === "" && by === "priority") return { kind: "no", why: "knobas does not take a ticket's priority away. Drop it in a priority's lane." };
    if (key === "" && field.required) return { kind: "no", why: `${card.key} must have an assignee.` };
    if (by === "priority" && key !== "" && field.kind.type === "choice" && !field.kind.options.some((one) => sameStatus(one.value, key))) {
      return { kind: "no", why: `${source} has no priority ${key}.` };
    }
    return { kind: "yes" };
  }

  return {
    offer(entityId) {
      const card = host.cardOf(entityId);
      if (card === null || !host.edits(card.source_id)) return null;
      const held = fieldsOf(card);
      const source = host.sourceName(card.source_id);
      if (held?.state === "ready") {
        const fields = held.fields?.fields ?? [];
        const assignee = fields.find((one) => one.id === "assignee") ?? null;
        const priority = fields.find((one) => one.id === "priority") ?? null;
        const options: FieldOption[] = priority?.kind.type === "choice" ? priority.kind.options : [];
        return {
          assign: assignee === null ? null : { disabled: why(card, "assignee") },
          priority:
            priority === null
              ? null
              : { options: options.map(({ value, label }) => ({ value, label })), current: priority.value == null ? null : String(priority.value), asking: null, disabled: why(card, "priority") },
        };
      }
      // Not answered yet: offered, and the menu redraws when it is.
      return { assign: { disabled: null }, priority: { options: [], current: null, asking: `Asking ${source}…`, disabled: null } };
    },
    ask(entityId, when = "menu") {
      const card = host.cardOf(entityId);
      if (card === null || !host.edits(card.source_id)) return;
      void askFields(card, when, port());
      if (refusals.has(entityId)) return;
      void askRefused(
        [
          { entity: entityId, op: "edit_ticket", field: "assignee" },
          { entity: entityId, op: "edit_ticket" },
        ],
        host.refused,
      ).then((answer) => {
        const of = (field: string | null) => answer.find((one) => one.entity === entityId && one.op === "edit_ticket" && (one.field ?? null) === field)?.reason ?? null;
        refusals.set(entityId, { assignee: of("assignee"), edit: of(null) });
      });
    },
    assignee: (card) => fieldOf(card, "assignee"),
    why,
    options: (card, field) => optionsOf(card.entity_id, field),
    search: (card, field, typed) => void askOptions(card.entity_id, field, typed, port()),
    assign,
    priority,
    echoed(entityId) {
      const echoes = echoesOn(entityId).filter((one) => one.kind.startsWith("field:"));
      if (echoes.length === 0) return null;
      const out: FieldEchoed = {};
      for (const echo of echoes) {
        const field = echo.kind.slice("field:".length);
        if (field === "assignee") out.assignee = { account: echo.into ?? "", name: echo.text };
        else if (field === "priority") out.priority = echo.text;
        else if (field === "title") out.title = echo.text;
        else if (field === "labels") (out.labels ??= []).push(echo.text);
      }
      return out;
    },
    chip(entityId) {
      const echo = echoesOn(entityId).filter((one) => one.kind.startsWith("field:")).at(-1);
      if (echo === undefined) return null;
      const field = echo.kind.slice("field:".length);
      const sent = echo.state === "sent";
      return { words: chipWords(field, echo.text, echo.state), other: chipWords(field, echo.text, sent ? "queued" : "sent"), tone: sent ? "muted" : "info" };
    },
    settle(cards) {
      for (const card of cards) {
        if (!echoesOn(card.entity_id).some((one) => one.kind.startsWith("field:"))) continue;
        settleEchoes(card.entity_id, (echo) => {
          switch (echo.kind) {
            case fieldKind("assignee"):
              return (echo.into ?? "") === (card.assignee ?? "");
            case fieldKind("priority"):
              return sameStatus(echo.text, card.priority);
            case fieldKind("title"):
              return echo.text === card.title;
            case fieldKind("labels"):
              return echo.text.startsWith("+") === card.labels.includes(echo.text.slice(1));
            default:
              return false;
          }
        });
      }
    },
    lane,
    relane(card, by, key) {
      if (lane(card, by, key).kind === "no") return null;
      const words = laneWords(by, key);
      if (by === "assignee") {
        const field = fieldOf(card, "assignee");
        const listed = field === null ? undefined : optionsOf(card.entity_id, field)?.options.find((one) => one.value === key)?.label;
        const name = key === "" ? words : (listed ?? key);
        assign(card, key === "" ? null : key, name);
        return key === "" ? `${card.key} unassigned. Queued.` : `${card.key} assigned to ${name}. Queued.`;
      }
      priority(card, key, words);
      return `${card.key} set to ${words}. Queued.`;
    },
  };
}
