//! The document Uptime Kuma's `add` event takes, and what knobas is allowed
//! to put in it (issue #453, spec #427 story 70).
//!
//! # Why the whole document and not three fields
//!
//! `add` is not a partial update. `server.js` hands what it is given to
//! RedBean's `bean.import` and then to `bean.validate()`, so a field knobas
//! leaves out is a column left at whatever the ORM defaults it to -- and two
//! of those defaults are load-bearing: a monitor created with no
//! `accepted_statuscodes` fails `.every(...)` inside the server (it runs that
//! for **every** monitor of **every** type, with no type check first), and one
//! created with no `active` never starts checking. So this module sends the
//! same complete document `testenv/kuma-monitor.mjs` sends, which is the shape
//! read off the pinned image (2.5.3) rather than recalled -- and
//! [`tests::the_document_is_the_seeds_own`] holds the two in step by reading
//! the seed's file.
//!
//! # What a reader chooses, and what is chosen for them
//!
//! Two fields: the **name** and the **URL**. Everything else is fixed, and
//! each of the three groups is fixed for its own reason.
//!
//! * **`type` is `http`.** [`knobas_source::WriteOp::CreateMonitor`] carries a
//!   URL and no type, because a monitor of a URL is an HTTP check. The pane's
//!   form is prefilled from an asset's route or its hostname, so a URL is what
//!   a reader has.
//! * **The schedule** -- a check a minute, a 16-second timeout, no retries --
//!   is Uptime Kuma's own default schedule and the one every monitor in
//!   `testenv/monitors.json` runs on. Making it a form field would be asking a
//!   reader for a number before knobas has anything to say about which one is
//!   right; the monitor's own page in Kuma is one click away (story 71) and is
//!   where a schedule is tuned.
//! * **The switches** -- notifications, TLS-expiry warnings, ignored
//!   certificates, upside-down mode, conditions -- are all *off*. A create
//!   from knobas makes the plainest possible check: the reader asked for
//!   something to be watched, not for a policy, and every one of these has a
//!   sensible home in Kuma and none in a small form.

use knobas_source::SourceError;

/// The one monitor type this adapter creates. See the module note.
pub(crate) const TYPE: &str = "http";

/// Every field the `add` document carries beside the name, the URL and the
/// type. See the module note for why each is fixed.
const INTERVAL_SECS: i64 = 60;
const RETRY_INTERVAL_SECS: i64 = 60;
const TIMEOUT_SECS: i64 = 16;
const MAX_REDIRECTS: i64 = 10;

/// The document `add` is emitted with, for a monitor called `name` watching
/// `url`.
///
/// # Errors
///
/// [`SourceError::Protocol`] for a name or a URL knobas will not send: a blank
/// either, or a URL that is not an http(s) one. Refused **here** rather than
/// at Kuma, for `crate::source::monitor_id`'s reason -- Kuma answers a bad
/// create with its own validation message, which reads as knobas having sent
/// something reasonable and Kuma having disagreed. What is actually true is
/// that the write could never have been delivered.
pub(crate) fn document(name: &str, url: &str) -> Result<serde_json::Value, SourceError> {
    let name = name.trim();
    let url = url.trim();
    if name.is_empty() {
        return Err(SourceError::protocol(
            "an Uptime Kuma monitor needs a name -- it is also what attaches it to an asset"
                .to_owned(),
        ));
    }
    // The same two schemes `KumaHttp` accepts for the base URL, and the same
    // reason: what Kuma does with anything else is fetch nothing and report a
    // check that never runs.
    let parsed = url::Url::parse(url)
        .ok()
        .filter(|parsed| matches!(parsed.scheme(), "http" | "https"))
        .ok_or_else(|| {
            SourceError::protocol(format!(
                "{url:?} is not an http(s) URL, so an Uptime Kuma HTTP monitor cannot watch it"
            ))
        })?;
    Ok(serde_json::json!({
        "type": TYPE,
        "name": name,
        // The parsed URL and not the caller's string: a paste carrying a
        // trailing space or a missing path is normalised once, here, so the
        // monitor knobas asks for is the monitor Kuma reports back -- and
        // `monitor_url_host`'s suggestion rule reads that same URL out of the
        // mirror (#478).
        "url": parsed.as_str(),
        "interval": INTERVAL_SECS,
        "retryInterval": RETRY_INTERVAL_SECS,
        "resendInterval": 0,
        "maxretries": 0,
        "timeout": TIMEOUT_SECS,
        "accepted_statuscodes": ["200-299"],
        "active": true,
        "expiryNotification": false,
        "ignoreTls": false,
        "upsideDown": false,
        "notificationIDList": {},
        "conditions": [],
        "method": "GET",
        "maxredirects": MAX_REDIRECTS,
    }))
}

