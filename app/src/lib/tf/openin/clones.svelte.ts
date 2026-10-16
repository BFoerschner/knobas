/**
 * The clones this window started (#846, R-8.25), over `clone_repo` and its
 * `clone:progress` events (#823): one store, so a clone started on a row of
 * Repositories reads *Cloning… 52 %* on the repository's own page too.
 *
 * Held for the session and never stored: a clone that is running belongs to
 * the backend, and the scan finds it once it is done.
 */
import { listen as tauriListen } from "@tauri-apps/api/event";

import { EVENTS, ipcErrorMessage } from "../../ipc";
import { cloneRepo, entityCheckout, type CloneAnswer, type CloneEvent, type CheckoutView } from "../../ipc/entity";
import { push } from "../../shell/toasts.svelte";
import { apps as sharedApps, runDefault, type Apps } from "./apps.svelte";
import { cloneFailure, defaultApp } from "./openin";

export interface ClonePorts {
  cloneRepo: (entityId: string, stop?: boolean) => Promise<CloneAnswer>;
  entityCheckout: (entityId: string) => Promise<CheckoutView>;
  listen: (event: string, handler: (event: { payload: unknown }) => void) => Promise<() => void>;
}

export const realClonePorts: ClonePorts = {
  cloneRepo: (entityId, stop) => cloneRepo(entityId, stop),
  entityCheckout: (entityId) => entityCheckout(entityId),
  listen: (event, handler) => tauriListen(event, handler),
};

/** Where one repository's clone is, as its button draws it. */
export type CloneMark =
  | { kind: "running"; percent: number; path: string }
  | { kind: "failed"; text: string };

/** What the store needs to say about a repository: its name and its URL's host. */
export interface CloneSubject {
  entityId: string;
  /** The repository's own name, `payout-service`. */
  name: string;
  repoUrl: string | null;
  /** The clones root the folder goes under, for *Nothing was left in ~/code*. */
  root: string | null;
}

export interface Clones {
  /** By repo entity id. */
  readonly marks: Readonly<Record<string, CloneMark>>;
  /** Moves each time a clone finishes, so a page re-reads its checkouts. */
  readonly finished: number;
  start(subject: CloneSubject): Promise<void>;
  stop(entityId: string): Promise<void>;
}

export function createClones(ports: ClonePorts = realClonePorts, apps: Apps = sharedApps): Clones {
  const marks = $state<Record<string, CloneMark>>({});
  let finished = $state(0);
  const subjects = new Map<string, CloneSubject>();
  let listening: Promise<unknown> | null = null;

  function failed(entityId: string, text: string) {
    marks[entityId] = { kind: "failed", text };
  }

  async function done(entityId: string, path: string) {
    delete marks[entityId];
    finished += 1;
    const subject = subjects.get(entityId);
    const view = await ports.entityCheckout(entityId).catch(() => null);
    const branch = view?.tree?.status === "read" ? view.tree.branch : null;
    await apps.ensure();
    const app = defaultApp(apps.settings);
    push({
      text: `Cloned ${subject?.name ?? entityId} into ${path}${branch ? `, on ${branch}` : ""}`,
      ...(app ? { action: { label: `Open in ${app.name}`, run: () => runDefault(apps, entityId) } } : {}),
    });
  }

  function onEvent(event: CloneEvent) {
    const id = event.entity_id;
    if (event.state === "progress") {
      marks[id] = { kind: "running", percent: event.percent, path: event.path };
    } else if (event.state === "done") {
      void done(id, event.path);
    } else if (event.state === "failed") {
      failed(id, cloneFailure(event.cause, event.reason, subjects.get(id)?.repoUrl ?? null));
    } else if (event.state === "stopped") {
      delete marks[id];
    }
  }

  function listen() {
    listening ??= ports
      .listen(EVENTS.cloneProgress, ({ payload }) => onEvent(payload as CloneEvent))
      .catch(() => (listening = null));
    return listening;
  }

  return {
    get marks() {
      return marks;
    },
    get finished() {
      return finished;
    },
    async start(subject) {
      subjects.set(subject.entityId, subject);
      await listen();
      marks[subject.entityId] = { kind: "running", percent: 0, path: "" };
      try {
        const answer = await ports.cloneRepo(subject.entityId);
        if (answer.status === "adopted") {
          await done(subject.entityId, answer.path);
        } else if (answer.status === "cloning") {
          const mark = marks[subject.entityId];
          if (mark?.kind === "running") mark.path = answer.path;
        }
      } catch (cause) {
        failed(subject.entityId, ipcErrorMessage(cause));
      }
    },
    async stop(entityId) {
      const subject = subjects.get(entityId);
      try {
        const answer = await ports.cloneRepo(entityId, true);
        delete marks[entityId];
        if (answer.status === "stopped") {
          const where = subject?.root ?? answer.path;
          push({
            text: answer.removed
              ? `Stopped cloning ${subject?.name ?? entityId}. Nothing was left in ${where}.`
              : `Stopped cloning ${subject?.name ?? entityId}. ${answer.path} is still there; remove it by hand.`,
          });
        }
      } catch (cause) {
        push({ text: `Could not stop the clone: ${ipcErrorMessage(cause)}`, tone: "err" });
      }
    },
  };
}

/** The window's one store. Tests build their own. */
export const clones = createClones();
