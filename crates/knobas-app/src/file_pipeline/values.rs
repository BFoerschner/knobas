//! The steps a **value pipeline** needs (the XML view's design, 2026-11-23,
//! sections 15 to 19; ADR-0059): [`PipeStep::Take`], which cuts a value out
//! of the file and reads XML's references or JSON's escapes, and the seven
//! steps a value is often wrapped in. Each writes through [`Capped`], as every
//! built-in step does, so the cap and the stop flag hold here too.
//!
//! Nothing decoded rests: a value pipeline is a file pipeline, kept as any
//! run is, in memory (ADR-0032), and written nowhere.

use std::io::Write as _;

use base64::Engine as _;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};

use super::steps::{Capped, Failure, io_failure};
use super::{Charset, PipeStep, TakeAs};

/// `step`'s output for `input`, where `step` is one of this module's.
pub(super) fn run(step: &PipeStep, input: &[u8], out: &mut Capped<'_>) -> Result<(), Failure> {
    let bytes = match step {
        PipeStep::Take { from, to, read } => take(input, *from, *to, *read)?,
        PipeStep::HexDecode => hex_decode(input)?,
        PipeStep::Inflate => return inflate(input, out),
        PipeStep::UrlDecode => percent_encoding::percent_decode(input).collect(),
        PipeStep::Unescape => unescape(text(input)?).into_bytes(),
        PipeStep::JsonString => json_string(text(input)?)?.into_bytes(),
        PipeStep::Jwt => jwt(text(input)?)?.into_bytes(),
        PipeStep::Charset { charset } => read_as(input, *charset).into_bytes(),
        _ => return Err(Failure::Error("not a value step".to_owned())),
    };
    out.write_all(&bytes).map_err(|e| io_failure(&e))
}

fn text(input: &[u8]) -> Result<&str, Failure> {
    std::str::from_utf8(input)
        .map_err(|error| Failure::Error(format!("the input is not UTF-8 text: {error}")))
}

/// Bytes `from` to `to` of the file, read as `read` says. The places count
/// from after a byte-order mark: the web view reads the file's text without one.
fn take(input: &[u8], from: u64, to: u64, read: TakeAs) -> Result<Vec<u8>, Failure> {
    let input = input.strip_prefix(b"\xef\xbb\xbf").unwrap_or(input);
    let len = input.len() as u64;
    if from > to || to > len {
        return Err(Failure::Error(format!(
            "the value's place, bytes {from} to {to}, is not in the file's {len} bytes: the file changed since it was read"
        )));
    }
    let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
        return Err(Failure::Error("the value's place is too far in".to_owned()));
    };
    let cut = &input[from..to];
    match read {
        TakeAs::Bytes => Ok(cut.to_vec()),
        TakeAs::XmlText => Ok(xml_text(text(cut)?).into_bytes()),
        TakeAs::XmlAttribute => Ok(xml_refs(text(cut)?).into_bytes()),
        TakeAs::JsonString => Ok(json_string(text(cut)?)?.into_bytes()),
    }
}

/// An element's text as XML reads it: CDATA sections as written, references
/// read everywhere else.
fn xml_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find("<![CDATA[") {
        out.push_str(&xml_refs(&rest[..at]));
        let inside = &rest[at + 9..];
        match inside.find("]]>") {
            Some(end) => {
                out.push_str(&inside[..end]);
                rest = &inside[end + 3..];
            }
            None => {
                out.push_str(inside);
                rest = "";
            }
        }
    }
    out.push_str(&xml_refs(rest));
    out
}

/// XML's five predefined references and character references, read; any other
/// reference stays as written.
fn xml_refs(s: &str) -> String {
    references(s, |name| match name {
        "lt" => Some("<"),
        "gt" => Some(">"),
        "amp" => Some("&"),
        "quot" => Some("\""),
        "apos" => Some("'"),
        _ => None,
    })
}

