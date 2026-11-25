//! Commands (*Run…*, ADR-0060): a person's own program, run once on a press on
//! a file, a folder, the repository, a commit or a comparison, whose output
//! knobas reads and shows in a dialog.
//!
//! This is the pure half: the saved shape, what a command is offered on, what
//! Save refuses and the argument vector a command becomes. The spawn, the
//! reader and the registry of runs are `knobas_app::oneoff`.
//!
//! # A command is *Open in* that knobas waits for
//!
//! The words are split as [`super::open::words`] splits an application's, with
//! quotes as grouping only and no shell. A placeholder's value is a path
//! inside the checkout the app found on this disk, a line number, a git name
//! held to ADR-0034's grammar ([`super::open::is_sha`],
//! [`super::open::is_ref_name`]) or a path knobas made for the run. Nothing
//! else of the mirror reaches an argument (ADR-0016).
//!
//! # A file's name is never shell text
//!
//! A placeholder that carries a path is refused in a word that holds
//! whitespace, which only a quoted script can, and in the word after `-c`,
//! `/c`, `/k` or `-Command`: such a word is a script that some shell reads
//! again. The page then shows the form that works, `sh -c 'eslint "$1"' _
//! {file}`, where the path is a word of its own.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::open::{is_ref_name, is_sha, words};

/// Every placeholder a command may hold, in the order the page offers them.
pub const PLACEHOLDERS: [&str; 15] = [
    "file", "relfile", "files", "dir", "reldir", "root", "name", "line", "endline", "sha", "base",
    "head", "branch", "out", "outdir",
];

/// The placeholders whose value is a path: refused inside a script.
const PATHS: [&str; 8] = [
    "file", "relfile", "files", "dir", "reldir", "root", "out", "outdir",
];

/// The programs that read their first argument, or the one after a script
/// flag, as a script.
const SHELLS: [&str; 11] = [
    "sh",
    "bash",
    "zsh",
    "dash",
    "fish",
    "cmd",
    "cmd.exe",
    "powershell",
    "powershell.exe",
    "pwsh",
    "pwsh.exe",
];

/// The flags after which a shell's next word is a script.
const SCRIPT_FLAGS: [&str; 7] = [
    "-c",
    "-lc",
    "-ic",
    "/c",
    "/k",
    "-command",
    "-encodedcommand",
];

/// What a command reads on stdin (step 8): nothing, the file as the page shows
/// it, or the selected lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "OneOffStdin")]
pub enum Stdin {
    #[default]
    None,
    File,
    Selection,
}

/// What a command is offered on, read from its placeholders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "OneOffKind")]
pub enum Kind {
    File,
    Folder,
    Repository,
    Commit,
    Range,
}

/// How a run's result is drawn (section 16): as its bytes say, or as named.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "OneOffShows")]
pub enum Shows {
    #[default]
    Auto,
    Text,
    Problems,
    Code,
    Diff,
    Json,
    Markdown,
    Page,
    Image,
    Pdf,
}

/// Where a run starts: the repository's top, or the folder it runs on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts", rename = "OneOffStartIn")]
pub enum StartIn {
    #[default]
    Root,
    Target,
}

/// One `NAME=value` line of a command's environment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffEnv")]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

/// The timeout a command has unless the person set one, in seconds.
pub const DEFAULT_TIMEOUT: u32 = 120;
/// The longest timeout a command may have, in seconds.
pub const MAX_TIMEOUT: u32 = 3600;

