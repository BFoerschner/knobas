//! Reading Prometheus' text exposition format, to the depth `/metrics` needs.
//!
//! Uptime Kuma has no REST API for reading monitors -- the dashboard is
//! socket.io -- and `/metrics` is the one channel an API key opens (spec #427,
//! *The Kuma adapter*). So the adapter's read path is a Prometheus scrape, and
//! this module is the scraper.
//!
//! # Why hand-rolled, and how much of the format it claims
//!
//! A Prometheus client library would bring a parser, a registry, an encoder and
//! a metrics model this crate has no use for; what is needed is one function
//! from a response body to `(name, labels, value)` triples. So this parses the
//! format's **sample lines** and nothing else: `# HELP` and `# TYPE` are
//! skipped, exemplars and the optional per-sample timestamp are ignored, and
//! histogram and summary suffixes are just metric names like any other.
//!
//! What it does take seriously is **quoting**. A label value is a quoted string
//! with `\\`, `\"` and `\n` escapes, and a monitor is named by whoever added
//! it: `jira (tunnel)` is in the seeded estate today, and a monitor called
//! `a"b}` is a name a person may type tomorrow. Splitting a label set on `,`
//! and `}` -- the obvious shortcut -- reads that name as a truncated one and
//! silently mirrors the wrong monitor, so the label set is scanned rather than
//! split.
//!
//! # What a malformed line does
//!
//! It is dropped, and the rest of the body is read. The alternative -- failing
//! the sync -- would make one unreadable line of a 250-line body cost the whole
//! corpus, and `/metrics` is a document knobas does not control the shape of:
//! it carries node.js runtime gauges beside the monitor families, and a future
//! Kuma may add anything to it. The families this adapter reads are named
//! explicitly ([`crate::model`]), so an unreadable line that matters shows up
//! as a monitor missing a field rather than as a mirror of nonsense.

use std::collections::BTreeMap;

/// One sample line: a metric name, its labels, and its value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sample {
    pub(crate) name: String,
    pub(crate) labels: BTreeMap<String, String>,
    pub(crate) value: f64,
}

impl Sample {
    /// One label, or `None` where the exposition did not carry it.
    pub(crate) fn label(&self, key: &str) -> Option<&str> {
        self.labels.get(key).map(String::as_str)
    }
}

/// Every readable sample line in a Prometheus text body, in the order it
/// appeared.
///
/// Order is kept because the caller folds samples into monitors and takes the
/// first identity it sees for each, so "in the order it appeared" is part of
/// what the folding means.
pub(crate) fn parse(body: &str) -> Vec<Sample> {
    body.lines().filter_map(parse_line).collect()
}

/// One line, or `None` for a comment, a blank line, or anything unreadable.
fn parse_line(line: &str) -> Option<Sample> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    let bytes: Vec<char> = line.chars().collect();
    let mut at = 0;
    let name = take_name(&bytes, &mut at)?;
    if name.is_empty() {
        return None;
    }

    let labels = if bytes.get(at) == Some(&'{') {
        at += 1;
        take_labels(&bytes, &mut at)?
    } else {
        BTreeMap::new()
    };

    // The value, and then whatever follows it. A sample line may carry a
    // trailing millisecond timestamp; it is skipped rather than parsed,
    // because nothing here has any use for the scrape's own clock and Kuma
    // does not emit one.
    while bytes.get(at).is_some_and(|c| c.is_whitespace()) {
        at += 1;
    }
    let start = at;
    while bytes.get(at).is_some_and(|c| !c.is_whitespace()) {
        at += 1;
    }
    let token: String = bytes[start..at].iter().collect();
    // `f64::from_str` takes `NaN`, `inf`, `+Inf` and `-Inf` in every casing the
    // format allows, so the special values need no arm of their own.
    let value: f64 = token.parse().ok()?;

    Some(Sample {
        name,
        labels,
        value,
    })
}

