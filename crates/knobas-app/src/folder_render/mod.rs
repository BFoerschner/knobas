//! Folder renderers (ux17, ADR-0044): a program the person names in Settings
//! › Renderers that takes a folder of a repository and makes a picture, text,
//! Markdown or a page out of it, drawn on the Code page beside the folder.
//!
//! Björn, on PR #2479: "I might want to build a tool that takes a path to the
//! folder and outputs a png, a text, or anything. Because the lowcode tool
//! takes a folder and its contents and generates something out of it."
//!
//! # One settings row
//!
//! The list is one `knobas.setting` row, [`FOLDERS_KEY`], a JSON array of
//! [`FolderRenderer`], for the file renderers' reason (`crate::renderers`): a
//! short list a person types once is not a relation. No migration. A row
//! that does not parse reads as no folder renderers.
//!
//! # What the program is given (ADR-0044, amending ADR-0016 and ADR-0033)
//!
//! - The program and its arguments are what the person typed, split by
//!   [`knobas_core::checkout::open::words`], no shell. Three placeholders and
//!   no others: `{path}`, a copy knobas wrote of the folder at the commit the
//!   page shows; `{out}`, a path in an empty folder the program may write
//!   its one output file to (text after it stays: `{out}.png`); or, instead
//!   of `{out}`, `{outdir}`, an empty folder it writes a folder of files
//!   into (ADR-0046, [`keep`]). All are paths knobas made; the copy's own
//!   name is the folder's when it passes [`copy::copy_name`]'s grammar, else
//!   `folder`.
//! - It runs in an empty folder of its own (not the copy: on Windows a
//!   program's children are looked for in the current directory before
//!   `PATH`), with stdin closed, the parent environment less every `KNOBAS_*`
//!   variable, and is killed with everything it started at its timeout.
//! - The output is the file under `{out}`, or stdout when the command has no
//!   `{out}`, capped at [`OUTPUT_CAP`]; or the files under `{outdir}`, kept
//!   by [`keep`] in the [`store`].
//!
//! The copy, the run and the cache are [`copy`], [`run`] and [`runs`].

pub mod copy;
pub mod keep;
pub mod made;
pub mod resolve;
pub mod run;
pub mod runs;
pub mod store;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::IpcError;
use crate::settings::{read_value, write_value};

/// The `knobas.setting` key holding the list.
pub const FOLDERS_KEY: &str = "render.folders";

/// The timeout a folder renderer gets until the person sets one: a generator
/// is slower than a file renderer.
pub const DEFAULT_TIMEOUT_SECS: u32 = 30;
/// The longest timeout a folder renderer may ask for.
pub const MAX_TIMEOUT_SECS: u32 = 600;
/// The most output a run may make; past it the run is stopped and refused.
pub const OUTPUT_CAP: usize = 64 * 1024 * 1024;

/// The placeholder for the folder's copy.
pub const PATH: &str = "{path}";
/// The placeholder for the output file's path.
pub const OUT: &str = "{out}";
/// The placeholder for an empty folder the program writes a folder of files
/// into (ADR-0046): a command holding it makes [`FolderOutput::Folder`].
pub const OUTDIR: &str = "{outdir}";

/// What a folder renderer makes. `detect` reads it from the bytes (and the
/// output file's extension, for Markdown).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "app.ts")]
pub enum FolderOutput {
    #[default]
    Detect,
    /// PNG, JPEG, GIF, WebP or SVG, told by the bytes.
    Image,
    Text,
    Markdown,
    /// Drawn sanitized in a frame that runs no script.
    Html,
    /// A folder of files written into `{outdir}`, kept by knobas and browsed
    /// on the Code page in the folder's place (ADR-0046). Set by the command
    /// holding `{outdir}`, never by itself.
    Folder,
}

/// One saved folder renderer, Settings › Renderers › *Folder renderers*.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct FolderRenderer {
    /// Made from the name when first saved, kept through a rename.
    pub id: String,
    pub name: String,
    /// The program and its arguments, as typed: quotes group words, no shell,
    /// `{path}` and one of `{out}` and `{outdir}` the only placeholders.
    pub command: String,
    /// File globs one of the folder's own files must match (`*.lcproj`);
    /// empty takes any folder.
    pub holds: Vec<String>,
    /// Globs the folder's path must match (`docs/*`, `flows`); a glob with no
    /// `/` is matched against the folder's name. Empty takes any folder.
    pub folders: Vec<String>,
    pub output: FolderOutput,
    /// Killed after this many seconds (the copy is not counted).
    pub timeout_secs: u32,
}

