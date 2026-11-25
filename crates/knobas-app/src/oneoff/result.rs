//! A run's result (ADR-0060, sections 16 and 17): what it made that is not
//! its lines. The file under `{out}`, the files under `{outdir}`, or a stdout
//! that is not text, drawn as the file viewer draws one.
//!
//! - **As the bytes say**: PNG, JPEG, GIF, WebP and SVG are an image, `%PDF-`
//!   a PDF, `<!doctype html` or `<html` a page, text that parses as JSON under
//!   2 MiB is JSON, any other valid UTF-8 is text, and the rest bytes. **The
//!   bytes overrule a label**: a result said to be an image that is not one is
//!   drawn as what it is.
//! - **Caps**: a result past 64 MiB is refused, not cut; `{outdir}` keeps at
//!   most 5,000 files and 256 MiB, by ADR-0046's walk (`folder_render::keep`):
//!   no link followed, no `.git`, no name Windows refuses.

use std::path::{Path, PathBuf};

use knobas_core::checkout::oneoff::Shows;
use serde::Serialize;

use crate::folder_render::keep::{self, Caps, LeftOut, Made, MadeFile};

/// The most bytes a result that is one file, or a stdout that is not text, may
/// have.
pub const RESULT_CAP: u64 = 64 * 1024 * 1024;
/// The most files a run's `{outdir}` may keep.
pub const OUTDIR_FILES: usize = 5_000;
/// The most bytes a run's `{outdir}` may keep.
pub const OUTDIR_BYTES: u64 = 256 * 1024 * 1024;
/// JSON past this is drawn as text.
pub const JSON_CAP: usize = 2 * 1024 * 1024;

/// What a result is drawn as.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "OneOffResultKind")]
pub enum Kind {
    Image {
        mime: String,
    },
    Pdf,
    Page,
    Json,
    Markdown,
    /// Code, highlighted as a file of this extension.
    Code {
        ext: String,
    },
    Diff,
    Text,
    /// Bytes knobas cannot draw: a hex dump of the first 64 KiB.
    Bytes,
}

/// A run's result, as the dialog is told it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(tag = "from", rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "OneOffResult")]
pub enum RunResult {
    /// One file: the one under `{out}`, or the stdout that was not text.
    One {
        kind: Kind,
        size: u64,
        /// Its name: the file's under `{out}`, `stdout` for the stdout.
        name: String,
        /// The label overruled by the bytes, where it was.
        #[ts(optional = nullable)]
        overruled: Option<String>,
    },
    /// The files under `{outdir}`, kept.
    Files {
        files: Vec<MadeFile>,
        bytes: u64,
        left_out: Vec<LeftOut>,
        left_out_total: u32,
    },
    /// Nothing kept, and why: past a cap, no file written.
    Refused { why: String },
}

/// What the first bytes of `bytes` say, before any label.
#[must_use]
pub fn sniff(bytes: &[u8]) -> Kind {
    let image = |mime: &str| Kind::Image {
        mime: mime.to_owned(),
    };
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return image("image/png");
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return image("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return image("image/gif");
    }
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return image("image/webp");
    }
    if bytes.starts_with(b"%PDF-") {
        return Kind::Pdf;
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Kind::Bytes;
    };
    if text.contains('\0') {
        return Kind::Bytes;
    }
    let head = text.trim_start_matches('\u{feff}').trim_start();
    let lower: String = head
        .chars()
        .take(256)
        .collect::<String>()
        .to_ascii_lowercase();
    if lower.starts_with("<svg") || (lower.starts_with("<?xml") && lower.contains("<svg")) {
        return image("image/svg+xml");
    }
    if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
        return Kind::Page;
    }
    if bytes.len() <= JSON_CAP
        && (head.starts_with('{') || head.starts_with('['))
        && serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok()
    {
        return Kind::Json;
    }
    Kind::Text
}

