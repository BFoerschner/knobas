//! The Passwords role (ADR-0056): a password manager's entries and folders,
//! the one live read that answers a secret, and the direct writes.
//!
//! **A secret never rests.** A password, a protected field's value, an
//! attachment's bytes and an earlier password are read live
//! ([`Source::read_secret`](crate::Source::read_secret)), carried as a
//! [`SecretText`], and written to no table, no file, no log and no cache. A
//! one-time code key leaves an adapter only through
//! [`Sink::otp_key`](crate::Sink::otp_key), and the host keeps it in the OS
//! keychain.
//!
//! **The role fixes the record.** Every other role reads a source's own
//! payload through declared paths. This one fixes the payload's shape,
//! [`EntryRecord`] and [`FolderRecord`], because the payload is where a
//! secret would leak: a raw Pleasant entry carries its code keys in its custom
//! fields and, on some calls, the password. The engine refuses an item of
//! either kind whose payload does not decode as its record
//! ([`check_payload`]), so an adapter that puts anything beside the known
//! keys writes nothing to the mirror.
//!
//! **A write is direct.** [`Write`] is its own enum and
//! [`Source::passwords_write`](crate::Source::passwords_write) its own method,
//! so the write queue, whose rows are [`WriteOp`](crate::WriteOp)s in a table
//! that is in the backup, cannot hold one.
//!
//! **Frozen once landed**, as every role is (§10.8).

use serde::{Deserialize, Serialize};

use crate::{KindInfo, SyncItem};
use knobas_spi_types::entity::EntityRef;
use knobas_spi_types::payload::{KindPaths, ParentPath, PayloadPath};

/// The role's id, as a claim names it.
pub const PASSWORDS: &str = "passwords";
/// One record of a password manager. Never "credential", "login" or "item".
pub const ENTRY_KIND: &str = "password_entry";
/// A place in a password source's tree. A Bitwarden collection is one too.
pub const FOLDER_KIND: &str = "password_folder";

/// The optional read that answers an entry's earlier passwords or its log.
pub const ENTRY_HISTORY: &str = "entry_history";
/// The optional read that lists (and reads) an entry's attachments.
pub const ENTRY_ATTACHMENTS: &str = "entry_attachments";
/// The optional read that answers where an access request stands.
pub const ACCESS_STATE: &str = "access_state";
/// The read every password source answers: [`crate::Source::read_secret`].
pub const READ_SECRET: &str = "read_secret";

/// How long a shown secret stays shown, and a copied one stays on the
/// clipboard, in seconds. Mirrored in the window's `tf/passwords/hold.ts`.
pub const HOLD_SECS: u64 = 30;

// ---- The secret -------------------------------------------------------------

/// A value read live and never mirrored: a password, a protected field, an
/// earlier password, a code key.
///
/// Its `Debug` prints `<redacted>`, and it zeroes its bytes when dropped. It
/// serializes as a plain string because a shown password has to cross to the
/// window, and it is exported to TypeScript as `string`. Nothing outside
/// `knobas-app`'s `passwords` module and the adapters names it (a house rule
/// there holds it).
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(transparent)]
#[ts(export_to = "passwords.ts", type = "string")]
pub struct SecretText(String);

impl SecretText {
    /// Wrap a value read from a source or typed in a dialog.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value, for the one place that has to use it: a request body, the
    /// clipboard, the code generator, the window.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

// ---- The records ------------------------------------------------------------

/// What a usage comment is asked for, as the source says it for this entry
/// or folder. Pleasant's `AskForCommentOnViewPassword` and
/// `AskForCommentOnModifyEntries`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "passwords.ts")]
pub struct CommentRules {
    /// A read of the password, a protected field or an earlier password.
    pub read: bool,
    /// *Edit*, *Change password* and a new entry in that folder.
    pub modify: bool,
}

/// Whether a use of the entry waits for somebody's approval.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
#[ts(export_to = "passwords.ts")]
pub enum Approval {
    /// No approval is needed, as far as the listing says.
    #[default]
    None,
    /// Each use waits for `approver`'s approval.
    Required { approver: Option<String> },
    /// The source cannot say until a read is tried.
    Unknown,
}

/// One attachment, as a listing names it. Never its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "passwords.ts")]
pub struct AttachmentMeta {
    pub id: String,
    pub name: String,
    #[ts(type = "number | null")]
    pub size: Option<u64>,
}

/// What the mirror knows of an entry's attachments.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(
    tag = "state",
    content = "files",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[ts(export_to = "passwords.ts")]
pub enum Attachments {
    /// None, or the source offers none.
    #[default]
    None,
    /// The listing named them.
    Listed(Vec<AttachmentMeta>),
    /// The source cannot list them without reading them: drawn as a closed
    /// fold, read when it is opened.
    Unlisted,
}

/// How a one-time code is made: the hash.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "passwords.ts")]
pub enum OtpAlgorithm {
    #[default]
    #[serde(rename = "SHA1")]
    Sha1,
    #[serde(rename = "SHA256")]
    Sha256,
    #[serde(rename = "SHA512")]
    Sha512,
}

impl OtpAlgorithm {
    /// The spelling an `otpauth://` URI and KeePass use.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "SHA1",
            Self::Sha256 => "SHA256",
            Self::Sha512 => "SHA512",
        }
    }

    /// Read `SHA1`, `sha-256`, `HMAC-SHA-512` and the like; `None` for
    /// anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let bare: String = text
            .to_ascii_uppercase()
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect();
        match bare.trim_start_matches("HMAC") {
            "SHA1" => Some(Self::Sha1),
            "SHA256" => Some(Self::Sha256),
            "SHA512" => Some(Self::Sha512),
            _ => None,
        }
    }
}