/// One saved command, a row of Settings › Commands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOff")]
pub struct OneOff {
    /// Stable: what a run, a key and the last runs name it by.
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub stdin: Stdin,
    /// A file's command: the files it takes, globs, empty for any.
    #[serde(default)]
    pub files: Vec<String>,
    /// A folder's command: the folders it takes by name or path.
    #[serde(default)]
    pub folders: Vec<String>,
    /// A folder's command: one of the folder's own files must match.
    #[serde(default)]
    pub has: Vec<String>,
    /// Every kind: a file at the repository's top must match.
    #[serde(default)]
    pub repo_holds: Vec<String>,
    /// Only in these repositories (step 10), by entity id; empty for every one.
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub shows: Shows,
    #[serde(default)]
    pub start_in: StartIn,
    /// Folders put in front of `PATH`; `{root}` may stand in one.
    #[serde(default)]
    pub path: Vec<String>,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    /// Ask the program for colours (`FORCE_COLOR` and friends), else `NO_COLOR`.
    #[serde(default = "yes")]
    pub colours: bool,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u32,
    /// On Windows: hand every path as WSL sees it (step 9).
    #[serde(default)]
    pub wsl_paths: bool,
}

const fn yes() -> bool {
    true
}

const fn default_timeout() -> u32 {
    DEFAULT_TIMEOUT
}

/// A command as the page sends it: a new one has no id yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffDraft")]
pub struct Draft {
    #[serde(default)]
    #[ts(optional = nullable)]
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub stdin: Stdin,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub folders: Vec<String>,
    #[serde(default)]
    pub has: Vec<String>,
    #[serde(default)]
    pub repo_holds: Vec<String>,
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub shows: Shows,
    #[serde(default)]
    pub start_in: StartIn,
    #[serde(default)]
    pub path: Vec<String>,
    #[serde(default)]
    pub env: Vec<EnvVar>,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub colours: Option<bool>,
    /// Blank or 0 is the default, 120 s.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub timeout_secs: Option<u32>,
    #[serde(default)]
    pub wsl_paths: bool,
}

/// Why a command cannot be saved or run, in the person's own string's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    Empty,
    UnclosedQuote,
    UnclosedPlaceholder,
    /// `{path}`, which means the checkout in *Open in* and a folder's copy in
    /// a folder renderer.
    PathMeansTwo,
    Unknown(String),
    /// `{files}` with other text in its word.
    NotAlone(String),
    /// A path placeholder in a word that holds whitespace: a quoted script.
    InScript(String),
    /// A path placeholder as the script after `-c`, `/c`, `/k` or `-Command`.
    AfterScriptFlag {
        name: String,
        program: String,
        flag: String,
    },
    /// `{out}` and `{outdir}` together, or one of them twice.
    OutOnce,
    /// `wsl` with a path and no `-e` before the program.
    WslNeedsE,
    /// A shell with no script of its own and stdin set: it would run the file.
    StdinShell(String),
    /// The target has no value for this placeholder.
    NoValue(String),
    /// `{relfile}` or `{reldir}` that a program would read as an option.
    ReadAsOption {
        name: String,
        value: String,
    },
    /// A git name refused by ADR-0034's grammar.
    Refused {
        name: String,
        value: String,
    },
    /// A path for `cmd` or a batch file that holds `%`, `!`, `^` or `"`.
    CmdChars {
        name: String,
        value: String,
    },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the command is empty, so there is no program to run"),
            Self::UnclosedQuote => write!(f, "the command has a quote that is never closed"),
            Self::UnclosedPlaceholder => {
                write!(f, "the command has a '{{' that is never closed by a '}}'")
            }
            Self::PathMeansTwo => write!(
                f,
                "{{path}} is not one of a command's: {{root}} is the repository, {{dir}} the \
                 folder and {{file}} the file"
            ),
            Self::Unknown(name) => write!(
                f,
                "{{{name}}} is not something knobas fills in: a command may take {}",
                PLACEHOLDERS
                    .iter()
                    .map(|p| format!("{{{p}}}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::NotAlone(name) => write!(
                f,
                "{{{name}}} becomes an argument a file, so it stands alone as a word"
            ),
            Self::InScript(name) => write!(
                f,
                "{{{name}}} stands inside a quoted script, where a file's name would be read as \
                 shell text. Pass it after the script and read it as \"$1\": sh -c 'eslint \"$1\" \
                 | head -40' _ {{{name}}}"
            ),
            Self::AfterScriptFlag {
                name,
                program,
                flag,
            } => write!(
                f,
                "{{{name}}} is the script of {program} {flag}: a file's name would be run as \
                 shell text. Pass it after the script and read it as \"$1\": sh -c 'eslint \"$1\"' \
                 _ {{{name}}}"
            ),
            Self::OutOnce => write!(
                f,
                "a command writes to {{out}} or to {{outdir}}, one of them, once"
            ),
            Self::WslNeedsE => write!(
                f,
                "wsl hands its words to a shell unless -e stands before the program: write wsl -e \
                 ruff check {{file}}"
            ),
            Self::StdinShell(program) => write!(
                f,
                "{program} with no script of its own would run what it reads on stdin as a script"
            ),
            Self::NoValue(name) => write!(f, "there is no {{{name}}} here"),
            Self::ReadAsOption { name, value } => {
                let instead = if name == "relfile" { "file" } else { "dir" };
                write!(
                    f,
                    "{{{name}}} would be '{value}', which a program would read as an option: use \
                     {{{instead}}}"
                )
            }
            Self::Refused { name, value } => write!(
                f,
                "{{{name}}} would be '{value}', which is not a name knobas passes to a program"
            ),
            Self::CmdChars { name, value } => write!(
                f,
                "{{{name}}} would be '{value}', and cmd reads %, !, ^ and \" in an argument again, \
                 so it is not handed to a batch file"
            ),
        }
    }
}

