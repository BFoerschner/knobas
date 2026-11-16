//! A path as git prints it in a patch's header (`quote.c`'s
//! `quote_c_style`, with `core.quotePath` on, the default): as it is unless
//! it holds a control character, a double quote, a backslash or a byte past
//! ASCII, and then in double quotes with those escaped.

/// Whether `path` must be quoted.
fn needs(path: &[u8], high: bool) -> bool {
    path.iter()
        .any(|&b| b < 0x20 || b == b'"' || b == b'\\' || b == 0x7f || (high && b >= 0x80))
}

fn escape(out: &mut Vec<u8>, path: &[u8], high: bool) {
    for &b in path {
        match b {
            0x07 => out.extend_from_slice(b"\\a"),
            0x08 => out.extend_from_slice(b"\\b"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\n' => out.extend_from_slice(b"\\n"),
            0x0b => out.extend_from_slice(b"\\v"),
            0x0c => out.extend_from_slice(b"\\f"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b if b < 0x20 || b == 0x7f || (high && b >= 0x80) => {
                out.extend_from_slice(format!("\\{b:03o}").as_bytes());
            }
            b => out.push(b),
        }
    }
}

/// `prefix` and `path` as one name (`a/src/lib.rs`), quoted as a whole when
/// the path needs it (`quote_two_c_style`). `high`: bytes past ASCII are
/// escaped too (`core.quotePath`).
pub(crate) fn two(prefix: &str, path: &[u8], high: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(prefix.len() + path.len() + 2);
    if needs(path, high) {
        out.push(b'"');
        out.extend_from_slice(prefix.as_bytes());
        escape(&mut out, path, high);
        out.push(b'"');
    } else {
        out.extend_from_slice(prefix.as_bytes());
        out.extend_from_slice(path);
    }
    out
}

/// `path` alone, as `rename from` and `rename to` print it.
pub(crate) fn one(path: &[u8], high: bool) -> Vec<u8> {
    two("", path, high)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_quoted_only_where_git_quotes_it() {
        assert_eq!(two("a/", b"src/lib.rs", true), b"a/src/lib.rs");
        assert_eq!(two("a/", b"with space", true), b"a/with space");
        assert_eq!(two("a/", b"tab\there", true), b"\"a/tab\\there\"");
        assert_eq!(two("b/", "ü.txt".as_bytes(), true), b"\"b/\\303\\274.txt\"");
        assert_eq!(two("b/", "ü.txt".as_bytes(), false), "b/ü.txt".as_bytes());
        assert_eq!(one(b"q\"uote\\", true), b"\"q\\\"uote\\\\\"");
        assert_eq!(one(b"\x01", true), b"\"\\001\"");
    }
}
