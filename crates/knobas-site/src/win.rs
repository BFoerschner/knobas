//! Windows: what knobas sets on a site's WebView2 controller after wry made
//! it (decisions §22).
//!
//! - WebView2's own channel to the host and host objects, off; password
//!   saving off (autofill is off on the builder).
//! - Keys: `AcceleratorKeyPressed`, raised for every chord, every key that
//!   types no character, and always for Esc, before the page sees it.
//! - Every permission refused without a prompt. Every certificate error
//!   cancelled, never bypassed.
//! - The address and history from `SourceChanged` and `HistoryChanged`,
//!   which fire for `pushState` too; failures from `NavigationCompleted`,
//!   whose status wry drops; the process ending from `ProcessFailed`.
//! - A confined site (ux30, [`crate::confine`]): `WebResourceRequested` for
//!   every request answers 403 to what the rule refuses, and
//!   `FrameNavigationStarting` cancels such a frame.

use std::rc::Rc;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND, COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SEPARATOR,
    COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_SUBMENU, COREWEBVIEW2_KEY_EVENT_KIND,
    COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    COREWEBVIEW2_PERMISSION_STATE_DENY, COREWEBVIEW2_PHYSICAL_KEY_STATUS,
    COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_SCRIPT_DIALOG_KIND,
    COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD,
    COREWEBVIEW2_SERVER_CERTIFICATE_ERROR_ACTION_CANCEL, COREWEBVIEW2_WEB_ERROR_STATUS,
    COREWEBVIEW2_WEB_RESOURCE_CONTEXT, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL, ICoreWebView2, ICoreWebView2_2,
    ICoreWebView2_3, ICoreWebView2_11, ICoreWebView2_14, ICoreWebView2_22,
    ICoreWebView2ContextMenuRequestedEventArgs, ICoreWebView2Settings4,
};
use webview2_com::{
    AcceleratorKeyPressedEventHandler, ContextMenuRequestedEventHandler,
    HistoryChangedEventHandler, NavigationCompletedEventHandler, NavigationStartingEventHandler,
    PermissionRequestedEventHandler, WebResourceRequestedEventHandler,
    ProcessFailedEventHandler, ScriptDialogOpeningEventHandler,
    ServerCertificateErrorDetectedEventHandler, SourceChangedEventHandler,
    TrySuspendCompletedHandler,
};
use windows_core::{BOOL, HSTRING, Interface, PWSTR};
use wry::{MemoryUsageLevel, WebView, WebViewBuilder, WebViewBuilderExtWindows, WebViewExtWindows};

use crate::host::{Hooks, LeaveFreely, ViewEvent};
use crate::keys::{Key, Press};
use crate::state::SiteFailure;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetKeyState(virtual_key: i32) -> i16;
    fn MapVirtualKeyW(code: u32, map_type: u32) -> u32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn WaitForSingleObject(handle: isize, millis: u32) -> u32;
    fn CloseHandle(handle: isize) -> i32;
}

/// `SYNCHRONIZE`: the right to wait on a process.
const SYNCHRONIZE: u32 = 0x0010_0000;

/// The site's browser process: the one that writes its cookies to the
/// session folder (ux29).
pub fn browser_process(view: &WebView) -> Option<u32> {
    let core = view.webview();
    let mut pid = 0_u32;
    // SAFETY: a WebView2 COM read on the main thread.
    unsafe { core.BrowserProcessId(&raw mut pid) }.ok()?;
    crate::glue::known_process(pid)
}