/// A metric or label name: `[a-zA-Z_:][a-zA-Z0-9_:]*`, as far as it goes.
fn take_name(chars: &[char], at: &mut usize) -> Option<String> {
    let start = *at;
    while chars
        .get(*at)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ':')
    {
        *at += 1;
    }
    (start < *at).then(|| chars[start..*at].iter().collect())
}

/// The label set, from just after `{` to just after the matching `}`.
///
/// `None` for a label set that does not close, a label with no `=`, or a value
/// that is not a quoted string -- each of which is a line this module has no
/// honest reading of.
fn take_labels(chars: &[char], at: &mut usize) -> Option<BTreeMap<String, String>> {
    let mut labels = BTreeMap::new();
    loop {
        while chars
            .get(*at)
            .is_some_and(|c| c.is_whitespace() || *c == ',')
        {
            *at += 1;
        }
        match chars.get(*at)? {
            '}' => {
                *at += 1;
                return Some(labels);
            }
            _ => {
                let key = take_name(chars, at)?;
                if chars.get(*at) != Some(&'=') {
                    return None;
                }
                *at += 1;
                if chars.get(*at) != Some(&'"') {
                    return None;
                }
                *at += 1;
                let value = take_quoted(chars, at)?;
                labels.insert(key, value);
            }
        }
    }
}