impl std::error::Error for Refusal {}

impl From<super::open::TemplateError> for Refusal {
    fn from(error: super::open::TemplateError) -> Self {
        use super::open::TemplateError as T;
        match error {
            T::UnclosedQuote => Self::UnclosedQuote,
            T::UnclosedPlaceholder => Self::UnclosedPlaceholder,
            _ => Self::Empty,
        }
    }
}

/// The placeholder names in `word`, in order.
fn names_in(word: &str) -> Result<Vec<&str>, Refusal> {
    let mut out = Vec::new();
    let mut rest = word;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let close = after.find('}').ok_or(Refusal::UnclosedPlaceholder)?;
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    Ok(out)
}

/// What a command is offered on: a file where it names the file or reads
/// stdin, else a folder where it names one, else a comparison for `{base}` or
/// `{head}`, a commit for `{sha}`, else the repository.
///
/// # Errors
///
/// The command does not split into words.
pub fn kind_of(command: &str, stdin: Stdin) -> Result<Kind, Refusal> {
    let mut held = Vec::new();
    for word in words(command)? {
        for name in names_in(&word)? {
            held.push(name.to_owned());
        }
    }
    let has = |names: &[&str]| names.iter().any(|n| held.iter().any(|h| h == n));
    Ok(
        if stdin != Stdin::None || has(&["file", "relfile", "files", "line", "endline"]) {
            Kind::File
        } else if has(&["dir", "reldir"]) {
            Kind::Folder
        } else if has(&["base", "head"]) {
            Kind::Range
        } else if has(&["sha"]) {
            Kind::Commit
        } else {
            Kind::Repository
        },
    )
}

/// A program word's own name, lower-cased, without its folder.
fn program_name(word: &str) -> String {
    word.rsplit(['/', '\\'])
        .next()
        .unwrap_or(word)
        .to_ascii_lowercase()
}

/// Whether `program` is `cmd` or a batch file, which `cmd` reads again.
fn is_cmd(program: &str) -> bool {
    let name = program_name(program);
    name == "cmd" || name == "cmd.exe" || name.ends_with(".cmd") || name.ends_with(".bat")
}

