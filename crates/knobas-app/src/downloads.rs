//! Where a run's downloads land (D8, #854, #856): a file in the downloads
//! folder, written as the source streams it.
//!
//! The name is the source's suggestion, and the source is not trusted with a
//! path: only its last segment is kept, stripped of anything a file system
//! would read as more than a name, and a name already taken gets ` (1)`,
//! ` (2)` before its extension. A download that fails leaves no file.

use std::path::{Path, PathBuf};

use knobas_source::SourceError;
use knobas_source::run::DownloadSink;
use tokio::io::AsyncWriteExt;

/// The longest name kept, in bytes: room left under 255 for ` (999)`.
const NAME_BYTES_MAX: usize = 200;

/// A file name from `suggested`: its last path segment, without control
/// characters, `:` or a leading `.`, `download` when nothing is left.
#[must_use]
pub fn safe_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut cleaned = String::new();
    for c in last
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        // A file system counts a name's bytes (255 on APFS and ext4), so the
        // cap is in bytes, cut on a character.
        if cleaned.len() + c.len_utf8() > NAME_BYTES_MAX {
            break;
        }
        cleaned.push(c);
    }
    // Dots and spaces together, so `. .` cannot leave a `.` behind.
    let cleaned = cleaned
        .trim_start_matches(|c: char| c == '.' || c.is_whitespace())
        .trim_end();
    if cleaned.is_empty() {
        "download".to_owned()
    } else {
        cleaned.to_owned()
    }
}

/// `name` with ` (n)` before its extension: `a.log` -> `a (2).log`.
fn numbered(name: &str, n: u32) -> String {
    match name.rsplit_once('.').filter(|(stem, _)| !stem.is_empty()) {
        Some((stem, ext)) => format!("{stem} ({n}).{ext}"),
        None => format!("{name} ({n})"),
    }
}

/// Create `name` in `dir`, or the first free one of `name (1)` to
/// `name (999)`: a name is claimed with `create_new`, so a file already there
/// is never opened, let alone truncated. Every download and the Docs export
/// (#1454) take their name here.
///
/// # Errors
///
/// The file system's, with the path it failed on; once all thousand names
/// are taken, [`std::io::ErrorKind::AlreadyExists`] on `dir`, saying so.
pub async fn create_free(
    dir: &Path,
    name: &str,
) -> Result<(PathBuf, tokio::fs::File), (PathBuf, std::io::Error)> {
    for n in 0..1000 {
        let candidate = dir.join(if n == 0 {
            name.to_owned()
        } else {
            numbered(name, n)
        });
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
            .await
        {
            Ok(file) => return Ok((candidate, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err((candidate, e)),
        }
    }
    Err((
        dir.to_owned(),
        std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "{} holds {name} and a thousand numbered copies of it",
                dir.display()
            ),
        ),
    ))
}

/// Writes one download into `dir`.
pub struct FileSink {
    dir: PathBuf,
    file: Option<(PathBuf, tokio::fs::File)>,
}

impl FileSink {
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
            file: None,
        }
    }

    /// The download is whole: flush it, and answer where it is.
    ///
    /// # Errors
    ///
    /// The file system's, as [`SourceError::Sink`]; and a download that never
    /// started (an adapter that answered without naming a file), which is the
    /// adapter's bug and not an empty file.
    pub async fn finish(&mut self) -> Result<PathBuf, SourceError> {
        match self.file.as_mut() {
            Some((path, file)) => {
                file.flush().await.map_err(|e| sink_error(path, &e))?;
                Ok(path.clone())
            }
            None => Err(SourceError::Sink(
                "the source answered the download without starting it".to_owned(),
            )),
        }
    }

    /// The download failed: take away what it wrote.
    pub async fn discard(&mut self) {
        if let Some((path, file)) = self.file.take() {
            drop(file);
            let _ = tokio::fs::remove_file(&path).await;
        }
    }
}

fn sink_error(path: &Path, error: &std::io::Error) -> SourceError {
    SourceError::Sink(format!("writing {}: {error}", path.display()))
}

#[async_trait::async_trait]
impl DownloadSink for FileSink {
    async fn start(&mut self, file_name: &str) -> Result<(), SourceError> {
        let name = safe_name(file_name);
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| sink_error(&self.dir, &e))?;
        match create_free(&self.dir, &name).await {
            Ok(file) => {
                self.file = Some(file);
                Ok(())
            }
            // Every name taken: the message says so whole.
            Err((_, e)) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(SourceError::Sink(e.to_string()))
            }
            Err((path, e)) => Err(sink_error(&path, &e)),
        }
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), SourceError> {
        let Some((path, file)) = self.file.as_mut() else {
            return Err(SourceError::Sink(
                "a download wrote before it started".to_owned(),
            ));
        };
        file.write_all(bytes)
            .await
            .map_err(|e| sink_error(path, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_suggested_name_is_only_ever_a_name() {
        assert_eq!(safe_name("junit.xml"), "junit.xml");
        assert_eq!(safe_name("../../.ssh/authorized_keys"), "authorized_keys");
        assert_eq!(safe_name("C:\\evil\\x.txt"), "x.txt");
        assert_eq!(safe_name(".bashrc"), "bashrc");
        assert_eq!(safe_name("a\u{0}b:c?.log"), "abc.log");
        assert_eq!(safe_name(".."), "download");
        assert_eq!(safe_name(""), "download");
        assert_eq!(safe_name(". ."), "download");
        assert_eq!(safe_name(" .. x"), "x");
        let long = safe_name(&"é".repeat(200));
        assert_eq!(long.len(), NAME_BYTES_MAX);
        assert_eq!(long.chars().count(), 100);
    }

    #[test]
    fn a_taken_name_is_numbered_before_its_extension() {
        assert_eq!(numbered("Payout-1187.log", 2), "Payout-1187 (2).log");
        assert_eq!(numbered("SHA256SUMS", 1), "SHA256SUMS (1)");
    }
}
