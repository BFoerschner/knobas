//! The decisions inside the native glue (`wry_host`, `mac`, `win`), kept
//! where a test reaches them on every platform. The glue itself is one-line
//! calls into a web view or the OS, witnessed in the real windows; what it
//! decides is here (the final suite's mutants, 2026-11-22).

use crate::host::Rect;

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
}
