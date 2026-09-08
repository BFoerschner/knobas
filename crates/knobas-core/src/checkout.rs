//! Checkouts: what knobas knows about the clones on this disk.
//!
//! `CONTEXT.md`, **Checkout**: "a clone of a repo on this machine, found under
//! the **clones root** -- a directory setting knobas scans two levels deep,
//! matching each clone's remote to a repo entity by host and owner/repo -- or
//! set by hand per repo as an override. Knobas-owned data about the local
//! disk, never a field of the mirrored repo, and never written to."
//!
//! Two halves live here, and both are pure. The *finding* half (#499)
//! normalises a remote URL to a key and walks the clones root for directories
//! that carry one. The *template* half (#501) turns a command template a
//! person set in settings into the argument vector `knobas_app::checkout`
//! spawns -- [`expand`], and the one placeholder it fills in. The override,
//! the setting, the entity and the spawn itself are `knobas_app::checkout`;
//! this module has no database in it, no knowledge of an entity, and starts no
//! process.
//!
//! # Nothing here runs git (ADR-0016)
//!
//! A clone's remote is read out of `.git/config`, which is a file, and not out
//! of `git remote get-url`, which is a program. That is not a performance
//! choice: ADR-0016 says a program knobas spawns takes its arguments only from
//! what knobas found on the local disk or was told by hand, and the whole
//! point of the scan is that it runs over directories a *mirrored* repo named.
//! A `git` invoked with a path assembled here would be the first crack in
//! that.
//!
//! # The failure direction is absence
//!
//! Every function in the finding half misses rather than guesses: a URL it cannot parse into
//! host + owner/repo yields `None`, a directory whose `.git` is unreadable
//! contributes nothing, a config with no `origin` contributes nothing. A
//! wrongly *found* checkout is a path knobas would later hand to an editor
//! (#501); a wrongly *missed* one is a *no checkout* line and a clone command
//! to copy, which is the state a person can see and correct with the override.

use std::path::{Path, PathBuf};

/// A remote URL reduced to the three things that identify a repository.
///
/// Two remotes name the same repository exactly when their keys are equal, so
/// `git@gitea.example.com:tidewater/payout-service.git` and
/// `https://gitea.example.com/tidewater/payout-service/` are one key and the
/// scan matches either spelling against a repo entity's stored URL.
///
/// # Why all three parts are lower-cased
///
/// `host` because DNS is case-insensitive and nothing else would be defensible.
/// `owner` and `repo` because the three forges knobas mirrors or could mirror
/// -- Gitea, GitHub, GitLab -- all resolve an owner and a repository name
/// case-insensitively, so `Tidewater/Payout-Service` and
/// `tidewater/payout-service` are one repository and a key that told them apart
/// would miss a checkout for a difference the server does not have.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RemoteKey {
    pub host: String,
    pub owner: String,
    pub repo: String,
}

impl std::fmt::Display for RemoteKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.host, self.owner, self.repo)
    }
}

