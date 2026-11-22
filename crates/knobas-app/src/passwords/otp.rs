//! One-time codes, made on this machine (ADR-0056, RFC 6238).
//!
//! A code is made from the key the last sync kept in the OS keychain and the
//! clock. It reads nothing from the source, so it writes no line in the
//! source's audit log and works while the source is down.

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use knobas_source::passwords::{OtpAlgorithm, OtpKey};

/// A code for one instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    /// The digits, zero-padded to the key's length.
    pub code: String,
    pub digits: u8,
    /// Seconds a code holds.
    pub period: u16,
    /// Milliseconds until the next code.
    pub left_ms: u64,
}

/// Why no code could be made from a key.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodeError {
    #[error("the code key is not Base32")]
    NotBase32,
    #[error("the code key is empty")]
    Empty,
    #[error("a code of {0} digits or a period of {1} s is not one knobas makes")]
    Shape(u8, u16),
}

/// The key's bytes: Base32, any case, spaces and padding ignored.
fn key_bytes(secret: &str) -> Result<Vec<u8>, CodeError> {
    let clean: String = secret
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=' && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if clean.is_empty() {
        return Err(CodeError::Empty);
    }
    BASE32_NOPAD
        .decode(clean.as_bytes())
        .map_err(|_| CodeError::NotBase32)
}

fn hmac_of(algorithm: OtpAlgorithm, key: &[u8], counter: u64) -> Vec<u8> {
    let message = counter.to_be_bytes();
    // `new_from_slice` takes a key of any length for HMAC, so it never fails.
    match algorithm {
        OtpAlgorithm::Sha1 => {
            let mut mac = Hmac::<sha1::Sha1>::new_from_slice(key).expect("HMAC takes any key");
            mac.update(&message);
            mac.finalize().into_bytes().to_vec()
        }
        OtpAlgorithm::Sha256 => {
            let mut mac =
                Hmac::<sha2_otp::Sha256>::new_from_slice(key).expect("HMAC takes any key");
            mac.update(&message);
            mac.finalize().into_bytes().to_vec()
        }
        OtpAlgorithm::Sha512 => {
            let mut mac =
                Hmac::<sha2_otp::Sha512>::new_from_slice(key).expect("HMAC takes any key");
            mac.update(&message);
            mac.finalize().into_bytes().to_vec()
        }
    }
}

/// RFC 4226's dynamic truncation of an HMAC, to `digits` digits.
fn truncate(hash: &[u8], digits: u8) -> String {
    let offset = usize::from(hash[hash.len() - 1] & 0x0f);
    let binary = (u32::from(hash[offset] & 0x7f) << 24)
        | (u32::from(hash[offset + 1]) << 16)
        | (u32::from(hash[offset + 2]) << 8)
        | u32::from(hash[offset + 3]);
    let modulo = 10u64.pow(u32::from(digits));
    let value = u64::from(binary) % modulo;
    format!("{value:0width$}", width = usize::from(digits))
}

/// The code for the instant `unix_ms`, milliseconds since 1970.
///
/// # Errors
/// [`CodeError`] for a key that is not Base32, or a shape outside 6 to 8
/// digits and 1 to 300 seconds.
pub fn code_at(key: &OtpKey, unix_ms: u64) -> Result<Code, CodeError> {
    if !(6..=8).contains(&key.digits) || !(1..=300).contains(&key.period) {
        return Err(CodeError::Shape(key.digits, key.period));
    }
    let bytes = key_bytes(key.secret.expose())?;
    let period_ms = u64::from(key.period) * 1_000;
    let counter = unix_ms / period_ms;
    let hash = hmac_of(key.algorithm, &bytes, counter);
    Ok(Code {
        code: truncate(&hash, key.digits),
        digits: key.digits,
        period: key.period,
        left_ms: period_ms - unix_ms % period_ms,
    })
}

