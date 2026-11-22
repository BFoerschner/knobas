//! From Pleasant's tree to the role's records (ADR-0056).
//!
//! Depth first, a folder before what is in it. Pleasant's own root folder is
//! not emitted, and an entry directly in it has no folder. `Password` is never
//! read (it is not even declared in [`crate::api::ApiEntry`]). Every code-key
//! field is taken out of an entry before anything else sees its fields.

use std::collections::BTreeMap;

use knobas_source::SyncItem;
use knobas_source::passwords::{
    self, Approval, Attachments, CommentRules, EntryHead, EntryRecord, FieldRecord, FolderRecord,
    OtpKey, PathStep,
};

use crate::api::{self, ApiEntry, ApiFolder, CommentPrompts};

/// What one walk of the tree produced.
#[derive(Debug, Default)]
pub struct Walked {
    pub items: Vec<SyncItem>,
    /// Entity id and key, for every entry whose key could be read.
    pub keys: Vec<(String, OtpKey)>,
    pub folders: usize,
    pub entries: usize,
}

/// A field's value as the listing sent it: a string, or a scalar written as
/// text. `None` for a null, an empty string, or anything else: read live.
fn field_value(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// The comment rules in force for something under `inherited`, with its own
/// prompts laid over them. A folder's rule covers everything below it: knobas
/// asks where the server would, and asking once too often costs a field, not
/// a refused read.
fn rules(inherited: CommentRules, own: Option<&CommentPrompts>) -> CommentRules {
    let own = own.copied().unwrap_or_default();
    CommentRules {
        read: inherited.read || own.ask_for_comment_on_view_password,
        modify: inherited.modify || own.ask_for_comment_on_modify_entries,
    }
}

fn some_text(text: Option<&String>) -> Option<String> {
    text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

/// One entry's record and its code key.
pub fn entry(
    source: &str,
    e: &ApiEntry,
    folder: Option<&str>,
    path: &[PathStep],
    inherited: CommentRules,
) -> (SyncItem, Option<OtpKey>) {
    // Every field as text first, so the key fields can be taken out whole.
    let mut texts: BTreeMap<String, String> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for (name, value) in &e.custom_user_fields {
        order.push(name.clone());
        if let Some(text) = field_value(value.as_ref()) {
            texts.insert(name.clone(), text);
        }
    }
    let found = crate::otp::take(&mut texts);
    let fields: Vec<FieldRecord> = order
        .into_iter()
        .filter(|name| !crate::otp::is_key_field(name))
        .map(|name| FieldRecord {
            value: texts.get(&name).cloned(),
            name,
        })
        .collect();
    let (otp, key) = match found {
        Some(f) => (Some(f.info), f.key),
        None => (None, None),
    };
    let record = EntryRecord {
        username: some_text(e.username.as_ref()),
        address: some_text(e.url.as_ref()),
        folder: folder.map(|id| format!("{source}:{id}")),
        path: path.to_vec(),
        also_in: Vec::new(),
        notes: some_text(e.notes.as_ref()),
        fields,
        otp,
        expires: api::day(e.expires.as_deref()),
        changed_by: None,
        comment: rules(inherited, e.comment_prompts.as_ref()),
        approval: Approval::None,
        attachments: Attachments::None,
        can_read: e.has_view_entry_contents_access.unwrap_or(true),
        can_modify: e.has_modify_entries_access.unwrap_or(false),
    };
    let item = passwords::entry_item(
        EntryHead {
            source: source.to_owned(),
            key: e.id.clone(),
            title: some_text(e.name.as_ref()).unwrap_or_else(|| "(untitled)".to_owned()),
            updated_at: api::instant(e.modified.as_deref().or(e.created.as_deref())),
        },
        &record,
    );
    (item, key)
}

/// Walk the tree under Pleasant's root. The root itself is not emitted.
#[must_use]
pub fn walk(source: &str, root: &ApiFolder) -> Walked {
    let mut out = Walked::default();
    let inherited = rules(CommentRules::default(), root.comment_prompts.as_ref());
    for e in &root.credentials {
        push_entry(&mut out, source, e, None, &[], inherited);
    }
    for child in &root.children {
        walk_folder(&mut out, source, child, None, &[], inherited);
    }
    out
}

fn push_entry(
    out: &mut Walked,
    source: &str,
    e: &ApiEntry,
    folder: Option<&str>,
    path: &[PathStep],
    inherited: CommentRules,
) {
    let (item, key) = entry(source, e, folder, path, inherited);
    if let Some(key) = key {
        out.keys.push((item.entity.to_string(), key));
    }
    out.entries += 1;
    out.items.push(item);
}

fn walk_folder(
    out: &mut Walked,
    source: &str,
    f: &ApiFolder,
    parent: Option<&str>,
    above: &[PathStep],
    inherited: CommentRules,
) {
    let comment = rules(inherited, f.comment_prompts.as_ref());
    let name = some_text(f.name.as_ref()).unwrap_or_else(|| "(unnamed)".to_owned());
    let record = FolderRecord {
        name: name.clone(),
        parent: parent.map(|id| format!("{source}:{id}")),
        path: above.to_vec(),
        comment,
        can_add: f.has_modify_entries_access.unwrap_or(false),
    };
    out.items
        .push(passwords::folder_item(source, &f.id, &record));
    out.folders += 1;
    let mut path = above.to_vec();
    path.push(PathStep {
        id: format!("{source}:{}", f.id),
        name,
    });
    for e in &f.credentials {
        push_entry(out, source, e, Some(&f.id), &path, comment);
    }
    for child in &f.children {
        walk_folder(out, source, child, Some(&f.id), &path, comment);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> ApiFolder {
        serde_json::from_str(
            r#"{
            "Id": "root", "Name": "Root", "ParentId": null,
            "Credentials": [{"Id": "e0", "Name": "Loose entry", "Username": "u0"}],
            "Children": [{
                "Id": "f1", "Name": "Tidewater", "ParentId": "root", "HasModifyEntriesAccess": true,
                "Children": [{
                    "Id": "f2", "Name": "Payments", "ParentId": "f1",
                    "CommentPrompts": {"AskForCommentOnViewPassword": true},
                    "Children": [{"Id": "f3", "Name": "Databases", "ParentId": "f2", "Children": [], "Credentials": [{
                        "Id": "e1", "Name": "payout-service database user", "Username": "payout_svc",
                        "Password": "hunter2", "Url": "pg-pay-01:5432", "Notes": "Rotated quarterly.",
                        "GroupId": "f3", "Modified": "2026-08-30T10:22:00Z", "Expires": "2027-01-31T00:00:00",
                        "HasViewEntryContentsAccess": true, "HasModifyEntriesAccess": false,
                        "CustomUserFields": {"Port": "5432", "Recovery PIN": null,
                                             "TimeOtp-Secret-Base32": "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"}
                    }]}],
                    "Credentials": []
                }],
                "Credentials": []
            }]
        }"#,
        )
        .unwrap()
    }

    #[test]
    fn the_root_is_left_out_and_paths_chain_the_folders() {
        let walked = walk("pleasant", &tree());
        assert_eq!((walked.folders, walked.entries), (3, 2));
        let ids: Vec<String> = walked.items.iter().map(|i| i.entity.to_string()).collect();
        assert_eq!(
            ids,
            [
                "pleasant:e0",
                "pleasant:f1",
                "pleasant:f2",
                "pleasant:f3",
                "pleasant:e1"
            ]
        );
        let loose: EntryRecord = serde_json::from_value(walked.items[0].payload.clone()).unwrap();
        assert_eq!(loose.folder, None);
        let db = &walked.items[4];
        let record: EntryRecord = serde_json::from_value(db.payload.clone()).unwrap();
        assert_eq!(record.folder.as_deref(), Some("pleasant:f3"));
        assert_eq!(
            record
                .path
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["Tidewater", "Payments", "Databases"]
        );
        // The comment rule of Payments covers the folder below it.
        assert!(record.comment.read);
        assert!(!record.comment.modify);
        assert!(!record.can_modify && record.can_read);
        assert_eq!(record.expires, chrono::NaiveDate::from_ymd_opt(2027, 1, 31));
        assert_eq!(
            db.updated_at.unwrap().to_rfc3339(),
            "2026-08-30T10:22:00+00:00"
        );
        // The key left the fields; the field the listing did not send is read live.
        assert_eq!(
            record.fields,
            vec![
                FieldRecord {
                    name: "Port".into(),
                    value: Some("5432".into())
                },
                FieldRecord {
                    name: "Recovery PIN".into(),
                    value: None
                },
            ]
        );
        assert_eq!(record.otp.as_ref().unwrap().format, "keepass");
        assert_eq!(walked.keys.len(), 1);
        assert_eq!(walked.keys[0].0, "pleasant:e1");
        let wire = db.payload.to_string() + &db.body_text + &db.title;
        assert!(
            !wire.contains("hunter2"),
            "the password reached the item: {wire}"
        );
        assert!(
            !wire.contains("GEZDGNBV"),
            "the code key reached the item: {wire}"
        );
        let folder: FolderRecord = serde_json::from_value(walked.items[1].payload.clone()).unwrap();
        assert_eq!(folder.parent, None);
        assert!(folder.can_add);
    }

    #[test]
    fn a_fields_value_is_text_a_number_or_a_flag_and_nothing_else() {
        use serde_json::json;
        assert_eq!(field_value(Some(&json!("5432"))), Some("5432".into()));
        assert_eq!(field_value(Some(&json!(5432))), Some("5432".into()));
        assert_eq!(field_value(Some(&json!(true))), Some("true".into()));
        assert_eq!(field_value(Some(&json!(""))), None);
        assert_eq!(field_value(Some(&json!(null))), None);
        assert_eq!(field_value(Some(&json!({"a": 1}))), None);
        assert_eq!(field_value(None), None);
    }
}
