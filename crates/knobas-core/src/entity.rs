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

    #[test]
    fn serde_roundtrips_as_string() {
        let e = EntityRef::parse("jira:PAY-231").unwrap();
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(s, "\"jira:PAY-231\"");
        assert_eq!(serde_json::from_str::<EntityRef>(&s).unwrap(), e);
    }
}
