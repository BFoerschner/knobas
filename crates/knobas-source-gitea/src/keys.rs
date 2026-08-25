//! The entity-id grammar (interfaces §4.2, spec §5a).
//!
//! Four forms, all namespaced by the **instance** id -- `gitea:` for the
//! default instance, `gitea-eu:` for a second one (ruling P10), never the
//! literal adapter kind:
//!
//! | kind | key | example id |
//! | --- | --- | --- |
//! | repo | `owner/repo` | `gitea:tidewater/payout-service` |
//! | pr | `owner/repo#142` | `gitea:tidewater/payout-service#142` |
//! | commit | `owner/repo@<oid>` | `gitea:tidewater/payout-service@c90d11…` |
//! | branch | `owner/repo@refs/heads/<name>` | `gitea:tidewater/payout-service@refs/heads/feature/PAY-231-sepa-retry` |
//!
//! The `refs/heads/` prefix is load-bearing: a branch may legally be named
//! `deadbeef…` (40 hex characters), and without the prefix that branch and the
//! commit it points at would be the same entity. Ids are permanent -- links,
//! activity rows and contexts are written against them (spec §5a) -- so the
//! forms have to be unambiguous by construction, which is what [`parse_key`]
//! exists to prove.
//!
//! Owner and repository names cannot contain `/`, `#` or `@` in Gitea, so the
//! first of those characters after the repository name is always the separator.

/// What separates a branch key from a commit key.
pub const BRANCH_PREFIX: &str = "refs/heads/";

/// `owner/repo`.
#[must_use]
pub fn repo_key(owner: &str, repo: &str) -> String {
    format!("{owner}/{repo}")
}

/// `owner/repo#142`.
#[must_use]
pub fn pr_key(owner: &str, repo: &str, number: u64) -> String {
    format!("{owner}/{repo}#{number}")
}

/// `owner/repo@<oid>`.
///
/// The object id verbatim, as the server reports it: 40 hex characters for a
/// SHA-1 repository and 64 for a SHA-256 one (Gitea's `object_format_name`).
/// Interfaces §4.2 writes this form as `@<sha40>`; the length is the source's
/// to decide, and truncating would make two commits share an id.
#[must_use]
pub fn commit_key(owner: &str, repo: &str, oid: &str) -> String {
    format!("{owner}/{repo}@{oid}")
}

/// `owner/repo@refs/heads/<name>`.
#[must_use]
pub fn branch_key(owner: &str, repo: &str, branch: &str) -> String {
    format!("{owner}/{repo}@{BRANCH_PREFIX}{branch}")
}

/// One parsed key, the inverse of the four constructors above.
///
/// Nothing in M1 needs to go backwards -- but the forms are only unambiguous
/// if something can actually tell them apart, and this is what proves it (and
/// what M2's write-back will resolve an entity id to an API path with).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GiteaKey {
    Repo {
        owner: String,
        repo: String,
    },
    Pr {
        owner: String,
        repo: String,
        number: u64,
    },
    Branch {
        owner: String,
        repo: String,
        branch: String,
    },
    Commit {
        owner: String,
        repo: String,
        sha: String,
    },
}

