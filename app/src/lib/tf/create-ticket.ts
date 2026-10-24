/**
 * The one create-ticket dialog's draft (#1451, `TfCreateTicket.svelte`): what
 * Standup's action item and a failed run's problem block file through it.
 */
import type { Project } from "../ipc/entity";
import type { WriteOpPayload } from "../ipc/sources";

/** What the dialog files: `CreateTicket`'s project entity, type, title and body. */
export interface TicketDraft {
  /** The project's entity id, `<source>:<key>`, or `null` until one is picked. */
  project: string | null;
  /** The source's own name for the kind of ticket (`Task`, `Bug`). */
  type: string;
  title: string;
  body: string;
}

/** A project's entity id, what `CreateTicket` takes as its `entity`. */
export function projectEntity(project: Pick<Project, "source_id" | "key">): string {
  return `${project.source_id}:${project.key}`;
}

/** The source a project's entity id names: what the ticket's write goes to. */
export function projectSource(project: string): string {
  return project.slice(0, Math.max(project.indexOf(":"), 0));
}

/** Whether a draft can be filed: a project, a type and a title. */
export function canFile(draft: TicketDraft): draft is TicketDraft & { project: string } {
  return draft.project !== null && draft.type.trim() !== "" && draft.title.trim() !== "";
}

/** The `CreateTicket` payload a filed draft queues. */
export function createTicketPayload(draft: TicketDraft & { project: string }): WriteOpPayload {
  return {
    CreateTicket: { entity: draft.project, title: draft.title.trim(), body: draft.body, ticket_type: draft.type.trim() },
  };
}

/**
 * The draft a queued `CreateTicket` filed (#1953): what *Edit and resend*
 * reopens for a create read back from the queue after a reload, when the
 * dialog's own draft is gone. `null` for any other op.
 */
export function draftOfPayload(payload: WriteOpPayload): (TicketDraft & { project: string }) | null {
  if (!("CreateTicket" in payload)) return null;
  const { entity, title, body, ticket_type } = payload.CreateTicket;
  return { project: entity, type: ticket_type, title, body };
}