/// What `bytes` are drawn as under `shows`, `ext` the extension of the file
/// it ran on or the result's own. The bytes overrule a label that does not
/// fit them: answers the kind and the label overruled.
#[must_use]
pub fn detect(bytes: &[u8], shows: Shows, ext: Option<&str>) -> (Kind, Option<String>) {
    let seen = sniff(bytes);
    let text = matches!(seen, Kind::Text | Kind::Json | Kind::Page);
    let asked = match shows {
        Shows::Auto | Shows::Problems => return (seen, None),
        Shows::Text => Kind::Text,
        Shows::Code => Kind::Code {
            ext: ext.unwrap_or("txt").to_ascii_lowercase(),
        },
        Shows::Diff => Kind::Diff,
        Shows::Json => Kind::Json,
        Shows::Markdown => Kind::Markdown,
        Shows::Page => Kind::Page,
        Shows::Image => Kind::Image {
            mime: String::new(),
        },
        Shows::Pdf => Kind::Pdf,
    };
    let fits = match &asked {
        Kind::Image { .. } => matches!(seen, Kind::Image { .. }),
        Kind::Pdf => seen == Kind::Pdf,
        Kind::Json => seen == Kind::Json,
        Kind::Text | Kind::Code { .. } | Kind::Diff | Kind::Markdown | Kind::Page => text,
        Kind::Bytes => true,
    };
    if fits {
        let kind = match asked {
            Kind::Image { .. } => seen,
            other => other,
        };
        (kind, None)
    } else {
        let label = serde_json::to_value(&asked)
            .ok()
            .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_owned));
        (seen, label)
    }
}

/// The one file a run wrote under `out`, and its result.
pub fn of_out(out: &Path, shows: Shows) -> (RunResult, Option<PathBuf>) {
    let files: Vec<_> = std::fs::read_dir(out)
        .map(|listed| listed.flatten().filter(|e| e.path().is_file()).collect())
        .unwrap_or_default();
    let [only] = files.as_slice() else {
        return (
            RunResult::Refused {
                why: if files.is_empty() {
                    "it wrote no file to {out}".to_owned()
                } else {
                    format!(
                        "it wrote {} files to {{out}}, where one is drawn",
                        files.len()
                    )
                },
            },
            None,
        );
    };
    let size = only.metadata().map(|m| m.len()).unwrap_or(0);
    let name = only.file_name().to_string_lossy().into_owned();
    if size > RESULT_CAP {
        return (
            RunResult::Refused {
                why: format!(
                    "it wrote {name} of {} MiB, past the {} MiB a result may have",
                    size / 1024 / 1024,
                    RESULT_CAP / 1024 / 1024
                ),
            },
            None,
        );
    }
    let Ok(bytes) = std::fs::read(only.path()) else {
        return (
            RunResult::Refused {
                why: format!("{name} could not be read"),
            },
            None,
        );
    };
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_owned());
    let (kind, overruled) = detect(&bytes, shows, ext.as_deref());
    (
        RunResult::One {
            kind,
            size,
            name,
            overruled,
        },
        Some(only.path()),
    )
}