/// The key a remote URL reduces to, or nothing.
///
/// Accepts the two spellings git itself accepts for the same repository, plus
/// the browser URL a mirrored repo carries:
///
/// * `scheme://[user@]host[:port]/owner/repo[.git][/]` -- `https`, `http`,
///   `ssh` and `git`, and in fact any scheme, because the scheme is not part of
///   the identity;
/// * `[user@]host:owner/repo[.git]` -- git's scp-like form, which has no
///   scheme and no leading slash on the path.
///
/// The owner and the repository are the **last two** path segments, not the
/// first two: a GitLab subgroup puts the owning group deeper
/// (`/group/subgroup/repo`), and the last two are what a clone of it lands in.
/// Anything with fewer than two segments is a miss -- there is no repository
/// there to name.
///
/// # What is deliberately refused
///
/// A Windows path reads as a scp-like remote to a naive split on `:`, and
/// **one** guard is what keeps the dangerous spelling out: a scp-like host must
/// be longer than one character, so a drive letter is not a host. The spelling
/// that needs it is the drive-*relative* one with forward slashes,
/// `C:src/tidewater/payout-service`, whose path splits into segments an owner
/// and a repository could be read out of; mutation-checking says so, since it
/// is the one negative that fails when the guard is removed. Every other
/// Windows spelling is refused by rules that are there anyway -- `C:/src/a/b`
/// by the leading slash, and `C:\src\payout-service` and `C:\src\a\b` by
/// having no `/` and therefore one path segment, where an owner *and* a
/// repository are wanted. A third guard on "the path must carry a `/`" was
/// written and removed: mutation-checked, it killed nothing that the
/// one-segment miss did not already kill, and armour no test can see go is
/// armour nobody can maintain.
#[must_use]
pub fn remote_key(url: &str) -> Option<RemoteKey> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    let (authority, path) = split_remote(url)?;
    // `user@host:port` -- the user is a credential, not an identity, and the
    // port is a route to the same server. Neither is part of the repository.
    let host = authority.rsplit('@').next()?;
    let host = match host.rsplit_once(':') {
        // Only a numeric tail is a port; an IPv6 literal is full of colons and
        // is not one, and neither is a hostname somebody mistyped.
        Some((before, after)) if !after.is_empty() && after.bytes().all(|b| b.is_ascii_digit()) => {
            before
        }
        _ => host,
    };
    let host = host.trim_matches(['[', ']']).trim();
    if host.is_empty() {
        return None;
    }

    let mut segments = path.split('/').filter(|part| !part.is_empty());
    let last = segments.next_back()?;
    let owner = segments.next_back()?;
    let repo = last.strip_suffix(".git").unwrap_or(last);
    if repo.is_empty() || owner.is_empty() {
        return None;
    }

    Some(RemoteKey {
        host: host.to_ascii_lowercase(),
        owner: owner.to_ascii_lowercase(),
        repo: repo.to_ascii_lowercase(),
    })
}

/// Split a remote into `(authority, path)`, whichever of the two forms it is.
fn split_remote(url: &str) -> Option<(&str, &str)> {
    if let Some((_, rest)) = url.split_once("://") {
        let (authority, path) = rest.split_once('/')?;
        return Some((authority, path));
    }
    // The scp-like form. `rsplit` would take the last colon, which an IPv6
    // literal is made of; the *first* colon is the separator git uses.
    let (authority, path) = url.split_once(':')?;
    // `C:src/tidewater/payout-service` is not a remote: a one-character
    // authority is a drive letter, not a host, and that drive-relative
    // spelling is the one this guard alone refuses. `C:/src/a/b` is refused by
    // the leading slash, and the backslash spellings by having one path
    // segment where two are wanted -- see the doc comment for why there is no
    // third guard here.
    if authority.chars().count() < 2 || path.starts_with('/') {
        return None;
    }
    Some((authority, path))
}

/// The command a person copies when there is no checkout to open.
///
/// **Text, and only ever text.** Nothing in knobas runs it: ADR-0016's
/// consequence is written out as "nothing knobas spawns writes to a working
/// tree: no clone, no checkout, no fetch; a repo with no checkout shows the
/// clone command to copy". This is that string, built from the repo's own
/// stored URL and nothing else -- no clones root pasted in front of it, because
/// a person choosing where to clone is the decision the override exists for.
#[must_use]
pub fn clone_command(repo_url: &str) -> String {
    format!("git clone {}", repo_url.trim())
}

/// One clone the scan found: where it is, and what its `origin` says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundClone {
    pub path: PathBuf,
    /// The `origin` URL verbatim, as `.git/config` spells it.
    pub remote: String,
    pub key: RemoteKey,
}

/// How deep under the clones root a clone is looked for.
///
/// Two, which is the layout people actually have: `~/src/payout-service` and
/// `~/src/gitea.example.com/payout-service` are both reached, and a tree of
/// build output three deep is not walked. Spec #491: "two directory levels
/// under the root".
pub const SCAN_DEPTH: usize = 2;

