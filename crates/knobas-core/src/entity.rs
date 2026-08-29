//! Entity addressing.
//!
//! An entity id is the string `"<namespace>:<key>"`. The namespace is either a
//! source id (`jira`, `gitea`, `confluence`) or a local kind (`note`, `ctx`).
//! Only the *first* `:` separates the two halves, so a key is free to contain
//! further `:` and `#` characters -- `confluence:ENG:SEPA design` has the key
//! `ENG:SEPA design`. The entity's kind is not part of the id; it lives in
//! `knobas.entity.kind`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// A reference to an entity, addressed as `"<namespace>:<key>"`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct EntityRef {
    pub namespace: String,
    pub key: String,
}

/// Why a string is not a valid entity id.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EntityRefError {
    /// No `:` at all, so there is nothing to split on.
    #[error("entity id {0:?} has no ':' separating namespace from key")]
    MissingSeparator(String),
    /// The half before the first `:` is empty or whitespace-only.
    #[error("entity id {0:?} has an empty namespace")]
    EmptyNamespace(String),
    /// The half after the first `:` is empty or whitespace-only.
    #[error("entity id {0:?} has an empty key")]
    EmptyKey(String),
}

impl EntityRef {
    /// Build a reference from its two halves without parsing.
    pub fn new(namespace: &str, key: &str) -> Self {
        Self {
            namespace: namespace.to_owned(),
            key: key.to_owned(),
        }
    }

    /// Parse `"<namespace>:<key>"`, splitting on the **first** `:`.
    ///
    /// Both halves must carry non-whitespace content. The halves are stored
    /// verbatim, so `parse(s).to_string() == s` for every accepted `s`.
    pub fn parse(s: &str) -> Result<Self, EntityRefError> {
        let (namespace, key) = s
            .split_once(':')
            .ok_or_else(|| EntityRefError::MissingSeparator(s.to_owned()))?;
        if namespace.trim().is_empty() {
            return Err(EntityRefError::EmptyNamespace(s.to_owned()));
        }
        if key.trim().is_empty() {
            return Err(EntityRefError::EmptyKey(s.to_owned()));
        }
        Ok(Self::new(namespace, key))
    }
}

/// Namespaces knobas keeps for its own entities.
///
/// The namespace half of an id is either a source id or a local kind, and
/// these are the local kinds: `note:…`, `ctx:…`, `asset:…`, `route:…` and
/// `monitor:…` are written by knobas itself, never by an adapter. A source
/// that called itself `monitor` would write its items where monitors live, and
/// the per-item namespace guard could not tell -- it can only compare an item
/// against the source id it was given.
///
/// Here rather than in the sync engine because two independent places have to
/// agree on it: the engine rejects a run whose descriptor claims one of these,
/// and the SPI's contract battery rejects the adapter *before* it ever gets
/// that far. A second copy is how an adapter ends up certified against a list
/// that no longer matches the one that will refuse it.
pub const RESERVED_NAMESPACES: [&str; 5] = ["note", "ctx", "asset", "route", "monitor"];

/// Whether `namespace` is one knobas keeps for itself.
///
/// Case-insensitive: `NOTE:7f2c` addresses the same namespace as `note:7f2c`
/// to every human reading it, and a guard that disagreed would be trivially
/// side-stepped.
#[must_use]
pub fn is_reserved_namespace(namespace: &str) -> bool {
    RESERVED_NAMESPACES
        .iter()
        .any(|reserved| namespace.eq_ignore_ascii_case(reserved))
}

/// Display metadata for a kind **knobas owns**, declared by knobas itself.
///
/// The shape [`knobas_source::KindInfo`] has, minus the one field that only
/// means something for a synced kind: an owned kind is emitted by no sync, so
/// `full_sync_exhaustive` has no answer to give and the catalog supplies
/// `false` when it builds a `KindInfo` from this.
///
/// `&'static str` throughout because there is nothing to configure: the list
/// below is the whole of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnedKind {
    /// The `knobas.entity.kind` value, which is **also** the id namespace --
    /// see [`OWNED_KINDS`].
    pub id: &'static str,
    /// What one of them is called.
    pub label: &'static str,
    /// What several are called.
    pub plural: &'static str,
    /// Two characters, for the fixed-width chip.
    pub monogram: &'static str,
}

/// The entity kinds knobas **owns**: written by knobas itself, mirrored from
/// nowhere, emitted by no sync.
///
/// This is the list the kind catalog carries *in addition to* what source
/// descriptors declare (issue #46, story 21: an owned kind enters the catalog
/// "without pretending to be a source"). It lives here, beside
/// [`RESERVED_NAMESPACES`], because of the invariant that ties the two:
///
/// **every owned kind's id is also a reserved namespace**, and that is
/// load-bearing rather than tidy. Migration `0006`'s `item_entity_reserved_chk`
/// is written on the *namespace* half of an entity id, and the argument that a
/// note can never be swept crosses from the kind ("notes are owned") to the
/// namespace ("no mirror row may name a `note:` id"). If a later owned kind
/// were given an id that is not a reserved namespace, that argument would
/// silently stop holding for it -- so a test walks this list against
/// [`is_reserved_namespace`], and a second one walks `RESERVED_NAMESPACES`
/// against the migration.
///
/// The monograms are knobas' own words and are deliberately the ones the shell
/// already uses (`app/src/lib/shell/kinds.ts`): a kind whose chip said `NO` in
/// the launcher and `NT` in a room tile would be two kinds to a reader.
pub const OWNED_KINDS: &[OwnedKind] = &[OwnedKind {
    id: "note",
    label: "Note",
    plural: "Notes",
    monogram: "NT",
}];