/// The references in `s` read: numeric ones always, a name by `named`.
fn references(s: &str, named: impl Fn(&str) -> Option<&'static str>) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let end = after.find(';').filter(|&end| end <= 32);
        let read = end.and_then(|end| {
            let name = &after[..end];
            let ch = if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X"))
            {
                u32::from_str_radix(hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map(String::from)
            } else if let Some(dec) = name.strip_prefix('#') {
                dec.parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(String::from)
            } else {
                named(name).map(str::to_owned)
            };
            ch.map(|ch| (ch, end))
        });
        match read {
            Some((ch, end)) => {
                out.push_str(&ch);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The entities HTML writes most, beside XML's five: what a value escaped by
/// an HTML layer holds.
fn html_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "lt" => "<",
        "gt" => ">",
        "amp" => "&",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        "copy" => "©",
        "reg" => "®",
        "trade" => "™",
        "euro" => "€",
        "pound" => "£",
        "yen" => "¥",
        "sect" => "§",
        "deg" => "°",
        "plusmn" => "±",
        "times" => "×",
        "divide" => "÷",
        "middot" => "·",
        "hellip" => "…",
        "ndash" => "–",
        "mdash" => "—",
        "lsquo" => "‘",
        "rsquo" => "’",
        "ldquo" => "“",
        "rdquo" => "”",
        "laquo" => "«",
        "raquo" => "»",
        "auml" => "ä",
        "ouml" => "ö",
        "uuml" => "ü",
        "Auml" => "Ä",
        "Ouml" => "Ö",
        "Uuml" => "Ü",
        "szlig" => "ß",
        "eacute" => "é",
        "egrave" => "è",
        "agrave" => "à",
        "ccedil" => "ç",
        _ => return None,
    })
}

/// *Unescape XML or HTML*: references read, an HTML entity's name too.
fn unescape(s: &str) -> String {
    references(s, html_entity)
}

/// *Hex decode*: pairs of hex digits, with or without spaces, colons and line
/// breaks between them, and an `0x` before them.
fn hex_decode(input: &[u8]) -> Result<Vec<u8>, Failure> {
    let s = text(input)?.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    let digits: Vec<u8> = s
        .bytes()
        .filter(|b| !(b.is_ascii_whitespace() || *b == b':' || *b == b'-'))
        .collect();
    if !digits.len().is_multiple_of(2) {
        return Err(Failure::Error(format!(
            "not hex: {} digits, and hex comes in pairs",
            digits.len()
        )));
    }
    let value = |b: u8, at: usize| {
        char::from(b).to_digit(16).ok_or_else(|| {
            Failure::Error(format!("not hex: '{}' at digit {}", char::from(b), at + 1))
        })
    };
    let mut out = Vec::with_capacity(digits.len() / 2);
    for (i, pair) in digits.chunks(2).enumerate() {
        let high = value(pair[0], i * 2)?;
        let low = value(pair[1], i * 2 + 1)?;
        out.push(u8::try_from(high * 16 + low).unwrap_or(0));
    }
    Ok(out)
}

/// *Inflate*: zlib, then raw deflate where it is not zlib.
fn inflate(input: &[u8], out: &mut Capped<'_>) -> Result<(), Failure> {
    let zlib = input.len() >= 2
        && input[0] & 0x0f == 8
        && (u16::from(input[0]) << 8 | u16::from(input[1])) % 31 == 0;
    let read = if zlib {
        std::io::copy(&mut flate2::read::ZlibDecoder::new(input), out)
    } else {
        std::io::copy(&mut flate2::read::DeflateDecoder::new(input), out)
    };
    match read {
        Ok(_) => Ok(()),
        Err(error) => match io_failure(&error) {
            Failure::Error(said) => Err(Failure::Error(format!("not deflate data: {said}"))),
            other => Err(other),
        },
    }
}

/// *JSON string*: a JSON string's text, its escapes read. The quotes around it
/// may be there or not.
fn json_string(s: &str) -> Result<String, Failure> {
    let t = s.trim();
    let quoted = if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        t.to_owned()
    } else {
        format!("\"{t}\"")
    };
    serde_json::from_str::<String>(&quoted)
        .map_err(|error| Failure::Error(format!("not a JSON string: {error}")))
}

