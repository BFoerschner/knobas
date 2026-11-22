//! Where a password source's one-time-code keys rest (ADR-0056): in the OS
//! credential store beside the source's credential, as chunked text items,
//! and nowhere else.
//!
//! **Why chunks.** Windows' Credential Manager refuses an item over 2,560
//! bytes of UTF-16, 1,280 characters, which is about fifteen keys. One item
//! per entry is no better: Linux's kernel keyring allows a user 200 keys in
//! all, and a macOS debug build prompts once for every item. So a source's
//! keys are one JSON map, from the entry's entity id to
//! `secret|digits|period|algorithm`, cut into chunks of at most
//! [`CHUNK_CHARS`] UTF-16 units, each under `otp:<source>:<n>`.
//!
//! **A write that stops half way leaves no mixed set.** Each chunk starts
//! with a header naming the write it belongs to and how many chunks that
//! write made: `k1 <write> <n>/<count>\n`. A reader takes chunk 0's count and
//! write, and fails cleanly when any later chunk is missing or belongs to
//! another write, rather than reading half of one set and half of another.
//! A write stores chunks `0..count`, then deletes the tail the old count
//! named.

use std::collections::BTreeMap;
use std::sync::Arc;

use knobas_source::passwords::{OtpAlgorithm, OtpKey, SecretText};

use crate::{KeychainAccount, SecretError, SecretStore};

/// The most UTF-16 units one chunk holds, header included: under Windows'
/// 1,280, with room to spare.
pub const CHUNK_CHARS: usize = 1_200;

/// The header's tag: the format of this module's chunks.
const TAG: &str = "k1";

/// A source's keys, by entry entity id.
pub type OtpKeys = BTreeMap<String, OtpKey>;

/// One key as the map spells it: the bare secret for the common case
/// (6 digits, 30 seconds, SHA-1), else `secret|digits|period|algorithm`.
fn spell(key: &OtpKey) -> String {
    if key.digits == 6 && key.period == 30 && key.algorithm == OtpAlgorithm::Sha1 {
        key.secret.expose().to_owned()
    } else {
        format!(
            "{}|{}|{}|{}",
            key.secret.expose(),
            key.digits,
            key.period,
            key.algorithm.as_str()
        )
    }
}