/// The format id of a code key knobas could not read: it is removed from
/// the record all the same, and no key is handed over.
pub const OTP_UNREADABLE: &str = "unreadable";

/// That an entry has a code key and how its codes are made. **Never the key.**
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "passwords.ts")]
pub struct OtpInfo {
    /// Where the key was found: `keepass`, `tray_totp`, `keeotp`, `otpauth`,
    /// or [`OTP_UNREADABLE`].
    pub format: String,
    pub digits: u8,
    pub period: u16,
    pub algorithm: OtpAlgorithm,
}

/// One custom field. `value: None` is one read live (open question 6): a
/// field the source marks protected, or one its listing does not send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "passwords.ts")]
pub struct FieldRecord {
    pub name: String,
    pub value: Option<String>,
}

/// One folder of an entry's or a folder's path: its id and its name.
/// Outermost first, the source's single shared root left out. What the
/// kinds' declared `parent` reads, so the launcher's path line and `under:`
/// read *Payments / Prod / Databases* with no code of their own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "passwords.ts")]
pub struct PathStep {
    pub id: String,
    pub name: String,
}

/// What `sync.item.payload` holds of an entry, and nothing else.
///
/// A field added later needs `#[serde(default)]`, or an old mirror row stops
/// decoding where the engine checks it. The page's reads decode leniently.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "passwords.ts")]
pub struct EntryRecord {
    pub username: Option<String>,
    pub address: Option<String>,
    /// The folder it sits in, as an entity id. `None` directly under the
    /// source's root.
    pub folder: Option<String>,
    /// The folders above it, outermost first: see [`PathStep`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<PathStep>,
    /// Further folders it also sits in: a Bitwarden entry in several
    /// collections.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also_in: Vec<String>,
    pub notes: Option<String>,
    /// Custom fields in the source's order, the code-key fields removed.
    #[serde(default)]
    pub fields: Vec<FieldRecord>,
    pub otp: Option<OtpInfo>,
    #[ts(type = "string | null")]
    pub expires: Option<chrono::NaiveDate>,
    pub changed_by: Option<String>,
    #[serde(default)]
    pub comment: CommentRules,
    #[serde(default)]
    pub approval: Approval,
    #[serde(default)]
    pub attachments: Attachments,
    pub can_read: bool,
    pub can_modify: bool,
}

/// What `sync.item.payload` holds of a folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "passwords.ts")]
pub struct FolderRecord {
    /// Its name, as a parent answers to it ([`PathStep`]).
    pub name: String,
    /// The folder it sits in, as an entity id; `None` at the source's top.
    pub parent: Option<String>,
    /// The folders above it, outermost first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<PathStep>,
    #[serde(default)]
    pub comment: CommentRules,
    pub can_add: bool,
}

/// Hold an item's payload to its kind's record, as the engine does at the
/// sink's item write. `Ok` for any other kind.
///
/// # Errors
///
/// The decode's message, for a payload with an unknown key, a missing one or
/// a wrong type.
pub fn check_payload(kind: &str, payload: &serde_json::Value) -> Result<(), String> {
    match kind {
        ENTRY_KIND => serde_json::from_value::<EntryRecord>(payload.clone())
            .map(drop)
            .map_err(|e| e.to_string()),
        FOLDER_KIND => serde_json::from_value::<FolderRecord>(payload.clone())
            .map(drop)
            .map_err(|e| e.to_string()),
        _ => Ok(()),
    }
}