/// Every clone under `root`, at most [`SCAN_DEPTH`] directories deep, ordered
/// by path.
///
/// A directory that **is** a clone is not descended into: its own
/// subdirectories are its working tree, and a submodule inside one is not a
/// checkout of anything a repo entity names. That is also what makes the depth
/// limit mean what it says -- `root/a/b/c` is never visited, whether or not
/// `a` and `b` are clones.
///
/// # A `.git` file is not a clone here
///
/// A linked worktree and a submodule carry a `.git` **file** pointing at a
/// directory elsewhere, and following it means resolving `commondir` to find
/// the config that holds the remotes. That is deliberately not done: spec #491
/// gives the worktree case to the hand-set override ("a checkout path set by
/// hand on a repo that the scan misses **or that has a worktree**"), and a
/// half-resolved worktree would be the guess this module refuses to make.
///
/// Ordered **shallowest first, then by path**, so two clones of one repository
/// resolve to the same one on every read -- an answer that changed with the
/// directory-listing order would send an editor somewhere new each time it was
/// opened. Shallow before deep because that is the one half of the order a
/// person can predict: a clone sitting directly in the root is the one they
/// mean, and `~/src/host/repo` is where a second copy ends up.
#[must_use]
pub fn scan(root: &Path) -> Vec<FoundClone> {
    let mut found = Vec::new();
    walk(root, 1, &mut found);
    found.sort_by(|a, b| {
        a.path
            .components()
            .count()
            .cmp(&b.path.components().count())
            .then_with(|| a.path.cmp(&b.path))
    });
    found
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<FoundClone>) {
    if depth > SCAN_DEPTH {
        return;
    }
    // An unreadable directory contributes nothing: a clones root that is a
    // typo, or a directory the user cannot read, is a *no checkout*, never an
    // error thrown at somebody who opened a repo.
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(clone) = read_clone(&path) {
            found.push(clone);
            // Not descended into -- see the doc comment.
            continue;
        }
        walk(&path, depth + 1, found);
    }
}

/// The clone `dir` is, if it is one.
fn read_clone(dir: &Path) -> Option<FoundClone> {
    let config = dir.join(".git").join("config");
    // `read_to_string` fails for a `.git` *file* too, which is the worktree
    // case the doc comment gives to the override.
    let text = std::fs::read_to_string(config).ok()?;
    let remote = origin_url(&text)?;
    let key = remote_key(&remote)?;
    Some(FoundClone {
        path: dir.to_path_buf(),
        remote,
        key,
    })
}

/// The `origin` remote's URL out of a git config file.
///
/// A hand-rolled read of the two lines that matter rather than an INI crate:
/// what is wanted is one value under one section, the failure direction is
/// absence, and a dependency whose error cases all collapse to `None` here
/// would be carrying nothing.
///
/// `origin` and no other remote. A clone with an `upstream` and no `origin` is
/// a miss, which the override answers -- picking some other remote would be
/// the guess about which server a person means, and on a fork the two are
/// different repositories.
fn origin_url(config: &str) -> Option<String> {
    let mut in_origin = false;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // `[remote "origin"]`, and the spelling git writes: the section
            // name is lower-case, the subsection is case-sensitive.
            in_origin = line.replace(char::is_whitespace, "") == "[remote\"origin\"]";
            continue;
        }
        if !in_origin {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("url")
        {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }
    None
}

/// The clone under `root` whose remote names the same repository as
/// `repo_url`, or nothing.
///
/// The whole scan runs and the first match in path order wins, so two clones
/// of one repository answer the same path on every read.
#[must_use]
pub fn find(root: &Path, repo_url: &str) -> Option<PathBuf> {
    let wanted = remote_key(repo_url)?;
    scan(root)
        .into_iter()
        .find(|clone| clone.key == wanted)
        .map(|clone| clone.path)
}

// -- the spawn half: command templates (#501, ADR-0016) ---------------------