fn unspell(text: &str) -> Option<OtpKey> {
    let mut parts = text.split('|');
    let secret = parts.next()?.to_owned();
    if secret.is_empty() {
        return None;
    }
    let (digits, period, algorithm) = match (parts.next(), parts.next(), parts.next()) {
        (None, None, None) => (6, 30, OtpAlgorithm::Sha1),
        (Some(d), Some(p), Some(a)) => (d.parse().ok()?, p.parse().ok()?, OtpAlgorithm::parse(a)?),
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(OtpKey {
        secret: SecretText::new(secret),
        digits,
        period,
        algorithm,
    })
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Cut `body` into pieces whose header plus text stays within
/// [`CHUNK_CHARS`]. The header's width is fixed at the widest it can be for
/// this write, so the pieces are cut once.
fn cut(body: &str, write: &str) -> Vec<String> {
    // A header for up to 99,999 chunks: far past any keychain's quota.
    let widest = units(&format!("{TAG} {write} 99999/99999\n"));
    let room = CHUNK_CHARS - widest;
    let mut pieces = Vec::new();
    let mut piece = String::new();
    let mut used = 0;
    for ch in body.chars() {
        let w = ch.len_utf16();
        if used + w > room {
            pieces.push(std::mem::take(&mut piece));
            used = 0;
        }
        piece.push(ch);
        used += w;
    }
    if !piece.is_empty() || pieces.is_empty() {
        pieces.push(piece);
    }
    pieces
}

/// The chunks one write stores, header and all, for `keys`.
#[must_use]
pub fn chunks(keys: &OtpKeys, write: &str) -> Vec<String> {
    let map: BTreeMap<&str, String> = keys
        .iter()
        .map(|(id, key)| (id.as_str(), spell(key)))
        .collect();
    let body = serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_owned());
    let pieces = cut(&body, write);
    let count = pieces.len();
    pieces
        .into_iter()
        .enumerate()
        .map(|(n, piece)| format!("{TAG} {write} {n}/{count}\n{piece}"))
        .collect()
}

/// Read one chunk's header: its write, its number and the count.
fn header(chunk: &str) -> Result<(&str, usize, usize, &str), SecretError> {
    let bad = || {
        SecretError::Backend(
            "a one-time-code key chunk is not in the form knobas writes".to_owned(),
        )
    };
    let (head, body) = chunk.split_once('\n').ok_or_else(bad)?;
    let mut parts = head.split(' ');
    if parts.next() != Some(TAG) {
        return Err(bad());
    }
    let write = parts.next().ok_or_else(bad)?;
    let (n, count) = parts
        .next()
        .and_then(|p| p.split_once('/'))
        .ok_or_else(bad)?;
    let n = n.parse().map_err(|_| bad())?;
    let count = count.parse().map_err(|_| bad())?;
    if parts.next().is_some() || n >= count {
        return Err(bad());
    }
    Ok((write, n, count, body))
}

/// The count chunk 0 names, or `None` when there is no chunk 0.
fn stored_count(store: &dyn SecretStore, source_id: &str) -> Result<Option<usize>, SecretError> {
    match store.get_text(&KeychainAccount::otp(source_id, 0))? {
        None => Ok(None),
        Some(first) => Ok(Some(header(&first)?.2)),
    }
}

/// Every key stored for `source_id`; empty when none is.
///
/// # Errors
/// [`SecretError`] from the store, or [`SecretError::Backend`] when a chunk
/// is missing, malformed or belongs to another write: the set is then
/// unreadable as a whole and no key of it is answered.
pub fn read(store: &dyn SecretStore, source_id: &str) -> Result<OtpKeys, SecretError> {
    let Some(first) = store.get_text(&KeychainAccount::otp(source_id, 0))? else {
        return Ok(OtpKeys::new());
    };
    let (write, _, count, body) = header(&first)?;
    let write = write.to_owned();
    let mut joined = body.to_owned();
    for n in 1..count {
        let chunk = store
            .get_text(&KeychainAccount::otp(source_id, n))?
            .ok_or_else(|| {
                SecretError::Backend(format!(
                    "one-time-code key chunk {n} of {count} is missing, so none of the keys is read"
                ))
            })?;
        let (theirs, at, of, body) = header(&chunk)?;
        if theirs != write || at != n || of != count {
            return Err(SecretError::Backend(format!(
                "one-time-code key chunk {n} belongs to another write, so none of the keys is read"
            )));
        }
        joined.push_str(body);
    }
    let map: BTreeMap<String, String> = serde_json::from_str(&joined)
        .map_err(|_| SecretError::Backend("the one-time-code keys do not decode".to_owned()))?;
    Ok(map
        .into_iter()
        .filter_map(|(id, text)| unspell(&text).map(|key| (id, key)))
        .collect())
}

/// A write id: unique enough that two writes of one source never share it.
fn write_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!(
        "{:x}{:x}",
        nanos & 0xffff_ffff_ffff,
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Store exactly `keys` for `source_id`, replacing what was there. No keys
/// deletes every chunk. Answers how many chunks it wrote.
///
/// # Errors
/// [`SecretError`] from the store. A failure part way leaves a set that
/// [`read`] refuses as a whole, never a mixed one.
pub fn replace(
    store: &dyn SecretStore,
    source_id: &str,
    keys: &OtpKeys,
) -> Result<usize, SecretError> {
    let old = stored_count(store, source_id)
        .unwrap_or(Some(usize::MAX))
        .unwrap_or(0);
    let written = if keys.is_empty() {
        0
    } else {
        let chunks = chunks(keys, &write_id());
        for (n, chunk) in chunks.iter().enumerate() {
            store.put_text(&KeychainAccount::otp(source_id, n), chunk)?;
        }
        chunks.len()
    };
    // The tail the old count named; a chunk 0 that did not read is cleared as
    // far as anything is found.
    if old == usize::MAX {
        let mut n = written;
        while store
            .get_text(&KeychainAccount::otp(source_id, n))?
            .is_some()
        {
            store.delete(&KeychainAccount::otp(source_id, n))?;
            n += 1;
        }
    } else {
        for n in written..old {
            store.delete(&KeychainAccount::otp(source_id, n))?;
        }
    }
    Ok(written)
}

/// Add `keys` to what is stored for `source_id`, overwriting a key of the
/// same entry and removing none. A stored set that does not read is
/// replaced by `keys` alone.
///
/// # Errors
/// As [`replace`].
pub fn merge(
    store: &dyn SecretStore,
    source_id: &str,
    keys: OtpKeys,
) -> Result<usize, SecretError> {
    let mut all = read(store, source_id).unwrap_or_default();
    all.extend(keys);
    replace(store, source_id, &all)
}

/// Delete every chunk of `source_id`'s keys: when the source is removed.
///
/// # Errors
/// As [`replace`].
pub fn delete(store: &dyn SecretStore, source_id: &str) -> Result<(), SecretError> {
    replace(store, source_id, &OtpKeys::new()).map(drop)
}

/// [`read`], [`replace`], [`merge`] and [`delete`] off the async runtime: a
/// keychain call blocks, on a prompt at worst (`crate::spawn`).
pub mod spawn {
    use super::{Arc, OtpKeys, SecretError, SecretStore};

    async fn blocking<T: Send + 'static>(
        f: impl FnOnce() -> Result<T, SecretError> + Send + 'static,
    ) -> Result<T, SecretError> {
        tokio::task::spawn_blocking(f).await.unwrap_or_else(|join| {
            Err(SecretError::Backend(format!(
                "the store's blocking task failed: {join}"
            )))
        })
    }

    /// # Errors
    /// As [`super::read`].
    pub async fn read(
        store: &Arc<dyn SecretStore>,
        source_id: &str,
    ) -> Result<OtpKeys, SecretError> {
        let (store, id) = (Arc::clone(store), source_id.to_owned());
        blocking(move || super::read(store.as_ref(), &id)).await
    }

    /// # Errors
    /// As [`super::replace`].
    pub async fn replace(
        store: &Arc<dyn SecretStore>,
        source_id: &str,
        keys: OtpKeys,
    ) -> Result<usize, SecretError> {
        let (store, id) = (Arc::clone(store), source_id.to_owned());
        blocking(move || super::replace(store.as_ref(), &id, &keys)).await
    }

    /// # Errors
    /// As [`super::merge`].
    pub async fn merge(
        store: &Arc<dyn SecretStore>,
        source_id: &str,
        keys: OtpKeys,
    ) -> Result<usize, SecretError> {
        let (store, id) = (Arc::clone(store), source_id.to_owned());
        blocking(move || super::merge(store.as_ref(), &id, keys)).await
    }

    /// # Errors
    /// As [`super::delete`].
    pub async fn delete(store: &Arc<dyn SecretStore>, source_id: &str) -> Result<(), SecretError> {
        let (store, id) = (Arc::clone(store), source_id.to_owned());
        blocking(move || super::delete(store.as_ref(), &id)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryStore;

    fn keys(n: usize) -> OtpKeys {
        (0..n)
            .map(|i| {
                let key = OtpKey {
                    secret: SecretText::new(format!("JBSWY3DPEHPK3PXP{i:04}AB")),
                    digits: if i % 7 == 0 { 8 } else { 6 },
                    period: if i % 5 == 0 { 60 } else { 30 },
                    algorithm: if i % 3 == 0 {
                        OtpAlgorithm::Sha256
                    } else {
                        OtpAlgorithm::Sha1
                    },
                };
                (format!("pleasant:0b6e2f0e-7c1a-4f3b-9d55-{i:012}"), key)
            })
            .collect()
    }

    fn count_items(store: &MemoryStore, source: &str) -> usize {
        (0..1000)
            .take_while(|n| {
                store
                    .get_text(&KeychainAccount::otp(source, *n))
                    .unwrap()
                    .is_some()
            })
            .count()
    }

    #[test]
    fn every_size_round_trips_and_no_chunk_is_over_the_limit() {
        for n in [0, 1, 15, 16, 400] {
            let store = MemoryStore::new();
            let wrote = replace(&store, "pw", &keys(n)).unwrap();
            assert_eq!(read(&store, "pw").unwrap(), keys(n), "{n} keys");
            assert_eq!(count_items(&store, "pw"), wrote, "{n} keys");
            let cut = chunks(&keys(n), &write_id());
            for (i, chunk) in cut.iter().enumerate() {
                let len = chunk.encode_utf16().count();
                assert!(len <= CHUNK_CHARS, "{len} > {CHUNK_CHARS}");
                assert_eq!(units(chunk), len);
                // Windows' own limit, in its own unit: 2,560 bytes of UTF-16.
                assert!(len * 2 <= 2_560);
                // A chunk is cut full: every one but the last nearly fills.
                if i + 1 < cut.len() {
                    assert!(len > 1_150, "chunk {i} of {} holds only {len}", cut.len());
                }
            }
        }
        // About fifteen keys a chunk at Pleasant's id length: 400 is many.
        assert!(replace(&MemoryStore::new(), "pw", &keys(400)).unwrap() > 20);
    }

    #[test]
    fn a_shrink_deletes_the_tail_and_nothing_clears_everything() {
        let store = MemoryStore::new();
        let many = replace(&store, "pw", &keys(400)).unwrap();
        let few = replace(&store, "pw", &keys(16)).unwrap();
        assert!(few < many);
        assert_eq!(count_items(&store, "pw"), few, "the old tail is left");
        assert!(
            store
                .get_text(&KeychainAccount::otp("pw", few))
                .unwrap()
                .is_none()
        );
        delete(&store, "pw").unwrap();
        assert_eq!(count_items(&store, "pw"), 0);
        assert!(read(&store, "pw").unwrap().is_empty());
    }

    #[test]
    fn a_write_that_stopped_after_chunk_0_fails_cleanly_and_mixes_nothing() {
        let store = MemoryStore::new();
        replace(&store, "pw", &keys(100)).unwrap();
        // A new write whose chunk 0 landed and whose later chunks did not.
        let new = chunks(&keys(120), "newwrite");
        store
            .put_text(&KeychainAccount::otp("pw", 0), &new[0])
            .unwrap();
        let refused = read(&store, "pw").unwrap_err();
        assert!(refused.to_string().contains("another write"), "{refused}");
        // And one whose count reaches past what is stored.
        let store = MemoryStore::new();
        store
            .put_text(&KeychainAccount::otp("pw", 0), &new[0])
            .unwrap();
        assert!(
            read(&store, "pw")
                .unwrap_err()
                .to_string()
                .contains("missing")
        );
        // A replace over the broken set clears it and stores the new one.
        replace(&store, "pw", &keys(3)).unwrap();
        assert_eq!(read(&store, "pw").unwrap(), keys(3));
    }

    #[test]
    fn a_merge_adds_and_overwrites_and_removes_nothing() {
        let store = MemoryStore::new();
        replace(&store, "pw", &keys(3)).unwrap();
        let mut newer = OtpKeys::new();
        let first = keys(1).into_keys().next().unwrap();
        newer.insert(
            first.clone(),
            OtpKey {
                secret: SecretText::new("NEWKEYAAAAAAAAAA"),
                digits: 6,
                period: 30,
                algorithm: OtpAlgorithm::Sha1,
            },
        );
        newer.insert(
            "pleasant:new".into(),
            OtpKey {
                secret: SecretText::new("OTHERKEYAAAAAAAA"),
                digits: 6,
                period: 30,
                algorithm: OtpAlgorithm::Sha1,
            },
        );
        merge(&store, "pw", newer).unwrap();
        let stored = read(&store, "pw").unwrap();
        assert_eq!(stored.len(), 4);
        assert_eq!(stored[&first].secret.expose(), "NEWKEYAAAAAAAAAA");
        // Two sources never touch each other's chunks.
        replace(&store, "other", &keys(2)).unwrap();
        assert_eq!(read(&store, "pw").unwrap().len(), 4);
    }

    #[test]
    fn a_key_spelled_short_reads_as_the_common_case() {
        let short = unspell("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(
            (short.digits, short.period, short.algorithm),
            (6, 30, OtpAlgorithm::Sha1)
        );
        let long = unspell("JBSWY3DPEHPK3PXP|8|60|SHA512").unwrap();
        assert_eq!(
            (long.digits, long.period, long.algorithm),
            (8, 60, OtpAlgorithm::Sha512)
        );
        assert!(unspell("").is_none());
        assert!(unspell("A|8").is_none());
        assert!(unspell("A|8|60|MD5").is_none());
        assert!(unspell("A|8|60|SHA1|x").is_none());
    }

    #[test]
    fn a_store_that_keeps_no_text_refuses_rather_than_dropping_keys() {
        struct Plain;
        impl SecretStore for Plain {
            fn get(&self, _: &KeychainAccount) -> Result<Option<crate::Secret>, SecretError> {
                Ok(None)
            }
            fn put(&self, _: &KeychainAccount, _: &crate::Secret) -> Result<(), SecretError> {
                Ok(())
            }
            fn delete(&self, _: &KeychainAccount) -> Result<(), SecretError> {
                Ok(())
            }
        }
        assert!(replace(&Plain, "pw", &keys(1)).is_err());
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::MemoryStore;

    fn one(secret: &str) -> OtpKey {
        OtpKey {
            secret: SecretText::new(secret),
            digits: 6,
            period: 30,
            algorithm: OtpAlgorithm::Sha1,
        }
    }

    /// Entity ids outside the BMP take two UTF-16 units a character: the cut
    /// counts them so, and every chunk still fits and nearly fills.
    #[test]
    fn wide_characters_are_counted_in_utf16_units() {
        let keys: OtpKeys = (0..300)
            .map(|i| {
                (
                    format!("pw:\u{1F511}\u{1F511}{i:05}"),
                    one("JBSWY3DPEHPK3PXP"),
                )
            })
            .collect();
        let store = MemoryStore::new();
        replace(&store, "pw", &keys).unwrap();
        assert_eq!(read(&store, "pw").unwrap(), keys);
        let cut = chunks(&keys, "w");
        for (i, chunk) in cut.iter().enumerate() {
            let len = chunk.encode_utf16().count();
            assert!(len <= CHUNK_CHARS, "{len}");
            if i + 1 < cut.len() {
                assert!(len > 1_150, "chunk {i} holds only {len}");
            }
        }
    }

    #[test]
    fn a_header_that_is_not_ours_is_refused() {
        for bad in [
            "k1 w 3/2\n{}",
            "k1 w 0/1 x\n{}",
            "k2 w 0/1\n{}",
            "k1 w 0-1\n{}",
            "nonewline",
        ] {
            assert!(header(bad).is_err(), "{bad:?}");
        }
        assert!(header("k1 w 0/1\n{}").is_ok());
    }

    #[test]
    fn a_later_chunk_numbered_or_counted_otherwise_is_refused() {
        let keys: OtpKeys = (0..100)
            .map(|i| (format!("pw:{i:060}"), one("JBSWY3DPEHPK3PXP")))
            .collect();
        let cut = chunks(&keys, "same");
        assert!(cut.len() >= 3);
        // Chunk 1 replaced by one of the same write but another number.
        let store = MemoryStore::new();
        for (n, c) in cut.iter().enumerate() {
            store.put_text(&KeychainAccount::otp("pw", n), c).unwrap();
        }
        store
            .put_text(&KeychainAccount::otp("pw", 1), &cut[2])
            .unwrap();
        assert!(read(&store, "pw").is_err());
        // Chunk 1 of the same write id but a different count.
        let other = chunks(
            &keys
                .iter()
                .take(90)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            "same",
        );
        assert_ne!(other.len(), cut.len());
        store
            .put_text(&KeychainAccount::otp("pw", 1), &other[1])
            .unwrap();
        assert!(read(&store, "pw").is_err());
    }

    #[test]
    fn two_writes_never_share_an_id() {
        let (a, b) = (write_id(), write_id());
        assert_ne!(a, b);
        assert!(!a.is_empty() && !a.contains(' ') && !a.contains('\n'));
    }

    /// A chunk 0 that does not read leaves the count unknown: a replace clears
    /// every chunk it finds after the ones it wrote.
    #[test]
    fn a_replace_over_an_unreadable_chunk_0_clears_the_whole_tail() {
        let store = MemoryStore::new();
        for n in 0..5 {
            store
                .put_text(&KeychainAccount::otp("pw", n), "garbage")
                .unwrap();
        }
        replace(
            &store,
            "pw",
            &[("pw:e1".to_owned(), one("JBSWY3DPEHPK3PXP"))]
                .into_iter()
                .collect(),
        )
        .unwrap();
        for n in 1..5 {
            assert!(
                store
                    .get_text(&KeychainAccount::otp("pw", n))
                    .unwrap()
                    .is_none(),
                "chunk {n} left"
            );
        }
        assert_eq!(read(&store, "pw").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn the_async_wrappers_reach_the_store() {
        let store: Arc<dyn SecretStore> = Arc::new(MemoryStore::new());
        let keys: OtpKeys = [("pw:e1".to_owned(), one("JBSWY3DPEHPK3PXP"))]
            .into_iter()
            .collect();
        assert_eq!(spawn::replace(&store, "pw", keys.clone()).await.unwrap(), 1);
        assert_eq!(spawn::read(&store, "pw").await.unwrap(), keys);
        let more: OtpKeys = [("pw:e2".to_owned(), one("KRSXG5CTMVRXEZLU"))]
            .into_iter()
            .collect();
        assert_eq!(spawn::merge(&store, "pw", more).await.unwrap(), 1);
        assert_eq!(spawn::read(&store, "pw").await.unwrap().len(), 2);
        spawn::delete(&store, "pw").await.unwrap();
        assert!(spawn::read(&store, "pw").await.unwrap().is_empty());
        assert!(
            store
                .get_text(&KeychainAccount::otp("pw", 0))
                .unwrap()
                .is_none()
        );
    }
}
