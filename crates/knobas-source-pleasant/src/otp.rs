//! One-time-code keys in an entry's custom fields, in the four forms the
//! KeePass plugins keep them (ADR-0056, Björn's decision 5).
//!
//! | Plugin | Field | Read as |
//! |---|---|---|
//! | KeePass 2.47+, built in | `TimeOtp-Secret-Base32` (or `-Secret`, `-Hex`, `-Base64`), `TimeOtp-Length`, `-Period`, `-Algorithm` | `keepass` |
//! | KeeTrayTOTP | `TOTP Seed`, `TOTP Settings` (`period;digits[;url]`) | `tray_totp` |
//! | KeeOtp | `otp` = `key=…&step=…&size=…&otpHashMode=…` | `keeotp` |
//! | KeePassXC | `otp` = `otpauth://totp/…?secret=…` | `otpauth` |
//!
//! [`take`] removes every one of those fields from the entry's fields,
//! whatever it finds in them, so none can reach the mirror.

use std::collections::BTreeMap;

use data_encoding::{BASE32_NOPAD, BASE64, HEXLOWER_PERMISSIVE};
use knobas_source::passwords::{OTP_UNREADABLE, OtpAlgorithm, OtpInfo, OtpKey, SecretText};

/// The built-in plugin's fields.
const KEEPASS_FIELDS: &[&str] = &[
    "TimeOtp-Secret-Base32",
    "TimeOtp-Secret",
    "TimeOtp-Secret-Hex",
    "TimeOtp-Secret-Base64",
    "TimeOtp-Length",
    "TimeOtp-Period",
    "TimeOtp-Algorithm",
];
const TRAY_SEED: &str = "TOTP Seed";
const TRAY_SETTINGS: &str = "TOTP Settings";
const OTP_FIELD: &str = "otp";

/// Whether a field's name is one a code key lives in.
#[must_use]
pub fn is_key_field(name: &str) -> bool {
    KEEPASS_FIELDS.iter().any(|f| f.eq_ignore_ascii_case(name))
        || name.eq_ignore_ascii_case(TRAY_SEED)
        || name.eq_ignore_ascii_case(TRAY_SETTINGS)
        || name.eq_ignore_ascii_case(OTP_FIELD)
}

/// What [`take`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The record's `otp`: the format and how codes are made. Format
    /// [`OTP_UNREADABLE`] for a key knobas cannot read.
    pub info: OtpInfo,
    /// The key itself, `None` when unreadable.
    pub key: Option<OtpKey>,
}

/// A key's bytes, re-encoded as canonical Base32 (upper case, no padding).
fn base32(bytes: &[u8]) -> Option<String> {
    (!bytes.is_empty()).then(|| BASE32_NOPAD.encode(bytes))
}

/// Base32 text as written in a field: any case, spaces and padding ignored.
fn from_base32(text: &str) -> Option<Vec<u8>> {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '=' && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if clean.is_empty() {
        return None;
    }
    BASE32_NOPAD.decode(clean.as_bytes()).ok()
}

fn number<T: std::str::FromStr>(text: Option<&str>, default: T) -> Option<T> {
    match text.map(str::trim) {
        None | Some("") => Some(default),
        Some(t) => t.parse().ok(),
    }
}

fn unreadable() -> Found {
    Found {
        info: OtpInfo {
            format: OTP_UNREADABLE.to_owned(),
            digits: 6,
            period: 30,
            algorithm: OtpAlgorithm::Sha1,
        },
        key: None,
    }
}

fn found(
    format: &str,
    secret: Option<String>,
    digits: Option<u8>,
    period: Option<u16>,
    algorithm: Option<OtpAlgorithm>,
) -> Found {
    match (secret, digits, period, algorithm) {
        (Some(secret), Some(digits), Some(period), Some(algorithm))
            if (6..=8).contains(&digits) && (1..=300).contains(&period) =>
        {
            Found {
                info: OtpInfo {
                    format: format.to_owned(),
                    digits,
                    period,
                    algorithm,
                },
                key: Some(OtpKey {
                    secret: SecretText::new(secret),
                    digits,
                    period,
                    algorithm,
                }),
            }
        }
        _ => unreadable(),
    }
}

fn get<'a>(fields: &'a BTreeMap<String, String>, name: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.trim().is_empty())
}

