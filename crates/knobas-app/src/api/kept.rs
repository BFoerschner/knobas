//! What was sent and answered since knobas started (ux31, section 20): the
//! last [`LINES`] sends, their bodies bounded by [`BODY_BYTES`] in all, the
//! oldest body dropped first while its line stays. In the app process's memory
//! only: nothing of a request or an answer reaches the database, the mirror, a
//! log or a file, so no backup or share export carries any of it. A request is
//! kept as the page sent it, which never holds a secret's value.

use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex, PoisonError};

use knobas_http::person::Bytes;
use serde::Serialize;

use super::send::{ApiAnswer, ApiSend};

/// How many sends are kept.
pub const LINES: usize = 50;
/// How many bytes of answers are kept in all.
pub const BODY_BYTES: usize = 64 * 1024 * 1024;

/// One send: what went, what came back, and its body while it is kept.
pub struct Sent {
    pub at_ms: u64,
    pub send: ApiSend,
    pub answer: ApiAnswer,
    pub body: Option<Bytes>,
}

/// One line of *Sent since knobas started*.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "api.ts")]
pub struct SentLine {
    pub id: u64,
    pub at_ms: u64,
    pub send: ApiSend,
    pub answer: ApiAnswer,
    /// The body is still kept and can be drawn again.
    pub kept: bool,
}

/// The kept sends, the oldest first.
#[derive(Default)]
pub struct Kept {
    sent: VecDeque<Sent>,
    bytes: usize,
}

impl Kept {
    /// Keeps a send, dropping the oldest line past [`LINES`] and the oldest
    /// bodies past [`BODY_BYTES`].
    pub fn keep(&mut self, sent: Sent) {
        self.bytes += sent.body.as_ref().map_or(0, Bytes::len);
        self.sent.push_back(sent);
        while self.sent.len() > LINES {
            if let Some(gone) = self.sent.pop_front() {
                self.bytes -= gone.body.as_ref().map_or(0, Bytes::len);
            }
        }
        for line in &mut self.sent {
            if self.bytes <= BODY_BYTES {
                break;
            }
            if let Some(body) = line.body.take() {
                self.bytes -= body.len();
            }
        }
    }

    /// The body of send `id`, while kept.
    #[must_use]
    pub fn body(&self, id: u64) -> Option<&Bytes> {
        self.sent
            .iter()
            .find(|s| s.send.id == id)
            .and_then(|s| s.body.as_ref())
    }

    /// The answer of send `id`.
    #[must_use]
    pub fn answer(&self, id: u64) -> Option<&ApiAnswer> {
        self.sent
            .iter()
            .find(|s| s.send.id == id)
            .map(|s| &s.answer)
    }

    /// The lines of the sends of `file`, the newest first.
    #[must_use]
    pub fn lines(&self, file: &str) -> Vec<SentLine> {
        self.sent
            .iter()
            .rev()
            .filter(|s| s.send.file == file)
            .map(|s| SentLine {
                id: s.send.id,
                at_ms: s.at_ms,
                send: s.send.clone(),
                answer: s.answer.clone(),
                kept: s.body.is_some(),
            })
            .collect()
    }

    /// How many lines, and how many bytes of bodies, are kept.
    #[must_use]
    pub fn size(&self) -> (usize, usize) {
        (self.sent.len(), self.bytes)
    }
}

/// The app's kept sends.
pub static KEPT: LazyLock<Mutex<Kept>> = LazyLock::new(Mutex::default);

/// The kept sends, locked.
pub fn kept() -> std::sync::MutexGuard<'static, Kept> {
    KEPT.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{BODY_BYTES, Kept, LINES, Sent};
    use crate::api::send::{ApiAnswer, ApiSend};
    use knobas_http::person::Bytes;

    fn sent(id: u64, file: &str, body: usize) -> Sent {
        Sent {
            at_ms: id,
            send: ApiSend {
                id,
                file: file.into(),
                ..ApiSend::default()
            },
            answer: ApiAnswer::Failed {
                id,
                stage: "connect".into(),
                words: String::new(),
            },
            body: Some(Bytes::from(vec![b'x'; body])),
        }
    }

    #[test]
    fn the_fifty_first_send_drops_the_first_line() {
        let mut kept = Kept::default();
        for id in 0..=LINES as u64 {
            kept.keep(sent(id, "a.yaml", 10));
        }
        let lines = kept.lines("a.yaml");
        assert_eq!(lines.len(), LINES);
        assert_eq!(lines.last().map(|l| l.id), Some(1));
        assert_eq!(kept.size().1, LINES * 10);
    }

    /// The bound is 64 MiB: two answers of 20 MiB are both kept, a third of
    /// 30 MiB takes the oldest body (the ux30 end gate's mutants: a bound of
    /// about 1 MiB passed the test below, which scales with it).
    #[test]
    fn answers_up_to_64_mib_in_all_are_kept() {
        let mut kept = Kept::default();
        kept.keep(sent(0, "a.yaml", 20 << 20));
        kept.keep(sent(1, "a.yaml", 20 << 20));
        assert!(kept.body(0).is_some() && kept.body(1).is_some());
        kept.keep(sent(2, "a.yaml", 30 << 20));
        assert!(kept.body(0).is_none(), "70 MiB is past the bound");
        assert!(kept.body(1).is_some() && kept.body(2).is_some());
    }

    #[test]
    fn bodies_past_the_bound_drop_oldest_first_and_keep_their_lines() {
        let mut kept = Kept::default();
        let third = BODY_BYTES / 3 + 1;
        for id in 0..3 {
            kept.keep(sent(id, "a.yaml", third));
        }
        // Three thirds and a byte over: the oldest body goes, its line stays.
        assert!(kept.body(0).is_none());
        assert!(kept.body(1).is_some() && kept.body(2).is_some());
        let lines = kept.lines("a.yaml");
        assert_eq!(
            lines.iter().map(|l| (l.id, l.kept)).collect::<Vec<_>>(),
            [(2, true), (1, true), (0, false)]
        );
        assert!(kept.size().1 <= BODY_BYTES);
        assert!(kept.lines("b.yaml").is_empty());
    }
}
