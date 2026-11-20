//! WebView2's own right-click menu, trimmed to an allow-list (decisions §11;
//! `knobas-app`'s `webview_menu.rs` for the main window). Pure: the Windows
//! hook reads the items, asks [`removals`], and removes them.
//!
//! On a site the list is the main window's plus *Back*, *Forward* and
//! *Print*. WebView2's own *Reload* is left out: it would skip the question a
//! reload asks of a page with unsaved changes.

/// One item of a menu, as [`removals`] reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item<'a> {
    /// A command, a check box or a radio item, by WebView2's name.
    Command(&'a str),
    /// An item that opens more items.
    Submenu(&'a str),
    /// A rule between groups.
    Separator,
}

/// The items a site's page menu keeps, by WebView2's name, besides
/// `spellcheck` in either spelling.
pub const SITE_KEPT: &[&str] = &[
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
    "back",
    "forward",
    "print",
];

/// Whether a site's page menu keeps the command `name`. `inspect`: a
/// development build, whose *Inspect* stays.
#[must_use]
pub fn site_keeps(name: &str, inspect: bool) -> bool {
    name.eq_ignore_ascii_case("spellcheck")
        || SITE_KEPT.contains(&name)
        || (inspect && name == "inspectElement")
}

/// The indexes of `items` to remove, highest first: every command `keeps`
/// drops and every submenu, then any separator left first, last or after
/// another.
#[must_use]
pub fn removals(items: &[Item<'_>], keeps: impl Fn(&str) -> bool) -> Vec<usize> {
    let mut stays = vec![false; items.len()];
    let mut item_before = false;
    let mut pending: Option<usize> = None;
    for (index, item) in items.iter().enumerate() {
        match item {
            Item::Command(name) if keeps(name) => {
                if let Some(rule) = pending.take() {
                    stays[rule] = true;
                }
                stays[index] = true;
                item_before = true;
            }
            Item::Separator if item_before => {
                pending = Some(index);
                item_before = false;
            }
            Item::Command(_) | Item::Submenu(_) | Item::Separator => {}
        }
    }
    (0..items.len())
        .rev()
        .filter(|index| !stays[*index])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::Item::{Command, Separator, Submenu};
    use super::*;

    fn left<'a>(items: &[Item<'a>]) -> Vec<Item<'a>> {
        let gone = removals(items, |name| site_keeps(name, false));
        items
            .iter()
            .enumerate()
            .filter(|(i, _)| !gone.contains(i))
            .map(|(_, item)| *item)
            .collect()
    }

    #[test]
    fn a_sites_page_menu_keeps_history_and_print_and_never_reload() {
        let menu = [
            Command("back"),
            Command("forward"),
            Command("reload"),
            Separator,
            Command("saveAs"),
            Command("print"),
            Separator,
            Command("other"),
            Submenu("moreTools"),
            Separator,
            Command("inspectElement"),
        ];
        assert_eq!(
            left(&menu),
            [
                Command("back"),
                Command("forward"),
                Separator,
                Command("print")
            ]
        );
    }

    #[test]
    fn a_field_in_a_site_keeps_its_editing() {
        let menu = [
            Command("spellCheck"),
            Separator,
            Command("cut"),
            Command("copy"),
            Command("paste"),
            Separator,
            Submenu("other"),
        ];
        assert_eq!(
            left(&menu),
            [
                Command("spellCheck"),
                Separator,
                Command("cut"),
                Command("copy"),
                Command("paste")
            ]
        );
    }

    #[test]
    fn a_development_build_keeps_inspect() {
        assert!(site_keeps("inspectElement", true));
        assert!(!site_keeps("inspectElement", false));
    }
}