/// The code for now.
///
/// # Errors
/// As [`code_at`].
pub fn code_now(key: &OtpKey) -> Result<Code, CodeError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
    code_at(key, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use knobas_source::passwords::SecretText;

    fn key(seed: &[u8], digits: u8, period: u16, algorithm: OtpAlgorithm) -> OtpKey {
        OtpKey {
            secret: SecretText::new(BASE32_NOPAD.encode(seed)),
            digits,
            period,
            algorithm,
        }
    }

    /// RFC 6238 appendix B: the test vectors, 8 digits, for the three hashes,
    /// each with the seed the errata give for its hash's length.
    #[test]
    fn the_rfc_6238_vectors() {
        let sha1 = key(b"12345678901234567890", 8, 30, OtpAlgorithm::Sha1);
        let sha256 = key(
            b"12345678901234567890123456789012",
            8,
            30,
            OtpAlgorithm::Sha256,
        );
        let sha512 = key(
            b"1234567890123456789012345678901234567890123456789012345678901234",
            8,
            30,
            OtpAlgorithm::Sha512,
        );
        let vectors: [(u64, &str, &str, &str); 6] = [
            (59, "94287082", "46119246", "90693936"),
            (1_111_111_109, "07081804", "68084774", "25091201"),
            (1_111_111_111, "14050471", "67062674", "99943326"),
            (1_234_567_890, "89005924", "91819424", "93441116"),
            (2_000_000_000, "69279037", "90698825", "38618901"),
            (20_000_000_000, "65353130", "77737706", "47863826"),
        ];
        for (secs, one, two, five) in vectors {
            let ms = secs * 1_000;
            assert_eq!(code_at(&sha1, ms).unwrap().code, one, "SHA-1 at {secs}");
            assert_eq!(code_at(&sha256, ms).unwrap().code, two, "SHA-256 at {secs}");
            assert_eq!(
                code_at(&sha512, ms).unwrap().code,
                five,
                "SHA-512 at {secs}"
            );
        }
    }

    #[test]
    fn six_digits_is_the_last_six_of_eight_and_left_counts_down() {
        let six = key(b"12345678901234567890", 6, 30, OtpAlgorithm::Sha1);
        let at = code_at(&six, 59_000).unwrap();
        assert_eq!(at.code, "287082");
        assert_eq!(at.left_ms, 1_000);
        assert_eq!(code_at(&six, 60_000).unwrap().left_ms, 30_000);
        assert_eq!(code_at(&six, 30_001).unwrap().left_ms, 29_999);
    }

    #[test]
    fn a_sixty_second_key_holds_its_code_for_sixty_seconds() {
        let slow = key(b"12345678901234567890", 6, 60, OtpAlgorithm::Sha1);
        let a = code_at(&slow, 60_000).unwrap();
        let b = code_at(&slow, 119_999).unwrap();
        let c = code_at(&slow, 120_000).unwrap();
        assert_eq!(a.code, b.code);
        assert_ne!(b.code, c.code);
        assert_eq!(a.left_ms, 60_000);
        assert_eq!(a.period, 60);
        // The same counter as a 30 s key at twice the time.
        let fast = key(b"12345678901234567890", 6, 30, OtpAlgorithm::Sha1);
        assert_eq!(a.code, code_at(&fast, 30_000).unwrap().code);
    }

    #[test]
    fn a_key_is_read_in_any_case_with_spaces_and_padding() {
        let plain = key(b"12345678901234567890", 6, 30, OtpAlgorithm::Sha1);
        let spelled = OtpKey {
            secret: SecretText::new("gezd gnbv gy3t qojq gezd gnbv gy3t qojq===="),
            ..plain.clone()
        };
        assert_eq!(code_at(&plain, 59_000), code_at(&spelled, 59_000));
        let bad = OtpKey {
            secret: SecretText::new("not base32!"),
            ..plain.clone()
        };
        assert_eq!(code_at(&bad, 0), Err(CodeError::NotBase32));
        let empty = OtpKey {
            secret: SecretText::new("  "),
            ..plain.clone()
        };
        assert_eq!(code_at(&empty, 0), Err(CodeError::Empty));
        let odd = OtpKey { digits: 4, ..plain };
        assert_eq!(code_at(&odd, 0), Err(CodeError::Shape(4, 30)));
    }
}
