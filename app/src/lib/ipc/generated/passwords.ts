// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).

/**
 * Whether a use of the entry waits for somebody's approval.
 */
export type Approval = { "state": "none" } | { "state": "required", approver: string | null, } | { "state": "unknown" };

/**
 * One attachment, as a listing names it. Never its bytes.
 */
export type AttachmentMeta = { id: string, name: string, size: number | null, };

/**
 * What the mirror knows of an entry's attachments.
 */
export type Attachments = { "state": "none" } | { "state": "listed", "files": Array<AttachmentMeta> } | { "state": "unlisted" };

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
 * A change to a password source, sent the moment it is made and never
 * stored (ADR-0056). Not a [`WriteOp`](crate::WriteOp): the write queue's
 * column is one, so it cannot hold this.
 *
 * No derived `Debug`: the hand-written one prints the variant and the
 * entity, never a value.
 */
export type Write = { "write": "create_entry", source: string, folder: string | null, title: string, username: string | null, address: string | null, notes: string | null, expires: string | null, password: SecretText, otp_key: SecretText | null, comment: string | null, } | { "write": "edit_entry", entity: string, title?: string, username?: string | null, address?: string | null, notes?: string | null, expires?: string | null, folder?: string | null, otp_key?: SecretText, comment: string | null, } | { "write": "set_password", entity: string, password: SecretText, comment: string | null, } | { "write": "request_access", entity: string, comment: string, };

/**
 * A direct write a source performs.
 */
export type WriteKind = "create_entry" | "edit_entry" | "set_password" | "request_access";
