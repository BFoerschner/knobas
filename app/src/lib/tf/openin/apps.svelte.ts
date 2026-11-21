/**
 * The *Open in…* applications (#844), one store for every menu and for
 * Settings › Checkouts: read once, written whole (`set_checkout_applications`),
 * and every menu redraws from the answer.
 */
import { ipcErrorMessage } from "../../ipc";
import {
  checkoutSettings,
  openCheckout,
  openFolder,
  openObject,
  setCheckoutApplications,
  type CheckoutApplication,
  type CheckoutApplicationDraft,
  type CheckoutSettings,
  type OpenAt,
  type OpenObject,
} from "../../ipc/entity";
import { acknowledge, lastName, openingWords } from "../../shell/ack.svelte";
import { push } from "../../shell/toasts.svelte";
import { defaultApp } from "./openin";

export interface AppsPorts {
  checkoutSettings: () => Promise<CheckoutSettings>;
  setCheckoutApplications: (drafts: CheckoutApplicationDraft[], defaultApplication: string | null) => Promise<CheckoutSettings>;
  openCheckout: (entityId: string, at: OpenAt) => Promise<string[]>;
  /** `open_object` (#2274); tests that open no code object leave it out. */
  openObject?: (entityId: string, application: string | null, object: OpenObject) => Promise<string[]>;
  /** `open_folder` (2026-11-16); tests that open no folder leave it out. */
  openFolder?: (entityId: string, application: string, folder: string) => Promise<string[]>;
}

export const realAppsPorts: AppsPorts = {
  checkoutSettings: () => checkoutSettings(),
  setCheckoutApplications: (drafts, defaultApplication) => setCheckoutApplications(drafts, defaultApplication),
  openCheckout: (entityId, at) => openCheckout(entityId, at),
  openObject: (entityId, application, object) => openObject(entityId, application, object),
  openFolder: (entityId, application, folder) => openFolder(entityId, application, folder),
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
  /**
   * Run a saved application: a chip where the press was says it is opening (`shell/ack.svelte.ts`, ux21), and a toast
   * says why when it didn't start.
   */
  run(entityId: string, name: string, at: OpenAt): Promise<void>;
  /**
   * Run a saved application on a code object (#2274), as {@link run} does. `item` names it on the chip in place of the
   * object's own name: *Try it* on a repository names the repository it was tried on.
   */
  runObject(entityId: string, name: string, application: string, object: OpenObject, item?: string | null): Promise<void>;
  /** Run a saved application on a folder of the checkout as on the checkout (2026-11-16), as {@link run} does. */
  runFolder(entityId: string, name: string, application: string, folder: string): Promise<void>;
  /**
   * Run a saved application on a file (Fable's ruling of 2026-11-16): its file command; one with none opens the file's
   * folder as it opens a folder, and the checkout for a file at its top.
   */
  runFile(entityId: string, app: CheckoutApplication, file: string, line: number | null): Promise<void>;
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
      // A menu with no list draws only *Add an application* and *Manage applications*; Settings says why.
    }
  }

  /**
   * Start one: the chip at once, named by the application and what it opens, never its command line or a path (Björn,
   * ux21: "the long path also makes it huge"); a toast only for why it did not start.
   */
  async function said(name: string, item: string | null, start: () => Promise<string[]>) {
    const ack = acknowledge(openingWords(name, item));
    try {
      await start();
      ack.settle();
    } catch (cause) {
      ack.fail();
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
      await said(name, at.file ? lastName(at.file) : null, () => ports.openCheckout(entityId, at));
    },
    async runObject(entityId, name, application, object, item) {
      const open = ports.openObject ?? realAppsPorts.openObject!;
      await said(name, item ?? objectName(object), () => open(entityId, application, object));
    },
    async runFolder(entityId, name, application, folder) {
      const open = ports.openFolder ?? realAppsPorts.openFolder!;
      await said(name, lastName(folder), () => open(entityId, application, folder));
    },
    async runFile(entityId, app, file, line) {
      const folder = fileFolder(app, file);
      if (folder === null) return this.run(entityId, app.name, { application: app.id, file, line });
      if (folder === "") return this.run(entityId, app.name, { application: app.id });
      return this.runFolder(entityId, app.name, app.id, folder);
    },
  };
}

/** What a code object is called on an opening's chip: a file's last name, a commit's short sha, a pull request's number. */
export function objectName(object: OpenObject): string | null {
  switch (object.kind) {
    case "repository":
      return null;
    case "file":
      return lastName(object.path);
    case "commit":
      return short(object.sha);
    case "range":
      return `${short(object.base)}…${short(object.head)}`;
    case "pull_request":
      return `#${object.number}`;
  }
}

/** A sha as seven characters; a branch's name as it is. */
function short(end: string): string {
  return /^[0-9a-f]{8,40}$/.test(end) ? end.slice(0, 7) : end;
}

/** The window's one list. Tests build their own. */
export const apps = createApps();

/** Run the default application at a checkout, for a toast's *Open in VS Code*. */
export function runDefault(store: Apps, entityId: string): void {
  const app = defaultApp(store.settings);
  if (app !== null) void store.run(entityId, app.name, { application: app.id });
}

/**
 * Where an application opens a file: `null` for the file itself (it has a file command), else the folder the file is
 * in, `""` for the checkout's top. An application that knows only folders opens the file's (Fable, 2026-11-16).
 */
export function fileFolder(app: Pick<CheckoutApplication, "file_command">, file: string): string | null {
  if (app.file_command) return null;
  const at = file.lastIndexOf("/");
  return at < 0 ? "" : file.slice(0, at);
}