/// Parse the key half of an entity id (everything after the instance id).
#[must_use]
pub fn parse_key(key: &str) -> Option<GiteaKey> {
    let (owner, rest) = key.split_once('/')?;
    if owner.is_empty() || rest.is_empty() {
        return None;
    }
    // Neither an owner nor a repository name may contain `#` or `@`, so the
    // first one that appears is always the separator.
    let Some(cut) = rest.find(['#', '@']) else {
        return Some(GiteaKey::Repo {
            owner: owner.to_owned(),
            repo: rest.to_owned(),
        });
    };
    let (repo, tail) = rest.split_at(cut);
    let (marker, value) = tail.split_at(1);
    if repo.is_empty() || value.is_empty() {
        return None;
    }
    let (owner, repo) = (owner.to_owned(), repo.to_owned());
    if marker == "#" {
        return value.parse().ok().map(|number| GiteaKey::Pr {
            owner,
            repo,
            number,
        });
    }
    match value.strip_prefix(BRANCH_PREFIX) {
        Some("") => None,
        Some(branch) => Some(GiteaKey::Branch {
            owner,
            repo,
            branch: branch.to_owned(),
        }),
        None => Some(GiteaKey::Commit {
            owner,
            repo,
            sha: value.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_core::entity::EntityRef;

    #[test]
    fn the_four_forms_round_trip() {
        let o = "tidewater";
        let r = "payout-service";
        for (key, expected) in [
            (
                repo_key(o, r),
                GiteaKey::Repo {
                    owner: o.into(),
                    repo: r.into(),
                },
            ),
            (
                pr_key(o, r, 142),
                GiteaKey::Pr {
                    owner: o.into(),
                    repo: r.into(),
                    number: 142,
                },
            ),
            (
                commit_key(o, r, "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192"),
                GiteaKey::Commit {
                    owner: o.into(),
                    repo: r.into(),
                    sha: "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192".into(),
                },
            ),
            (
                branch_key(o, r, "feature/PAY-231-sepa-retry"),
                GiteaKey::Branch {
                    owner: o.into(),
                    repo: r.into(),
                    branch: "feature/PAY-231-sepa-retry".into(),
                },
            ),
        ] {
            assert_eq!(parse_key(&key), Some(expected), "key {key:?}");
        }
    }

    /// The collision the `refs/heads/` prefix exists to prevent.
    #[test]
    fn a_branch_named_like_a_sha_is_still_a_branch() {
        let sha = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
        let branch = branch_key("tidewater", "payout-service", sha);
        let commit = commit_key("tidewater", "payout-service", sha);
        assert_ne!(branch, commit);
        assert!(matches!(parse_key(&branch), Some(GiteaKey::Branch { .. })));
        assert!(matches!(parse_key(&commit), Some(GiteaKey::Commit { .. })));
    }

    /// Git allows `@` and `/` inside a branch name; the grammar splits on the
    /// *first* `@` after the repository name, so the rest is the name verbatim.
    #[test]
    fn branch_names_may_contain_the_separators() {
        let key = branch_key("tidewater", "payout-service", "release/v1.0@rc");
        assert_eq!(
            parse_key(&key),
            Some(GiteaKey::Branch {
                owner: "tidewater".into(),
                repo: "payout-service".into(),
                branch: "release/v1.0@rc".into(),
            })
        );
    }

    /// Every key this module builds has to survive the round trip the contract
    /// battery and the sync engine both make -- `EntityRef::parse` on the
    /// rendered id must give back the same reference.
    #[test]
    fn keys_survive_entity_ref_parsing_under_any_instance_id() {
        for instance in ["gitea", "gitea-eu"] {
            for key in [
                repo_key("tidewater", "payout-service"),
                pr_key("tidewater", "payout-service", 142),
                commit_key(
                    "tidewater",
                    "payout-service",
                    "c90d11a3f5e2b7c4d9018e6a2b3c4d5e6f708192",
                ),
                branch_key("tidewater", "payout-service", "feature/PAY-231-sepa-retry"),
            ] {
                let reference = EntityRef::new(instance, &key);
                let rendered = reference.to_string();
                assert_eq!(
                    EntityRef::parse(&rendered).as_ref(),
                    Ok(&reference),
                    "id {rendered:?}"
                );
            }
        }
    }

    #[test]
    fn malformed_keys_are_rejected() {
        for bad in [
            "",
            "noslash",
            "/payout-service",
            "tidewater/",
            "tidewater/repo#",
            "tidewater/repo#abc",
            "tidewater/repo@",
            "tidewater/repo@refs/heads/",
        ] {
            assert_eq!(parse_key(bad), None, "{bad:?} should not parse");
        }
    }
}
