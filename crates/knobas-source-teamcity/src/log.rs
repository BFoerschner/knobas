//! A build's whole log, read as it streams (#854): a range of its lines and
//! its steps, never the whole of it in memory.
//!
//! `downloadBuildLog.html` answers the plain-text log the build log page's
//! *Download* link saves. Each line the server stamped reads
//! `[HH:MM:SS]S:<tabs> <message>`: `S` is the status (`E` an error, `W` a
//! warning, ` ` or `i` neither), each tab one level of the build's block
//! tree. A step opens with a top-level `Step N/M: <name>` line, and the lines
//! inside it carry `[Step N/M]` in front of their message. A line with no
//! stamp is the rest of a message that spans lines, and belongs where the
//! line before it does.

use std::collections::VecDeque;

use knobas_source::Tone;
use knobas_source::run::{LOG_LINE_CHARS_MAX, LogLine, LogStep, RunLog};

/// The longest unfinished line kept while it streams, in bytes; the rest is
/// dropped before it is cut to [`LOG_LINE_CHARS_MAX`] characters anyway.
const PENDING_BYTES_MAX: usize = 4 * LOG_LINE_CHARS_MAX;

/// Reads one log as it arrives, chunk by chunk.
pub(crate) struct LogReader {
    from: Option<u64>,
    count: usize,
    total: u64,
    kept: VecDeque<LogLine>,
    steps: Vec<LogStep>,
    /// The step the last line was in, and its `[Step N/M]` marker.
    open: Option<(u32, String)>,
    /// The time of the step's last stamped line, for its duration.
    step_started: Option<u64>,
    step_last_time: Option<u64>,
    pending: Vec<u8>,
    pending_cut: bool,
}

impl LogReader {
    /// `count` lines from line `from` (from 1), or the last `count` when
    /// `from` is `None`.
    pub(crate) fn new(from: Option<u64>, count: u32) -> Self {
        Self {
            from: from.map(|f| f.max(1)),
            count: count as usize,
            total: 0,
            kept: VecDeque::new(),
            steps: Vec::new(),
            open: None,
            step_started: None,
            step_last_time: None,
            pending: Vec::new(),
            pending_cut: false,
        }
    }

    /// The next bytes of the log.
    pub(crate) fn feed(&mut self, mut bytes: &[u8]) {
        while let Some(at) = bytes.iter().position(|b| *b == b'\n') {
            self.push_pending(&bytes[..at]);
            let line = std::mem::take(&mut self.pending);
            let cut = std::mem::replace(&mut self.pending_cut, false);
            self.line(&line, cut);
            bytes = &bytes[at + 1..];
        }
        self.push_pending(bytes);
    }

    fn push_pending(&mut self, bytes: &[u8]) {
        let room = PENDING_BYTES_MAX.saturating_sub(self.pending.len());
        if bytes.len() > room {
            self.pending_cut = true;
        }
        self.pending
            .extend_from_slice(&bytes[..bytes.len().min(room)]);
    }

    /// The log has ended.
    pub(crate) fn finish(mut self) -> RunLog {
        if !self.pending.is_empty() || self.pending_cut {
            let line = std::mem::take(&mut self.pending);
            let cut = self.pending_cut;
            self.line(&line, cut);
        }
        self.close_step();
        RunLog {
            lines: self.kept.into(),
            steps: self.steps,
            total: self.total,
        }
    }

