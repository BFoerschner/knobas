//! Where a source's own website shows one item (ux30, Fable's ruling while
//! Björn was busy, 2026-11-22), declared by the adapter so knobas never
//! learns which product it is.
//!
//! The Passwords page opens a password manager's own web interface on the
//! entry or folder it shows, and the role names no vendor (Björn's decision 2).
//! So the address is not an item's stored `web_url`, which every generic
//! *Open in ⟨source⟩* would offer: a password item keeps `web_url: None`. It
//! is built the way [`crate::code_pages`] builds a code host's page: the
//! instance's **typed base URL**, then a template the adapter declares here,
//! with the item's key percent-encoded in its place. A template cannot name
//! another host, and an address already mirrored gets its link without a new
//! sync.
//!
//! - [`WebLinks::pages`]: a template per entity kind the descriptor declares,
//!   holding `{key}` once: the entity's key at the source
//!   ([`EntityRef::key`](knobas_spi_types::entity::EntityRef)).
//! - [`WebLinks::home`]: the web interface's own front page, for a press with
//!   no item, when the base URL alone is not it.
//!
//! A template starts with a single `/`, may carry a query and a fragment (a
//! single-page vault routes by its fragment, `/#/vault?itemId={key}`), and
//! holds no `.` or `..` segment, backslash, space or control character.
//! [`check`] holds a declaration to these rules (contract battery clause 18).
//! An external adapter declares the same field in its descriptor's JSON, read
//! from `knobas:adapter@1.7.0` on.

use std::collections::BTreeMap;

/// The placeholder a page template carries once.
pub const KEY: &str = "{key}";

/// Where the site shows an item of each kind, and its front page.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct WebLinks {
    /// The front page, after the base URL. `None`: the base URL is it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub home: Option<String>,
    /// Kind id to the template of its page, after the base URL.
    #[serde(default)]
    pub pages: BTreeMap<String, String>,
}

impl WebLinks {
    /// The template of `kind`'s page, if declared.
    #[must_use]
    pub fn page(&self, kind: &str) -> Option<&str> {
        self.pages.get(kind).map(String::as_str)
    }
}

/// Why a declaration is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebLinksError {
    #[error("declares no page and no home")]
    Empty,
    #[error("a page for kind {0:?}, which the descriptor does not declare")]
    UnknownKind(String),
    #[error("template {0:?} does not start with a single `/`")]
    Relative(String),
    #[error("template {0:?} holds a backslash, a space or a control character")]
    Odd(String),
    #[error("template {0:?} has a `.` or `..` segment")]
    Dots(String),
    #[error("page template {0:?} does not hold `{{key}}` exactly once")]
    Key(String),
    #[error("home {0:?} holds `{{` or `}}`")]
    HomeBraces(String),
}

fn shape(template: &str) -> Result<(), WebLinksError> {
    if !template.starts_with('/') || template.starts_with("//") {
        return Err(WebLinksError::Relative(template.to_owned()));
    }
    if template
        .chars()
        .any(|ch| ch == '\\' || ch.is_whitespace() || ch.is_control())
    {
        return Err(WebLinksError::Odd(template.to_owned()));
    }
    let path = template.split(['?', '#']).next().unwrap_or_default();
    if path
        .split('/')
        .any(|segment| segment == "." || segment == "..")
    {
        return Err(WebLinksError::Dots(template.to_owned()));
    }
    Ok(())
}

/// A declaration's rules, against the kinds its descriptor declares.
///
/// # Errors
///
/// The first rule a template breaks.
pub fn check(links: &WebLinks, kinds: &[&str]) -> Result<(), WebLinksError> {
    if links.pages.is_empty() && links.home.is_none() {
        return Err(WebLinksError::Empty);
    }
    if let Some(home) = &links.home {
        shape(home)?;
        if home.contains(['{', '}']) {
            return Err(WebLinksError::HomeBraces(home.clone()));
        }
    }
    for (kind, template) in &links.pages {
        if !kinds.contains(&kind.as_str()) {
            return Err(WebLinksError::UnknownKind(kind.clone()));
        }
        shape(template)?;
        let rest = template.replacen(KEY, "", 1);
        if rest.len() == template.len() || rest.contains(['{', '}']) {
            return Err(WebLinksError::Key(template.clone()));
        }
    }
    Ok(())
}

/// `base_url` (trailing `/` dropped), then `template` with `{key}` replaced
/// by `key`, percent-encoded: every byte but the unreserved ones, so a key
/// cannot leave its place in the path or the query. `None` for an empty key
/// or a template without `{key}`.
#[must_use]
pub fn page(base_url: &str, template: &str, key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    let (before, after) = template.split_once(KEY)?;
    let mut out = base_url.trim_end_matches('/').to_owned();
    out.push_str(before);
    for byte in key.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out.push_str(after);
    Some(out)
}

