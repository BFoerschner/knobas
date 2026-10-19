//! A stable fingerprint of one raw record, for an incremental sync's `seen`
//! set (`CONTEXT.md` **Incremental sync**, **Cursor**).
//!
//! **Not a digest.** In `CONTEXT.md` a *digest* is the standup's three lists;
//! this is a fingerprint of a record, and the two words must not blur.
//!
//! # Why a cursor needs one
//!
//! A cursor's `seen` set answers *"was this exact version of this record
//! already handed to the sink?"*. A source's clock does not answer it: Jira's
//! `/search` reports `updated` to the second (#345), and so does Gitea's
//! `updated_at` (#764). Two changes inside one second share a timestamp, and
//! a run that recorded the record between them skips the second change for
//! ever: the timestamp will not move again on its own. A fingerprint of the
//! record closes that hole. An unchanged record fingerprints the same, so an
//! idle poll stays idle (battery clause 2). A changed one fingerprints
//! differently whatever its clock said. Confluence has a version counter and
//! needs none.
//!
//! The adapter fingerprints the **raw** record, the payload it stores (spec
//! §3a), not the mapped item, so a field the mapping ignores still counts. An
//! adapter whose record carries fields that move without the record changing
//! strips them before calling [`of`] (Gitea's pull requests do).
//!
//! # Stability is the whole contract
//!
//! The value is **persisted inside a cursor**, so it has to mean the same
//! thing in the next build, on every target this crate builds for (wasm32
//! included, so an external adapter can use it too). A value that changed would
//! turn one upgrade into a full re-delivery of every source. Hence:
//!
//! 1. **FNV-1a, 64-bit, spelled out** rather than `DefaultHasher` (which does
//!    not promise stability across Rust releases) or a crate's hash (which a
//!    version bump could change). A named twelve-line algorithm cannot drift.
//! 2. **The encoding is canonical here, not in `serde_json`.** Object keys are
//!    sorted by this module, because `Value`'s map type is a feature flag
//!    (`preserve_order`) away from keeping insertion order, and any crate in
//!    the tree may turn it on.
//! 3. **Every value is type-tagged and length-framed**, so no two different
//!    records encode to the same bytes.
//! 4. **The output is pinned** to literals computed before the two adapter
//!    copies were folded into this module (#1314), so a drift fails here
//!    first. The real servers are the other witness (ADR-0013): the Jira and
//!    Gitea live suites poll an untouched source and fail on an idle run that
//!    re-emits a record.

use serde_json::Value;

const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a, 64-bit.
struct Fnv(u64);

impl Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }
}

/// The fingerprint of one raw record, as sixteen lowercase hex characters.
///
/// Not cryptographic and does not need to be: it is only ever compared against
/// the fingerprint of the same record one run earlier, to answer "did this
/// change?". A collision costs one skipped re-delivery of one record, which
/// the next change to it corrects.
#[must_use]
pub fn of(raw: &Value) -> String {
    let mut hasher = Fnv(OFFSET_BASIS);
    feed_value(&mut hasher, raw);
    format!("{:016x}", hasher.0)
}

/// Every scalar is written **length-prefixed and type-tagged**, so no two
/// different records can encode to the same bytes: without the length, the
/// arrays `["a", "4b"]` and `["a4", "b"]` differ only by where a boundary
/// falls, and any separator character can also appear inside a record.
fn feed_value(hasher: &mut Fnv, value: &Value) {
    match value {
        Value::Null => hasher.write(b"0"),
        Value::Bool(false) => hasher.write(b"1"),
        Value::Bool(true) => hasher.write(b"2"),
        Value::Number(number) => {
            hasher.write(b"3");
            // `Number`'s own rendering: it round-trips the literal the source
            // sent, integer or float, without this module deciding which.
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
            // Sorted here, deliberately: see the module docs, point 2.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            for key in keys {
                feed_framed(hasher, key.as_bytes());
                feed_value(hasher, &map[key]);
            }
        }
    }
}

