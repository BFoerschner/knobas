//! The diff between any two commits (#2271, spec #2234 step 6b): a
//! repository's section `compare:<base>...<head>`, which the app's diff
//! address reads and Compare reads with *Into* as its base.
//!
//! Either end is whatever Gitea's compare resolves: a whole or short sha, a
//! branch, or `<sha>^`, the commit before one, which is how a range picked in
//! *Commits* names its base (Gitea 1.27.3 resolves `^` there, measured
//! 2026-10-30). Git forbids `..` in a ref name, so the first `...` splits the
//! two ends. The read is the compare alone: no repository record, no write
//! offered.
//!
//! The one exception is the repository's first commit, which has no commit
//! before it: Gitea answers `<root>^` with 404 (measured on 1.27.3), and has
//! no compare from the empty tree. A range of that one commit is its own
//! diff; a longer range that starts at it is refused, saying why.
//!
//! Beside the diff, the compare's commits are read (#2310) for the two whole
//! shas the diff is between ([`between`]), which the diff block names as its
//! range so a reader can read either side's file whole at a fixed commit. A
//! failed or unclear read of them names no range and fails nothing.

use knobas_source::SourceError;
use std::collections::HashSet;

use knobas_source::view::{Block, DiffRange, ViewSection};

use crate::source::GiteaSource;

/// The section's prefix.
const PREFIX: &str = "compare:";

/// Whether `section` is a compare (well formed or not: a malformed one is
/// this module's to refuse).
pub(super) fn owns(section: &str) -> bool {
    section.starts_with(PREFIX)
}

/// The two ends `section` names, `None` unless both are there.
pub(super) fn ends(section: &str) -> Option<(&str, &str)> {
    let (base, head) = section.strip_prefix(PREFIX)?.split_once("...")?;
    (!base.is_empty() && !head.is_empty()).then_some((base, head))
}

/// Whether both ends stand still: whole shas, the base possibly with one `^`.
pub(super) fn fixed(section: &str) -> bool {
    ends(section).is_some_and(|(base, head)| {
        super::full_sha(base.strip_suffix('^').unwrap_or(base)) && super::full_sha(head)
    })
}

/// The diff `section` names, as `git diff` prints it.
pub(super) async fn read(
    source: &GiteaSource,
    entity: &str,
    owner: &str,
    repo: &str,
    section: &str,
) -> Result<ViewSection, SourceError> {
    let Some((base, head)) = ends(section) else {
        return Err(SourceError::protocol(format!(
            "gitea: {section:?} names no compare `compare:<base>...<head>` (asked for {entity:?})"
        )));
    };
    let (text, listing) = tokio::join!(
        source.client.compare_diff(owner, repo, base, head),
        source
            .client
            .compare_commits::<Listing>(owner, repo, base, head),
    );
    let range = listing
        .ok()
        .and_then(|listing| between(&listing, base, head))
        .map(|(base, head)| DiffRange {
            repository: entity.to_owned(),
            base,
            head,
            commits: Vec::new(),
            reviewed: None,
        });
    let text = match text {
        Ok(text) => text,
        Err(error) if error.status() == Some(404) => {
            first_commit(source, owner, repo, base, head, error).await?
        }
        Err(error) => return Err(error),
    };
    let (files, truncated) = knobas_source::diff::parse_view(&text);
    Ok(ViewSection {
        id: section.to_owned(),
        title: "Diff".to_owned(),
        count: Some(files.len() as u64),
        deferred: false,
        blocks: vec![Block::Diff {
            files,
            truncated,
            commentable: None,
            gone: Vec::new(),
            range,
        }],
    })
}

/// The diff for a compare Gitea did not find (`refused`, a 404): when the
/// base is `<oldest>^` and `oldest` is the repository's first commit, the
/// range of that commit alone is its own diff, and a longer one is refused
/// with the reason. Anything else is `refused` as it came.
async fn first_commit(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    base: &str,
    head: &str,
    refused: SourceError,
) -> Result<String, SourceError> {
    let Some(oldest) = base.strip_suffix('^') else {
        return Err(refused);
    };
    match source.client.commit(owner, repo, oldest).await {
        Ok(commit) if commit.parents.is_empty() => {}
        _ => return Err(refused),
    }
    if oldest == head {
        return source.client.commit_diff(owner, repo, oldest).await;
    }
    Err(SourceError::Protocol {
        status: Some(404),
        message: format!(
            "gitea: {oldest} is the repository's first commit, so no commit comes \
             before it and Gitea cannot read a range that starts there; pick the \
             range from the commit after it, or open {oldest} on its own"
        ),
    })
}

/// A compare's commits as Gitea lists them: each sha with its parents', and
/// how many there are in all.
#[derive(Debug, serde::Deserialize)]
struct Listing {
    #[serde(default)]
    total_commits: Option<usize>,
    #[serde(default)]
    commits: Vec<Listed>,
}

