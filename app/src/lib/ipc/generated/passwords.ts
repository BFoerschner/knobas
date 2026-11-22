// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).

/**
 * Whether a use of the entry waits for somebody's approval.
 */
export type Approval = { "state": "none" } | { "state": "required", approver: string | null, } | { "state": "unknown" };

/**
 * What opening one attachment answered (step 9a).
 */
export type AttachmentAnswer = { "kind": "read", name: string, size: number, 
/**
 * Base64.
 */
data: string, text: string | null, } | { "kind": "too_large", size: number, } | { "kind": "not_offered" } | { "kind": "unreachable", message: string, };

/**
 * One attachment, as a listing names it. Never its bytes.
 */
export type AttachmentMeta = { id: string, name: string, size: number | null, };

/**
 * What the mirror knows of an entry's attachments.
 */
export type Attachments = { "state": "none" } | { "state": "listed", "files": Array<AttachmentMeta> } | { "state": "unlisted" };

/**
 * What the clipboard holds for knobas now, for the status bar: what it is,
 * whose, and until when. `None` fields when it holds nothing of knobas's.
 */
export type ClipState = { 
/**
 * *password*, *one-time code*, or a field's name.
 */
what: string | null, 
/**
 * The entry it came from.
 */
entity_id: string | null, 
/**
 * When it leaves the clipboard, RFC 3339.
 */
until: string | null, };

/**
 * What a code ask answered.
 */
export type CodeAnswer = { "kind": "code", code: string, digits: number, period: number, left_ms: number, } | { "kind": "needs_sync" } | { "kind": "hidden" } | { "kind": "unreadable" } | { "kind": "not_offered" };

/**
 * What a usage comment is asked for, as the source says it for this entry
 * or folder. Pleasant's `AskForCommentOnViewPassword` and
 * `AskForCommentOnModifyEntries`.
 */
export type CommentRules = { 
/**
 * A read of the password, a protected field or an earlier password.
 */
read: boolean, 
/**
 * *Edit*, *Change password* and a new entry in that folder.
 */
modify: boolean, };

/**
 * Where a copy of a code or a typed secret stays.
 */
export type Copied = { until: string, };

/**
 * What `sync.item.payload` holds of an entry, and nothing else.
 *
 * A field added later needs `#[serde(default)]`, or an old mirror row stops
 * decoding where the engine checks it. The page's reads decode leniently.
 */
export type EntryRecord = { username: string | null, address: string | null, 
/**
 * The folder it sits in, as an entity id. `None` directly under the
 * source's root.
 */
folder: string | null, 
/**
 * The folders above it, outermost first: see [`PathStep`].
 */
path?: Array<PathStep>, 
/**
 * Further folders it also sits in: a Bitwarden entry in several
 * collections.
 */
also_in?: Array<string>, notes: string | null, 
/**
 * Custom fields in the source's order, the code-key fields removed.
 */
fields: Array<FieldRecord>, otp: OtpInfo | null, expires: string | null, changed_by: string | null, comment: CommentRules, approval: Approval, attachments: Attachments, can_read: boolean, can_modify: boolean, };

/**
 * One custom field. `value: None` is one read live (open question 6): a
 * field the source marks protected, or one its listing does not send.
 */
export type FieldRecord = { name: string, value: string | null, };

/**
 * What `sync.item.payload` holds of a folder.
 */
export type FolderRecord = { 
/**
 * Its name, as a parent answers to it ([`PathStep`]).
 */
name: string, 
/**
 * The folder it sits in, as an entity id; `None` at the source's top.
 */
parent: string | null, 
/**
 * The folders above it, outermost first.
 */
path?: Array<PathStep>, comment: CommentRules, can_add: boolean, };

/**
 * Everything the page draws from.
 */
export type Held = { sources: Array<PwSource>, folders: Array<PwFolder>, entries: Array<PwRow>, 
/**
 * The entries used last, newest first, at most four.
 */
recent: Array<string>, };

/**
 * What an entry's history is, where a source offers one.
 */
export type HistoryKind = "log" | "earlier";

/**
 * The eight abilities a password source may offer beyond the minimum (list
 * entries, read one secret): Björn's decision 1. Declared once, in
 * [`SourceDescriptor::passwords`](crate::SourceDescriptor::passwords); the
 * page draws a control only where the entry's source declares it (ADR-0020).
 */
export type Offers = { one_time_codes: boolean, expiry: boolean, usage_comments: boolean, 
/**
 * The source records each read under the account knobas signs in as.
 */
audit_log: boolean, history: HistoryKind | null, attachments: boolean, access_requests: boolean, 
/**
 * Whether the source marks which custom fields are protected. One that
 * does keeps a protected value out of its records (read live, drawn
 * covered); one that does not sends every value openly, and Settings
 * says so (Fable's ruling while Björn was away, 2026-11-22).
 */
marks_protected_fields: boolean, 
/**
 * The direct writes it performs.
 */
writes: Array<WriteKind>, };

/**
 * How a one-time code is made: the hash.
 */
export type OtpAlgorithm = "SHA1" | "SHA256" | "SHA512";

/**
 * That an entry has a code key and how its codes are made. **Never the key.**
 */
export type OtpInfo = { 
/**
 * Where the key was found: `keepass`, `tray_totp`, `keeotp`, `otpauth`,
 * or [`OTP_UNREADABLE`].
 */
format: string, digits: number, period: number, algorithm: OtpAlgorithm, };