/// **Framed**, never bare: the length goes in before the bytes. That prefix is
/// the whole reason two different records cannot encode to the same stream.
fn feed_framed(hasher: &mut Fnv, bytes: &[u8]) {
    hasher.write(&(bytes.len() as u64).to_le_bytes());
    hasher.write(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every record the Jira and Gitea fingerprint tests used, with the value
    /// each adapter's own copy of this module gave it before #1314 folded the
    /// copies into one. The values come from running those copies, not this
    /// function. A value that moves here re-delivers every record on a
    /// watermark once after the upgrade, which is the backfill #1314 promised
    /// not to trigger.
    ///
    /// Jira's golden records and Gitea's wiremock records are pinned the same
    /// way in their own crates (`sync::tests::golden_records_keep_their_fingerprints`,
    /// `it::sync::fixture_records_keep_their_fingerprints`).
    #[test]
    fn every_fixture_record_keeps_the_fingerprint_it_had() {
        let mut forwards = serde_json::Map::new();
        for key in ["assignee", "labels", "summary", "updated"] {
            forwards.insert(key.to_owned(), json!(key));
        }
        let pinned = [
            // Jira's copy.
            (
                json!({ "key": "PAY-240", "fields": { "summary": "a", "updated": "b" } }),
                "9fd8bc41fbbf435a",
            ),
            (
                json!({ "key": "PAY-240", "fields": {
                    "updated": "2026-09-03T22:54:59.000+0000",
                    "assignee": { "name": "mara.lindqvist" } } }),
                "a74a72bab3a51f72",
            ),
            (
                json!({ "key": "PAY-240", "fields": {
                    "updated": "2026-09-03T22:54:59.000+0000",
                    "assignee": { "name": "knobas" } } }),
                "4366095382bb77b7",
            ),
            (json!({ "a": "4b" }), "1c1298fd91129914"),
            (json!({ "a4": "b" }), "e983e2f299b5d84e"),
            (json!(["a", "4b"]), "208e9c24b9a3d7ce"),
            (json!(["a4", "b"]), "a5d2c32b70362d48"),
            (json!(["a", "bc"]), "21b29d24ba9bb5bb"),
            (json!(["ab", "c"]), "35fd67cd3a8e175d"),
            (json!([""]), "fe9f0d17beef0a17"),
            (json!([]), "4e4267df7ce3e238"),
            (Value::Object(forwards), "1f8c00149ac4818d"),
            (json!("1"), "e9820f284a193249"),
            (json!(1), "8b909cf503e03336"),
            (json!(true), "af63af4c8601a015"),
            (json!(null), "af63ad4c86019caf"),
            (json!([1]), "e0c817ac7ad8610a"),
            (json!({ "fields": {} }), "383209b8c33e9587"),
            (json!({}), "4ae6d747921b0181"),
            (json!({ "fields": { "labels": [] } }), "d3201fbd54a2312e"),
            // Gitea's copy. The first is the literal its test carried since
            // 2026-10-14, computed then by an independent re-implementation.
            (
                json!({ "number": 144, "state": "open", "merged": false }),
                "d24648878fd1af9b",
            ),
            (
                json!({ "number": 144, "state": "open" }),
                "7a22a159c395a55f",
            ),
            (
                json!({ "number": 144, "updated_at": "2026-08-22T13:50:00Z",
                        "state": "open", "merged": false }),
                "52bb39c21485edcc",
            ),
            (
                json!({ "number": 144, "updated_at": "2026-08-22T13:50:00Z",
                        "state": "closed", "merged": true }),
                "462bdb3e826aa651",
            ),
        ];
        for (record, was) in &pinned {
            assert_eq!(of(record), *was, "{record}");
        }
    }

    /// Same record in, same sixteen characters out, however the object was
    /// built.
    #[test]
    fn the_same_record_fingerprints_the_same_whatever_order_its_keys_arrived_in() {
        let one = json!({ "key": "PAY-240", "fields": { "summary": "a", "updated": "b" } });
        let other = json!({ "fields": { "updated": "b", "summary": "a" }, "key": "PAY-240" });
        assert_eq!(of(&one), of(&other));
        assert_eq!(of(&one).len(), 16, "{}", of(&one));
    }

    /// A changed field changes the fingerprint, including one nested where
    /// Jira's assignee lives and one whose `updated` did not move with it.
    #[test]
    fn a_changed_field_changes_the_fingerprint_even_with_updated_untouched() {
        let before = json!({ "key": "PAY-240", "fields": {
            "updated": "2026-09-03T22:54:59.000+0000",
            "assignee": { "name": "mara.lindqvist" } } });
        let mut after = before.clone();
        after["fields"]["assignee"]["name"] = json!("knobas");
        assert_ne!(of(&before), of(&after));
    }

    /// **Length prefixes, not separators.** The pairs are chosen so the type
    /// tag alone does not save them: `{"a": "4b"}` and `{"a4": "b"}` feed the
    /// same bytes once the lengths are removed, because the `4` that tags a
    /// string is absorbed into the neighbouring key or value.
    #[test]
    fn a_boundary_cannot_be_forged_by_the_content_around_it() {
        assert_ne!(of(&json!({ "a": "4b" })), of(&json!({ "a4": "b" })));
        assert_ne!(of(&json!(["a", "4b"])), of(&json!(["a4", "b"])));
        assert_ne!(of(&json!(["a", "bc"])), of(&json!(["ab", "c"])));
        assert_ne!(of(&json!([""])), of(&json!([])));
    }

    /// **The sort is a guard against a feature flag, and is vacuous until that
    /// flag flips.** `serde_json::Value`'s map is a `BTreeMap` unless some
    /// crate turns `preserve_order` on, and a `BTreeMap` hands its keys over
    /// sorted already, so deleting the sort changes nothing in this build.
    /// This asserts the invariant rather than the mechanism: it becomes able
    /// to fail the day the flag flips.
    #[test]
    fn key_order_cannot_reach_the_fingerprint_however_the_map_is_built() {
        let mut forwards = serde_json::Map::new();
        for key in ["assignee", "labels", "summary", "updated"] {
            forwards.insert(key.to_owned(), json!(key));
        }
        let mut backwards = serde_json::Map::new();
        for key in ["updated", "summary", "labels", "assignee"] {
            backwards.insert(key.to_owned(), json!(key));
        }
        assert_eq!(of(&Value::Object(forwards)), of(&Value::Object(backwards)));
    }

    /// `"1"`, `1` and `true` are three different records.
    #[test]
    fn the_type_is_part_of_the_record() {
        let all = [json!("1"), json!(1), json!(true), json!(null), json!([1])];
        for (i, one) in all.iter().enumerate() {
            for other in &all[i + 1..] {
                assert_ne!(of(one), of(other), "{one} and {other} fingerprint the same");
            }
        }
    }

    /// An empty container and a missing key are different records, which keeps
    /// a truncated container from reading as an absent one.
    #[test]
    fn an_absent_field_and_an_empty_one_are_different_records() {
        assert_ne!(of(&json!({ "fields": {} })), of(&json!({})));
        assert_ne!(
            of(&json!({ "fields": { "labels": [] } })),
            of(&json!({ "fields": {} }))
        );
    }
}
