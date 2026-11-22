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
        web_links: Some(web_links()),
        web_sign_in: Some(web_sign_in()),
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

/// The web client's sign-in form (measured on 9.5.4, 2026-11-22): drawn at
/// `/SignIn/Direct`, posted to `/Account/SignIn`, where a refused password
/// draws it again, so the one-try rule sees a refusal. A signed-out address
/// lands on `/Account/SignIn` with no form, only a *Connect* link to
/// `/SignIn/Direct`. The web client signs in with the account the API does,
/// so the source's own credential is filled (ux30); its *Remember me* keeps
/// the session 4 hours.
pub const SIGN_IN_PATHS: [&str; 2] = ["/SignIn/Direct", "/Account/SignIn"];

fn web_sign_in() -> knobas_source::web_sign_in::WebSignInPages {
    knobas_source::web_sign_in::WebSignInPages::at(&SIGN_IN_PATHS).with_credential()
}

/// Where the web client shows a folder (9.5.4): `/WebClient/Main?itemId=<GUID>`
/// selects it. The same address opens an entry in its read-only dialog, and
/// that dialog's load writes "Password Fetched" under the account (measured
/// in knobas's window, 2026-11-22), so no entry page is declared: an entry
/// opens its folder (Fable's ruling while Björn was busy). Each goes
/// through `/SignIn/Direct?ReturnUrl=…`, which draws the form when signed out
/// (so the fill needs no press on *Connect*) and answers a 302 to the page
/// when signed in. `{key}` is encoded once inside the encoded `ReturnUrl`;
/// a GUID needs no encoding. `ReturnUrl` counts from the server's root, as
/// Pleasant writes it: an instance behind a path prefix was not measured.
pub const WEB_ITEM: &str = "/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain%3FitemId%3D{key}";

/// The web client's front page, through the sign-in form as [`WEB_ITEM`].
pub const WEB_HOME: &str = "/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain";

fn web_links() -> knobas_source::web_links::WebLinks {
    knobas_source::web_links::WebLinks {
        home: Some(WEB_HOME.to_owned()),
        pages: [(passwords::FOLDER_KIND.to_owned(), WEB_ITEM.to_owned())].into(),
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

    /// The web client's sign-in pages and folder link hold to their rules, a
    /// folder's link returns, once signed in, to the folder's own page, and no
    /// entry page is declared: opening one writes "Password Fetched".
    #[test]
    fn the_web_client_is_declared() {
        let d = descriptor_template();
        let pages = d.web_sign_in.as_ref().unwrap();
        assert_eq!(knobas_source::web_sign_in::check(pages), Ok(()));
        assert!(pages.uses_credential);
        assert_eq!(pages.paths, ["/SignIn/Direct", "/Account/SignIn"]);
        let links = d.web_links.as_ref().unwrap();
        let kinds: Vec<&str> = d.entity_kinds.iter().map(|k| k.id.as_str()).collect();
        assert_eq!(knobas_source::web_links::check(links, &kinds), Ok(()));
        let guid = "07c469c8-e880-4927-8c31-a8342cd66d98";
        assert_eq!(links.page(passwords::ENTRY_KIND), None);
        let url = knobas_source::web_links::page(
            "https://pw.example:10001/",
            links.page(passwords::FOLDER_KIND).unwrap(),
            guid,
        )
        .unwrap();
        let parsed = url::Url::parse(&url).unwrap();
        assert_eq!(parsed.path(), "/SignIn/Direct");
        let back: Vec<(String, String)> = parsed.query_pairs().into_owned().collect();
        assert_eq!(
            back,
            [(
                "ReturnUrl".to_owned(),
                format!("/WebClient/Main?itemId={guid}")
            )]
        );
        assert_eq!(
            knobas_source::web_links::home("https://pw.example:10001", Some(links)),
            "https://pw.example:10001/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain"
        );
    }
}
