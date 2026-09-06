//! One monitor, as the mirror holds it.
//!
//! The payload is the record a later reader resolves declared paths against
//! (#277, ADR-0007), so it carries the facts spec #427 names -- id, name, type,
//! URL, state, response time, uptime ratios, certificate days remaining -- and
//! carries them under keys of this adapter's own choosing, because `/metrics`
//! has no record shape to keep verbatim. It is a fold of four gauge families,
//! not a document Kuma would recognise, so there is nothing here to be faithful
//! *to* beyond the values themselves.
//!
//! # Every key is always present
//!
//! A field a monitor has nothing for is `null`, never absent. That is what
//! makes `payload_paths`' `state` declaration true of every item this adapter
//! emits (contract battery clause 3 reads an absent key as an adapter naming a
//! key its own records do not have), and it is what lets a reader tell "Kuma
//! said nothing" from "this payload predates the field".

use knobas_source::SyncItem;

use crate::model::{Monitor, state_word};

/// The path a monitor's own page sits at in Uptime Kuma's UI.
///
/// Read off the pinned image's own router (`path:"/dashboard/:id"` in the
/// served bundle, 2.5.3), not guessed: *Open in browser* renders from
/// `SyncItem::web_url` and nothing else (interfaces §8 P5), so a wrong path
/// here is a dead link with no second opinion anywhere downstream.
const MONITOR_PATH: &str = "dashboard";

/// The item the mirror stores for one live monitor.
///
/// `updated_at` is `None`, and deliberately: `/metrics` carries no timestamp of
/// any kind -- not the last check, not the last change -- so there is no
/// instant to report. Stamping the poll's own clock would make every monitor
/// look freshly modified on every run, which is a fact knobas would have made
/// up. The row's provenance is `synced_at`, which the engine writes.
pub(crate) fn to_sync_item(source_id: &str, base_url: &str, monitor: &Monitor) -> SyncItem {
    let name = monitor.name.clone().unwrap_or_else(|| monitor.id.clone());
    let state = monitor.state_code.and_then(state_word);
    SyncItem {
        entity: knobas_core::entity::EntityRef::new(source_id, &monitor.id),
        kind: crate::KIND_MONITOR.to_owned(),
        title: name.clone(),
        body_text: body_text(&name, state, monitor),
        // A monitor has no author: nobody wrote it, a machine reports it.
        author: None,
        updated_at: None,
        payload: payload(monitor, state),
        web_url: Some(format!("{base_url}/{MONITOR_PATH}/{}", monitor.id)),
        deleted: false,
    }
}

/// The item that says a monitor knobas held is no longer in `/metrics`.
///
/// `name` is what the last run recorded, because the monitor itself is gone and
/// there is nothing left to read a title off. No `web_url`: a monitor Kuma no
/// longer publishes has no page left to open.
pub(crate) fn tombstone(source_id: &str, id: &str, name: &str) -> SyncItem {
    SyncItem {
        entity: knobas_core::entity::EntityRef::new(source_id, id),
        kind: crate::KIND_MONITOR.to_owned(),
        title: name.to_owned(),
        body_text: String::new(),
        author: None,
        updated_at: None,
        payload: serde_json::json!({
            "id": id,
            "name": name,
            // Present and null, like every other key here: the declaration
            // says a monitor's state lives at `state`, and a tombstone is
            // still a monitor.
            "state": serde_json::Value::Null,
        }),
        web_url: None,
        deleted: true,
    }
}

