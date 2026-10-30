//! *Open in…* (#501, #844, R-8.21 to R-8.24): the applications a person runs
//! at a checkout, and the argument vector one of them becomes.
//!
//! An **application** is a name and one command per [`ObjectKind`] it
//! accepts (#2274): the repository's, which every application has, and
//! optionally a file's, a commit's, a range's and a pull request's. One
//! without a file command opens a file with its repository command, so the
//! applications saved before #2274 are repository and file applications.
//!
//! # Placeholders
//!
//! `{path}` (the checkout), `{file}` (a file inside it), `{line}` (a line
//! number) and `{name}` (the checkout folder's name) are from this disk.
//! `{sha}`, `{base}`, `{head}`, `{pr}` and `{branch}` name the code object, and
//! `{url}` is its page at the source ([`ObjectKind::placeholders`] says which
//! object carries which).
//!
//! # What reaches an argument (ADR-0016, ADR-0034)
//!
//! The disk's values are the checkout path knobas resolved on this machine, a
//! file path the caller has already found *inside* that checkout on disk
//! ([`inside`] is the lexical half of that check), and a number. `{name}` is
//! the checkout folder's own name, read off the path.
//!
//! The object's values are git names a remote system may have written, so
//! each is held to a grammar before it becomes an argument ([`fill`] refuses
//! one that is not): a sha is 7 to 64 hex digits; a base, head or branch is a
//! git ref name of `[A-Za-z0-9._/-]` that starts with none of `-`, `.` or
//! `/` (a base may end in one `^`); a pull request is a number; a URL is one
//! the app built from the source's typed base URL (`knobas_source::code_pages`)
//! and starts with `http://` or `https://`. None of them can be read as an
//! option, and there is no shell to read anything else. `{title}`, `{author}`
//! and the like stay unknown and are refused by name.
//!
//! # No shell
//!
//! The answer is an argv. Quotes in a command only group words; a path with a
//! space in it, or a `;`, is one argument and stays one.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The placeholders a command may carry, in the order the dialog offers them.
pub const PLACEHOLDERS: [&str; 10] = [
    "path", "file", "line", "name", "sha", "base", "head", "pr", "branch", "url",
];

/// What an application is opened on (#2274).
///
/// - `repository`: the checkout.
/// - `file`: a file in the repository, relative to it, at a line, read `at` a
///   commit or branch (the page's; the checkout is the working tree as left).
/// - `commit`: one commit, by its sha.
/// - `range`: the diff between two commits or branches, as the diff address
///   names them (`base` may be `<sha>^`).
/// - `pull_request`: by its number, with its base and head as the page knows
///   them, and the head's branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "entity.ts", rename = "OpenObject")]
pub enum Object {
    Repository,
    File {
        path: String,
        line: Option<u32>,
        at: Option<String>,
    },
    Commit {
        sha: String,
    },
    Range {
        base: String,
        head: String,
    },
    PullRequest {
        number: u64,
        base: String,
        head: String,
        branch: String,
    },
}

/// Which kind of [`Object`]: what an application declares it accepts by
/// having a command for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts", rename = "OpenObjectKind")]
pub enum ObjectKind {
    Repository,
    File,
    Commit,
    Range,
    PullRequest,
}

impl ObjectKind {
    /// Every kind, in the order the dialog draws their commands.
    pub const ALL: [Self; 5] = [
        Self::Repository,
        Self::File,
        Self::Commit,
        Self::Range,
        Self::PullRequest,
    ];

    /// The placeholders a command for this kind may carry. A repository and
    /// a file command take the same four as before #2274, and `{url}`.
    #[must_use]
    pub const fn placeholders(self) -> &'static [&'static str] {
        match self {
            Self::Repository | Self::File => &["path", "file", "line", "name", "url"],
            Self::Commit => &["path", "name", "sha", "url"],
            Self::Range => &["path", "name", "base", "head", "url"],
            Self::PullRequest => &["path", "name", "pr", "base", "head", "branch", "url"],
        }
    }

    /// What the messages call this kind's command.
    #[must_use]
    pub const fn command_word(self) -> &'static str {
        match self {
            Self::Repository => "command",
            Self::File => "file command",
            Self::Commit => "commit command",
            Self::Range => "range command",
            Self::PullRequest => "pull request command",
        }
    }
}

