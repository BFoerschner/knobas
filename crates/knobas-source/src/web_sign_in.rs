//! Where a source's own website asks for a sign-in (ADR-0050, ux29), declared
//! by the adapter so knobas never learns which product it is.
//!
//! A source's site opens inside knobas, and a username and password saved for
//! it in Settings is typed into the site's own sign-in form: on a press of
//! *Fill sign-in*, and by itself once a page the adapter declares here has
//! loaded on the source's own origin. Two ways to say where that form is:
//!
//! - [`WebSignInPages::paths`]: the sign-in page's path after the instance's
//!   base URL. Jira Data Center's `/login.jsp`, Confluence Data Center's
//!   `/login.action`, Gitea's and Forgejo's `/user/login`, TeamCity's
//!   `/login.html`.
//! - [`WebSignInPages::any_page`]: the form can stand on any page of the site,
//!   as a single-page application's sign-in dialog does (Uptime Kuma). knobas
//!   then looks for a sign-in form on each page of the source's own origin
//!   that loads, a few times while it renders.
//!
//! An external adapter declares the same field in its descriptor's JSON; one
//! that declares nothing gets the button alone. [`check`] holds a declaration
//! to these rules (contract battery clause 16).

/// Where the site shows its sign-in form.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "sources.ts")]
pub struct WebSignInPages {
    /// Paths after the base URL, each starting with `/`, with no query, no
    /// fragment and no `.` or `..` segment.
    #[serde(default)]
    pub paths: Vec<String>,
    /// The form can stand on any page of the site's own origin.
    #[serde(default)]
    pub any_page: bool,
}

impl WebSignInPages {
    /// Declared at `paths`.
    #[must_use]
    pub fn at(paths: &[&str]) -> Self {
        Self {
            paths: paths.iter().map(|path| (*path).to_owned()).collect(),
            any_page: false,
        }
    }

    /// The form can stand on any page.
    #[must_use]
    pub fn any_page() -> Self {
        Self {
            paths: Vec::new(),
            any_page: true,
        }
    }
}

/// Why a declaration is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebSignInError {
    #[error("declares no sign-in path and not `any_page`")]
    Empty,
    #[error("path {0:?} does not start with `/`")]
    Relative(String),
    #[error("path {0:?} holds a query, a fragment, a backslash or a space")]
    Odd(String),
    #[error("path {0:?} has a `.` or `..` segment")]
    Dots(String),
    #[error("path {0:?} is declared twice")]
    Twice(String),
}

/// A declaration's rules.
///
/// # Errors
///
/// The first rule a path breaks.
pub fn check(pages: &WebSignInPages) -> Result<(), WebSignInError> {
    if pages.paths.is_empty() && !pages.any_page {
        return Err(WebSignInError::Empty);
    }
    for (index, path) in pages.paths.iter().enumerate() {
        if !path.starts_with('/') {
            return Err(WebSignInError::Relative(path.clone()));
        }
        if path
            .chars()
            .any(|ch| matches!(ch, '?' | '#' | '\\') || ch.is_whitespace() || ch.is_control())
        {
            return Err(WebSignInError::Odd(path.clone()));
        }
        if path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
        {
            return Err(WebSignInError::Dots(path.clone()));
        }
        if pages.paths[..index].contains(path) {
            return Err(WebSignInError::Twice(path.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_shapes_pass() {
        for pages in [
            WebSignInPages::at(&["/login.jsp"]),
            WebSignInPages::at(&["/login.action"]),
            WebSignInPages::at(&["/user/login"]),
            WebSignInPages::at(&["/login.html"]),
            WebSignInPages::any_page(),
        ] {
            assert_eq!(check(&pages), Ok(()), "{pages:?}");
        }
    }

    #[test]
    fn a_bad_path_is_refused() {
        assert_eq!(
            check(&WebSignInPages::default()),
            Err(WebSignInError::Empty)
        );
        for (path, want) in [
            ("login.jsp", WebSignInError::Relative("login.jsp".into())),
            ("/login?x=1", WebSignInError::Odd("/login?x=1".into())),
            ("/login#a", WebSignInError::Odd("/login#a".into())),
            ("/a b", WebSignInError::Odd("/a b".into())),
            ("/a\\b", WebSignInError::Odd("/a\\b".into())),
            ("/../login", WebSignInError::Dots("/../login".into())),
            ("/./login", WebSignInError::Dots("/./login".into())),
        ] {
            assert_eq!(check(&WebSignInPages::at(&[path])), Err(want), "{path}");
        }
        assert_eq!(
            check(&WebSignInPages::at(&["/l", "/l"])),
            Err(WebSignInError::Twice("/l".into()))
        );
    }

    /// An external adapter's descriptor JSON declares it as a built-in's
    /// does, and one that predates it decodes as declaring nothing.
    #[test]
    fn it_reads_from_json() {
        let pages: WebSignInPages = serde_json::from_str(r#"{"paths":["/signin"]}"#).unwrap();
        assert_eq!(pages, WebSignInPages::at(&["/signin"]));
        let pages: WebSignInPages = serde_json::from_str(r#"{"any_page":true}"#).unwrap();
        assert!(pages.any_page && pages.paths.is_empty());
    }
}
