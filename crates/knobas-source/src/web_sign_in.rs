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
//! [`WebSignInPages::uses_credential`] says the site's sign-in is the source's
//! own credential (ux30): a password manager's web interface signs in with the
//! very account its API does, so knobas fills that form with the username and
//! password the source already keeps, and nothing new is saved. Only with
//! declared `paths`, never `any_page`, where a form in other people's text
//! could collect it.
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
    /// The site signs in with the source's own username and password, so
    /// that credential is filled unless another sign-in is saved for the site
    /// or the person turned the fill off (ux30).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(as = "Option<bool>", optional)]
    pub uses_credential: bool,
}

impl WebSignInPages {
    /// Declared at `paths`.
    #[must_use]
    pub fn at(paths: &[&str]) -> Self {
        Self {
            paths: paths.iter().map(|path| (*path).to_owned()).collect(),
            any_page: false,
            uses_credential: false,
        }
    }

    /// The same pages, signed in with the source's own credential.
    #[must_use]
    pub fn with_credential(self) -> Self {
        Self {
            uses_credential: true,
            ..self
        }
    }

    /// The form can stand on any page.
    #[must_use]
    pub fn any_page() -> Self {
        Self {
            paths: Vec::new(),
            any_page: true,
            uses_credential: false,
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
    #[error("`uses_credential` needs declared paths and not `any_page`")]
    CredentialAnywhere,
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
    if pages.uses_credential && (pages.any_page || pages.paths.is_empty()) {
        return Err(WebSignInError::CredentialAnywhere);
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

    /// The source's own credential is filled only on declared pages.
    #[test]
    fn the_credential_is_declared_with_paths_alone() {
        assert_eq!(
            check(&WebSignInPages::at(&["/SignIn/Direct"]).with_credential()),
            Ok(())
        );
        assert_eq!(
            check(&WebSignInPages::any_page().with_credential()),
            Err(WebSignInError::CredentialAnywhere)
        );
        let both = WebSignInPages {
            any_page: true,
            ..WebSignInPages::at(&["/login"]).with_credential()
        };
        assert_eq!(check(&both), Err(WebSignInError::CredentialAnywhere));
        let read: WebSignInPages =
            serde_json::from_str(r#"{"paths":["/s"],"uses_credential":true}"#).unwrap();
        assert!(read.uses_credential);
        let read: WebSignInPages = serde_json::from_str(r#"{"paths":["/s"]}"#).unwrap();
        assert!(!read.uses_credential);
        assert_eq!(
            serde_json::to_value(WebSignInPages::at(&["/s"])).unwrap(),
            serde_json::json!({"paths": ["/s"], "any_page": false}),
            "an older reader sees the bytes it knew"
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
