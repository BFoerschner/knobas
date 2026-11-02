/**
 * The writes and reads the Tab sheet (#2426) reaches, injectable as the
 * session's ports are: production passes nothing and gets the app's own
 * commands, the ones the pages already send; a test passes fakes.
 */
import { ackAlert } from "../ipc/assets";
import { checkoutSettings, createLink, entityCheckout, listEntities, openCheckout, submitWrite } from "../ipc/entity";
import type { CheckoutSettings, EntityFilter, OpenAt } from "../ipc/entity";
import type { WriteOpPayload } from "../ipc/sources";
import { inbox } from "../inbox/inbox.svelte";
import { clones } from "../tf/openin/clones.svelte";
import { repoName } from "../tf/openin/openin";
import { readAllRows, repoOfRow } from "../tf/repo/repo";

export interface SheetPorts {
  createLink: (fromId: string, toId: string, relation: string) => Promise<unknown>;
  submitWrite: (payload: WriteOpPayload) => Promise<unknown>;
  ackAlert: (monitorId: string) => Promise<unknown>;
  copyText: (text: string) => Promise<void>;
  /** `open_checkout`: answers the argument vector it started. */
  openCheckout: (entityId: string, at: OpenAt) => Promise<string[]>;
  checkoutSettings: () => Promise<CheckoutSettings>;
  /** A pull request's repository, by its declared parent (`path`, `owner/name`) in its source; `null` when the mirror holds none. */
  repoOf: (pr: { sourceId: string; path: string | null }) => Promise<string | null>;
  /** *Clone* (ADR-0023), through the window's clone store, so the repository's page shows the progress. */
  clone: (repo: { entityId: string; title: string }) => Promise<void>;
  /** The Inbox page's own writes on a line (`inbox.svelte.ts`). */
  inboxAck: (key: string) => Promise<void>;
  inboxDone: (key: string) => Promise<void>;
}

/** One read's rows, as the pull request page reads its repository (`TfPr.readRepo`). */
const LIMIT = 1000;

export const realSheetPorts: SheetPorts = {
  createLink: (fromId, toId, relation) => createLink(fromId, toId, relation),
  submitWrite: (payload) => submitWrite(payload),
  ackAlert: (monitorId) => ackAlert(monitorId),
  copyText: (text) => navigator.clipboard.writeText(text),
  openCheckout: (entityId, at) => openCheckout(entityId, at),
  checkoutSettings: () => checkoutSettings(),
  async repoOf(pr) {
    if (pr.path === null) return null;
    const row = { source_id: pr.sourceId, path: pr.path };
    const filter: EntityFilter = {
      sources: [pr.sourceId],
      kinds: ["repo"],
      updated_within_days: null,
      order: "updated_desc",
      context: null,
      project: null,
      include_deleted: false,
      reach: null,
    };
    const rows = await readAllRows(
      (offset) => listEntities(filter, LIMIT, offset),
      (read) => repoOfRow(row, read) !== null,
    );
    return repoOfRow(row, rows)?.entity_id ?? null;
  },
  async clone(repo) {
    const checkout = await entityCheckout(repo.entityId);
    if (checkout.clones_root === null) throw new Error("Set where clones go in Settings › Checkouts first.");
    await clones.start({ entityId: repo.entityId, name: repoName(repo.title), repoUrl: checkout.repo_url, root: checkout.clones_root });
  },
  inboxAck: (key) => inbox.ack(key),
  inboxDone: (key) => inbox.complete(key),
};