impl Object {
    #[must_use]
    pub const fn kind(&self) -> ObjectKind {
        match self {
            Self::Repository => ObjectKind::Repository,
            Self::File { .. } => ObjectKind::File,
            Self::Commit { .. } => ObjectKind::Commit,
            Self::Range { .. } => ObjectKind::Range,
            Self::PullRequest { .. } => ObjectKind::PullRequest,
        }
    }
}

/// The object's own values for [`fill`], as given: each is checked when it
/// becomes an argument, so a fork's head no command uses refuses nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Code {
    pub sha: Option<String>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub pr: Option<u64>,
    pub branch: Option<String>,
    /// The object's page, built by the app from the source's typed base URL.
    pub url: Option<String>,
}

impl Code {
    /// No object values: a repository or a file without a page.
    pub const NONE: Self = Self {
        sha: None,
        base: None,
        head: None,
        pr: None,
        branch: None,
        url: None,
    };

    /// `object`'s values, and `url` for its page.
    #[must_use]
    pub fn of(object: &Object, url: Option<String>) -> Self {
        let mut code = Self {
            url,
            ..Self::NONE
        };
        match object {
            Object::Repository | Object::File { .. } => {}
            Object::Commit { sha } => code.sha = Some(sha.clone()),
            Object::Range { base, head } => {
                code.base = Some(base.clone());
                code.head = Some(head.clone());
            }
            Object::PullRequest {
                number,
                base,
                head,
                branch,
            } => {
                code.pr = Some(*number);
                code.base = Some(base.clone());
                code.head = Some(head.clone());
                code.branch = Some(branch.clone());
            }
        }
        code
    }
}

/// Whether `sha` is 7 to 64 hex digits.
#[must_use]
pub fn is_sha(sha: &str) -> bool {
    (7..=64).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether `name` is a git name knobas passes to a program: `[A-Za-z0-9._/-]`,
/// not starting with `-`, `.` or `/`, no `..` or `//`, not ending in `/` or
/// `.lock`, at most 255 bytes; with `caret`, one trailing `^` may follow.
#[must_use]
pub fn is_ref_name(name: &str, caret: bool) -> bool {
    let bare = if caret {
        name.strip_suffix('^').unwrap_or(name)
    } else {
        name
    };
    !bare.is_empty()
        && bare.len() <= 255
        && bare
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'))
        && !bare.starts_with(['-', '.', '/'])
        && !bare.ends_with('/')
        && !bare.ends_with(".lock")
        && !bare.contains("..")
        && !bare.contains("//")
}

/// One application on the *Open in…* menu.
///
/// Issue #844, R-8.21, #2274. It accepts the objects it has a command for:
/// every application a repository and a file, and a commit, a range or a
/// pull request when it has that command.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "CheckoutApplication")]
pub struct Application {
    /// Stable id: what `open_checkout` takes and the default names.
    pub id: String,
    /// What the menu says, e.g. "VS Code".
    pub name: String,
    /// The command that opens the checkout.
    pub command: String,
    /// The command that opens a file, if the application has one.
    pub file_command: Option<String>,
    /// The command that opens a commit; without one the application is not
    /// offered for a commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub commit_command: Option<String>,
    /// The command that opens a range of commits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub range_command: Option<String>,
    /// The command that opens a pull request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pull_request_command: Option<String>,
}

impl Application {
    /// The command this application runs for `kind`, if it accepts it. A file
    /// without a file command opens with the repository command.
    #[must_use]
    pub fn command_for(&self, kind: ObjectKind) -> Option<&str> {
        match kind {
            ObjectKind::Repository => Some(&self.command),
            ObjectKind::File => Some(self.file_command.as_deref().unwrap_or(&self.command)),
            ObjectKind::Commit => self.commit_command.as_deref(),
            ObjectKind::Range => self.range_command.as_deref(),
            ObjectKind::PullRequest => self.pull_request_command.as_deref(),
        }
    }

    /// Whether this application is offered for `kind`.
    #[must_use]
    pub fn accepts(&self, kind: ObjectKind) -> bool {
        self.command_for(kind).is_some()
    }
}

/// The id of the application the shipped list makes the default.
pub const SHIPPED_DEFAULT: &str = "vscode";

/// The applications knobas starts with on `os` (`std::env::consts::OS`'s
/// spelling): VS Code, Terminal, Fork and IntelliJ IDEA on macOS, none
/// elsewhere.
///
/// `open -a <application>` and not a CLI name such as `code` or `idea`: an app
/// started from Finder has `/usr/bin:/bin:/usr/sbin:/sbin` for `PATH`, where
/// those shims are not, and `open` resolves the application through Launch
/// Services. `open -a` takes no line number, so the file commands open the
/// file without one; a person who has the shim edits the command.
///
/// The ids `vscode`, `jetbrains` and `terminal` are #501's action ids, so a
/// caller that still names one of those reaches the same application.
#[must_use]
pub fn shipped_on(os: &str) -> Vec<Application> {
    if os != "macos" {
        return Vec::new();
    }
    let app = |id: &str, name: &str, command: &str, file: Option<&str>| Application {
        id: id.to_owned(),
        name: name.to_owned(),
        command: command.to_owned(),
        file_command: file.map(str::to_owned),
        ..Application::default()
    };
    vec![
        app(
            "vscode",
            "VS Code",
            "open -a \"Visual Studio Code\" {path}",
            Some("open -a \"Visual Studio Code\" {file}"),
        ),
        app("terminal", "Terminal", "open -a Terminal {path}", None),
        app("fork", "Fork", "open -a Fork {path}", None),
        app(
            "jetbrains",
            "IntelliJ IDEA",
            "open -a \"IntelliJ IDEA\" {path}",
            Some("open -a \"IntelliJ IDEA\" {file}"),
        ),
    ]
}

/// [`shipped_on`] for this machine.
#[must_use]
pub fn shipped() -> Vec<Application> {
    shipped_on(std::env::consts::OS)
}

/// What the placeholders are filled from.
#[derive(Clone, Copy, Debug)]
pub struct Fill<'a> {
    /// The checkout on this disk.
    pub checkout: &'a Path,
    /// A file inside it, already found on disk; `{file}` is the checkout
    /// without one.
    pub file: Option<&'a Path>,
    /// The line; `{line}` is 1 without one.
    pub line: Option<u32>,
    /// The object's values: `{sha}`, `{base}`, `{head}`, `{pr}`, `{branch}`
    /// and `{url}`.
    pub code: &'a Code,
}

