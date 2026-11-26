//! One folder renderer's run over a copy already written (ADR-0044): the
//! argument vector, the program, its output and what kind of output it is.
//!
//! The program runs as a file renderer's does (`crate::renderers::run_argv`,
//! ADR-0033): no shell, the parent environment less every `KNOBAS_*`
//! variable, no console window on Windows, its own process group on Unix,
//! everything it started killed at the timeout or a stop. It differs in
//! three ways: stdin is closed (the folder is the input), it starts in an
//! empty folder of its own rather than the temp directory, and its output may
//! be the one file it wrote under `{out}`.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::{FolderOutput, OUT, OUTDIR, OUTPUT_CAP, PATH};
use crate::renderers::{STDERR_CAP, read_capped, stop};

/// What kind of output a run made, as the Code page draws it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum DrawnKind {
    /// An image of the type its first bytes name (`image/png`,
    /// `image/svg+xml`).
    Image {
        mime: String,
    },
    Text,
    Markdown,
    Html,
    /// Bytes knobas cannot draw: their size is said.
    Undrawable,
    /// A folder of files kept from `{outdir}` (ADR-0046): how many, their
    /// bytes, and how many entries were left out.
    Folder {
        files: u32,
        bytes: u64,
        left_out: u32,
    },
}

/// How a run ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ran {
    Made {
        bytes: Vec<u8>,
        kind: DrawnKind,
    },
    Failed {
        status: Option<i32>,
        stderr: String,
    },
    TimedOut {
        seconds: u32,
        stderr: String,
    },
    TooLarge,
    NotStarted(String),
    /// Exit 0 with `{out}` in the command and not one file written there.
    NoOutputFile {
        found: usize,
        stderr: String,
    },
    /// Exit 0 with `{outdir}` in the command: what it wrote there is read by
    /// [`super::keep`], not here.
    Wrote {
        stderr: String,
    },
    Stopped,
}

/// `command`'s words with `{path}`, `{out}` and `{outdir}` filled: `{out}`
/// is a file `output` in the folder `out`, `{outdir}` the folder itself.
///
/// # Errors
///
/// The command no longer splits.
pub fn argv(command: &str, copy: &Path, out: &Path) -> Result<Vec<String>, String> {
    let words = knobas_core::checkout::open::words(command).map_err(|e| e.to_string())?;
    let copy = copy.to_string_lossy();
    let outdir = out.to_string_lossy();
    let out = out.join("output");
    let out = out.to_string_lossy();
    Ok(words
        .into_iter()
        .map(|w| {
            w.replace(OUTDIR, &outdir)
                .replace(PATH, &copy)
                .replace(OUT, &out)
        })
        .collect())
}

/// What says which tool `command` runs, for the cache key (2026-11-24, Björn:
/// a drawing made by an older version of a tool was compared with one made
/// by the new version). For the program, and for each other word that holds
/// no placeholder and names an existing absolute file (a jar, a script): its
/// canonical path, size and modification time. The program is found as the
/// run finds it: a word with a path separator as written, else the first
/// file of that name on `search` (on Windows with `.exe` where the name has
/// no extension). A program not found is stamped by its word, so a look never
/// fails over it and the run says why as before. A wrapper script's imports,
/// `npx` or a plugin elsewhere stay unseen: *Run again* runs them anew.
#[must_use]
pub fn tool_stamp(command: &str, search: Option<&std::ffi::OsStr>) -> String {
    use std::fmt::Write as _;
    let Ok(words) = knobas_core::checkout::open::words(command) else {
        return String::new();
    };
    let mut stamp = String::new();
    for (at, word) in words.iter().enumerate() {
        if [PATH, OUT, OUTDIR].iter().any(|p| word.contains(p)) {
            continue;
        }
        let file = if at == 0 {
            program_file(word, search)
        } else {
            Path::new(word)
                .is_absolute()
                .then(|| std::path::PathBuf::from(word))
        };
        let meta = file
            .as_ref()
            .and_then(|f| std::fs::metadata(f).ok())
            .filter(std::fs::Metadata::is_file);
        match (file, meta) {
            (Some(file), Some(meta)) => {
                let path = std::fs::canonicalize(&file).unwrap_or(file);
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                let _ = write!(stamp, "{}\0{}\0{modified}\0", path.display(), meta.len());
            }
            _ if at == 0 => {
                let _ = write!(stamp, "?{word}\0");
            }
            _ => {}
        }
    }
    stamp
}