/// A quoted label value, from just after the opening `"` to just after the
/// closing one.
///
/// The three escapes the format defines, and nothing else: an unknown escape
/// keeps its backslash, which is what Prometheus' own parser does and what
/// makes a Windows path in a label survive a round trip.
fn take_quoted(chars: &[char], at: &mut usize) -> Option<String> {
    let mut out = String::new();
    loop {
        match chars.get(*at)? {
            '"' => {
                *at += 1;
                return Some(out);
            }
            '\\' => {
                *at += 1;
                match chars.get(*at)? {
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    'n' => out.push('\n'),
                    other => {
                        out.push('\\');
                        out.push(*other);
                    }
                }
                *at += 1;
            }
            other => {
                out.push(*other);
                *at += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape every monitor family arrives in, read off the pinned image
    /// (2.5.3) on 2026-09-06 and re-asserted against the real server by
    /// `tests/live_kuma.rs`.
    #[test]
    fn reads_a_kuma_sample_line() {
        let samples = parse(
            "monitor_status{monitor_id=\"7\",monitor_name=\"gitea\",monitor_type=\"http\",\
             monitor_url=\"http://gitea:3000/api/healthz\",monitor_hostname=\"null\",\
             monitor_port=\"null\"} 1",
        );
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].name, "monitor_status");
        assert_eq!(samples[0].label("monitor_id"), Some("7"));
        assert_eq!(samples[0].label("monitor_name"), Some("gitea"));
        assert_eq!(
            samples[0].label("monitor_url"),
            Some("http://gitea:3000/api/healthz")
        );
        // The literal four letters, not a JSON null: what Kuma writes for a
        // label a monitor of that type has no value for. Turning it into an
        // absence is `crate::model`'s job and not this one's.
        assert_eq!(samples[0].label("monitor_hostname"), Some("null"));
        assert!((samples[0].value - 1.0).abs() < f64::EPSILON);
    }

    /// `# HELP`, `# TYPE` and blank lines carry no samples, and a family with
    /// no series at all -- which is what `monitor_cert_days_remaining` is on an
    /// estate of plain HTTP and ping checks -- contributes nothing rather than
    /// a zero.
    #[test]
    fn skips_comments_and_blank_lines() {
        let body = "# HELP monitor_cert_days_remaining Days until the certificate expires\n\
                    # TYPE monitor_cert_days_remaining gauge\n\
                    \n\
                    monitor_status{monitor_id=\"1\"} 0\n";
        let samples = parse(body);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].name, "monitor_status");
        assert!(samples[0].value.abs() < f64::EPSILON);
    }

    /// **The reason this is a scanner and not a `split(',')`.** A monitor is
    /// named by whoever added it, and the estate already holds
    /// `jira (tunnel)`; a name carrying a quote, a comma or a closing brace is
    /// one keystroke away. Split on the punctuation and such a monitor is
    /// mirrored under a truncated name -- silently, because a truncated name
    /// is still a name.
    #[test]
    fn a_label_value_may_hold_the_punctuation_the_format_uses() {
        let samples = parse("monitor_status{monitor_name=\"a\\\"b}, c\",monitor_id=\"9\"} 1");
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].label("monitor_name"), Some("a\"b}, c"));
        assert_eq!(samples[0].label("monitor_id"), Some("9"));
    }

    /// The other two escapes, and the rule for one this parser does not know:
    /// keep the backslash rather than eat it, which is Prometheus' own answer
    /// and what makes `C:\Users` survive.
    #[test]
    fn the_escapes_are_the_three_the_format_defines() {
        let samples = parse("m{a=\"x\\\\y\",b=\"one\\ntwo\",c=\"C:\\Users\"} 1");
        assert_eq!(samples[0].label("a"), Some("x\\y"));
        assert_eq!(samples[0].label("b"), Some("one\ntwo"));
        assert_eq!(samples[0].label("c"), Some("C:\\Users"));
    }

    /// A sample with no labels at all -- `process_cpu_seconds_total 504.69`,
    /// which shares the body with the monitor families.
    #[test]
    fn reads_a_sample_with_no_labels() {
        let samples = parse("process_cpu_seconds_total 504.69390599999997");
        assert_eq!(samples.len(), 1);
        assert!(samples[0].labels.is_empty());
        assert!((samples[0].value - 504.693_906).abs() < 1e-6);
    }

    /// The sentinels the format allows, and the negative Kuma writes for the
    /// response time of a monitor that is down.
    #[test]
    fn reads_the_values_the_format_allows() {
        let samples = parse("a 1\nb -1\nc 0.8034557235421166\nd NaN\ne +Inf\nf -Inf\ng 1.5e-3\n");
        let by = |n: &str| samples.iter().find(|s| s.name == n).unwrap().value;
        assert!((by("b") + 1.0).abs() < f64::EPSILON);
        assert!((by("c") - 0.803_455_723_542_116_6).abs() < f64::EPSILON);
        assert!(by("d").is_nan());
        assert!(by("e").is_infinite() && by("e").is_sign_positive());
        assert!(by("f").is_infinite() && by("f").is_sign_negative());
        assert!((by("g") - 0.0015).abs() < f64::EPSILON);
    }

    /// A trailing timestamp is part of the format and Kuma does not write one.
    /// It is skipped rather than parsed: nothing here has a use for the
    /// scrape's clock, and a line carrying one must still read.
    #[test]
    fn a_trailing_timestamp_does_not_break_the_line() {
        let samples = parse("monitor_status{monitor_id=\"1\"} 1 1788691089000");
        assert_eq!(samples.len(), 1);
        assert!((samples[0].value - 1.0).abs() < f64::EPSILON);
    }

    /// **A malformed line is dropped and the rest of the body is read.** The
    /// body carries node.js gauges beside the monitor families and knobas does
    /// not control its shape, so one unreadable line must not cost the corpus.
    #[test]
    fn an_unreadable_line_costs_only_itself() {
        let body = "monitor_status{monitor_id=\"1\"} 1\n\
                    monitor_status{unclosed=\"x\" 1\n\
                    monitor_status{bad=notquoted} 1\n\
                    monitor_status{monitor_id=\"2\"} not-a-number\n\
                    {monitor_id=\"3\"} 1\n\
                    monitor_status{monitor_id=\"4\"} 0\n";
        let samples = parse(body);
        let ids: Vec<_> = samples
            .iter()
            .filter_map(|s| s.label("monitor_id"))
            .collect();
        assert_eq!(ids, vec!["1", "4"], "{samples:?}");
    }
}
