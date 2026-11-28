//! The Passwords page's commands (ADR-0056).
//!
//! `password_show` is the only command that sends a secret to the window.
//! A copy is made here, on the Rust side, and the value never crosses. Every
//! secret read goes through [`read`], which is the only caller of
//! `Source::read_secret` in the app: one press, at most one request.

use std::time::SystemTime;

use knobas_source::SourceError;
use knobas_source::passwords::{self, SecretAnswer, SecretAsk, SecretRef, SecretText};
use tauri::Manager;

use crate::IpcError;
use crate::passwords::reads::{self, Held, PwEntry, SourceFacts};
use crate::passwords::{PasswordsState, clipboard::ClipState};

/// What a show or a copy answered.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "passwords.ts")]
pub enum ReadAnswer {
    /// The value, for the window to draw until `until`. Show only.
    Shown {
        #[ts(type = "string")]
        value: SecretText,
        read_at: String,
        until: String,
    },
    /// It is on the clipboard until `until`.
    Copied {
        read_at: String,
        until: String,
    },
    /// The source asks for a usage comment; `scope` is the folder that asks,
    /// as a path, when it says.
    CommentRequired {
        scope: Vec<String>,
    },
    ApprovalRequired {
        approver: Option<String>,
    },
    /// No such secret, or this source reads none.
    NotOffered,
    /// The source refused this read for this account, in its words.
    Refused {
        reason: String,
    },
    /// The source did not answer.
    Unreachable {
        message: String,
    },
}

/// What a code ask answered.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "passwords.ts")]
pub enum CodeAnswer {
    Code {
        code: String,
        digits: u8,
        period: u16,
        #[ts(type = "number")]
        left_ms: u64,
    },
    /// The entry has a code key and this machine has not kept it yet.
    NeedsSync,
    /// Behind an access request that is not approved.
    Hidden,
    /// Its key is in a form knobas cannot read.
    Unreadable,
    /// The entry has no one-time code.
    NotOffered,
}

/// Where a copy of a code or a typed secret stays.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "passwords.ts")]
pub struct Copied {
    pub until: String,
}

fn state<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<tauri::State<'_, PasswordsState>, IpcError> {
    app.try_state::<PasswordsState>()
        .ok_or_else(|| IpcError::not_ready("the passwords state is not up yet".to_owned()))
}

fn stamp(at: SystemTime) -> String {
    chrono::DateTime::<chrono::Utc>::from(at).to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn now() -> String {
    stamp(SystemTime::now())
}

/// The words for a secret on the status bar's clipboard line.
fn what(which: &SecretRef) -> String {
    match which {
        SecretRef::Password => "password".to_owned(),
        SecretRef::Field { name } => name.clone(),
        SecretRef::Earlier { .. } => "earlier password".to_owned(),
    }
}

/// What a live read came back with, before the window's answer is made.
enum Read {
    Value(SecretText),
    Ask(ReadAnswer),
}

/// The one live read of a secret: through the source's adapter, once. A
/// source that does not answer is marked down until a call succeeds.
async fn read<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    entry: &PwEntry,
    which: &SecretRef,
    comment: Option<String>,
) -> Result<Read, IpcError> {
    let sources = crate::sources::state(app)?;
    let pw = state(app)?;
    let lent = sources
        .live
        .source(
            &sources.pool,
            &sources.secrets,
            &sources.registry,
            &entry.id,
        )
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&entry.source)))?;
    let ask = SecretAsk {
        entity: entry.id.clone(),
        which: which.clone(),
        comment,
        folder: entry.folder.clone(),
    };
    match lent.source.read_secret(&ask).await {
        Ok(answer) => {
            pw.mark_up(&entry.source);
            Ok(match answer {
                SecretAnswer::Value(value) => Read::Value(value),
                SecretAnswer::CommentRequired { scope } => {
                    Read::Ask(ReadAnswer::CommentRequired { scope })
                }
                SecretAnswer::ApprovalRequired { approver } => {
                    Read::Ask(ReadAnswer::ApprovalRequired { approver })
                }
                SecretAnswer::NotOffered => Read::Ask(ReadAnswer::NotOffered),
                SecretAnswer::Refused { reason } => Read::Ask(ReadAnswer::Refused { reason }),
            })
        }
        Err(error @ (SourceError::Unreachable(_) | SourceError::Unconfirmed { .. })) => {
            pw.mark_down(&entry.source);
            Ok(Read::Ask(ReadAnswer::Unreachable {
                message: error.to_string(),
            }))
        }
        Err(error) => Err(IpcError::from_source_error(&error, Some(&entry.source))),
    }
}