/// The file the program word `word` starts: as written where it names a
/// folder, else the first regular file of that name on `search`.
fn program_file(word: &str, search: Option<&std::ffi::OsStr>) -> Option<std::path::PathBuf> {
    if word.contains('/') || (cfg!(windows) && word.contains('\\')) {
        return Some(std::path::PathBuf::from(word));
    }
    let name = if cfg!(windows) && Path::new(word).extension().is_none() {
        format!("{word}.exe")
    } else {
        word.to_owned()
    };
    std::env::split_paths(search?)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}

/// Run `command` over the copy at `copy`, writing under `out`, started in
/// `cwd`.
pub async fn run(
    command: &str,
    output: FolderOutput,
    timeout_secs: u32,
    folders: (&Path, &Path, &Path),
    parent: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    cancel: CancellationToken,
) -> Ran {
    let (copy, out, cwd) = folders;
    let argv = match argv(command, copy, out) {
        Ok(argv) => argv,
        Err(why) => return Ran::NotStarted(format!("the command cannot be run: {why}")),
    };
    let Some((program, arguments)) = argv.split_first() else {
        return Ran::NotStarted("the command has no program in it".to_owned());
    };
    let mut built = knobas_db::console::command(program);
    knobas_db::console::scrub_knobas_env(&mut built, parent)
        .args(arguments)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut built, 0);
    let mut command_ = tokio::process::Command::from(built);
    command_.kill_on_drop(true);
    let mut child = match command_.spawn() {
        Ok(child) => child,
        Err(error) => return Ran::NotStarted(format!("{program} could not be started: {error}")),
    };
    let pid = child.id();
    if let Some(pid) = pid {
        super::runs::started(pid);
    }
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        stop(&mut child).await;
        return Ran::NotStarted("the program's streams could not be opened".to_owned());
    };
    let stderr_task = tokio::spawn(read_capped(stderr, STDERR_CAP, false));
    let wants_folder = command.contains(OUTDIR);
    let wants_file = wants_folder || command.contains(OUT);
    // A program that writes its output file prints what it likes: its stdout
    // is read and dropped past the cap, never refused.
    let collected = tokio::select! {
        () = cancel.cancelled() => None,
        timed = tokio::time::timeout(Duration::from_secs(u64::from(timeout_secs)), async {
            let read = read_capped(stdout, OUTPUT_CAP, !wants_file).await;
            match read {
                Some(stdout) => Some((stdout, child.wait().await)),
                None => None,
            }
        }) => Some(timed),
    };
    let take_stderr = |task: tokio::task::JoinHandle<Option<Vec<u8>>>| async move {
        tokio::time::timeout(Duration::from_millis(500), task)
            .await
            .ok()
            .and_then(Result::ok)
            .flatten()
            .map(lossy)
            .unwrap_or_default()
    };
    let ran = match collected {
        None => {
            stop(&mut child).await;
            stderr_task.abort();
            Ran::Stopped
        }
        Some(Err(_)) => {
            stop(&mut child).await;
            Ran::TimedOut {
                seconds: timeout_secs,
                stderr: take_stderr(stderr_task).await,
            }
        }
        Some(Ok(None)) => {
            stop(&mut child).await;
            stderr_task.abort();
            Ran::TooLarge
        }
        Some(Ok(Some((stdout, status)))) => {
            let status = status.ok().and_then(|s| s.code());
            let stderr = take_stderr(stderr_task).await;
            if wants_folder {
                // What it started may still write: killed before `{outdir}`
                // is read (ADR-0046).
                kill_left(pid).await;
            }
            if status != Some(0) {
                Ran::Failed { status, stderr }
            } else if wants_folder {
                Ran::Wrote { stderr }
            } else if wants_file {
                from_file(out, output, stderr).await
            } else {
                let kind = kind_of(&stdout, output, None);
                Ran::Made {
                    bytes: stdout,
                    kind,
                }
            }
        }
    };
    if let Some(pid) = pid {
        super::runs::ended(pid);
    }
    ran
}

/// Kill what a tool that has ended left running: its process group on Unix
/// (the leader is reaped, so its id is the group's until the group is empty;
/// "no such process" is the usual answer), `taskkill /T` on Windows, which
/// finds no tree under a parent that has ended (ADR-0046 names the gap).
async fn kill_left(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    #[cfg(unix)]
    if let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    #[cfg(windows)]
    crate::renderers::kill_tree(pid).await;
}

