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
//! Three fields: the **name**, the **kind** of check and **what it checks**
//! (#864 added the kind). Everything else is fixed, and each of the groups is
//! fixed for its own reason.
//!
//! * **The kind picks Kuma's `type` and the fields it reads.** An HTTP check
//!   is `http` with a `url`; a TCP check is Kuma's `port` with a `hostname`
//!   and a `port`; a ping is `ping` with a `hostname`; a Docker check is
//!   `docker` with a `docker_container` and a `docker_host`. The HTTP document
//!   is the seed's own; the other three are it with the URL swapped for their
//!   own fields, read off the pinned image's `monitor.js` (2.5.3).
//! * **A Docker check names a Docker host, and knobas does not ask for one.**
//!   Kuma pushes the account's Docker hosts at login (`dockerHostList`); the
//!   create takes the first, lowest id, and an account with none is refused
//!   here with the reason, since a Docker monitor with no host is created and
//!   then down for ever.
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

use knobas_source::{MonitorKind, SourceError};

/// Kuma's `type` for an HTTP check, the seed's own. See the module note.
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
#[cfg(test)]
pub(crate) fn document(name: &str, url: &str) -> Result<serde_json::Value, SourceError> {
    document_of(name, MonitorKind::Http, url, None)
}

/// The document `add` is emitted with, for a check of `kind` called `name`
/// watching `target` (#864): an http(s) URL, `host:port`, a host, or a
/// container's name. `docker_hosts` is what the login pushed, `None` when it
/// pushed nothing; only a Docker check reads it.
///
/// # Errors
///
/// [`SourceError::Protocol`] for a check knobas will not send, for the
/// reason the HTTP-only `document` gives: a blank name or target, a target the
/// kind cannot read, or a Docker check on an account with no Docker host.
pub(crate) fn document_of(
    name: &str,
    kind: MonitorKind,
    target: &str,
    docker_hosts: Option<&[i64]>,
) -> Result<serde_json::Value, SourceError> {
    let (name, mut fields) = checked(name, kind, target)?;
    if kind == MonitorKind::Docker {
        let host = docker_hosts
            .and_then(|hosts| hosts.first().copied())
            .ok_or_else(|| {
                SourceError::protocol(
                    "this Uptime Kuma has no Docker host, so it cannot watch a container -- add \
                 one in Kuma's settings, or pick another kind of check"
                        .to_owned(),
                )
            })?;
        if let Some(map) = fields.as_object_mut() {
            map.insert("docker_host".into(), host.into());
        }
    }
    Ok(with_common(name, fields))
}

/// The name, trimmed, and the kind's own fields: everything [`document_of`]
/// can refuse before a session opens. A Docker check's host is the one thing
/// left out, since which hosts exist is known only inside the session.
///
/// # Errors
///
/// [`document_of`]'s, but the missing Docker host.
pub(crate) fn checked<'a>(
    name: &'a str,
    kind: MonitorKind,
    target: &str,
) -> Result<(&'a str, serde_json::Value), SourceError> {
    let name = name.trim();
    let url = target.trim();
    if name.is_empty() {
        return Err(SourceError::protocol(
            "an Uptime Kuma monitor needs a name -- it is also what attaches it to an asset"
                .to_owned(),
        ));
    }
    let fields = match kind {
        MonitorKind::Http => http_fields(url)?,
        MonitorKind::Tcp => {
            let (host, port) = host_and_port(url)?;
            serde_json::json!({ "type": "port", "hostname": host, "port": port })
        }
        MonitorKind::Ping => serde_json::json!({ "type": "ping", "hostname": host_of(url)? }),
        MonitorKind::Docker => serde_json::json!({
            "type": "docker",
            "docker_container": plain(url, "a Docker check needs a container's name")?,
        }),
    };
    Ok((name, fields))
}

/// `fields` with the name and every field fixed for every kind (the module
/// note's schedule and switches) added.
fn with_common(name: &str, mut fields: serde_json::Value) -> serde_json::Value {
    let common = serde_json::json!({
        "name": name,
        "interval": INTERVAL_SECS,
        "retryInterval": RETRY_INTERVAL_SECS,
        "resendInterval": 0,
        "maxretries": 0,
        "timeout": TIMEOUT_SECS,
        // Kuma runs `.every(...)` over this for every monitor of every type.
        "accepted_statuscodes": ["200-299"],
        "active": true,
        "expiryNotification": false,
        "ignoreTls": false,
        "upsideDown": false,
        "notificationIDList": {},
        "conditions": [],
    });
    if let (Some(map), Some(common)) = (fields.as_object_mut(), common.as_object()) {
        for (key, value) in common {
            map.insert(key.clone(), value.clone());
        }
    }
    fields
}