/// Why a command cannot be run. Each `Display` speaks about the person's own
/// string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateError {
    /// Nothing but whitespace: there is no program to run.
    Empty,
    /// A `"` or `'` that never closes.
    UnclosedQuote,
    /// A `{` with no `}` after it.
    UnclosedPlaceholder,
    /// A placeholder knobas does not fill, carried so the message can name it.
    UnknownPlaceholder(String),
    /// A placeholder knobas fills, but not in a command for this object.
    NotForObject {
        name: String,
        object: ObjectKind,
    },
    /// The object has no value for this placeholder: a `{url}` whose source
    /// declares no page.
    NoValue(String),
    /// The value is not one knobas passes to a program (see the module's
    /// *What reaches an argument*).
    Refused { name: String, value: String },
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the command is empty, so there is no program to run"),
            Self::UnclosedQuote => write!(f, "the command has a quote that is never closed"),
            Self::UnclosedPlaceholder => {
                write!(f, "the command has a '{{' that is never closed by a '}}'")
            }
            Self::UnknownPlaceholder(name) => write!(
                f,
                "{{{name}}} is not something knobas fills in: a command may take {{path}}, \
                 {{file}}, {{line}}, {{name}}, {{sha}}, {{base}}, {{head}}, {{pr}}, {{branch}} \
                 and {{url}}"
            ),
            Self::NotForObject { name, object } => {
                let takes: Vec<String> = object
                    .placeholders()
                    .iter()
                    .map(|p| format!("{{{p}}}"))
                    .collect();
                write!(
                    f,
                    "{{{name}}} has no value in a {}: it takes {}",
                    object.command_word(),
                    takes.join(", ")
                )
            }
            Self::NoValue(name) => {
                if name == "url" {
                    write!(f, "the source declares no web page for this, so {{url}} has no value")
                } else {
                    write!(f, "there is no {{{name}}} here")
                }
            }
            Self::Refused { name, value } => write!(
                f,
                "{{{name}}} would be '{value}', which is not a name knobas passes to a program"
            ),
        }
    }
}

impl std::error::Error for TemplateError {}

/// The argument vector `template` becomes at `at`, or why it cannot.
///
/// # Errors
///
/// [`TemplateError`], one variant per way a person's string is unusable.
pub fn fill(template: &str, at: &Fill<'_>) -> Result<Vec<String>, TemplateError> {
    let words = split_words(template)?;
    if words.is_empty() {
        return Err(TemplateError::Empty);
    }
    words.iter().map(|word| substitute(word, at)).collect()
}

