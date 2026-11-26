//! The decisions inside the native glue (`wry_host`, `mac`, `win`), kept
//! where a test reaches them on every platform. The glue itself is one-line
//! calls into a web view or the OS, witnessed in the real windows; what it
//! decides is here (the final suite's mutants, 2026-11-22).

use crate::host::{Rect, Travel};

/// `bounds`, measured in knobas's page, moved down by `down` (the room the
/// page starts below its window's content view, the macOS title bar) and
/// shortened by as much, never below one point.
#[must_use]
pub fn under_bar(bounds: Rect, down: f64) -> Rect {
    Rect {
        y: bounds.y + down,
        height: (bounds.height - down).max(1.0),
        ..bounds
    }
}

/// How far the page starts down a content view `full` high whose layout
/// rectangle is `layout` high: the title bar's height, never negative.
#[must_use]
pub fn title_room(full: f64, layout: f64) -> f64 {
    (full - layout).max(0.0)
}

/// Whether a site's data folder is gone after its removal: removed now, or
/// never there.
#[must_use]
pub fn removed(result: &std::io::Result<()>) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => error.kind() == std::io::ErrorKind::NotFound,
    }
}

/// The page's address as the view reports it: none while it has none.
#[must_use]
pub fn page_address(url: String) -> Option<String> {
    (!url.is_empty()).then_some(url)
}

/// A browser process id as WebView2 reports it: 0 is none.
#[must_use]
pub fn known_process(pid: u32) -> Option<u32> {
    (pid != 0).then_some(pid)
}

/// Whether a confined site's request or frame for `url` is refused: with a
/// rule, what it does not let go; with none, nothing (ux30).
#[must_use]
pub fn refuses(rules: Option<&crate::confine::Confine>, url: &str) -> bool {
    rules.is_some_and(|rule| !rule.may_request(url))
}

/// The resource kind WebView2 names by its number
/// (`COREWEBVIEW2_WEB_RESOURCE_CONTEXT`), for the log.
#[must_use]
pub fn resource_kind(context: i32) -> &'static str {
    match context {
        1 => "document",
        2 => "stylesheet",
        3 => "image",
        4 => "media",
        5 => "font",
        6 => "script",
        7 => "xhr",
        8 => "fetch",
        9 => "text-track",
        10 => "event-source",
        11 => "websocket",
        12 => "manifest",
        13 => "signed-exchange",
        14 => "ping",
        15 => "csp-report",
        _ => "other",
    }
}

/// `scheme://host:port` of `url`, never its path or query (the vendor's
/// news address carries the install id in its query), for the log.
#[must_use]
pub fn origin_text(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .map(|url| {
            format!(
                "{}://{}:{}",
                url.scheme(),
                url.host_str().unwrap_or_default(),
                url.port_or_known_default().unwrap_or_default()
            )
        })
        .unwrap_or_else(|| "unparsed".to_owned())
}

/// The travel an AppKit mouse button number stands for: 3 back, 4 forward
/// (#2528). The middle button (2) stays the page's.
#[must_use]
pub fn travel_of_button(button: isize) -> Option<Travel> {
    match button {
        3 => Some(Travel::Back),
        4 => Some(Travel::Forward),
        _ => None,
    }
}

/// `WM_XBUTTONDOWN`, `WM_XBUTTONUP` and `WM_XBUTTONDBLCLK`.
pub const WM_XBUTTONDOWN: u32 = 0x020B;
pub const WM_XBUTTONUP: u32 = 0x020C;
pub const WM_XBUTTONDBLCLK: u32 = 0x020D;

