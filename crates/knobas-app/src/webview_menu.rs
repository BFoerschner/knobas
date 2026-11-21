//! *WebView2's own menu, trimmed to an allow-list* (Windows;
//! `docs/decisions/2026-11-17-a-hover-a-drag-and-the-web-views-menu-on-the-pc.md`).
//!
//! knobas draws its own right-click menu and stops the web view's with
//! `preventDefault()` everywhere but in a field a person types in, where the
//! platform's menu stays for its spelling, emoji and paste
//! (`docs/decisions/2026-11-16-right-click-menus.md`). On Windows that menu
//! came with Edge's own *Import passwords*, *Writing direction* and *More
//! tools*; and inside the frames knobas draws (a Docs page's embed, a
//! renderer's output), which are sandboxed so the page cannot listen in them,
//! WebView2 showed its page menu: *Back*, *Refresh* (which reloads the whole
//! app), *Save as*, *Print*.
//!
//! WebView2 raises `ContextMenuRequested` only where the page let the menu
//! through: a field, and anything inside a frame. The hook keeps an item only
//! when its name (WebView2's own, the same in every language) is in
//! [`KEPT`], removes every submenu, tidies the separators, and shows no menu
//! where nothing is left. Chromium offers only what applies to the press, so
//! one list serves a field and a frame alike. `other`, the name of everything
//! WebView2 does not name (*Import passwords* among it), never stays: the next
//! thing Edge adds arrives the same way.
//!
//! Tauri has no setting for it, and `AreDefaultContextMenusEnabled` would take
//! the fields' spelling with it. A runtime without the event (before
//! `ICoreWebView2_11`) or a web view that cannot be reached leaves the menu as
//! WebView2 draws it, with one `debug` line.

/// The items that stay, by WebView2's name. Spelling suggestions and *Add to
/// dictionary* are all named `spellcheck` (WebView2 157 on Windows 11, read
/// in a scratch harness on 2026-11-17; the documentation spells it
/// `spellCheck`, and [`keeps`] takes either).
pub const KEPT: &[&str] = &[
    "spellcheck",
    "emoji",
    "undo",
    "redo",
    "cut",
    "copy",
    "paste",
    "pasteAndMatchStyle",
    "selectAll",
    "copyLinkLocation",
    "copyImage",
];

/// One item of a menu, as [`removals`] reads it: the one rule a site's page
/// menu is trimmed by too (`knobas_site::menu`).
pub use knobas_site::menu::Item;

/// Whether a command named `name` stays. `inspect`: a development build,
/// whose *Inspect* stays.
#[must_use]
pub fn keeps(name: &str, inspect: bool) -> bool {
    name.eq_ignore_ascii_case("spellcheck")
        || KEPT.contains(&name)
        || (inspect && name == "inspectElement")
}

/// The indexes of `items` to remove, highest first: every item [`keeps`]
/// drops and every submenu, then any separator left first, last or after
/// another.
#[must_use]
pub fn removals(items: &[Item<'_>], inspect: bool) -> Vec<usize> {
    knobas_site::menu::removals(items, |name| keeps(name, inspect))
}

/// Hook `window`'s web view. Does nothing off Windows.
pub fn trim<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    #[cfg(windows)]
    {
        let asked = window.with_webview(|webview| {
            // SAFETY: this runs on the main thread, where Tauri hands out the
            // live controller; every call is a WebView2 COM method on it.
            if let Err(error) = unsafe { webview2::hook(&webview.controller()) } {
                tracing::debug!(%error, "the web view's menu is left as WebView2 draws it");
            }
        });
        if let Err(error) = asked {
            tracing::debug!(%error, "the web view's menu: the web view could not be reached");
        }
    }
    #[cfg(not(windows))]
    let _ = window;
}