/// A folder renderer as Settings sends it: a new one has no id yet, and no
/// timeout is the default.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct FolderRendererDraft {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub holds: Vec<String>,
    #[serde(default)]
    pub folders: Vec<String>,
    #[serde(default)]
    pub output: FolderOutput,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
}

/// The folder renderer a person chose for one folder of one repository (ux26, the pick): one `knobas.setting` row of
/// them, so a backup carries it, as it carries `render.defaults`.
pub const PICKS_KEY: &str = "render.folder_picks";
/// The most picks kept: past it the oldest goes.
pub const MAX_PICKS: usize = 2_000;

/// One pick: the renderer a folder of a repository gets, wherever a folder renderer is chosen for it (`docs/decisions/
/// 2026-11-20-a-comparison-is-drawn-rendered-and-may-end-at-the-working-tree.md` ruling 10). A pick whose renderer is
/// gone is passed over and left in place, so a renderer saved again under its id takes its folders back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct FolderPick {
    /// The repository's entity id.
    pub repo: String,
    /// The folder's path from the top; `""` the top.
    pub folder: String,
    /// The saved renderer's id.
    pub renderer: String,
}

/// The picks, the newest last.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn picks(pool: &PgPool) -> Result<Vec<FolderPick>, IpcError> {
    Ok(read_value(pool, PICKS_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// `picks` with `folder` of `repo` picked to `renderer` (`None`: no pick), the newest last, at most [`MAX_PICKS`].
#[must_use]
pub fn with_pick(
    mut picks: Vec<FolderPick>,
    repo: &str,
    folder: &str,
    renderer: Option<&str>,
) -> Vec<FolderPick> {
    let folder = folder.trim_matches('/');
    picks.retain(|pick| !(pick.repo == repo && pick.folder == folder));
    if let Some(renderer) = renderer {
        picks.push(FolderPick {
            repo: repo.to_owned(),
            folder: folder.to_owned(),
            renderer: renderer.to_owned(),
        });
    }
    let over = picks.len().saturating_sub(MAX_PICKS);
    picks.drain(..over);
    picks
}

/// Picks `renderer` for `folder` of the repository `entity_id` names (a branch's id names its repository), and answers
/// the picks as stored.
///
/// # Errors
///
/// As [`picks`]; [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn set_pick(
    pool: &PgPool,
    entity_id: &str,
    folder: &str,
    renderer: Option<&str>,
) -> Result<Vec<FolderPick>, IpcError> {
    let repo = crate::checkout::repository_of(pool, entity_id)
        .await
        .unwrap_or_else(|| entity_id.to_owned());
    let next = with_pick(picks(pool).await?, &repo, folder, renderer);
    let value = serde_json::to_value(&next).map_err(IpcError::internal)?;
    write_value(pool, PICKS_KEY, &value).await?;
    Ok(next)
}

/// The saved folder renderers, in the person's order.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn list(pool: &PgPool) -> Result<Vec<FolderRenderer>, IpcError> {
    Ok(read_value(pool, FOLDERS_KEY)
        .await?
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default())
}

/// Save the whole list, checked and trimmed, and answer it as stored.
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for an entry with no name, a
/// command that cannot be run, one without `{path}` or with a placeholder
/// other than `{path}` and `{out}`, naming it;
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn save(
    pool: &PgPool,
    drafts: &[FolderRendererDraft],
) -> Result<Vec<FolderRenderer>, IpcError> {
    let settled = settle(drafts)?;
    let value = serde_json::to_value(&settled).map_err(IpcError::internal)?;
    write_value(pool, FOLDERS_KEY, &value).await?;
    Ok(settled)
}