/// The one placeholder a command template may carry.
///
/// **The whole of ADR-0016 is in this constant.** A spawned command takes no
/// argument from the mirror, so [`expand`] is handed a single value -- the
/// checkout on this disk -- and every other `{...}` in a template is refused
/// by name. There is deliberately no `{repo_url}`, no `{branch}` and no
/// `{title}`: each of those is a field a remote system wrote, and a template
/// that could interpolate one would be the remote-code path the ADR exists to
/// keep shut.
pub const PATH_PLACEHOLDER: &str = "path";

/// A button that runs something on this machine.
///
/// Three, because spec §5 and §11 name three: an editor, a second editor
/// people actually use, and a terminal. Adding a fourth is adding a settings
/// row and a button, and nothing else -- the expansion and the spawn know
/// nothing about which of them they are serving.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenAction {
    VsCode,
    JetBrains,
    Terminal,
}

/// Every action, in the order the panel draws them.
pub const OPEN_ACTIONS: [OpenAction; 3] = [
    OpenAction::VsCode,
    OpenAction::JetBrains,
    OpenAction::Terminal,
];

/// The macOS default for each action, in [`OPEN_ACTIONS`] order.
///
/// `open -a <application>` and not a CLI name, for all three: `code`, `idea`
/// and friends are optional shims a person installs from inside the editor,
/// and a default that names one would be *not configured* on a Mac that has
/// the application. `open` is in `/usr/bin` on every Mac there has ever been,
/// and it resolves the application by name through Launch Services -- the same
/// lookup the Dock does.
///
/// Named applications rather than bundle identifiers because this is a string
/// a person edits: "IntelliJ IDEA" is what they see in `/Applications`, and
/// the JetBrains user whose IDE is RustRover or PyCharm changes one word.
///
/// Declared unconditionally -- it is what [`default_template`] answers *on
/// macOS*, and a constant the tests can read on any platform -- so the
/// expansion of all three is asserted wherever the gate runs.
pub const MACOS_DEFAULT_TEMPLATES: [&str; 3] = [
    "open -a \"Visual Studio Code\" {path}",
    "open -a \"IntelliJ IDEA\" {path}",
    "open -a Terminal {path}",
];

impl OpenAction {
    /// The stored id: the settings key's suffix and the IPC argument.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::VsCode => "vscode",
            Self::JetBrains => "jetbrains",
            Self::Terminal => "terminal",
        }
    }

    /// What the button says.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::VsCode => "Open in VS Code",
            Self::JetBrains => "Open in JetBrains",
            Self::Terminal => "Open terminal here",
        }
    }

    /// The action with this id, if there is one.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        OPEN_ACTIONS.into_iter().find(|action| action.id() == id)
    }

    /// The template this action starts with on this platform, if any.
    ///
    /// macOS has three defaults; every other platform has none, and the button
    /// reads *not configured* until somebody sets one. That is spec #491's
    /// wording and it is the honest shape: knobas has never been run on
    /// Windows or Linux by anyone, so a default there would be a guess shipped
    /// as a fact -- and a guess that spawns a process is the wrong kind.
    #[must_use]
    pub fn default_template(self) -> Option<&'static str> {
        self.default_template_on(std::env::consts::OS)
    }

    /// The template this action starts with **on `os`**.
    ///
    /// The platform is an argument rather than a `cfg!`, and that is the whole
    /// reason this function exists: the gate runs on one operating system, so
    /// a rule written as `cfg!(target_os = ...)` has exactly one half of
    /// itself under test and the other half is prose. `os` takes
    /// [`std::env::consts::OS`]'s spelling -- `"macos"`, `"linux"`,
    /// `"windows"` -- and `default_template` passes this machine's.
    #[must_use]
    pub fn default_template_on(self, os: &str) -> Option<&'static str> {
        if os != "macos" {
            return None;
        }
        let index = OPEN_ACTIONS.iter().position(|action| *action == self)?;
        MACOS_DEFAULT_TEMPLATES.get(index).copied()
    }
}