/// What Save refuses: every rule that does not need a target. `os` is
/// `std::env::consts::OS`'s spelling, so the Windows rules are tested anywhere.
///
/// # Errors
///
/// [`Refusal`] for the first rule the command breaks.
pub fn check(command: &str, stdin: Stdin, os: &str) -> Result<(), Refusal> {
    let _ = os;
    let words = words(command)?;
    let (mut outs, mut path_held) = (0, false);
    for (at, word) in words.iter().enumerate() {
        for name in names_in(word)? {
            if name == "path" {
                return Err(Refusal::PathMeansTwo);
            }
            if !PLACEHOLDERS.contains(&name) {
                return Err(Refusal::Unknown(name.to_owned()));
            }
            if name == "files" && word != "{files}" {
                return Err(Refusal::NotAlone(name.to_owned()));
            }
            if name == "out" || name == "outdir" {
                outs += 1;
            }
            if PATHS.contains(&name) {
                path_held = true;
                if word.chars().any(char::is_whitespace) {
                    return Err(Refusal::InScript(name.to_owned()));
                }
                // The flag is a script's only after a shell: `eslint -c` names a config.
                if at > 0
                    && SCRIPT_FLAGS.contains(&words[at - 1].to_ascii_lowercase().as_str())
                    && words[..at - 1]
                        .iter()
                        .any(|w| SHELLS.contains(&program_name(w).as_str()))
                {
                    return Err(Refusal::AfterScriptFlag {
                        name: name.to_owned(),
                        program: words[0].clone(),
                        flag: words[at - 1].clone(),
                    });
                }
            }
        }
    }
    if outs > 1 {
        return Err(Refusal::OutOnce);
    }
    let program = program_name(&words[0]);
    if (program == "wsl" || program == "wsl.exe")
        && path_held
        && !words.iter().any(|w| w == "-e" || w == "--exec")
    {
        return Err(Refusal::WslNeedsE);
    }
    if stdin != Stdin::None && words.len() < 2 && SHELLS.contains(&program.as_str()) {
        return Err(Refusal::StdinShell(words[0].clone()));
    }
    Ok(())
}

/// What a command's placeholders are filled from. Paths are relative to the
/// root with `/`, already found inside the checkout on disk by the caller.
#[derive(Clone, Copy, Debug)]
pub struct At<'a> {
    /// The checkout on this disk.
    pub root: &'a Path,
    /// The file, from the root.
    pub file: Option<&'a str>,
    /// A batch's files, from the root.
    pub files: &'a [String],
    /// The folder, from the root, `""` for the top.
    pub dir: Option<&'a str>,
    /// The first and last selected line.
    pub lines: Option<(u32, u32)>,
    pub sha: Option<&'a str>,
    pub base: Option<&'a str>,
    pub head: Option<&'a str>,
    /// The checkout's own branch.
    pub branch: Option<&'a str>,
    /// The run's own folder: `{outdir}` is its `out`, `{out}` that folder's
    /// `output`.
    pub scratch: &'a Path,
    pub stdin: Stdin,
    pub os: &'a str,
    /// Hand every path as WSL sees it (step 9, Windows only).
    pub wsl: bool,
}

impl<'a> At<'a> {
    /// The repository's top, and nothing else.
    #[must_use]
    pub const fn root(root: &'a Path, scratch: &'a Path, os: &'a str) -> Self {
        Self {
            root,
            file: None,
            files: &[],
            dir: None,
            lines: None,
            sha: None,
            base: None,
            head: None,
            branch: None,
            scratch,
            stdin: Stdin::None,
            os,
            wsl: false,
        }
    }
}

/// `relative` under `root`, or `root` for `""` and `.`.
fn absolute(root: &Path, relative: &str) -> String {
    let mut out = root.to_path_buf();
    for part in relative.split('/') {
        if !part.is_empty() && part != "." {
            out.push(part);
        }
    }
    out.to_string_lossy().into_owned()
}

/// A Windows path as WSL sees it: `C:\src\a.py` is `/mnt/c/src/a.py`, a path
/// under `\\wsl$\<distribution>\` or `\\wsl.localhost\<distribution>\` the one
/// inside the distribution. `None` for anything else.
#[must_use]
pub fn wsl_path(path: &str) -> Option<String> {
    let slashed = path.replace('\\', "/");
    for host in ["//wsl$/", "//wsl.localhost/"] {
        if slashed.len() > host.len() && slashed[..host.len()].eq_ignore_ascii_case(host) {
            let rest = &slashed[host.len()..];
            let inside = rest.find('/').map_or("", |at| &rest[at..]);
            return Some(if inside.is_empty() {
                "/".to_owned()
            } else {
                inside.to_owned()
            });
        }
    }
    let bytes = slashed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let rest = slashed[2..].trim_start_matches('/');
        let drive = (bytes[0] as char).to_ascii_lowercase();
        return Some(if rest.is_empty() {
            format!("/mnt/{drive}")
        } else {
            format!("/mnt/{drive}/{rest}")
        });
    }
    None
}

