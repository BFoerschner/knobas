//! What a failed load means, from each engine's own codes (decisions §11,
//! §21 item 1, §22). Pure, so both maps are tested on every platform.

use crate::state::SiteFailure;

/// WebKit's `NSError` of a failed load, or `None` for a cancel (−999, and
/// WebKit's 101 and 102 when knobas's own rule stopped a navigation).
#[must_use]
pub fn of_webkit(domain: &str, code: isize) -> Option<SiteFailure> {
    match (domain, code) {
        ("NSURLErrorDomain", -999) | ("WebKitErrorDomain", 101 | 102) => None,
        ("NSURLErrorDomain", -1206..=-1200) => Some(SiteFailure::Certificate),
        ("NSURLErrorDomain", -1003 | -1006) => Some(SiteFailure::NotFound),
        ("NSURLErrorDomain", -1004 | -1005) => Some(SiteFailure::Refused),
        ("NSURLErrorDomain", -1001) => Some(SiteFailure::Timeout),
        _ => Some(SiteFailure::Other(format!("{domain} {code}"))),
    }
}

/// WebView2's `COREWEBVIEW2_WEB_ERROR_STATUS` of a navigation that did not
/// succeed, or `None` for a cancel (14: knobas's own rule, or a stop).
#[must_use]
pub fn of_webview2(status: i32) -> Option<SiteFailure> {
    match status {
        14 => None,
        1..=5 => Some(SiteFailure::Certificate),
        13 => Some(SiteFailure::NotFound),
        6 | 9..=12 => Some(SiteFailure::Refused),
        7 => Some(SiteFailure::Timeout),
        other => Some(SiteFailure::Other(format!("WebView2 error {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webkit_codes() {
        assert_eq!(
            of_webkit("NSURLErrorDomain", -1202),
            Some(SiteFailure::Certificate)
        );
        assert_eq!(
            of_webkit("NSURLErrorDomain", -1003),
            Some(SiteFailure::NotFound)
        );
        assert_eq!(
            of_webkit("NSURLErrorDomain", -1004),
            Some(SiteFailure::Refused)
        );
        assert_eq!(
            of_webkit("NSURLErrorDomain", -1001),
            Some(SiteFailure::Timeout)
        );
        assert_eq!(of_webkit("NSURLErrorDomain", -999), None);
        assert_eq!(of_webkit("WebKitErrorDomain", 102), None);
        assert_eq!(
            of_webkit("NSPOSIXErrorDomain", 57),
            Some(SiteFailure::Other("NSPOSIXErrorDomain 57".into()))
        );
    }

    #[test]
    fn webview2_codes() {
        for certificate in 1..=5 {
            assert_eq!(of_webview2(certificate), Some(SiteFailure::Certificate));
        }
        assert_eq!(of_webview2(13), Some(SiteFailure::NotFound));
        assert_eq!(of_webview2(12), Some(SiteFailure::Refused));
        assert_eq!(of_webview2(6), Some(SiteFailure::Refused));
        assert_eq!(of_webview2(7), Some(SiteFailure::Timeout));
        assert_eq!(of_webview2(14), None, "a cancel, knobas's own rule");
        assert_eq!(
            of_webview2(16),
            Some(SiteFailure::Other("WebView2 error 16".into()))
        );
    }
}