/// Why a command template cannot be run.
///
/// Every one of these is reported to the person who typed the template, so
/// each `Display` says what is wrong with *their* string rather than what the
/// parser was doing when it gave up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateError {
    /// Nothing but whitespace: there is no program to run.
    Empty,
    /// A `"` or `'` that never closes.
    UnclosedQuote,
    /// A `{` with no `}` after it.
    UnclosedPlaceholder,
    /// No `{path}`, so the command would open nothing.
    NoPath,
    /// More than one `{path}`.
    RepeatedPath,
    /// A placeholder that is not `{path}`, carried so the message can name it.
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
            Self::NoPath => write!(
                f,
                "the command has no {{{PATH_PLACEHOLDER}}}, so it would open nothing"
            ),
            Self::RepeatedPath => write!(
                f,
                "the command uses {{{PATH_PLACEHOLDER}}} more than once, and knobas substitutes it once"
            ),
            Self::UnknownPlaceholder(name) => write!(
                f,
                "{{{name}}} is not something knobas fills in: the only value a command \
                 knobas runs may take is {{{PATH_PLACEHOLDER}}}, the checkout on this disk"
            ),
        }
    }
}

impl std::error::Error for TemplateError {}

/// The argument vector `template` becomes for `path`, or why it cannot.
///
/// # The signature is the argument rule
///
/// One `&Path` in, and it is the checkout. ADR-0016: *a spawned command takes
/// no argument from the mirror*. Nothing else is in scope to substitute here,
/// so no future edit to a caller can widen what a template reaches -- widening
/// it would mean changing this signature, which is the visible act the ADR
/// wants it to be.
///
/// # No shell
///
/// The answer is an argv, and the caller spawns `argv[0]` with the rest as
/// arguments. Nothing is passed to `sh -c`, so a checkout path containing a
/// space, a `$`, a `;` or a newline is one argument and stays one -- and a
/// template cannot pipe, redirect or chain. Quoting in the template is
/// therefore only about **grouping words into one argument**: `"` and `'` both
/// group, they do not nest, and neither survives into the argv.
///
/// # Substitution is textual, inside one word
///
/// `{path}` may sit anywhere in a word, so `--folder-uri=file://{path}` is a
/// usable template; the word it is in stays one argument however long the path
/// is. Exactly one `{path}` is required: none would open nothing, and more
/// than one is asking for a shape ("substituted once", spec #491) that no
/// editor's command line has.
///
/// # Errors
///
/// [`TemplateError`], one variant per way a person's string can be unusable.
pub fn expand(template: &str, path: &Path) -> Result<Vec<String>, TemplateError> {
    let words = split_words(template)?;
    if words.is_empty() {
        return Err(TemplateError::Empty);
    }
    let mut argv = Vec::with_capacity(words.len());
    let mut substitutions = 0usize;
    for word in words {
        argv.push(substitute(&word, path, &mut substitutions)?);
    }
    match substitutions {
        0 => Err(TemplateError::NoPath),
        1 => Ok(argv),
        _ => Err(TemplateError::RepeatedPath),
    }
}

/// Whether `template` is one knobas could run, without running anything.
///
/// The check a settings write makes before it stores what somebody typed: the
/// refusal belongs at the field they are typing in, not on the button they
/// press three days later. It is [`expand`] against a stand-in path, because
/// there is exactly one rule and a second copy of it would drift.
///
/// # Errors
///
/// [`TemplateError`], the same ones [`expand`] gives.
pub fn check_template(template: &str) -> Result<(), TemplateError> {
    expand(template, Path::new("/checkout")).map(|_| ())
}

/// Split a template into words, honouring `"` and `'` as grouping only.
///
/// A quote inside a word is allowed (`--dir="{path}"` is one word), which is
/// why this tracks a quote character rather than requiring a word to start
/// with one.
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
                // A word that is only `""` is still a word -- an empty
                // argument is a thing a command line can carry.
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