/// *JWT*: a token's header and payload as JSON, its times written out, and its
/// signature counted and said not to be checked: knobas has no key, and a tick
/// it cannot earn would be worse than none.
fn jwt(s: &str) -> Result<String, Failure> {
    let parts: Vec<&str> = s.trim().split('.').collect();
    if parts.len() != 3 && parts.len() != 2 {
        return Err(Failure::Error(format!(
            "not a JWT: it has {} parts, and a token has three, split by dots",
            parts.len()
        )));
    }
    let config =
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent);
    let url = GeneralPurpose::new(&base64::alphabet::URL_SAFE, config);
    let part = |at: usize, what: &str| -> Result<serde_json::Value, Failure> {
        let bytes = url
            .decode(parts[at])
            .map_err(|error| Failure::Error(format!("its {what} is not base64url: {error}")))?;
        serde_json::from_slice(&bytes)
            .map_err(|error| Failure::Error(format!("its {what} is not JSON: {error}")))
    };
    let header = part(0, "header")?;
    let payload = part(1, "payload")?;
    let mut times = serde_json::Map::new();
    for key in ["iat", "nbf", "exp", "auth_time"] {
        if let Some(at) = payload.get(key).and_then(serde_json::Value::as_i64)
            && let Some(when) = chrono::DateTime::from_timestamp(at, 0)
        {
            times.insert(
                key.to_owned(),
                serde_json::Value::String(when.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
            );
        }
    }
    let signature = parts
        .get(2)
        .map_or(0, |sig| url.decode(sig).map_or(0, |b| b.len()));
    let mut out = serde_json::Map::new();
    out.insert("header".to_owned(), header);
    out.insert("payload".to_owned(), payload);
    if !times.is_empty() {
        out.insert("times".to_owned(), serde_json::Value::Object(times));
    }
    out.insert(
        "signature".to_owned(),
        serde_json::Value::String(if signature == 0 {
            "none".to_owned()
        } else {
            format!("{signature} bytes, not checked: knobas has no key to check it with")
        }),
    );
    serde_json::to_string_pretty(&serde_json::Value::Object(out))
        .map(|mut s| {
            s.push('\n');
            s
        })
        .map_err(|error| Failure::Error(error.to_string()))
}