/// The argument vector `command` becomes at `at`, or why it runs nothing.
///
/// # Errors
///
/// [`Refusal`]: a rule of [`check`], a placeholder with no value here, a
/// relative path that would be an option, a git name the grammar refuses, or
/// a path `cmd` would read again.
pub fn fill(command: &str, at: &At<'_>) -> Result<Vec<String>, Refusal> {
    check(command, at.stdin, at.os)?;
    let words = words(command)?;
    let for_cmd = at.os == "windows" && is_cmd(&words[0]);
    let path_value = |name: &str, value: String| -> Result<String, Refusal> {
        let value = if at.wsl {
            wsl_path(&value).unwrap_or(value)
        } else {
            value
        };
        if for_cmd && value.contains(['%', '!', '^', '"']) {
            return Err(Refusal::CmdChars {
                name: name.to_owned(),
                value,
            });
        }
        Ok(value)
    };
    let mut argv = Vec::with_capacity(words.len());
    for word in &words {
        if word == "{files}" {
            let files: Vec<&str> = if at.files.is_empty() {
                at.file.into_iter().collect()
            } else {
                at.files.iter().map(String::as_str).collect()
            };
            if files.is_empty() {
                return Err(Refusal::NoValue("files".to_owned()));
            }
            for file in files {
                argv.push(path_value("files", absolute(at.root, file))?);
            }
            continue;
        }
        let mut out = String::with_capacity(word.len());
        let mut rest = word.as_str();
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let close = after.find('}').ok_or(Refusal::UnclosedPlaceholder)?;
            let name = &after[..close];
            let none = || Refusal::NoValue(name.to_owned());
            let value = match name {
                "root" => path_value(name, at.root.to_string_lossy().into_owned())?,
                "name" => at
                    .root
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                "file" => path_value(name, absolute(at.root, at.file.ok_or_else(none)?))?,
                "relfile" => {
                    let value = at.file.ok_or_else(none)?;
                    if value.starts_with('-') {
                        return Err(Refusal::ReadAsOption {
                            name: name.to_owned(),
                            value: value.to_owned(),
                        });
                    }
                    path_value(name, value.to_owned())?
                }
                "dir" => path_value(name, absolute(at.root, at.dir.ok_or_else(none)?))?,
                "reldir" => {
                    let value = at.dir.ok_or_else(none)?;
                    let value = if value.is_empty() { "." } else { value };
                    if value.starts_with('-') {
                        return Err(Refusal::ReadAsOption {
                            name: name.to_owned(),
                            value: value.to_owned(),
                        });
                    }
                    path_value(name, value.to_owned())?
                }
                "line" => at.lines.map_or(1, |(first, _)| first).to_string(),
                "endline" => at.lines.map_or(1, |(_, last)| last).to_string(),
                "sha" => git_name(name, at.sha, is_sha)?,
                "base" => git_name(name, at.base, |v| is_ref_name(v, true))?,
                "head" => git_name(name, at.head, |v| is_ref_name(v, false))?,
                "branch" => git_name(name, at.branch, |v| is_ref_name(v, false))?,
                "out" => path_value(
                    name,
                    at.scratch
                        .join("out")
                        .join("output")
                        .to_string_lossy()
                        .into_owned(),
                )?,
                "outdir" => {
                    path_value(name, at.scratch.join("out").to_string_lossy().into_owned())?
                }
                other => return Err(Refusal::Unknown(other.to_owned())),
            };
            out.push_str(&value);
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        argv.push(out);
    }
    Ok(argv)
}