/// The monitor id Kuma minted, out of its acknowledgement.
///
/// `{"ok":true,"msg":"successAdded","msgi18n":true,"monitorID":31}`, measured
/// on the pinned image. `None` when the answer carries no id -- which is not a
/// failure: `ok` was already checked, so the monitor exists, and a receipt
/// knobas does not have is [`knobas_source::WriteReceipt::none`]'s own case.
/// Inventing one would be knobas naming an artefact it never saw.
pub(crate) fn minted_id(answer: &serde_json::Value) -> Option<String> {
    let id = answer.get("monitorID")?;
    // A number on the wire, and a string in the receipt: `WriteReceipt` holds
    // "the id the source gave, in the source's own spelling", and a monitor's
    // own spelling of its id is what the entity key carries (`kuma:31`).
    id.as_i64()
        .map(|id| id.to_string())
        .or_else(|| id.as_str().map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The document knobas sends is the one the test environment's own seed
    /// sends -- field for field, value for value.
    ///
    /// **This is the fidelity guard, and it reads the other file.**
    /// `testenv/kuma-monitor.mjs` is the node one-shot `just kuma-live` uses
    /// to add and delete monitors, its payload was read off the pinned image
    /// (2.5.3), and its header records the three traps in Kuma's socket.io
    /// dialect. Two implementations of one document is exactly the drift this
    /// repository has been bitten by before, so rather than restate the shape
    /// in a literal here, this parses the seed's own object out of that file
    /// and compares. A field added to one and not the other fails here.
    ///
    /// The two deliberate differences are named below, and both are about what
    /// the *caller* chose: the name and the URL.
    #[test]
    fn the_document_is_the_seeds_own() {
        let seed = include_str!("../../../testenv/kuma-monitor.mjs");
        let object = seed
            .split_once("await call(\"add\", {")
            .expect("kuma-monitor.mjs emits an `add`")
            .1
            .split_once("\n  })")
            .expect("the add's object closes")
            .0;

        // The seed is JavaScript: bare keys, a trailing comma, `name` and
        // `url` as identifiers rather than literals. Turned into JSON here so
        // the comparison is over values and not over formatting.
        let mut fields: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::new();
        for line in object.lines() {
            let line = line.trim().trim_end_matches(',');
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let (key, value) = match line.split_once(':') {
                Some((key, value)) => (key.trim(), value.trim()),
                // `name,` and `url,` -- shorthand for the caller's own two.
                None => (line, line),
            };
            fields.insert(key.to_owned(), value.replace('\'', "\""));
        }

        let ours = document("gitea", "http://gitea:3000/api/healthz").expect("a legal create");
        let ours = ours.as_object().expect("an object");
        assert_eq!(
            fields.keys().cloned().collect::<Vec<_>>(),
            ours.keys().cloned().collect::<Vec<_>>(),
            "the adapter's `add` document and testenv/kuma-monitor.mjs's must carry the same fields"
        );
        for (key, seeded) in &fields {
            let mine = &ours[key];
            if key == "name" || key == "url" {
                // The caller's two, which the seed takes from its argv.
                assert!(mine.is_string(), "{key} must be the caller's own");
                continue;
            }
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(seeded).unwrap_or_else(|_| panic!(
                    "kuma-monitor.mjs's {key} is not a JSON value: {seeded}"
                )),
                *mine,
                "{key} disagrees with testenv/kuma-monitor.mjs"
            );
        }
    }

    /// A create knobas will not send, refused before Kuma is asked.
    ///
    /// The direction that matters is the URL's. Kuma takes an `add` with a URL
    /// of `"not a url"` **and stores it**, then reports the monitor down for
    /// ever with a message about the fetch -- so a bad URL that got through
    /// here would not be an error a reader could act on, it would be a broken
    /// check somebody has to find and delete.
    #[test]
    fn a_create_knobas_cannot_deliver_is_refused_here() {
        for (name, url) in [
            ("", "http://gitea:3000/"),
            ("   ", "http://gitea:3000/"),
            ("gitea", ""),
            ("gitea", "gitea:3000"),
            ("gitea", "127.0.0.1:3000"),
            ("gitea", "ftp://gitea:3000/"),
            // A scheme Kuma cannot fetch, and the one a reader is most likely
            // to paste out of the address bar of a file they opened.
            ("gitea", "file:///etc/hosts"),
        ] {
            assert!(
                matches!(document(name, url), Err(SourceError::Protocol { .. })),
                "({name:?}, {url:?}) must not read as a monitor knobas can create"
            );
        }
    }

    /// The two fields a reader gives, carried verbatim but for the trimming a
    /// paste needs.
    #[test]
    fn the_name_and_the_url_are_the_callers_own() {
        let doc = document("  gitea (local)  ", " http://gitea:3000/api/healthz ")
            .expect("a legal create");
        assert_eq!(doc["name"], "gitea (local)");
        assert_eq!(doc["url"], "http://gitea:3000/api/healthz");
        assert_eq!(doc["type"], TYPE);
        // A host with no path is Kuma's own `/`, which is what the server
        // stores either way -- normalising here means the mirror's URL and the
        // one a suggestion rule matches on are the same string.
        let doc = document("gitea", "http://gitea:3000").expect("a legal create");
        assert_eq!(doc["url"], "http://gitea:3000/");
    }

    /// The receipt, and the two ways it can be absent.
    #[test]
    fn the_minted_id_is_read_out_of_kumas_own_answer() {
        assert_eq!(
            minted_id(&serde_json::json!({
                "ok": true, "msg": "successAdded", "msgi18n": true, "monitorID": 31
            })),
            Some("31".to_owned())
        );
        // Not a number in some future build: still an id, and still the
        // source's own spelling.
        assert_eq!(
            minted_id(&serde_json::json!({ "ok": true, "monitorID": "31" })),
            Some("31".to_owned())
        );
        assert_eq!(minted_id(&serde_json::json!({ "ok": true })), None);
        assert_eq!(
            minted_id(&serde_json::json!({ "ok": true, "monitorID": null })),
            None
        );
    }
}
