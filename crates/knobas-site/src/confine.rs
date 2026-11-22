//! A confined site: what it loads, not only where it navigates, stays on
//! its own origin and the origins the person allowed for it (ux30,
//! `docs/decisions/2026-11-22-a-password-managers-site-loads-nothing-from-other-websites.md`).
//!
//! A password manager's sign-in page may frame its vendor's news, with the
//! server's version and install id in the address (Pleasant 9.5.4 does).
//! [`crate::policy::decide`] rules top-level navigations only, so a frame,
//! a script, a style, an image, a font, a fetch or a WebSocket to another
//! origin would load as in a browser. For a confined site the platform
//! refuses each of them itself: on macOS a `WKContentRuleList` from
//! [`Confine::content_rules`] and the navigation delegate for frames, on
//! Windows `WebResourceRequested` and `FrameNavigationStarting`, both asking
//! [`Confine::may_load`]. Pure: no window, no I/O.
//!
//! The rule depends on whose page is on top (Fable's ruling, 2026-11-22):
//! - any page but one on an allowed origin (the source's own above all):
//!   requests go to the source's own origin and the allowed origins only;
//! - a page on an allowed origin (an identity provider the person let in):
//!   nothing is refused. It is that provider's page and holds no vault data,
//!   and its own scripts come from its CDN.

use url::Url;

use crate::policy::Origin;

/// What a confined site may load: the source's own origin and the origins
/// the person allowed for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confine {
    own: Origin,
    allowed: Vec<Origin>,
}

impl Confine {
    /// A site on `own` that the person let reach `allowed` too.
    #[must_use]
    pub fn new(own: Origin, allowed: Vec<Origin>) -> Self {
        Self { own, allowed }
    }

    /// The origins requests may go to: the own one first.
    pub fn origins(&self) -> impl Iterator<Item = &Origin> {
        std::iter::once(&self.own).chain(self.allowed.iter())
    }

    /// Whether the page on top, at `top`, is guarded: every page is, but one
    /// on an allowed origin other than the source's own.
    #[must_use]
    pub fn guards(&self, top: Option<&str>) -> bool {
        let top = top
            .and_then(|top| Url::parse(top).ok())
            .and_then(|top| request_origin(&top));
        !top.is_some_and(|top| top != self.own && self.allowed.contains(&top))
    }

    /// Whether a load of `url` may leave while `top` is the page on top:
    /// always under an allowed origin's page ([`Self::guards`]), else only
    /// what [`Self::may_request`] lets go.
    #[must_use]
    pub fn may_load(&self, top: Option<&str>, url: &str) -> bool {
        !self.guards(top) || self.may_request(url)
    }

    /// Whether a request to `url` may leave a guarded page: an `http(s)` or
    /// `ws(s)` address on the own or an allowed origin (a WebSocket's
    /// `ws`/`wss` counts as `http`/`https` on the same host and port), a
    /// `blob:` of one of them, or `data:` and `about:`, which go nowhere.
    /// Anything else, an address that does not parse included, is refused.
    #[must_use]
    pub fn may_request(&self, url: &str) -> bool {
        let Ok(parsed) = Url::parse(url) else {
            return false;
        };
        let inner;
        let target = match parsed.scheme() {
            "data" | "about" => return true,
            "blob" => {
                let Ok(url) = Url::parse(parsed.path()) else {
                    return false;
                };
                inner = url;
                &inner
            }
            _ => &parsed,
        };
        request_origin(target).is_some_and(|origin| self.origins().any(|o| *o == origin))
    }

