//! How loud a value is drawn: the five words a source may use (#666, #660).
//!
//! The shell owns the colours; an adapter only says which of these a value
//! is. A row's state chip (`payload::StateChip`) carries one, and so does the
//! source-view vocabulary of ADR-0020 (#661), which is why it lives here and
//! not in either of them: `knobas-source` depends on this crate, and a
//! declared path in `payload` has to name a tone without depending back.

use serde::{Deserialize, Serialize};

/// What a value means to a reader, in the shell's five words.
///
/// * `ok`: done and good (a merged pull request, a passing build).
/// * `warn`: needs a look, nothing has failed yet.
/// * `err`: failed (a failing build).
/// * `info`: in motion or open (an open pull request, a running build).
/// * `muted`: over and of no further interest (a closed pull request).
///
/// Serialized lowercase, the spelling the TypeScript mirror and a declared
/// path's value map both use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "entity.ts")]
pub enum Tone {
    Ok,
    Warn,
    Err,
    Info,
    Muted,
}

#[cfg(test)]
mod tests {
    use super::Tone;

    /// The five words on the wire, exactly: the SQL resolver copies them out
    /// of the declaration as text, and the frontend's `Tone` union is these.
    #[test]
    fn the_five_tones_travel_lowercase() {
        let wire: Vec<String> = [Tone::Ok, Tone::Warn, Tone::Err, Tone::Info, Tone::Muted]
            .iter()
            .map(|t| {
                serde_json::to_value(t)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect();
        assert_eq!(wire, ["ok", "warn", "err", "info", "muted"]);
    }
}
