//! A stable fingerprint of one record as Gitea's listing returned it.
//!
//! # Why the cursor needs one (issue #764)
//!
//! Gitea stamps `updated_at` to the **second**. The cursor remembers which
//! pull requests sit exactly on its watermark second so an idle run does not
//! re-deliver them (contract battery clause 2), and until this module it
//! remembered them by number alone. A pull request merged in the same second
//! the previous run read it keeps its `updated_at`, so the number matched, the
//! merge was skipped, and no later incremental run reached it either: the
//! mirror said `open` until the pull request was touched again or a backfill
//! ran. The repository record had the same hole through `repo_updated_at`.
//!
//! The run already re-reads the watermark second (Gitea's listing is newest
//! first and the walk stops only *below* the mark), so the overlap the ruling
//! asks for is there; what was wrong is the identity it compared against. A
//! fingerprint of the record is the identity: unchanged fingerprints the same,
//! so an idle poll stays idle; changed fingerprints differently whatever its
//! clock said. It is the move Jira made for the same hole in issue #345
//! (`knobas-source-jira`'s `fingerprint.rs`, whose module docs measure it).
//!
//! **Why not leave it to the sink.** The engine's sink rewrites every item it
//! is handed (`synced_at`, the payload, and the passes that read a written
//! row), and battery clause 2 asks the *adapter* for an empty idle run, so
//! "re-deliver and let the write be a no-op" is not available here.
//!
//! # What is fingerprinted
//!
//! The **raw listing record**, which is the payload the item stores (spec §3a)
//! and costs no request; for a pull request, less the fields Gitea moves
//! without the pull request changing ([`of_pull`] names them and says why). For a pull request that is before its
//! discussion is read: a new comment moves the listing's `comments` count (and
//! `updated_at`), and an edit to an existing comment is not this class of
//! change.
//!
//! # The algorithm is spelled out, and is Jira's
//!
//! FNV-1a over a canonical, length-framed, type-tagged encoding with object
//! keys sorted here rather than by `serde_json`'s map type (a feature flag
//! away from insertion order). The value is persisted inside a cursor, so it
//! has to mean the same thing in the next build; `DefaultHasher` does not
//! promise that, and a crate's hash could change under a version bump. It is
//! a copy of Jira's rather than a shared helper on purpose: the crates both
//! adapters depend on are `knobas-core`, `knobas-http` and the SPI, the first
//! and last are frozen surfaces under §10.8 (a new export needs its own
//! entry), and moving Jira's persisted value there would also touch the Jira
//! adapter's cursor for a Gitea fix. Folding the two copies into one is a
//! follow-up for whoever next needs a third.
//! `a_known_record_keeps_its_fingerprint` pins the output to a literal so a
//! drift between builds fails here first.

use serde_json::Value;

const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

struct Fnv(u64);

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }
}

/// The fingerprint of one raw record, as sixteen hex characters.
///
/// Only ever compared against the fingerprint of the same record one run
/// earlier; a collision costs one skipped re-delivery, which the next change
/// to that record corrects.
pub(crate) fn of(raw: &Value) -> String {
    let mut hasher = Fnv(OFFSET_BASIS);
    feed_value(&mut hasher, raw);
    format!("{:016x}", hasher.0)
}

/// The fingerprint of one pull request, **less what moves without it**.
///
/// Three parts of Gitea's pull request record change on events that are not
/// changes to the pull request, and are left out:
///
/// - **`base.repo` / `head.repo`**, the whole repository record twice, with
///   the repository's `updated_at`, size and counters, which move on any push
///   into the repository. Seen against the live fixture (2026-10-14) with the
///   whole record fingerprinted: after a push onto a fresh branch, the idle
///   run following the incremental one re-delivered all five seeded (closed,
///   untouched) pull requests on the watermark second, the same churn #140
///   measured on the repository entity. Only each one's `full_name` stays:
///   it is what the mirror shows of them (which repository a fork's head is
///   in, and the parent), and it moves only on a rename, a transfer or a
///   deleted fork.
/// - **`base.sha`**, which Gitea fills with the base branch's *current* tip,
///   for merged and closed pull requests as well (Gitea 1.27.3, the live
///   fixture, 2026-10-14: every pull request named `main`'s head). A push to
///   the base branch would re-deliver every pull request on the watermark;
///   the branch item reports that push.
/// - **A user record's `last_login` and follow/star counters**, wherever a
///   user is embedded (`user`, `merged_by`, `assignees`,
///   `requested_reviewers`). An admin token gets them filled in, so an
///   author signing in to the web UI would otherwise move the record.
///
/// Everything else the record carries stays in, so a change to anything the
/// mirror shows of a pull request (state, merged, title, body, labels,
/// assignees, requested reviewers, head sha, mergeable, comment count) moves
/// the fingerprint even inside the watermark second.
pub(crate) fn of_pull(raw: &Value) -> String {
    let mut own = raw.clone();
    for side in ["base", "head"] {
        if let Some(branch) = own.get_mut(side).and_then(Value::as_object_mut) {
            if let Some(repo) = branch.get_mut("repo") {
                let full_name = repo.get("full_name").cloned().unwrap_or(Value::Null);
                *repo = full_name;
            }
            if side == "base" {
                branch.remove("sha");
            }
        }
    }
    strip_user_churn(&mut own);
    of(&own)
}

/// The fields of an embedded user record that move without the pull request.
const USER_CHURN: [&str; 4] = [
    "last_login",
    "followers_count",
    "following_count",
    "starred_repos_count",
];