/// Whether `kind` is one of the role's two.
#[must_use]
pub fn is_password_kind(kind: &str) -> bool {
    kind == ENTRY_KIND || kind == FOLDER_KIND
}

// ---- Building items ---------------------------------------------------------

/// What [`entry_item`] needs besides the record.
#[derive(Debug, Clone)]
pub struct EntryHead {
    /// The adapter instance id, the entity namespace.
    pub source: String,
    /// The source's own id for the entry.
    pub key: String,
    pub title: String,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// What an entry is found by in the launcher and Everything search, beside
/// its title: the username, the address, the notes, and each custom field
/// whose value the listing sent (Björn's answers 1 and 2 of 2026-11-22).
/// Never a protected field, never a code key: neither is in the record.
#[must_use]
pub fn entry_body_text(record: &EntryRecord) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.extend(record.username.iter().cloned());
    parts.extend(record.address.iter().cloned());
    parts.extend(record.notes.iter().cloned());
    for field in &record.fields {
        if let Some(value) = &field.value {
            parts.push(format!("{}: {value}", field.name));
        }
    }
    parts.retain(|p| !p.trim().is_empty());
    parts.join("\n")
}

/// An entry as the sync emits it. The one way an adapter spells an entry's
/// payload, so it cannot put anything beside the record.
#[must_use]
pub fn entry_item(head: EntryHead, record: &EntryRecord) -> SyncItem {
    SyncItem {
        entity: EntityRef {
            namespace: head.source,
            key: head.key,
        },
        kind: ENTRY_KIND.to_owned(),
        title: head.title,
        body_text: entry_body_text(record),
        author: None,
        updated_at: head.updated_at,
        payload: serde_json::to_value(record).unwrap_or(serde_json::Value::Null),
        // The page names no vendor and has no "open in".
        web_url: None,
        deleted: false,
    }
}

/// A folder as the sync emits it. Its body text is empty: a folder is found
/// by its title.
#[must_use]
pub fn folder_item(source: &str, key: &str, record: &FolderRecord) -> SyncItem {
    SyncItem {
        entity: EntityRef {
            namespace: source.to_owned(),
            key: key.to_owned(),
        },
        kind: FOLDER_KIND.to_owned(),
        title: record.name.clone(),
        body_text: String::new(),
        author: None,
        updated_at: None,
        payload: serde_json::to_value(record).unwrap_or(serde_json::Value::Null),
        web_url: None,
        deleted: false,
    }
}

/// The two kinds' [`KindInfo`], for a password adapter's descriptor.
/// `full_sync_exhaustive` is the adapter's to say; both keep their titles in
/// the mirror ([`KindInfo::title_stays_in_mirror`]).
#[must_use]
pub fn kind_infos(full_sync_exhaustive: bool) -> Vec<KindInfo> {
    vec![
        KindInfo {
            id: ENTRY_KIND.to_owned(),
            label: "Password entry".to_owned(),
            plural: "Password entries".to_owned(),
            monogram: "PW".to_owned(),
            full_sync_exhaustive,
            refreshes: false,
            title_stays_in_mirror: true,
        },
        KindInfo {
            id: FOLDER_KIND.to_owned(),
            label: "Password folder".to_owned(),
            plural: "Password folders".to_owned(),
            monogram: "PF".to_owned(),
            full_sync_exhaustive,
            refreshes: false,
            title_stays_in_mirror: true,
        },
    ]
}

/// The two kinds' declared paths: each names its parent folders at `path`,
/// by `name`, so `sync.item_parent` holds the tree as it holds Docs'.
#[must_use]
pub fn payload_paths() -> Vec<KindPaths> {
    let parent = || {
        vec![ParentPath {
            kind: FOLDER_KIND.to_owned(),
            at: PayloadPath::of(["path"]),
            names: vec![PayloadPath::of(["name"])],
        }]
    };
    vec![
        KindPaths {
            kind: ENTRY_KIND.to_owned(),
            parent: parent(),
            ..KindPaths::default()
        },
        KindPaths {
            kind: FOLDER_KIND.to_owned(),
            parent: parent(),
            ..KindPaths::default()
        },
    ]
}

// ---- What a source offers ---------------------------------------------------

/// What an entry's history is, where a source offers one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "passwords.ts")]
pub enum HistoryKind {
    /// Who read or changed it, when: the source's log of the entry.
    Log,
    /// Its earlier passwords, each read live.
    Earlier,
}