/// The drafts as they are stored: names and globs trimmed, ids settled, the
/// timeout held to 1 to [`MAX_TIMEOUT_SECS`].
///
/// # Errors
///
/// As [`save`].
pub fn settle(drafts: &[FolderRendererDraft]) -> Result<Vec<FolderRenderer>, IpcError> {
    let mut out: Vec<FolderRenderer> = Vec::with_capacity(drafts.len());
    for (index, draft) in drafts.iter().enumerate() {
        let name = draft.name.trim();
        if name.is_empty() {
            return Err(IpcError::invalid(format!(
                "folder renderer {} has no name",
                index + 1
            )));
        }
        let command = crate::settings::checked_command(name, &draft.command)?;
        check_placeholders(command).map_err(|why| IpcError::invalid(format!("{name}: {why}")))?;
        // `{outdir}` decides the kind, so the command and *It makes* cannot
        // disagree.
        let output = if placeholders(command).contains(&OUTDIR) {
            FolderOutput::Folder
        } else if draft.output == FolderOutput::Folder {
            return Err(IpcError::invalid(format!(
                "{name}: a folder of files needs {OUTDIR} where the tool takes its output folder"
            )));
        } else {
            draft.output
        };
        let taken: Vec<&str> = out.iter().map(|r| r.id.as_str()).collect();
        let id = crate::slug::settled_id(draft.id.as_deref(), name, "folder", &taken);
        let list = |items: &[String]| -> Vec<String> {
            items
                .iter()
                .map(|item| item.trim().trim_matches('/'))
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect()
        };
        out.push(FolderRenderer {
            id,
            name: name.to_owned(),
            command: command.to_owned(),
            holds: list(&draft.holds),
            folders: list(&draft.folders),
            output,
            timeout_secs: draft
                .timeout_secs
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .clamp(1, MAX_TIMEOUT_SECS),
        });
    }
    Ok(out)
}

/// The slot a folder's render runs in.
#[must_use]
pub fn slot(entity_id: &str, renderer: &str, folder: &str) -> runs::Slot {
    runs::Slot {
        entity_id: entity_id.to_owned(),
        renderer: renderer.to_owned(),
        folder: folder.trim_matches('/').to_owned(),
    }
}

/// Saved renderer `renderer` on `folder` of `entity_id` at `commit`: the
/// folder resolved where it is read from, then [`runs::Runs::view`].
///
/// # Errors
///
/// [`Invalid`](crate::IpcErrorCode::Invalid) for a renderer that is not saved;
/// [`Internal`](crate::IpcErrorCode::Internal) for a query failure.
#[allow(
    clippy::too_many_arguments,
    reason = "the command's arguments, one each"
)]
pub async fn ask(
    pool: &PgPool,
    forge: Option<&resolve::Forge>,
    entity_id: &str,
    renderer: &str,
    folder: &str,
    commit: Option<&str>,
    paths: Option<&str>,
    ask: runs::Ask,
) -> Result<runs::FolderRenderView, IpcError> {
    let saved = list(pool)
        .await?
        .into_iter()
        .find(|r| r.id == renderer)
        .ok_or_else(|| {
            IpcError::invalid(format!(
                "'{renderer}' is not one of your folder renderers: see Settings › Renderers"
            ))
        })?;
    let resolved = resolve::resolve(pool, forge, &saved, entity_id, folder, commit, paths).await?;
    Ok(runs::Runs::global().view(&slot(entity_id, renderer, folder), &saved, resolved, ask))
}

/// Every `{word}` in `command`, in order: what [`check_placeholders`] holds
/// to `{path}` and `{out}`.
fn placeholders(command: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = command;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let word = &after[..close];
        if !word.is_empty() && word.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            found.push(&rest[open..open + close + 2]);
        }
        rest = &after[close + 1..];
    }
    found
}

/// `{path}` must be there, and nothing but `{path}` and one of `{out}` and
/// `{outdir}`: a placeholder for a branch, a commit or a name would put a
/// value from the mirror in an argument (ADR-0016).
///
/// # Errors
///
/// What is wrong, in words.
pub fn check_placeholders(command: &str) -> Result<(), String> {
    let found = placeholders(command);
    if let Some(other) = found
        .iter()
        .find(|p| **p != PATH && **p != OUT && **p != OUTDIR)
    {
        return Err(format!(
            "{other} is not something a folder renderer is given: it takes {PATH}, the folder, and {OUT}, \
             a file to write to, or {OUTDIR}, a folder to write files into"
        ));
    }
    if found.contains(&OUT) && found.contains(&OUTDIR) {
        return Err(format!(
            "{OUT} is one file and {OUTDIR} is a folder of files: use one of them"
        ));
    }
    if !found.contains(&PATH) {
        return Err(format!(
            "the command needs {PATH} where the folder goes (it runs in an empty folder of its own)"
        ));
    }
    Ok(())
}

