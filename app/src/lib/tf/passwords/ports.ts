/**
 * The Passwords page's reads and actions (ADR-0056), injectable so a test needs no bridge. The real ones are the
 * `passwords` barrel's commands; a plain copy (a username, an address) is the window's own, which then tells the Rust
 * side its deadline is moot.
 */
import {
  clipboardForget,
  otpCode,
  otpCopy,
  passwordCopy,
  passwordHide,
  passwordShow,
  passwordsEntry,
  passwordsHeld,
  type CodeAnswer,
  type Held,
  type PwEntry,
  type ReadAnswer,
  type SecretRef,
} from "../../ipc/passwords";
import { openWebPort } from "../../shell/open-web";

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
