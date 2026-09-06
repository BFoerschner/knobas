//! One monitor, folded out of the sample lines that mention it.
//!
//! `/metrics` is not a list of monitors: it is four families of gauges, each
//! carrying the monitor's identity in its labels. `monitor_status` says what a
//! monitor is doing, `monitor_response_time` how long its last check took,
//! `monitor_uptime_ratio` three sliding windows, and
//! `monitor_cert_days_remaining` the certificate countdown of a TLS check. So a
//! monitor is what a *set* of samples sharing a `monitor_id` adds up to, and
//! this module does the adding up.
//!
//! # What Kuma's exposition means, measured rather than assumed
//!
//! Read off the pinned image (2.5.3) on 2026-09-06, and re-asserted against the
//! real server by `tests/live_kuma.rs`:
//!
//! * **`monitor_id` is the identity.** Kuma v2 labels every series with it;
//!   a monitor's *name* is renameable and its URL editable, so the id is what
//!   the mirror keys on.
//! * **`"null"` is how Kuma writes a label a monitor has nothing for** -- the
//!   literal four letters, not an absent label. A ping monitor carries
//!   `monitor_url="null"` and an HTTP one `monitor_hostname="null"`. Reading
//!   that as a URL would put the word *null* on screen as an address.
//! * **`-1` is the response time of a check that did not answer.** A sentinel,
//!   not a duration, and the mirror carries it as "no response time" rather
//!   than as minus one millisecond.
//! * **A monitor with no `monitor_status` sample has no state**, and the state
//!   is `null` rather than a guess. Kuma sets the gauge on every heartbeat, so
//!   this is the shape of a monitor whose first beat has not landed.
//! * **A paused monitor is not in `/metrics` at all.** Measured: pausing the
//!   scratch monitor removed all eight of its lines and resuming put them
//!   back. Through this channel *paused* and *deleted* are the same
//!   observation, which is why [`crate::cursor`] tombstones on absence and says
//!   so.

use std::collections::BTreeMap;

use crate::metrics::Sample;

/// The families this adapter reads. Everything else in the body -- the node.js
/// runtime gauges, the express histograms -- is another program's business.
pub(crate) const STATUS: &str = "monitor_status";
pub(crate) const RESPONSE_TIME: &str = "monitor_response_time";
pub(crate) const UPTIME_RATIO: &str = "monitor_uptime_ratio";
pub(crate) const CERT_DAYS_REMAINING: &str = "monitor_cert_days_remaining";

/// The gauge Kuma publishes its own version through, read by
/// `Source::test_connection` so the sources view can say which Kuma answered.
pub(crate) const APP_VERSION: &str = "app_version";

/// The label a monitor's identity rides on.
const ID: &str = "monitor_id";
/// The label that says a monitor has nothing for this field.
const ABSENT: &str = "null";

/// One monitor as `/metrics` describes it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Monitor {
    /// Kuma's own monitor id, and the key half of this item's `EntityRef`.
    pub(crate) id: String,
    pub(crate) name: Option<String>,
    pub(crate) monitor_type: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) hostname: Option<String>,
    pub(crate) port: Option<String>,
    /// `monitor_status` as Kuma wrote it, before it is read as a word.
    pub(crate) state_code: Option<f64>,
    /// Milliseconds, or `None` for the `-1` a check that did not answer gets.
    pub(crate) response_time_ms: Option<f64>,
    /// Window (`1d`, `30d`, `365d`) to ratio, as Kuma's `window` label names
    /// them. A map rather than three fields: the windows are Kuma's choice and
    /// a future release adding a fourth should widen the payload, not need a
    /// new field here.
    pub(crate) uptime: BTreeMap<String, f64>,
    pub(crate) cert_days_remaining: Option<f64>,
}

/// What `monitor_status` means, in Kuma's own words.
///
/// The mapping is on the metric's own `# HELP` line: *Monitor Status (1 = UP,
/// 0= DOWN, 2= PENDING, 3= MAINTENANCE)*. Lowercased because the mirror's
/// state words are knobas' vocabulary in the glossary (**Monitor**: down, warn,
/// up) and Kuma's UI capitalises for display, not in its data.
///
/// **`None` for a code this adapter does not know**, which is the ADR-0007
/// miss: the raw code stays in the payload beside it, so nothing is lost, and
/// nothing invents a word for a state Kuma added after this was written.
/// *warn* is not here and must not be: it is knobas-derived from the response
/// time at sample time (spec #427, story 56) and Kuma has no such state.
pub(crate) fn state_word(code: f64) -> Option<&'static str> {
    // `as i64` after an exactness check: the gauge is an integer written as a
    // float, and a value that is not one is a Kuma this mapping has not seen.
    if code.fract() != 0.0 {
        return None;
    }
    match code as i64 {
        0 => Some("down"),
        1 => Some("up"),
        2 => Some("pending"),
        3 => Some("maintenance"),
        _ => None,
    }
}