/// A direct write a source performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "passwords.ts")]
pub enum WriteKind {
    CreateEntry,
    EditEntry,
    SetPassword,
    RequestAccess,
}

/// The eight abilities a password source may offer beyond the minimum (list
/// entries, read one secret): Björn's decision 1. Declared once, in
/// [`SourceDescriptor::passwords`](crate::SourceDescriptor::passwords); the
/// page draws a control only where the entry's source declares it (ADR-0020).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(deny_unknown_fields)]
#[ts(export_to = "passwords.ts")]
pub struct Offers {
    pub one_time_codes: bool,
    pub expiry: bool,
    pub usage_comments: bool,
    /// The source records each read under the account knobas signs in as.
    pub audit_log: bool,
    pub history: Option<HistoryKind>,
    pub attachments: bool,
    pub access_requests: bool,
    /// The direct writes it performs.
    #[serde(default)]
    pub writes: Vec<WriteKind>,
}

impl Offers {
    /// Whether it performs `kind`.
    #[must_use]
    pub fn writes(&self, kind: WriteKind) -> bool {
        self.writes.contains(&kind)
    }
}

// ---- The live read ----------------------------------------------------------

/// Which secret of an entry is asked for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "passwords.ts")]
pub enum SecretRef {
    Password,
    /// A custom field the mirror holds no value for.
    Field {
        name: String,
    },
    /// An earlier password, by the id the history named.
    Earlier {
        id: String,
    },
}

/// One ask for a secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretAsk {
    /// The entry's entity id.
    pub entity: String,
    pub which: SecretRef,
    /// The usage comment, where one was asked for and given.
    pub comment: Option<String>,
    /// The entry's folder as the mirror holds it, an entity id: where a
    /// source that reads a field by listing the entry's folder (Pleasant)
    /// looks. `None` directly under the source's root, or when unknown.
    #[serde(default)]
    pub folder: Option<String>,
}

/// What a secret read answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretAnswer {
    Value(SecretText),
    /// The source wants a usage comment for this read. `scope` is the folder
    /// that asks, as a path, outermost first; empty when it does not say.
    CommentRequired {
        scope: Vec<String>,
    },
    ApprovalRequired {
        approver: Option<String>,
    },
    /// This source answers no such read, or the entity is no entry.
    NotOffered,
}

/// A one-time code key, as the sync hands it to the sink. **Never mirrored.**
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtpKey {
    /// The key in Base32, upper case, no padding.
    pub secret: SecretText,
    pub digits: u8,
    pub period: u16,
    pub algorithm: OtpAlgorithm,
}

// ---- Direct writes ----------------------------------------------------------

/// A change to a password source, sent the moment it is made and never
/// stored (ADR-0056). Not a [`WriteOp`](crate::WriteOp): the write queue's
/// column is one, so it cannot hold this.
///
/// No derived `Debug`: the hand-written one prints the variant and the
/// entity, never a value.
#[derive(Clone, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "write", rename_all = "snake_case")]
#[ts(export_to = "passwords.ts")]
pub enum Write {
    CreateEntry {
        source: String,
        folder: Option<String>,
        title: String,
        username: Option<String>,
        address: Option<String>,
        notes: Option<String>,
        #[ts(type = "string | null")]
        expires: Option<chrono::NaiveDate>,
        password: SecretText,
        otp_key: Option<SecretText>,
        comment: Option<String>,
    },
    /// Only what changed. `folder` moves it within its source.
    EditEntry {
        entity: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "string | null")]
        username: Option<Option<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "string | null")]
        address: Option<Option<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "string | null")]
        notes: Option<Option<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "string | null")]
        expires: Option<Option<chrono::NaiveDate>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "string | null")]
        folder: Option<Option<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        otp_key: Option<SecretText>,
        comment: Option<String>,
    },
    SetPassword {
        entity: String,
        password: SecretText,
        comment: Option<String>,
    },
    RequestAccess {
        entity: String,
        comment: String,
    },
}

impl Write {
    /// Which kind of write this is.
    #[must_use]
    pub fn kind(&self) -> WriteKind {
        match self {
            Write::CreateEntry { .. } => WriteKind::CreateEntry,
            Write::EditEntry { .. } => WriteKind::EditEntry,
            Write::SetPassword { .. } => WriteKind::SetPassword,
            Write::RequestAccess { .. } => WriteKind::RequestAccess,
        }
    }