async fn entry_of<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    entity_id: &str,
) -> Result<Option<PwEntry>, IpcError> {
    let sources = crate::sources::state(app)?;
    reads::entry(&sources.pool, entity_id)
        .await
        .map_err(IpcError::internal)
}

async fn used<R: tauri::Runtime>(app: &tauri::AppHandle<R>, entity_id: &str) {
    if let Ok(sources) = crate::sources::state(app)
        && let Err(error) = reads::used(&sources.pool, entity_id).await
    {
        tracing::warn!(entity = entity_id, %error, "the recent entries could not be kept");
    }
}

/// Every password source, folder and entry: the page's one read.
///
/// # Errors
/// [`IpcError`] while the engine starts, or from the database.
#[tauri::command]
pub async fn passwords_held<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<Held, IpcError> {
    let sources = crate::sources::state(&app)?;
    let pw = state(&app)?;
    let fillers = crate::sources::crud::fillers(
        &sources.pool,
        sources.registry.as_ref(),
        passwords::PASSWORDS,
    )
    .await
    .map_err(IpcError::internal)?;
    let templates = sources.registry.descriptors();
    let down = pw.down();
    reads::held(
        &sources.pool,
        reads::Ask {
            fillers: &fillers,
            templates: &templates,
            down: &down,
        },
    )
    .await
    .map_err(IpcError::internal)
}

/// The address of a password source's own web interface on `item` (an
/// entry's or a folder's entity id) or, with none, its front page (ux30).
/// `None` for a source that is not a password source or whose base URL is no
/// web address. Read at press time; the page opens it through `openWeb`.
///
/// # Errors
/// As [`passwords_held`].
#[tauri::command]
pub async fn password_web_link<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    source: String,
    item: Option<String>,
) -> Result<Option<crate::passwords::web::WebLink>, IpcError> {
    use crate::passwords::web::{self, Target};
    let sources = crate::sources::state(&app)?;
    let Some(row) = knobas_sync::config::get(&sources.pool, &source).await? else {
        return Ok(None);
    };
    let template = sources
        .registry
        .descriptors()
        .into_iter()
        .find(|t| t.adapter_kind == row.adapter_kind);
    let Some(template) = template.filter(|t| t.passwords.is_some()) else {
        return Ok(None);
    };
    let found = match &item {
        Some(id) => reads::web_target(&sources.pool, &source, id)
            .await
            .map_err(IpcError::internal)?,
        None => None,
    };
    let target = match &found {
        Some(reads::WebTarget::Entry { key, folder }) => Target::Entry {
            key,
            folder: folder.as_deref(),
        },
        Some(reads::WebTarget::Folder { key }) => Target::Folder { key },
        None => Target::Home,
    };
    Ok(web::web_link(
        &row.base_url,
        template.web_links.as_ref(),
        target,
    ))
}

/// The rest of one entry.
///
/// # Errors
/// As [`passwords_held`].
#[tauri::command]
pub async fn passwords_entry<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
) -> Result<Option<PwEntry>, IpcError> {
    entry_of(&app, &entity_id).await
}

/// Show a secret for [`HOLD_SECS`](crate::passwords::HOLD_SECS): the one
/// command that sends one to the window.
///
/// # Errors
/// As [`passwords_held`], or the source's refusal.
#[tauri::command]
pub async fn password_show<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    which: SecretRef,
    comment: Option<String>,
) -> Result<ReadAnswer, IpcError> {
    let Some(entry) = entry_of(&app, &entity_id).await? else {
        return Ok(ReadAnswer::NotOffered);
    };
    let pw = state(&app)?;
    if let Some(value) = pw.hold.get(&entity_id, &which) {
        let until = pw.hold.show(&entity_id, &which, value.clone());
        return Ok(ReadAnswer::Shown {
            value,
            read_at: now(),
            until: stamp(until),
        });
    }
    match read(&app, &entry, &which, comment).await? {
        Read::Value(value) => {
            let until = pw.hold.show(&entity_id, &which, value.clone());
            used(&app, &entity_id).await;
            Ok(ReadAnswer::Shown {
                value,
                read_at: now(),
                until: stamp(until),
            })
        }
        Read::Ask(answer) => Ok(answer),
    }
}