/// Fold every sample that names a monitor into one [`Monitor`] each, ordered by
/// id.
///
/// Ordered because the emitted corpus is compared against the cursor's, and two
/// runs of one unchanged Kuma must produce the same bytes; ordering by id
/// rather than by the order Kuma listed them is what makes that true, since
/// `/metrics` orders its series by nothing in particular.
///
/// A sample with no `monitor_id` label contributes nothing: it is either
/// another program's gauge or a Kuma that stopped labelling its series, and
/// folding it into a monitor keyed on the empty string would mirror an item
/// that does not exist.
pub(crate) fn monitors(samples: &[Sample]) -> Vec<Monitor> {
    let mut by_id: BTreeMap<String, Monitor> = BTreeMap::new();

    for sample in samples {
        let Some(id) = sample.label(ID) else { continue };
        if !matches!(
            sample.name.as_str(),
            STATUS | RESPONSE_TIME | UPTIME_RATIO | CERT_DAYS_REMAINING
        ) {
            continue;
        }
        let monitor = by_id.entry(id.to_owned()).or_insert_with(|| Monitor {
            id: id.to_owned(),
            name: None,
            monitor_type: None,
            url: None,
            hostname: None,
            port: None,
            state_code: None,
            response_time_ms: None,
            uptime: BTreeMap::new(),
            cert_days_remaining: None,
        });

        // Identity travels on every family, so it is taken from whichever
        // sample arrives first and never overwritten -- one monitor cannot
        // disagree with itself, and re-reading it per family would only make
        // the result depend on which family Kuma printed last.
        monitor.name.get_or_insert_with(|| present(sample, "monitor_name").unwrap_or_default());
        if monitor.monitor_type.is_none() {
            monitor.monitor_type = present(sample, "monitor_type");
        }
        if monitor.url.is_none() {
            monitor.url = present(sample, "monitor_url");
        }
        if monitor.hostname.is_none() {
            monitor.hostname = present(sample, "monitor_hostname");
        }
        if monitor.port.is_none() {
            monitor.port = present(sample, "monitor_port");
        }

        match sample.name.as_str() {
            STATUS => monitor.state_code = Some(sample.value),
            // The sentinel, turned into the absence it means.
            RESPONSE_TIME => {
                monitor.response_time_ms = (sample.value >= 0.0).then_some(sample.value);
            }
            UPTIME_RATIO => {
                if let Some(window) = sample.label("window") {
                    monitor.uptime.insert(window.to_owned(), sample.value);
                }
            }
            CERT_DAYS_REMAINING => monitor.cert_days_remaining = Some(sample.value),
            _ => {}
        }
    }

    // A name of the empty string is the `get_or_insert_with` default above: a
    // monitor every one of whose samples was unlabelled. Kept as `None` so the
    // mapping has one thing to check rather than two.
    by_id
        .into_values()
        .map(|mut m| {
            if m.name.as_deref() == Some("") {
                m.name = None;
            }
            m
        })
        .collect()
}

