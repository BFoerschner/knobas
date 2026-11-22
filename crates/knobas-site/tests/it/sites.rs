//! The controller on the fake platform (decisions §10, §14, §17, §24): every
//! command, hook and lifetime rule, with no window.

use std::collections::HashMap;
use std::sync::Arc;

use knobas_site::fake::{FakeHost, FakeShell, ProbeAnswer};
use knobas_site::host::ViewEvent;
use knobas_site::keys::{Key, Platform, Press};
use knobas_site::policy::{Base, OtherSite, SiteRules};
use knobas_site::sites::{
    SiteAct, SiteAsked, SiteAsks, SiteClosedWhy, SiteInsets, SiteRoutedTo, install_host,
};
use knobas_site::state::{IDLE_CLOSE_MS, SiteFailure, SitePhase};
use knobas_site::{Config, SiteError, SiteEvent, Sites};

struct World {
    sites: Sites,
    shell: Arc<FakeShell>,
    host: FakeHost,
    _dir: std::path::PathBuf,
}

fn rules(base: &str) -> SiteRules {
    SiteRules {
        base: Base::parse(base).unwrap(),
        allowed: vec![],
        others: vec![],
        confined: false,
    }
}

fn world() -> World {
    let shell = Arc::new(FakeShell::new());
    let host = FakeHost::default();
    install_host(Box::new(host.clone()));
    let dir = std::env::temp_dir().join(format!(
        "knobas-site-test-{}-{:?}",
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
            platform: Platform::Mac,
        },
    );
    let mut map = HashMap::new();
    let mut jira = rules("https://jira.example");
    jira.others = vec![OtherSite {
        source: "wiki".into(),
        base: Base::parse("https://wiki.example").unwrap(),
    }];
    map.insert("jira".to_owned(), jira);
    map.insert("wiki".to_owned(), rules("https://wiki.example"));
    map.insert("kuma".to_owned(), rules("https://kuma.example"));
    sites.set_rules(map);
    World {
        sites,
        shell,
        host,
        _dir: dir,
    }
}

const INSETS: SiteInsets = SiteInsets {
    top: 93.0,
    right: 0.0,
    bottom: 28.0,
    left: 0.0,
};

fn statuses(shell: &FakeShell) -> Vec<knobas_site::state::SiteStatus> {
    shell
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            SiteEvent::Status(status) => Some(status),
            _ => None,
        })
        .collect()
}