/// `value` when there is one and `ok` admits it.
fn git_name(name: &str, value: Option<&str>, ok: impl Fn(&str) -> bool) -> Result<String, Refusal> {
    let value = value.ok_or_else(|| Refusal::NoValue(name.to_owned()))?;
    if ok(value) {
        Ok(value.to_owned())
    } else {
        Err(Refusal::Refused {
            name: name.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// An argument vector as one line a person can read and paste: a word with a
/// space or a quote in it is quoted.
#[must_use]
pub fn shown(argv: &[String]) -> String {
    argv.iter()
        .map(|a| {
            if a.is_empty()
                || a.chars()
                    .any(|c| c.is_whitespace() || c == '"' || c == '\'')
            {
                format!("\"{}\"", a.replace('"', "\\\""))
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// -- which things a command takes ------------------------------------------------

/// Whether `pattern` matches `text`, ignoring case: `*` any run of characters
/// (`/` too), `?` one. The renderers' glob (`folder_render::glob_matches`).
#[must_use]
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// A glob with a `/` against the whole path, else against the last part.
fn named(pattern: &str, path: &str) -> bool {
    if pattern.contains('/') {
        glob(pattern, path)
    } else {
        glob(pattern, path.rsplit('/').next().unwrap_or(path))
    }
}

/// What the page knows of the repository a command would run in.
#[derive(Clone, Copy, Debug)]
pub struct Facts<'a> {
    /// The repository's entity id.
    pub repo: &'a str,
    /// The names at its top, where the listing has been read: `None` counts
    /// *Only where the repository holds* as met.
    pub top: Option<&'a [String]>,
}

impl OneOff {
    /// What this command is offered on.
    #[must_use]
    pub fn kind(&self) -> Kind {
        kind_of(&self.command, self.stdin).unwrap_or(Kind::Repository)
    }

    /// The repository-wide lists: *Only in these repositories* and *Only where
    /// the repository holds*.
    fn in_repository(&self, facts: &Facts<'_>) -> bool {
        let repo = self.repos.is_empty() || self.repos.iter().any(|r| r == facts.repo);
        let holds = self.repo_holds.is_empty()
            || facts.top.is_none_or(|top| {
                self.repo_holds
                    .iter()
                    .any(|pattern| top.iter().any(|name| glob(pattern, name)))
            });
        repo && holds
    }

    /// Whether it takes the file at `path`.
    #[must_use]
    pub fn takes_file(&self, path: &str, facts: &Facts<'_>) -> bool {
        self.kind() == Kind::File
            && (self.files.is_empty() || self.files.iter().any(|p| named(p, path)))
            && self.in_repository(facts)
    }

    /// Whether it takes the folder at `path` (`""` the top), whose own files
    /// are `own` where the listing has been read.
    #[must_use]
    pub fn takes_folder(&self, path: &str, own: Option<&[String]>, facts: &Facts<'_>) -> bool {
        self.kind() == Kind::Folder
            && (self.folders.is_empty() || self.folders.iter().any(|p| named(p, path)))
            && (self.has.is_empty()
                || own.is_none_or(|own| {
                    self.has
                        .iter()
                        .any(|pattern| own.iter().any(|name| named(pattern, name)))
                }))
            && self.in_repository(facts)
    }

    /// Whether it takes the repository itself.
    #[must_use]
    pub fn takes_repository(&self, facts: &Facts<'_>) -> bool {
        self.kind() == Kind::Repository && self.in_repository(facts)
    }

    /// Whether it takes a commit.
    #[must_use]
    pub fn takes_commit(&self, facts: &Facts<'_>) -> bool {
        self.kind() == Kind::Commit && self.in_repository(facts)
    }

    /// Whether it takes a comparison.
    #[must_use]
    pub fn takes_range(&self, facts: &Facts<'_>) -> bool {
        self.kind() == Kind::Range && self.in_repository(facts)
    }
}

// -- the saved list ---------------------------------------------------------------

/// Why a list of commands cannot be saved, naming the entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListError {
    NoName(usize),
    Command { name: String, error: Refusal },
    Env { name: String, line: String },
    Path { name: String, folder: String },
}

impl std::fmt::Display for ListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoName(index) => write!(f, "command {} has no name", index + 1),
            Self::Command { name, error } => write!(f, "{name} cannot be run: {error}"),
            Self::Env { name, line } => write!(
                f,
                "{name}'s environment line '{line}' is not a NAME=value a program can be given \
                 (a name of letters, digits and _, never KNOBAS_)"
            ),
            Self::Path { name, folder } => write!(
                f,
                "{name}'s PATH folder '{folder}' may hold {{root}} and no other placeholder"
            ),
        }
    }
}

impl std::error::Error for ListError {}

/// Whether `name` is an environment variable's name a person may set.
fn env_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !name.as_bytes()[0].is_ascii_digit()
        && !name.to_ascii_uppercase().starts_with("KNOBAS_")
}

fn trimmed(list: &[String]) -> Vec<String> {
    list.iter()
        .flat_map(|item| item.split_whitespace())
        .map(str::to_owned)
        .collect()
}

/// The list a person saved, checked and trimmed, every entry with an id that
/// is kept through a rename. Checked for `os`.
///
/// # Errors
///
/// [`ListError`] for the first entry that cannot be saved.
pub fn settle(drafts: &[Draft], os: &str) -> Result<Vec<OneOff>, ListError> {
    let mut out: Vec<OneOff> = Vec::with_capacity(drafts.len());
    for (index, draft) in drafts.iter().enumerate() {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(ListError::NoName(index));
        }
        let command = draft.command.trim();
        check(command, draft.stdin, os).map_err(|error| ListError::Command {
            name: name.to_owned(),
            error,
        })?;
        let mut env = Vec::new();
        for var in &draft.env {
            let var_name = var.name.trim();
            if var_name.is_empty() && var.value.is_empty() {
                continue;
            }
            if !env_name_ok(var_name) {
                return Err(ListError::Env {
                    name: name.to_owned(),
                    line: format!("{var_name}={}", var.value),
                });
            }
            env.push(EnvVar {
                name: var_name.to_owned(),
                value: var.value.clone(),
            });
        }
        let mut path = Vec::new();
        for folder in &draft.path {
            let folder = folder.trim();
            if folder.is_empty() {
                continue;
            }
            if names_in(folder).map_or(true, |names| names.iter().any(|n| *n != "root")) {
                return Err(ListError::Path {
                    name: name.to_owned(),
                    folder: folder.to_owned(),
                });
            }
            path.push(folder.to_owned());
        }
        let timeout_secs = match draft.timeout_secs {
            None | Some(0) => DEFAULT_TIMEOUT,
            Some(secs) => secs.min(MAX_TIMEOUT),
        };
        let given = draft.id.as_deref().map(str::trim).unwrap_or_default();
        let taken = |id: &str| out.iter().any(|c| c.id == id);
        let id = if !given.is_empty() && !taken(given) {
            given.to_owned()
        } else {
            let base = slug(name);
            let mut id = base.clone();
            let mut n = 2;
            while taken(&id) {
                id = format!("{base}-{n}");
                n += 1;
            }
            id
        };
        out.push(OneOff {
            id,
            name: name.to_owned(),
            command: command.to_owned(),
            stdin: draft.stdin,
            files: trimmed(&draft.files),
            folders: trimmed(&draft.folders),
            has: trimmed(&draft.has),
            repo_holds: trimmed(&draft.repo_holds),
            repos: trimmed(&draft.repos),
            shows: draft.shows,
            start_in: draft.start_in,
            path,
            env,
            colours: draft.colours.unwrap_or(true),
            timeout_secs,
            wsl_paths: draft.wsl_paths,
        });
    }
    Ok(out)
}

/// An id from a name: lower-case letters and digits, runs of anything else
/// one `-`; `command` when nothing is left.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() {
        "command".to_owned()
    } else {
        out
    }
}

#[cfg(test)]
mod tests;
