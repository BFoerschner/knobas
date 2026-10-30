//! Where a code host serves the page of a repository, a file, a commit, a
//! range and a pull request (#2274, spec #2234 step 6c), declared by the
//! adapter so the app never learns which host it is.
//!
//! *Open in › Browser* and the `{url}` placeholder of an *Open in*
//! application open one of these pages. ADR-0016 keeps a mirrored value out
//! of every spawned command, and the opener takes only `http` and `https`; so
//! the address is never an item's stored `web_url`. It is built the way
//! ADR-0023 builds a clone URL: the instance's **typed base URL**, then the
//! template the adapter declares here, then the values, each percent-encoded
//! and none with a `.` or `..` segment, so none of them can leave its place
//! in the path.
//!
//! A template is the address **after the base URL**, starting with `/`, with
//! these placeholders:
//!
//! | placeholder | is | in |
//! |---|---|---|
//! | `{repo}` | the repository's `owner/name`, checked by the app | every page |
//! | `{ref}` | the commit or branch the file is read at | `file` |
//! | `{file}` | the file's path in the repository, `/` kept | `file` |
//! | `{line}` | the line number | `file` |
//! | `{sha}` | a commit | `commit` |
//! | `{base}`, `{head}` | the two ends, a commit or a branch | `range`, `pull_request` |
//! | `{pr}` | the pull request's number | `pull_request` |
//!
//! [`check`] holds a declaration to that table (contract battery clause 14).

/// The pages one code host serves. Gitea's: `/{repo}`,
/// `/{repo}/src/{ref}/{file}#L{line}`, `/{repo}/commit/{sha}`,
/// `/{repo}/compare/{base}...{head}`, `/{repo}/pulls/{pr}`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct CodePages {
    pub repository: String,
    pub file: String,
    pub commit: String,
    pub range: String,
    pub pull_request: String,
}

/// Which page, and so which placeholders it may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Repository,
    File,
    Commit,
    Range,
    PullRequest,
}

impl Page {
    /// The placeholders a template of this page may carry.
    #[must_use]
    pub const fn placeholders(self) -> &'static [&'static str] {
        match self {
            Self::Repository => &["repo"],
            Self::File => &["repo", "ref", "file", "line"],
            Self::Commit => &["repo", "sha"],
            Self::Range => &["repo", "base", "head"],
            Self::PullRequest => &["repo", "base", "head", "pr"],
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::File => "file",
            Self::Commit => "commit",
            Self::Range => "range",
            Self::PullRequest => "pull_request",
        }
    }
}

impl CodePages {
    /// The template of `page`.
    #[must_use]
    pub fn template(&self, page: Page) -> &str {
        match page {
            Page::Repository => &self.repository,
            Page::File => &self.file,
            Page::Commit => &self.commit,
            Page::Range => &self.range,
            Page::PullRequest => &self.pull_request,
        }
    }
}

const ALL: [Page; 5] = [
    Page::Repository,
    Page::File,
    Page::Commit,
    Page::Range,
    Page::PullRequest,
];

/// Whether every template starts with `/`, closes each `{`, and carries only
/// its page's placeholders.
///
/// # Errors
///
/// A sentence naming the page and what is wrong with it.
pub fn check(pages: &CodePages) -> Result<(), String> {
    for page in ALL {
        let template = pages.template(page);
        if !template.starts_with('/') {
            return Err(format!(
                "the {} page '{template}' does not start with '/'",
                page.name()
            ));
        }
        for name in names(template).map_err(|()| {
            format!(
                "the {} page '{template}' has a '{{' that is never closed",
                page.name()
            )
        })? {
            if !page.placeholders().contains(&name) {
                return Err(format!(
                    "the {} page '{template}' carries {{{name}}}, which a {} page does not have",
                    page.name(),
                    page.name()
                ));
            }
        }
    }
    Ok(())
}

fn names(template: &str) -> Result<Vec<&str>, ()> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let close = after.find('}').ok_or(())?;
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    Ok(out)
}

/// `base_url` (a trailing `/` dropped), then `template` with each
/// placeholder replaced by its value from `values`, percent-encoded: every
/// byte but the unreserved ones and `/` (so `{file}` and a branch named
/// `feature/x` keep their segments). `None` when the template names a value
/// `values` does not hold, or a value has a `.` or `..` segment: with `/`
/// kept, such a value would climb out of its place in the path, and a
/// browser reads `%2E%2E` as `..` too, so encoding it would not help.
#[must_use]
pub fn page(base_url: &str, template: &str, values: &[(&str, &str)]) -> Option<String> {
    let mut out = base_url.trim_end_matches('/').to_owned();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after.find('}')?;
        let name = &after[..close];
        let value = values.iter().find(|(key, _)| *key == name)?.1;
        if value.split('/').any(|segment| segment == "." || segment == "..") {
            return None;
        }
        encode_into(&mut out, value);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

fn encode_into(out: &mut String, value: &str) {
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gitea() -> CodePages {
        CodePages {
            repository: "/{repo}".into(),
            file: "/{repo}/src/{ref}/{file}#L{line}".into(),
            commit: "/{repo}/commit/{sha}".into(),
            range: "/{repo}/compare/{base}...{head}".into(),
            pull_request: "/{repo}/pulls/{pr}".into(),
        }
    }

    #[test]
    fn a_page_is_the_base_url_then_the_template_with_its_values_encoded() {
        let pages = gitea();
        assert_eq!(
            page(
                "https://git.example/",
                pages.template(Page::Range),
                &[
                    ("repo", "tidewater/payout-service"),
                    ("base", "c90d11e4^"),
                    ("head", "feature/PAY-231 x?#")
                ]
            )
            .as_deref(),
            Some(
                "https://git.example/tidewater/payout-service/compare/c90d11e4%5E...feature/PAY-231%20x%3F%23"
            )
        );
        assert_eq!(
            page(
                "https://git.example",
                pages.template(Page::File),
                &[
                    ("repo", "o/r"),
                    ("ref", "abc1234"),
                    ("file", "src/a b.rs"),
                    ("line", "42")
                ]
            )
            .as_deref(),
            Some("https://git.example/o/r/src/abc1234/src/a%20b.rs#L42")
        );
        assert_eq!(page("https://g", "/{repo}/{sha}", &[("repo", "o/r")]), None);
    }

    /// A value keeps its `/`, so one with a dot segment could climb out of
    /// its place in the path: it has no page.
    #[test]
    fn a_value_with_a_dot_segment_has_no_page() {
        let pages = gitea();
        for head in ["../../admin", "a/../b", "..", "a/.", "./a"] {
            assert_eq!(
                page(
                    "https://git.example",
                    pages.template(Page::Range),
                    &[("repo", "o/r"), ("base", "main"), ("head", head)]
                ),
                None,
                "{head}"
            );
        }
        assert!(
            page(
                "https://git.example",
                pages.template(Page::Range),
                &[("repo", "o/r"), ("base", "main"), ("head", "a/.b/c..d")]
            )
            .is_some()
        );
    }

    #[test]
    fn a_declaration_carries_only_its_pages_placeholders() {
        assert_eq!(check(&gitea()), Ok(()));
        let mut wrong = gitea();
        wrong.commit = "/{repo}/commit/{pr}".into();
        assert!(check(&wrong).unwrap_err().contains("{pr}"));
        let mut relative = gitea();
        relative.repository = "{repo}".into();
        assert!(check(&relative).unwrap_err().contains("start with '/'"));
        let mut open = gitea();
        open.range = "/{repo/compare".into();
        assert!(check(&open).is_err());
    }
}
