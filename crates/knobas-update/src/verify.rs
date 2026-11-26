//! The signature check (decisions D3, D8).
//!
//! A `.sig` file is base64 of a four-line minisign signature in the prehashed form, whose trusted
//! comment is `timestamp:<seconds>` TAB `file:<the file's base name>` (measured on the pinned
//! CLI). A download is used only when one of the trusted keys verifies its bytes **and** the
//! comment names the file the manifest names; [`crate::manifest::bind`] adds the version.

use std::io::Read as _;
use std::path::Path;

use base64::Engine as _;
use minisign_verify::{Signature, StreamVerifier};

use crate::keys::Keys;

/// Why a file was not accepted.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VerifyError {
    #[error("the signature does not read: {0}")]
    Unreadable(String),
    #[error("no key this build trusts made the signature")]
    UnknownKey,
    #[error("the signature does not match the file")]
    Mismatch,
    #[error("the signature is for {signed:?}, not {expected:?}")]
    OtherFile { signed: String, expected: String },
    #[error("{0}")]
    Io(String),
}

/// A `.sig` file's text, read.
///
/// # Errors
///
/// [`VerifyError::Unreadable`].
pub fn decode(text: &str) -> Result<Signature, VerifyError> {
    let text = text.trim();
    let lines = match base64::engine::general_purpose::STANDARD.decode(text) {
        Ok(bytes) => {
            String::from_utf8(bytes).map_err(|_| VerifyError::Unreadable("not UTF-8".to_owned()))?
        }
        Err(_) => text.to_owned(),
    };
    Signature::decode(&lines).map_err(|error| VerifyError::Unreadable(error.to_string()))
}

/// The file name the signature's trusted comment names, if it names one.
#[must_use]
pub fn signed_file(signature: &Signature) -> Option<&str> {
    signature
        .trusted_comment()
        .split('\t')
        .find_map(|part| part.strip_prefix("file:"))
}

/// The comment's file name against `expected`, by equality.
///
/// # Errors
///
/// [`VerifyError::OtherFile`].
pub fn names(signature: &Signature, expected: &str) -> Result<(), VerifyError> {
    match signed_file(signature) {
        Some(signed) if signed == expected => Ok(()),
        signed => Err(VerifyError::OtherFile {
            signed: signed.unwrap_or_default().to_owned(),
            expected: expected.to_owned(),
        }),
    }
}

/// A check in progress: bytes go in as they land, [`Streaming::finish`] says whether they were
/// what the signature signed.
pub struct Streaming<'a> {
    inner: StreamVerifier<'a>,
}

impl Streaming<'_> {
    /// The next bytes.
    pub fn update(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }

    /// Whether the bytes so far are what was signed.
    ///
    /// # Errors
    ///
    /// [`VerifyError::Mismatch`].
    pub fn finish(mut self) -> Result<(), VerifyError> {
        self.inner.finalize().map_err(|_| VerifyError::Mismatch)
    }
}

impl Keys {
    /// A streamed check of `signature`, by whichever trusted key made it.
    ///
    /// # Errors
    ///
    /// [`VerifyError::UnknownKey`] when no trusted key did, or the signature is not prehashed.
    pub fn stream<'a>(&'a self, signature: &'a Signature) -> Result<Streaming<'a>, VerifyError> {
        self.all()
            .iter()
            .find_map(|trusted| trusted.key.verify_stream(signature).ok())
            .map(|inner| Streaming { inner })
            .ok_or(VerifyError::UnknownKey)
    }

    /// Whether `bytes` are what `signature` signed, and it names `expected`.
    ///
    /// # Errors
    ///
    /// [`VerifyError`].
    pub fn verify_bytes(
        &self,
        bytes: &[u8],
        signature: &Signature,
        expected: &str,
    ) -> Result<(), VerifyError> {
        let mut check = self.stream(signature)?;
        check.update(bytes);
        check.finish()?;
        names(signature, expected)
    }

