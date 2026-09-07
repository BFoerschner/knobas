//! Checkouts: what knobas knows about the clones on this disk.
//!
//! `CONTEXT.md`, **Checkout**: "a clone of a repo on this machine, found under
//! the **clones root** -- a directory setting knobas scans two levels deep,
//! matching each clone's remote to a repo entity by host and owner/repo -- or
//! set by hand per repo as an override. Knobas-owned data about the local
//! disk, never a field of the mirrored repo, and never written to."
//!
//! Everything here is the *finding* half: normalising a remote URL to a key,
//! and walking the clones root for directories that carry one. The override,
//! the setting and the IPC surface are `knobas_app::checkout`; this module has
//! no database in it and no knowledge of an entity.
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
//! Every function here misses rather than guesses: a URL it cannot parse into
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
/// A Windows path (`C:\src\payout-service`) reads as a scp-like remote to a
/// naive split on `:`. **One** guard is what keeps it out: a scp-like host must
/// be longer than one character, so a drive letter is not a host. The other
/// spellings are refused by rules that are there anyway -- `C:/src/a/b` by the
/// leading slash, `C:\src\a\b` by having no `/` and therefore one path
/// segment, where an owner *and* a repository are wanted. A third guard on
/// "the path must carry a `/`" was written and removed: mutation-checked, it
/// killed nothing that the one-segment miss did not already kill, and armour no
/// test can see go is armour nobody can maintain.
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
    // `C:\src\payout-service` is not a remote. A one-character authority is a
    // drive letter, and a path with no `/` in it cannot be `owner/repo`.
    if authority.chars().count() < 2 || !path.contains('/') || path.starts_with('/') {
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
        let config = "[remote \"upstream\"]\n\turl = https://gitea.example.com/upstream/other.git\n";
        assert_eq!(origin_url(config), None);
        assert_eq!(origin_url("[core]\n\tbare = false\n"), None);
        assert_eq!(origin_url("[remote \"origin\"]\n\turl =\n"), None);
    }
}