/// Whether `pattern` matches `text`, ignoring case: `*` any run of characters
/// (`/` too), `?` one, as `render/registry.ts` matches a file pattern.
#[must_use]
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    // The classic two-pointer wildcard match, with the last `*` to go back to.
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

/// Whether a folder glob takes the folder at `path` (`""` the top): a glob
/// with a `/` against the whole path, else against the folder's own name.
#[must_use]
pub fn folder_matches(pattern: &str, path: &str) -> bool {
    let target = if pattern.contains('/') {
        path
    } else {
        path.rsplit('/').next().unwrap_or(path)
    };
    glob_matches(pattern, target)
}

impl FolderRenderer {
    /// Whether this renderer takes the folder at `path` whose own files are
    /// `files` (names, not paths): both lists hold, an empty one always.
    pub fn applies<'a>(&self, path: &str, files: impl IntoIterator<Item = &'a str>) -> bool {
        if !self.folders.is_empty() && !self.folders.iter().any(|g| folder_matches(g, path)) {
            return false;
        }
        if self.holds.is_empty() {
            return true;
        }
        files
            .into_iter()
            .any(|name| self.holds.iter().any(|g| glob_matches(g, name)))
    }

    /// What of the entry decides its output, hashed into the cache key: a
    /// rename or a new timeout keeps what was drawn, a new command does not.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        use sha2::Digest as _;
        let mut hash = sha2::Sha256::new();
        hash.update(self.id.as_bytes());
        hash.update([0]);
        hash.update(self.command.as_bytes());
        hash.update([0]);
        hash.update(format!("{:?}", self.output).as_bytes());
        hex(&hash.finalize()[..12])
    }
}

/// Lowercase hex of `bytes`.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// The folders of a tree that a renderer holding globs takes, per renderer:
/// what the Code page's tree marks without reading each folder's listing.
/// Renderers with no `holds` glob are left out (the page matches them by
/// path alone).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "app.ts")]
pub struct FolderMarks {
    pub renderer: String,
    /// Folder paths from the top (`""` the top itself), in tree order.
    pub folders: Vec<String>,
}

