/**
 * The reads and writes the job page (#816) calls, injectable so a test needs
 * no bridge. Every one is an existing command; the job adds none.
 */
import {
  checkoutCommands,
  entityCheckout,
  getEntity,
  listEntities,
  openCheckout,
  promoteContext,
  reachableTransitions,
  submitWrite,
  type CheckoutCommand,
  type CheckoutView,
  type ContextRow,
  type EntityDetail,
  type EntityFilter,
  type EntityPage,
} from "../../ipc/entity";
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
};