/// Wait, until `deadline`, for each browser process to end after its views
/// closed. Chromium writes its cookie store when its browser process shuts
/// down, and commits it only every 30 seconds before; a knobas that exits
/// first may leave a sign-in of the last half minute unwritten (ux29).
pub fn wait_for_exit(pids: &[u32], deadline: std::time::Instant) {
    for pid in pids {
        // SAFETY: plain Win32 calls on a process handle this function owns.
        unsafe {
            let handle = OpenProcess(SYNCHRONIZE, 0, *pid);
            if handle == 0 {
                continue;
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let millis = u32::try_from(left.as_millis()).unwrap_or(u32::MAX);
            let _ = WaitForSingleObject(handle, millis);
            CloseHandle(handle);
        }
    }
}

/// wry's default arguments, which `with_additional_browser_args` replaces,
/// and the cache's cap of 100 MB a site.
const BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disk-cache-size=104857600";

/// The builder's Windows settings.
pub fn builder(builder: WebViewBuilder<'_>) -> WebViewBuilder<'_> {
    builder
        .with_additional_browser_args(BROWSER_ARGS)
        // Ctrl+F, Ctrl+P and Alt+← stay WebView2's own (decisions §8).
        .with_browser_accelerator_keys(true)
}

fn held(virtual_key: i32) -> bool {
    // SAFETY: a plain Win32 query of this thread's keyboard state.
    unsafe { GetKeyState(virtual_key) < 0 }
}

/// The key `virtual_key` names, with the board's own character for it.
fn key_of(virtual_key: u32) -> Key {
    match virtual_key {
        0x1B => Key::Escape,
        0x74 => Key::F5,
        0x6B => Key::Char('+'),
        0x6D => Key::Char('-'),
        0x60 => Key::Char('0'),
        _ => {
            // MAPVK_VK_TO_CHAR: the unshifted character, upper case for a
            // letter; the high bit marks a dead key.
            // SAFETY: a plain Win32 query.
            let code = unsafe { MapVirtualKeyW(virtual_key, 2) } & 0x7FFF;
            char::from_u32(code)
                .filter(|ch| *ch != '\0')
                .map_or(Key::Other, |ch| Key::Char(ch.to_ascii_lowercase()))
        }
    }
}

/// The resource kind WebView2 names, for the log.
fn kind_name(context: COREWEBVIEW2_WEB_RESOURCE_CONTEXT) -> &'static str {
    match context.0 {
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
/// news address carries the install id in its query).
fn origin_text(url: &str) -> String {
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

/// The top-level address the view shows now.
fn top_of(core: &ICoreWebView2) -> Option<String> {
    let mut source = PWSTR::null();
    // SAFETY: a WebView2 COM getter on the main thread.
    unsafe { core.Source(&raw mut source) }
        .ok()
        .map(|()| webview2_com::take_pwstr(source))
}

/// Refuse what a confined site's rule refuses (ux30): every request through
/// `WebResourceRequested`, answered 403, and every frame through
/// `FrameNavigationStarting`, cancelled. Both read `confined.rules` at the
/// request, so a change applies at once; with no rule nothing is refused.
/// One log line per refused origin and kind per page.
///
/// # Errors
///
/// WebView2's, when the filter or a handler does not register: the caller
/// fails closed.
pub fn confine(view: &WebView, confined: &crate::wry_host::Confined) -> windows_core::Result<()> {
    let core = view.webview();
    // SAFETY: WebView2 COM on the main thread, with the controller wry made.
    unsafe {
        let environment = core.cast::<ICoreWebView2_2>()?.Environment()?;
        let any = HSTRING::from("*");
        // Workers' requests too, where the runtime knows the source kinds.
        if let Ok(core22) = core.cast::<ICoreWebView2_22>() {
            core22.AddWebResourceRequestedFilterWithRequestSourceKinds(
                &any,
                COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
            )?;
        } else {
            core.AddWebResourceRequestedFilter(&any, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
        }
        let rules = Rc::clone(&confined.rules);
        let source = confined.source.clone();
        let logged: std::cell::RefCell<(Option<String>, std::collections::HashSet<String>)> =
            std::cell::RefCell::default();
        let mut token = 0_i64;
        core.add_WebResourceRequested(
            &WebResourceRequestedEventHandler::create(Box::new(move |webview, args| {
                let (Some(webview), Some(args)) = (webview, args) else {
                    return Ok(());
                };
                let request = args.Request()?;
                let mut uri = PWSTR::null();
                request.Uri(&mut uri)?;
                let uri = webview2_com::take_pwstr(uri);
                let top = top_of(&webview);
                let refused = rules
                    .borrow()
                    .as_ref()
                    .is_some_and(|rule| !rule.may_load(top.as_deref(), &uri));
                if !refused {
                    return Ok(());
                }
                let response = environment.CreateWebResourceResponse(
                    None,
                    403,
                    &HSTRING::from("Forbidden"),
                    &HSTRING::new(),
                )?;
                args.SetResponse(&response)?;
                let mut context = COREWEBVIEW2_WEB_RESOURCE_CONTEXT::default();
                let _ = args.ResourceContext(&mut context);
                let kind = kind_name(context);
                let origin = origin_text(&uri);
                let mut logged = logged.borrow_mut();
                if logged.0 != top {
                    *logged = (top, std::collections::HashSet::new());
                }
                if logged.1.insert(format!("{origin} {kind}")) {
                    tracing::info!(source = %source, %origin, kind, "a confined site's request to another website refused");
                }
                Ok(())
            })),
            &raw mut token,
        )?;
        let rules = Rc::clone(&confined.rules);
        core.add_FrameNavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |webview, args| {
                let (Some(webview), Some(args)) = (webview, args) else {
                    return Ok(());
                };
                let mut uri = PWSTR::null();
                args.Uri(&mut uri)?;
                let uri = webview2_com::take_pwstr(uri);
                let top = top_of(&webview);
                let refused = rules
                    .borrow()
                    .as_ref()
                    .is_some_and(|rule| !rule.may_load(top.as_deref(), &uri));
                if refused {
                    args.SetCancel(true)?;
                }
                Ok(())
            })),
            &raw mut token,
        )?;
    }
    tracing::info!(source = %confined.source, "site confined: content from other websites is blocked");
    Ok(())
}

/// What lives as long as a site's view on Windows.
pub struct Hardened;

/// Set the controller's settings and add knobas's handlers.
pub fn harden(view: &WebView, hooks: &Hooks, leaving: &LeaveFreely) -> Hardened {
    let controller = view.controller();
    let core = view.webview();
    let key_hook = Rc::clone(&hooks.key);
    let failed_hook = Rc::clone(&hooks.event);
    let cert_hook = Rc::clone(&hooks.event);
    let source_hook = Rc::clone(&hooks.event);
    let history_hook = Rc::clone(&hooks.event);
    let ended_hook = Rc::clone(&hooks.event);
    // SAFETY: on the main thread, with the controller wry just made; every
    // call is a WebView2 COM method on it.
    let result: windows_core::Result<()> = unsafe {
        (|| {
            let settings = core.Settings()?;
            settings.SetIsWebMessageEnabled(false)?;
            settings.SetAreHostObjectsAllowed(false)?;
            if let Ok(settings4) = settings.cast::<ICoreWebView2Settings4>() {
                settings4.SetIsPasswordAutosaveEnabled(false)?;
                settings4.SetIsGeneralAutofillEnabled(false)?;
            }
            let mut token = 0_i64;
            controller.add_AcceleratorKeyPressed(
                &AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut kind = COREWEBVIEW2_KEY_EVENT_KIND::default();
                    args.KeyEventKind(&mut kind)?;
                    if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                        && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                    {
                        return Ok(());
                    }
                    let mut virtual_key = 0_u32;
                    args.VirtualKey(&mut virtual_key)?;
                    let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
                    args.PhysicalKeyStatus(&mut status)?;
                    let press = Press {
                        key: Some(key_of(virtual_key)),
                        primary: held(0x11),
                        shift: held(0x10),
                        alt: held(0x12) || kind == COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
                        other: held(0x5B) || held(0x5C),
                        repeat: status.WasKeyDown.as_bool(),
                        stamp: None,
                    };
                    if !key_hook(&press).pass {
                        args.SetHandled(true)?;
                    }
                    Ok(())
                })),
                &raw mut token,
            )?;
            core.add_PermissionRequested(
                &PermissionRequestedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY)?;
                    }
                    Ok(())
                })),
                &raw mut token,
            )?;
            if let Ok(core14) = core.cast::<ICoreWebView2_14>() {
                core14.add_ServerCertificateErrorDetected(
                    &ServerCertificateErrorDetectedEventHandler::create(Box::new(
                        move |_, args| {
                            if let Some(args) = args {
                                args.SetAction(
                                    COREWEBVIEW2_SERVER_CERTIFICATE_ERROR_ACTION_CANCEL,
                                )?;
                                cert_hook(ViewEvent::Failed(SiteFailure::Certificate));
                            }
                            Ok(())
                        },
                    )),
                    &raw mut token,
                )?;
            }
            core.add_NavigationCompleted(
                &NavigationCompletedEventHandler::create(Box::new(move |webview, args| {
                    // A navigation that asked nothing ends the "leave anyway" too.
                    if let Some(webview) = &webview {
                        set_own_dialogs(webview, true);
                    }
                    let Some(args) = args else { return Ok(()) };
                    let mut ok = BOOL::default();
                    args.IsSuccess(&mut ok)?;
                    if !ok.as_bool() {
                        let mut status = COREWEBVIEW2_WEB_ERROR_STATUS::default();
                        args.WebErrorStatus(&mut status)?;
                        if let Some(failure) = crate::failures::of_webview2(status.0) {
                            failed_hook(ViewEvent::Failed(failure));
                        }
                    }
                    Ok(())
                })),
                &raw mut token,
            )?;
            core.add_SourceChanged(
                &SourceChangedEventHandler::create(Box::new(move |webview, _| {
                    if let Some(webview) = webview {
                        source_hook(history(&webview));
                    }
                    Ok(())
                })),
                &raw mut token,
            )?;
            core.add_HistoryChanged(
                &HistoryChangedEventHandler::create(Box::new(move |webview, _| {
                    if let Some(webview) = webview {
                        history_hook(history(&webview));
                    }
                    Ok(())
                })),
                &raw mut token,
            )?;
            core.add_ProcessFailed(
                &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                    args.ProcessFailedKind(&mut kind)?;
                    // The browser or the page's renderer exited.
                    if kind.0 == 0 || kind.0 == 1 {
                        ended_hook(ViewEvent::Ended);
                    }
                    Ok(())
                })),
                &raw mut token,
            )?;
            // With WebView2's own dialogs off (only while the person's "leave anyway" stands), a page's "Leave
            // site?" is answered yes here; any other dialog in that moment is dismissed.
            let dialog_leaving = leaving.clone();
            let dialog_core = core.clone();
            core.add_ScriptDialogOpening(
                &ScriptDialogOpeningEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut kind = COREWEBVIEW2_SCRIPT_DIALOG_KIND::default();
                    args.Kind(&mut kind)?;
                    if kind == COREWEBVIEW2_SCRIPT_DIALOG_KIND_BEFOREUNLOAD && dialog_leaving.take()
                    {
                        args.Accept()?;
                    }
                    set_own_dialogs(&dialog_core, true);
                    Ok(())
                })),
                &raw mut token,
            )?;
            if let Ok(core11) = core.cast::<ICoreWebView2_11>() {
                core11.add_ContextMenuRequested(
                    &ContextMenuRequestedEventHandler::create(Box::new(|_, args| {
                        if let Some(args) = args {
                            trim_menu(&args)?;
                        }
                        Ok(())
                    })),
                    &raw mut token,
                )?;
            }
            Ok(())
        })()
    };
    if let Err(error) = result {
        tracing::warn!(%error, "a site's controller could not be fully set up");
    }
    Hardened
}

