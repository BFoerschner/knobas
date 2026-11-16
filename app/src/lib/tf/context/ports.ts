/**
 * What a right-click's menu reads and writes, injectable as the launcher sheet's ports are (`sheet-ports.ts`):
 * production passes nothing and gets the app's own commands, the ones the pages already send; a test passes fakes.
 * Called through, not bound at load, so a test's mock of the bridge needn't carry every command.
 */
import { ackAlert, unackAlert } from "../../ipc/assets";
import {
  createLink,
  entityCheckout,
  entityHead,
  objectPage,
  openFolder,
  resolveUrl,
  submitWrite,
  type CheckoutView,
  type EntityDetail,
  type OpenObject,
  type UrlMatch,
} from "../../ipc/entity";
import { listSources, type QueuedWrite, type SourceSummary, type WriteOpPayload } from "../../ipc/sources";
import { inbox } from "../../inbox/inbox.svelte";
import { realSheetPorts } from "../../launcher/sheet-ports";
import { openExternal } from "../../shell/open-external";

export interface MenuPorts {
  head: (entityId: string) => Promise<EntityDetail>;
  checkout: (entityId: string) => Promise<CheckoutView>;
  objectPage: (repoId: string, object: OpenObject) => Promise<string | null>;
  /** The sources, read once a run: *Open in ⟨source⟩*'s names and the base URLs *Open in knobas* is offered on. */
  sources: () => Promise<SourceSummary[]>;
  resolveUrl: (url: string) => Promise<UrlMatch | null>;
  /** The clipboard: the app's one path (`navigator.clipboard.writeText`, as the sheet's `copyText`). */
  copyText: (text: string) => Promise<void>;
  /**
   * A copy whose text is still being read: the write starts inside the press, as WebKit asks, with the text to come
   * (`ClipboardItem`'s promise); where there is no `ClipboardItem`, once the text is there.
   */
  copyLater: (text: Promise<string>) => Promise<void>;
  /** The document's own copy of the selection, so a page's `copy` handler shapes what lands (the file view's lines). */
  copySelection: () => boolean;
  openExternal: (url: string) => Promise<void>;
  submitWrite: (payload: WriteOpPayload) => Promise<QueuedWrite>;
  ackAlert: (monitorId: string) => Promise<unknown>;
  unackAlert: (monitorId: string) => Promise<unknown>;
  createLink: (fromId: string, toId: string, relation: string) => Promise<unknown>;
  inboxAck: (key: string) => Promise<void>;
  inboxDone: (key: string) => Promise<void>;
  openFolder: (entityId: string, application: string, folder: string) => Promise<string[]>;
  clone: (repo: { entityId: string; title: string }) => Promise<void>;
  go: (hash: string) => void;
}

export const realMenuPorts: MenuPorts = {
  head: (entityId) => entityHead(entityId),
  checkout: (entityId) => entityCheckout(entityId),
  objectPage: (repoId, object) => objectPage(repoId, object),
  sources: () => listSources(),
  resolveUrl: (url) => resolveUrl(url),
  copyText: (text) => navigator.clipboard.writeText(text),
  async copyLater(text) {
    if (typeof ClipboardItem === "function" && typeof navigator.clipboard.write === "function") {
      const blob = text.then((value) => new Blob([value], { type: "text/plain" }));
      await navigator.clipboard.write([new ClipboardItem({ "text/plain": blob })]);
      return;
    }
    await navigator.clipboard.writeText(await text);
  },
  copySelection: () => document.execCommand("copy"),
  openExternal: (url) => openExternal(url),
  submitWrite: (payload) => submitWrite(payload),
  ackAlert: (monitorId) => ackAlert(monitorId),
  unackAlert: (monitorId) => unackAlert(monitorId),
  createLink: (fromId, toId, relation) => createLink(fromId, toId, relation),
  inboxAck: (key) => inbox.ack(key),
  inboxDone: (key) => inbox.complete(key),
  openFolder: (entityId, application, folder) => openFolder(entityId, application, folder),
  clone: (repo) => realSheetPorts.clone(repo),
  go: (hash) => {
    location.hash = hash;
  },
};
