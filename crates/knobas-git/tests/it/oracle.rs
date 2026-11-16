//! The `git` on `PATH` as the reference, whatever its version.
//!
//! One answer of git's changed while this reader was written: with blanks
//! ignored (`-w`), `git diff --raw` lists since git 2.52 only the files in
//! which something is left to show, where an older git lists every changed
//! path. The reader prints what git prints now. Where the git on `PATH` is
//! older (Apple's 2.50.1), [`since_2_52`] makes the newer answer out of the
//! older git's own output, so the comparison holds on every machine; on a
//! newer git (the CI runners') it is git's output as it is.

use std::collections::HashMap;
use std::process::Command;

/// The git on `PATH`'s version, `(2, 50)` for `git version 2.50.1 (Apple
/// Git-155)` and `(2, 56)` for `git version 2.56.0.windows.4`.
fn version() -> (u32, u32) {
    let out = Command::new("git")
        .arg("--version")
        .output()
        .expect("git runs");
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut numbers = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u32>().unwrap_or(0));
    (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0))
}

/// Whether this git leaves a file out of `--raw` when `-w` leaves nothing of
/// its change to show.
pub fn raw_is_content_aware() -> bool {
    version() >= (2, 52)
}

/// What a git since 2.52 prints for `--raw -z -w` (and `--numstat` with
/// `numstat`), made from what an older git printed for `--raw --numstat -z
/// -w`: its raw records less those of a file whose mode did not change and
/// whose counts say nothing is left (no numstat record at all for a changed
/// file, `0 0` for a renamed one, a binary one whose two blobs are the same),
/// then its numstat records as they are.
pub fn since_2_52(older: &[u8], numstat: bool) -> Vec<u8> {
    let tokens: Vec<&[u8]> = older.split(|b| *b == 0).collect();
    // The raw records, each with the bytes it spans, then the counts by path.
    let mut records: Vec<(Vec<u8>, String, String)> = Vec::new();
    let mut at = 0;
    while at < tokens.len() && tokens[at].first() == Some(&b':') {
        let meta = String::from_utf8_lossy(tokens[at]).into_owned();
        let status = meta.split(' ').nth(4).unwrap_or_default().to_owned();
        let paths = if status.starts_with('R') || status.starts_with('C') {
            2
        } else {
            1
        };
        let mut bytes = Vec::new();
        for token in &tokens[at..=at + paths] {
            bytes.extend_from_slice(token);
            bytes.push(0);
        }
        let path = String::from_utf8_lossy(tokens[at + paths]).into_owned();
        records.push((bytes, meta, path));
        at += paths + 1;
    }
    let counts_from = at;
    let mut counts: HashMap<String, (String, String)> = HashMap::new();
    while at < tokens.len() && !tokens[at].is_empty() {
        let text = String::from_utf8_lossy(tokens[at]).into_owned();
        let mut fields = text.splitn(3, '\t');
        let (added, removed, path) = (
            fields.next().unwrap_or_default().to_owned(),
            fields.next().unwrap_or_default().to_owned(),
            fields.next().unwrap_or_default().to_owned(),
        );
        if path.is_empty() {
            // A rename: the old path, then the new.
            counts.insert(
                String::from_utf8_lossy(tokens[at + 2]).into_owned(),
                (added, removed),
            );
            at += 3;
        } else {
            counts.insert(path, (added, removed));
            at += 1;
        }
    }
    let mut out = Vec::new();
    for (bytes, meta, path) in &records {
        let fields: Vec<&str> = meta.trim_start_matches(':').split(' ').collect();
        let (old_mode, new_mode, old_blob, new_blob, status) =
            (fields[0], fields[1], fields[2], fields[3], fields[4]);
        let shown = matches!(status.as_bytes()[0], b'A' | b'D' | b'T')
            || old_mode != new_mode
            || match counts.get(path) {
                None => false,
                Some((added, _)) if added == "-" => old_blob != new_blob,
                Some((added, removed)) => added != "0" || removed != "0",
            };
        if shown {
            out.extend_from_slice(bytes);
        }
    }
    if numstat {
        for token in &tokens[counts_from..tokens.len().saturating_sub(1)] {
            out.extend_from_slice(token);
            out.push(0);
        }
    }
    out
}