/// The files a run wrote under `out`, kept in `into` by ADR-0046's walk.
pub fn of_outdir(out: &Path, into: &Path) -> (RunResult, Option<Made>) {
    let progress: keep::Progress = std::sync::Arc::new(|_, _| {});
    let cancel = tokio_util::sync::CancellationToken::new();
    let caps = Caps {
        files: OUTDIR_FILES,
        bytes: OUTDIR_BYTES,
    };
    match keep::keep_with(out, into, &progress, &cancel, caps, false) {
        Ok(made) => (
            RunResult::Files {
                files: made.manifest.files.clone(),
                bytes: made.manifest.bytes,
                left_out: made.manifest.left_out.clone(),
                left_out_total: made.manifest.left_out_total,
            },
            Some(made),
        ),
        Err(keep::KeepError::TooBig { files, bytes }) => (
            RunResult::Refused {
                why: format!(
                    "it wrote {files} files and {} MiB to {{outdir}}, past the {OUTDIR_FILES} files and {} MiB a run keeps",
                    bytes / 1024 / 1024,
                    OUTDIR_BYTES / 1024 / 1024
                ),
            },
            None,
        ),
        Err(keep::KeepError::Nothing) => (
            RunResult::Refused {
                why: "it wrote no file to {outdir}".to_owned(),
            },
            None,
        ),
        Err(keep::KeepError::Stopped) => (
            RunResult::Refused {
                why: "it was stopped".to_owned(),
            },
            None,
        ),
        Err(keep::KeepError::Io(why)) => (RunResult::Refused { why }, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_bytes_say_what_a_result_is() {
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        assert_eq!(
            sniff(png),
            Kind::Image {
                mime: "image/png".into()
            }
        );
        assert_eq!(
            sniff(&[0xff, 0xd8, 0xff, 0xe0]),
            Kind::Image {
                mime: "image/jpeg".into()
            }
        );
        assert_eq!(
            sniff(b"GIF89a.."),
            Kind::Image {
                mime: "image/gif".into()
            }
        );
        assert_eq!(
            sniff(b"RIFF\0\0\0\0WEBPVP8 "),
            Kind::Image {
                mime: "image/webp".into()
            }
        );
        assert_eq!(
            sniff(b"<?xml version=\"1.0\"?>\n<svg xmlns=\"x\"/>"),
            Kind::Image {
                mime: "image/svg+xml".into()
            }
        );
        assert_eq!(sniff(b"%PDF-1.7\n"), Kind::Pdf);
        assert_eq!(sniff(b"\n<!DOCTYPE html><html>"), Kind::Page);
        assert_eq!(sniff(b"<html><body>x"), Kind::Page);
        assert_eq!(sniff(b"{\"a\": [1, 2]}"), Kind::Json);
        assert_eq!(sniff(b"[1, 2"), Kind::Text);
        assert_eq!(sniff(b"plain words\n"), Kind::Text);
        assert_eq!(sniff(b"a\0b"), Kind::Bytes);
        assert_eq!(sniff(&[0xc3, 0x28]), Kind::Bytes);
        // Look-alikes: text that starts like a page, a PNG's name, a PDF's word.
        assert_eq!(sniff(b"the <html> tag"), Kind::Text);
        assert_eq!(sniff(b"PNG files"), Kind::Text);
        assert_eq!(sniff(b"PDF-1.7"), Kind::Text);
    }

    #[test]
    fn the_bytes_overrule_a_label() {
        let (kind, overruled) = detect(b"not an image", Shows::Image, None);
        assert_eq!((kind, overruled.as_deref()), (Kind::Text, Some("image")));
        let (kind, overruled) = detect(b"\x89PNG\r\n\x1a\n", Shows::Image, None);
        assert_eq!(
            (kind, overruled),
            (
                Kind::Image {
                    mime: "image/png".into()
                },
                None
            )
        );
        assert_eq!(
            detect(b"fn main() {}", Shows::Code, Some("RS")).0,
            Kind::Code { ext: "rs".into() }
        );
        assert_eq!(detect(b"# Title", Shows::Markdown, None).0, Kind::Markdown);
        assert_eq!(
            detect(b"%PDF-1", Shows::Text, None),
            (Kind::Pdf, Some("text".into()))
        );
        assert_eq!(detect(b"--- a\n+++ b", Shows::Diff, None).0, Kind::Diff);
        assert_eq!(detect(b"{}", Shows::Auto, None).0, Kind::Json);
    }

    #[test]
    fn out_holds_one_file_or_says_why() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            of_out(dir.path(), Shows::Auto).0,
            RunResult::Refused { .. }
        ));
        std::fs::write(dir.path().join("output.html"), "<!doctype html><p>hi").unwrap();
        let (result, path) = of_out(dir.path(), Shows::Auto);
        assert_eq!(
            result,
            RunResult::One {
                kind: Kind::Page,
                size: 20,
                name: "output.html".into(),
                overruled: None
            }
        );
        assert!(path.unwrap().ends_with("output.html"));
        std::fs::write(dir.path().join("b"), "x").unwrap();
        assert!(
            matches!(of_out(dir.path(), Shows::Auto).0, RunResult::Refused { why } if why.contains("2 files"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn outdir_keeps_plain_files_and_says_what_it_left_out() {
        let out = tempfile::tempdir().unwrap();
        let into = tempfile::tempdir().unwrap();
        std::fs::create_dir(out.path().join("sub")).unwrap();
        std::fs::write(out.path().join("a.txt"), "a").unwrap();
        std::fs::write(out.path().join("sub/b.json"), "{}").unwrap();
        std::fs::create_dir(out.path().join(".git")).unwrap();
        std::fs::write(out.path().join(".git/HEAD"), "x").unwrap();
        std::os::unix::fs::symlink("/etc/passwd", out.path().join("link")).unwrap();
        let (result, made) = of_outdir(out.path(), &into.path().join("kept"));
        let RunResult::Files {
            files,
            left_out_total,
            ..
        } = result
        else {
            panic!("{result:?}")
        };
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a.txt", "sub/b.json"]);
        assert_eq!(left_out_total, 2);
        assert!(made.unwrap().path_of("sub/b.json").unwrap().is_file());
    }
}