/// One word with its placeholder filled in, counting what it substituted.
fn substitute(word: &str, path: &Path, substitutions: &mut usize) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(word.len());
    let mut rest = word;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after.find('}').ok_or(TemplateError::UnclosedPlaceholder)?;
        let name = &after[..close];
        if name != PATH_PLACEHOLDER {
            return Err(TemplateError::UnknownPlaceholder(name.to_owned()));
        }
        // `to_string_lossy`: the argv this builds is `String`, and a path that
        // is not UTF-8 is a path nobody typed into a settings field on this
        // machine. The lossy character would arrive at the editor as a wrong
        // path, which is a visible failure rather than a silent one.
        out.push_str(&path.to_string_lossy());
        *substitutions += 1;
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(host: &str, owner: &str, repo: &str) -> Option<RemoteKey> {
        Some(RemoteKey {
            host: host.to_owned(),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
        })
    }

    /// The one property the whole feature rests on: the spellings a person's
    /// `.git/config` may hold and the URL a mirrored repo carries reduce to
    /// **one** key. Every line here is a remote git itself accepts.
    #[test]
    fn ssh_and_https_forms_normalise_to_the_same_key() {
        let expected = key("gitea.example.com", "tidewater", "payout-service");
        for url in [
            "git@gitea.example.com:tidewater/payout-service.git",
            "git@gitea.example.com:tidewater/payout-service",
            "ssh://git@gitea.example.com/tidewater/payout-service.git",
            "ssh://git@gitea.example.com:2222/tidewater/payout-service.git",
            "https://gitea.example.com/tidewater/payout-service",
            "https://gitea.example.com/tidewater/payout-service.git",
            "https://gitea.example.com/tidewater/payout-service/",
            "https://mara@gitea.example.com/tidewater/payout-service.git",
            "http://gitea.example.com/tidewater/payout-service",
            "git://gitea.example.com/tidewater/payout-service.git",
            "  https://GITEA.Example.COM/Tidewater/Payout-Service.git  ",
        ] {
            assert_eq!(remote_key(url), expected, "{url}");
        }
    }

    /// The owner is the *last but one* segment, so a GitLab subgroup resolves
    /// to the directory a clone of it actually lands in.
    #[test]
    fn a_nested_group_keeps_the_last_two_segments() {
        assert_eq!(
            remote_key("https://gitlab.example.com/platform/payments/payout-service.git"),
            key("gitlab.example.com", "payments", "payout-service")
        );
    }

    /// Every shape that must miss rather than produce a key. A key assembled
    /// out of any of these would match a directory that is not the repository.
    #[test]
    fn a_url_that_names_no_repository_misses() {
        for url in [
            "",
            "   ",
            "https://gitea.example.com/",
            "https://gitea.example.com/tidewater",
            "https://gitea.example.com/tidewater/",
            "https:///tidewater/payout-service",
            "git@:tidewater/payout-service.git",
            // A Windows path, which the scp-like form would otherwise swallow.
            r"C:\src\payout-service",
            r"C:\src\tidewater\payout-service",
            // Drive-relative, and the one Windows spelling that reaches the
            // scp-like branch with a usable-looking path behind it.
            r"C:src\tidewater\payout-service",
            "C:src/tidewater/payout-service",
            "C:/src/tidewater/payout-service",
            // An absolute local path: no host, and no repository named.
            "/Users/mara/src/payout-service",
            "payout-service",
        ] {
            assert_eq!(remote_key(url), None, "{url:?} must not produce a key");
        }
    }

    /// `.git` is stripped from the repository and from nowhere else -- an
    /// owner or a host that happens to end in those characters keeps them.
    #[test]
    fn only_the_repository_loses_a_git_suffix() {
        assert_eq!(
            remote_key("https://gitea.example.com/dot.git/payout-service.git"),
            key("gitea.example.com", "dot.git", "payout-service")
        );
    }

    /// The clone command is the repo's own URL, verbatim, behind `git clone`.
    /// It is never run (ADR-0016) and never has a destination pasted onto it.
    #[test]
    fn the_clone_command_is_the_repos_url_and_nothing_else() {
        assert_eq!(
            clone_command("https://gitea.example.com/tidewater/payout-service.git"),
            "git clone https://gitea.example.com/tidewater/payout-service.git"
        );
    }

    #[test]
    fn the_origin_url_is_read_out_of_a_git_config() {
        let config = "[core]\n\trepositoryformatversion = 0\n\
                      [remote \"upstream\"]\n\turl = https://gitea.example.com/upstream/other.git\n\
                      [remote \"origin\"]\n\turl = git@gitea.example.com:tidewater/payout-service.git\n\
                      \tfetch = +refs/heads/*:refs/remotes/origin/*\n";
        assert_eq!(
            origin_url(config).as_deref(),
            Some("git@gitea.example.com:tidewater/payout-service.git")
        );
    }

    /// A clone with remotes but no `origin` is a miss, not the first remote it
    /// happens to have: on a fork, `upstream` is a *different* repository.
    #[test]
    fn a_config_with_no_origin_misses() {
        let config =
            "[remote \"upstream\"]\n\turl = https://gitea.example.com/upstream/other.git\n";
        assert_eq!(origin_url(config), None);
        assert_eq!(origin_url("[core]\n\tbare = false\n"), None);
        assert_eq!(origin_url("[remote \"origin\"]\n\turl =\n"), None);
    }

    // -- the spawn half (#501) ---------------------------------------------

    fn expanded(template: &str) -> Vec<String> {
        expand(template, Path::new("/Users/mara/src/payout-service"))
            .unwrap_or_else(|error| panic!("{template:?} must expand: {error}"))
    }

    /// The placeholder is filled in once, and the word around it survives.
    #[test]
    fn the_path_is_substituted_once_inside_its_own_word() {
        assert_eq!(
            expanded("code {path}"),
            ["code", "/Users/mara/src/payout-service"]
        );
        assert_eq!(
            expanded("code --folder-uri=file://{path}"),
            ["code", "--folder-uri=file:///Users/mara/src/payout-service"]
        );
    }

    /// Quotes group words and do not survive into the argv, which is what lets
    /// the macOS defaults name an application with a space in it.
    #[test]
    fn quotes_group_a_word_and_are_not_passed_on() {
        assert_eq!(
            expanded("open -a \"Visual Studio Code\" {path}"),
            [
                "open",
                "-a",
                "Visual Studio Code",
                "/Users/mara/src/payout-service"
            ]
        );
        assert_eq!(
            expanded("open -a 'IntelliJ IDEA' {path}"),
            [
                "open",
                "-a",
                "IntelliJ IDEA",
                "/Users/mara/src/payout-service"
            ]
        );
        // An empty quoted word is still a word: a command line can carry an
        // empty argument, and dropping it would shift every argument after it.
        assert_eq!(
            expanded("thing \"\" {path}"),
            ["thing", "", "/Users/mara/src/payout-service"]
        );
    }

    /// A path with a space in it is **one** argument, because no shell is
    /// involved -- the failure this whole design avoids.
    #[test]
    fn a_path_with_a_space_stays_one_argument() {
        assert_eq!(
            expand(
                "open -a Terminal {path}",
                Path::new("/Users/mara/My Code/payout service")
            )
            .expect("expands"),
            [
                "open",
                "-a",
                "Terminal",
                "/Users/mara/My Code/payout service"
            ]
        );
    }

    /// **ADR-0016 in a test.** A template that reaches for a field of the
    /// mirrored repo is refused, and the refusal says which one -- so the
    /// person who typed `{repo_url}` reads why it is not there rather than
    /// finding an editor opened at the string `{repo_url}`.
    #[test]
    fn a_placeholder_that_is_not_the_path_is_refused_by_name() {
        for name in ["repo_url", "web_url", "branch", "title", "PATH", ""] {
            let error = expand(&format!("code {{{name}}} {{path}}"), Path::new("/src/x"))
                .expect_err("only {path} may be substituted");
            assert_eq!(error, TemplateError::UnknownPlaceholder(name.to_owned()));
            assert!(
                error.to_string().contains(&format!("{{{name}}}")),
                "the refusal must name the placeholder: {error}"
            );
        }
    }

    /// Every other way a template is unusable, each with its own answer.
    #[test]
    fn an_unusable_template_says_what_is_wrong_with_it() {
        let path = Path::new("/src/x");
        assert_eq!(expand("", path), Err(TemplateError::Empty));
        assert_eq!(expand("   \t ", path), Err(TemplateError::Empty));
        assert_eq!(
            expand("code {path", path),
            Err(TemplateError::UnclosedPlaceholder)
        );
        assert_eq!(
            expand("open -a \"Visual {path}", path),
            Err(TemplateError::UnclosedQuote)
        );
        assert_eq!(expand("code .", path), Err(TemplateError::NoPath));
        assert_eq!(
            expand("cp {path} {path}", path),
            Err(TemplateError::RepeatedPath)
        );
    }

    /// The three macOS defaults expand to the documented commands.
    ///
    /// Spelled out rather than derived from the constant: a test that built
    /// its expectation out of `MACOS_DEFAULT_TEMPLATES` would pass whatever
    /// that constant said, including a template that opens the wrong
    /// application. These four-word answers are what `open` receives.
    #[test]
    fn the_three_macos_defaults_expand_to_the_documented_commands() {
        let path = Path::new("/Users/mara/src/payout-service");
        let expanded: Vec<Vec<String>> = MACOS_DEFAULT_TEMPLATES
            .iter()
            .map(|template| expand(template, path).expect("a default must expand"))
            .collect();
        assert_eq!(
            expanded,
            [
                vec![
                    "open",
                    "-a",
                    "Visual Studio Code",
                    "/Users/mara/src/payout-service"
                ],
                vec![
                    "open",
                    "-a",
                    "IntelliJ IDEA",
                    "/Users/mara/src/payout-service"
                ],
                vec!["open", "-a", "Terminal", "/Users/mara/src/payout-service"],
            ]
        );
    }

    /// The defaults are macOS's, and the other platforms have none -- which is
    /// what makes a button there read *not configured* rather than run a guess.
    ///
    /// **Both halves run wherever the gate runs**, because the platform is an
    /// argument. A `cfg!` here would leave the half that is not this machine
    /// asserted by nothing, and *not configured* is a state no Mac can reach.
    #[test]
    fn only_macos_starts_with_a_template() {
        for (index, action) in OPEN_ACTIONS.into_iter().enumerate() {
            assert_eq!(
                action.default_template_on("macos"),
                Some(MACOS_DEFAULT_TEMPLATES[index]),
                "{}",
                action.id()
            );
            for os in ["linux", "windows", "freebsd", "MACOS", ""] {
                assert_eq!(
                    action.default_template_on(os),
                    None,
                    "{} on {os}",
                    action.id()
                );
            }
        }
        // And this machine's answer is the one its own name selects, which is
        // the single line the two halves above cannot cover.
        for action in OPEN_ACTIONS {
            assert_eq!(
                action.default_template(),
                action.default_template_on(std::env::consts::OS)
            );
        }
    }

    /// The ids are the settings keys' suffixes and the IPC argument, so they
    /// round-trip; the labels are what the buttons say.
    #[test]
    fn every_action_is_found_by_its_own_id() {
        let ids: Vec<&str> = OPEN_ACTIONS.iter().map(|a| a.id()).collect();
        assert_eq!(ids, ["vscode", "jetbrains", "terminal"]);
        for action in OPEN_ACTIONS {
            assert_eq!(OpenAction::from_id(action.id()), Some(action));
        }
        assert_eq!(OpenAction::from_id("emacs"), None);
        assert_eq!(OpenAction::from_id("VsCode"), None);
        let labels: Vec<&str> = OPEN_ACTIONS.iter().map(|a| a.label()).collect();
        assert_eq!(
            labels,
            ["Open in VS Code", "Open in JetBrains", "Open terminal here"]
        );
    }
}