    fn line(&mut self, raw: &[u8], cut: bool) {
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        let text = String::from_utf8_lossy(raw);
        let parsed = parse(&text);
        self.total += 1;
        let number = self.total;
        let seconds = parsed.time.as_deref().and_then(seconds_of);

        if parsed.stamped && parsed.depth == 0 {
            self.close_step();
            if let Some((marker, name)) = step_heading(&parsed.message) {
                let index = u32::try_from(self.steps.len()).unwrap_or(u32::MAX);
                self.steps.push(LogStep {
                    name,
                    first: number,
                    last: number,
                    seconds: None,
                    tone: None,
                });
                self.open = Some((index, marker));
                self.step_started = seconds;
            }
        }
        let step = self.open.as_ref().map(|(index, _)| *index);
        let mut message = parsed.message.as_str();
        if let Some((_, marker)) = &self.open
            && let Some(rest) = message.strip_prefix(marker.as_str())
        {
            message = rest.strip_prefix(' ').unwrap_or(rest);
        }
        if let Some(index) = step {
            let s = &mut self.steps[index as usize];
            s.last = number;
            s.tone = match (s.tone, parsed.tone) {
                (Some(Tone::Err), _) | (_, Some(Tone::Err)) => Some(Tone::Err),
                (Some(Tone::Warn), _) | (_, Some(Tone::Warn)) => Some(Tone::Warn),
                _ => None,
            };
            if seconds.is_some() {
                self.step_last_time = seconds;
            }
        }
        let wanted = match self.from {
            Some(from) => number >= from && number < from.saturating_add(self.count as u64),
            None => true,
        };
        if !wanted || self.count == 0 {
            return;
        }
        self.kept.push_back(LogLine {
            number,
            time: parsed.time,
            text: capped(message, cut),
            tone: parsed.tone,
            step,
        });
        if self.from.is_none() && self.kept.len() > self.count {
            self.kept.pop_front();
        }
    }

    fn close_step(&mut self) {
        if let Some((index, _)) = self.open.take()
            && let (Some(start), Some(end)) = (self.step_started, self.step_last_time)
        {
            // A stamp is a time of day: a step across midnight wraps.
            let took = if end >= start {
                end - start
            } else {
                end + 86_400 - start
            };
            self.steps[index as usize].seconds = Some(took);
        }
        self.step_started = None;
        self.step_last_time = None;
    }
}

/// One line, read.
struct Parsed {
    stamped: bool,
    time: Option<String>,
    tone: Option<Tone>,
    depth: usize,
    message: String,
}

/// `[10:15:30]E:\t\t [Step 1/2] text` is stamped at `10:15:30`, an error, two
/// levels down; anything else is a line with no stamp, as it is.
fn parse(line: &str) -> Parsed {
    let plain = || Parsed {
        stamped: false,
        time: None,
        tone: None,
        depth: 0,
        message: line.trim_end().to_owned(),
    };
    let Some(rest) = line.strip_prefix('[') else {
        return plain();
    };
    let Some(close) = rest.find(']').filter(|at| *at <= 32) else {
        return plain();
    };
    let time = &rest[..close];
    if !time.chars().any(|c| c.is_ascii_digit()) || time.contains(['[', ']']) {
        return plain();
    }
    let after = &rest[close + 1..];
    let mut chars = after.chars();
    let (Some(status), Some(':')) = (chars.next(), chars.next()) else {
        return plain();
    };
    let body = chars.as_str();
    let depth = body.chars().take_while(|c| *c == '\t').count();
    let body = &body[depth..];
    let body = body.strip_prefix(' ').unwrap_or(body);
    Parsed {
        stamped: true,
        time: Some(time.trim().to_owned()),
        tone: match status {
            'E' | 'F' => Some(Tone::Err),
            'W' => Some(Tone::Warn),
            _ => None,
        },
        depth,
        message: body.trim_end().to_owned(),
    }
}

/// `Step 2/5: cargo test (Command Line) (1m:02s)` opens step 2 named `cargo
/// test`; the lines inside it start `[Step 2/5]`.
fn step_heading(message: &str) -> Option<(String, String)> {
    let rest = message.strip_prefix("Step ")?;
    let (counter, name) = rest.split_once(": ")?;
    let (n, m) = counter.split_once('/')?;
    if n.is_empty()
        || m.is_empty()
        || !n.chars().all(|c| c.is_ascii_digit())
        || !m.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some((
        format!("[Step {counter}]"),
        without_runner(without_duration(name)).to_owned(),
    ))
}