    /// The `WKContentRuleList` JSON that refuses what [`Self::may_load`]
    /// refuses: one rule blocking every address unless the page on top is on
    /// an allowed origin, then one `ignore-previous-rules` for each origin's
    /// `http(s)`, `ws(s)` and `blob:` addresses, and for `data:` and
    /// `about:`.
    ///
    /// WebKit matches `url-filter` and `unless-top-url` against the address
    /// it normalised: the host in lower case, no default port, a path of at
    /// least `/`. So `^https://host:8443/` is that origin and nothing else
    /// (not `host:84430`, not `host.evil`). A default port is also written
    /// out once more, in case an address keeps it.
    #[must_use]
    pub fn content_rules(&self) -> String {
        let mut block = serde_json::json!({
            "trigger": { "url-filter": ".*" },
            "action": { "type": "block" },
        });
        let tops: Vec<String> = self
            .allowed
            .iter()
            .filter(|origin| **origin != self.own)
            .flat_map(|origin| prefixes(origin, &origin.scheme))
            .collect();
        if !tops.is_empty() {
            block["trigger"]["unless-top-url"] = serde_json::json!(tops);
        }
        let mut filters: Vec<String> = Vec::new();
        for origin in self.origins() {
            let socket = if origin.scheme == "https" {
                "wss"
            } else {
                "ws"
            };
            filters.extend(prefixes(origin, &origin.scheme));
            filters.extend(prefixes(origin, socket));
            filters.extend(prefixes(origin, &format!("blob:{}", origin.scheme)));
        }
        filters.push("^data:".to_owned());
        filters.push("^about:".to_owned());
        let mut rules = vec![block];
        rules.extend(filters.into_iter().map(|filter| {
            serde_json::json!({
                "trigger": { "url-filter": filter },
                "action": { "type": "ignore-previous-rules" },
            })
        }));
        serde_json::Value::Array(rules).to_string()
    }
}

/// `url-filter` prefixes for `origin`'s addresses under `scheme`: the host
/// and the port as WebKit writes them, and with a default port also the port
/// written out.
fn prefixes(origin: &Origin, scheme: &str) -> Vec<String> {
    let host = escape(&origin.host);
    let default = (origin.scheme == "https" && origin.port == 443)
        || (origin.scheme == "http" && origin.port == 80);
    let scheme = escape(scheme);
    let written = format!("^{scheme}://{host}:{}/", origin.port);
    if default {
        vec![format!("^{scheme}://{host}/"), written]
    } else {
        vec![written]
    }
}

/// Whether a confined view may load yet. On macOS the rule list compiles
/// after the view exists; until it is added, nothing loads, and the latest
/// address asked for waits. If it never compiles, nothing loads at all.
#[derive(Debug, Default)]
pub struct Gate(std::cell::RefCell<GateState>);

#[derive(Debug, PartialEq, Eq)]
enum GateState {
    /// The refusal is not in place yet: the address to load once it is.
    Waiting(Option<String>),
    Open,
    Shut,
}

impl Default for GateState {
    fn default() -> Self {
        Self::Waiting(None)
    }
}

impl Gate {
    /// A gate that lets every load through: an unconfined view's.
    #[must_use]
    pub fn opened() -> Self {
        Self(std::cell::RefCell::new(GateState::Open))
    }

    /// A gate that holds `url` until [`Self::open`].
    #[must_use]
    pub fn waiting(url: &str) -> Self {
        Self(std::cell::RefCell::new(GateState::Waiting(Some(
            url.to_owned(),
        ))))
    }

    /// A load of `url` asked for: `Some(url)` to load now; `None` when it
    /// waits for [`Self::open`] (the latest one wins) or the gate is shut.
    #[must_use]
    pub fn load(&self, url: &str) -> Option<String> {
        match &mut *self.0.borrow_mut() {
            GateState::Open => Some(url.to_owned()),
            GateState::Waiting(next) => {
                *next = Some(url.to_owned());
                None
            }
            GateState::Shut => None,
        }
    }

    /// The refusal is in place: the address waiting, to load now.
    #[must_use]
    pub fn open(&self) -> Option<String> {
        let mut state = self.0.borrow_mut();
        match &mut *state {
            GateState::Shut | GateState::Open => None,
            GateState::Waiting(next) => {
                let next = next.take();
                *state = GateState::Open;
                next
            }
        }
    }