/// `command` as an argument vector with no placeholder filled: quotes group
/// words, nothing else is special, so `a;b` and `$HOME` stay one literal
/// argument each. A renderer command (#2262, ADR-0033) takes no placeholder,
/// so a `{` in it is just a character.
///
/// # Errors
///
/// [`TemplateError::Empty`] for no program, [`TemplateError::UnclosedQuote`].
pub fn words(command: &str) -> Result<Vec<String>, TemplateError> {
    let words = split_words(command)?;
    if words.is_empty() {
        return Err(TemplateError::Empty);
    }
    Ok(words)
}

/// Whether `template` is one knobas could run, without running anything: the
/// check a settings write makes. [`fill`] against a stand-in checkout, so there
/// is one rule.
///
/// # Errors
///
/// [`TemplateError`], the same ones [`fill`] gives.
pub fn check(template: &str) -> Result<(), TemplateError> {
    check_for(template, ObjectKind::Repository)
}

/// [`check`] for a command of `object`: one of its placeholders only.
///
/// # Errors
///
/// [`TemplateError`]; [`TemplateError::NotForObject`] for a placeholder
/// knobas fills elsewhere.
pub fn check_for(template: &str, object: ObjectKind) -> Result<(), TemplateError> {
    let words = split_words(template)?;
    if words.is_empty() {
        return Err(TemplateError::Empty);
    }
    for word in &words {
        for name in placeholder_names(word)? {
            if !PLACEHOLDERS.contains(&name) {
                return Err(TemplateError::UnknownPlaceholder(name.to_owned()));
            }
            if !object.placeholders().contains(&name) {
                return Err(TemplateError::NotForObject {
                    name: name.to_owned(),
                    object,
                });
            }
        }
    }
    Ok(())
}

fn placeholder_names(word: &str) -> Result<Vec<&str>, TemplateError> {
    let mut out = Vec::new();
    let mut rest = word;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let close = after.find('}').ok_or(TemplateError::UnclosedPlaceholder)?;
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    Ok(out)
}

