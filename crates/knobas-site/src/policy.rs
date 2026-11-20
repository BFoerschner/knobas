//! Which addresses a site may show, and what crosses from a site to knobas
//! (ADR-0048, decisions §4 and §6). Pure: no window, no clock, no I/O.
//!
//! Every top-level navigation of a site, its redirect hops and its new-window
//! requests included, goes through [`decide`]. The platform hook answers wry's
//! `bool` from it and posts the rest to the site's state.

use std::fmt;

use url::Url;

/// A web origin: scheme, host and port. Two origins are the same only when
/// all three are, so `http` on an `https` source's host is another origin
/// (no downgrade).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Origin {
    /// `http` or `https`.
    pub scheme: String,
    /// Lower case, punycode for an IDN, no trailing dot.
    pub host: String,
    /// The port, the scheme's default when the address names none.
    pub port: u16,
}

impl Origin {
    /// The origin of `url`, when it is an `http` or `https` address with a
    /// host.
    #[must_use]
    pub fn of(url: &Url) -> Option<Self> {
        let scheme = url.scheme();
        if scheme != "http" && scheme != "https" {
            return None;
        }
        let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
        if host.is_empty() {
            return None;
        }
        Some(Self {
            scheme: scheme.to_owned(),
            host,
            port: url.port_or_known_default()?,
        })
    }

    /// The origin `text` names, when it is exactly `http(s)://host[:port]`,
    /// with at most a trailing `/`: no user name, no password, no path, no
    /// query and no fragment. What *Allow* and Settings store.
    ///
    /// # Errors
    ///
    /// [`BadOrigin`] when `text` is anything else.
    pub fn parse(text: &str) -> Result<Self, BadOrigin> {
        let url = Url::parse(text.trim()).map_err(|_| BadOrigin)?;
        if !url.username().is_empty()
            || url.password().is_some()
            || !(url.path().is_empty() || url.path() == "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(BadOrigin);
        }
        Self::of(&url).ok_or(BadOrigin)
    }

    fn default_port(&self) -> bool {
        (self.scheme == "https" && self.port == 443) || (self.scheme == "http" && self.port == 80)
    }
}

impl fmt::Display for Origin {
    /// `https://host`, with `:port` only when it is not the scheme's default.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.default_port() {
            write!(f, "{}://{}", self.scheme, self.host)
        } else {
            write!(f, "{}://{}:{}", self.scheme, self.host, self.port)
        }
    }
}

/// An origin that is not `http(s)://host[:port]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BadOrigin;

impl fmt::Display for BadOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "an address may only be http:// or https:// with a host and a port, nothing after it",
        )
    }
}

impl std::error::Error for BadOrigin {}

/// A source's base address, as [`owner`] compares it: its origin and its
/// path, without a trailing `/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Base {
    pub origin: Origin,
    /// `""` for a base at the host's root, else `/segment[/segment…]`.
    pub path: String,
}

impl Base {
    /// The base of a source whose base URL is `base_url`.
    #[must_use]
    pub fn parse(base_url: &str) -> Option<Self> {
        let url = Url::parse(base_url.trim()).ok()?;
        let origin = Origin::of(&url)?;
        Some(Self {
            origin,
            path: url.path().trim_end_matches('/').to_owned(),
        })
    }

    /// Whether `url` lies under this base: the same origin, and its path is
    /// the base's path or goes on from it at a `/`.
    #[must_use]
    pub fn holds(&self, origin: &Origin, path: &str) -> bool {
        if *origin != self.origin {
            return false;
        }
        if self.path.is_empty() {
            return true;
        }
        path == self.path
            || path
                .strip_prefix(self.path.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    }
}

/// Another source whose pages open inside knobas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OtherSite {
    pub source: String,
    pub base: Base,
}

/// What one site may show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteRules {
    /// The source's own base: its origin is `own(S)`.
    pub base: Base,
    /// The origins the person allowed for this source.
    pub allowed: Vec<Origin>,
    /// The other sources with *Open its pages inside knobas* on.
    pub others: Vec<OtherSite>,
}

impl SiteRules {
    /// `own(S)`.
    #[must_use]
    pub fn own(&self) -> &Origin {
        &self.base.origin
    }

