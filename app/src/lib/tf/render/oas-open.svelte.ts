/**
 * A spec opened in an API client (ux31, sections 23 to 27), as the file view's *Open in…* menu asks for it: what the
 * menu's small line says, and the open itself. The explorer makes one for the spec it shows; the menu reads it.
 *
 * - Whether the shipped Bruno is on this computer is asked once a session, when the first spec opens, so the menu
 *   knows before it is drawn: not found, Bruno is hidden (Björn).
 * - The collection is written by the spec's worker (`openapi-bru.ts`) only where the command names `{collection}` or
 *   the application is Bruno; the spec itself crosses as the text shown.
 * - On Windows and Linux Bruno cannot be pointed at a folder (section 24): knobas brings it forward and says where
 *   the folder is, with *Copy path*.
 */
import { apiClientsFound, apiOpenIn } from "../../ipc/api";
import { ipcErrorMessage, type CheckoutApplication } from "../../ipc";
import { onMac } from "../../shell/platform";
import { push } from "../../shell/toasts.svelte";
import { BRUNO } from "../openin/openin";

class Found {
  bruno = $state<{ bruno: boolean } | null>(null);
  #asked = false;
  /** Asks once a session; a failed ask finds nothing. */
  look(): void {
    if (this.#asked) return;
    this.#asked = true;
    void apiClientsFound().then(
      (got) => (this.bruno = got),
      () => (this.bruno = { bruno: false }),
    );
  }
}

/** Which shipped API clients are installed, asked once a session. */
export const clientsFound = new Found();

/** What the menu needs of the spec shown. */
export interface SpecOpener {
  title: string;
  /** How many requests and folders a collection holds, for Bruno's small line. */
  requests: number;
  folders: number;
  open: (app: CheckoutApplication) => Promise<void>;
}

/** The opener for one spec: its text, its worker's collection, and where it is kept. */
export function specOpener(spec: {
  title: string;
  text: string;
  path: string;
  key: string;
  requests: number;
  folders: number;
  collection: (specFile: string) => Promise<[string, string][]>;
}): SpecOpener {
  const ext = /^\s*[{[]/.test(spec.text) || spec.path.endsWith(".json") ? "json" : "yaml";
  return {
    title: spec.title,
    requests: spec.requests,
    folders: spec.folders,
    async open(app) {
      const wants = app.id === BRUNO || (app.api_command ?? "").includes("{collection}");
      try {
        const collection = wants ? await spec.collection(`openapi.${ext}`) : null;
        const opened = await apiOpenIn(app.id, { name: spec.title, spec_text: spec.text, spec_ext: ext, collection, key: spec.key });
        if (app.id === BRUNO && !onMac()) {
          const where = navigator.userAgent.includes("Windows") ? "Windows" : "Linux";
          push({
            text: `Bruno is open. It takes no folder from outside on ${where}: choose Open Collection there, and pick ${opened.folder}.`,
            subject: "api-open",
            ms: 20_000,
            action: { label: "Copy path", run: () => void navigator.clipboard.writeText(opened.folder) },
          });
        } else push({ text: `Opening ${spec.title} in ${app.name}`, subject: "api-open" });
      } catch (cause) {
        push({ text: ipcErrorMessage(cause), tone: "err", subject: "api-open" });
      }
    },
  };
}
