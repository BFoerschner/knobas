//! A stable fingerprint of one issue as `/search` returned it.
//!
//! # Why the cursor needs one (issue #345)
//!
//! [`crate::cursor::JiraCursor`]'s `seen` set exists to answer *"was this
//! exact version of this issue already handed to the sink?"*, and until this
//! module it answered it with the pair `(key, updated)`. That pair does not
//! identify a version at the resolution this adapter can observe.
//!
//! Measured against Jira DC 10.3.24: `GET /rest/api/2/issue/{key}` reports
//! `updated` to the **millisecond** (`22:54:59.036`), and
//! `GET /rest/api/2/search` -- the only one a sync run reads -- reports the
//! same instant as `22:54:59.000`. So two changes to one issue inside one
//! second are, to a run, the same `updated`. If a run recorded the issue
//! between them, the second change matches the pair already in `seen`, is
//! dropped before the sink, and is **lost for ever**: `updated` will not move
//! again on its own, so no later run reaches it either and only a backfill
//! recovers it.
//!
//! That is not a hypothetical race. `knobas_app::sources::write_queue`'s
//! `refresh` syncs the source after every landed write, so *comment through
//! knobas, then reassign in Jira within the same second* loses the
//! reassignment -- and `author` is the assignee on Jira, which the standup
//! digest's mirror half, the inbox's author matching (#82) and every `@me`
//! filter read.
//!
//! A digest of the record closes it exactly: an unchanged issue fingerprints
//! the same, so battery clause 2 still holds and an idle poll is still idle;
//! a changed one fingerprints differently whatever its clock said. It is the
//! same move interfaces §4.2 already records for Confluence, whose `seen`
//! identity is `(content id, version.number)` rather than `(id, timestamp)`
//! "because Confluence's version counter closes the *edited twice in one
//! second* hole the Jira cursor documents". Jira has no version counter; this
//! is the same guarantee computed from data already in hand, with no extra
//! request.
//!
//! # What is digested, and when
//!
//! The **raw `/search` record**, before [`crate::sync::SyncRun::complete`]
//! fills in a truncated `comment` or `worklog` container. Both halves of that
//! matter:
//!
//! * **Raw, not mapped.** Spec §3a keeps `payload` verbatim, so the raw record
//!   *is* what the run delivers. A digest of the mapped [`knobas_source::SyncItem`]
//!   would go blind to every field the current mapping ignores -- which is
//!   exactly the data §3a preserves so a later mapping can use it.
//! * **Before completion, not after.** The skip decision happens before
//!   completion, and it has to: completing first would cost a request per
//!   issue with a truncated container on every run, including the runs that go
//!   on to skip it. Digesting the pre-completion record is consistent because
//!   *both* runs digest the pre-completion record -- the comparison is
//!   like-for-like, and a comment added upstream moves `updated` and the
//!   container's `total` alike.
//!
//! # Stability is the whole contract
//!
//! A field that differs between two reads of an unchanged issue would make
//! every poll re-emit every issue in the window: correctness would survive
//! (upserts are idempotent) and battery clause 2 would not. Two things protect
//! it, and the second is the one that would actually catch a surprise:
//!
//! 1. **The encoding is canonical here, not in `serde_json`.** Object keys are
//!    sorted by this module rather than relying on `Value`'s map type, because
//!    that type is a *feature flag* away from preserving insertion order --
//!    any dependency in the tree may turn `preserve_order` on, and the digest
//!    must not silently start depending on the order Jira happened to serialize
//!    a field in.
//! 2. **The real server is the witness**, ADR-0013.
//!    `tests/live_jira_seeded.rs`'s repeated idle poll is what makes a flapping
//!    field loud: it polls an untouched source many times over and fails on the
//!    first run that emits anything, rather than quietly re-delivering the
//!    corpus for ever.

use serde_json::Value;

/// FNV-1a, 64-bit. Spelled out rather than taken from a crate or from
/// `DefaultHasher`: the value is **persisted inside a cursor**, so it has to
/// mean the same thing in the next build of this binary. `DefaultHasher`
/// explicitly does not promise that across Rust releases, and a hash that
/// silently changed would turn one upgrade into a full re-delivery of every
/// source. A named 12-line algorithm cannot drift.
const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

struct Fnv(u64);

impl Fnv {
    fn feed(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }
}

