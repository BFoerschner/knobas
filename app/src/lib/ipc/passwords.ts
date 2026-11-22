/**
 * The Passwords role (ADR-0056) — `crates/knobas-app/src/commands/passwords.rs`.
 *
 * The types are generated from the Rust (`./generated/passwords.ts`): a field is explained in its Rust doc comment
 * and nowhere else. A secret crosses as a plain `string` (`SecretText`), and only `password_show` sends one: a copy is
 * made on the Rust side and never reaches the window.
 */
import { invoke } from "./invoke";

import type { ClipState, CodeAnswer, Copied, Held, PwEntry, ReadAnswer, SecretRef, SourceFacts } from "./generated/passwords";

export type {
  Approval,
  AttachmentMeta,
  Attachments,
  ClipState,
  CodeAnswer,
  CommentRules,
  Copied,
  EntryRecord,
  FieldRecord,
  FolderRecord,
  Held,
  HistoryKind,
  Offers,
  OtpAlgorithm,
  OtpInfo,
  PathStep,
  PwEntry,
  PwFolder,
  PwRow,
  PwSource,
  ReadAnswer,
  SecretRef,
  SecretText,
  SourceFacts,
  Write,
  WriteKind,
} from "./generated/passwords";

/** Every password source, folder and entry: the page's one read. */
export function passwordsHeld(): Promise<Held> {
  return invoke<Held>("passwords_held");
}

/** The rest of one entry, `null` for one that is gone. */
export function passwordsEntry(entityId: string): Promise<PwEntry | null> {
  return invoke<PwEntry | null>("passwords_entry", { entityId });
}

/** Show a secret for 30 seconds: the one command that sends one to the window. */
export function passwordShow(entityId: string, which: SecretRef, comment: string | null): Promise<ReadAnswer> {
  return invoke<ReadAnswer>("password_show", { entityId, which, comment });
}

/** Copy a secret on the Rust side; it leaves the clipboard after 30 seconds if it is still there. */
export function passwordCopy(entityId: string, which: SecretRef, comment: string | null): Promise<ReadAnswer> {
  return invoke<ReadAnswer>("password_copy", { entityId, which, comment });
}

/** Hide one shown secret, or every one (no arguments): the page was left. */
export function passwordHide(entityId?: string, which?: SecretRef): Promise<void> {
  return invoke<void>("password_hide", { entityId: entityId ?? null, which: which ?? null });
}

/** The entry's one-time code for now, made on this machine. */
export function otpCode(entityId: string): Promise<CodeAnswer> {
  return invoke<CodeAnswer>("otp_code", { entityId });
}

/** Copy the entry's code for now. */
export function otpCopy(entityId: string): Promise<CodeAnswer> {
  return invoke<CodeAnswer>("otp_copy", { entityId });
}

/** Copy a value typed or generated in a dialog, as a secret. */
export function clipboardCopySecret(text: string, what: string): Promise<Copied> {
  return invoke<Copied>("clipboard_copy_secret", { text, what });
}

/** *Clear now*: take back what knobas put on the clipboard, if it is still there. */
export function clipboardClear(): Promise<boolean> {
  return invoke<boolean>("clipboard_clear");
}

/** The window copied something itself: knobas's deadline is dropped. */
export function clipboardForget(): Promise<void> {
  return invoke<void>("clipboard_forget");
}

/** What the clipboard holds for knobas now. */
export function clipboardState(): Promise<ClipState> {
  return invoke<ClipState>("clipboard_state");
}

/** What a password source's Settings page counts: entries, folders and the code-key formats the sync found. */
export function passwordSourceFacts(sourceId: string): Promise<SourceFacts> {
  return invoke<SourceFacts>("password_source_facts", { sourceId });
}