/// *Read as*: bytes in a character set, as UTF-8 text.
fn read_as(input: &[u8], charset: Charset) -> String {
    let encoding = match charset {
        Charset::Windows1252 => encoding_rs::WINDOWS_1252,
        Charset::Iso885915 => encoding_rs::ISO_8859_15,
        Charset::Utf16le => encoding_rs::UTF_16LE,
        Charset::Utf16be => encoding_rs::UTF_16BE,
    };
    let (text, _, _) = encoding.decode(input);
    text.into_owned()
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::sync::atomic::AtomicBool;

    use super::super::{Charset, PipeStep, TakeAs, steps};

    fn ran(step: &PipeStep, input: &[u8]) -> Result<Vec<u8>, String> {
        let stop = AtomicBool::new(false);
        steps::run(step, input, 1024 * 1024, &stop).map_err(|failure| match failure {
            steps::Failure::Error(said) => said,
            other => format!("{other:?}"),
        })
    }
    fn said(step: &PipeStep, input: &str) -> String {
        String::from_utf8(ran(step, input.as_bytes()).expect("the step ran")).expect("text")
    }
    fn take(from: u64, to: u64, read: TakeAs) -> PipeStep {
        PipeStep::Take { from, to, read }
    }

    #[test]
    fn take_cuts_the_bytes_and_reads_xml_text_with_its_cdata_as_written() {
        let file = "<a>x &lt; y<![CDATA[ & <raw> ]]>&#x41;</a>";
        assert_eq!(said(&take(3, 38, TakeAs::XmlText), file), "x < y & <raw> A");
        assert_eq!(said(&take(0, 3, TakeAs::Bytes), file), "<a>");
    }

    #[test]
    fn take_reads_an_attribute_and_a_json_string() {
        assert_eq!(
            said(
                &take(8, 32, TakeAs::XmlAttribute),
                r#"<a b = "say &quot;hi&quot; &amp;"/>"#
            ),
            "say \"hi\" &"
        );
        let json = r#"{"k": "ä\nb ü"}"#;
        assert_eq!(
            said(&take(6, json.len() as u64 - 1, TakeAs::JsonString), json),
            "ä\nb ü"
        );
    }

    #[test]
    fn take_refuses_a_range_past_the_end_and_reads_no_byte_past_to() {
        let error = ran(&take(2, 9, TakeAs::Bytes), b"abcdef").expect_err("past the end");
        assert!(error.contains("bytes 2 to 9"), "{error}");
        assert!(ran(&take(4, 2, TakeAs::Bytes), b"abcdef").is_err());
        assert_eq!(
            ran(&take(1, 3, TakeAs::Bytes), b"abcdef").expect("ran"),
            b"bc"
        );
        assert_eq!(
            ran(&take(6, 6, TakeAs::Bytes), b"abcdef").expect("ran"),
            b""
        );
    }

    #[test]
    fn take_counts_from_after_a_byte_order_mark() {
        assert_eq!(
            ran(&take(3, 4, TakeAs::Bytes), b"\xef\xbb\xbf<a>x</a>").expect("ran"),
            b"x"
        );
    }

    #[test]
    fn hex_decode_reads_pairs_with_spaces_and_colons() {
        assert_eq!(
            ran(&PipeStep::HexDecode, b"0x48 65:6c-6c\n6f").expect("ran"),
            b"Hello"
        );
        assert!(
            ran(&PipeStep::HexDecode, b"abc")
                .expect_err("odd")
                .contains("pairs")
        );
        assert!(
            ran(&PipeStep::HexDecode, b"zz")
                .expect_err("not hex")
                .contains("'z'")
        );
    }

    #[test]
    fn inflate_reads_zlib_and_then_raw_deflate() {
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(b"zlib text").expect("write");
        assert_eq!(
            ran(&PipeStep::Inflate, &z.finish().expect("zlib")).expect("ran"),
            b"zlib text"
        );
        let mut d = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        d.write_all(b"raw text").expect("write");
        assert_eq!(
            ran(&PipeStep::Inflate, &d.finish().expect("deflate")).expect("ran"),
            b"raw text"
        );
        assert!(ran(&PipeStep::Inflate, b"\xff\xfe nothing").is_err());
    }

    #[test]
    fn url_decode_unescape_and_json_string() {
        assert_eq!(said(&PipeStep::UrlDecode, "a%20b%2Fc+d"), "a b/c+d");
        assert_eq!(
            said(
                &PipeStep::Unescape,
                "&lt;p&gt;caf&eacute; &amp; &euro;5 &bogus;"
            ),
            "<p>café & €5 &bogus;"
        );
        assert_eq!(said(&PipeStep::JsonString, r#""{\"a\":1}""#), r#"{"a":1}"#);
        assert!(ran(&PipeStep::JsonString, br#""\q""#).is_err());
    }

    #[test]
    fn a_jwt_is_its_header_and_payload_with_its_times_and_its_signature_not_checked() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiI0NzExIiwiaWF0IjoxNzk1MDAwMDAwfQ.c2lnbmF0dXJlLWJ5dGVz";
        let out = said(&PipeStep::Jwt, token);
        let value: serde_json::Value = serde_json::from_str(&out).expect("JSON");
        assert_eq!(value["header"]["alg"], "HS256");
        assert_eq!(value["payload"]["sub"], "4711");
        assert_eq!(value["times"]["iat"], "2026-11-18 11:06:40 UTC");
        assert_eq!(
            value["signature"],
            "15 bytes, not checked: knobas has no key to check it with"
        );
        assert!(!out.contains("verified") && !out.contains("valid"));
        assert!(ran(&PipeStep::Jwt, b"one.two.three.four").is_err());
    }

    #[test]
    fn read_as_a_character_set() {
        assert_eq!(
            ran(
                &PipeStep::Charset {
                    charset: Charset::Windows1252
                },
                b"caf\xe9 \x80"
            )
            .expect("ran"),
            "café €".as_bytes()
        );
        assert_eq!(
            ran(
                &PipeStep::Charset {
                    charset: Charset::Iso885915
                },
                b"\xa4"
            )
            .expect("ran"),
            "€".as_bytes()
        );
        assert_eq!(
            ran(
                &PipeStep::Charset {
                    charset: Charset::Utf16le
                },
                b"h\x00i\x00"
            )
            .expect("ran"),
            b"hi"
        );
        assert_eq!(
            ran(
                &PipeStep::Charset {
                    charset: Charset::Utf16be
                },
                b"\x00h\x00i"
            )
            .expect("ran"),
            b"hi"
        );
    }
}