/// What a Windows low-level mouse message is to a site (#2528): the travel
/// of `XButton1` (back) or `XButton2` (forward), and whether this is the
/// release, the moment a browser travels. The button is the high word of
/// `MSLLHOOKSTRUCT::mouseData`. `None` for every other message.
#[must_use]
pub fn xbutton(message: u32, mouse_data: u32) -> Option<(Travel, bool)> {
    let release = match message {
        WM_XBUTTONUP => true,
        WM_XBUTTONDOWN | WM_XBUTTONDBLCLK => false,
        _ => return None,
    };
    let travel = match mouse_data >> 16 {
        1 => Travel::Back,
        2 => Travel::Forward,
        _ => return None,
    };
    Some((travel, release))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_site_under_the_title_bar_moves_down_and_loses_as_much() {
        let bounds = Rect {
            x: 10.0,
            y: 40.0,
            width: 800.0,
            height: 600.0,
        };
        let placed = under_bar(bounds, 32.0);
        assert_eq!(
            (placed.x, placed.y, placed.width, placed.height),
            (10.0, 72.0, 800.0, 568.0)
        );
        let unmoved = under_bar(bounds, 0.0);
        assert_eq!((unmoved.y, unmoved.height), (40.0, 600.0));
        let squeezed = under_bar(
            Rect {
                height: 20.0,
                ..bounds
            },
            32.0,
        );
        assert_eq!(squeezed.height, 1.0, "never below one point");
    }

    #[test]
    fn the_title_room_is_the_content_views_height_less_its_layout() {
        assert_eq!(title_room(900.0, 868.0), 32.0);
        assert_eq!(title_room(868.0, 868.0), 0.0);
        assert_eq!(title_room(860.0, 868.0), 0.0, "never negative");
    }

    #[test]
    fn a_folder_removed_or_never_there_is_gone_and_any_other_failure_is_not() {
        assert!(removed(&Ok(())));
        assert!(removed(&Err(std::io::ErrorKind::NotFound.into())));
        assert!(!removed(&Err(std::io::ErrorKind::PermissionDenied.into())));
    }

    #[test]
    fn an_empty_address_is_none_and_a_process_zero_is_none() {
        assert_eq!(page_address(String::new()), None);
        assert_eq!(
            page_address("https://wiki.example/x".into()).as_deref(),
            Some("https://wiki.example/x")
        );
        assert_eq!(known_process(0), None);
        assert_eq!(known_process(4242), Some(4242));
    }

    /// A confined site refuses what its rule refuses; with no rule, nothing.
    #[test]
    fn a_rule_refuses_what_it_does_not_let_go_and_no_rule_refuses_nothing() {
        let own = crate::policy::Origin::parse("https://vault.example").expect("origin");
        let rule = crate::confine::Confine::new(own, Vec::new());
        assert!(!refuses(Some(&rule), "https://vault.example/api"));
        assert!(refuses(Some(&rule), "https://news.vendor.example/feed"));
        assert!(!refuses(None, "https://news.vendor.example/feed"));
    }

    /// Every kind WebView2 numbers has its name in the log line.
    #[test]
    fn each_resource_kind_has_its_name() {
        let names: Vec<&str> = (0..=16).map(resource_kind).collect();
        assert_eq!(
            names,
            [
                "other",
                "document",
                "stylesheet",
                "image",
                "media",
                "font",
                "script",
                "xhr",
                "fetch",
                "text-track",
                "event-source",
                "websocket",
                "manifest",
                "signed-exchange",
                "ping",
                "csp-report",
                "other",
            ]
        );
    }

    /// The log names an origin, never a path or a query.
    #[test]
    fn an_origin_is_its_scheme_host_and_port_and_nothing_else() {
        assert_eq!(
            origin_text("https://news.vendor.example/feed?install=1234#top"),
            "https://news.vendor.example:443"
        );
        assert_eq!(
            origin_text("http://10.0.0.5:8080/x"),
            "http://10.0.0.5:8080"
        );
        assert_eq!(origin_text("not a url"), "unparsed");
    }

    #[test]
    fn the_mouse_buttons_past_the_middle_one_are_back_and_forward() {
        assert_eq!(travel_of_button(3), Some(Travel::Back));
        assert_eq!(travel_of_button(4), Some(Travel::Forward));
        assert_eq!(travel_of_button(0), None);
        assert_eq!(travel_of_button(2), None, "the middle button stays the page's");
        assert_eq!(travel_of_button(5), None);
    }

    #[test]
    fn xbutton1_is_back_xbutton2_forward_and_the_release_travels() {
        // `mouseData`'s high word names the button; its low word is unused.
        assert_eq!(xbutton(WM_XBUTTONUP, 0x0001_0000), Some((Travel::Back, true)));
        assert_eq!(xbutton(WM_XBUTTONUP, 0x0002_0000), Some((Travel::Forward, true)));
        assert_eq!(xbutton(WM_XBUTTONDOWN, 0x0001_0000), Some((Travel::Back, false)));
        assert_eq!(xbutton(WM_XBUTTONDBLCLK, 0x0002_0000), Some((Travel::Forward, false)));
        assert_eq!(xbutton(WM_XBUTTONUP, 0x0000_0001), None, "no button in the high word");
        // WM_LBUTTONUP, WM_MOUSEMOVE, WM_MBUTTONUP: not a site's travel.
        for other in [0x0202, 0x0200, 0x0208] {
            assert_eq!(xbutton(other, 0x0001_0000), None);
        }
    }
}