/// What the full-text index holds, and therefore what the launcher searches
/// and shows under the title.
///
/// The name and the **state** first, because *the launcher finds a monitor and
/// its state* is what spec #427's story 52 asks for and a launcher row shows a
/// title and an excerpt of this. Then the type and the address, so that
/// searching for a host or a port finds the check watching it -- which is also
/// what makes `to_tsvector`'s lexing of a URL into host and path useful here.
///
/// **The state is here to be *shown*, not to be searched for**, and the
/// difference is worth knowing before somebody types `kuma down` and files a
/// bug. `websearch_to_tsquery('english', …)` drops `up` and `down` as
/// stopwords, so a query that is only a state lexes to the empty tsquery and
/// matches nothing. Measured, and pinned by
/// `crates/knobas-app/tests/adapter_to_mirror.rs`'s
/// `a_kuma_monitor_reaches_the_mirror_and_the_launcher_finds_it_by_name`. The
/// state still belongs in this string: it is the excerpt the launcher draws
/// under the title, which is the half of story 52 that is deliverable.
fn body_text(name: &str, state: Option<&str>, monitor: &Monitor) -> String {
    [
        Some(name),
        state,
        monitor.monitor_type.as_deref(),
        monitor.url.as_deref(),
        monitor.hostname.as_deref(),
        monitor.port.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
}

fn payload(monitor: &Monitor, state: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": monitor.id,
        "name": monitor.name,
        "type": monitor.monitor_type,
        "url": monitor.url,
        "hostname": monitor.hostname,
        "port": monitor.port,
        // The word for the reader and the declaration, the code for the record.
        // A code this adapter has no word for leaves `state` null rather than
        // inventing one, and `state_code` is then the only thing that can say
        // what Kuma actually answered.
        "state": state,
        "state_code": monitor.state_code,
        "response_time_ms": monitor.response_time_ms,
        "uptime": monitor.uptime,
        "cert_days_remaining": monitor.cert_days_remaining,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::parse;
    use crate::model::monitors;

    const BODY: &str = "\
monitor_status{monitor_id=\"8\",monitor_name=\"canary\",monitor_type=\"http\",monitor_url=\"http://host.docker.internal:8299/\",monitor_hostname=\"null\",monitor_port=\"null\"} 1
monitor_response_time{monitor_id=\"8\",monitor_name=\"canary\",monitor_type=\"http\",monitor_url=\"http://host.docker.internal:8299/\",monitor_hostname=\"null\",monitor_port=\"null\"} 13
monitor_uptime_ratio{monitor_id=\"8\",monitor_name=\"canary\",monitor_type=\"http\",monitor_url=\"http://host.docker.internal:8299/\",monitor_hostname=\"null\",monitor_port=\"null\",window=\"1d\"} 0.98
monitor_status{monitor_id=\"1\",monitor_name=\"knobas-teamcity\",monitor_type=\"ping\",monitor_url=\"null\",monitor_hostname=\"46.224.117.158\",monitor_port=\"null\"} 0
monitor_status{monitor_id=\"99\",monitor_name=\"future\",monitor_type=\"http\",monitor_url=\"https://example.invalid/\",monitor_hostname=\"null\",monitor_port=\"null\"} 7
";

    fn item(id: &str) -> SyncItem {
        let folded = monitors(&parse(BODY));
        let monitor = folded.iter().find(|m| m.id == id).unwrap();
        to_sync_item("kuma", "http://127.0.0.1:3001", monitor)
    }

    /// The whole record spec #427 asks a monitor to carry, at keys a later
    /// reader can declare a path to.
    #[test]
    fn a_monitor_carries_what_the_spec_names() {
        let it = item("8");
        assert_eq!(it.entity.to_string(), "kuma:8");
        assert_eq!(it.kind, "monitor");
        assert_eq!(it.title, "canary");
        assert_eq!(it.payload["id"], "8");
        assert_eq!(it.payload["name"], "canary");
        assert_eq!(it.payload["type"], "http");
        assert_eq!(it.payload["url"], "http://host.docker.internal:8299/");
        assert_eq!(it.payload["state"], "up");
        assert_eq!(it.payload["state_code"], 1.0);
        assert_eq!(it.payload["response_time_ms"], 13.0);
        assert_eq!(it.payload["uptime"]["1d"], 0.98);
        assert_eq!(it.payload["cert_days_remaining"], serde_json::Value::Null);
        assert!(!it.deleted);
    }

    /// *The launcher finds a monitor and its state* (story 52): the name and
    /// the state are the first thing the indexed text says, so a launcher row
    /// shows the state under the title and `kuma down` finds what is down.
    #[test]
    fn the_indexed_text_leads_with_the_name_and_the_state() {
        assert_eq!(
            item("8").body_text,
            "canary up http http://host.docker.internal:8299/"
        );
        assert_eq!(
            item("1").body_text,
            "knobas-teamcity down ping 46.224.117.158"
        );
    }

    /// *Open in browser* renders from `web_url` and nothing else, and Kuma's
    /// own router puts a monitor at `/dashboard/:id`.
    #[test]
    fn a_monitor_links_to_its_own_page_in_kuma() {
        assert_eq!(
            item("8").web_url.as_deref(),
            Some("http://127.0.0.1:3001/dashboard/8")
        );
    }

    /// A state code this adapter has no word for: `state` is null, the raw
    /// code is kept, and the indexed text simply says less. Nothing here
    /// invents a word for a state Kuma has not published.
    #[test]
    fn an_unknown_state_code_is_a_miss_and_not_a_guess() {
        let it = item("99");
        assert_eq!(it.payload["state"], serde_json::Value::Null);
        assert_eq!(it.payload["state_code"], 7.0);
        assert_eq!(it.body_text, "future http https://example.invalid/");
        // The key is still there. An absent one would be this adapter naming a
        // key its own records do not have (battery clause 3).
        assert!(it.payload.as_object().unwrap().contains_key("state"));
    }

    /// A tombstone still has to render: the name comes from what the last run
    /// recorded, and there is no page left to open.
    #[test]
    fn a_tombstone_keeps_the_name_and_drops_the_link() {
        let gone = tombstone("kuma", "9", "knobas-live-scratch");
        assert!(gone.deleted);
        assert_eq!(gone.entity.to_string(), "kuma:9");
        assert_eq!(gone.title, "knobas-live-scratch");
        assert_eq!(gone.web_url, None);
        assert!(gone.payload.as_object().unwrap().contains_key("state"));
    }
}