    /// Whether `origin` is `own(S)` or one the person allowed.
    #[must_use]
    pub fn allows(&self, origin: &Origin) -> bool {
        origin == self.own() || self.allowed.contains(origin)
    }
}

/// Why a navigation was cancelled outright.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The address's scheme is not `http` or `https`; the scheme as written,
    /// lower case, or `None` when the address did not parse.
    Scheme(Option<String>),
}

/// What happens to one top-level navigation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// It loads in the site.
    Allow,
    /// Line 1: cancelled, and nothing is handed to the operating system.
    Refuse(Refusal),
    /// Line 2: cancelled; knobas opens that source's site at the address.
    ToSite { source: String },
    /// Line 4: cancelled; the site asks whether it may continue there.
    Ask { origin: Origin },
    /// Line 5: cancelled; the address opens in the browser.
    Browser,
}

/// The source among `others` and the site's own whose base is the longest
/// prefix of `url`, at a segment boundary. `None` when it is the site's own
/// source or nobody's.
fn owner<'r>(rules: &'r SiteRules, origin: &Origin, path: &str) -> Option<&'r OtherSite> {
    let own = rules
        .base
        .holds(origin, path)
        .then_some(rules.base.path.len());
    let mut best: Option<(&OtherSite, usize)> = None;
    for other in &rules.others {
        if other.base.holds(origin, path)
            && best.is_none_or(|(_, length)| other.base.path.len() > length)
        {
            best = Some((other, other.base.path.len()));
        }
    }
    match (best, own) {
        (Some((other, length)), Some(own)) if length > own => Some(other),
        (Some((other, _)), None) => Some(other),
        _ => None,
    }
}

/// The rule of decisions §6, for one top-level navigation of the site
/// `rules` describe. `landing`: knobas started a load in the view and no page
/// on `own(S)` has committed since.
#[must_use]
pub fn decide(rules: &SiteRules, url: &str, landing: bool) -> Decision {
    if url == "about:blank" {
        return Decision::Allow;
    }
    // Line 1.
    let Ok(parsed) = Url::parse(url) else {
        return Decision::Refuse(Refusal::Scheme(None));
    };
    let Some(origin) = Origin::of(&parsed) else {
        return Decision::Refuse(Refusal::Scheme(Some(parsed.scheme().to_ascii_lowercase())));
    };
    // Line 2.
    if let Some(other) = owner(rules, &origin, parsed.path()) {
        return Decision::ToSite {
            source: other.source.clone(),
        };
    }
    // Line 3.
    if rules.allows(&origin) {
        return Decision::Allow;
    }
    // Lines 4 and 5.
    if landing {
        Decision::Ask { origin }
    } else {
        Decision::Browser
    }
}

/// The most characters of a page's title knobas takes.
pub const TITLE_MAX: usize = 200;

/// Whether `ch` is a control character or one that reorders text: a bidi
/// embedding, override or isolate, or a directional mark.
fn unsafe_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{061C}'
        )
}

/// A page's title as knobas draws it: control and bidi characters removed,
/// runs of white space made one space, trimmed, and cut at [`TITLE_MAX`]
/// characters. It is drawn as text, never as HTML.
#[must_use]
pub fn clean_title(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(TITLE_MAX * 4));
    let mut space = false;
    let mut count = 0;
    for ch in raw.chars() {
        if ch.is_whitespace() {
            space = !out.is_empty();
            continue;
        }
        if unsafe_char(ch) {
            continue;
        }
        if space {
            if count + 1 >= TITLE_MAX {
                break;
            }
            out.push(' ');
            count += 1;
            space = false;
        }
        if count >= TITLE_MAX {
            break;
        }
        out.push(ch);
        count += 1;
    }
    out
}

/// The most characters of a saved file's name.
pub const FILE_NAME_MAX: usize = 120;