/// A target with no whitespace and nothing in it, or the protocol fault
/// `missing` names.
fn plain<'a>(target: &'a str, missing: &str) -> Result<&'a str, SourceError> {
    if target.is_empty() || target.chars().any(char::is_whitespace) {
        return Err(SourceError::protocol(format!(
            "{missing}, and {target:?} is not one"
        )));
    }
    Ok(target)
}

/// A host to ping: no scheme, no path, no port.
fn host_of(target: &str) -> Result<&str, SourceError> {
    let host = plain(target, "a ping needs a host")?;
    // One colon is `host:port`; an IPv6 address has several.
    if host.contains('/') || host.matches(':').count() == 1 {
        return Err(SourceError::protocol(format!(
            "a ping needs a host with no scheme, port or path, and {host:?} is not one"
        )));
    }
    Ok(host.trim_start_matches('[').trim_end_matches(']'))
}

/// `host:port` (or `[v6]:port`) for a TCP check.
fn host_and_port(target: &str) -> Result<(&str, u16), SourceError> {
    let target = plain(target, "a TCP check needs host:port")?;
    let refused = || {
        SourceError::protocol(format!(
            "a TCP check needs host:port, and {target:?} is not one"
        ))
    };
    let (host, port) = target.rsplit_once(':').ok_or_else(refused)?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() || host.contains('/') {
        return Err(refused());
    }
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(refused)?;
    Ok((host, port))
}

/// The HTTP check's own fields; with [`with_common`]'s, the seed's document.
fn http_fields(url: &str) -> Result<serde_json::Value, SourceError> {
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
            // The parsed URL and not the caller's string: a paste carrying a
            // trailing space or a missing path is normalised once, here, so the
            // monitor knobas asks for is the monitor Kuma reports back -- and
            // `monitor_url_host`'s suggestion rule reads that same URL out of the
            // mirror (#478).
            "url": parsed.as_str(),
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

    /// The other three kinds (#864): Kuma's `type` and the fields it reads
    /// for each, with every fixed field the HTTP document carries but the
    /// three that only an HTTP check reads.
    #[test]
    fn each_kind_names_its_own_type_and_fields() {
        let tcp = document_of("pg", MonitorKind::Tcp, " db.internal:5432 ", None).unwrap();
        assert_eq!(tcp["type"], "port");
        assert_eq!(tcp["hostname"], "db.internal");
        assert_eq!(tcp["port"], 5432);
        assert_eq!(tcp["accepted_statuscodes"], serde_json::json!(["200-299"]));
        assert_eq!(tcp["active"], true);
        assert!(tcp.get("url").is_none(), "{tcp}");

        let v6 = document_of("pg", MonitorKind::Tcp, "[fd00::5]:5432", None).unwrap();
        assert_eq!(v6["hostname"], "fd00::5");

        let ping = document_of("vm", MonitorKind::Ping, "10.0.0.4", None).unwrap();
        assert_eq!(ping["type"], "ping");
        assert_eq!(ping["hostname"], "10.0.0.4");
        assert_eq!(ping["name"], "vm");

        let docker = document_of(
            "worker",
            MonitorKind::Docker,
            "payout-worker",
            Some(&[3, 7]),
        )
        .unwrap();
        assert_eq!(docker["type"], "docker");
        assert_eq!(docker["docker_container"], "payout-worker");
        assert_eq!(docker["docker_host"], 3);

        let http = document_of("gitea", MonitorKind::Http, "http://gitea:3000", None).unwrap();
        assert_eq!(http, document("gitea", "http://gitea:3000").unwrap());
    }

    /// A target its kind cannot read, and a Docker check with no Docker host
    /// to ask, are refused before Kuma is asked (#864).
    #[test]
    fn a_target_its_kind_cannot_read_is_refused_here() {
        for (kind, target) in [
            (MonitorKind::Tcp, "db.internal"),
            (MonitorKind::Tcp, "db.internal:0"),
            (MonitorKind::Tcp, "db.internal:99999"),
            (MonitorKind::Tcp, ":5432"),
            (MonitorKind::Tcp, "http://db:5432"),
            (MonitorKind::Ping, ""),
            (MonitorKind::Ping, "vm 1"),
            (MonitorKind::Ping, "vm-1:22"),
            (MonitorKind::Ping, "http://vm-1"),
            (MonitorKind::Docker, ""),
        ] {
            assert!(
                matches!(
                    document_of("n", kind, target, Some(&[1])),
                    Err(SourceError::Protocol { .. })
                ),
                "{kind:?} {target:?} must be refused"
            );
        }
        for hosts in [None, Some(&[][..])] {
            let refused = document_of("n", MonitorKind::Docker, "worker", hosts).unwrap_err();
            assert!(refused.to_string().contains("no Docker host"), "{refused}");
        }
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