fn keepass(fields: &BTreeMap<String, String>) -> Option<Found> {
    let bytes = if let Some(b32) = get(fields, "TimeOtp-Secret-Base32") {
        from_base32(b32)
    } else if let Some(hex) = get(fields, "TimeOtp-Secret-Hex") {
        let clean: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
        HEXLOWER_PERMISSIVE.decode(clean.as_bytes()).ok()
    } else if let Some(b64) = get(fields, "TimeOtp-Secret-Base64") {
        BASE64.decode(b64.trim().as_bytes()).ok()
    } else if let Some(text) = get(fields, "TimeOtp-Secret") {
        Some(text.as_bytes().to_vec())
    } else {
        // Only the settings fields: no key to read.
        return KEEPASS_FIELDS
            .iter()
            .any(|f| get(fields, f).is_some())
            .then(unreadable);
    };
    let algorithm = match get(fields, "TimeOtp-Algorithm") {
        None => Some(OtpAlgorithm::Sha1),
        Some(a) => OtpAlgorithm::parse(a),
    };
    Some(found(
        "keepass",
        bytes.as_deref().and_then(base32),
        number(get(fields, "TimeOtp-Length"), 6),
        number(get(fields, "TimeOtp-Period"), 30),
        algorithm,
    ))
}

fn tray(fields: &BTreeMap<String, String>) -> Option<Found> {
    let seed = get(fields, TRAY_SEED);
    let settings = get(fields, TRAY_SETTINGS);
    if seed.is_none() && settings.is_none() {
        return None;
    }
    let Some(seed) = seed else {
        return Some(unreadable());
    };
    let mut parts = settings.unwrap_or("30;6").split(';');
    let period = number(parts.next(), 30);
    // `S` is KeeTrayTOTP's Steam: five characters, not digits.
    let digits = number(parts.next(), 6);
    Some(found(
        "tray_totp",
        from_base32(seed).as_deref().and_then(base32),
        digits,
        period,
        Some(OtpAlgorithm::Sha1),
    ))
}

fn otpauth(text: &str) -> Found {
    let Ok(uri) = url::Url::parse(text.trim()) else {
        return unreadable();
    };
    if !uri.scheme().eq_ignore_ascii_case("otpauth")
        || !uri
            .host_str()
            .is_some_and(|h| h.eq_ignore_ascii_case("totp"))
    {
        return unreadable();
    }
    let pairs: BTreeMap<String, String> = uri
        .query_pairs()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.into_owned()))
        .collect();
    if pairs.contains_key("encoder") {
        // `encoder=steam`: Steam's five-character codes.
        return unreadable();
    }
    let algorithm = match pairs.get("algorithm") {
        None => Some(OtpAlgorithm::Sha1),
        Some(a) => OtpAlgorithm::parse(a),
    };
    found(
        "otpauth",
        pairs
            .get("secret")
            .and_then(|s| from_base32(s))
            .as_deref()
            .and_then(base32),
        number(pairs.get("digits").map(String::as_str), 6),
        number(pairs.get("period").map(String::as_str), 30),
        algorithm,
    )
}

fn keeotp(text: &str) -> Found {
    let pairs: BTreeMap<String, String> = url::form_urlencoded::parse(text.trim().as_bytes())
        .map(|(k, v)| (k.to_ascii_lowercase(), v.into_owned()))
        .collect();
    if pairs
        .get("type")
        .is_some_and(|t| !t.eq_ignore_ascii_case("totp"))
    {
        return unreadable();
    }
    let algorithm = match pairs.get("otphashmode") {
        None => Some(OtpAlgorithm::Sha1),
        Some(a) => OtpAlgorithm::parse(a),
    };
    found(
        "keeotp",
        pairs
            .get("key")
            .and_then(|s| from_base32(s))
            .as_deref()
            .and_then(base32),
        number(pairs.get("size").map(String::as_str), 6),
        number(pairs.get("step").map(String::as_str), 30),
        algorithm,
    )
}

