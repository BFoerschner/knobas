//! *Open in…* (#501, #844, R-8.21 to R-8.24): the applications a person runs
//! at a checkout, and the argument vector one of them becomes.
//!
//! An **application** is a name, a command and an optional command for a
//! file. Its placeholders are `{path}` (the checkout), `{file}` (a file inside
//! it), `{line}` (a line number) and `{name}` (the checkout folder's name).
//!
//! # Every value comes from this disk (ADR-0016)
//!
//! [`fill`] takes a [`Fill`]: the checkout path knobas resolved on this
//! machine, a file path the caller has already found *inside* that checkout
//! on disk ([`inside`] is the lexical half of that check), and a number.
//! `{name}` is the checkout folder's own name, read off the path, not the
//! mirrored repository's name. There is no `{branch}`, `{title}` or `{url}`:
//! each would be a field a remote system wrote, and a placeholder this module
//! does not know is refused by name.
//!
//! # No shell
//!
//! The answer is an argv. Quotes in a command only group words; a path with a
//! space in it, or a `;`, is one argument and stays one.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The placeholders a command may carry, in the order the dialog offers them.
pub const PLACEHOLDERS: [&str; 4] = ["path", "file", "line", "name"];

/// One application on the *Open in…* menu.
///
/// Issue #844, R-8.21. Its commands take `{path}`, `{file}`, `{line}` and
/// `{name}`, all filled from this disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
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
                 {{file}}, {{line}} and {{name}}, all from this disk"
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
    fill(
        template,
        &Fill {
            checkout: Path::new("/checkout"),
            file: None,
            line: None,
        },
    )
    .map(|_| ())
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
            other => return Err(TemplateError::UnknownPlaceholder(other.to_owned())),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// An application as a person typed it in the dialog: a new one has no id
/// yet.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, ts_rs::TS)]
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
}

/// Why a list of applications cannot be saved, naming the entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListError {
    /// The entry at this position has a blank name.
    NoName(usize),
    /// This application's command (or, with `for_file`, its file command)
    /// cannot be run.
    Command {
        name: String,
        for_file: bool,
        error: TemplateError,
    },
}

impl std::fmt::Display for ListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoName(index) => write!(f, "application {} has no name", index + 1),
            Self::Command {
                name,
                for_file,
                error,
            } => {
                let which = if *for_file { "file command" } else { "command" };
                write!(f, "{name}'s {which} cannot be run: {error}")
            }
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
        check(command).map_err(|error| ListError::Command {
            name: name.to_owned(),
            for_file: false,
            error,
        })?;
        let file_command = draft
            .file_command
            .as_deref()
            .map(str::trim)
            .filter(|file| !file.is_empty());
        if let Some(file) = file_command {
            check(file).map_err(|error| ListError::Command {
                name: name.to_owned(),
                for_file: true,
                error,
            })?;
        }
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
            file_command: file_command.map(str::to_owned),
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
        for name in ["branch", "title", "repo_url", "web_url", "PATH", ""] {
            let error = fill(&format!("code {{{name}}} {{path}}"), &at_checkout())
                .expect_err("only the four placeholders fill");
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
                for_file: false,
                error: TemplateError::UnknownPlaceholder("branch".to_owned()),
            }
        );
        assert!(error.to_string().contains("Viewer's command"), "{error}");
        let error = settle(&[draft(None, "Viewer", "v {path}", Some("v \"{file}"))]).unwrap_err();
        assert!(
            error.to_string().contains("Viewer's file command"),
            "{error}"
        );
        assert_eq!(
            settle(&[draft(None, "Viewer", "  ", None)]).unwrap_err(),
            ListError::Command {
                name: "Viewer".to_owned(),
                for_file: false,
                error: TemplateError::Empty,
            }
        );
    }
}