/// The fingerprint of one raw issue, as sixteen hex characters.
///
/// Not cryptographic and does not need to be: it is only ever compared against
/// other digests **of the same issue key**, one run apart, to answer "did this
/// change?". The failure a collision would cause is one skipped re-delivery of
/// one issue, which the next change to it corrects -- and 64 bits over that
/// population is far past the point where anything else is the weak link.
pub(crate) fn of(raw: &Value) -> String {
    let mut hasher = Fnv(OFFSET_BASIS);
    feed(&mut hasher, raw);
    format!("{:016x}", hasher.0)
}

/// Every scalar is written **length-prefixed and type-tagged**, so no two
/// different records can encode to the same bytes: without the length, the
/// arrays `["a", "bc"]` and `["ab", "c"]` differ only by where a separator
/// falls, and any separator character can also appear inside a Jira summary.
fn feed(hasher: &mut Fnv, value: &Value) {
    match value {
        Value::Null => hasher.feed(b"0"),
        Value::Bool(false) => hasher.feed(b"1"),
        Value::Bool(true) => hasher.feed(b"2"),
        Value::Number(number) => {
            hasher.feed(b"3");
            // `Number`'s own rendering: it round-trips the literal Jira sent,
            // integer or float, without this module having to decide which.
            feed_bytes(hasher, number.to_string().as_bytes());
        }
        Value::String(text) => {
            hasher.feed(b"4");
            feed_bytes(hasher, text.as_bytes());
        }
        Value::Array(items) => {
            hasher.feed(b"5");
            feed_bytes(hasher, &(items.len() as u64).to_le_bytes());
            for item in items {
                feed(hasher, item);
            }
        }
        Value::Object(map) => {
            hasher.feed(b"6");
            feed_bytes(hasher, &(map.len() as u64).to_le_bytes());
            // Sorted here, deliberately: see the module docs, point 1.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            for key in keys {
                feed_bytes(hasher, key.as_bytes());
                feed(hasher, &map[key]);
            }
        }
    }
}

fn feed_bytes(hasher: &mut Fnv, bytes: &[u8]) {
    hasher.feed(&(bytes.len() as u64).to_le_bytes());
    hasher.feed(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The property the cursor rests on: same record in, same sixteen
    /// characters out, however the object was built.
    #[test]
    fn the_same_record_digests_the_same_whatever_order_its_keys_arrived_in() {
        let one = json!({ "key": "PAY-240", "fields": { "summary": "a", "updated": "b" } });
        let other = json!({ "fields": { "updated": "b", "summary": "a" }, "key": "PAY-240" });
        assert_eq!(of(&one), of(&other));
        assert_eq!(of(&one).len(), 16, "{}", of(&one));
    }

    /// The property the fix rests on: a field that changed changes the digest,
    /// **including one nested where the assignee really lives** and including
    /// one whose `updated` did not move with it.
    #[test]
    fn a_changed_field_changes_the_digest_even_with_updated_untouched() {
        let before = json!({
            "key": "PAY-240",
            "fields": {
                "updated": "2026-09-03T22:54:59.000+0000",
                "assignee": { "name": "mara.lindqvist" }
            }
        });
        let mut after = before.clone();
        after["fields"]["assignee"]["name"] = json!("knobas");
        assert_ne!(
            of(&before),
            of(&after),
            "this is the whole of what the second-resolution `updated` could not tell apart"
        );
    }

    /// Length prefixes, not separators: two records that differ only in where
    /// a boundary falls must not collide, and a Jira summary may contain any
    /// character a separator could be spelled with.
    #[test]
    fn a_boundary_cannot_be_forged_by_the_content_around_it() {
        assert_ne!(of(&json!(["a", "bc"])), of(&json!(["ab", "c"])));
        assert_ne!(of(&json!({ "a": "b" })), of(&json!({ "ab": "" })));
        assert_ne!(of(&json!([""])), of(&json!([])));
    }

    /// The shapes a value can take are distinguished by type, not only by
    /// content: `"1"`, `1` and `true` are three different records.
    #[test]
    fn the_type_is_part_of_the_record() {
        let all = [json!("1"), json!(1), json!(true), json!(null), json!([1])];
        for (i, one) in all.iter().enumerate() {
            for other in &all[i + 1..] {
                assert_ne!(of(one), of(other), "{one} and {other} digest the same");
            }
        }
    }

    /// The digest is a **value**, not an address: an empty container and a
    /// missing key are different records, which is what keeps a truncated
    /// container from reading as an absent one.
    #[test]
    fn an_absent_field_and_an_empty_one_are_different_records() {
        assert_ne!(of(&json!({ "fields": {} })), of(&json!({})));
        assert_ne!(
            of(&json!({ "fields": { "labels": [] } })),
            of(&json!({ "fields": {} }))
        );
    }
}
