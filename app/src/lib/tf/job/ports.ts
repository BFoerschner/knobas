/**
 * The reads and writes the job page (#816, #817, #818, #819, #820) calls,
 * injectable so a test needs no bridge. Every one is an existing command but
 * the two conversation reads #819 adds (`visit_entity`, `entity_thread`),
 * `job_reach`, which #817 adds, and `job_link_sets`, #867's.
 */
import { getAsset, type AssetDetail } from "../../ipc/assets";
import {
  createLink,
  entityCheckout,
  entityThread,
  getEntity,
  hideLinkSetMember,
  jobLinkSets,
  jobReach,
  linkSets,
  listEntities,
  previewLinkSetRule,
  saveLinkSet,
  promoteContext,
  reachableTransitions,
  roomSuggestions,
  submitWrite,
  unlink,
  visitEntity,
  type CheckoutView,
  type ContextRow,
  type EntityDetail,
  type EntityFilter,
  type EntityPage,
  type JobLinkSets,
  type LinkRow,
  type SuggestionPage,
  type Thread,
  type Reached,
} from "../../ipc/entity";
import { launcherHome, search } from "../../ipc/search";
import type { SessionPorts } from "../../launcher/session.svelte";
import {
  credentialHealth,
  listSources,
  syncStatus,
  type CredentialHealth,
  type QueuedWrite,
  type SourceSyncStatus,
  type WriteOpPayload,
} from "../../ipc/sources";
import { weekTimesheet, type DayWindow, type Week } from "../../ipc/time";
import { BRIDGE as startWorkBridge, type StartWorkApi } from "../../start-work/start-work.svelte";
import type { RefusedWritesPort } from "../../shell/refused-writes";
import type { SetPorts } from "../sets/ports";
import type { SourceFacts } from "./job";

export interface JobPorts {
  listSources: () => Promise<SourceFacts[]>;
  getEntity: (entityId: string) => Promise<EntityDetail>;
  promoteContext: (entityId: string) => Promise<ContextRow>;
  listEntities: (filter: EntityFilter, limit: number, offset: number) => Promise<EntityPage>;
  reachableTransitions: (entityId: string) => Promise<string[]>;
  submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
  entityCheckout: (entityId: string) => Promise<CheckoutView>;
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
  /** What the ticket reaches (#817). */
  jobReach: (entityId: string) => Promise<Reached[]>;
  /** The link sets that apply and the parent (#867): *Also on this job* and the drawer's groups. */
  jobLinkSets: (entityId: string) => Promise<JobLinkSets>;
  /** *Hide here* and *Show again* (R-7.5); never edits the set. */
  hideLinkSetMember: (ticketId: string, memberId: string, hidden: boolean) => Promise<boolean>;
  /** One reached asset, for the Assets section. */
  getAsset: (assetId: string) => Promise<AssetDetail>;
  /** Every source's credential, for the Builds section's warning. */
  credentialHealth: () => Promise<CredentialHealth[]>;
  syncStatus: () => Promise<SourceSyncStatus[]>;
  /** The start-work flow's five commands (#44), the job's stepper's (#818). */
  startWork: StartWorkApi;
  /** What the ticket's source's credential cannot write (#2052); the bridge's when left out. */
  refusedWrites?: RefusedWritesPort;
}

/**
 * What *Apply a link set…* and *Save as a link set…* call (#869): existing
 * Link sets commands, no new ones. Apart from `JobPorts` so the job's other
 * suites need not stand them in.
 */
export type JobSetPorts = Pick<SetPorts, "linkSets" | "saveLinkSet" | "previewLinkSetRule">;

export const bridgeSetPorts: JobSetPorts = {
  linkSets: () => linkSets(),
  saveLinkSet: (draft) => saveLinkSet(draft),
  previewLinkSetRule: (rule) => previewLinkSetRule(rule),
};

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
  weekTimesheet: (days) => weekTimesheet(days),
  visitEntity: (entityId) => visitEntity(entityId),
  entityThread: (entityId) => entityThread(entityId),
  createLink: (fromId, toId, relation, note) => createLink(fromId, toId, relation, note),
  unlink: (linkId) => unlink(linkId),
  roomSuggestions: (sources, ctx, limit) => roomSuggestions(sources, ctx, limit),
  search: (query) => search(query),
  launcherHome: () => launcherHome(),
  jobReach: (entityId) => jobReach(entityId),
  jobLinkSets: (entityId) => jobLinkSets(entityId),
  hideLinkSetMember: (ticketId, memberId, hidden) => hideLinkSetMember(ticketId, memberId, hidden),
  getAsset: (assetId) => getAsset(assetId),
  credentialHealth: () => credentialHealth(),
  syncStatus: () => syncStatus(),
  startWork: startWorkBridge,
};