/// A heading's name without the runner TeamCity appends to a named step:
/// `IntegrationTests (Command Line)` is `IntegrationTests`, the name its
/// configuration gives it (#1016). Only the last parenthesis goes, matched
/// to its opening one, so `Build (release) (Gradle)` keeps its words and
/// `Compile (Visual Studio (sln))` loses the whole runner. An unnamed step's
/// heading is the runner alone; one whose runner ends in a parenthesis
/// (`Visual Studio (sln)`) loses it, which the record cannot tell apart.
fn without_runner(name: &str) -> &str {
    let mut depth = 0usize;
    for (at, c) in name.char_indices().rev() {
        match c {
            ')' => depth += 1,
            '(' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    return match name[..at].strip_suffix(' ') {
                        Some(kept) if !kept.is_empty() => kept,
                        _ => name,
                    };
                }
            }
            _ if depth == 0 => return name,
            _ => {}
        }
    }
    name
}

/// A heading's name without the duration TeamCity may append: `cargo test
/// (1m:02s)` is `cargo test`. Only a parenthesis of digits and `h m s : . <`
/// is a duration; `Build (release)` keeps its words.
fn without_duration(name: &str) -> &str {
    let name = name.trim_end();
    let Some(open) = name.rfind(" (") else {
        return name;
    };
    let inner = &name[open + 2..];
    let Some(inner) = inner.strip_suffix(')') else {
        return name;
    };
    let duration = inner.chars().any(|c| c.is_ascii_digit())
        && inner
            .chars()
            .all(|c| c.is_ascii_digit() || " hms:.<".contains(c));
    if duration { &name[..open] } else { name }
}

/// `10:15:30` as seconds into the day; `None` for a stamp of another shape.
fn seconds_of(time: &str) -> Option<u64> {
    let clock = time.rsplit(' ').next()?;
    let mut parts = clock.split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let s = s.split('.').next()?;
    Some(h.parse::<u64>().ok()? * 3600 + m.parse::<u64>().ok()? * 60 + s.parse::<u64>().ok()?)
}

