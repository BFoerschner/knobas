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

use knobas_source::SourceError;
use knobas_source::view::{Block, ViewSection};

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
    let text = source.client.compare_diff(owner, repo, base, head).await?;
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
        }],
    })
}