/// The one file a run wrote under `out`.
async fn from_file(out: &Path, output: FolderOutput, stderr: String) -> Ran {
    let out = out.to_path_buf();
    let read = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, String), usize> {
        let files: Vec<_> = std::fs::read_dir(&out)
            .map(|listed| listed.flatten().filter(|e| e.path().is_file()).collect())
            .unwrap_or_default();
        let [only] = files.as_slice() else {
            return Err(files.len());
        };
        let size = only.metadata().map(|m| m.len()).unwrap_or(0);
        if size > OUTPUT_CAP as u64 {
            return Err(usize::MAX);
        }
        let name = only.file_name().to_string_lossy().into_owned();
        std::fs::read(only.path())
            .map(|bytes| (bytes, name))
            .map_err(|_| 0)
    })
    .await;
    match read {
        Ok(Ok((bytes, name))) => {
            let ext = name
                .rsplit_once('.')
                .map(|(_, ext)| ext.to_ascii_lowercase());
            let kind = kind_of(&bytes, output, ext.as_deref());
            Ran::Made { bytes, kind }
        }
        Ok(Err(usize::MAX)) => Ran::TooLarge,
        Ok(Err(found)) => Ran::NoOutputFile { found, stderr },
        Err(_) => Ran::NoOutputFile { found: 0, stderr },
    }
}

fn lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
}

/// The image type `bytes` open with, `None` for none knobas draws. SVG is
/// text whose first element is `<svg` (after an XML declaration, a comment
/// or a doctype), not any text that opens with `<`, which HTML does too.
fn image_type(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        _ if is_svg(bytes) => Some("image/svg+xml"),
        _ => None,
    }
}

fn is_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    let Ok(text) =
        std::str::from_utf8(head).or_else(|e| std::str::from_utf8(&head[..e.valid_up_to()]))
    else {
        return false;
    };
    let mut rest = text.trim_start_matches('\u{feff}').trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("<?") {
            let Some(end) = after.find("?>") else {
                return false;
            };
            rest = after[end + 2..].trim_start();
        } else if let Some(after) = rest.strip_prefix("<!--") {
            let Some(end) = after.find("-->") else {
                return false;
            };
            rest = after[end + 3..].trim_start();
        } else if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case("<!doctype") {
            let Some(end) = rest.find('>') else {
                return false;
            };
            if rest[..end].to_ascii_lowercase().contains("html") {
                return false;
            }
            rest = rest[end + 1..].trim_start();
        } else {
            return rest.starts_with("<svg") || rest.starts_with("<svg:svg");
        }
    }
}

fn is_html(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    let head = head.trim_start_matches('\u{feff}').trim_start();
    head.starts_with("<!doctype html") || head.starts_with("<html") || head.starts_with('<')
}

