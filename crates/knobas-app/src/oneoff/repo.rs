//! A repository's own commands (ADR-0060, section 22): never run from the
//! repository. A file `.knobas/commands.toml` in the checkout on disk (never
//! the mirror or the forge) is read when a *Run…* menu of that repository
//! opens, and its commands are offered to be copied into the person's own
//! list after they have read each line. Once copied, a command is theirs,
//! scoped to that repository: a later change of the file changes nothing by
//! itself, and the menu then says one changed.
//!
//! ```toml
//! [[command]]
//! name = "ESLint"
//! command = "npx eslint {file}"
//! files = ["*.ts", "*.tsx"]
//! ```

use std::path::Path;

use knobas_core::checkout::oneoff::{self as rules, Draft, Kind, OneOff, Shows, StartIn, Stdin};
use serde::{Deserialize, Serialize};

/// The file, from the checkout's top.
pub const FILE: &str = ".knobas/commands.toml";
/// The most bytes of it read.
pub const CAP: u64 = 64 * 1024;

#[derive(Deserialize)]
struct Listed {
    #[serde(default)]
    command: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    command: String,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    folders: Vec<String>,
    #[serde(default)]
    has: Vec<String>,
    #[serde(default)]
    repo_holds: Vec<String>,
    #[serde(default)]
    stdin: Stdin,
    #[serde(default)]
    shows: Shows,
    #[serde(default)]
    start_in: StartIn,
    #[serde(default)]
    timeout_secs: Option<u32>,
}

/// One command the repository's file holds, as the copy dialog shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts", rename = "OneOffOffered")]
pub struct Offered {
    pub name: String,
    pub command: String,
    #[ts(optional = nullable)]
    pub kind: Option<Kind>,
    /// What Save would refuse; such a command cannot be copied.
    #[ts(optional = nullable)]
    pub refused: Option<String>,
    /// The person's copy of it, by id, where they made one.
    #[ts(optional = nullable)]
    pub copied: Option<String>,
    /// The copy's command line, where it differs from the file's now.
    #[ts(optional = nullable)]
    pub theirs: Option<String>,
    pub files: Vec<String>,
    pub folders: Vec<String>,
    pub has: Vec<String>,
    pub repo_holds: Vec<String>,
    pub stdin: Stdin,
    pub shows: Shows,
    pub start_in: StartIn,
    #[ts(optional = nullable)]
    pub timeout_secs: Option<u32>,
}

/// What `text`, the file's contents, offers against the person's `saved`
/// commands for repository `repo`.
///
/// # Errors
///
/// The file does not parse, in TOML's words.
pub fn offered(text: &str, repo: &str, saved: &[OneOff], os: &str) -> Result<Vec<Offered>, String> {
    let listed: Listed = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(listed
        .command
        .into_iter()
        .map(|e| {
            let copy = saved
                .iter()
                .find(|c| c.repos.iter().any(|r| r == repo) && c.name == e.name.trim());
            Offered {
                kind: rules::kind_of(&e.command, e.stdin).ok(),
                refused: rules::check(&e.command, e.stdin, os)
                    .err()
                    .map(|r| r.to_string()),
                copied: copy.map(|c| c.id.clone()),
                theirs: copy
                    .filter(|c| c.command != e.command.trim())
                    .map(|c| c.command.clone()),
                name: e.name.trim().to_owned(),
                command: e.command.trim().to_owned(),
                files: e.files,
                folders: e.folders,
                has: e.has,
                repo_holds: e.repo_holds,
                stdin: e.stdin,
                shows: e.shows,
                start_in: e.start_in,
                timeout_secs: e.timeout_secs,
            }
        })
        .collect())
}

/// The draft that copies `offered` into the person's list, scoped to `repo`.
#[must_use]
pub fn draft(offered: &Offered, repo: &str, id: Option<String>) -> Draft {
    Draft {
        id,
        name: offered.name.clone(),
        command: offered.command.clone(),
        stdin: offered.stdin,
        files: offered.files.clone(),
        folders: offered.folders.clone(),
        has: offered.has.clone(),
        repo_holds: offered.repo_holds.clone(),
        repos: vec![repo.to_owned()],
        shows: offered.shows,
        start_in: offered.start_in,
        timeout_secs: offered.timeout_secs,
        ..Draft::default()
    }
}

