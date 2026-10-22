/**
 * The *Open in…* applications (#844), one store for every menu and for
 * Settings › Checkouts: read once, written whole (`set_checkout_applications`),
 * and every menu redraws from the answer.
 */
import { ipcErrorMessage } from "../../ipc";
import {
  checkoutSettings,
  openCheckout,
  setCheckoutApplications,
  type CheckoutApplication,
  type CheckoutApplicationDraft,
  type CheckoutSettings,
  type OpenAt,
} from "../../ipc/entity";
import { push } from "../../shell/toasts.svelte";
import { argvText, defaultApp } from "./openin";

export interface AppsPorts {
  checkoutSettings: () => Promise<CheckoutSettings>;
  setCheckoutApplications: (drafts: CheckoutApplicationDraft[], defaultApplication: string | null) => Promise<CheckoutSettings>;
  openCheckout: (entityId: string, at: OpenAt) => Promise<string[]>;
}

export const realAppsPorts: AppsPorts = {
  checkoutSettings: () => checkoutSettings(),
  setCheckoutApplications: (drafts, defaultApplication) => setCheckoutApplications(drafts, defaultApplication),
  openCheckout: (entityId, at) => openCheckout(entityId, at),
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
  /** Run an application (or a command, for *Try it*) and say so in a toast. */
  run(entityId: string, name: string, at: OpenAt): Promise<void>;
  /** Tests only: forget the settings read (#1743). */
  reset(): void;
}

/** The draft the backend takes for an application that has, or has not yet, an id. */
export function draftOf(app: CheckoutApplication): CheckoutApplicationDraft {
  return { id: app.id === "" ? null : app.id, name: app.name, command: app.command, file_command: app.file_command };
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
      try {
        const argv = await ports.openCheckout(entityId, at);
        push({ text: `Started ${name}: ${argvText(argv)}` });
      } catch (cause) {
        push({ text: `Could not start ${name}: ${ipcErrorMessage(cause)}`, tone: "err" });
      }
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