/// [`FolderMarks`] over a tree's files, each its whole path from the top.
#[must_use]
pub fn marks<'a>(
    renderers: &[FolderRenderer],
    files: impl IntoIterator<Item = &'a str>,
) -> Vec<FolderMarks> {
    let wanted: Vec<&FolderRenderer> = renderers.iter().filter(|r| !r.holds.is_empty()).collect();
    let mut out: Vec<FolderMarks> = wanted
        .iter()
        .map(|r| FolderMarks {
            renderer: r.id.clone(),
            folders: Vec::new(),
        })
        .collect();
    if wanted.is_empty() {
        return out;
    }
    // A folder's own files are not all together in git's order (a subfolder's
    // come between), so each renderer keeps the folders it has named.
    let mut named: Vec<std::collections::HashSet<&str>> = vec![Default::default(); wanted.len()];
    for file in files {
        let (dir, name) = file.rsplit_once('/').unwrap_or(("", file));
        for (at, renderer) in wanted.iter().enumerate() {
            if named[at].contains(dir) || !renderer.applies(dir, [name]) {
                continue;
            }
            named[at].insert(dir);
            out[at].folders.push(dir.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{FolderPick, MAX_PICKS, with_pick};

    fn pick(repo: &str, folder: &str, renderer: &str) -> FolderPick {
        FolderPick {
            repo: repo.to_owned(),
            folder: folder.to_owned(),
            renderer: renderer.to_owned(),
        }
    }

    #[test]
    fn picks_are_kept_per_repository_and_folder() {
        let picks = with_pick(Vec::new(), "g:o/a", "flows/sepa", Some("lowcode"));
        let picks = with_pick(picks, "g:o/b", "flows/sepa", Some("flowviz"));
        let picks = with_pick(picks, "g:o/a", "/flows/sepa/", Some("flowviz"));
        assert_eq!(
            picks,
            vec![
                pick("g:o/b", "flows/sepa", "flowviz"),
                pick("g:o/a", "flows/sepa", "flowviz")
            ]
        );
        let picks = with_pick(picks, "g:o/a", "flows/sepa", None);
        assert_eq!(picks, vec![pick("g:o/b", "flows/sepa", "flowviz")]);
    }

    #[test]
    fn the_oldest_goes_past_the_cap() {
        let mut picks = Vec::new();
        for n in 0..=MAX_PICKS {
            picks = with_pick(picks, "g:o/a", &format!("f{n}"), Some("r"));
        }
        assert_eq!(picks.len(), MAX_PICKS);
        assert_eq!(picks[0].folder, "f1");
        assert_eq!(picks.last().unwrap().folder, format!("f{MAX_PICKS}"));
    }

    use super::*;

    fn draft(name: &str, command: &str) -> FolderRendererDraft {
        FolderRendererDraft {
            id: None,
            name: name.to_owned(),
            command: command.to_owned(),
            holds: vec![" *.lcproj ".to_owned(), String::new()],
            folders: vec!["/flows/".to_owned()],
            output: FolderOutput::Image,
            timeout_secs: Some(9000),
        }
    }

    /// A saved entry is trimmed, given an id from its name, and held to the
    /// longest timeout.
    #[test]
    fn a_draft_is_settled() {
        let settled = settle(&[draft(" Low code ", "lc render {path} -o {out}.png")]).unwrap();
        let one = &settled[0];
        assert_eq!(one.name, "Low code");
        assert_eq!(one.id, "low-code");
        assert_eq!(one.holds, ["*.lcproj"]);
        assert_eq!(one.folders, ["flows"]);
        assert_eq!(one.timeout_secs, MAX_TIMEOUT_SECS);
        let defaulted = settle(&[FolderRendererDraft {
            timeout_secs: None,
            ..draft("x", "tree {path}")
        }])
        .unwrap();
        assert_eq!(defaulted[0].timeout_secs, DEFAULT_TIMEOUT_SECS);
    }

    /// `{path}` is needed, and nothing but `{path}` and `{out}` is taken:
    /// `{branch}` would be a mirrored value as an argument.
    #[test]
    fn only_path_and_out_are_placeholders() {
        assert!(check_placeholders("tree {path}").is_ok());
        assert!(check_placeholders("lc {path} --out={out}.png").is_ok());
        let missing = check_placeholders("tree .").unwrap_err();
        assert!(missing.contains("{path}"), "{missing}");
        for other in ["{name}", "{branch}", "{sha}", "{file}"] {
            let refused =
                check_placeholders(&format!("tool {path} {other}", path = PATH)).unwrap_err();
            assert!(refused.contains(other), "{refused}");
        }
        // Braces that are not a placeholder (a JSON argument) are text.
        assert!(check_placeholders(r#"tool {path} '{"a": 1}'"#).is_ok());
        let error = settle(&[draft("x", "tool {path} {branch}")]).unwrap_err();
        assert!(error.message.contains("{branch}"), "{error:?}");
    }

    #[test]
    fn globs_match_as_the_file_patterns_do() {
        assert!(glob_matches("*.lcproj", "App.LCPROJ"));
        assert!(glob_matches("a?c", "abc"));
        assert!(!glob_matches("a?c", "ac"));
        assert!(glob_matches("*", ""));
        assert!(!glob_matches("*.md", "readme.txt"));
        assert!(glob_matches("docs/*", "docs/a/b"));
        assert!(folder_matches("flows", "app/flows"));
        assert!(!folder_matches("flows", "flows/app"));
        assert!(folder_matches("app/*", "app/flows"));
        assert!(!folder_matches("app/*", "lib/app/flows"));
    }

    /// Both lists hold, an empty one always: a folder of the right name
    /// without the project file is not taken, nor the project file elsewhere.
    #[test]
    fn a_renderer_takes_a_folder_when_both_lists_hold() {
        let r = &settle(&[draft("x", "t {path}")]).unwrap()[0];
        assert!(r.applies("app/flows", ["main.lcproj", "a.txt"]));
        assert!(!r.applies("app/flows", ["a.txt"]));
        assert!(!r.applies("app/other", ["main.lcproj"]));
        let any = FolderRenderer {
            holds: vec![],
            folders: vec![],
            ..r.clone()
        };
        assert!(any.applies("", std::iter::empty()));
    }

    /// `{outdir}` is a placeholder, and a command holding it is stored as a
    /// folder kind whatever the draft said.
    #[test]
    fn outdir_is_a_placeholder_and_sets_the_kind() {
        assert!(check_placeholders("lc docs {path} --out {outdir}/site").is_ok());
        let settled = settle(&[FolderRendererDraft {
            output: FolderOutput::Text,
            ..draft("x", "lc docs {path} --out {outdir}")
        }])
        .unwrap();
        assert_eq!(settled[0].output, FolderOutput::Folder);
        // `{out}` is not `{outdir}`'s prefix in the check.
        let plain = settle(&[draft("y", "lc {path} {out}.png")]).unwrap();
        assert_eq!(plain[0].output, FolderOutput::Image);
    }

    #[test]
    fn out_and_outdir_together_are_refused() {
        let refused = check_placeholders("lc {path} {out} {outdir}").unwrap_err();
        assert_eq!(
            refused,
            "{out} is one file and {outdir} is a folder of files: use one of them"
        );
        assert!(settle(&[draft("x", "lc {path} {out} {outdir}")]).is_err());
    }

    #[test]
    fn a_folder_kind_without_outdir_is_refused() {
        let error = settle(&[FolderRendererDraft {
            output: FolderOutput::Folder,
            ..draft("Docs", "lc {path} {out}")
        }])
        .unwrap_err();
        assert_eq!(
            error.message,
            "Docs: a folder of files needs {outdir} where the tool takes its output folder"
        );
    }

    /// A row saved before `{outdir}` (at `dc367839`) reads and settles to the
    /// same value: no migration.
    #[test]
    fn a_stored_list_without_outdir_reads_as_before() {
        let stored = r#"[{"id":"lc","name":"Low code","command":"lc {path} {out}.png","holds":["*.lcproj"],"folders":[],"output":"image","timeout_secs":60},
                          {"id":"tree","name":"Tree","command":"tree {path}","holds":[],"folders":["flows"],"output":"detect","timeout_secs":30}]"#;
        let read: Vec<FolderRenderer> = serde_json::from_str(stored).unwrap();
        assert_eq!(read[0].output, FolderOutput::Image);
        assert_eq!(read[1].output, FolderOutput::Detect);
        let drafts: Vec<FolderRendererDraft> = serde_json::from_str(stored).unwrap();
        assert_eq!(settle(&drafts).unwrap(), read);
        // A draft that names no output is Detect, not Folder.
        let bare: FolderRendererDraft =
            serde_json::from_str(r#"{"name":"t","command":"t {path}"}"#).unwrap();
        assert_eq!(bare.output, FolderOutput::Detect);
    }

    /// The tree's marks: each folder that holds a matching file of its own,
    /// once, and nothing for a renderer that holds no glob.
    #[test]
    fn marks_name_each_folder_holding_a_match_once() {
        let mut list = settle(&[draft("x", "t {path}")]).unwrap();
        list[0].folders.clear();
        list.push(FolderRenderer {
            id: "all".to_owned(),
            holds: vec![],
            ..list[0].clone()
        });
        let files = [
            "a.lcproj",
            "a/b.lcproj",
            "a/c.lcproj",
            "a/d/x.txt",
            "e/f.LCPROJ",
        ];
        let found = marks(&list, files);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].folders, ["", "a", "e"]);
    }

    /// The fingerprint follows the command and the output, not the name or
    /// the timeout.
    #[test]
    fn the_fingerprint_follows_what_decides_the_output() {
        let a = &settle(&[draft("x", "t {path}")]).unwrap()[0];
        let renamed = FolderRenderer {
            name: "y".to_owned(),
            timeout_secs: 5,
            ..a.clone()
        };
        assert_eq!(a.fingerprint(), renamed.fingerprint());
        let other = FolderRenderer {
            command: "t {path} -v".to_owned(),
            ..a.clone()
        };
        assert_ne!(a.fingerprint(), other.fingerprint());
        let text = FolderRenderer {
            output: FolderOutput::Text,
            ..a.clone()
        };
        assert_ne!(a.fingerprint(), text.fingerprint());
    }
}
