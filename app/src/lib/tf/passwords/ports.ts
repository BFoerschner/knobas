/**
 * The Passwords page's reads and actions (ADR-0056), injectable so a test needs no bridge. The real ones are the
 * `passwords` barrel's commands; a plain copy (a username, an address) is the window's own, which then tells the Rust
 * side its deadline is moot.
 */
import {
  clipboardCopySecret,
  clipboardForget,
  otpCode,
  otpCopy,
  passwordCopy,
  passwordHide,
  passwordShow,
  passwordsEntry,
  passwordsHeld,
  passwordsWrite,
  passwordAttachment,
  passwordAttachmentSave,
  passwordWebLink,
  type AttachmentAnswer,
  type CodeAnswer,
  type Held,
  type PwEntry,
  type ReadAnswer,
  type SecretRef,
  type WebLink,
  type Write,
  type WriteAnswer,
} from "../../ipc/passwords";
import { openWeb, openWebPort } from "../../shell/open-web";

export interface PasswordPorts {
  held: () => Promise<Held>;
  entry: (entityId: string) => Promise<PwEntry | null>;
  show: (entityId: string, which: SecretRef, comment: string | null) => Promise<ReadAnswer>;
  copy: (entityId: string, which: SecretRef, comment: string | null) => Promise<ReadAnswer>;
  hide: (entityId?: string, which?: SecretRef) => Promise<void>;
  code: (entityId: string) => Promise<CodeAnswer>;
  copyCode: (entityId: string) => Promise<CodeAnswer>;
  /** Copy text that is no secret (a username, an address). */
  copyPlain: (text: string) => Promise<void>;
  openExternal: (url: string) => Promise<void>;
  /** A direct write (ADR-0056). */
  write: (write: Write) => Promise<WriteAnswer>;
  /** Copy a value typed or generated in a dialog, as a secret. */
  copySecret: (text: string, what: string) => Promise<unknown>;
  /** One attachment, read live for the viewer (step 9a). */
  attachment: (entityId: string, attachmentId: string) => Promise<AttachmentAnswer>;
  /** One attachment saved to the Downloads folder; answers its path. */
  saveAttachment: (entityId: string, attachmentId: string) => Promise<string>;
  /** The source's web interface on an entry or folder, or its front page (ux30). */
  webLink: (source: string, item: string | null) => Promise<WebLink | null>;
  /** Open a web interface address: its source's site, or the browser when `elsewhere` (Shift W, ⌘ press). */
  openWeb: (url: string, source: string, elsewhere: boolean, item: string | null) => void;
}

export const realPasswordPorts: PasswordPorts = {
  held: () => passwordsHeld(),
  entry: (id) => passwordsEntry(id),
  show: (id, which, comment) => passwordShow(id, which, comment),
  copy: (id, which, comment) => passwordCopy(id, which, comment),
  hide: (id, which) => passwordHide(id, which),
  code: (id) => otpCode(id),
  copyCode: (id) => otpCopy(id),
  async copyPlain(text) {
    await navigator.clipboard.writeText(text);
    await clipboardForget().catch(() => {});
  },
  openExternal: (url) => openWebPort()(url),
  write: (write) => passwordsWrite(write),
  copySecret: (text, what) => clipboardCopySecret(text, what),
  attachment: (id, aid) => passwordAttachment(id, aid),
  saveAttachment: (id, aid) => passwordAttachmentSave(id, aid),
  webLink: (source, item) => passwordWebLink(source, item),
  openWeb: (url, source, elsewhere, item) => openWeb(url, { source, elsewhere, item }),
};

/** The password, as a secret ref. */
export const PASSWORD: SecretRef = { kind: "password" };

/** The usage comment last sent this session: what the next prompt starts with. Written nowhere. */
let lastComment: string | null = null;

export function rememberComment(comment: string): void {
  lastComment = comment;
}

/** What a usage comment prompt starts with: the last comment, else the timed ticket, else nothing. */
export function startingComment(timedTicket: string | null): string {
  return lastComment ?? timedTicket ?? "";
}

/** Tests only. */
export function forgetComment(): void {
  lastComment = null;
}