/// WebView2's page menu on a site, trimmed to `menu::SITE_KEPT`.
///
/// # Safety
///
/// On the main thread, with the live args of a `ContextMenuRequested`.
unsafe fn trim_menu(args: &ICoreWebView2ContextMenuRequestedEventArgs) -> windows_core::Result<()> {
    use crate::menu::{Item, removals, site_keeps};
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
        let gone = removals(&menu, |name| site_keeps(name, cfg!(debug_assertions)));
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

/// The address and history, read from the view.
fn history(webview: &ICoreWebView2) -> ViewEvent {
    // SAFETY: WebView2 COM getters on the main thread.
    unsafe {
        let mut source = PWSTR::null();
        let url = webview
            .Source(&mut source)
            .ok()
            .map(|()| webview2_com::take_pwstr(source));
        let mut back = BOOL::default();
        let mut forward = BOOL::default();
        let _ = webview.CanGoBack(&mut back);
        let _ = webview.CanGoForward(&mut forward);
        ViewEvent::History {
            url,
            can_back: back.as_bool(),
            can_forward: forward.as_bool(),
            loading: None,
        }
    }
}

pub enum Go {
    Back,
    Forward,
    Stop,
}

pub fn go(view: &WebView, go: Go) {
    let core = view.webview();
    // SAFETY: WebView2 COM methods on the main thread.
    let _ = unsafe {
        match go {
            Go::Back => core.GoBack(),
            Go::Forward => core.GoForward(),
            Go::Stop => core.Stop(),
        }
    };
}

/// WebView2's own script dialogs on or off.
fn set_own_dialogs(core: &ICoreWebView2, on: bool) {
    // SAFETY: WebView2 COM on the main thread.
    unsafe {
        if let Ok(settings) = core.Settings() {
            let _ = settings.SetAreDefaultScriptDialogsEnabled(on);
        }
    }
}

/// WebView2's own script dialogs for `view`, on or off.
pub fn own_dialogs(view: &WebView, on: bool) {
    set_own_dialogs(&view.webview(), on);
}

/// A hidden site's memory level: low while hidden, normal when shown.
pub fn memory(view: &WebView, visible: bool) {
    let _ = view.set_memory_usage_level(if visible {
        MemoryUsageLevel::Normal
    } else {
        MemoryUsageLevel::Low
    });
}

/// A hidden site sleeps: low memory, then suspended; woken before it shows.
pub fn sleep(view: &WebView, asleep: bool) {
    let core = view.webview();
    if asleep {
        let _ = view.set_memory_usage_level(MemoryUsageLevel::Low);
        if let Ok(core3) = core.cast::<ICoreWebView2_3>() {
            // SAFETY: WebView2 COM on the main thread; the view is hidden.
            let _ = unsafe {
                core3.TrySuspend(&TrySuspendCompletedHandler::create(Box::new(|_, _| Ok(()))))
            };
        }
    } else {
        if let Ok(core3) = core.cast::<ICoreWebView2_3>() {
            // SAFETY: WebView2 COM on the main thread.
            let _ = unsafe { core3.Resume() };
        }
        let _ = view.set_memory_usage_level(MemoryUsageLevel::Normal);
    }
}

/// The main window moved: a page's drop-down opens where the window is now.
pub fn moved(view: &WebView) {
    // SAFETY: WebView2 COM on the main thread.
    let _ = unsafe { view.controller().NotifyParentWindowPositionChanged() };
}
