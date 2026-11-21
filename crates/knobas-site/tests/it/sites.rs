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
use knobas_site::state::{IDLE_CLOSE_MS, SitePhase};
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