/// Copy a secret to the clipboard, which the Rust side clears again after
/// the hold if it still holds it. A shown value is copied from the hold and
/// asks the source nothing.
///
/// # Errors
/// As [`password_show`], or the clipboard's refusal.
#[tauri::command]
pub async fn password_copy<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    which: SecretRef,
    comment: Option<String>,
) -> Result<ReadAnswer, IpcError> {
    let Some(entry) = entry_of(&app, &entity_id).await? else {
        return Ok(ReadAnswer::NotOffered);
    };
    let pw = state(&app)?;
    let value = if let Some(value) = pw.hold.get(&entity_id, &which) {
        value
    } else {
        match read(&app, &entry, &which, comment).await? {
            Read::Value(value) => value,
            Read::Ask(answer) => return Ok(answer),
        }
    };
    let until = pw
        .clipboard
        .copy(value.expose(), &what(&which), Some(&entity_id))
        .map_err(|e| IpcError::internal(format!("the clipboard refused it: {e}")))?;
    used(&app, &entity_id).await;
    Ok(ReadAnswer::Copied {
        read_at: now(),
        until: stamp(until),
    })
}

/// Hide one shown secret, or every one (`entity_id` absent): the page was
/// left.
///
/// # Errors
/// [`IpcError`] before bring-up.
#[tauri::command]
pub fn password_hide<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: Option<String>,
    which: Option<SecretRef>,
) -> Result<(), IpcError> {
    let pw = state(&app)?;
    match (entity_id, which) {
        (Some(entity), Some(which)) => pw.hold.hide(&entity, &which),
        (Some(entity), None) => {
            pw.hold.hide(&entity, &SecretRef::Password);
        }
        _ => pw.hold.hide_all(),
    }
    Ok(())
}

/// The one-time code of an entry for now, made on this machine: it reads
/// nothing from the source.
///
/// # Errors
/// As [`passwords_held`].
#[tauri::command]
pub async fn otp_code<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
) -> Result<CodeAnswer, IpcError> {
    let Some(entry) = entry_of(&app, &entity_id).await? else {
        return Ok(CodeAnswer::NotOffered);
    };
    code_for(&app, &entry).await
}

async fn code_for<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    entry: &PwEntry,
) -> Result<CodeAnswer, IpcError> {
    let Some(otp) = &entry.otp else {
        return Ok(CodeAnswer::NotOffered);
    };
    if otp.format == passwords::OTP_UNREADABLE {
        return Ok(CodeAnswer::Unreadable);
    }
    if matches!(entry.approval, passwords::Approval::Required { .. }) {
        return Ok(CodeAnswer::Hidden);
    }
    let sources = crate::sources::state(app)?;
    let pw = state(app)?;
    let last_run = reads::last_ok_run(&sources.pool, &entry.source)
        .await
        .map_err(IpcError::internal)?;
    let keys = match pw.cached_keys(&entry.source, last_run) {
        Some(keys) => keys,
        None => {
            let read = knobas_secrets::otp::spawn::read(&sources.secrets, &entry.source)
                .await
                .map_err(|e| IpcError::internal(format!("the code keys could not be read: {e}")))?;
            pw.keep_keys(&entry.source, last_run, read)
        }
    };
    let Some(key) = keys.get(&entry.id) else {
        return Ok(CodeAnswer::NeedsSync);
    };
    match crate::passwords::otp::code_now(key) {
        Ok(code) => Ok(CodeAnswer::Code {
            code: code.code,
            digits: code.digits,
            period: code.period,
            left_ms: code.left_ms,
        }),
        Err(_) => Ok(CodeAnswer::Unreadable),
    }
}

