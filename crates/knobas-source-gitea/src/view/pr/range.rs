//! What a pull request's diff is between (#2273, #2310, spec #2234 step 6):
//! its merge base and head, its commits to pick one or a range from, and the
//! head the viewer last reviewed at, for *since my last review*.
//!
//! Gitea's `.diff` of a pull request is the head's tree against the merge
//! base's, so those are the two ends a reader reads a file whole at. Each is
//! a whole sha the record names; a record without one names no range, and
//! the diff draws as before, without expanding or picking.

use knobas_source::view::{DiffRange, RangeCommit};

use super::super::rows::{self, ListedCommit};
use super::{Pull, Review, login};
use crate::keys;

/// The verdicts a reviewer submits. A `PENDING` review is not submitted yet,
/// and a `REQUEST_REVIEW` is somebody asking, not a review.
const SUBMITTED: [&str; 3] = ["APPROVED", "REQUEST_CHANGES", "COMMENT"];

/// The head's sha at `viewer`'s latest submitted review among `reviews`
/// (Gitea lists them oldest first), `None` where the viewer never submitted
/// one or could not be read. A dismissed review was still given at its
/// commit, so it counts.
pub(super) fn reviewed(reviews: &[Review], viewer: Option<&str>) -> Option<String> {
    let viewer = viewer?;
    reviews
        .iter()
        .rev()
        .filter(|r| login(r.user.as_ref()).as_deref() == Some(viewer))
        .filter(|r| r.state.as_deref().is_some_and(|s| SUBMITTED.contains(&s)))
        .find_map(|r| {
            r.commit_id
                .clone()
                .filter(|sha| super::super::is_full_sha(sha))
        })
}

/// The range `pull`'s diff is, in `owner/repo` of source `source_id`: from
/// its merge base to its head, with `commits` (newest first, as Gitea lists
/// them) and the head `reviewed` at. `None` unless the record names both ends
/// as whole shas.
pub(super) fn of(
    source_id: &str,
    owner: &str,
    repo: &str,
    pull: &Pull,
    commits: &[ListedCommit],
    reviewed: Option<String>,
) -> Option<DiffRange> {
    let base = pull
        .merge_base
        .clone()
        .filter(|s| super::super::is_full_sha(s))?;
    let head = pull
        .head
        .as_ref()
        .and_then(|h| h.sha.clone())
        .filter(|s| super::super::is_full_sha(s))?;
    let commits = commits
        .iter()
        .map(|c| {
            let row = rows::commit_row(source_id, owner, repo, &c.record);
            RangeCommit {
                sha: c.record.sha.clone(),
                title: row.title,
                meta: row.meta,
            }
        })
        .collect();
    Some(DiffRange {
        repository: format!("{source_id}:{}", keys::repo_key(owner, repo)),
        base,
        head,
        commits,
        reviewed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "97cb5a8667f8aac8e6cdaeff83ffec9574f56a4b";
    const B: &str = "1262a5b61048aaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn review(who: &str, state: &str, at: &str) -> Review {
        serde_json::from_value(serde_json::json!({
            "user": { "login": who }, "state": state, "commit_id": at,
        }))
        .unwrap()
    }

    #[test]
    fn the_viewers_latest_submitted_review_names_the_head_it_was_given_at() {
        let reviews = [
            review("knobas", "COMMENT", A),
            review("priya", "APPROVED", B),
            review("knobas", "REQUEST_CHANGES", B),
            // Not given yet, and not a review: neither is the latest.
            review("knobas", "PENDING", A),
            review("knobas", "REQUEST_REVIEW", A),
        ];
        assert_eq!(reviewed(&reviews, Some("knobas")).as_deref(), Some(B));
        assert_eq!(reviewed(&reviews[..1], Some("knobas")).as_deref(), Some(A));
    }

    #[test]
    fn a_viewer_who_never_reviewed_or_is_unknown_has_no_last_review() {
        let reviews = [review("priya", "APPROVED", B)];
        assert_eq!(reviewed(&reviews, Some("knobas")), None);
        assert_eq!(reviewed(&reviews, None), None);
        assert_eq!(
            reviewed(&[review("knobas", "COMMENT", "1262a5b")], Some("knobas")),
            None
        );
    }
}
