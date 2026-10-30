/**
 * The *Open in…* applications (#844), one store for every menu and for
 * Settings › Checkouts: read once, written whole (`set_checkout_applications`),
 * and every menu redraws from the answer.
 */
import { ipcErrorMessage } from "../../ipc";
import {
  checkoutSettings,
  openCheckout,
  openObject,
  setCheckoutApplications,
  type CheckoutApplication,
  type CheckoutApplicationDraft,
  type CheckoutSettings,
  type OpenAt,
  type OpenObject,
} from "../../ipc/entity";
import { push } from "../../shell/toasts.svelte";
import { argvText, defaultApp } from "./openin";

export interface AppsPorts {
  checkoutSettings: () => Promise<CheckoutSettings>;
  setCheckoutApplications: (drafts: CheckoutApplicationDraft[], defaultApplication: string | null) => Promise<CheckoutSettings>;
  openCheckout: (entityId: string, at: OpenAt) => Promise<string[]>;
  /** `open_object` (#2274); tests that open no code object leave it out. */
  openObject?: (entityId: string, application: string | null, object: OpenObject) => Promise<string[]>;
}

export const realAppsPorts: AppsPorts = {
  checkoutSettings: () => checkoutSettings(),
  setCheckoutApplications: (drafts, defaultApplication) => setCheckoutApplications(drafts, defaultApplication),
  openCheckout: (entityId, at) => openCheckout(entityId, at),
  openObject: (entityId, application, object) => openObject(entityId, application, object),
};

export interface Apps {
  /** `null` until the first read answers. */
  readonly settings: CheckoutSettings | null;
  /** Read it, unless it has been read. */
  ensure(): Promise<void>;
  /** Read it again: Settings opening, a clones root just set. */
  reload(): Promise<void>;
  /** Save the list and the default; rejects with the backend's refusal. */
  save(applications: readonly CheckoutApplication[], defaultApplication: string | null): Promise<void>;
  /** Run a saved application and say so in a toast. */
  run(entityId: string, name: string, at: OpenAt): Promise<void>;
  /** Run a saved application on a code object (#2274) and say so in a toast. */
  runObject(entityId: string, name: string, application: string, object: OpenObject): Promise<void>;
  /** Tests only: forget the settings read (#1743). */
  reset(): void;
}

/** The draft the backend takes for an application that has, or has not yet, an id. */
export function draftOf(app: CheckoutApplication): CheckoutApplicationDraft {
  return {
    id: app.id === "" ? null : app.id,
    name: app.name,
    command: app.command,
    file_command: app.file_command,
    commit_command: app.commit_command ?? null,
    range_command: app.range_command ?? null,
    pull_request_command: app.pull_request_command ?? null,
  };
}

export function createApps(ports: AppsPorts = realAppsPorts): Apps {
  let settings = $state.raw<CheckoutSettings | null>(null);
  let pending: Promise<void> | null = null;

  async function read() {
    try {
      settings = await ports.checkoutSettings();
    } catch {
      // A menu with no list draws only *Add an application…*; Settings says why.
    }
  }

  /** Start one, and say what started or why it did not. */
  async function said(name: string, start: () => Promise<string[]>) {
    try {
      const argv = await start();
      push({ text: `Started ${name}: ${argvText(argv)}` });
    } catch (cause) {
      push({ text: `Could not start ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
    }
  }

  return {
    get settings() {
      return settings;
    },
    ensure() {
      if (settings !== null) return Promise.resolve();
      pending ??= read().finally(() => (pending = null));
      return pending;
    },
    reload() {
      return read();
    },
    async save(applications, defaultApplication) {
      settings = await ports.setCheckoutApplications(applications.map(draftOf), defaultApplication);
    },
    reset() {
      settings = null;
      pending = null;
    },
    async run(entityId, name, at) {
      await said(name, () => ports.openCheckout(entityId, at));
    },
    async runObject(entityId, name, application, object) {
      const open = ports.openObject ?? realAppsPorts.openObject!;
      await said(name, () => open(entityId, application, object));
    },
  };
}

/** The window's one list. Tests build their own. */
export const apps = createApps();

/** Run the default application at a checkout, for a toast's *Open in VS Code*. */
export function runDefault(store: Apps, entityId: string): void {
  const app = defaultApp(store.settings);
  if (app !== null) void store.run(entityId, app.name, { application: app.id });
}
