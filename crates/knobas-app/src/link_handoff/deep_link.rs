//! `knobas://open?url=<encoded>` deep-link parsing and `argv` extraction (ADR-0061).

use url::Url;

use crate::link_handoff::is_http_or_https_url;

/// The registered OS URL scheme (`knobas://open?url=...`).
pub const DEEP_LINK_SCHEME: &str = "knobas";

/// Parse a `knobas://open?url=<encoded>` URI and return the validated target
/// `http`/`https` URL.
///
/// Refuses every other scheme, host/action, missing `url` parameter, or
/// non-`http(s)` target (such as `file:///`, `javascript:`, or `vscode://`).
///
/// # Errors
///
/// Human-readable reason the URI was refused.
pub fn parse_deep_link(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let parsed = Url::parse(trimmed).map_err(|error| format!("invalid deep link: {error}"))?;
    if parsed.scheme() != DEEP_LINK_SCHEME {
        return Err(format!(
            "unsupported scheme {:?}; expected {DEEP_LINK_SCHEME}://",
            parsed.scheme()
        ));
    }
    let host = parsed.host_str().unwrap_or_default();
    let path = parsed.path();
    if !host.eq_ignore_ascii_case("open") || (!path.is_empty() && path != "/") {
        return Err(format!(
            "unsupported knobas:// action `{host}{path}`; expected knobas://open?url=..."
        ));
    }
    let target = parsed
        .query_pairs()
        .find(|(key, _)| key == "url")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "missing `url` query parameter".to_owned())?;

    if !is_http_or_https_url(&target) {
        return Err(format!(
            "refused deep-link target {target:?}: only http and https URLs are allowed"
        ));
    }
    Ok(target)
}

/// Extract the first valid `knobas://open?url=...` target URL from an argv
/// iterator (such as `single_instance` forwarded args or cold-start args).
#[must_use]
pub fn find_deep_link_in_args<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for arg in args {
        let s = arg.as_ref().trim();
        if s.len() >= DEEP_LINK_SCHEME.len() + 3
            && s[..DEEP_LINK_SCHEME.len()].eq_ignore_ascii_case(DEEP_LINK_SCHEME)
            && s[DEEP_LINK_SCHEME.len()..].starts_with("://")
            && let Ok(url) = parse_deep_link(s)
        {
            return Some(url);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_encoded_http_and_https_targets() {
        let raw = "knobas://open?url=https%3A%2F%2Fwiki.example%2Fpages%2Fviewpage.action%3FpageId%3D98307%23top";
        assert_eq!(
            parse_deep_link(raw).unwrap(),
            "https://wiki.example/pages/viewpage.action?pageId=98307#top"
        );
        assert_eq!(
            parse_deep_link("KNOBAS://OPEN/?url=http%3A%2F%2F127.0.0.1%3A8090%2Fbrowse%2FPAY-1")
                .unwrap(),
            "http://127.0.0.1:8090/browse/PAY-1"
        );
    }

    #[test]
    fn refuses_non_http_targets_and_unknown_actions() {
        for bad in [
            "knobas://open?url=file%3A%2F%2F%2Fetc%2Fpasswd",
            "knobas://open?url=javascript%3Aalert(1)",
            "knobas://open?url=vscode%3A%2F%2Ffile%2Ftmp",
            "knobas://open?url=",
            "knobas://open",
            "knobas://exec?url=https%3A%2F%2Fjira.example",
            "knobas://open/extra?url=https%3A%2F%2Fjira.example",
            "https://jira.example/browse/PAY-1",
            "not a url",
        ] {
            assert!(parse_deep_link(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn extracts_deep_link_from_argv() {
        let args = [
            "/Applications/knobas.app/Contents/MacOS/knobas",
            "--some-flag",
            "knobas://open?url=https%3A%2F%2Fjira.example%2Fbrowse%2FPAY-231",
        ];
        assert_eq!(
            find_deep_link_in_args(args).as_deref(),
            Some("https://jira.example/browse/PAY-231")
        );
        assert_eq!(find_deep_link_in_args(["knobas", "--demo"]), None);
    }
}