    /// The refusal could not be put in place: nothing loads until it is
    /// tried again ([`Self::hold`]).
    pub fn shut(&self) {
        *self.0.borrow_mut() = GateState::Shut;
    }

    /// New rules are on their way (the allow-list changed, or a Retry after
    /// a failure): loads wait again; one already waiting keeps waiting.
    pub fn hold(&self) {
        let mut state = self.0.borrow_mut();
        if !matches!(*state, GateState::Waiting(_)) {
            *state = GateState::Waiting(None);
        }
    }

    /// Whether the refusal failed and nothing loads.
    #[must_use]
    pub fn is_shut(&self) -> bool {
        *self.0.borrow() == GateState::Shut
    }

    /// Whether loads go through now.
    #[must_use]
    pub fn is_open(&self) -> bool {
        *self.0.borrow() == GateState::Open
    }
}

/// What a confined site's rectangle says when its refusal could not be put
/// in place, so it loaded nothing (fail closed: Fable's ruling 3). A
/// `SiteFailure::Other`: the failure enum crosses the frozen IPC.
pub const NOT_CONFINED: &str = "knobas could not block other websites' content on this site, so it loaded nothing. Retry, or quit and start knobas again.";

/// The web origin a request to `url` goes to: `ws` as `http` and `wss` as
/// `https`, on the same host and port.
fn request_origin(url: &Url) -> Option<Origin> {
    let scheme = match url.scheme() {
        "http" | "ws" => "http",
        "https" | "wss" => "https",
        _ => return None,
    };
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(Origin {
        scheme: scheme.to_owned(),
        host,
        port: url.port_or_known_default()?,
    })
}