/// The file in the checkout at `root`, if it is there: read from the disk,
/// never followed out of the checkout, at most [`CAP`] bytes.
#[must_use]
pub fn read(root: &Path) -> Option<String> {
    let path = root.join(".knobas").join("commands.toml");
    let real = std::fs::canonicalize(&path).ok()?;
    let real_root = std::fs::canonicalize(root).ok()?;
    if !real.starts_with(&real_root) || !real.is_file() {
        return None;
    }
    let meta = std::fs::metadata(&real).ok()?;
    if meta.len() > CAP {
        return None;
    }
    std::fs::read_to_string(real).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = r#"
[[command]]
name = "ESLint"
command = "npx eslint {file}"
files = ["*.ts"]

[[command]]
name = "Unsafe"
command = "sh -c 'eslint {file}'"

[[command]]
name = "Tests"
command = "npm test"
"#;

    fn saved(command: &str) -> OneOff {
        rules::settle(
            &[Draft {
                name: "ESLint".to_owned(),
                command: command.to_owned(),
                repos: vec!["gitea:a/b".to_owned()],
                ..Draft::default()
            }],
            "linux",
        )
        .unwrap()
        .remove(0)
    }

    #[test]
    fn the_file_offers_its_commands_and_says_what_save_refuses() {
        let got = offered(TOML, "gitea:a/b", &[], "linux").unwrap();
        assert_eq!(
            got.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(),
            ["ESLint", "Unsafe", "Tests"]
        );
        assert_eq!(got[0].kind, Some(Kind::File));
        assert!(got[0].refused.is_none());
        assert!(got[1].refused.as_deref().unwrap().contains("quoted script"));
        assert_eq!(got[2].kind, Some(Kind::Repository));
        assert!(got.iter().all(|o| o.copied.is_none()));
        let d = draft(&got[0], "gitea:a/b", None);
        assert_eq!(d.repos, ["gitea:a/b"]);
        assert_eq!(d.files, ["*.ts"]);
    }

    #[test]
    fn a_copy_is_theirs_and_a_changed_line_is_said_not_taken() {
        let same = offered(TOML, "gitea:a/b", &[saved("npx eslint {file}")], "linux").unwrap();
        assert_eq!(same[0].copied.as_deref(), Some("eslint"));
        assert_eq!(same[0].theirs, None);
        let changed = offered(TOML, "gitea:a/b", &[saved("eslint --fix {file}")], "linux").unwrap();
        assert_eq!(changed[0].theirs.as_deref(), Some("eslint --fix {file}"));
        // A copy for another repository is not this one's.
        let other = offered(TOML, "gitea:a/c", &[saved("npx eslint {file}")], "linux").unwrap();
        assert!(other[0].copied.is_none());
    }

    #[test]
    fn a_file_that_does_not_parse_says_why_and_one_out_of_the_checkout_is_not_read() {
        assert!(offered("[[command]]\nname = ", "x", &[], "linux").is_err());
        let root = tempfile::tempdir().unwrap();
        assert_eq!(read(root.path()), None);
        std::fs::create_dir(root.path().join(".knobas")).unwrap();
        std::fs::write(root.path().join(".knobas/commands.toml"), TOML).unwrap();
        assert!(read(root.path()).unwrap().contains("ESLint"));
        #[cfg(unix)]
        {
            let other = tempfile::tempdir().unwrap();
            std::fs::write(other.path().join("x.toml"), TOML).unwrap();
            std::fs::remove_file(root.path().join(".knobas/commands.toml")).unwrap();
            std::os::unix::fs::symlink(
                other.path().join("x.toml"),
                root.path().join(".knobas/commands.toml"),
            )
            .unwrap();
            assert_eq!(read(root.path()), None);
        }
    }
}