/// Whether `kind` is one knobas owns rather than mirrors.
#[must_use]
pub fn is_owned_kind(kind: &str) -> bool {
    OWNED_KINDS
        .iter()
        .any(|owned| kind.eq_ignore_ascii_case(owned.id))
}

impl fmt::Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.key)
    }
}

impl FromStr for EntityRef {
    type Err = EntityRefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for EntityRef {
    type Error = EntityRefError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::parse(&s)
    }
}

impl From<EntityRef> for String {
    fn from(e: EntityRef) -> Self {
        e.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_source_and_local_ids() {
        let e = EntityRef::parse("jira:PAY-231").unwrap();
        assert_eq!((e.namespace.as_str(), e.key.as_str()), ("jira", "PAY-231"));
        // key may itself contain ':' and '#'
        let e = EntityRef::parse("gitea:tidewater/payout-service#142").unwrap();
        assert_eq!(e.key, "tidewater/payout-service#142");
        let e = EntityRef::parse("confluence:ENG:SEPA design").unwrap();
        assert_eq!(e.key, "ENG:SEPA design");
        assert_eq!(EntityRef::new("note", "7f2c").to_string(), "note:7f2c");
    }

    #[test]
    fn rejects_malformed() {
        for bad in ["", "jira", ":PAY-1", "jira:", "  :  "] {
            assert!(EntityRef::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    /// The list is matched the way ids are actually written, which includes
    /// the case a person types.
    #[test]
    fn reserved_namespaces_are_matched_case_insensitively() {
        for reserved in RESERVED_NAMESPACES {
            assert!(is_reserved_namespace(reserved));
            assert!(is_reserved_namespace(&reserved.to_uppercase()));
        }
        for open in ["jira", "gitea", "mock", "uptime-kuma", "notes", "context"] {
            assert!(!is_reserved_namespace(open), "{open:?} is not reserved");
        }
    }

    /// Every kind knobas owns is addressed in a namespace knobas keeps.
    ///
    /// The two lists are one fact written twice, and migration `0006` reads
    /// only the second: `item_entity_reserved_chk` forbids a mirror row whose
    /// `entity_id` is in a reserved *namespace*, and that is the whole reason
    /// an owned kind cannot be swept. An owned kind whose id were not also a
    /// reserved namespace would inherit none of it, silently.
    #[test]
    fn every_owned_kind_is_addressed_in_a_reserved_namespace() {
        assert!(
            !OWNED_KINDS.is_empty(),
            "the walk must have something to do"
        );
        for owned in OWNED_KINDS {
            assert!(
                is_reserved_namespace(owned.id),
                "{:?} is an owned kind that no reserved namespace covers",
                owned.id
            );
            assert!(is_owned_kind(owned.id));
            assert!(is_owned_kind(&owned.id.to_uppercase()));
        }
        for mirrored in ["ticket", "pr", "build", "page", "commit", "branch", "repo"] {
            assert!(!is_owned_kind(mirrored), "{mirrored:?} is mirrored");
        }
    }

    /// A monogram is drawn into a fixed-width chip, so "two characters" is an
    /// invariant here for the same reason it is one in
    /// `knobas_search::vocab::derive_monogram` -- which is what a kind knobas
    /// does *not* declare falls through to.
    #[test]
    fn an_owned_kinds_display_metadata_is_complete() {
        for owned in OWNED_KINDS {
            assert_eq!(
                owned.monogram.chars().count(),
                2,
                "{:?} has monogram {:?}",
                owned.id,
                owned.monogram
            );
            for (field, value) in [("label", owned.label), ("plural", owned.plural)] {
                assert!(!value.trim().is_empty(), "{:?} has no {field}", owned.id);
            }
        }
    }

    /// The namespaces this crate reserves and the ones migration `0006`'s
    /// `item_entity_reserved_chk` refuses are one list in two languages, and
    /// neither may grow without the other.
    ///
    /// A namespace added here but not there is a mirror row that can be written
    /// over knobas-owned content -- which is exactly what the constraint exists
    /// to make impossible, so the omission would be silent and total. One added
    /// there but not here is a namespace the sync engine still hands out as a
    /// source id, and every row that source writes then fails the CHECK.
    ///
    /// The same shape as `link::Origin`'s pin against `0003`, and it reads
    /// `0006` because `0006` is where the constraint is: applied migrations are
    /// never edited, so widening the list means a later migration that drops
    /// and re-adds it, and repointing this test is part of doing so.
    #[test]
    fn the_reserved_namespaces_are_exactly_what_the_migration_refuses() {
        let migration = include_str!("../../knobas-db/migrations/0006_notes.sql");
        let line = migration
            .lines()
            .find(|line| line.contains("check (entity_id !~*"))
            .expect("item_entity_reserved_chk is missing from 0006");

        let listed: Vec<&str> = line
            .split_once("'^(")
            .and_then(|(_, rest)| rest.split_once("):'"))
            .map(|(group, _)| group.split('|').collect())
            .expect("the constraint lists its namespaces as one alternation");

        for namespace in RESERVED_NAMESPACES {
            assert!(
                listed.contains(&namespace),
                "{namespace:?} is reserved here but not refused by the constraint: {line}"
            );
        }
        assert_eq!(
            listed.len(),
            RESERVED_NAMESPACES.len(),
            "the constraint and RESERVED_NAMESPACES list different numbers: {line}"
        );
    }

    #[test]
    fn serde_roundtrips_as_string() {
        let e = EntityRef::parse("jira:PAY-231").unwrap();
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(s, "\"jira:PAY-231\"");
        assert_eq!(serde_json::from_str::<EntityRef>(&s).unwrap(), e);
    }
}