/// Copy the entry's code for now to the clipboard. No usage comment, no
/// audit line: it is made here.
///
/// # Errors
/// As [`passwords_held`], or the clipboard's refusal.
#[tauri::command]
pub async fn otp_copy<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
) -> Result<CodeAnswer, IpcError> {
    let Some(entry) = entry_of(&app, &entity_id).await? else {
        return Ok(CodeAnswer::NotOffered);
    };
    let answer = code_for(&app, &entry).await?;
    if let CodeAnswer::Code { code, .. } = &answer {
        state(&app)?
            .clipboard
            .copy(code, "one-time code", Some(&entity_id))
            .map_err(|e| IpcError::internal(format!("the clipboard refused it: {e}")))?;
        used(&app, &entity_id).await;
    }
    Ok(answer)
}

/// Copy a value typed or generated in a dialog, as a secret: it leaves the
/// clipboard after the hold.
///
/// # Errors
/// [`IpcError`] before bring-up, or the clipboard's refusal.
#[tauri::command]
pub fn clipboard_copy_secret<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    text: String,
    what: String,
) -> Result<Copied, IpcError> {
    let until = state(&app)?
        .clipboard
        .copy(&text, &what, None)
        .map_err(|e| IpcError::internal(format!("the clipboard refused it: {e}")))?;
    Ok(Copied {
        until: stamp(until),
    })
}

/// *Clear now*: take back what knobas put on the clipboard, if it is still
/// there. Answers whether it was.
///
/// # Errors
/// [`IpcError`] before bring-up.
#[tauri::command]
pub fn clipboard_clear<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<bool, IpcError> {
    Ok(state(&app)?.clipboard.clear_if_ours(None))
}

/// The window copied something itself (a username, an address): whatever
/// knobas put there is gone, so its deadline is dropped.
///
/// # Errors
/// [`IpcError`] before bring-up.
#[tauri::command]
pub fn clipboard_forget<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), IpcError> {
    state(&app)?.clipboard.forget();
    Ok(())
}

/// What the clipboard holds for knobas now.
///
/// # Errors
/// [`IpcError`] before bring-up.
#[tauri::command]
pub fn clipboard_state<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<ClipState, IpcError> {
    Ok(state(&app)?.clipboard.state())
}

/// What a password source's Settings page counts: entries, folders and the
/// code-key formats the sync found.
///
/// # Errors
/// As [`passwords_held`].
#[tauri::command]
pub async fn password_source_facts<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    source_id: String,
) -> Result<SourceFacts, IpcError> {
    let sources = crate::sources::state(&app)?;
    reads::source_facts(&sources.pool, &source_id)
        .await
        .map_err(IpcError::internal)
}

/// What a direct write answered.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "passwords.ts")]
pub enum WriteAnswer {
    /// The source took it; the entry is `entity_id`.
    Written {
        entity_id: String,
        /// The entry sits in a folder whose path the mirror does not hold
        /// yet: it was moved or made there, and the one listing it was read
        /// back from names no ancestors, so its path shows after the next
        /// sync (ADR-0064 rule 3, #2596).
        path_unread: bool,
    },
    /// The source did not answer: nothing was saved.
    Unreachable { message: String },
    /// It was sent and no answer came: Sync now shows whether it arrived.
    Unconfirmed { message: String },
}

/// The words an activity line says a write did: fixed, never a title.
fn did(write: &passwords::Write) -> &'static str {
    match write.kind() {
        passwords::WriteKind::CreateEntry => "Added an entry",
        passwords::WriteKind::EditEntry => "Edited an entry",
        passwords::WriteKind::SetPassword => "Changed a password",
        passwords::WriteKind::RequestAccess => "Requested access",
    }
}

