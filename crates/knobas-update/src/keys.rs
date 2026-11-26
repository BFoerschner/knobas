//! The keys a download must be signed by (decisions D2).
//!
//! Two public keys are compiled in, a primary and a spare, from `keys/primary.pub` and
//! `keys/spare.pub` (`build.rs`). A download is accepted when either verifies it. A local build
//! that must accept a test signature names one more key in `KNOBAS_UPDATE_EXTRA_PUBKEY` at
//! **compile** time; the release run fails when a build trusts any key but the two committed ones.

use base64::Engine as _;
use minisign_verify::PublicKey;

include!(concat!(env!("OUT_DIR"), "/shipped_keys.rs"));

/// A test key compiled into a local build (decisions D16), or none.
const EXTRA: Option<&str> = option_env!("KNOBAS_UPDATE_EXTRA_PUBKEY");

/// One trusted key.
#[derive(Debug, Clone)]
pub struct TrustedKey {
    /// `primary`, `spare` or `extra`.
    pub name: &'static str,
    /// Its minisign key id, as minisign prints it (16 hex digits).
    pub id: String,
    pub(crate) key: PublicKey,
}

/// The keys a signature is checked against.
#[derive(Debug, Clone, Default)]
pub struct Keys(Vec<TrustedKey>);

/// Why a key's text did not read as a key.
#[derive(Debug, thiserror::Error)]
#[error("{name} is not a minisign public key: {why}")]
pub struct KeyError {
    name: String,
    why: String,
}

impl Keys {
    /// The keys this build trusts: the two committed ones and the compile-time extra.
    ///
    /// # Errors
    ///
    /// [`KeyError`] when a compiled-in key does not decode, which the tests stop before a build
    /// ships it.
    pub fn compiled() -> Result<Self, KeyError> {
        let mut named: Vec<(&'static str, &str)> = SHIPPED.to_vec();
        if let Some(extra) = EXTRA.filter(|text| !text.trim().is_empty()) {
            named.push(("extra", extra));
        }
        Self::from_texts(&named)
    }

    /// Keys from their `.pub` files' text, each named.
    ///
    /// # Errors
    ///
    /// [`KeyError`] naming the first that does not decode.
    pub fn from_texts(named: &[(&'static str, &str)]) -> Result<Self, KeyError> {
        named
            .iter()
            .map(|(name, text)| {
                let (key, id) = decode(text).map_err(|why| KeyError {
                    name: (*name).to_owned(),
                    why,
                })?;
                Ok(TrustedKey { name, id, key })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    /// Whether this build trusts no key at all: then updates are not set up.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The keys, in order.
    #[must_use]
    pub fn all(&self) -> &[TrustedKey] {
        &self.0
    }

    /// Whether the two committed keys are compiled into this build.
    #[must_use]
    pub fn shipped() -> bool {
        SHIPPED.len() == 2
    }
}

/// A `.pub` file's text (base64 of minisign's two lines), or the bare second line, as a key and
/// its id.
///
/// # Errors
///
/// What did not decode, in words.
pub fn decode(text: &str) -> Result<(PublicKey, String), String> {
    let text = text.trim();
    let lines = match base64::engine::general_purpose::STANDARD.decode(text) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| "not UTF-8".to_owned())?,
        Err(_) => text.to_owned(),
    };
    let key_line = lines
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))
        .ok_or_else(|| "no key line".to_owned())?;
    let key = PublicKey::from_base64(key_line).map_err(|error| error.to_string())?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(key_line)
        .map_err(|error| error.to_string())?;
    let id: String = raw
        .get(2..10)
        .ok_or_else(|| "too short".to_owned())?
        .iter()
        .rev()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    Ok((key, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: &str = include_str!("../tests/fixtures/one.pub");
    const TWO: &str = include_str!("../tests/fixtures/two.pub");
    const THREE: &str = include_str!("../tests/fixtures/three.pub");

    #[test]
    fn a_pub_file_decodes_with_the_id_minisign_prints() {
        let (_, id) = decode(ONE).unwrap();
        assert_eq!(id, "6A21557FFC573546");
    }

    /// Both committed keys parse, with different ids; until Björn commits them the build
    /// trusts none and says it is not set up (the brief: "Updates aren't set up").
    #[test]
    fn the_committed_keys_decode() {
        let keys = Keys::from_texts(SHIPPED).expect("the committed keys decode");
        match SHIPPED.len() {
            0 => assert!(keys.is_empty()),
            2 => assert_ne!(keys.all()[0].id, keys.all()[1].id),
            n => panic!("{n} committed keys"),
        }
    }

    /// Decisions step 0's stop: a build never ships the test keys as its own.
    #[test]
    fn the_shipped_keys_are_not_the_fixture_keys() {
        let fixtures: Vec<String> = [ONE, TWO, THREE]
            .iter()
            .map(|text| decode(text).unwrap().1)
            .collect();
        let shipped = Keys::from_texts(SHIPPED).unwrap();
        for key in shipped.all() {
            assert!(!fixtures.contains(&key.id), "{} is a fixture key", key.name);
        }
    }

    #[test]
    fn a_line_that_is_no_key_is_refused_by_name() {
        let error = Keys::from_texts(&[("spare", "bm90IGEga2V5")]).unwrap_err();
        assert!(error.to_string().starts_with("spare is not"), "{error}");
    }
}