/// Names Windows keeps for devices, which no file may take.
const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The name a download is saved under in the Downloads folder: the last path
/// segment of what the site suggested, with control characters, path
/// separators and the characters Windows refuses removed, dots and spaces
/// trimmed from its ends, cut at [`FILE_NAME_MAX`] characters with its
/// extension kept, and `name (2).ext` when `taken` says the name is in use.
/// Never a path: the caller joins it to the Downloads folder and nothing
/// else.
#[must_use]
pub fn download_name(suggested: &str, taken: impl Fn(&str) -> bool) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let mut name: String = last
        .chars()
        .filter(|ch| !unsafe_char(*ch) && !matches!(ch, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect();
    name = name
        .trim_matches(|ch: char| ch == '.' || ch.is_whitespace())
        .to_owned();
    if name.is_empty() {
        name = "download".to_owned();
    }
    let (stem, ext) = split_ext(&name);
    let mut stem = stem.to_owned();
    if RESERVED.contains(&stem.to_ascii_lowercase().as_str()) {
        stem.insert(0, '_');
    }
    let ext_len = ext.chars().count();
    let room = FILE_NAME_MAX.saturating_sub(ext_len).max(1);
    if stem.chars().count() > room {
        stem = stem
            .chars()
            .take(room)
            .collect::<String>()
            .trim_end()
            .to_owned();
    }
    let first = format!("{stem}{ext}");
    if !taken(&first) {
        return first;
    }
    (2..10_000)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| format!("{stem} (copy){ext}"))
}

