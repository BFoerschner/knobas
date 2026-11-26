//! The mouse's back and forward buttons over a site, and where the keyboard
//! goes when the window comes back (#2528), on the fake platform.

use std::collections::HashMap;
use std::sync::Arc;

use knobas_site::fake::{FakeHost, FakeShell};
use knobas_site::host::{Travel, ViewEvent};
use knobas_site::keys::Platform;
use knobas_site::policy::{Base, SiteRules};
use knobas_site::sites::{SiteInsets, install_host};
use knobas_site::{Config, SiteEvent, Sites};

struct World {
    sites: Sites,
    shell: Arc<FakeShell>,
    host: FakeHost,
}

fn world() -> World {
    let shell = Arc::new(FakeShell::new());
    let host = FakeHost::default();
    install_host(Box::new(host.clone()));
    let dir = std::env::temp_dir().join(format!(
        "knobas-site-travel-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let sites = Sites::new(
        shell.clone(),
        Config {
            profile_dir: dir.clone(),
            keychain_service: "dev.knobas.desktop.test".into(),
            downloads_dir: Some(dir.join("Downloads")),
            devtools: false,
            platform: Platform::Windows,
        },
    );
    let mut map = HashMap::new();
    for (source, base) in [("jira", "https://jira.example"), ("wiki", "https://wiki.example")] {
        map.insert(
            source.to_owned(),
            SiteRules {
                base: Base::parse(base).unwrap(),
                allowed: vec![],
                others: vec![],
                confined: false,
            },
        );
    }
    sites.set_rules(map);
    World { sites, shell, host }
}

const INSETS: SiteInsets = SiteInsets {
    top: 93.0,
    right: 0.0,
    bottom: 28.0,
    left: 0.0,
};

/// `source`'s site open, committed and shown, with this history.
async fn shown(w: &World, source: &str, can_back: bool, can_forward: bool) {
    w.sites.open(source, None, INSETS, false).await.unwrap();
    let url = format!("https://{source}.example/");
    w.host.report(source, ViewEvent::Committed(url.clone()));
    w.host.report(
        source,
        ViewEvent::History {
            url: Some(url),
            can_back,
            can_forward,
            loading: None,
        },
    );
    w.shell.drain();
    w.sites.show(source, INSETS, false, true);
    w.shell.drain();
    w.shell.take_events();
    w.host.view(source).borrow_mut().calls.clear();
    w.host.calls.borrow_mut().clear();
}

fn calls(w: &World, source: &str) -> Vec<String> {
    w.host.view(source).borrow().calls.clone()
}

/// The keys and statuses the page was told, in order: `key back`,
/// `status can_back=false`.
fn told(w: &World) -> Vec<String> {
    w.shell
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            SiteEvent::Key(key) => Some(format!("key {}", key.key)),
            SiteEvent::Status(status) => Some(format!(
                "status can_back={} can_forward={}",
                status.can_back, status.can_forward
            )),
            _ => None,
        })
        .collect()
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn mouse_back_with_site_history_goes_back_in_the_site_and_knobas_stays() {
    let w = world();
    shown(&w, "jira", true, false).await;
    w.host.mouse("jira", Travel::Back);
    w.shell.drain();
    assert_eq!(calls(&w, "jira"), ["back"]);
    assert!(!told(&w).iter().any(|t| t.starts_with("key")));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn mouse_back_with_no_site_history_hands_back_to_knobas_after_its_status() {
    let w = world();
    shown(&w, "jira", false, false).await;
    w.host.mouse("jira", Travel::Back);
    w.shell.drain();
    assert!(calls(&w, "jira").is_empty(), "the site has nowhere to go");
    // The shell's `back` steps in the site while its status says it can; the
    // status that says it cannot reaches the page first.
    let told = told(&w);
    assert_eq!(told.last().map(String::as_str), Some("key back"));
    assert!(
        !told.iter().any(|t| t.contains("can_back=true")),
        "no stale history: {told:?}"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn mouse_forward_steps_forward_in_the_site_when_it_can_and_hands_over_when_not() {
    let w = world();
    shown(&w, "jira", false, true).await;
    w.host.mouse("jira", Travel::Forward);
    w.shell.drain();
    assert_eq!(calls(&w, "jira"), ["forward"]);
    assert!(told(&w).is_empty());

    shown(&w, "wiki", true, false).await;
    w.host.mouse("wiki", Travel::Forward);
    w.shell.drain();
    assert!(calls(&w, "wiki").is_empty());
    assert_eq!(told(&w).last().map(String::as_str), Some("key forward"));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_decision_reads_the_views_history_at_the_press() {
    // The Mac reports a `pushState` only when the clock polls: the press
    // reads the view itself.
    let w = world();
    shown(&w, "jira", false, false).await;
    w.host.view("jira").borrow_mut().poll = Some(ViewEvent::History {
        url: Some("https://jira.example/browse/PAY-1".into()),
        can_back: true,
        can_forward: false,
        loading: None,
    });
    w.host.mouse("jira", Travel::Back);
    w.shell.drain();
    assert_eq!(calls(&w, "jira"), ["back"]);
    assert!(!told(&w).iter().any(|t| t.starts_with("key")));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_history_read_at_the_press_reaches_the_page_before_the_hand_over() {
    // The page sent `can_back` last; the site has since gone back to its
    // first page by itself. The shell must hear "cannot" before `back`, or
    // its `siteBack` steps in a site with nowhere to go and knobas stays.
    let w = world();
    shown(&w, "jira", true, false).await;
    w.host.view("jira").borrow_mut().poll = Some(ViewEvent::History {
        url: Some("https://jira.example/".into()),
        can_back: false,
        can_forward: true,
        loading: None,
    });
    w.host.mouse("jira", Travel::Back);
    w.shell.drain();
    assert!(calls(&w, "jira").is_empty());
    let told = told(&w);
    let key = told.iter().position(|t| t == "key back").expect("handed over");
    let cannot = told
        .iter()
        .position(|t| t.starts_with("status can_back=false"))
        .expect("status sent");
    assert!(cannot < key, "status first: {told:?}");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_hidden_site_takes_no_mouse_travel() {
    let w = world();
    shown(&w, "jira", true, false).await;
    w.sites.hide("jira");
    w.shell.drain();
    w.host.view("jira").borrow_mut().calls.clear();
    w.shell.take_events();
    w.host.mouse("jira", Travel::Back);
    w.shell.drain();
    assert!(calls(&w, "jira").is_empty());
    assert!(told(&w).is_empty());
}

// ------------------------------------------------------------- the keyboard

fn shell_focused(w: &World) -> usize {
    w.host
        .calls
        .borrow()
        .iter()
        .filter(|call| *call == "focus shell")
        .count()
}

fn site_focused(w: &World, source: &str) -> usize {
    calls(w, source).iter().filter(|call| *call == "focus").count()
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn back_in_the_window_with_no_site_holding_the_keyboard_knobas_takes_it() {
    // Cmd+Tab back on the Mac: the window's focus events are its own. Its
    // first responder may be nothing (a site view that held it went off
    // screen): ⌘K then reaches no web view until a click.
    let w = world();
    shown(&w, "jira", false, false).await;
    w.host.view("jira").borrow_mut().holds_keyboard = Some(false);
    w.sites.window_focused(false);
    w.sites.window_focused(true);
    assert_eq!(shell_focused(&w), 1);
    assert_eq!(site_focused(&w, "jira"), 0);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_site_clicked_into_takes_the_keyboard_back_with_the_window() {
    // Clicked into, never typed in: the view holds the keyboard all the same.
    let w = world();
    shown(&w, "jira", false, false).await;
    w.host.view("jira").borrow_mut().holds_keyboard = Some(true);
    w.sites.window_focused(false);
    w.sites.window_focused(true);
    assert_eq!(site_focused(&w, "jira"), 1);
    assert_eq!(shell_focused(&w), 0);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_focus_event_with_the_window_still_in_front_moves_no_keyboard() {
    // Windows: Tauri makes the window's focus events out of knobas's own web
    // view's. A click from knobas into the site is a "blur", the click back
    // onto knobas's bar a "focus": the site must not take the keyboard back
    // from the bar (the trap).
    let w = world();
    shown(&w, "jira", false, false).await;
    *w.host.active.borrow_mut() = Some(true);
    w.host.focus("jira", true);
    w.sites.window_focused(false);
    w.host.focus("jira", false);
    w.sites.window_focused(true);
    assert_eq!(site_focused(&w, "jira"), 0);
    assert_eq!(shell_focused(&w), 0);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn on_windows_a_site_that_loses_the_keyboard_to_another_app_takes_it_back() {
    // Alt+Tab away from a site: knobas's own web view had no focus to lose,
    // so only the site's controller says so. Alt+Tab back: wry gives the
    // keyboard to knobas's web view, whose focus is the window's.
    let w = world();
    shown(&w, "jira", false, false).await;
    *w.host.active.borrow_mut() = Some(true);
    w.host.focus("jira", true);
    *w.host.active.borrow_mut() = Some(false);
    w.host.focus("jira", false);
    *w.host.active.borrow_mut() = Some(true);
    w.sites.window_focused(true);
    assert_eq!(site_focused(&w, "jira"), 1);
    assert_eq!(shell_focused(&w), 0);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn on_windows_the_window_back_from_another_app_gives_knobas_the_keyboard() {
    let w = world();
    shown(&w, "jira", false, false).await;
    *w.host.active.borrow_mut() = Some(false);
    w.sites.window_focused(false);
    *w.host.active.borrow_mut() = Some(true);
    w.sites.window_focused(true);
    assert_eq!(shell_focused(&w), 1);
    assert_eq!(site_focused(&w, "jira"), 0);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_status_follows_the_keyboard_as_windows_reports_it() {
    let w = world();
    shown(&w, "jira", false, false).await;
    *w.host.active.borrow_mut() = Some(true);
    w.host.focus("jira", true);
    w.shell.drain();
    assert_eq!(
        w.sites.statuses()[0].keyboard,
        knobas_site::state::SiteKeyboard::Site
    );
    w.host.focus("jira", false);
    w.shell.drain();
    assert_eq!(
        w.sites.statuses()[0].keyboard,
        knobas_site::state::SiteKeyboard::Shell
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn hiding_a_site_that_holds_the_keyboard_without_a_key_gives_it_to_knobas() {
    // Clicked into, then left by the mouse's back button: no key ever said
    // the site had the keyboard, the view does.
    let w = world();
    shown(&w, "jira", false, false).await;
    w.host.view("jira").borrow_mut().holds_keyboard = Some(true);
    w.sites.hide("jira");
    w.shell.drain();
    assert_eq!(shell_focused(&w), 1);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn closing_a_site_that_holds_the_keyboard_without_a_key_gives_it_to_knobas() {
    let w = world();
    shown(&w, "jira", false, false).await;
    w.host.view("jira").borrow_mut().holds_keyboard = Some(true);
    w.sites.close("jira", true).await.unwrap();
    w.shell.drain();
    assert_eq!(shell_focused(&w), 1);
}