/// At most [`LOG_LINE_CHARS_MAX`] characters, ending in `…` where cut.
fn capped(text: &str, cut: bool) -> String {
    match text.char_indices().nth(LOG_LINE_CHARS_MAX) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None if cut => format!("{text}…"),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A log in the shape `downloadBuildLog.html` answers: a preamble, two
    /// steps (the second with a warning and an error), a message that spans
    /// lines, and a closing line outside every step.
    const LOG: &str = "[10:05:58]i: TeamCity server version is 2026.1.3\n\
[10:05:58] : Collecting changes in 1 VCS root\n\
[10:05:58] :\t [Collecting changes in 1 VCS root] VCS Root details\n\
[10:06:00] : Step 1/2: Checkout (2s)\n\
[10:06:00] :\t [Step 1/2] Updating sources\n\
[10:06:02] :\t [Step 1/2] Done in 2 s\n\
[10:06:03] : Step 2/2: cargo test\n\
[10:06:04]W:\t [Step 2/2] warning: unused variable\n\
[10:06:05] :\t [Step 2/2] thread 'retry' panicked at tests/sepa_retry.rs:14:5:\n\
assertion `left == right` failed\n\
[10:07:25]E:\t [Step 2/2] Process exited with code 101\n\
[10:07:26]E: Build finished with problems\n";

    fn read(from: Option<u64>, count: u32, chunk: usize) -> RunLog {
        let mut reader = LogReader::new(from, count);
        for piece in LOG.as_bytes().chunks(chunk) {
            reader.feed(piece);
        }
        reader.finish()
    }

    #[test]
    fn a_log_splits_into_its_steps_with_their_lines_durations_and_tones() {
        let log = read(None, 100, 7);
        assert_eq!(log.total, 12);
        assert_eq!(
            log.steps,
            [
                LogStep {
                    name: "Checkout".into(),
                    first: 4,
                    last: 6,
                    seconds: Some(2),
                    tone: None,
                },
                LogStep {
                    name: "cargo test".into(),
                    first: 7,
                    last: 11,
                    seconds: Some(82),
                    tone: Some(Tone::Err),
                },
            ]
        );
        let line = |n: u64| log.lines.iter().find(|l| l.number == n).unwrap();
        assert_eq!(line(3).step, None, "a preamble block is in no step");
        assert_eq!(
            line(5).text,
            "Updating sources",
            "the step marker is not drawn"
        );
        assert_eq!(line(5).time.as_deref(), Some("10:06:00"));
        assert_eq!(line(8).tone, Some(Tone::Warn));
        assert_eq!(line(10).text, "assertion `left == right` failed");
        assert_eq!(
            (line(10).step, line(10).time.as_deref()),
            (Some(1), None),
            "a continuation stays in its step"
        );
        assert_eq!(line(11).tone, Some(Tone::Err));
        assert_eq!(
            (line(12).step, line(12).tone),
            (None, Some(Tone::Err)),
            "the closing line is outside the steps"
        );
    }

    #[test]
    fn a_range_keeps_only_its_lines_and_still_knows_every_step() {
        let tail = read(None, 3, 5);
        assert_eq!(
            tail.lines.iter().map(|l| l.number).collect::<Vec<_>>(),
            [10, 11, 12]
        );
        assert_eq!(tail.steps.len(), 2);
        assert_eq!(tail.total, 12);
        let head = read(Some(4), 2, 64);
        assert_eq!(
            head.lines.iter().map(|l| l.number).collect::<Vec<_>>(),
            [4, 5]
        );
        assert_eq!(head.lines[0].text, "Step 1/2: Checkout (2s)");
        assert_eq!(head.total, 12);
        let none = read(Some(40), 10, 64);
        assert!(none.lines.is_empty());
    }

    #[test]
    fn a_range_from_past_the_end_is_empty_and_does_not_overflow() {
        let log = read(Some(u64::MAX), 20_000, 64);
        assert!(log.lines.is_empty());
        assert_eq!(log.total, 12);
    }

    #[test]
    fn a_line_longer_than_the_cap_is_cut_and_says_so() {
        let mut reader = LogReader::new(None, 10);
        let long = "x".repeat(PENDING_BYTES_MAX * 3);
        reader.feed(format!("[10:00:00] : {long}\nnext").as_bytes());
        let log = reader.finish();
        assert_eq!(log.total, 2);
        assert_eq!(log.lines[0].text.chars().count(), LOG_LINE_CHARS_MAX + 1);
        assert!(log.lines[0].text.ends_with('…'));
        assert_eq!(
            log.lines[1].text, "next",
            "a last line with no newline is a line"
        );
    }

    #[test]
    fn a_heading_loses_only_a_duration_in_parentheses() {
        assert_eq!(without_duration("cargo test (1m:02s)"), "cargo test");
        assert_eq!(without_duration("Build (release)"), "Build (release)");
        assert_eq!(without_duration("Publish (< 1s)"), "Publish");
        assert_eq!(
            step_heading("Step 10/12: Deploy").unwrap().0,
            "[Step 10/12]"
        );
        assert!(step_heading("Step x/2: no").is_none());
        assert!(step_heading("Stepping out").is_none());
    }

    /// TeamCity heads a named step `<name> (<runner>)`, as the seeded server
    /// does (`IntegrationTests (Command Line)`), and an unnamed one by the
    /// runner alone. The step is named as its configuration names it (#1016),
    /// so the run page can tell which configured steps a run has not reached.
    #[test]
    fn a_heading_names_the_step_without_its_runner() {
        let name = |heading: &str| step_heading(heading).unwrap().1;
        assert_eq!(
            name("Step 1/1: IntegrationTests (Command Line)"),
            "IntegrationTests"
        );
        assert_eq!(
            name("Step 2/3: Build (release) (Gradle) (1m:02s)"),
            "Build (release)"
        );
        assert_eq!(name("Step 3/3: Command Line"), "Command Line");
        // A runner whose own name ends in a parenthesis goes whole.
        assert_eq!(
            name("Step 1/2: Compile (Visual Studio (sln)) (12s)"),
            "Compile"
        );
        assert_eq!(
            name("Step 2/2: Deploy (prod) (Command Line)"),
            "Deploy (prod)"
        );
        // Unbalanced: no runner to take off.
        assert_eq!(name("Step 1/1: odd)"), "odd)");
    }
}