/// `text` as a WebKit `url-filter` that matches it literally: every
/// character but letters, digits, `-`, `:` and `/` escaped.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '-' | ':' | '/')) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(text: &str) -> Origin {
        Origin::parse(text).expect("an origin")
    }

    fn confine(own: &str, allowed: &[&str]) -> Confine {
        Confine::new(
            origin(own),
            allowed.iter().map(|text| origin(text)).collect(),
        )
    }

    const OWN: &str = "https://127.0.0.1:24801";
    const PAGE: &str = "https://127.0.0.1:24801/SignIn/Direct?ReturnUrl=%2FWebClient%2FMain";

    #[test]
    fn the_sources_own_origin_may_be_asked_for_anything() {
        let c = confine(OWN, &[]);
        for url in [
            "https://127.0.0.1:24801/",
            PAGE,
            "https://127.0.0.1:24801/WebClient/Main?itemId=5f0c",
            "https://127.0.0.1:24801/Content/site.css",
            "HTTPS://127.0.0.1:24801/fonts/a.woff2",
            "wss://127.0.0.1:24801/signalr/connect",
            "blob:https://127.0.0.1:24801/4f1e-77aa",
        ] {
            assert!(c.may_request(url), "{url}");
            assert!(c.may_load(Some(PAGE), url), "{url}");
        }
    }

    #[test]
    fn every_other_origin_is_refused_the_vendors_news_frame_first() {
        let c = confine(OWN, &[]);
        for url in [
            "https://pleasantpasswords.com/product-news?FeedID=1&Version=9.5.4.0.Unregistered,3&ref=abc",
            "https://cdn.example/jquery.js",
            "wss://push.example/socket",
            "blob:https://other.example/4f1e",
            // The same host on another port or scheme is another origin.
            "https://127.0.0.1:24802/",
            "http://127.0.0.1:24801/",
            "ws://127.0.0.1:24801/",
            "https://127.0.0.1/",
            // Not a prefix match.
            "https://127.0.0.1:248010/",
            "https://127.0.0.1.evil.example:24801/",
            "ftp://127.0.0.1:24801/",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "blob:nothing",
            "not an address",
            "",
        ] {
            assert!(!c.may_request(url), "{url}");
            assert!(!c.may_load(Some(PAGE), url), "{url}");
            assert!(!c.may_load(None, url), "{url}");
        }
    }

    #[test]
    fn data_and_about_go_nowhere_and_pass() {
        let c = confine("https://vault.example", &[]);
        assert!(c.may_request("data:image/png;base64,iVBORw0KGgo="));
        assert!(c.may_request("about:blank"));
        assert!(c.may_request("about:srcdoc"));
    }

    #[test]
    fn a_default_port_is_the_same_origin_written_or_not() {
        let c = confine("https://vault.example", &[]);
        assert!(c.may_request("https://vault.example:443/api"));
        assert!(c.may_request("https://VAULT.example./api"));
        assert!(c.may_request("wss://vault.example/hub"));
        assert!(c.may_request("wss://vault.example:443/hub"));
        assert!(!c.may_request("https://vault.example:8443/api"));
        let plain = confine("http://vault.example:8080", &[]);
        assert!(plain.may_request("http://vault.example:8080/a"));
        assert!(plain.may_request("ws://vault.example:8080/hub"));
        assert!(!plain.may_request("wss://vault.example:8080/hub"));
        assert!(!plain.may_request("https://vault.example:8080/a"));
    }

    #[test]
    fn an_allowed_origin_loads_from_the_own_page_and_others_still_do_not() {
        let c = confine("https://vault.example", &["https://sso.example"]);
        assert!(c.may_load(
            Some("https://vault.example/"),
            "https://sso.example/login.js"
        ));
        assert!(c.may_load(Some("https://vault.example/"), "https://vault.example/a"));
        assert!(!c.may_load(Some("https://vault.example/"), "https://cdn.example/"));
        assert_eq!(c.origins().count(), 2);
    }

    #[test]
    fn under_an_allowed_origins_page_nothing_is_refused() {
        let c = confine("https://vault.example", &["https://sso.example"]);
        let sso = Some("https://sso.example/authorize?client=vault");
        assert!(!c.guards(sso));
        assert!(c.may_load(sso, "https://cdn.sso-provider.example/app.js"));
        assert!(c.may_load(sso, "https://vault.example/after"));
    }

    #[test]
    fn every_other_page_on_top_is_guarded() {
        let c = confine("https://vault.example", &["https://sso.example"]);
        for top in [
            Some("https://vault.example/WebClient"),
            Some("https://cdn.example/"),
            // The allowed host on another port is not the allowed origin.
            Some("https://sso.example:8443/"),
            Some("about:blank"),
            Some("not an address"),
            None,
        ] {
            assert!(c.guards(top), "{top:?}");
            assert!(!c.may_load(top, "https://cdn.example/x.js"), "{top:?}");
        }
        // The own origin in the allowed list does not unguard its pages.
        let odd = confine("https://vault.example", &["https://vault.example"]);
        assert!(odd.guards(Some("https://vault.example/")));
        assert!(confine("https://vault.example", &[]).guards(Some("https://x.example/")));
    }

    fn filters(c: &Confine) -> Vec<(String, String)> {
        let rules: serde_json::Value = serde_json::from_str(&c.content_rules()).expect("JSON");
        rules
            .as_array()
            .expect("a list")
            .iter()
            .map(|rule| {
                (
                    rule["trigger"]["url-filter"]
                        .as_str()
                        .expect("a filter")
                        .to_owned(),
                    rule["action"]["type"].as_str().expect("a type").to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn the_rule_list_blocks_everything_then_lets_the_origins_through() {
        let rules: serde_json::Value =
            serde_json::from_str(&confine(OWN, &[]).content_rules()).expect("JSON");
        assert!(rules[0]["trigger"].get("unless-top-url").is_none());
        let got = filters(&confine(OWN, &[]));
        let want: Vec<(String, String)> = [
            (".*", "block"),
            (r"^https://127\.0\.0\.1:24801/", "ignore-previous-rules"),
            (r"^wss://127\.0\.0\.1:24801/", "ignore-previous-rules"),
            (
                r"^blob:https://127\.0\.0\.1:24801/",
                "ignore-previous-rules",
            ),
            ("^data:", "ignore-previous-rules"),
            ("^about:", "ignore-previous-rules"),
        ]
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn the_rule_list_writes_a_default_port_both_ways_and_names_allowed_tops() {
        let c = confine("http://vault.example:80", &["https://[::1]:8443"]);
        let got = filters(&c);
        let names: Vec<&str> = got.iter().map(|(filter, _)| filter.as_str()).collect();
        assert_eq!(
            names,
            [
                ".*",
                r"^http://vault\.example/",
                r"^http://vault\.example:80/",
                r"^ws://vault\.example/",
                r"^ws://vault\.example:80/",
                r"^blob:http://vault\.example/",
                r"^blob:http://vault\.example:80/",
                r"^https://\[::1\]:8443/",
                r"^wss://\[::1\]:8443/",
                r"^blob:https://\[::1\]:8443/",
                "^data:",
                "^about:",
            ]
        );
        let rules: serde_json::Value = serde_json::from_str(&c.content_rules()).expect("JSON");
        assert_eq!(
            rules[0]["trigger"]["unless-top-url"],
            serde_json::json!([r"^https://\[::1\]:8443/"])
        );
    }

    /// WebKit's reading of a rule list, for the subset `content_rules`
    /// writes: `.*`, or `^` and a literal prefix, matched without case
    /// against the address normalised as WebKit does.
    fn webkit_blocks(rules: &str, top: Option<&str>, url: &str) -> bool {
        fn normal(url: &str) -> String {
            Url::parse(url)
                .map(|u| u.to_string())
                .unwrap_or_else(|_| url.to_owned())
                .to_ascii_lowercase()
        }
        fn matches(filter: &str, url: &str) -> bool {
            if filter == ".*" {
                return true;
            }
            let mut literal = String::new();
            let mut chars = filter.strip_prefix('^').expect("anchored").chars();
            while let Some(ch) = chars.next() {
                literal.push(if ch == '\\' {
                    chars.next().expect("escaped")
                } else {
                    ch
                });
            }
            url.starts_with(&literal.to_ascii_lowercase())
        }
        let rules: serde_json::Value = serde_json::from_str(rules).expect("JSON");
        let url = normal(url);
        let top = top.map(normal).unwrap_or_default();
        let mut blocked = false;
        for rule in rules.as_array().expect("a list") {
            let trigger = &rule["trigger"];
            if !matches(trigger["url-filter"].as_str().expect("a filter"), &url) {
                continue;
            }
            if let Some(unless) = trigger.get("unless-top-url") {
                let tops = unless.as_array().expect("a list");
                if tops
                    .iter()
                    .any(|t| matches(t.as_str().expect("a pattern"), &top))
                {
                    continue;
                }
            }
            match rule["action"]["type"].as_str() {
                Some("block") => blocked = true,
                Some("ignore-previous-rules") => blocked = false,
                other => panic!("unexpected action {other:?}"),
            }
        }
        blocked
    }

    #[test]
    fn the_rule_list_refuses_exactly_what_may_load_refuses() {
        let cases = [
            confine(OWN, &[]),
            confine(
                "https://vault.example",
                &["https://sso.example", "http://idp.example:8080"],
            ),
            confine("http://vault.example:80", &["https://[::1]:8443"]),
        ];
        let tops = [
            None,
            Some(PAGE),
            Some("https://vault.example/WebClient/Main"),
            Some("http://vault.example/"),
            Some("https://sso.example/authorize"),
            Some("http://idp.example:8080/login"),
            Some("https://[::1]:8443/x"),
            Some("https://sso.example:444/"),
            Some("https://cdn.example/"),
            Some("about:blank"),
        ];
        let urls = [
            "https://127.0.0.1:24801/a",
            "wss://127.0.0.1:24801/hub",
            "blob:https://127.0.0.1:24801/1",
            "https://127.0.0.1:24802/a",
            "https://127.0.0.1:248010/a",
            "https://127.0.0.1.evil.example:24801/a",
            "https://pleasantpasswords.com/product-news?ref=1",
            "https://vault.example/a",
            "https://vault.example:443/a",
            "HTTPS://Vault.Example/A",
            "wss://vault.example/hub",
            "ws://vault.example/hub",
            "http://vault.example/a",
            "http://vault.example:80/a",
            "ws://vault.example:80/hub",
            "blob:http://vault.example/1",
            "https://vault.example.evil/a",
            "https://sso.example/login.js",
            "https://sso.example:444/login.js",
            "http://idp.example:8080/x",
            "http://idp.example/x",
            "https://[::1]:8443/x",
            "wss://[::1]:8443/x",
            "https://[::2]:8443/x",
            "https://cdn.example/x.js",
            "wss://cdn.example/s",
            "data:image/png;base64,AAAA",
            "about:blank",
            "about:srcdoc",
        ];
        for c in &cases {
            let rules = c.content_rules();
            for top in tops {
                for url in urls {
                    assert_eq!(
                        webkit_blocks(&rules, top, url),
                        !c.may_load(top, url),
                        "{c:?} top {top:?} url {url}"
                    );
                }
            }
        }
    }

    #[test]
    fn escape_leaves_letters_digits_dash_colon_slash_and_escapes_the_rest() {
        assert_eq!(escape("aZ09-:/"), "aZ09-:/");
        assert_eq!(escape(".*+?()[]^$|{}\\"), r"\.\*\+\?\(\)\[\]\^\$\|\{\}\\");
    }

    #[test]
    fn a_gate_holds_the_latest_address_until_it_opens() {
        let gate = Gate::waiting("https://vault.example/");
        assert!(!gate.is_open());
        assert_eq!(gate.load("https://vault.example/a"), None);
        assert_eq!(gate.load("https://vault.example/b"), None);
        assert_eq!(gate.open().as_deref(), Some("https://vault.example/b"));
        assert!(gate.is_open());
        assert_eq!(gate.open(), None);
        assert_eq!(
            gate.load("https://vault.example/c").as_deref(),
            Some("https://vault.example/c")
        );
    }

    #[test]
    fn a_gate_opened_with_nothing_asked_loads_nothing_then_everything() {
        let gate = Gate::default();
        assert_eq!(gate.open(), None);
        assert_eq!(gate.load("about:blank").as_deref(), Some("about:blank"));
        assert_eq!(
            Gate::waiting("https://vault.example/").open().as_deref(),
            Some("https://vault.example/")
        );
    }

    #[test]
    fn an_opened_gate_lets_every_load_through_until_held() {
        let gate = Gate::opened();
        assert!(gate.is_open());
        assert_eq!(
            gate.load("https://a.example/").as_deref(),
            Some("https://a.example/")
        );
        gate.hold();
        assert!(!gate.is_open());
        assert_eq!(gate.load("https://a.example/b"), None);
        assert_eq!(gate.open().as_deref(), Some("https://a.example/b"));
    }

    #[test]
    fn holding_a_waiting_gate_keeps_its_address_and_a_shut_one_waits_again() {
        let gate = Gate::waiting("https://a.example/");
        gate.hold();
        assert_eq!(gate.open().as_deref(), Some("https://a.example/"));
        let shut = Gate::waiting("https://a.example/");
        shut.shut();
        assert!(shut.is_shut());
        shut.hold();
        assert!(!shut.is_shut());
        assert_eq!(shut.load("https://a.example/retry"), None);
        assert_eq!(shut.open().as_deref(), Some("https://a.example/retry"));
        assert!(!Gate::opened().is_shut());
        assert!(!Gate::waiting("x").is_shut());
    }

    #[test]
    fn a_shut_gate_loads_nothing_ever() {
        let gate = Gate::waiting("https://vault.example/");
        gate.shut();
        assert_eq!(gate.load("https://vault.example/a"), None);
        assert_eq!(gate.open(), None);
        assert!(!gate.is_open());
        assert_eq!(gate.load("https://vault.example/b"), None);
    }
}