    /// The entry it changes, or the source a new one goes to.
    #[must_use]
    pub fn target(&self) -> &str {
        match self {
            Write::CreateEntry { source, .. } => source,
            Write::EditEntry { entity, .. }
            | Write::SetPassword { entity, .. }
            | Write::RequestAccess { entity, .. } => entity,
        }
    }

    /// The usage comment it carries.
    #[must_use]
    pub fn comment(&self) -> Option<&str> {
        match self {
            Write::CreateEntry { comment, .. }
            | Write::EditEntry { comment, .. }
            | Write::SetPassword { comment, .. } => comment.as_deref(),
            Write::RequestAccess { comment, .. } => Some(comment),
        }
    }
}

impl std::fmt::Debug for Write {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}({})", self.kind(), self.target())
    }
}

/// What a direct write answers once the source has: the entry as a sync
/// would emit it, and a code key the write set.
#[derive(Debug, Clone, Default)]
pub struct Written {
    pub item: Option<SyncItem>,
    pub otp_key: Option<OtpKey>,
}

/// The refusal of a write the source does not declare: nothing was sent.
#[must_use]
pub fn undeclared(source: &str, kind: WriteKind) -> crate::SourceError {
    crate::SourceError::protocol(format!(
        "source {source:?} does not perform the password write {kind:?}, so nothing was sent"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn an_entry() -> EntryRecord {
        EntryRecord {
            username: Some("payout_svc".into()),
            address: Some("pg-pay-01.tidewater.internal:5432".into()),
            folder: Some("pleasant:f3".into()),
            path: vec![
                PathStep {
                    id: "pleasant:f1".into(),
                    name: "Payments".into(),
                },
                PathStep {
                    id: "pleasant:f3".into(),
                    name: "Databases".into(),
                },
            ],
            notes: Some("Rotated quarterly.".into()),
            fields: vec![
                FieldRecord {
                    name: "Port".into(),
                    value: Some("5432".into()),
                },
                FieldRecord {
                    name: "Recovery PIN".into(),
                    value: None,
                },
            ],
            otp: Some(OtpInfo {
                format: "keepass".into(),
                digits: 6,
                period: 30,
                algorithm: OtpAlgorithm::Sha1,
            }),
            expires: chrono::NaiveDate::from_ymd_opt(2027, 1, 31),
            changed_by: Some("Priya Nair".into()),
            comment: CommentRules {
                read: true,
                modify: false,
            },
            approval: Approval::Required {
                approver: Some("Priya Nair".into()),
            },
            attachments: Attachments::Listed(vec![AttachmentMeta {
                id: "a1".into(),
                name: "ca.pem".into(),
                size: Some(1200),
            }]),
            can_read: true,
            can_modify: false,
            ..EntryRecord::default()
        }
    }

    #[test]
    fn a_secret_prints_nothing_of_its_value() {
        let secret = SecretText::new("hunter2-very-secret");
        let printed = format!("{secret:?} {:?}", Some(&secret));
        assert!(!printed.contains("hunter"), "{printed}");
        assert!(printed.contains("<redacted>"));
        // It still crosses as a plain string, to be shown.
        assert_eq!(
            serde_json::to_value(&secret).unwrap(),
            serde_json::json!("hunter2-very-secret")
        );
    }

    #[test]
    fn a_write_prints_its_kind_and_target_and_no_value() {
        let write = Write::SetPassword {
            entity: "pleasant:e1".into(),
            password: SecretText::new("s3cr3t-pass"),
            comment: Some("PAY-231".into()),
        };
        let printed = format!("{write:?}");
        assert_eq!(printed, "SetPassword(pleasant:e1)");
    }

    #[test]
    fn each_record_round_trips_and_an_unknown_key_fails_to_decode() {
        let entry = an_entry();
        let wire = serde_json::to_value(&entry).unwrap();
        assert_eq!(
            serde_json::from_value::<EntryRecord>(wire.clone()).unwrap(),
            entry
        );
        assert!(check_payload(ENTRY_KIND, &wire).is_ok());

        let mut leaked = wire.clone();
        leaked["Password"] = serde_json::json!("hunter2");
        let refused = check_payload(ENTRY_KIND, &leaked).unwrap_err();
        assert!(refused.contains("Password"), "{refused}");

        let folder = FolderRecord {
            name: "Databases".into(),
            parent: Some("pleasant:f1".into()),
            path: vec![PathStep {
                id: "pleasant:f1".into(),
                name: "Payments".into(),
            }],
            comment: CommentRules::default(),
            can_add: true,
        };
        let wire = serde_json::to_value(&folder).unwrap();
        assert_eq!(
            serde_json::from_value::<FolderRecord>(wire.clone()).unwrap(),
            folder
        );
        let mut leaked = wire;
        leaked["CustomUserFields"] = serde_json::json!({});
        assert!(check_payload(FOLDER_KIND, &leaked).is_err());
        // Another kind is not this module's to check.
        assert!(check_payload("ticket", &serde_json::json!({"x": 1})).is_ok());
    }

    #[test]
    fn an_entry_is_found_by_its_open_text_and_never_by_a_hidden_field() {
        let item = entry_item(
            EntryHead {
                source: "pleasant".into(),
                key: "e1".into(),
                title: "payout-service database user".into(),
                updated_at: None,
            },
            &an_entry(),
        );
        assert_eq!(item.entity.to_string(), "pleasant:e1");
        assert_eq!(item.kind, ENTRY_KIND);
        assert!(item.web_url.is_none());
        assert!(item.body_text.contains("payout_svc"));
        assert!(item.body_text.contains("pg-pay-01"));
        assert!(item.body_text.contains("Rotated quarterly."));
        assert!(item.body_text.contains("Port: 5432"));
        assert!(
            !item.body_text.contains("Recovery PIN"),
            "a field read live is not searched by name: {}",
            item.body_text
        );
        assert!(check_payload(&item.kind, &item.payload).is_ok());
    }

    #[test]
    fn an_algorithm_is_read_in_its_common_spellings() {
        assert_eq!(OtpAlgorithm::parse("SHA1"), Some(OtpAlgorithm::Sha1));
        assert_eq!(OtpAlgorithm::parse("sha-256"), Some(OtpAlgorithm::Sha256));
        assert_eq!(
            OtpAlgorithm::parse("HMAC-SHA-512"),
            Some(OtpAlgorithm::Sha512)
        );
        assert_eq!(OtpAlgorithm::parse("MD5"), None);
        assert_eq!(
            serde_json::to_value(OtpAlgorithm::Sha256).unwrap(),
            serde_json::json!("SHA256")
        );
    }

    #[test]
    fn an_algorithm_reads_back_its_own_spelling() {
        for algorithm in [
            OtpAlgorithm::Sha1,
            OtpAlgorithm::Sha256,
            OtpAlgorithm::Sha512,
        ] {
            assert_eq!(OtpAlgorithm::parse(algorithm.as_str()), Some(algorithm));
            assert_eq!(
                serde_json::to_value(algorithm).unwrap(),
                serde_json::json!(algorithm.as_str())
            );
        }
    }

    #[test]
    fn the_two_kinds_are_the_roles_and_name_their_folders_at_path() {
        assert!(is_password_kind(ENTRY_KIND) && is_password_kind(FOLDER_KIND));
        assert!(!is_password_kind("ticket") && !is_password_kind(""));
        let paths = payload_paths();
        assert_eq!(
            paths.iter().map(|p| p.kind.as_str()).collect::<Vec<_>>(),
            [ENTRY_KIND, FOLDER_KIND]
        );
        for p in &paths {
            assert_eq!(p.parent.len(), 1);
            assert_eq!(p.parent[0].kind, FOLDER_KIND);
            assert_eq!(p.parent[0].at, PayloadPath::of(["path"]));
            assert_eq!(p.parent[0].names, vec![PayloadPath::of(["name"])]);
        }
    }

    #[test]
    fn a_write_names_its_comment_its_target_and_its_kind() {
        let set = Write::SetPassword {
            entity: "pw:e1".into(),
            password: SecretText::new("x"),
            comment: Some("PAY-231".into()),
        };
        assert_eq!(set.comment(), Some("PAY-231"));
        assert_eq!(set.target(), "pw:e1");
        assert_eq!(set.kind(), WriteKind::SetPassword);
        let ask = Write::RequestAccess {
            entity: "pw:e2".into(),
            comment: "deploy".into(),
        };
        assert_eq!(ask.comment(), Some("deploy"));
        assert_eq!(ask.kind(), WriteKind::RequestAccess);
        let create = Write::CreateEntry {
            source: "pw".into(),
            folder: None,
            title: "t".into(),
            username: None,
            address: None,
            notes: None,
            expires: None,
            password: SecretText::new("x"),
            otp_key: None,
            comment: None,
        };
        assert_eq!(create.comment(), None);
        assert_eq!(create.target(), "pw");
        assert_eq!(create.kind(), WriteKind::CreateEntry);
    }
}