/// One label, as a value the monitor actually has: absent, blank, or Kuma's
/// literal `"null"` all read as nothing.
fn present(sample: &Sample, key: &str) -> Option<String> {
    let value = sample.label(key)?.trim();
    (!value.is_empty() && value != ABSENT).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::parse;

    /// A transcription of what the real Kuma answered on 2026-09-06, trimmed to
    /// two monitors and the four families. `tests/live_kuma.rs` is what keeps
    /// it honest.
    const BODY: &str = "\
# HELP monitor_uptime_ratio Uptime ratio calculated over sliding window
# TYPE monitor_uptime_ratio gauge
monitor_uptime_ratio{monitor_id=\"7\",monitor_name=\"gitea\",monitor_type=\"http\",monitor_url=\"http://gitea:3000/api/healthz\",monitor_hostname=\"null\",monitor_port=\"null\",window=\"1d\"} 1
monitor_uptime_ratio{monitor_id=\"7\",monitor_name=\"gitea\",monitor_type=\"http\",monitor_url=\"http://gitea:3000/api/healthz\",monitor_hostname=\"null\",monitor_port=\"null\",window=\"30d\"} 0.5
monitor_uptime_ratio{monitor_id=\"1\",monitor_name=\"knobas-teamcity\",monitor_type=\"ping\",monitor_url=\"null\",monitor_hostname=\"46.224.117.158\",monitor_port=\"null\",window=\"1d\"} 1
# HELP monitor_response_time Monitor Response Time (ms)
# TYPE monitor_response_time gauge
monitor_response_time{monitor_id=\"7\",monitor_name=\"gitea\",monitor_type=\"http\",monitor_url=\"http://gitea:3000/api/healthz\",monitor_hostname=\"null\",monitor_port=\"null\"} 35
monitor_response_time{monitor_id=\"1\",monitor_name=\"knobas-teamcity\",monitor_type=\"ping\",monitor_url=\"null\",monitor_hostname=\"46.224.117.158\",monitor_port=\"null\"} -1
# HELP monitor_status Monitor Status (1 = UP, 0= DOWN, 2= PENDING, 3= MAINTENANCE)
# TYPE monitor_status gauge
monitor_status{monitor_id=\"7\",monitor_name=\"gitea\",monitor_type=\"http\",monitor_url=\"http://gitea:3000/api/healthz\",monitor_hostname=\"null\",monitor_port=\"null\"} 1
monitor_status{monitor_id=\"1\",monitor_name=\"knobas-teamcity\",monitor_type=\"ping\",monitor_url=\"null\",monitor_hostname=\"46.224.117.158\",monitor_port=\"null\"} 0
# HELP process_open_fds Number of open file descriptors.
# TYPE process_open_fds gauge
process_open_fds 23
";

    fn folded() -> Vec<Monitor> {
        monitors(&parse(BODY))
    }

    /// Four families, one monitor: the whole point of the fold.
    #[test]
    fn a_monitor_is_every_family_that_names_it() {
        let gitea = folded().into_iter().find(|m| m.id == "7").unwrap();
        assert_eq!(gitea.name.as_deref(), Some("gitea"));
        assert_eq!(gitea.monitor_type.as_deref(), Some("http"));
        assert_eq!(gitea.url.as_deref(), Some("http://gitea:3000/api/healthz"));
        assert_eq!(gitea.state_code, Some(1.0));
        assert_eq!(gitea.response_time_ms, Some(35.0));
        assert_eq!(gitea.uptime.get("1d"), Some(&1.0));
        assert_eq!(gitea.uptime.get("30d"), Some(&0.5));
        // The estate's checks are plain HTTP and ping, so the certificate
        // family has no series at all -- an absence, never a zero.
        assert_eq!(gitea.cert_days_remaining, None);
    }

    /// Kuma's literal `"null"` for a field a monitor of that type has not got.
    /// Read as a value, a ping monitor's URL becomes the word *null* and the
    /// detail view offers to open it.
    #[test]
    fn kumas_literal_null_label_is_an_absence() {
        let ping = folded().into_iter().find(|m| m.id == "1").unwrap();
        assert_eq!(ping.hostname.as_deref(), Some("46.224.117.158"));
        assert_eq!(ping.url, None);
        assert_eq!(ping.port, None);
    }

    /// `-1` is what Kuma writes for a check that did not answer. Carried as a
    /// duration it is a monitor that responded in minus one millisecond.
    #[test]
    fn the_response_time_sentinel_is_an_absence() {
        let ping = folded().into_iter().find(|m| m.id == "1").unwrap();
        assert_eq!(ping.state_code, Some(0.0));
        assert_eq!(ping.response_time_ms, None);
    }

    /// Everything that is not a monitor family stays out, and a sample with no
    /// `monitor_id` cannot conjure a monitor keyed on nothing.
    #[test]
    fn only_the_monitor_families_make_monitors() {
        let ids: Vec<_> = folded().into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec!["1", "7"], "ordered by id, and nothing else in");
    }

    /// The `# HELP` line's own mapping, and a refusal to invent a word for a
    /// code Kuma has not published.
    #[test]
    fn the_state_words_are_the_help_lines_own() {
        assert_eq!(state_word(0.0), Some("down"));
        assert_eq!(state_word(1.0), Some("up"));
        assert_eq!(state_word(2.0), Some("pending"));
        assert_eq!(state_word(3.0), Some("maintenance"));
        assert_eq!(state_word(4.0), None, "a state this adapter has not met");
        assert_eq!(state_word(1.5), None, "not an integer code at all");
    }
}