async fn open_live(w: &World, source: &str, url: &str) {
    assert_eq!(
        w.sites.open(source, None, INSETS, false).await,
        Ok(SiteAsked {
            done: true,
            asks: None
        })
    );
    w.host.report(source, ViewEvent::Committed(url.into()));
    w.shell.drain();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn opening_makes_a_hidden_view_at_the_base_and_shows_it_at_its_first_commit() {
    let w = world();
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    assert_eq!(w.host.calls.borrow()[0], "make jira https://jira.example/");
    let view = w.host.view("jira");
    assert!(!view.borrow().visible);
    w.sites.show("jira", INSETS, true, true);
    assert!(!view.borrow().visible, "nothing committed yet");
    let first = statuses(&w.shell);
    assert_eq!(first.last().unwrap().phase, SitePhase::Opening);
    w.host.report(
        "jira",
        ViewEvent::Committed("https://jira.example/secure/Dashboard.jspa".into()),
    );
    w.shell.drain();
    assert!(view.borrow().visible);
    let shown = statuses(&w.shell);
    let last = shown.last().unwrap();
    assert_eq!(last.phase, SitePhase::Live);
    assert!(last.shown && last.own);
    // The view sits in the rectangle: the window minus the insets.
    let bounds = view.borrow().bounds;
    assert_eq!(
        (bounds.y, bounds.height, bounds.width),
        (93.0, 868.0 - 93.0 - 28.0, 1440.0)
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn an_address_the_site_may_not_show_is_invalid_and_a_source_with_the_switch_off_is_not_offered()
 {
    let w = world();
    assert!(matches!(
        w.sites
            .open("jira", Some("https://evil.test/".into()), INSETS, false)
            .await,
        Err(SiteError::Invalid(_))
    ));
    assert!(
        matches!(
            w.sites
                .open("jira", Some("https://wiki.example/x".into()), INSETS, false)
                .await,
            Err(SiteError::Invalid(_))
        ),
        "another source's address is that source's site"
    );
    assert!(matches!(
        w.sites
            .open("jira", Some("javascript:alert(1)".into()), INSETS, false)
            .await,
        Err(SiteError::Invalid(_))
    ));
    assert_eq!(
        w.sites.open("gitea", None, INSETS, false).await,
        Err(SiteError::NotOffered)
    );
    assert!(w.host.calls.borrow().is_empty(), "nothing was made");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn hiding_keeps_the_page_and_showing_brings_it_back() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.sites.show("jira", INSETS, true, true);
    let view = w.host.view("jira");
    assert!(view.borrow().visible);
    w.sites.hide("jira");
    assert!(!view.borrow().visible);
    assert!(
        !view.borrow().calls.contains(&"dropped".to_owned()),
        "leaving is not closing"
    );
    w.sites.show("jira", INSETS, false, true);
    assert!(view.borrow().visible);
    assert_eq!(
        w.host
            .calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("make"))
            .count(),
        1
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn closing_a_clean_page_closes_at_once() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.host.answer("jira", ProbeAnswer::Clean);
    let view = w.host.view("jira");
    assert_eq!(
        w.sites.close("jira", false).await,
        Ok(SiteAsked {
            done: true,
            asks: None
        })
    );
    assert!(view.borrow().calls.contains(&"dropped".to_owned()));
    assert!(w.sites.statuses().is_empty());
    assert!(
        w.shell
            .take_events()
            .iter()
            .any(|e| matches!(e, SiteEvent::Closed(c) if c.why == SiteClosedWhy::Asked))
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn closing_a_page_with_changes_asks_and_closes_nothing_until_forced() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.host.answer("jira", ProbeAnswer::Changed);
    let view = w.host.view("jira");
    assert_eq!(
        w.sites.close("jira", false).await,
        Ok(SiteAsked {
            done: false,
            asks: Some(SiteAsks::Unsaved)
        })
    );
    assert!(!view.borrow().calls.contains(&"dropped".to_owned()));
    assert_eq!(
        w.sites.close("jira", true).await,
        Ok(SiteAsked {
            done: true,
            asks: None
        })
    );
    assert!(view.borrow().calls.contains(&"dropped".to_owned()));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn no_answer_in_300_ms_or_an_odd_one_asks() {
    for answer in [ProbeAnswer::Never, ProbeAnswer::Odd] {
        let w = world();
        open_live(&w, "jira", "https://jira.example/").await;
        w.host.answer("jira", answer);
        let started = tokio::time::Instant::now();
        assert_eq!(
            w.sites.close("jira", false).await,
            Ok(SiteAsked {
                done: false,
                asks: Some(SiteAsks::Unsaved)
            }),
            "{answer:?}"
        );
        if answer == ProbeAnswer::Never {
            assert_eq!(started.elapsed(), std::time::Duration::from_millis(300));
        }
        w.sites.shutdown();
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn reload_asks_the_page_first() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.host.answer("jira", ProbeAnswer::Changed);
    let view = w.host.view("jira");
    assert_eq!(
        w.sites.act("jira", SiteAct::Reload, false).await,
        Ok(SiteAsked {
            done: false,
            asks: Some(SiteAsks::Unsaved)
        })
    );
    assert_eq!(view.borrow().probes, 1, "the page was asked");
    assert!(!view.borrow().calls.contains(&"reload".to_owned()));
    w.sites.act("jira", SiteAct::Reload, true).await.unwrap();
    assert!(view.borrow().calls.contains(&"reload".to_owned()));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn another_address_for_an_open_site_asks_first_and_then_loads_in_the_same_view() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.host.answer("jira", ProbeAnswer::Changed);
    let asked = w
        .sites
        .open(
            "jira",
            Some("https://jira.example/browse/PAY-1".into()),
            INSETS,
            false,
        )
        .await;
    assert_eq!(
        asked,
        Ok(SiteAsked {
            done: false,
            asks: Some(SiteAsks::Unsaved)
        })
    );
    w.sites
        .open(
            "jira",
            Some("https://jira.example/browse/PAY-1".into()),
            INSETS,
            true,
        )
        .await
        .unwrap();
    let view = w.host.view("jira");
    assert!(
        view.borrow()
            .calls
            .contains(&"load https://jira.example/browse/PAY-1".to_owned())
    );
    assert_eq!(
        w.host
            .calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("make"))
            .count(),
        1,
        "one view per source"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_redirect_to_an_unknown_host_while_landing_asks_and_allowing_lands_again() {
    let w = world();
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    w.sites.show("jira", INSETS, true, true);
    assert!(w.host.navigate("jira", "https://jira.example/login"));
    assert!(
        !w.host
            .navigate("jira", "https://idp.example/sso?SAMLRequest=x")
    );
    w.shell.drain();
    let status = statuses(&w.shell).pop().unwrap();
    assert_eq!(status.phase, SitePhase::Asks);
    assert_eq!(status.asks_origin.as_deref(), Some("https://idp.example"));
    assert!(matches!(
        w.sites.answer("jira", "https://other.example", true),
        Err(SiteError::Invalid(_))
    ));
    assert_eq!(
        w.sites.answer("jira", "https://idp.example", true),
        Ok(None)
    );
    let view = w.host.view("jira");
    assert!(
        view.borrow()
            .calls
            .contains(&"load https://jira.example/".to_owned()),
        "the landing again"
    );
    assert!(
        w.host
            .navigate("jira", "https://idp.example/sso?SAMLRequest=x"),
        "now allowed"
    );
    assert!(
        w.sites
            .rules("jira")
            .unwrap()
            .allowed
            .iter()
            .any(|o| o.host == "idp.example")
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn declining_with_nothing_shown_closes_and_hands_the_address_to_the_browser() {
    let w = world();
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    assert!(!w.host.navigate("jira", "https://idp.example/sso"));
    w.shell.drain();
    assert_eq!(
        w.sites.answer("jira", "https://idp.example", false),
        Ok(Some("https://idp.example/sso".into()))
    );
    assert!(w.sites.statuses().is_empty());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn after_landing_an_unknown_host_goes_to_the_browser_and_another_sources_to_its_site() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.shell.take_events();
    assert!(!w.host.navigate("jira", "https://news.example/a"));
    assert!(!w.host.navigate("jira", "https://wiki.example/display/PAY"));
    assert!(!w.host.navigate("jira", "file:///etc/passwd"));
    let routed: Vec<_> = w
        .shell
        .take_events()
        .into_iter()
        .filter_map(|e| match e {
            SiteEvent::Routed(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(routed[0].to, SiteRoutedTo::Browser);
    assert_eq!(routed[0].url, "https://news.example/a");
    assert_eq!(routed[1].to, SiteRoutedTo::Site);
    assert_eq!(routed[1].other.as_deref(), Some("wiki"));
    assert_eq!(routed[2].to, SiteRoutedTo::Refused);
    assert_eq!(routed[2].host, "file");
    assert_eq!(
        routed[2].url, "",
        "a refused address goes nowhere, not even to the page"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_new_window_loads_in_place_when_allowed_and_opens_none() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    let hooks_view = w.host.view("jira");
    let new_window = hooks_view.borrow().hooks.clone().unwrap().new_window;
    new_window("https://jira.example/browse/PAY-2");
    w.shell.drain();
    assert!(
        hooks_view
            .borrow()
            .calls
            .contains(&"load https://jira.example/browse/PAY-2".to_owned())
    );
    assert_eq!(
        w.host
            .calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("make"))
            .count(),
        1
    );
}

fn esc() -> Press {
    Press {
        key: Some(Key::Escape),
        ..Press::default()
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn esc_twice_and_claimed_chords_reach_the_shell_by_name() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.sites.show("jira", INSETS, true, true);
    w.shell.take_events();
    w.shell.set_now(1000);
    assert!(
        w.host.key("jira", &esc()).pass,
        "the first Esc is the site's"
    );
    w.shell.set_now(1300);
    assert!(!w.host.key("jira", &esc()).pass, "the second is knobas's");
    let cmd_k = Press {
        key: Some(Key::Char('k')),
        primary: true,
        ..Press::default()
    };
    assert!(!w.host.key("jira", &cmd_k).pass);
    let keys: Vec<String> = w
        .shell
        .take_events()
        .into_iter()
        .filter_map(|e| match e {
            SiteEvent::Key(k) => Some(k.key),
            _ => None,
        })
        .collect();
    assert_eq!(keys, ["esc-armed", "esc-leave", "search"]);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn with_no_opener_esc_is_the_sites_alone() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.sites.show("jira", INSETS, true, false);
    assert!(w.host.key("jira", &esc()).pass);
    assert!(w.host.key("jira", &esc()).pass);
    assert!(
        !w.shell
            .take_events()
            .iter()
            .any(|e| matches!(e, SiteEvent::Key(_)))
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn cmd_w_in_the_shell_closes_the_shown_site_and_cmd_q_quits_through_the_sites() {
    let w = world();
    let cmd_w = Press {
        key: Some(Key::Char('w')),
        primary: true,
        ..Press::default()
    };
    let cmd_q = Press {
        key: Some(Key::Char('q')),
        primary: true,
        ..Press::default()
    };
    assert!(!w.sites.shell_key(&cmd_w), "no site: the menu's own");
    open_live(&w, "jira", "https://jira.example/").await;
    assert!(!w.sites.shell_key(&cmd_w), "no site shown");
    w.sites.show("jira", INSETS, false, true);
    assert!(w.sites.shell_key(&cmd_w));
    assert!(w.sites.shell_key(&cmd_q));
    w.shell.drain();
    assert_eq!(w.shell.quits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_bound_closes_the_one_shown_longest_ago_and_never_one_that_objects() {
    let w = world();
    w.sites.set_kept(2);
    w.shell.set_now(10);
    open_live(&w, "jira", "https://jira.example/").await;
    w.sites.show("jira", INSETS, false, true);
    w.shell.set_now(20);
    w.sites.hide("jira");
    w.shell.set_now(30);
    open_live(&w, "wiki", "https://wiki.example/").await;
    w.sites.show("wiki", INSETS, false, true);
    w.shell.set_now(40);
    w.sites.hide("wiki");
    // jira holds changes: wiki is the one to go, though shown later.
    w.host.answer("jira", ProbeAnswer::Changed);
    w.shell.set_now(50);
    open_live(&w, "kuma", "https://kuma.example/").await;
    let alive: Vec<String> = w.sites.statuses().into_iter().map(|s| s.source).collect();
    assert_eq!(alive, ["jira", "kuma"]);
    // Both object: the count goes over.
    w.host.answer("jira", ProbeAnswer::Changed);
    w.host.answer("kuma", ProbeAnswer::Changed);
    w.sites.hide("kuma");
    open_live(&w, "wiki", "https://wiki.example/").await;
    assert_eq!(w.sites.statuses().len(), 3);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_bound_closes_the_oldest_clean_one() {
    let w = world();
    w.sites.set_kept(1);
    w.shell.set_now(10);
    open_live(&w, "jira", "https://jira.example/").await;
    w.shell.set_now(30);
    open_live(&w, "wiki", "https://wiki.example/").await;
    let alive: Vec<String> = w.sites.statuses().into_iter().map(|s| s.source).collect();
    assert_eq!(
        alive,
        ["wiki"],
        "the newest is the one just opened, never closed for room"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn two_hours_hidden_closes_a_clean_site_and_never_one_that_objects() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    open_live(&w, "wiki", "https://wiki.example/").await;
    w.sites.hide("jira");
    w.sites.hide("wiki");
    w.host.answer("wiki", ProbeAnswer::Changed);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    w.shell.set_now(IDLE_CLOSE_MS as u64 + 10);
    w.sites.tick().await;
    let alive: Vec<String> = w.sites.statuses().into_iter().map(|s| s.source).collect();
    assert_eq!(alive, ["wiki"]);
    assert!(
        w.host
            .view("jira")
            .borrow()
            .calls
            .contains(&"dropped".to_owned())
    );
    assert!(
        w.host
            .view("wiki")
            .borrow()
            .calls
            .contains(&"sleep".to_owned()),
        "a hidden site sleeps"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_landing_that_commits_nothing_in_ten_seconds_fails() {
    let w = world();
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    w.shell.set_now(10_000);
    w.sites.tick().await;
    assert_eq!(w.sites.statuses()[0].phase, SitePhase::Failed);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_last_address_survives_a_close_and_not_a_forget() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.host.report(
        "jira",
        ViewEvent::Committed("https://jira.example/browse/PAY-9".into()),
    );
    w.shell.drain();
    w.sites.close("jira", true).await.unwrap();
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    assert!(
        w.host
            .calls
            .borrow()
            .contains(&"make jira https://jira.example/browse/PAY-9".to_owned())
    );
    w.sites.forget("jira", true).await.unwrap();
    assert!(w.host.calls.borrow().contains(&"forget jira".to_owned()));
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    assert_eq!(
        w.host.calls.borrow().last().unwrap(),
        "make jira https://jira.example/"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_forget_that_cannot_delete_yet_goes_on_the_list() {
    let w = world();
    *w.host.forget_holds.borrow_mut() = true;
    w.sites.forget("jira", true).await.unwrap();
    let list = w._dir.join("sites").join("forget.list");
    assert_eq!(std::fs::read_to_string(&list).unwrap(), "jira\n");
    assert!(matches!(
        w.sites.forget("../x", true).await,
        Err(SiteError::Invalid(_))
    ));
    std::fs::remove_dir_all(&w._dir).unwrap();
}

/// WebKit holds a store for a moment after its last view closed: *Forget…*
/// tries again until it is gone, and lists nothing (ux30, measured on the
/// Mac: "data store is currently opened" right after the close).
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_forget_tries_again_while_the_store_is_held_for_a_moment() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    *w.host.forget_held_for.borrow_mut() = 2;
    let started = tokio::time::Instant::now();
    w.sites.forget("jira", true).await.unwrap();
    assert_eq!(
        started.elapsed(),
        knobas_site::sites::FORGET_AGAIN * 2,
        "a pause before each try after the first, and none before it"
    );
    let tries = w
        .host
        .calls
        .borrow()
        .iter()
        .filter(|c| *c == "forget jira")
        .count();
    assert_eq!(tries, 3, "held twice, gone the third time");
    assert!(!w._dir.join("sites").join("forget.list").exists());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn quitting_asks_only_the_pages_that_object() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    open_live(&w, "wiki", "https://wiki.example/").await;
    w.host.answer("wiki", ProbeAnswer::Changed);
    let unsaved: Vec<String> = w
        .sites
        .unsaved()
        .await
        .into_iter()
        .map(|s| s.source)
        .collect();
    assert_eq!(unsaved, ["wiki"]);
    w.host.answer("jira", ProbeAnswer::Never);
    let unsaved = w.sites.unsaved().await;
    assert_eq!(
        unsaved.len(),
        2,
        "no answer in 300 ms counts as changes, and does not hang"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_download_is_saved_under_downloads_with_an_id_knobas_gave() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    let (path, id) = w.host.download("jira", "../../evil.exe").unwrap();
    assert_eq!(path, w._dir.join("Downloads").join("evil.exe"));
    let id = u32::try_from(id).unwrap();
    assert_eq!(w.sites.download_path(id), Ok(path));
    assert!(matches!(
        w.sites.download_path(999),
        Err(SiteError::Invalid(_))
    ));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_title_is_cleaned_and_the_address_follows_a_push_state() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.host
        .report("jira", ViewEvent::Title("PAY-1 <b>x</b>\n\u{202E}".into()));
    w.host.report(
        "jira",
        ViewEvent::History {
            url: Some("https://jira.example/browse/PAY-1?focused=1".into()),
            can_back: true,
            can_forward: false,
            loading: None,
        },
    );
    w.shell.drain();
    let status = &w.sites.statuses()[0];
    assert_eq!(status.title, "PAY-1 <b>x</b>");
    assert_eq!(
        status.url.as_deref(),
        Some("https://jira.example/browse/PAY-1?focused=1")
    );
    assert!(status.can_back);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_process_that_ends_stops_the_site_and_hides_it() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.sites.show("jira", INSETS, false, true);
    w.host.report("jira", ViewEvent::Ended);
    w.shell.drain();
    assert_eq!(w.sites.statuses()[0].phase, SitePhase::Stopped);
    assert!(!w.host.view("jira").borrow().visible);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn zoom_reaches_every_live_view() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    w.sites.zoom(1.25);
    assert!(
        w.host
            .view("jira")
            .borrow()
            .calls
            .contains(&"zoom 1.25".to_owned())
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_first_own_page_marks_the_sign_in_kept_once() {
    let w = world();
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    w.sites.answer("jira", "https://idp.example", true).ok();
    w.host
        .report("jira", ViewEvent::Committed("https://jira.example/".into()));
    w.host.report(
        "jira",
        ViewEvent::Committed("https://jira.example/browse/A-1".into()),
    );
    assert_eq!(*w.shell.landed.lock().unwrap(), ["jira"]);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_keyboard_asked_for_before_the_first_commit_goes_to_the_site_when_it_shows() {
    let w = world();
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    w.sites.show("jira", INSETS, true, true);
    let view = w.host.view("jira");
    assert!(
        !view.borrow().calls.contains(&"focus".to_owned()),
        "nothing on screen to focus"
    );
    w.host
        .report("jira", ViewEvent::Committed("https://jira.example/".into()));
    w.shell.drain();
    let calls = view.borrow().calls.clone();
    assert_eq!(calls.iter().filter(|c| *c == "focus").count(), 1);
    let show_at = calls.iter().position(|c| c == "show").unwrap();
    let focus_at = calls.iter().position(|c| c == "focus").unwrap();
    assert!(show_at < focus_at, "shown, then focused: {calls:?}");
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn leave_anyway_answers_the_pages_own_leave_dialog_and_nothing_else_does() {
    let w = world();
    open_live(&w, "jira", "https://jira.example/").await;
    let view = w.host.view("jira");
    // Clean: no question, and the page's own dialog is not pre-answered.
    w.sites
        .open("jira", Some("https://jira.example/a".into()), INSETS, false)
        .await
        .unwrap();
    w.sites.act("jira", SiteAct::Reload, false).await.unwrap();
    assert!(!view.borrow().calls.contains(&"leave freely".to_owned()));
    // The person said "anyway": answered once, before the load and before the reload.
    w.sites
        .open("jira", Some("https://jira.example/b".into()), INSETS, true)
        .await
        .unwrap();
    w.sites.act("jira", SiteAct::Reload, true).await.unwrap();
    let calls = view.borrow().calls.clone();
    let at = |what: &str| calls.iter().position(|c| c == what).unwrap();
    assert!(
        at("leave freely") < at("load https://jira.example/b"),
        "{calls:?}"
    );
    assert_eq!(
        calls.iter().filter(|c| *c == "leave freely").count(),
        2,
        "{calls:?}"
    );
    assert_eq!(calls.last().map(String::as_str), Some("reload"));
}

// -- the saved web sign-in (ADR-0050, decisions §13 (b), ux29) ---------------

fn sign_ins(w: &World, saved: bool) {
    use knobas_site::sign_in::SignInRule;
    let mut map = HashMap::new();
    map.insert(
        "jira".to_owned(),
        SignInRule {
            saved,
            paths: vec!["/login.jsp".into()],
            any_page: false,
        },
    );
    map.insert(
        "kuma".to_owned(),
        SignInRule {
            saved,
            paths: vec![],
            any_page: true,
        },
    );
    w.sites.set_sign_ins(map);
    w.shell.drain();
}

fn asked(w: &World) -> Vec<String> {
    std::mem::take(&mut *w.shell.sign_ins.lock().unwrap())
}

fn last_status(w: &World, source: &str) -> knobas_site::state::SiteStatus {
    w.sites
        .statuses()
        .into_iter()
        .find(|status| status.source == source)
        .expect("the site is open")
}

/// What the shell does when asked: hand over the saved sign-in.
fn hand_over(w: &World, source: &str) {
    w.sites
        .fill_with(source, "bjorn".into(), "s3cr3t-Pa55".into());
    w.shell.drain();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_declared_sign_in_page_is_filled_by_itself_once_per_landing() {
    use knobas_site::scripts::FillMode;
    use knobas_site::sign_in::SiteSignIn;
    let w = world();
    sign_ins(&w, true);
    open_live(&w, "jira", "https://jira.example/login.jsp").await;
    assert!(asked(&w).is_empty(), "a commit alone is no loaded page");
    w.host.report(
        "jira",
        ViewEvent::Finished("https://jira.example/login.jsp".into()),
    );
    w.shell.drain();
    assert_eq!(asked(&w), ["jira"]);
    // The same page reported loaded again asks nothing more.
    w.host.report(
        "jira",
        ViewEvent::Finished("https://jira.example/login.jsp".into()),
    );
    w.shell.drain();
    assert!(asked(&w).is_empty(), "never twice per landing");
    let view = w.host.view("jira");
    view.borrow_mut()
        .fill_answers
        .push_back("\"filled\"".into());
    hand_over(&w, "jira");
    assert_eq!(view.borrow().sign_ins.len(), 1);
    assert_eq!(view.borrow().sign_ins[0].mode, FillMode::Fill);
    assert_eq!(view.borrow().sign_ins[0].username, "bjorn");
    assert_eq!(last_status(&w, "jira").sign_in, Some(SiteSignIn::Filling));
    // The site answers with its sign-in page again: not taken.
    w.host.report(
        "jira",
        ViewEvent::Committed("https://jira.example/login.jsp?os_error=1".into()),
    );
    w.host.report(
        "jira",
        ViewEvent::Finished("https://jira.example/login.jsp?os_error=1".into()),
    );
    w.shell.drain();
    view.borrow_mut().fill_answers.push_back("\"form\"".into());
    w.shell.set_now(1000);
    w.sites.tick().await;
    w.shell.drain();
    assert_eq!(view.borrow().sign_ins.last().unwrap().mode, FillMode::Look);
    assert!(
        view.borrow().sign_ins.last().unwrap().password.is_empty(),
        "a look carries no password"
    );
    assert_eq!(last_status(&w, "jira").sign_in, Some(SiteSignIn::NotTaken));
    assert!(
        asked(&w).is_empty(),
        "one try: nothing more until he presses"
    );
    // He presses Fill sign-in.
    assert!(last_status(&w, "jira").fill);
    w.sites.press_fill("jira").unwrap();
    w.shell.drain();
    assert_eq!(asked(&w), ["jira"]);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_taken_sign_in_clears_the_bar() {
    let w = world();
    sign_ins(&w, true);
    open_live(&w, "jira", "https://jira.example/login.jsp").await;
    w.host.report(
        "jira",
        ViewEvent::Finished("https://jira.example/login.jsp".into()),
    );
    w.shell.drain();
    let view = w.host.view("jira");
    view.borrow_mut()
        .fill_answers
        .push_back("\"filled\"".into());
    hand_over(&w, "jira");
    w.host.report(
        "jira",
        ViewEvent::Committed("https://jira.example/secure/Dashboard.jspa".into()),
    );
    w.host.report(
        "jira",
        ViewEvent::Finished("https://jira.example/secure/Dashboard.jspa".into()),
    );
    w.shell.drain();
    w.shell.set_now(400);
    w.sites.tick().await;
    w.shell.drain();
    let status = last_status(&w, "jira");
    assert_eq!(status.sign_in, None);
    assert!(status.fill);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn the_fill_is_refused_natively_off_the_own_origin_in_a_redirect_and_with_nothing_saved() {
    let w = world();
    sign_ins(&w, false);
    open_live(&w, "jira", "https://jira.example/login.jsp").await;
    w.host.report(
        "jira",
        ViewEvent::Finished("https://jira.example/login.jsp".into()),
    );
    w.shell.drain();
    assert!(asked(&w).is_empty(), "nothing saved, nothing asked");
    assert!(!last_status(&w, "jira").fill);
    assert!(matches!(
        w.sites.press_fill("jira"),
        Err(SiteError::Invalid(_))
    ));
    // Even if a sign-in reached it, nothing saved is nothing typed.
    hand_over(&w, "jira");
    assert!(w.host.view("jira").borrow().sign_ins.is_empty());

    sign_ins(&w, true);
    let view = w.host.view("jira");
    // In a redirect: committed on its own origin, the engine already
    // elsewhere.
    view.borrow_mut().url = Some("https://sso.example/login".into());
    hand_over(&w, "jira");
    assert!(
        view.borrow().sign_ins.is_empty(),
        "never into a page in a redirect"
    );
    // On an allowed address: a provider's page is the provider's.
    let mut map = HashMap::new();
    let mut jira = rules("https://jira.example");
    jira.allowed = vec![knobas_site::policy::Origin::parse("https://sso.example").unwrap()];
    map.insert("jira".to_owned(), jira);
    w.sites.set_rules(map);
    w.host.report(
        "jira",
        ViewEvent::Committed("https://sso.example/login".into()),
    );
    w.shell.drain();
    hand_over(&w, "jira");
    assert!(
        view.borrow().sign_ins.is_empty(),
        "never on an allowed origin"
    );
    let status = last_status(&w, "jira");
    assert!(!status.fill);
    assert_eq!(
        status.sign_in,
        Some(knobas_site::sign_in::SiteSignIn::Provider)
    );
    assert!(matches!(
        w.sites.press_fill("jira"),
        Err(SiteError::Invalid(_))
    ));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_form_on_any_page_is_looked_for_while_the_page_draws_it() {
    use knobas_site::sign_in::LOOK_EVERY_MS;
    let w = world();
    sign_ins(&w, true);
    open_live(&w, "kuma", "https://kuma.example/dashboard").await;
    w.host.report(
        "kuma",
        ViewEvent::Finished("https://kuma.example/dashboard".into()),
    );
    w.shell.drain();
    assert_eq!(asked(&w), ["kuma"]);
    let view = w.host.view("kuma");
    // The dialog is not drawn yet.
    hand_over(&w, "kuma");
    assert_eq!(view.borrow().sign_ins.len(), 1);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    w.shell.set_now(LOOK_EVERY_MS as u64);
    w.sites.tick().await;
    w.shell.drain();
    assert_eq!(asked(&w), ["kuma"], "looked for again");
    view.borrow_mut()
        .fill_answers
        .push_back("\"filled\"".into());
    hand_over(&w, "kuma");
    assert_eq!(view.borrow().sign_ins.len(), 2);
}

/// No log line holds the password, at every level, through a whole fill:
/// asked, handed over, typed, answered, judged.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn no_log_line_holds_the_password() {
    use std::io::Write;
    #[derive(Clone, Default)]
    struct Lines(Arc<std::sync::Mutex<Vec<u8>>>);
    impl Write for Lines {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let lines = Lines::default();
    let writer = lines.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let w = world();
    sign_ins(&w, true);
    open_live(&w, "jira", "https://jira.example/login.jsp").await;
    w.host.report(
        "jira",
        ViewEvent::Finished("https://jira.example/login.jsp".into()),
    );
    w.shell.drain();
    let view = w.host.view("jira");
    view.borrow_mut()
        .fill_answers
        .push_back("\"filled\"".into());
    hand_over(&w, "jira");
    w.host.report(
        "jira",
        ViewEvent::Finished("https://jira.example/login.jsp".into()),
    );
    w.shell.drain();
    w.shell.set_now(1000);
    w.sites.tick().await;
    w.shell.drain();
    // Refused fills log too.
    view.borrow_mut().url = Some("https://sso.example/".into());
    hand_over(&w, "jira");
    let text = String::from_utf8(lines.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("site sign-in"), "the fill was logged: {text}");
    assert!(
        !text.contains("s3cr3t-Pa55"),
        "a log line holds the password:\n{text}"
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn quitting_drops_every_view_then_waits_for_the_sessions_ahead() {
    let w = world();
    // Nothing alive: nothing to wait for.
    w.sites.shutdown();
    assert!(!w.host.calls.borrow().iter().any(|c| c.starts_with("flush")));
    open_live(&w, "jira", "https://jira.example/").await;
    let view = w.host.view("jira");
    w.sites.shutdown();
    assert!(view.borrow().calls.contains(&"dropped".to_owned()));
    assert!(w.sites.statuses().is_empty());
    assert_eq!(
        w.host
            .calls
            .borrow()
            .iter()
            .filter(|c| c.starts_with("flush"))
            .collect::<Vec<_>>(),
        ["flush 0 ahead=true"]
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_shell_with_no_saved_sign_in_turns_the_button_off() {
    let w = world();
    sign_ins(&w, true);
    open_live(&w, "jira", "https://jira.example/secure/Dashboard.jspa").await;
    assert!(last_status(&w, "jira").fill);
    w.sites.press_fill("jira").unwrap();
    w.shell.drain();
    w.sites.fill_none("jira");
    w.shell.drain();
    let status = last_status(&w, "jira");
    assert!(!status.fill, "nothing is saved after all");
    assert_eq!(status.sign_in, None);
    assert!(matches!(
        w.sites.press_fill("jira"),
        Err(SiteError::Invalid(_))
    ));
}

/// A page on another origin, a provider's, judges nothing: the fill is
/// judged when the site is back on its own page.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_provider_page_after_a_fill_judges_nothing() {
    use knobas_site::sign_in::SiteSignIn;
    let w = world();
    sign_ins(&w, true);
    let mut map = HashMap::new();
    let mut jira = rules("https://jira.example");
    jira.allowed = vec![knobas_site::policy::Origin::parse("https://sso.example").unwrap()];
    map.insert("jira".to_owned(), jira);
    w.sites.set_rules(map);
    open_live(&w, "jira", "https://jira.example/login.jsp").await;
    w.host.report(
        "jira",
        ViewEvent::Finished("https://jira.example/login.jsp".into()),
    );
    w.shell.drain();
    let view = w.host.view("jira");
    view.borrow_mut()
        .fill_answers
        .push_back("\"filled\"".into());
    hand_over(&w, "jira");
    w.host.report(
        "jira",
        ViewEvent::Committed("https://sso.example/mfa".into()),
    );
    w.host.report(
        "jira",
        ViewEvent::Finished("https://sso.example/mfa".into()),
    );
    w.shell.drain();
    w.shell.set_now(400);
    w.sites.tick().await;
    w.shell.drain();
    assert_eq!(
        view.borrow().sign_ins.len(),
        1,
        "no look on the provider's page"
    );
    assert_eq!(
        last_status(&w, "jira").sign_in,
        Some(SiteSignIn::Filling),
        "still waiting to judge the fill on the source's own page"
    );
}

// ---- a confined site (ux30): a password manager's loads nothing from other websites

fn vault_world() -> World {
    let w = world();
    let mut map = HashMap::new();
    let mut vault = rules("https://vault.example:8443");
    vault.confined = true;
    vault.allowed = vec![knobas_site::policy::Origin::parse("https://sso.example").unwrap()];
    map.insert("vault".to_owned(), vault);
    map.insert("jira".to_owned(), rules("https://jira.example"));
    w.sites.set_rules(map);
    w
}

fn confined_origins(w: &World, source: &str) -> Option<Vec<String>> {
    w.host
        .view(source)
        .borrow()
        .confine
        .as_ref()
        .map(|confine| confine.origins().map(ToString::to_string).collect())
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_confined_sources_view_is_made_with_its_own_and_allowed_origins_and_others_without() {
    let w = vault_world();
    w.sites.open("vault", None, INSETS, false).await.unwrap();
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    assert_eq!(
        confined_origins(&w, "vault"),
        Some(vec![
            "https://vault.example:8443".to_owned(),
            "https://sso.example".to_owned()
        ])
    );
    assert_eq!(confined_origins(&w, "jira"), None);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_changed_allow_list_reaches_the_live_confined_view_and_an_unchanged_one_sends_nothing() {
    let w = vault_world();
    open_live(&w, "vault", "https://vault.example:8443/").await;
    open_live(&w, "jira", "https://jira.example/").await;
    let mut map = HashMap::new();
    let mut vault = rules("https://vault.example:8443");
    vault.confined = true;
    vault.allowed = vec![knobas_site::policy::Origin::parse("https://sso.example").unwrap()];
    map.insert("vault".to_owned(), vault.clone());
    let mut jira = rules("https://jira.example");
    jira.allowed = vec![knobas_site::policy::Origin::parse("https://cdn.example").unwrap()];
    map.insert("jira".to_owned(), jira);
    w.sites.set_rules(map.clone());
    w.shell.drain();
    let calls = |source: &str| -> Vec<String> {
        w.host
            .view(source)
            .borrow()
            .calls
            .iter()
            .filter(|call| call.starts_with("confine"))
            .cloned()
            .collect()
    };
    assert!(calls("vault").is_empty(), "the same rules again");
    assert!(calls("jira").is_empty(), "an unconfined site's allow-list");
    vault.allowed.clear();
    map.insert("vault".to_owned(), vault);
    w.sites.set_rules(map);
    w.shell.drain();
    assert_eq!(calls("vault"), ["confine [https://vault.example:8443]"]);
    assert_eq!(
        confined_origins(&w, "vault"),
        Some(vec!["https://vault.example:8443".to_owned()])
    );
}

/// The `make` lines of the fake host, for `source`.
fn makes(w: &World, source: &str) -> Vec<String> {
    let prefix = format!("make {source} ");
    w.host
        .calls
        .borrow()
        .iter()
        .filter(|call| call.starts_with(&prefix))
        .cloned()
        .collect()
}

/// `vault`'s rules with `allowed`, beside an unconfined Jira whose
/// allow-list changes too.
fn vault_rules(allowed: &[&str]) -> HashMap<String, SiteRules> {
    let mut vault = rules("https://vault.example:8443");
    vault.confined = true;
    vault.allowed = allowed
        .iter()
        .map(|text| knobas_site::policy::Origin::parse(text).unwrap())
        .collect();
    let mut jira = rules("https://jira.example");
    jira.allowed = vec![knobas_site::policy::Origin::parse("https://cdn.example").unwrap()];
    HashMap::from([("vault".to_owned(), vault), ("jira".to_owned(), jira)])
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn where_a_view_takes_its_confinement_only_when_made_a_changed_allow_list_makes_it_again_where_it_was()
 {
    // Windows: WebView2 reads a confined site's proxy and WebRTC arguments
    // when its browser starts (Fable's WebRTC ruling, 2026-11-22).
    let w = vault_world();
    *w.host.remakes.borrow_mut() = true;
    open_live(
        &w,
        "vault",
        "https://vault.example:8443/WebClient/Main?itemId=5f0c",
    )
    .await;
    open_live(&w, "jira", "https://jira.example/").await;
    let old = w.host.view("vault");
    w.sites.set_rules(vault_rules(&[]));
    w.shell.drain();
    assert!(
        !old.borrow()
            .calls
            .iter()
            .any(|call| call.starts_with("confine")),
        "the old view is not told: {:?}",
        old.borrow().calls
    );
    assert_eq!(
        old.borrow().calls.last().map(String::as_str),
        Some("dropped")
    );
    assert_eq!(
        makes(&w, "vault"),
        [
            "make vault https://vault.example:8443/",
            "make vault https://vault.example:8443/WebClient/Main?itemId=5f0c"
        ]
    );
    assert_eq!(
        confined_origins(&w, "vault"),
        Some(vec!["https://vault.example:8443".to_owned()])
    );
    // An unconfined site's allow-list makes nothing again.
    assert_eq!(makes(&w, "jira").len(), 1);
    // The same rules again: nothing.
    w.sites.set_rules(vault_rules(&[]));
    w.shell.drain();
    assert_eq!(makes(&w, "vault").len(), 2);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn where_a_view_takes_its_confinement_only_when_made_allowing_while_landing_makes_it_again_at_the_landing()
 {
    let w = vault_world();
    *w.host.remakes.borrow_mut() = true;
    w.sites.open("vault", None, INSETS, false).await.unwrap();
    w.sites.show("vault", INSETS, true, true);
    assert!(!w.host.navigate("vault", "https://idp.example/sso"));
    w.shell.drain();
    assert_eq!(
        w.sites.answer("vault", "https://idp.example", true),
        Ok(None)
    );
    w.shell.drain();
    assert_eq!(
        makes(&w, "vault"),
        [
            "make vault https://vault.example:8443/",
            "make vault https://vault.example:8443/"
        ]
    );
    assert_eq!(
        confined_origins(&w, "vault"),
        Some(vec![
            "https://vault.example:8443".to_owned(),
            "https://sso.example".to_owned(),
            "https://idp.example".to_owned()
        ])
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_site_closed_or_made_again_while_its_remake_waits_is_not_made_twice() {
    let w = vault_world();
    *w.host.remakes.borrow_mut() = true;
    open_live(&w, "vault", "https://vault.example:8443/").await;
    w.sites.set_rules(vault_rules(&[]));
    // Closed before the remake runs: nothing is made for it.
    w.sites.close("vault", true).await.unwrap();
    w.shell.drain();
    assert_eq!(makes(&w, "vault").len(), 1);
    // Retry made it before the remake ran: it is not made a second time.
    open_live(&w, "vault", "https://vault.example:8443/").await;
    w.sites.set_rules(vault_rules(&["https://idp.example"]));
    w.sites.act("vault", SiteAct::Reload, true).await.unwrap();
    w.shell.drain();
    assert_eq!(makes(&w, "vault").len(), 3);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_view_that_cannot_be_made_again_loads_nothing_says_so_and_retry_makes_it() {
    let w = vault_world();
    *w.host.remakes.borrow_mut() = true;
    open_live(&w, "vault", "https://vault.example:8443/WebClient/Main").await;
    *w.host.fail.borrow_mut() = Some("the session folder is still in use".into());
    w.sites.set_rules(vault_rules(&[]));
    w.shell.drain();
    let status = w
        .sites
        .statuses()
        .into_iter()
        .find(|status| status.source == "vault")
        .expect("the site stays");
    assert_eq!(
        status.failure,
        Some(SiteFailure::Other(
            knobas_site::confine::NOT_CONFINED.to_owned()
        ))
    );
    *w.host.fail.borrow_mut() = None;
    w.sites.act("vault", SiteAct::Reload, false).await.unwrap();
    w.shell.drain();
    assert_eq!(
        makes(&w, "vault").last().map(String::as_str),
        Some("make vault https://vault.example:8443/WebClient/Main")
    );
    assert_eq!(
        confined_origins(&w, "vault"),
        Some(vec!["https://vault.example:8443".to_owned()])
    );
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn allowing_an_origin_on_a_confined_site_confines_it_anew_before_the_landing_loads_again() {
    let w = vault_world();
    w.sites.open("vault", None, INSETS, false).await.unwrap();
    w.sites.show("vault", INSETS, true, true);
    assert!(!w.host.navigate("vault", "https://idp.example/sso"));
    w.shell.drain();
    assert_eq!(
        w.sites.answer("vault", "https://idp.example", true),
        Ok(None)
    );
    w.shell.drain();
    let calls = w.host.view("vault").borrow().calls.clone();
    let confine = calls
        .iter()
        .position(|call| {
            call == "confine [https://vault.example:8443 https://sso.example https://idp.example]"
        })
        .expect("confined anew");
    let load = calls
        .iter()
        .position(|call| call == "load https://vault.example:8443/")
        .expect("the landing again");
    assert!(confine < load, "{calls:?}");
    // An unconfined site's answer confines nothing.
    let w = world();
    w.sites.open("jira", None, INSETS, false).await.unwrap();
    w.sites.show("jira", INSETS, true, true);
    assert!(!w.host.navigate("jira", "https://idp.example/sso"));
    w.shell.drain();
    assert_eq!(
        w.sites.answer("jira", "https://idp.example", true),
        Ok(None)
    );
    w.shell.drain();
    assert!(
        !w.host
            .view("jira")
            .borrow()
            .calls
            .iter()
            .any(|call| call.starts_with("confine"))
    );
}