#[cfg(windows)]
mod webview2 {
    use webview2_com::ContextMenuRequestedEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU, ICoreWebView2_11,
        ICoreWebView2ContextMenuRequestedEventArgs, ICoreWebView2Controller,
    };
    use windows_core::{Interface, PWSTR};

    use super::{Item, removals};

    /// Add the handler to `controller`'s web view.
    ///
    /// # Safety
    ///
    /// On the main thread, with a live controller.
    pub(super) unsafe fn hook(controller: &ICoreWebView2Controller) -> windows_core::Result<()> {
        // SAFETY: the caller's.
        let core = unsafe { controller.CoreWebView2() }?;
        let core: ICoreWebView2_11 = core.cast()?;
        // Synchronous: the items go before WebView2 draws them, with no read
        // and no wait, so the right-click stays a frame.
        let handler = ContextMenuRequestedEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                // SAFETY: WebView2 calls this on the main thread with live args.
                if let Err(error) = unsafe { trim(&args) } {
                    tracing::debug!(%error, "the web view's menu could not be trimmed");
                }
            }
            Ok(())
        }));
        let mut token = 0_i64;
        // SAFETY: a live web view and a handler WebView2 keeps a reference to.
        unsafe { core.add_ContextMenuRequested(&handler, &raw mut token) }
    }

    /// Remove what [`removals`] says from the menu `args` carries, and show
    /// none where nothing is left.
    ///
    /// # Safety
    ///
    /// On the main thread, with the live args of a `ContextMenuRequested`.
    unsafe fn trim(args: &ICoreWebView2ContextMenuRequestedEventArgs) -> windows_core::Result<()> {
        // SAFETY: COM calls on the live args, the out-pointers ours.
        unsafe {
            let items = args.MenuItems()?;
            let mut count = 0_u32;
            items.Count(&raw mut count)?;
            let mut read: Vec<(COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND, String)> =
                Vec::with_capacity(count as usize);
            for index in 0..count {
                let item = items.GetValueAtIndex(index)?;
                let mut kind = COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND::default();
                item.Kind(&raw mut kind)?;
                let mut name = PWSTR::null();
                item.Name(&raw mut name)?;
                read.push((kind, webview2_com::take_pwstr(name)));
            }
            let menu: Vec<Item<'_>> = read
                .iter()
                .map(|(kind, name)| {
                    if *kind == COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR {
                        Item::Separator
                    } else if *kind == COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU {
                        Item::Submenu(name)
                    } else {
                        Item::Command(name)
                    }
                })
                .collect();
            let gone = removals(&menu, cfg!(debug_assertions));
            for index in &gone {
                if let Ok(index) = u32::try_from(*index) {
                    items.RemoveValueAtIndex(index)?;
                }
            }
            if gone.len() == menu.len() {
                args.SetHandled(true)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Item::{Command, Separator, Submenu};
    use super::*;

    fn left<'a>(items: &[Item<'a>], inspect: bool) -> Vec<Item<'a>> {
        let gone = removals(items, inspect);
        items
            .iter()
            .enumerate()
            .filter(|(index, _)| !gone.contains(index))
            .map(|(_, item)| *item)
            .collect()
    }

    /// Tickets' filter (spellcheck off), as WebView2 157 offered it on the PC.
    const FILTER: &[Item<'static>] = &[
        Command("emoji"),
        Separator,
        Command("other"),
        Separator,
        Command("undo"),
        Command("redo"),
        Separator,
        Command("cut"),
        Command("copy"),
        Command("paste"),
        Command("pasteAndMatchStyle"),
        Command("selectAll"),
        Separator,
        Submenu("other"),
        Separator,
        Submenu("moreTools"),
        Separator,
        Command("inspectElement"),
    ];

    #[test]
    fn a_field_keeps_editing_and_loses_import_passwords_writing_direction_and_more_tools() {
        assert_eq!(
            left(FILTER, false),
            vec![
                Command("emoji"),
                Separator,
                Command("undo"),
                Command("redo"),
                Separator,
                Command("cut"),
                Command("copy"),
                Command("paste"),
                Command("pasteAndMatchStyle"),
                Command("selectAll"),
            ]
        );
    }

    #[test]
    fn a_misspelled_word_keeps_its_suggestions() {
        let menu = [
            Command("spellcheck"),
            Command("spellcheck"),
            Command("spellcheck"),
            Separator,
            Command("cut"),
            Command("copy"),
            Command("paste"),
            Command("pasteAndMatchStyle"),
            Separator,
            Submenu("moreTools"),
            Separator,
            Command("inspectElement"),
        ];
        assert_eq!(
            left(&menu, false),
            vec![
                Command("spellcheck"),
                Command("spellcheck"),
                Command("spellcheck"),
                Separator,
                Command("cut"),
                Command("copy"),
                Command("paste"),
                Command("pasteAndMatchStyle"),
            ]
        );
        assert!(keeps("spellCheck", false), "the documented spelling");
    }

    #[test]
    fn a_frames_page_menu_is_emptied() {
        let menu = [
            Command("back"),
            Command("forward"),
            Command("reload"),
            Separator,
            Command("saveAs"),
            Command("print"),
            Separator,
            Submenu("moreTools"),
            Separator,
            Command("inspectElement"),
        ];
        assert_eq!(removals(&menu, false).len(), menu.len());
    }

    #[test]
    fn a_frame_keeps_a_copy_of_the_selection_and_of_a_link() {
        let selected = [
            Command("copy"),
            Command("copyLinkToHighlight"),
            Command("print"),
            Separator,
            Submenu("moreTools"),
            Separator,
            Command("inspectElement"),
        ];
        assert_eq!(left(&selected, false), vec![Command("copy")]);
        let link = [
            Command("openLinkInNewWindow"),
            Separator,
            Command("saveLinkAs"),
            Command("copyLinkLocation"),
            Separator,
            Submenu("moreTools"),
        ];
        assert_eq!(left(&link, false), vec![Command("copyLinkLocation")]);
    }

    #[test]
    fn a_development_build_keeps_inspect() {
        assert_eq!(
            left(
                &[Command("reload"), Separator, Command("inspectElement")],
                true
            ),
            vec![Command("inspectElement")]
        );
    }

    #[test]
    fn a_submenu_goes_whatever_its_name() {
        assert!(left(&[Submenu("copy")], false).is_empty());
    }

    #[test]
    fn removals_come_highest_first() {
        let gone = removals(
            &[
                Command("back"),
                Command("copy"),
                Command("print"),
                Command("reload"),
            ],
            false,
        );
        assert_eq!(gone, vec![3, 2, 0]);
    }
}
