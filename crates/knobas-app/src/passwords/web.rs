//! *Open web interface* (ux30, Fable's ruling while Björn was busy,
//! 2026-11-22): the address of a password source's own web interface on an
//! entry, a folder or its front page.
//!
//! Built from the source's typed base URL and its adapter's declared
//! [`WebLinks`], never from a mirrored value, so it can only name the
//! source's own site. The page asks at press time; the fallback is the
//! entry's page, else its folder's, else the front page.

use knobas_source::passwords::{ENTRY_KIND, FOLDER_KIND};
use knobas_source::web_links::{self, WebLinks};
use serde::Serialize;
use ts_rs::TS;

/// Which page an address opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "passwords.ts")]
pub enum WebAt {
    Entry,
    Folder,
    Home,
}

/// An address of the source's web interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "passwords.ts")]
pub struct WebLink {
    pub url: String,
    pub at: WebAt,
}

/// Which item pages a source's adapter declares, for the page's words.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "passwords.ts")]
pub struct WebPages {
    pub entry: bool,
    pub folder: bool,
}

impl WebPages {
    /// From the adapter's declaration.
    #[must_use]
    pub fn of(links: Option<&WebLinks>) -> Self {
        Self {
            entry: links.is_some_and(|l| l.page(ENTRY_KIND).is_some()),
            folder: links.is_some_and(|l| l.page(FOLDER_KIND).is_some()),
        }
    }
}

/// What the press was on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'a> {
    /// An entry, by its key at the source, and its folder's key.
    Entry {
        key: &'a str,
        folder: Option<&'a str>,
    },
    /// A folder, by its key at the source.
    Folder { key: &'a str },
    /// The source itself.
    Home,
}

/// Whether `base_url` is an http(s) address a site or a browser can open.
fn is_web(base_url: &str) -> bool {
    url::Url::parse(base_url)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some())
}

/// The address for `target`: its own page where declared, else its folder's,
/// else the front page. `None` when the base URL is not an http(s) address.
#[must_use]
pub fn web_link(base_url: &str, links: Option<&WebLinks>, target: Target<'_>) -> Option<WebLink> {
    if !is_web(base_url) {
        return None;
    }
    let page = |kind: &str, key: &str| {
        links
            .and_then(|l| l.page(kind))
            .and_then(|template| web_links::page(base_url, template, key))
    };
    let (entry, folder) = match target {
        Target::Entry { key, folder } => (Some(key), folder),
        Target::Folder { key } => (None, Some(key)),
        Target::Home => (None, None),
    };
    if let Some(url) = entry.and_then(|key| page(ENTRY_KIND, key)) {
        return Some(WebLink {
            url,
            at: WebAt::Entry,
        });
    }
    if let Some(url) = folder.and_then(|key| page(FOLDER_KIND, key)) {
        return Some(WebLink {
            url,
            at: WebAt::Folder,
        });
    }
    Some(WebLink {
        url: web_links::home(base_url, links),
        at: WebAt::Home,
    })
}

/// The key at the source of an entity id of `source` (`<source>:<key>`).
#[must_use]
pub fn key_of<'a>(source: &str, entity_id: &'a str) -> Option<&'a str> {
    entity_id
        .strip_prefix(source)
        .and_then(|rest| rest.strip_prefix(':'))
        .filter(|key| !key.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://pw.example:10001";
    const ENTRY: &str = "/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain%3FitemId%3D{key}";

    fn pleasant() -> WebLinks {
        WebLinks {
            home: Some("/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain".into()),
            pages: [
                (ENTRY_KIND.to_owned(), ENTRY.to_owned()),
                (FOLDER_KIND.to_owned(), ENTRY.to_owned()),
            ]
            .into(),
        }
    }

    fn only(kind: &str) -> WebLinks {
        WebLinks {
            home: None,
            pages: [(kind.to_owned(), "/#/vault?id={key}".to_owned())].into(),
        }
    }

    #[test]
    fn an_entry_opens_its_own_page() {
        let link = web_link(
            BASE,
            Some(&pleasant()),
            Target::Entry {
                key: "e1",
                folder: Some("f1"),
            },
        )
        .unwrap();
        assert_eq!(link.at, WebAt::Entry);
        assert_eq!(
            link.url,
            "https://pw.example:10001/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain%3FitemId%3De1"
        );
    }

    #[test]
    fn without_an_entry_page_an_entry_opens_its_folder_then_home() {
        let folders = only(FOLDER_KIND);
        let link = web_link(
            BASE,
            Some(&folders),
            Target::Entry {
                key: "e1",
                folder: Some("f1"),
            },
        )
        .unwrap();
        assert_eq!(
            (link.at, link.url.as_str()),
            (WebAt::Folder, "https://pw.example:10001/#/vault?id=f1")
        );
        let link = web_link(
            BASE,
            Some(&folders),
            Target::Entry {
                key: "e1",
                folder: None,
            },
        )
        .unwrap();
        assert_eq!((link.at, link.url.as_str()), (WebAt::Home, BASE));
        let link = web_link(
            BASE,
            None,
            Target::Entry {
                key: "e1",
                folder: Some("f1"),
            },
        )
        .unwrap();
        assert_eq!((link.at, link.url.as_str()), (WebAt::Home, BASE));
    }

    #[test]
    fn a_folder_opens_its_page_and_never_an_entry_template() {
        let entries = only(ENTRY_KIND);
        let link = web_link(BASE, Some(&entries), Target::Folder { key: "f1" }).unwrap();
        assert_eq!((link.at, link.url.as_str()), (WebAt::Home, BASE));
        let link = web_link(BASE, Some(&pleasant()), Target::Folder { key: "f1" }).unwrap();
        assert_eq!(link.at, WebAt::Folder);
        assert!(link.url.ends_with("itemId%3Df1"), "{}", link.url);
    }

    #[test]
    fn the_source_opens_its_front_page() {
        let link = web_link(BASE, Some(&pleasant()), Target::Home).unwrap();
        assert_eq!(
            (link.at, link.url.as_str()),
            (
                WebAt::Home,
                "https://pw.example:10001/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain"
            )
        );
        let link = web_link("http://pw.example/", Some(&only(ENTRY_KIND)), Target::Home).unwrap();
        assert_eq!(link.url, "http://pw.example/");
    }

    #[test]
    fn a_base_url_that_is_no_web_address_opens_nothing() {
        for base in [
            "",
            "pw.example",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://",
        ] {
            assert_eq!(
                web_link(base, Some(&pleasant()), Target::Home),
                None,
                "{base:?}"
            );
        }
    }

    #[test]
    fn a_key_is_the_entity_id_after_its_source() {
        assert_eq!(key_of("pw", "pw:07c4-e8"), Some("07c4-e8"));
        assert_eq!(key_of("pw", "pw:a:b"), Some("a:b"));
        assert_eq!(key_of("pw", "pwx:a"), None);
        assert_eq!(key_of("pw", "pw:"), None);
        assert_eq!(key_of("pw", "other:a"), None);
    }

    #[test]
    fn the_declared_pages_are_said() {
        assert_eq!(
            WebPages::of(Some(&pleasant())),
            WebPages {
                entry: true,
                folder: true
            }
        );
        assert_eq!(
            WebPages::of(Some(&only(FOLDER_KIND))),
            WebPages {
                entry: false,
                folder: true
            }
        );
        assert_eq!(WebPages::of(None), WebPages::default());
    }
}