/**
 * One folder of an entry's or a folder's path: its id and its name.
 * Outermost first, the source's single shared root left out. What the
 * kinds' declared `parent` reads, so the launcher's path line and `under:`
 * read *Payments / Prod / Databases* with no code of their own.
 */
export type PathStep = { id: string, name: string, };

/**
 * The rest of one entry.
 */
export type PwEntry = { id: string, source: string, title: string, username: string | null, address: string | null, folder: string | null, notes: string | null, fields: Array<FieldRecord>, otp: OtpInfo | null, expires: string | null, changed: string | null, changed_by: string | null, comment: CommentRules, approval: Approval, attachments: Attachments, can_read: boolean, can_modify: boolean, };

/**
 * One folder.
 */
export type PwFolder = { id: string, 
/**
 * The index of its source in [`Held::sources`].
 */
source: number, 
/**
 * Its parent folder, `None` at its source's top.
 */
parent: string | null, name: string, comment_read: boolean, can_add: boolean, };

/**
 * One entry's row: `[id, source index, folder id, title, username, address,
 * changed (RFC 3339), flags, expires (a date), also in]`. An array, so 2,000
 * rows stay small on the bridge.
 */
export type PwRow = [string, number, string | null, string, string | null, string | null, string | null, number, string | null, Array<string>];

/**
 * One password source, as the page names it.
 */
export type PwSource = { id: string, name: string, 
/**
 * The account it signs in as: the name the source's log keeps reads under.
 */
account: string | null, by_hand: boolean, 
/**
 * When its last run that succeeded ended, RFC 3339.
 */
synced_at: string | null, 
/**
 * When it last failed to answer: the last run ended unreachable, or a
 * live read just did. `None` once a call to it succeeds.
 */
down_at: string | null, 
/**
 * It has never finished a run.
 */
never_synced: boolean, offers: Offers, 
/**
 * The folder every other one of its sat under, left out.
 */
skipped_root: string | null, 
/**
 * Its kind's display name, for Settings and nowhere on the page.
 */
adapter_name: string, };

/**
 * What a show or a copy answered.
 */
export type ReadAnswer = { "kind": "shown", value: string, read_at: string, until: string, } | { "kind": "copied", read_at: string, until: string, } | { "kind": "comment_required", scope: Array<string>, } | { "kind": "approval_required", approver: string | null, } | { "kind": "not_offered" } | { "kind": "refused", reason: string, } | { "kind": "unreachable", message: string, };

/**
 * Which secret of an entry is asked for.
 */
export type SecretRef = { "kind": "password" } | { "kind": "field", name: string, } | { "kind": "earlier", id: string, };

/**
 * A value read live and never mirrored: a password, a protected field, an
 * earlier password, a code key.
 *
 * Its `Debug` prints `<redacted>`, and it zeroes its bytes when dropped. It
 * serializes as a plain string because a shown password has to cross to the
 * window, and it is exported to TypeScript as `string`. Nothing outside
 * `knobas-app`'s `passwords` module and the adapters names it (a house rule
 * there holds it).
 */
export type SecretText = string;

/**
 * What a password source's Settings page counts (ADR-0056): its entries
 * and folders, and how many entries keep a code key in each format the
 * sync found (`payload->'otp'->>'format'`), `unreadable` included.
 */
export type SourceFacts = { entries: number, folders: number, 
/**
 * Format id and how many entries have it, most first.
 */
formats: Array<[string, number]>, };

/**
 * A change to a password source, sent the moment it is made and never
 * stored (ADR-0056). Not a [`WriteOp`](crate::WriteOp): the write queue's
 * column is one, so it cannot hold this.
 *
 * No derived `Debug`: the hand-written one prints the variant and the
 * entity, never a value.
 *
 * It cannot be queued: the write queue holds [`WriteOp`](crate::WriteOp)s,
 * and nothing turns one of these into one.
 *
 * ```compile_fail
 * use knobas_source::passwords::{SecretText, Write};
 * let write = Write::SetPassword {
 *     entity: "pw:e1".into(),
 *     password: SecretText::new("hunter2"),
 *     comment: None,
 *     listed_in: None,
 * };
 * let queued: knobas_source::WriteOp = write.into();
 * ```
 */
export type Write = { "write": "create_entry", source: string, folder: string | null, title: string, username: string | null, address: string | null, notes: string | null, expires: string | null, password: SecretText, otp_key: SecretText | null, comment: string | null, } | { "write": "edit_entry", entity: string, title?: string, username?: string | null, address?: string | null, notes?: string | null, expires?: string | null, folder?: string | null, otp_key?: SecretText, comment: string | null, 
/**
 * The folder the mirror lists it in, an entity id: where a source
 * that reads an entry back by listing its folder looks.
 */
listed_in: string | null, } | { "write": "set_password", entity: string, password: SecretText, comment: string | null, 
/**
 * As [`Write::EditEntry`]'s.
 */
listed_in: string | null, } | { "write": "request_access", entity: string, comment: string, };

/**
 * What a direct write answered.
 */
export type WriteAnswer = { "kind": "written", entity_id: string, } | { "kind": "unreachable", message: string, } | { "kind": "unconfirmed", message: string, };

/**
 * A direct write a source performs.
 */
export type WriteKind = "create_entry" | "edit_entry" | "set_password" | "request_access";
