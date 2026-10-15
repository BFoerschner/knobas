/**
 * The reads and writes the job page (#816) calls, injectable so a test needs
 * no bridge. Every one is an existing command but the two conversation
 * reads #819 adds (`visit_entity`, `entity_thread`).
 */
import {
  checkoutCommands,
  createLink,
  entityCheckout,
  entityThread,
  getEntity,
  listEntities,
  openCheckout,
  promoteContext,
  reachableTransitions,
  roomSuggestions,
  submitWrite,
  unlink,
  visitEntity,
  type CheckoutCommand,
  type CheckoutView,
  type ContextRow,
  type EntityDetail,
  type EntityFilter,
  type EntityPage,
  type LinkRow,
  type SuggestionPage,
  type Thread,
} from "../../ipc/entity";
import { launcherHome, search } from "../../ipc/search";
import type { SessionPorts } from "../../launcher/session.svelte";
import { listSources, type QueuedWrite, type WriteOpPayload } from "../../ipc/sources";
import { weekTimesheet, type DayWindow, type Week } from "../../ipc/time";

export interface JobPorts {
  listSources: () => Promise<{ id: string }[]>;
  getEntity: (entityId: string) => Promise<EntityDetail>;
  promoteContext: (entityId: string) => Promise<ContextRow>;
  listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
  reachableTransitions: (entityId: string) => Promise<string[]>;
  submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
  entityCheckout: (entityId: string) => Promise<CheckoutView>;
  checkoutCommands: () => Promise<CheckoutCommand[]>;
  openCheckout: (entityId: string, action: string) => Promise<void>;
  weekTimesheet: (days: DayWindow[]) => Promise<Week>;
  // The conversation (#819) and the Connections drawer (#820).
  visitEntity: (entityId: string) => Promise<string | null>;
  entityThread: (entityId: string) => Promise<Thread>;
  createLink: (fromId: string, toId: string, relation?: string, note?: string) => Promise<LinkRow>;
  unlink: (linkId: string) => Promise<void>;
  roomSuggestions: (sources: string[], ctx: string | null, limit: number) => Promise<SuggestionPage>;
  /** What *Link to…* searches with: the launcher's own reads. */
  search: SessionPorts["search"];
  launcherHome: SessionPorts["launcherHome"];
}

/**
 * The bridge's. Each read is wrapped rather than referenced, so importing the
 * shell touches no command until a job is opened (a suite that mocks
 * `ipc/entity` without these still loads the shell).
 */
export const bridgePorts: JobPorts = {
  listSources: () => listSources(),
  getEntity: (entityId) => getEntity(entityId),
  promoteContext: (entityId) => promoteContext(entityId),
  listEntities: (filter, limit, offset) => listEntities(filter, limit, offset),
  reachableTransitions: (entityId) => reachableTransitions(entityId),
  submitWrite: (payload) => submitWrite(payload),
  entityCheckout: (entityId) => entityCheckout(entityId),
  checkoutCommands: () => checkoutCommands(),
  openCheckout: (entityId, action) => openCheckout(entityId, action),
  weekTimesheet: (days) => weekTimesheet(days),
  visitEntity: (entityId) => visitEntity(entityId),
  entityThread: (entityId) => entityThread(entityId),
  createLink: (fromId, toId, relation, note) => createLink(fromId, toId, relation, note),
  unlink: (linkId) => unlink(linkId),
  roomSuggestions: (sources, ctx, limit) => roomSuggestions(sources, ctx, limit),
  search: (query) => search(query),
  launcherHome: () => launcherHome(),
};
