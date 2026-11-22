//! What a Pleasant source is, said once.

use knobas_source::passwords::{self, Offers, WriteKind};
use knobas_source::role::RoleClaim;
use knobas_source::{AuthMethod, SourceDescriptor};

/// The template every Pleasant instance is built from.
///
/// It declares what the real server was shown to do: one-time codes (made
/// on this machine), expiry dates, usage comments, and that the server
/// records each read under the account knobas signs in as. Writes,
/// attachments, history and access requests are declared by later steps,
/// each only where the API offers it.
#[must_use]
pub fn descriptor_template() -> SourceDescriptor {
    SourceDescriptor {
        id: crate::ADAPTER_KIND.to_owned(),
        adapter_kind: crate::ADAPTER_KIND.to_owned(),
        name: "Pleasant Password Server".to_owned(),
        capabilities: Vec::new(),
        adapter_version: crate::ADAPTER_VERSION.to_owned(),
        auth_methods: vec![AuthMethod::UserPassword],
        accepts_account: false,
        min_sync_interval_secs: knobas_source::DEFAULT_MIN_SYNC_INTERVAL_SECS,
        write_ops: Vec::new(),
        // Every run walks the whole tree, so absence is deletion.
        entity_kinds: passwords::kind_infos(true),
        config_schema: config_schema(),
        payload_paths: passwords::payload_paths(),
        roles: vec![RoleClaim::new(
            passwords::PASSWORDS,
            [passwords::READ_SECRET, passwords::ENTRY_ATTACHMENTS],
        )],
        source_actions: Vec::new(),
        clone_from: Vec::new(),
        url_forms: Vec::new(),
        code_pages: None,
        // Each read may be a line in the server's audit log under the
        // person's name: nothing is read in the background.
        by_hand_by_default: true,
        passwords: Some(Offers {
            one_time_codes: true,
            expiry: true,
            usage_comments: true,
            audit_log: true,
            history: None,
            // Listed by every listing, read one at a time (9.5.4, §6.3).
            attachments: true,
            access_requests: false,
            // Confirmed on 9.5.4: a listing sends every custom field's value
            // and marks none protected (`S/pleasant-server.md` §6.3).
            marks_protected_fields: false,
            writes: vec![
                WriteKind::CreateEntry,
                WriteKind::EditEntry,
                WriteKind::SetPassword,
            ],
        }),
    }
}

fn config_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "username": {
                "type": "string", "title": "Username",
                "description": "The account knobas signs in as, with the password below. Test connection says whom it signed in as. The server records each read under this account."
            },
            "walk": {
                "type": "string", "enum": ["tree", "folders"], "default": "tree",
                "title": "Reading the tree",
                "description": "tree reads every folder in one request. folders reads a folder at a time, for a server that is slow to send a large tree."
            }
        },
        "required": ["username"]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_fills_the_passwords_role_and_syncs_by_hand() {
        let d = descriptor_template();
        assert_eq!(knobas_source::role::check(&d), Ok(()));
        assert!(d.by_hand_by_default);
        assert!(d.write_ops.is_empty());
        assert!(d.entity_kinds.iter().all(|k| k.title_stays_in_mirror));
        // The schema's keys are the configuration's own.
        let keys: Vec<&String> = d.config_schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        let wire = serde_json::to_value(crate::PleasantConfig::default()).unwrap();
        assert_eq!(keys, wire.as_object().unwrap().keys().collect::<Vec<_>>());
    }
}