/// The front page: `base_url` then `home`, or the base URL alone.
#[must_use]
pub fn home(base_url: &str, links: Option<&WebLinks>) -> String {
    match links.and_then(|links| links.home.as_deref()) {
        Some(home) => format!("{}{home}", base_url.trim_end_matches('/')),
        None => base_url.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(pages: &[(&str, &str)], home: Option<&str>) -> WebLinks {
        WebLinks {
            home: home.map(str::to_owned),
            pages: pages
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        }
    }

    const KINDS: &[&str] = &["password_entry", "password_folder"];

    #[test]
    fn the_declared_shapes_pass() {
        for declared in [
            links(
                &[
                    (
                        "password_entry",
                        "/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain%3FitemId%3D{key}",
                    ),
                    (
                        "password_folder",
                        "/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain%3FitemId%3D{key}",
                    ),
                ],
                Some("/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain"),
            ),
            links(&[("password_entry", "/#/vault?itemId={key}")], None),
            links(&[], Some("/")),
        ] {
            assert_eq!(check(&declared, KINDS), Ok(()), "{declared:?}");
        }
    }

    #[test]
    fn a_bad_declaration_is_refused() {
        assert_eq!(
            check(&WebLinks::default(), KINDS),
            Err(WebLinksError::Empty)
        );
        let one = |template: &str| check(&links(&[("password_entry", template)], None), KINDS);
        assert_eq!(
            one("x/{key}"),
            Err(WebLinksError::Relative("x/{key}".into()))
        );
        assert_eq!(
            one("//evil.example/{key}"),
            Err(WebLinksError::Relative("//evil.example/{key}".into()))
        );
        assert_eq!(
            one("/a b/{key}"),
            Err(WebLinksError::Odd("/a b/{key}".into()))
        );
        assert_eq!(
            one("/a\\{key}"),
            Err(WebLinksError::Odd("/a\\{key}".into()))
        );
        assert_eq!(
            one("/a/\n{key}"),
            Err(WebLinksError::Odd("/a/\n{key}".into()))
        );
        assert_eq!(
            one("/../{key}"),
            Err(WebLinksError::Dots("/../{key}".into()))
        );
        assert_eq!(
            one("/a/./{key}"),
            Err(WebLinksError::Dots("/a/./{key}".into()))
        );
        // A `..` after the path is a value, not a segment.
        assert_eq!(one("/a?next=..&id={key}"), Ok(()));
        assert_eq!(one("/a"), Err(WebLinksError::Key("/a".into())));
        assert_eq!(
            one("/{key}/{key}"),
            Err(WebLinksError::Key("/{key}/{key}".into()))
        );
        assert_eq!(
            one("/{key}/{id}"),
            Err(WebLinksError::Key("/{key}/{id}".into()))
        );
        assert_eq!(
            check(&links(&[("survey", "/{key}")], None), KINDS),
            Err(WebLinksError::UnknownKind("survey".into()))
        );
        assert_eq!(
            check(&links(&[], Some("/x/{key}")), KINDS),
            Err(WebLinksError::HomeBraces("/x/{key}".into()))
        );
        assert_eq!(
            check(&links(&[], Some("home")), KINDS),
            Err(WebLinksError::Relative("home".into()))
        );
    }

    #[test]
    fn a_page_is_the_base_url_then_the_template_with_the_key_encoded() {
        assert_eq!(
            page(
                "https://pw.example:10001/",
                "/WebClient/Main?itemId={key}",
                "07c469c8-e880-4927-8c31-a8342cd66d98"
            )
            .as_deref(),
            Some(
                "https://pw.example:10001/WebClient/Main?itemId=07c469c8-e880-4927-8c31-a8342cd66d98"
            )
        );
        assert_eq!(
            page(
                "https://v.example",
                "/#/vault?itemId={key}&x=1",
                "a/b?c&d#e f"
            )
            .as_deref(),
            Some("https://v.example/#/vault?itemId=a%2Fb%3Fc%26d%23e%20f&x=1")
        );
        assert_eq!(
            page("https://v.example", "/{key}", "..").as_deref(),
            Some("https://v.example/..")
        );
        assert_eq!(page("https://v.example", "/{key}", ""), None);
        assert_eq!(page("https://v.example", "/no-key", "k"), None);
    }

    #[test]
    fn home_is_the_declared_page_or_the_base_url() {
        let declared = links(&[], Some("/SignIn/Direct"));
        assert_eq!(
            home("https://pw.example/", Some(&declared)),
            "https://pw.example/SignIn/Direct"
        );
        assert_eq!(
            home("https://pw.example/", Some(&WebLinks::default())),
            "https://pw.example/"
        );
        assert_eq!(home("https://pw.example", None), "https://pw.example");
    }

    #[test]
    fn it_reads_from_json() {
        let read: WebLinks =
            serde_json::from_str(r#"{"pages":{"password_entry":"/#/vault?itemId={key}"}}"#)
                .unwrap();
        assert_eq!(
            read,
            links(&[("password_entry", "/#/vault?itemId={key}")], None)
        );
        let read: WebLinks = serde_json::from_str(r#"{"home":"/vault"}"#).unwrap();
        assert_eq!(read.home.as_deref(), Some("/vault"));
    }
}