#[derive(Debug, serde::Deserialize)]
struct Listed {
    sha: String,
    #[serde(default)]
    parents: Vec<Parent>,
}

#[derive(Debug, serde::Deserialize)]
struct Parent {
    sha: String,
}

/// The whole shas of the two commits the diff of `base...head` is between,
/// from the compare's `listing`: the merge base of the two ends, and the
/// head. `None` where the listing does not settle them: no commits (nothing
/// to expand), a listing Gitea cut short, or a merge base the listing cannot
/// tell apart.
///
/// The listing is every commit `head` has that `base` lacks; the parents of
/// those outside it are where the range meets `base`'s history. The asked
/// base -- a whole sha, or `<sha>^` for a listed commit's first parent -- is
/// the merge base when it is one of them (it is then an ancestor of `head`);
/// otherwise a range that meets that history at one commit only meets it at
/// its merge base. A branch name settles only that second way.
fn between(listing: &Listing, base: &str, head: &str) -> Option<(String, String)> {
    let commits = &listing.commits;
    if commits.is_empty() || listing.total_commits.is_some_and(|t| t != commits.len()) {
        return None;
    }
    let inside: HashSet<&str> = commits.iter().map(|c| c.sha.as_str()).collect();
    let parents: HashSet<&str> = commits
        .iter()
        .flat_map(|c| c.parents.iter().map(|p| p.sha.as_str()))
        .collect();
    let mut tips = commits.iter().filter(|c| !parents.contains(c.sha.as_str()));
    let tip = tips.next()?;
    if tips.next().is_some() || (super::full_sha(head) && !tip.sha.eq_ignore_ascii_case(head)) {
        return None;
    }
    let mut outside: Vec<&str> = Vec::new();
    for c in commits {
        for p in &c.parents {
            if !inside.contains(p.sha.as_str()) && !outside.contains(&p.sha.as_str()) {
                outside.push(&p.sha);
            }
        }
    }
    let asked = if super::full_sha(base) {
        Some(base)
    } else {
        base.strip_suffix('^').and_then(|child| {
            commits
                .iter()
                .find(|c| c.sha.eq_ignore_ascii_case(child))
                .and_then(|c| c.parents.first())
                .map(|p| p.sha.as_str())
        })
    };
    let merge_base = match asked {
        Some(asked) if outside.iter().any(|o| o.eq_ignore_ascii_case(asked)) => asked,
        _ if outside.len() == 1 => outside[0],
        _ => return None,
    };
    super::full_sha(merge_base).then(|| (merge_base.to_owned(), tip.sha.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(c: char) -> String {
        std::iter::repeat_n(c, 40).collect()
    }

    /// A listing of `(commit, parents)`, newest first.
    fn listing(commits: &[(char, &[char])]) -> Listing {
        Listing {
            total_commits: Some(commits.len()),
            commits: commits
                .iter()
                .map(|&(c, parents)| Listed {
                    sha: sha(c),
                    parents: parents.iter().map(|&p| Parent { sha: sha(p) }).collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_range_of_commits_is_between_the_parent_of_its_oldest_and_its_newest() {
        // c <- b <- a, picked as `a^...c`, read as the commit before `a`.
        let l = listing(&[('c', &['b']), ('b', &['a']), ('a', &['0'])]);
        let base = format!("{}^", sha('a'));
        assert_eq!(between(&l, &base, &sha('c')), Some((sha('0'), sha('c'))));
        assert_eq!(
            between(&l, &sha('0'), &sha('c')),
            Some((sha('0'), sha('c')))
        );
    }

    #[test]
    fn a_merge_commit_is_between_its_first_parent_and_itself() {
        // m merges side branch s (forked at 0) into 1; `m^...m` lists m and s.
        let l = listing(&[('m', &['1', 's']), ('s', &['0'])]);
        let base = format!("{}^", sha('m'));
        assert_eq!(between(&l, &base, &sha('m')), Some((sha('1'), sha('m'))));
        // A base it names that is no ancestor and two places the range
        // meets the base's history: no range rather than a wrong one.
        assert_eq!(between(&l, &sha('9'), &sha('m')), None);
    }

    #[test]
    fn branch_names_settle_where_the_range_meets_the_base_once() {
        let l = listing(&[('c', &['b']), ('b', &['0'])]);
        assert_eq!(between(&l, "main", "feature"), Some((sha('0'), sha('c'))));
    }

    #[test]
    fn an_empty_cut_or_mismatched_listing_names_no_range() {
        assert_eq!(between(&listing(&[]), "main", "feature"), None);
        let mut cut = listing(&[('c', &['b']), ('b', &['0'])]);
        cut.total_commits = Some(3);
        assert_eq!(between(&cut, "main", "feature"), None);
        let l = listing(&[('c', &['b']), ('b', &['0'])]);
        assert_eq!(between(&l, &sha('0'), &sha('d')), None);
    }
}
