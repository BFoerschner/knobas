/**
 * The Passwords role (ADR-0056) — `crates/knobas-app/src/commands/passwords.rs`.
 *
 * The types are generated from the Rust (`./generated/passwords.ts`): a field is explained in its Rust doc comment
 * and nowhere else. A secret crosses as a plain `string` (`SecretText`), and only `password_show` sends one.
 */
export type {
  Approval,
  AttachmentMeta,
  Attachments,
  CommentRules,
  EntryRecord,
  FieldRecord,
  FolderRecord,
  HistoryKind,
  Offers,
  OtpAlgorithm,
  OtpInfo,
  PathStep,
  SecretRef,
  SecretText,
  Write,
  WriteKind,
} from "./generated/passwords";