fn strip_user_churn(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for key in USER_CHURN {
                map.remove(key);
            }
            for nested in map.values_mut() {
                strip_user_churn(nested);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(strip_user_churn),
        _ => {}
    }
}

fn feed_value(hasher: &mut Fnv, value: &Value) {
    match value {
        Value::Null => hasher.write(b"0"),
        Value::Bool(false) => hasher.write(b"1"),
        Value::Bool(true) => hasher.write(b"2"),
        Value::Number(number) => {
            hasher.write(b"3");
            feed_framed(hasher, number.to_string().as_bytes());
        }
        Value::String(text) => {
            hasher.write(b"4");
            feed_framed(hasher, text.as_bytes());
        }
        Value::Array(items) => {
            hasher.write(b"5");
            feed_framed(hasher, &(items.len() as u64).to_le_bytes());
            for item in items {
                feed_value(hasher, item);
            }
        }
        Value::Object(map) => {
            hasher.write(b"6");
            feed_framed(hasher, &(map.len() as u64).to_le_bytes());
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            for key in keys {
                feed_framed(hasher, key.as_bytes());
                feed_value(hasher, &map[key]);
            }
        }
    }
}

/// Length first, then the bytes: the prefix is what stops two different
/// records from encoding to the same stream.
fn feed_framed(hasher: &mut Fnv, bytes: &[u8]) {
    hasher.write(&(bytes.len() as u64).to_le_bytes());
    hasher.write(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The value is stored in every Gitea cursor: if it changed between two
    /// builds, every pull request on a watermark would be re-delivered once
    /// after the upgrade. The literal was computed by an independent
    /// re-implementation of the encoding (Python, 2026-10-14), not by this
    /// function.
    #[test]
    fn a_known_record_keeps_its_fingerprint() {
        let record = json!({ "number": 144, "state": "open", "merged": false });
        assert_eq!(of(&record), "d24648878fd1af9b");
    }

    #[test]
    fn key_order_does_not_reach_the_fingerprint() {
        let one = json!({ "number": 144, "state": "open" });
        let other = json!({ "state": "open", "number": 144 });
        assert_eq!(of(&one), of(&other));
    }

    /// The case #764 is about: same number, same `updated_at`, merged.
    #[test]
    fn a_merge_inside_the_second_changes_the_fingerprint() {
        let open = json!({ "number": 144, "updated_at": "2026-08-22T13:50:00Z",
                           "state": "open", "merged": false });
        let mut merged = open.clone();
        merged["state"] = json!("closed");
        merged["merged"] = json!(true);
        assert_ne!(of(&open), of(&merged));
    }

    /// The pull request's own fields count; the repositories it embeds do not,
    /// beyond their names.
    #[test]
    fn a_pull_request_is_fingerprinted_without_the_repositories_it_embeds() {
        let one = json!({ "number": 144, "state": "open",
                          "base": { "ref": "main", "sha": "09a1",
                                    "repo": { "full_name": "t/p", "updated_at": "a" } },
                          "head": { "sha": "c90d",
                                    "repo": { "full_name": "t/p", "updated_at": "a" } } });
        let mut repo_moved = one.clone();
        repo_moved["base"]["repo"]["updated_at"] = json!("b");
        repo_moved["head"]["repo"]["updated_at"] = json!("b");
        assert_eq!(of_pull(&one), of_pull(&repo_moved));

        let mut base_pushed = one.clone();
        base_pushed["base"]["sha"] = json!("77b2");
        assert_eq!(
            of_pull(&one),
            of_pull(&base_pushed),
            "the base tip is the branch's"
        );

        let mut head_moved = one.clone();
        head_moved["head"]["sha"] = json!("e1f2");
        assert_ne!(of_pull(&one), of_pull(&head_moved));
        let mut retargeted = one.clone();
        retargeted["base"]["ref"] = json!("release");
        assert_ne!(of_pull(&one), of_pull(&retargeted));
        let mut fork_gone = one.clone();
        fork_gone["head"]["repo"] = Value::Null;
        assert_ne!(
            of_pull(&one),
            of_pull(&fork_gone),
            "the head's repository is shown"
        );
    }

    /// A user signing in moves `last_login` in every record that embeds them.
    #[test]
    fn an_embedded_user_signing_in_does_not_move_the_fingerprint() {
        let one = json!({ "number": 144, "labels": [],
                          "user": { "login": "mara", "last_login": "a", "followers_count": 1 },
                          "requested_reviewers": [{ "login": "olu", "last_login": "a" }] });
        let mut signed_in = one.clone();
        signed_in["user"]["last_login"] = json!("b");
        signed_in["user"]["followers_count"] = json!(2);
        signed_in["requested_reviewers"][0]["last_login"] = json!("b");
        assert_eq!(of_pull(&one), of_pull(&signed_in));

        let mut reviewer_added = one.clone();
        reviewer_added["requested_reviewers"] = json!([{ "login": "olu" }, { "login": "jonas" }]);
        assert_ne!(of_pull(&one), of_pull(&reviewer_added));
        let mut labelled = one.clone();
        labelled["labels"] = json!([{ "name": "payments" }]);
        assert_ne!(of_pull(&one), of_pull(&labelled));
    }

    #[test]
    fn a_boundary_cannot_be_forged_by_the_content_around_it() {
        assert_ne!(of(&json!({ "a": "4b" })), of(&json!({ "a4": "b" })));
        assert_ne!(of(&json!(["a", "4b"])), of(&json!(["a4", "b"])));
        assert_ne!(of(&json!([""])), of(&json!([])));
        assert_ne!(of(&json!("1")), of(&json!(1)));
    }
}