/// Take the code key out of an entry's fields: every key field is removed
/// from `fields`, and what they held is answered. `None` when the entry has
/// no key field at all.
pub fn take(fields: &mut BTreeMap<String, String>) -> Option<Found> {
    let found = keepass(fields).or_else(|| tray(fields)).or_else(|| {
        get(fields, OTP_FIELD).map(|text| {
            if text
                .trim_start()
                .to_ascii_lowercase()
                .starts_with("otpauth:")
            {
                otpauth(text)
            } else {
                keeotp(text)
            }
        })
    });
    let had_any = fields.keys().any(|k| is_key_field(k));
    fields.retain(|name, _| !is_key_field(name));
    found.or_else(|| had_any.then(unreadable))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    /// RFC 6238's SHA-1 seed, `12345678901234567890`, in Base32.
    const RFC: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    fn key_of(f: &[(&str, &str)]) -> (Found, BTreeMap<String, String>) {
        let mut all = fields(f);
        all.insert("Port".into(), "5432".into());
        let found = take(&mut all).expect("a key field");
        (found, all)
    }

    #[test]
    fn the_four_formats_give_the_same_key_and_leave_no_key_field() {
        let cases: [(&[(&str, &str)], &str); 6] = [
            (&[("TimeOtp-Secret-Base32", RFC)], "keepass"),
            (&[("TimeOtp-Secret", "12345678901234567890")], "keepass"),
            (
                &[(
                    "TimeOtp-Secret-Hex",
                    "3132333435363738393031323334353637383930",
                )],
                "keepass",
            ),
            (
                &[
                    ("TOTP Seed", "gezd gnbv gy3t qojq gezd gnbv gy3t qojq"),
                    ("TOTP Settings", "30;6"),
                ],
                "tray_totp",
            ),
            (
                &[("otp", "key=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&step=30&size=6")],
                "keeotp",
            ),
            (
                &[(
                    "otp",
                    "otpauth://totp/Tidewater:payout?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&period=30&digits=6&issuer=Tidewater",
                )],
                "otpauth",
            ),
        ];
        for (f, format) in cases {
            let (found, rest) = key_of(f);
            assert_eq!(found.info.format, format, "{f:?}");
            let key = found.key.expect("readable");
            assert_eq!(key.secret.expose(), RFC, "{f:?}");
            assert_eq!(
                (key.digits, key.period, key.algorithm),
                (6, 30, OtpAlgorithm::Sha1)
            );
            assert_eq!(rest.keys().collect::<Vec<_>>(), vec!["Port"], "{f:?}");
        }
    }

    #[test]
    fn each_format_reads_its_settings() {
        let (k, _) = key_of(&[
            ("TimeOtp-Secret-Base32", RFC),
            ("TimeOtp-Length", "8"),
            ("TimeOtp-Period", "60"),
            ("TimeOtp-Algorithm", "HMAC-SHA-256"),
        ]);
        assert_eq!(
            (k.info.digits, k.info.period, k.info.algorithm),
            (8, 60, OtpAlgorithm::Sha256)
        );
        let (k, _) = key_of(&[
            ("TOTP Seed", RFC),
            ("TOTP Settings", "60;8;https://time.example"),
        ]);
        assert_eq!((k.info.digits, k.info.period), (8, 60));
        let (k, _) = key_of(&[(
            "otp",
            "key=GEZDGNBVGY3TQOJQ&step=60&size=8&otpHashMode=Sha512",
        )]);
        assert_eq!(
            (k.info.digits, k.info.period, k.info.algorithm),
            (8, 60, OtpAlgorithm::Sha512)
        );
        let (k, _) = key_of(&[(
            "otp",
            "otpauth://totp/x?secret=GEZDGNBVGY3TQOJQ&algorithm=SHA256&digits=8",
        )]);
        assert_eq!((k.info.digits, k.info.algorithm), (8, OtpAlgorithm::Sha256));
    }

    #[test]
    fn a_key_knobas_cannot_read_is_removed_and_said_unreadable() {
        for f in [
            &[(
                "otp",
                "otpauth://totp/x?secret=GEZDGNBVGY3TQOJQ&encoder=steam",
            )][..],
            &[("otp", "otpauth://hotp/x?secret=GEZDGNBVGY3TQOJQ&counter=1")],
            &[("otp", "key=not-base32!!&step=30")],
            &[("TOTP Seed", RFC), ("TOTP Settings", "30;S")],
            &[("TimeOtp-Secret-Base32", "!!!")],
            &[("TimeOtp-Length", "6")],
            &[("TOTP Settings", "30;6")],
            &[("otp", "key=GEZDGNBVGY3TQOJQ&type=Hotp")],
        ] {
            let (found, rest) = key_of(f);
            assert_eq!(found.info.format, OTP_UNREADABLE, "{f:?}");
            assert!(found.key.is_none(), "{f:?}");
            assert_eq!(rest.keys().collect::<Vec<_>>(), vec!["Port"], "{f:?}");
        }
    }

    #[test]
    fn an_entry_with_no_key_field_keeps_its_fields() {
        let mut f = fields(&[("Port", "5432"), ("Recovery PIN", "1234")]);
        assert!(take(&mut f).is_none());
        assert_eq!(f.len(), 2);
        assert!(is_key_field("OTP") && is_key_field("totp seed") && !is_key_field("Port"));
    }

    #[test]
    fn a_shape_outside_six_to_eight_digits_or_one_to_300_seconds_is_unreadable() {
        for (digits, period) in [("5", "30"), ("9", "30"), ("6", "0"), ("6", "301")] {
            let (found, _) = key_of(&[
                ("TimeOtp-Secret-Base32", RFC),
                ("TimeOtp-Length", digits),
                ("TimeOtp-Period", period),
            ]);
            assert_eq!(
                found.info.format, OTP_UNREADABLE,
                "{digits} digits, {period} s"
            );
        }
        let (found, _) = key_of(&[
            ("TimeOtp-Secret-Base32", RFC),
            ("TimeOtp-Length", "8"),
            ("TimeOtp-Period", "300"),
        ]);
        assert_eq!(found.info.format, "keepass");
    }

    #[test]
    fn a_tray_seed_without_settings_uses_thirty_seconds_and_six_digits() {
        let (found, _) = key_of(&[("TOTP Seed", RFC)]);
        assert_eq!(found.info.format, "tray_totp");
        assert_eq!((found.info.digits, found.info.period), (6, 30));
    }
}