    /// Read `path` once, checking it, and copy it in the same pass to `to` when given. What was
    /// checked is what is used (decisions D8): the copy is the file an installer runs or an
    /// archive is unpacked from, so a swap of the cache folder's file after the check changes
    /// nothing.
    ///
    /// On a failure the copy is removed.
    ///
    /// # Errors
    ///
    /// [`VerifyError`].
    pub fn verify_file(
        &self,
        path: &Path,
        signature: &Signature,
        expected: &str,
        to: Option<&Path>,
    ) -> Result<(), VerifyError> {
        let io = |error: std::io::Error| VerifyError::Io(format!("{}: {error}", path.display()));
        let mut check = self.stream(signature)?;
        let mut file = std::fs::File::open(path).map_err(io)?;
        let mut copy = match to {
            Some(to) => Some((
                std::fs::File::create(to)
                    .map_err(|error| VerifyError::Io(format!("{}: {error}", to.display())))?,
                to.to_path_buf(),
            )),
            None => None,
        };
        let outcome = (|| {
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                let read = file.read(&mut buffer).map_err(io)?;
                if read == 0 {
                    break;
                }
                check.update(&buffer[..read]);
                if let Some((out, to)) = copy.as_mut() {
                    std::io::Write::write_all(out, &buffer[..read])
                        .map_err(|error| VerifyError::Io(format!("{}: {error}", to.display())))?;
                }
            }
            if let Some((out, to)) = copy.as_mut() {
                out.sync_all()
                    .map_err(|error| VerifyError::Io(format!("{}: {error}", to.display())))?;
            }
            check.finish()?;
            names(signature, expected)
        })();
        if outcome.is_err()
            && let Some((out, to)) = copy
        {
            drop(out);
            let _ = std::fs::remove_file(&to);
        }
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "knobas_3.2.9-ux28_x64-setup.exe";
    const ONE: &str = include_str!("../tests/fixtures/one.pub");
    const TWO: &str = include_str!("../tests/fixtures/two.pub");
    const BODY: &[u8] = include_bytes!("../tests/fixtures/one/knobas_3.2.9-ux28_x64-setup.exe");
    const SIG_ONE: &str = include_str!("../tests/fixtures/one/knobas_3.2.9-ux28_x64-setup.exe.sig");
    const SIG_TWO: &str = include_str!("../tests/fixtures/two/knobas_3.2.9-ux28_x64-setup.exe.sig");
    const SIG_THREE: &str =
        include_str!("../tests/fixtures/three/knobas_3.2.9-ux28_x64-setup.exe.sig");

    fn pair() -> Keys {
        Keys::from_texts(&[("primary", ONE), ("spare", TWO)]).unwrap()
    }

    /// Decisions D2: the primary's signature and the spare's are both accepted, a third key's
    /// is not. Mutant: verify with the first key only, and the spare's fixture fails.
    #[test]
    fn a_signature_by_either_key_is_accepted_and_by_a_third_refused() {
        let keys = pair();
        assert_eq!(
            keys.verify_bytes(BODY, &decode(SIG_ONE).unwrap(), FILE),
            Ok(())
        );
        assert_eq!(
            keys.verify_bytes(BODY, &decode(SIG_TWO).unwrap(), FILE),
            Ok(())
        );
        assert_eq!(
            keys.verify_bytes(BODY, &decode(SIG_THREE).unwrap(), FILE),
            Err(VerifyError::UnknownKey)
        );
    }

    #[test]
    fn one_flipped_bit_is_a_mismatch() {
        let mut body = BODY.to_vec();
        body[3] ^= 1;
        assert_eq!(
            pair().verify_bytes(&body, &decode(SIG_ONE).unwrap(), FILE),
            Err(VerifyError::Mismatch)
        );
    }

    /// The comment's name by equality, not `contains`.
    #[test]
    fn a_signature_naming_another_file_is_refused() {
        let signature = decode(SIG_ONE).unwrap();
        assert!(matches!(
            pair().verify_bytes(BODY, &signature, "x-knobas_3.2.9-ux28_x64-setup.exe"),
            Err(VerifyError::OtherFile { .. })
        ));
        assert!(matches!(
            pair().verify_bytes(BODY, &signature, "knobas_3.2.9-ux28_x64-setup"),
            Err(VerifyError::OtherFile { .. })
        ));
    }

    #[test]
    fn a_file_is_checked_and_copied_in_one_pass_and_a_bad_copy_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(FILE);
        std::fs::write(&source, BODY).unwrap();
        let to = dir.path().join("copy");
        pair()
            .verify_file(&source, &decode(SIG_ONE).unwrap(), FILE, Some(&to))
            .unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), BODY);

        let mut bad = BODY.to_vec();
        bad[0] ^= 1;
        std::fs::write(&source, bad).unwrap();
        let to = dir.path().join("copy-2");
        assert_eq!(
            pair().verify_file(&source, &decode(SIG_ONE).unwrap(), FILE, Some(&to)),
            Err(VerifyError::Mismatch)
        );
        assert!(!to.exists());
    }

    #[test]
    fn the_comment_names_the_file() {
        assert_eq!(signed_file(&decode(SIG_ONE).unwrap()), Some(FILE));
    }
}