/// What kind of output `bytes` is, by what the entry declared, the output
/// file's extension (`ext`, lowercase) and the bytes themselves.
#[must_use]
pub fn kind_of(bytes: &[u8], declared: FolderOutput, ext: Option<&str>) -> DrawnKind {
    let image = || match image_type(bytes) {
        Some(mime) => DrawnKind::Image {
            mime: mime.to_owned(),
        },
        None => DrawnKind::Undrawable,
    };
    let text = || {
        if std::str::from_utf8(bytes).is_ok() {
            DrawnKind::Text
        } else {
            DrawnKind::Undrawable
        }
    };
    match declared {
        FolderOutput::Image => image(),
        FolderOutput::Text => text(),
        FolderOutput::Markdown => match text() {
            DrawnKind::Text => DrawnKind::Markdown,
            other => other,
        },
        FolderOutput::Html => DrawnKind::Html,
        // A folder kind is kept as files, never drawn from bytes.
        FolderOutput::Folder => DrawnKind::Undrawable,
        FolderOutput::Detect => {
            if let Some(mime) = image_type(bytes) {
                return DrawnKind::Image {
                    mime: mime.to_owned(),
                };
            }
            match ext {
                Some("md" | "markdown") if std::str::from_utf8(bytes).is_ok() => {
                    return DrawnKind::Markdown;
                }
                Some("html" | "htm") => return DrawnKind::Html,
                Some("txt" | "text" | "log") => return text(),
                _ => {}
            }
            if is_html(bytes) {
                DrawnKind::Html
            } else {
                text()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tool's stamp (2026-11-24): a tool changed on disk under the same
    /// command is another stamp, so what it made before is never current.
    #[test]
    fn a_tool_changed_on_disk_is_another_stamp() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let tool = bin.join(if cfg!(windows) {
            "lowcode.exe"
        } else {
            "lowcode"
        });
        std::fs::write(&tool, "version 1").unwrap();
        let plugin = dir.path().join("site.jar");
        std::fs::write(&plugin, "plugin 1").unwrap();
        let search = Some(bin.as_os_str());
        let command = format!(
            "lowcode docs {{path}} --out {{outdir}} --plugin {}",
            plugin.display()
        );
        let first = tool_stamp(&command, search);
        assert_eq!(
            tool_stamp(&command, search),
            first,
            "the same tool, the same stamp"
        );
        assert!(
            !first.contains("{path}") && !first.contains("{outdir}") && !first.contains("docs")
        );
        std::fs::write(&tool, "version 2, a longer one").unwrap();
        let second = tool_stamp(&command, search);
        assert_ne!(second, first, "a new version of the program");
        std::fs::write(&plugin, "plugin 2, a longer one").unwrap();
        assert_ne!(
            tool_stamp(&command, search),
            second,
            "a new version of a file the command names"
        );
        // A program found nowhere is stamped by its word, and the look goes on.
        assert_eq!(
            tool_stamp("no-such-tool {path} {out}", search),
            "?no-such-tool\0"
        );
        // A program named by its path is taken as written.
        let by_path = tool_stamp(&format!("{} {{path}}", tool.display()), None);
        let canonical = std::fs::canonicalize(&tool).unwrap();
        assert!(
            by_path.starts_with(&format!("{}\0", canonical.display())),
            "{by_path:?}"
        );
    }

    #[test]
    fn the_placeholders_are_filled_in_their_words() {
        let words = argv(
            r#"lc render "{path}" --out={out}.png --name "{path}/x y""#,
            Path::new("/t/in/proj"),
            Path::new("/t/out"),
        )
        .unwrap();
        assert_eq!(
            words,
            [
                "lc",
                "render",
                "/t/in/proj",
                "--out=/t/out/output.png",
                "--name",
                "/t/in/proj/x y"
            ]
        );
    }

    /// `{outdir}` is the folder itself, with no `output` file in it; text
    /// after it stays.
    #[test]
    fn the_outdir_is_filled_as_a_folder() {
        let words = argv(
            "lc docs {path} --out {outdir} --site={outdir}/site",
            Path::new("/t/in/proj"),
            Path::new("/s/.staging/1-0/out"),
        )
        .unwrap();
        assert_eq!(
            words,
            [
                "lc",
                "docs",
                "/t/in/proj",
                "--out",
                "/s/.staging/1-0/out",
                "--site=/s/.staging/1-0/out/site"
            ]
        );
    }

    /// Detect tells an image by its bytes, SVG from HTML by its first
    /// element, Markdown by the output file's name, and UTF-8 as text.
    #[test]
    fn detect_reads_the_kind_from_the_bytes() {
        let detect = |bytes: &[u8], ext| kind_of(bytes, FolderOutput::Detect, ext);
        let image = |mime: &str| DrawnKind::Image {
            mime: mime.to_owned(),
        };
        assert_eq!(detect(b"\x89PNG\r\n\x1a\n", None), image("image/png"));
        assert_eq!(detect(b"\xFF\xD8\xFF\xE0", None), image("image/jpeg"));
        assert_eq!(detect(b"GIF89a", None), image("image/gif"));
        assert_eq!(detect(b"RIFF\0\0\0\0WEBPVP8 ", None), image("image/webp"));
        assert_eq!(
            detect(b"  <svg viewBox='0 0 1 1'/>", None),
            image("image/svg+xml")
        );
        assert_eq!(
            detect(
                b"<?xml version='1.0'?>\n<!-- made -->\n<!DOCTYPE svg PUBLIC>\n<svg/>",
                None
            ),
            image("image/svg+xml")
        );
        assert_eq!(detect(b"<!doctype html><p>hi</p>", None), DrawnKind::Html);
        assert_eq!(detect(b"<div>hi</div>", None), DrawnKind::Html);
        assert_eq!(detect(b"# Title\n", Some("md")), DrawnKind::Markdown);
        assert_eq!(detect(b"# Title\n", None), DrawnKind::Text);
        assert_eq!(detect("a – b\n".as_bytes(), None), DrawnKind::Text);
        assert_eq!(detect(b"\x00\xff\xfe", None), DrawnKind::Undrawable);
        assert_eq!(detect(b"<p>x</p>", Some("txt")), DrawnKind::Text);
    }

    /// A declared kind is taken, as far as the bytes allow.
    #[test]
    fn a_declared_kind_is_taken() {
        assert_eq!(
            kind_of(b"plain", FolderOutput::Image, None),
            DrawnKind::Undrawable
        );
        assert_eq!(
            kind_of(b"# x", FolderOutput::Markdown, None),
            DrawnKind::Markdown
        );
        assert_eq!(kind_of(b"<p>", FolderOutput::Text, None), DrawnKind::Text);
        assert_eq!(
            kind_of(b"\xff\xfe", FolderOutput::Text, None),
            DrawnKind::Undrawable
        );
        assert_eq!(kind_of(b"x", FolderOutput::Html, None), DrawnKind::Html);
    }
}