/// `("name", ".ext")`, with an extension only when the name has a stem
/// before its last dot and the extension is short.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && name.len() - dot <= 12 => name.split_at(dot),
        _ => (name, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(text: &str) -> Origin {
        Origin::parse(text).expect("an origin")
    }

    fn rules(base: &str) -> SiteRules {
        SiteRules {
            base: Base::parse(base).expect("a base"),
            allowed: vec![],
            others: vec![],
        }
    }

    fn other(source: &str, base: &str) -> OtherSite {
        OtherSite {
            source: source.to_owned(),
            base: Base::parse(base).expect("a base"),
        }
    }

    // ---- the five lines, in order

    #[test]
    fn line_1_a_scheme_other_than_http_is_refused_and_goes_nowhere() {
        let r = rules("https://jira.example");
        for (url, scheme) in [
            ("javascript:alert(1)", "javascript"),
            ("data:text/html,<b>x</b>", "data"),
            ("file:///etc/passwd", "file"),
            ("blob:https://jira.example/1234", "blob"),
            ("mailto:someone@jira.example", "mailto"),
            ("ms-msdt:/id PCWDiagnostic", "ms-msdt"),
            ("FTP://jira.example/", "ftp"),
        ] {
            assert_eq!(
                decide(&r, url, true),
                Decision::Refuse(Refusal::Scheme(Some(scheme.into()))),
                "{url}"
            );
            assert_eq!(
                decide(&r, url, false),
                Decision::Refuse(Refusal::Scheme(Some(scheme.into()))),
                "{url}"
            );
        }
        assert_eq!(
            decide(&r, "not an address", false),
            Decision::Refuse(Refusal::Scheme(None))
        );
    }

    #[test]
    fn line_1_about_blank_is_allowed() {
        assert_eq!(
            decide(&rules("https://jira.example"), "about:blank", false),
            Decision::Allow
        );
    }

    #[test]
    fn line_2_another_sources_address_goes_to_its_site_before_the_allow_list() {
        let mut r = rules("https://jira.example");
        r.others = vec![other("wiki", "https://wiki.example")];
        // Even when the person allowed the wiki's origin for this site, its
        // pages are the wiki's.
        r.allowed = vec![origin("https://wiki.example")];
        assert_eq!(
            decide(&r, "https://wiki.example/display/PAY", false),
            Decision::ToSite {
                source: "wiki".into()
            }
        );
        assert_eq!(
            decide(&r, "https://wiki.example/display/PAY", true),
            Decision::ToSite {
                source: "wiki".into()
            }
        );
    }

    #[test]
    fn line_3_own_and_allowed_origins_load() {
        let mut r = rules("https://jira.example");
        r.allowed = vec![origin("https://sso.example")];
        assert_eq!(
            decide(&r, "https://jira.example/browse/PAY-1", false),
            Decision::Allow
        );
        assert_eq!(
            decide(&r, "https://sso.example/login", false),
            Decision::Allow
        );
        assert_eq!(
            decide(&r, "https://sso.example/login", true),
            Decision::Allow
        );
    }

    #[test]
    fn line_4_an_unknown_origin_while_landing_asks() {
        let r = rules("https://jira.example");
        assert_eq!(
            decide(&r, "https://sso.example/auth?x=1", true),
            Decision::Ask {
                origin: origin("https://sso.example")
            }
        );
    }

    #[test]
    fn line_5_an_unknown_origin_after_landing_goes_to_the_browser() {
        let r = rules("https://jira.example");
        assert_eq!(
            decide(&r, "https://news.example/", false),
            Decision::Browser
        );
    }

    // ---- what an origin is

    #[test]
    fn another_port_on_the_own_host_is_another_origin() {
        let r = rules("https://jira.example");
        assert_eq!(
            decide(&r, "https://jira.example:8443/", false),
            Decision::Browser
        );
        assert_eq!(
            decide(&r, "https://jira.example:443/x", false),
            Decision::Allow
        );
    }

    #[test]
    fn http_on_an_https_source_is_no_downgrade() {
        let r = rules("https://jira.example");
        assert_eq!(
            decide(&r, "http://jira.example/", true),
            Decision::Ask {
                origin: origin("http://jira.example")
            }
        );
        assert_eq!(decide(&r, "http://jira.example/", false), Decision::Browser);
    }

    #[test]
    fn a_host_that_only_starts_with_the_own_host_is_not_it() {
        let r = rules("https://jira.example");
        assert_eq!(
            decide(&r, "https://jira.example.evil.test/", false),
            Decision::Browser
        );
        assert_eq!(
            decide(&r, "https://evil-jira.example/", false),
            Decision::Browser
        );
    }

    #[test]
    fn user_info_does_not_make_an_address_the_own_host() {
        let r = rules("https://jira.example");
        assert_eq!(
            decide(&r, "https://jira.example@evil.test/", false),
            Decision::Browser
        );
        assert_eq!(
            decide(&r, "https://jira.example:pw@evil.test/", true),
            Decision::Ask {
                origin: origin("https://evil.test")
            }
        );
    }

    #[test]
    fn a_trailing_dot_and_upper_case_are_the_same_host() {
        let r = rules("https://jira.example");
        assert_eq!(
            decide(&r, "https://jira.example./browse", false),
            Decision::Allow
        );
        assert_eq!(
            decide(&r, "HTTPS://JIRA.Example/browse", false),
            Decision::Allow
        );
    }

    #[test]
    fn an_idn_host_and_its_punycode_are_one_origin() {
        let r = rules("https://bücher.example");
        assert_eq!(r.own().host, "xn--bcher-kva.example");
        assert_eq!(
            decide(&r, "https://xn--bcher-kva.example/x", false),
            Decision::Allow
        );
        assert_eq!(
            decide(&r, "https://bücher.example/x", false),
            Decision::Allow
        );
        // A look-alike in another script is another host.
        assert_eq!(
            decide(&r, "https://büсher.example/x", false),
            Decision::Browser
        );
    }

    #[test]
    fn two_sources_on_one_origin_split_by_path_at_a_segment() {
        let mut r = rules("https://tools.example/jira");
        r.others = vec![other("jira2", "https://tools.example/jira2")];
        assert_eq!(
            decide(&r, "https://tools.example/jira/browse/A-1", false),
            Decision::Allow
        );
        assert_eq!(
            decide(&r, "https://tools.example/jira2/browse/B-1", false),
            Decision::ToSite {
                source: "jira2".into()
            }
        );
        assert_eq!(
            decide(&r, "https://tools.example/jira2", false),
            Decision::ToSite {
                source: "jira2".into()
            }
        );
        // The other way round: from jira2's site, /jira is the first's.
        let mut r2 = rules("https://tools.example/jira2");
        r2.others = vec![other("jira", "https://tools.example/jira")];
        assert_eq!(
            decide(&r2, "https://tools.example/jira/browse/A-1", false),
            Decision::ToSite {
                source: "jira".into()
            }
        );
        assert_eq!(
            decide(&r2, "https://tools.example/jira2/x", false),
            Decision::Allow
        );
        // A root source and one under a path: the longer base wins.
        let mut r3 = rules("https://tools.example");
        r3.others = vec![other("wiki", "https://tools.example/wiki")];
        assert_eq!(
            decide(&r3, "https://tools.example/wiki/x", false),
            Decision::ToSite {
                source: "wiki".into()
            }
        );
        assert_eq!(
            decide(&r3, "https://tools.example/wikix", false),
            Decision::Allow
        );
    }

    #[test]
    fn a_redirect_chain_own_unknown_own() {
        let r = rules("https://jira.example");
        // While landing: the own hop loads, the unknown one asks.
        assert_eq!(
            decide(&r, "https://jira.example/login", true),
            Decision::Allow
        );
        assert_eq!(
            decide(&r, "https://idp.example/sso", true),
            Decision::Ask {
                origin: origin("https://idp.example")
            }
        );
        // After landing: the unknown hop leaves for the browser.
        assert_eq!(
            decide(&r, "https://jira.example/login", false),
            Decision::Allow
        );
        assert_eq!(
            decide(&r, "https://idp.example/sso", false),
            Decision::Browser
        );
        assert_eq!(decide(&r, "https://jira.example/", false), Decision::Allow);
    }

    // ---- origins as stored

    #[test]
    fn an_origin_to_store_is_scheme_host_and_port_alone() {
        assert_eq!(
            origin("https://sso.example").to_string(),
            "https://sso.example"
        );
        assert_eq!(
            origin("https://sso.example/").to_string(),
            "https://sso.example"
        );
        assert_eq!(
            origin("https://SSO.example:443").to_string(),
            "https://sso.example"
        );
        assert_eq!(
            origin("http://sso.example:8080").to_string(),
            "http://sso.example:8080"
        );
        for bad in [
            "https://sso.example/path",
            "https://user@sso.example",
            "https://user:pw@sso.example",
            "https://sso.example/?q",
            "https://sso.example/#f",
            "ftp://sso.example",
            "javascript:alert(1)",
            "sso.example",
            "",
        ] {
            assert_eq!(Origin::parse(bad), Err(BadOrigin), "{bad}");
        }
    }

    // ---- what crosses from a site

    #[test]
    fn a_title_loses_controls_and_bidi_and_is_cut() {
        assert_eq!(
            clean_title("<b>PAY-231</b> Payouts"),
            "<b>PAY-231</b> Payouts"
        );
        assert_eq!(
            clean_title("Line one\nline two\r\n\t end"),
            "Line one line two end"
        );
        assert_eq!(clean_title("abc\u{202E}gpj.exe"), "abcgpj.exe");
        assert_eq!(clean_title("\u{2066}x\u{2069}\u{200F}y\u{0007}"), "xy");
        assert_eq!(clean_title("   "), "");
        let long = "a".repeat(500);
        assert_eq!(clean_title(&long).chars().count(), TITLE_MAX);
        let words = "ab ".repeat(300);
        assert!(clean_title(&words).chars().count() <= TITLE_MAX);
        assert_eq!(clean_title(&"ü".repeat(300)).chars().count(), TITLE_MAX);
    }

    #[test]
    fn a_download_name_never_leaves_the_folder() {
        let free = |_: &str| false;
        assert_eq!(download_name("../../x.exe", free), "x.exe");
        assert_eq!(download_name("C:\\Windows\\x.dll", free), "x.dll");
        assert_eq!(download_name("C:x.txt", free), "Cx.txt");
        assert_eq!(download_name("..", free), "download");
        assert_eq!(download_name("...", free), "download");
        assert_eq!(download_name("", free), "download");
        assert_eq!(download_name("report\n.pdf", free), "report.pdf");
        assert_eq!(download_name("a\u{202E}fdp.exe", free), "afdp.exe");
        assert_eq!(download_name("  spaced .txt. ", free), "spaced .txt");
        assert_eq!(download_name("CON.txt", free), "_CON.txt");
        assert_eq!(download_name("nul", free), "_nul");
        assert_eq!(download_name("what?<>|\"*.txt", free), "what.txt");
    }

    #[test]
    fn a_long_download_name_keeps_its_extension() {
        let name = download_name(&format!("{}.tar.gz", "x".repeat(300)), |_| false);
        assert_eq!(name.chars().count(), FILE_NAME_MAX);
        assert!(name.ends_with(".gz"), "{name}");
    }

    #[test]
    fn a_download_never_takes_an_existing_name() {
        let taken = |name: &str| matches!(name, "spike.txt" | "spike (2).txt");
        assert_eq!(download_name("spike.txt", taken), "spike (3).txt");
        assert_eq!(
            download_name("README", |n: &str| n == "README"),
            "README (2)"
        );
    }
}