/// `relative` joined onto `checkout`, when it names something below it by
/// words alone: no root, no drive, no `..`. The lexical half of keeping
/// `{file}` inside the checkout; the caller still asks the disk, which is
/// where a symlink out of the checkout is caught.
#[must_use]
pub fn inside(checkout: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    let mut out = checkout.to_path_buf();
    let mut any = false;
    for component in relative.components() {
        match component {
            Component::Normal(part) => {
                out.push(part);
                any = true;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    any.then_some(out)
}

/// Split a template into words, honouring `"` and `'` as grouping only.
fn split_words(template: &str) -> Result<Vec<String>, TemplateError> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for ch in template.chars() {
        match quote {
            Some(open) if ch == open => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
                // `""` is still a word: an empty argument is one a command
                // line can carry.
                started = true;
            }
            None if ch.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(ch);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err(TemplateError::UnclosedQuote);
    }
    if started {
        words.push(current);
    }
    Ok(words)
}

/// One word with its placeholders filled in.
fn substitute(word: &str, at: &Fill<'_>) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(word.len());
    let mut rest = word;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after.find('}').ok_or(TemplateError::UnclosedPlaceholder)?;
        let name = &after[..close];
        // `to_string_lossy`: a path that is not UTF-8 arrives as a visibly
        // wrong path rather than silently as another one.
        match name {
            "path" => out.push_str(&at.checkout.to_string_lossy()),
            "file" => out.push_str(&at.file.unwrap_or(at.checkout).to_string_lossy()),
            "line" => out.push_str(&at.line.unwrap_or(1).to_string()),
            "name" => out.push_str(
                &at.checkout
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            "sha" => out.push_str(checked(name, at.code.sha.as_deref(), is_sha)?),
            "base" => out.push_str(checked(name, at.code.base.as_deref(), |v| {
                is_ref_name(v, true)
            })?),
            "head" => out.push_str(checked(name, at.code.head.as_deref(), |v| {
                is_ref_name(v, false)
            })?),
            "branch" => out.push_str(checked(name, at.code.branch.as_deref(), |v| {
                is_ref_name(v, false)
            })?),
            "pr" => out.push_str(
                &at.code
                    .pr
                    .ok_or_else(|| TemplateError::NoValue(name.to_owned()))?
                    .to_string(),
            ),
            "url" => out.push_str(checked(name, at.code.url.as_deref(), |v| {
                (v.starts_with("https://") || v.starts_with("http://"))
                    && !v.chars().any(char::is_whitespace)
            })?),
            other => return Err(TemplateError::UnknownPlaceholder(other.to_owned())),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// `value` when the object has one and `ok` admits it.
fn checked<'v>(
    name: &str,
    value: Option<&'v str>,
    ok: impl Fn(&str) -> bool,
) -> Result<&'v str, TemplateError> {
    let value = value.ok_or_else(|| TemplateError::NoValue(name.to_owned()))?;
    if ok(value) {
        Ok(value)
    } else {
        Err(TemplateError::Refused {
            name: name.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// An application as a person typed it in the dialog: a new one has no id
/// yet.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts", rename = "CheckoutApplicationDraft")]
pub struct Draft {
    /// The id it already has; `None` or blank for a new one.
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    /// Blank is no file command.
    #[serde(default)]
    pub file_command: Option<String>,
    /// Blank is none: not offered for a commit.
    #[serde(default)]
    #[ts(optional)]
    pub commit_command: Option<String>,
    /// Blank is none: not offered for a range.
    #[serde(default)]
    #[ts(optional)]
    pub range_command: Option<String>,
    /// Blank is none: not offered for a pull request.
    #[serde(default)]
    #[ts(optional)]
    pub pull_request_command: Option<String>,
}

/// Why a list of applications cannot be saved, naming the entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListError {
    /// The entry at this position has a blank name.
    NoName(usize),
    /// This application's command for `object` cannot be run.
    Command {
        name: String,
        object: ObjectKind,
        error: TemplateError,
    },
}

impl std::fmt::Display for ListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoName(index) => write!(f, "application {} has no name", index + 1),
            Self::Command {
                name,
                object,
                error,
            } => write!(
                f,
                "{name}'s {} cannot be run: {error}",
                object.command_word()
            ),
        }
    }
}

impl std::error::Error for ListError {}

/// The list a person saved, checked and trimmed, every entry with an id.
///
/// An id a draft carries is kept, so the default and a caller naming an
/// application keep reaching it through a rename; a new entry gets one made
/// from its name. Two entries never share one.
///
/// # Errors
///
/// [`ListError`] for the first entry that cannot be saved.
pub fn settle(drafts: &[Draft]) -> Result<Vec<Application>, ListError> {
    let mut out: Vec<Application> = Vec::with_capacity(drafts.len());
    for (index, draft) in drafts.iter().enumerate() {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(ListError::NoName(index));
        }
        let command = draft.command.trim();
        check_for(command, ObjectKind::Repository).map_err(|error| ListError::Command {
            name: name.to_owned(),
            object: ObjectKind::Repository,
            error,
        })?;
        let optional = |given: &Option<String>, object: ObjectKind| {
            let given = given
                .as_deref()
                .map(str::trim)
                .filter(|command| !command.is_empty());
            match given {
                Some(command) => check_for(command, object)
                    .map(|()| Some(command.to_owned()))
                    .map_err(|error| ListError::Command {
                        name: name.to_owned(),
                        object,
                        error,
                    }),
                None => Ok(None),
            }
        };
        let file_command = optional(&draft.file_command, ObjectKind::File)?;
        let commit_command = optional(&draft.commit_command, ObjectKind::Commit)?;
        let range_command = optional(&draft.range_command, ObjectKind::Range)?;
        let pull_request_command =
            optional(&draft.pull_request_command, ObjectKind::PullRequest)?;
        let given = draft.id.as_deref().map(str::trim).unwrap_or_default();
        let taken = |id: &str| out.iter().any(|app| app.id == id);
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
        out.push(Application {
            id,
            name: name.to_owned(),
            command: command.to_owned(),
            file_command,
            commit_command,
            range_command,
            pull_request_command,
        });
    }
    Ok(out)
}

/// An id from a name: lower-case letters and digits, runs of anything else
/// one `-`; `app` when nothing is left.
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
        "app".to_owned()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECKOUT: &str = "/Users/mara/code/payout-service";

    fn at_checkout() -> Fill<'static> {
        Fill {
            checkout: Path::new(CHECKOUT),
            file: None,
            line: None,
            code: &Code::NONE,
        }
    }

    fn filled(template: &str, at: &Fill<'_>) -> Vec<String> {
        fill(template, at).unwrap_or_else(|error| panic!("{template:?} must fill: {error}"))
    }

    /// The four placeholders, each from the disk, inside their own words.
    #[test]
    fn the_four_placeholders_fill_from_the_checkout_and_the_file() {
        let file = Path::new("/Users/mara/code/payout-service/src/sepa/retry.rs");
        let at = Fill {
            checkout: Path::new(CHECKOUT),
            file: Some(file),
            line: Some(42),
            code: &Code::NONE,
        };
        assert_eq!(
            filled("code --goto {file}:{line}", &at),
            [
                "code",
                "--goto",
                "/Users/mara/code/payout-service/src/sepa/retry.rs:42"
            ]
        );
        assert_eq!(
            filled("viewer --title={name} {path}", &at),
            [
                "viewer",
                "--title=payout-service",
                "/Users/mara/code/payout-service"
            ]
        );
    }

    /// Outside a file, `{file}` is the checkout and `{line}` is 1, as the
    /// dialog's warning says.
    #[test]
    fn without_a_file_the_file_is_the_checkout_and_the_line_is_one() {
        assert_eq!(
            filled("idea --line {line} {file}", &at_checkout()),
            ["idea", "--line", "1", CHECKOUT]
        );
    }

    /// A path with a space and shell characters is **one** argument: no shell.
    #[test]
    fn a_path_with_spaces_stays_one_argument() {
        let at = Fill {
            checkout: Path::new("/Users/mara/My Code/pay; rm -rf ~"),
            file: None,
            line: None,
            code: &Code::NONE,
        };
        assert_eq!(
            filled("open -a Terminal {path}", &at),
            [
                "open",
                "-a",
                "Terminal",
                "/Users/mara/My Code/pay; rm -rf ~"
            ]
        );
        // And the template's own shell characters are plain words.
        assert_eq!(
            filled("tool {path} | tee x", &at_checkout()),
            ["tool", CHECKOUT, "|", "tee", "x"]
        );
    }

    /// Quotes group words and do not survive into the argv.
    #[test]
    fn quotes_group_a_word_and_are_not_passed_on() {
        assert_eq!(
            filled("open -a \"Visual Studio Code\" {path}", &at_checkout()),
            ["open", "-a", "Visual Studio Code", CHECKOUT]
        );
        assert_eq!(
            filled("thing '' {path}", &at_checkout()),
            ["thing", "", CHECKOUT]
        );
    }

    /// **ADR-0016 in a test**: a synced value has no placeholder, and asking
    /// for one is refused by name.
    #[test]
    fn a_placeholder_for_a_synced_value_is_refused_by_name() {
        for name in ["title", "repo_url", "web_url", "author", "PATH", ""] {
            let error = fill(&format!("code {{{name}}} {{path}}"), &at_checkout())
                .expect_err("only the ten placeholders fill");
            assert_eq!(error, TemplateError::UnknownPlaceholder(name.to_owned()));
            assert!(
                error.to_string().contains(&format!("{{{name}}}")),
                "{error}"
            );
        }
    }

    /// The other unusable strings, and two the old rule refused that are now
    /// only the dialog's warnings: no `{path}`, and one used twice.
    #[test]
    fn an_unusable_command_says_what_is_wrong_with_it() {
        assert_eq!(check(""), Err(TemplateError::Empty));
        assert_eq!(check("  \t"), Err(TemplateError::Empty));
        assert_eq!(check("code {path"), Err(TemplateError::UnclosedPlaceholder));
        assert_eq!(
            check("open -a \"Visual {path}"),
            Err(TemplateError::UnclosedQuote)
        );
        assert_eq!(check("open -a Fork"), Ok(()));
        assert_eq!(check("cp {path} {path}"), Ok(()));
    }

    /// A mirrored git name reaches an argument only when it cannot be read
    /// as an option or as anything but a name (ADR-0034).
    #[test]
    fn an_object_value_that_is_not_a_plain_git_name_is_refused() {
        let at = |code: &Code| -> Result<Vec<String>, TemplateError> {
            fill(
                "tool {sha} {base} {head} {branch} {url}",
                &Fill {
                    checkout: Path::new(CHECKOUT),
                    file: None,
                    line: None,
                    code,
                },
            )
        };
        let good = Code {
            sha: Some("c90d11e4a7b2".into()),
            base: Some("7be0e4f15c60^".into()),
            head: Some("feature/PAY-231".into()),
            branch: Some("feature/PAY-231".into()),
            url: Some("https://git.example/o/r".into()),
            pr: None,
        };
        assert!(at(&good).is_ok());
        for (field, bad) in [
            ("sha", "--upload-pack=x"),
            ("sha", "c90d"),
            ("sha", "c90d11e4a7b2^"),
            ("base", "-x"),
            ("base", "a..b"),
            ("base", "a^^"),
            ("head", "a^"),
            ("head", "owner/repo:branch"),
            ("head", ".hidden"),
            ("branch", "a b"),
            ("branch", "x.lock"),
            ("url", "file:///etc/passwd"),
            ("url", "--help"),
        ] {
            let mut code = good.clone();
            let slot = match field {
                "sha" => &mut code.sha,
                "base" => &mut code.base,
                "head" => &mut code.head,
                "branch" => &mut code.branch,
                _ => &mut code.url,
            };
            *slot = Some(bad.to_owned());
            assert_eq!(
                at(&code),
                Err(TemplateError::Refused {
                    name: field.to_owned(),
                    value: bad.to_owned()
                }),
                "{field} = {bad}"
            );
        }
        assert_eq!(
            at(&Code {
                url: None,
                ..good
            }),
            Err(TemplateError::NoValue("url".to_owned()))
        );
    }

    /// Each object carries its own placeholders and no other's.
    #[test]
    fn a_command_takes_only_its_objects_placeholders() {
        assert_eq!(check_for("tig show {sha} {path}", ObjectKind::Commit), Ok(()));
        assert_eq!(
            check_for("difftool {base} {head}", ObjectKind::Range),
            Ok(())
        );
        assert_eq!(
            check_for("x {pr} {branch} {base} {head} {url}", ObjectKind::PullRequest),
            Ok(())
        );
        assert_eq!(
            check_for("tig show {head}", ObjectKind::Commit),
            Err(TemplateError::NotForObject {
                name: "head".to_owned(),
                object: ObjectKind::Commit
            })
        );
        assert_eq!(
            check_for("x {file}", ObjectKind::Range),
            Err(TemplateError::NotForObject {
                name: "file".to_owned(),
                object: ObjectKind::Range
            })
        );
        assert!(
            check_for("x {sha}", ObjectKind::Repository)
                .unwrap_err()
                .to_string()
                .contains("{path}, {file}, {line}, {name}, {url}")
        );
    }

    /// The applications saved before #2274 are repository and file
    /// applications, and one accepts a commit, a range or a pull request
    /// only by having that command.
    #[test]
    fn an_application_accepts_the_objects_it_has_a_command_for() {
        let old: Application =
            serde_json::from_str(r#"{"id":"vscode","name":"VS Code","command":"code {path}","file_command":null}"#)
                .unwrap();
        let kinds = |app: &Application| -> Vec<ObjectKind> {
            ObjectKind::ALL.into_iter().filter(|k| app.accepts(*k)).collect()
        };
        assert_eq!(kinds(&old), [ObjectKind::Repository, ObjectKind::File]);
        assert_eq!(old.command_for(ObjectKind::File), Some("code {path}"));
        let tig = Application {
            commit_command: Some("tig show {sha}".into()),
            ..old.clone()
        };
        assert_eq!(
            kinds(&tig),
            [ObjectKind::Repository, ObjectKind::File, ObjectKind::Commit]
        );
        assert_eq!(
            serde_json::to_value(&old).unwrap(),
            serde_json::json!({"id":"vscode","name":"VS Code","command":"code {path}","file_command":null}),
            "an old application is stored as it was"
        );
    }

    /// The shipped list is R-8.21's four, in its order, on macOS only.
    #[test]
    fn macos_ships_four_applications_and_the_others_none() {
        let names: Vec<String> = shipped_on("macos").into_iter().map(|a| a.name).collect();
        assert_eq!(names, ["VS Code", "Terminal", "Fork", "IntelliJ IDEA"]);
        for os in ["linux", "windows", "MACOS", ""] {
            assert!(shipped_on(os).is_empty(), "{os}");
        }
        assert!(shipped_on("macos").iter().any(|a| a.id == SHIPPED_DEFAULT));
    }

    /// What `open` receives from each shipped command, spelled out rather than
    /// derived from the list.
    #[test]
    fn the_shipped_commands_fill_to_the_documented_argv() {
        let file = Path::new("/Users/mara/code/payout-service/src/lib.rs");
        let at_file = Fill {
            checkout: Path::new(CHECKOUT),
            file: Some(file),
            line: Some(3),
            code: &Code::NONE,
        };
        let argv: Vec<(Vec<String>, Option<Vec<String>>)> = shipped_on("macos")
            .iter()
            .map(|a| {
                (
                    filled(&a.command, &at_checkout()),
                    a.file_command.as_deref().map(|c| filled(c, &at_file)),
                )
            })
            .collect();
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            argv,
            [
                (
                    s(&["open", "-a", "Visual Studio Code", CHECKOUT]),
                    Some(s(&[
                        "open",
                        "-a",
                        "Visual Studio Code",
                        file.to_str().unwrap()
                    ]))
                ),
                (s(&["open", "-a", "Terminal", CHECKOUT]), None),
                (s(&["open", "-a", "Fork", CHECKOUT]), None),
                (
                    s(&["open", "-a", "IntelliJ IDEA", CHECKOUT]),
                    Some(s(&["open", "-a", "IntelliJ IDEA", file.to_str().unwrap()]))
                ),
            ]
        );
    }

    /// A file path stays below the checkout by its words.
    #[test]
    fn a_file_outside_the_checkout_by_its_words_is_refused() {
        let checkout = Path::new(CHECKOUT);
        assert_eq!(
            inside(checkout, "src/sepa/retry.rs"),
            Some(PathBuf::from(
                "/Users/mara/code/payout-service/src/sepa/retry.rs"
            ))
        );
        assert_eq!(
            inside(checkout, "./README.md"),
            Some(PathBuf::from("/Users/mara/code/payout-service/README.md"))
        );
        for bad in ["../other/x", "src/../../x", "/etc/passwd", "", "."] {
            assert_eq!(inside(checkout, bad), None, "{bad:?}");
        }
    }

    fn draft(id: Option<&str>, name: &str, command: &str, file: Option<&str>) -> Draft {
        Draft {
            id: id.map(str::to_owned),
            name: name.to_owned(),
            command: command.to_owned(),
            file_command: file.map(str::to_owned),
            ..Draft::default()
        }
    }

    /// Saving keeps an id an entry has, makes one for a new entry from its
    /// name, never gives two entries one, and trims what was typed.
    #[test]
    fn a_saved_list_keeps_ids_and_makes_new_ones_from_names() {
        let list = settle(&[
            draft(Some("vscode"), " Code ", " code {path} ", Some("  ")),
            draft(None, "My Git Viewer!", "MyGitViewer {path}", None),
            draft(
                None,
                "My git viewer",
                "other {path}",
                Some("x --open {file}"),
            ),
            draft(Some("vscode"), "Copy", "code2 {path}", None),
            draft(None, "+++", "z {path}", None),
        ])
        .unwrap();
        let ids: Vec<&str> = list.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            ["vscode", "my-git-viewer", "my-git-viewer-2", "copy", "app"]
        );
        assert_eq!(list[0].name, "Code");
        assert_eq!(list[0].command, "code {path}");
        assert_eq!(list[0].file_command, None, "a blank file command is none");
        assert_eq!(list[2].file_command.as_deref(), Some("x --open {file}"));
    }

    /// An entry that cannot be saved is named, with what is wrong with it.
    #[test]
    fn a_list_with_an_unusable_entry_is_refused_by_name() {
        assert_eq!(
            settle(&[
                draft(None, "A", "a {path}", None),
                draft(None, " ", "b", None)
            ]),
            Err(ListError::NoName(1))
        );
        let error = settle(&[draft(None, "Viewer", "v {branch}", None)]).unwrap_err();
        assert_eq!(
            error,
            ListError::Command {
                name: "Viewer".to_owned(),
                object: ObjectKind::Repository,
                error: TemplateError::NotForObject {
                    name: "branch".to_owned(),
                    object: ObjectKind::Repository
                },
            }
        );
        assert!(error.to_string().contains("Viewer's command"), "{error}");
        let error = settle(&[draft(None, "Viewer", "v {path}", Some("v \"{file}"))]).unwrap_err();
        assert!(
            error.to_string().contains("Viewer's file command"),
            "{error}"
        );
        let error = settle(&[Draft {
            range_command: Some("d {sha}".into()),
            ..draft(None, "Diff", "d {path}", None)
        }])
        .unwrap_err();
        assert!(
            error.to_string().contains("Diff's range command"),
            "{error}"
        );
        let saved = settle(&[Draft {
            commit_command: Some(" tig show {sha} ".into()),
            range_command: Some(" ".into()),
            ..draft(None, "Tig", "tig {path}", None)
        }])
        .unwrap();
        assert_eq!(saved[0].commit_command.as_deref(), Some("tig show {sha}"));
        assert_eq!(saved[0].range_command, None, "a blank command is none");
        assert_eq!(
            settle(&[draft(None, "Viewer", "  ", None)]).unwrap_err(),
            ListError::Command {
                name: "Viewer".to_owned(),
                object: ObjectKind::Repository,
                error: TemplateError::Empty,
            }
        );
    }
}
