//! A file's text as a view carries it (ADR-0020 rule 6): cut at whole lines
//! under a byte and a line cap, told binary by a NUL byte near its start, and
//! named a language by its name. One rule for every reader of a file: the
//! Gitea adapter's read of the source, and the app's read of a checkout
//! (ADR-0041), so a file looks the same from either.

/// The most of a file a view carries, in bytes.
pub const BYTE_CAP: usize = 1_000_000;
/// The most of a file a view carries, in lines.
pub const LINE_CAP: usize = 10_000;
/// How far into a file a NUL byte makes it binary, as git looks.
pub const SNIFF: usize = 8_000;

/// Whether `bytes` are a binary file's: a NUL byte in the first [`SNIFF`].
#[must_use]
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(SNIFF).any(|b| *b == 0)
}

/// A file's text within both caps, and whether either cut it. `cut` says the
/// read already stopped past [`BYTE_CAP`].
#[must_use]
pub fn capped(bytes: &[u8], cut: bool) -> (String, bool) {
    let mut kept = bytes;
    let mut truncated = cut || bytes.len() > BYTE_CAP;
    if truncated {
        let head = &bytes[..BYTE_CAP.min(bytes.len())];
        // At the last whole line; a cap inside the first line keeps it cut.
        kept = head
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(head, |at| &head[..=at]);
    }
    if let Some(end) = kept
        .iter()
        .enumerate()
        .filter(|(_, b)| **b == b'\n')
        .nth(LINE_CAP - 1)
        .map(|(at, _)| at)
        && end + 1 < kept.len()
    {
        kept = &kept[..=end];
        truncated = true;
    }
    (String::from_utf8_lossy(kept).into_owned(), truncated)
}

/// The language a file's name says it is in, where it is one a reader would
/// name; `None` otherwise. A hint only: nothing is highlighted (#701).
#[must_use]
pub fn language_of(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    match name.as_str() {
        "dockerfile" => return Some("dockerfile"),
        "makefile" | "gnumakefile" => return Some("makefile"),
        "justfile" => return Some("just"),
        _ => {}
    }
    let (_, ext) = name.rsplit_once('.')?;
    Some(match ext {
        "rs" => "rust",
        "md" | "markdown" | "mdown" | "mkd" => "markdown",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "svelte" => "svelte",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "rb" => "ruby",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        "cs" => "csharp",
        "swift" => "swift",
        "sh" | "bash" | "zsh" => "shell",
        "sql" => "sql",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "gradle" => "gradle",
        "tf" => "terraform",
        "proto" => "protobuf",
        "txt" => "text",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_cut_at_whole_lines() {
        assert_eq!(capped(b"a\nb\n", false), ("a\nb\n".to_owned(), false));
        assert_eq!(
            capped(b"no newline", false),
            ("no newline".to_owned(), false)
        );
        // A read cut past the cap keeps the lines that fit.
        let mut long = b"one\n".to_vec();
        long.extend(std::iter::repeat_n(b'x', BYTE_CAP + 5));
        assert_eq!(capped(&long, true), ("one\n".to_owned(), true));
    }

    #[test]
    fn languages_are_named_by_extension_or_name() {
        assert_eq!(language_of("src/lib.rs"), Some("rust"));
        assert_eq!(language_of("README.MD"), Some("markdown"));
        assert_eq!(language_of("build/Dockerfile"), Some("dockerfile"));
        assert_eq!(language_of("LICENSE"), None);
        assert_eq!(language_of("a.unknownext"), None);
    }

    #[test]
    fn a_nul_near_the_start_is_binary() {
        assert!(is_binary(b"PNG\0..."));
        assert!(!is_binary(b"plain text"));
    }
}
