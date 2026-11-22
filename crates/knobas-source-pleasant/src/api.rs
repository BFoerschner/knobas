//! Pleasant Password Server's REST API v5, as far as knobas reads it.
//!
//! Every field is optional and unknown fields are ignored: a server that
//! leaves one out must not fail a sync. The shapes are the ones a real
//! Pleasant 9.5.4 answered (`tests/fixtures/recorded`, `S/pleasant-server.md`
//! §6). `Password` is not declared at all, so whatever a
//! listing sends there is never even parsed.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The comment rules a folder or an entry carries.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct CommentPrompts {
    pub ask_for_comment_on_view_password: bool,
    pub ask_for_comment_on_view_offline: bool,
    pub ask_for_comment_on_modify_entries: bool,
    pub ask_for_comment_on_move_entries: bool,
    pub ask_for_comment_on_move_folders: bool,
    pub ask_for_comment_on_modify_folders: bool,
}

/// One entry ("credential") as a folder listing carries it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ApiEntry {
    pub id: String,
    pub name: Option<String>,
    pub username: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub group_id: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    pub expires: Option<String>,
    pub custom_user_fields: BTreeMap<String, Option<serde_json::Value>>,
    pub has_modify_entries_access: Option<bool>,
    pub has_view_entry_contents_access: Option<bool>,
    /// Whether the account may read the password (9.5.4).
    pub has_view_entry_password_access: Option<bool>,
    /// Whether the account may see the entry's code key (9.5.4).
    #[serde(rename = "HasViewTOTPAccess")]
    pub has_view_totp_access: Option<bool>,
    /// The comment rules in force for the entry, its folders' included: an
    /// entry carries them in every listing, where a folder object carries
    /// its own only in the whole tree (§6.4).
    pub comment_prompts: Option<CommentPrompts>,
    /// Pleasant's own code key: plain Base32, `null` when unset.
    #[serde(rename = "TOTPSecret")]
    pub totp_secret: Option<String>,
    /// `0` when unset.
    #[serde(rename = "TOTPDigits")]
    pub totp_digits: Option<u32>,
    /// `0` when unset.
    #[serde(rename = "TOTPPeriod")]
    pub totp_period: Option<u32>,
    /// The entry's attachments, named with no bytes.
    pub attachments: Vec<ApiAttachment>,
}

/// One attachment as a listing names it: never its bytes (`FileData` is not
/// declared, so an answer that carries them is never parsed into memory
/// here).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ApiAttachment {
    pub attachment_id: String,
    pub file_name: Option<String>,
    pub file_size: Option<u64>,
}

/// One attachment with its bytes, as `entries/{id}/attachments/{aid}`
/// answers it (9.5.4: `FileData` is base64; reading it writes no audit line).
#[derive(Clone, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ApiFile {
    pub file_name: Option<String>,
    pub file_data: String,
}

impl std::fmt::Debug for ApiFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiFile")
            .field("file_name", &self.file_name)
            .field("file_data", &"<redacted>")
            .finish()
    }
}

/// One folder ("credential group") with what it holds.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ApiFolder {
    pub id: String,
    pub name: Option<String>,
    pub parent_id: Option<String>,
    pub children: Vec<ApiFolder>,
    pub credentials: Vec<ApiEntry>,
    pub has_modify_entries_access: Option<bool>,
    pub comment_prompts: Option<CommentPrompts>,
}

/// `POST /OAuth2/Token`'s answer.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TokenAnswer {
    pub access_token: String,
    /// Seconds, where the server says so.
    pub expires_in: Option<u64>,
    /// The vendor's documented spelling: seconds or an instant.
    pub expiry: Option<serde_json::Value>,
}

/// `GET GetServerInfo`'s answer, what *Test connection* reports.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct ServerInfo {
    pub version: Option<String>,
    pub server_version: Option<String>,
}

/// An instant as Pleasant spells it: RFC 3339, or a local time with no offset
/// (read as UTC).
#[must_use]
pub fn instant(text: Option<&str>) -> Option<chrono::DateTime<chrono::Utc>> {
    let text = text?.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(at) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(at.with_timezone(&chrono::Utc));
    }
    chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|naive| naive.and_utc())
}

/// The calendar day of an expiry.
#[must_use]
pub fn day(text: Option<&str>) -> Option<chrono::NaiveDate> {
    let text = text?.trim();
    text.get(..10)
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instants_and_days_read_in_pleasants_spellings() {
        assert!(instant(Some("2026-08-30T10:22:00.25-06:00")).is_some());
        assert_eq!(
            instant(Some("2026-08-30T10:22:00")).unwrap().to_rfc3339(),
            "2026-08-30T10:22:00+00:00"
        );
        assert!(instant(Some("")).is_none());
        assert_eq!(
            day(Some("2027-01-31T00:00:00")),
            chrono::NaiveDate::from_ymd_opt(2027, 1, 31)
        );
        assert!(day(None).is_none());
    }

    #[test]
    fn a_listing_is_read_and_its_password_never_parsed() {
        let entry: ApiEntry = serde_json::from_value(serde_json::json!({
            "Id": "e1", "Name": "db", "Password": "hunter2", "Unknown": 1,
            "CustomUserFields": {"Port": "5432", "Empty": null}
        }))
        .unwrap();
        assert_eq!(entry.name.as_deref(), Some("db"));
        assert_eq!(entry.custom_user_fields.len(), 2);
        assert!(!format!("{entry:?}").contains("hunter2"));
    }
}