/// Send a **direct write** to a password source (ADR-0056): at once, never
/// queued, answered when the source has. The entry the source answers goes
/// into the mirror under the source's run lock, as a refresh's does; a code
/// key it set goes to the keychain. Nothing of it is kept if the source does
/// not answer.
///
/// # Errors
/// [`IpcError`] for the source's refusal, or when the engine is not up.
#[tauri::command]
pub async fn passwords_write<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    write: passwords::Write,
) -> Result<WriteAnswer, IpcError> {
    let sources = crate::sources::state(&app)?;
    let pw = state(&app)?;
    let source_id = match &write {
        passwords::Write::CreateEntry { source, .. } => source.clone(),
        other => other
            .target()
            .split(':')
            .next()
            .unwrap_or_default()
            .to_owned(),
    };
    let lent = sources
        .live
        .source(
            &sources.pool,
            &sources.secrets,
            &sources.registry,
            &format!("{source_id}:write"),
        )
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&source_id)))?;
    let verb = did(&write);
    let target = write.target().to_owned();
    let written = match lent.source.passwords_write(write).await {
        Ok(written) => written,
        Err(SourceError::Unreachable(message)) => {
            pw.mark_down(&source_id);
            return Ok(WriteAnswer::Unreachable { message });
        }
        Err(SourceError::Unconfirmed { message, .. }) => {
            return Ok(WriteAnswer::Unconfirmed { message });
        }
        Err(error) => return Err(IpcError::from_source_error(&error, Some(&source_id))),
    };
    pw.mark_up(&source_id);
    let entity_id = written
        .item
        .as_ref()
        .map_or(target, |item| item.entity.to_string());
    let mut path_unread = false;
    if let Some(mut item) = written.item {
        let mut record: passwords::EntryRecord =
            serde_json::from_value(item.payload.clone()).unwrap_or_default();
        // One listing does not hold the entry's ancestors: keep the path the
        // mirror has while the entry stays in its folder.
        if record.path.is_empty()
            && let Ok(Some(old)) = reads::entry(&sources.pool, &entity_id).await
            && record.folder == old.folder
        {
            let path = reads::mirrored_path(&sources.pool, &entity_id)
                .await
                .ok()
                .flatten();
            record.path = path.unwrap_or_default();
            item.payload = serde_json::to_value(&record).unwrap_or(item.payload);
        }
        path_unread = record.path.is_empty() && record.folder.is_some();
        let deps = sources.scheduler.deps();
        crate::passwords::mirror_written(deps.connections.as_ref(), lent.source.as_ref(), item)
            .await
            .map_err(|e| {
                IpcError::internal(format!(
                    "the source took the change; the mirror did not: {e}"
                ))
            })?;
    }
    if let Some(key) = written.otp_key {
        let keys: knobas_secrets::otp::OtpKeys = [(entity_id.clone(), key)].into_iter().collect();
        if let Err(error) =
            knobas_secrets::otp::spawn::merge(&sources.secrets, &source_id, keys).await
        {
            tracing::warn!(source = %source_id, %error, "the written entry's code key could not be kept");
        }
        pw.forget_source(&source_id);
    }
    // The activity line names the entry by id and says what was done in fixed words: no title, no value.
    let entity = knobas_core::entity::EntityRef::parse(&entity_id).ok();
    if let Err(error) = knobas_core::activity::record(
        &sources.pool,
        "user",
        verb,
        entity.as_ref(),
        serde_json::json!({}),
    )
    .await
    {
        tracing::warn!(entity = %entity_id, %error, "the write's activity line was not kept");
    }
    Ok(WriteAnswer::Written {
        entity_id,
        path_unread,
    })
}

/// What opening one attachment answered (step 9a).
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "passwords.ts")]
pub enum AttachmentAnswer {
    /// The bytes, read just now, for the viewer to hold while it is open.
    /// `text` is the file as text where it is UTF-8 with no NUL.
    Read {
        name: String,
        #[ts(type = "number")]
        size: u64,
        /// Base64.
        data: String,
        text: Option<String>,
    },
    /// Over the viewer's cap: *Save* still saves it.
    TooLarge {
        #[ts(type = "number")]
        size: u64,
    },
    /// The entry lists no such attachment, or its source reads none.
    NotOffered,
    /// The source did not answer.
    Unreachable { message: String },
}

/// The attachment `attachment_id` as the mirror lists it on `entry`, or `None`.
fn listed<'a>(entry: &'a PwEntry, attachment_id: &str) -> Option<&'a passwords::AttachmentMeta> {
    match &entry.attachments {
        passwords::Attachments::Listed(files) => files.iter().find(|f| f.id == attachment_id),
        _ => None,
    }
}

/// Open one of an entry's attachments (step 9a): its bytes read live through
/// the source, once, and handed to the window. Nothing keeps them here: not
/// the live-read cache the ticket page's previews use (ADR-0042), not a file.
///
/// # Errors
/// [`IpcError`] for the source's refusal, or when the engine is not up.
#[tauri::command]
pub async fn password_attachment<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    attachment_id: String,
) -> Result<AttachmentAnswer, IpcError> {
    use base64::Engine as _;
    use knobas_source::view::{ATTACHMENT_CAP, FileBytes};
    let Some(entry) = entry_of(&app, &entity_id).await? else {
        return Ok(AttachmentAnswer::NotOffered);
    };
    let Some(file) = listed(&entry, &attachment_id) else {
        return Ok(AttachmentAnswer::NotOffered);
    };
    let sources = crate::sources::state(&app)?;
    let pw = state(&app)?;
    let lent = sources
        .live
        .source(
            &sources.pool,
            &sources.secrets,
            &sources.registry,
            &entry.id,
        )
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&entry.source)))?;
    match lent
        .source
        .attachment_bytes(&entry.id, &file.id, ATTACHMENT_CAP)
        .await
    {
        Ok(Some(FileBytes::Read(bytes))) => {
            pw.mark_up(&entry.source);
            used(&app, &entry.id).await;
            let text = std::str::from_utf8(&bytes)
                .ok()
                .filter(|t| !t.contains('\0'))
                .map(str::to_owned);
            Ok(AttachmentAnswer::Read {
                name: file.name.clone(),
                size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                data: base64::engine::general_purpose::STANDARD.encode(&bytes),
                text,
            })
        }
        Ok(Some(FileBytes::TooLarge { size })) => {
            pw.mark_up(&entry.source);
            Ok(AttachmentAnswer::TooLarge { size })
        }
        Ok(None) => Ok(AttachmentAnswer::NotOffered),
        Err(error @ (SourceError::Unreachable(_) | SourceError::Unconfirmed { .. })) => {
            pw.mark_down(&entry.source);
            Ok(AttachmentAnswer::Unreachable {
                message: error.to_string(),
            })
        }
        Err(error) => Err(IpcError::from_source_error(&error, Some(&entry.source))),
    }
}

/// Save one of an entry's attachments into the Downloads folder (step 9a),
/// read live and written once, where the person asked; answers its path.
///
/// # Errors
/// [`IpcError`]: `not_found` for an attachment the entry does not list, the
/// source's refusal, a file that cannot be written, no downloads folder.
#[tauri::command]
pub async fn password_attachment_save<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    entity_id: String,
    attachment_id: String,
) -> Result<String, IpcError> {
    let entry = entry_of(&app, &entity_id)
        .await?
        .ok_or_else(|| IpcError::not_found(format!("no entry {entity_id}")))?;
    let file = listed(&entry, &attachment_id)
        .ok_or_else(|| {
            IpcError::not_found(format!("{entity_id} lists no attachment {attachment_id}"))
        })?
        .clone();
    let dir = app
        .path()
        .download_dir()
        .map_err(|error| IpcError::internal(format!("no downloads folder: {error}")))?;
    let sources = crate::sources::state(&app)?;
    let pw = state(&app)?;
    let lent = sources
        .live
        .source(
            &sources.pool,
            &sources.secrets,
            &sources.registry,
            &entry.id,
        )
        .await
        .map_err(|error| crate::sources::to_ipc(&error, Some(&entry.source)))?;
    let mut sink = crate::downloads::FileSink::new(&dir);
    let streamed = lent
        .source
        .download_attachment(&entry.id, &file.id, &file.name, &mut sink)
        .await;
    let finished = match streamed {
        Ok(true) => sink.finish().await.map(Some),
        Ok(false) => Ok(None),
        Err(error) => Err(error),
    };
    match finished {
        Ok(Some(path)) => {
            pw.mark_up(&entry.source);
            Ok(path.display().to_string())
        }
        Ok(None) => Err(IpcError::invalid(format!(
            "{}'s source reads no attachments",
            entry.source
        ))),
        Err(error) => {
            sink.discard().await;
            if matches!(error, SourceError::Unreachable(_)) {
                pw.mark_down(&entry.source);
            }
            Err(IpcError::from_source_error(&error, Some(&entry.source)))
        }
    }
}
