//! macOS: what wry leaves out for somebody else's website, and how knobas
//! fills it (decisions §21).
//!
//! 1. wry's navigation hook is asked about every frame: its navigation
//!    delegate is wrapped, a frame that is not the main frame is allowed
//!    without the rule, and the failures wry ignores are reported.
//! 2. wry answers no JavaScript panel and grants media capture: its UI
//!    delegate is wrapped, `alert`, `confirm` and `prompt` are sheets on the
//!    window, media capture is denied, and WebKit's private before-unload
//!    panel asks "Leave this page?" (decisions §26 item 5).
//! 3. Keys: one local monitor for key-down, installed with the first site and
//!    removed with the last, sees every key before the first responder.
//! 4. The monitor sees a key the page did not use twice, with one time stamp:
//!    [`crate::keys::Keyboard`] counts it once.
//! 5. The user agent names a Safari, so sites do not answer "unsupported
//!    browser".

use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, NSObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSEvent, NSEventMask, NSEventModifierFlags};
use objc2_foundation::{NSObjectProtocol, NSString};
use objc2_web_kit::WKWebsiteDataStore;
use wry::{WebView, WebViewBuilder, WebViewBuilderExtDarwin, WebViewExtDarwin, WebViewExtMacOS};

use crate::host::{Hooks, KeyHook, LeaveFreely, ViewEvent, ViewSpec};
use crate::keys::{Key, Press};

/// The macOS release whose WebKit keeps a data store per identifier.
const DATA_STORES_FROM: u32 = 14;

fn os_major() -> u32 {
    // SAFETY: `processInfo` and `operatingSystemVersion` are always there.
    let version: objc2_foundation::NSOperatingSystemVersion = unsafe {
        let info: Retained<AnyObject> = msg_send![objc2::class!(NSProcessInfo), processInfo];
        msg_send![&*info, operatingSystemVersion]
    };
    u32::try_from(version.majorVersion).unwrap_or(0)
}

/// Whether macOS `major` keeps a data store per identifier, so [`forget`]
/// has data to clear first.
fn has_data_stores(major: u32) -> bool {
    major >= DATA_STORES_FROM
}

/// Whether a site's session outlives knobas on this macOS: data stores by
/// identifier came with macOS 14. Before it a site is private and its
/// sign-in lasts until knobas quits (decisions §5).
#[must_use]
pub fn sessions_persist() -> bool {
    os_major() >= DATA_STORES_FROM
}

/// The Safari that ships with macOS `major`: the major itself from 26 on,
/// three more before (14 had 17, 15 had 18).
#[must_use]
pub fn safari_version(major: u32) -> u32 {
    if major >= 26 { major } else { major + 3 }
}

/// WKWebView's own user agent with the Safari part it lacks.
#[must_use]
pub fn user_agent(major: u32) -> String {
    format!(
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/{}.0 Safari/605.1.15",
        safari_version(major)
    )
}

/// The builder's macOS settings: the session, the user agent, the process
/// that ends.
pub fn builder<'a>(
    builder: WebViewBuilder<'a>,
    spec: &ViewSpec,
    hooks: &Hooks,
) -> WebViewBuilder<'a> {
    let major = os_major();
    let ended = Rc::clone(&hooks.event);
    let builder = builder
        .with_user_agent(user_agent(major))
        .with_on_web_content_process_terminate_handler(move || ended(ViewEvent::Ended));
    if major >= DATA_STORES_FROM {
        builder.with_data_store_identifier(spec.data_store)
    } else {
        // wry would fall back to the default store, which is knobas's own
        // page's. Never that.
        builder.with_incognito(true)
    }
}

thread_local! {
    /// The stores whose data [`forget`] has cleared and whose removal is the
    /// next try's.
    static CLEARED: RefCell<std::collections::HashSet<[u8; 16]>> = RefCell::default();
}

/// Remove a site's data store. Its view is dropped already. `Sites::forget`
/// calls it again every half second until it answers `true`.
///
/// The first call clears the store's data through the store and answers
/// `false`; the next removes the store. Removing alone deletes the store's
/// files but, while another view (knobas's own page) keeps WebKit's network
/// process alive, leaves the session it holds for the store: in the real
/// window (2026-11-22) Confluence's cookies came back to the next view with
/// that identifier, which was signed in at once and wrote them back at the
/// quit (`tests/mac_forget.rs`). Clearing through the store reaches that
/// session. The removal waits for the next call because WebKit refuses to
/// remove a store while the clear's own handle on it is open.
pub fn forget(id: [u8; 16], done: Box<dyn FnOnce(bool) + Send>) {
    let Some(mtm) = MainThreadMarker::new() else {
        done(false);
        return;
    };
    if !has_data_stores(os_major()) || CLEARED.with(|cleared| cleared.borrow_mut().remove(&id)) {
        remove_store(id, done);
        return;
    }
    let store = objc2::rc::autoreleasepool(|_| {
        // SAFETY: a WebKit read on the main thread (`mtm`).
        unsafe {
            WKWebsiteDataStore::dataStoreForIdentifier(
                &objc2_foundation::NSUUID::from_bytes(id),
                mtm,
            )
        }
    });
    let held = RefCell::new(Some(done));
    let cleared = RcBlock::new(move || {
        if let Some(done) = held.take() {
            CLEARED.with(|cleared| cleared.borrow_mut().insert(id));
            tracing::debug!("a site's data store is cleared; its removal is the next try");
            done(false);
        }
    });
    // SAFETY: a WebKit call on the main thread (`mtm`), with the types its
    // selector takes; WebKit keeps the block until it has called it.
    unsafe {
        store.removeDataOfTypes_modifiedSince_completionHandler(
            &WKWebsiteDataStore::allWebsiteDataTypes(mtm),
            &objc2_foundation::NSDate::distantPast(),
            &cleared,
        );
    }
}

fn remove_store(id: [u8; 16], done: Box<dyn FnOnce(bool) + Send>) {
    WebView::remove_data_store(&id, move |result| {
        if let Err(error) = &result {
            tracing::debug!(%error, "a site's data store is not removed yet");
        }
        done(result.is_ok());
    });
}

/// Quitting: the sites' cookies to disk. WebKit's network process keeps
/// them and writes them itself (measured, ux29); nothing to wait for here.
pub fn flush(deadline: std::time::Instant) {
    let _ = deadline;
}

/// How far knobas's own page starts down the window's content view: on a
/// window whose content view runs under its title bar (measured on macOS 27:
/// a 900-point content view, a 868-point page 32 points down), the title
/// bar's height; else 0. The site's rectangle is measured in the page, so it
/// moves down by this and loses it from its height (ux29: before, a site
/// covered its own bar).
pub fn under_title(view: &WebView) -> f64 {
    let webview = view.webview();
    // SAFETY: `window`, `contentView`, `frame` and `contentLayoutRect` are
    // plain AppKit reads on the main thread.
    unsafe {
        let window: Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> =
            msg_send![&*webview, window];
        let Some(window) = window else {
            return 0.0;
        };
        let Some(content) = window.contentView() else {
            return 0.0;
        };
        let full = content.frame().size.height;
        let layout = window.contentLayoutRect().size.height;
        crate::glue::title_room(full, layout)
    }
}

/// Back, forward, stop.
pub enum Go {
    Back,
    Forward,
    Stop,
}

pub fn go(view: &WebView, go: Go) {
    let wk = view.webview();
    // SAFETY: WKWebView's own methods, on the main thread.
    unsafe {
        let _: Option<Retained<AnyObject>> = match go {
            Go::Back => msg_send![&*wk, goBack],
            Go::Forward => msg_send![&*wk, goForward],
            Go::Stop => {
                let () = msg_send![&*wk, stopLoading];
                None
            }
        };
    }
}

fn url_string(url: Option<Retained<AnyObject>>) -> Option<String> {
    let url = url?;
    // SAFETY: an NSURL.
    let text: Option<Retained<NSString>> = unsafe { msg_send![&*url, absoluteString] };
    text.map(|text| text.to_string())
}

/// The address, history and loading state, read now.
pub fn poll(view: &WebView) -> ViewEvent {
    let wk = view.webview();
    // SAFETY: WKWebView's own properties, on the main thread.
    unsafe {
        let url: Option<Retained<AnyObject>> = msg_send![&*wk, URL];
        let can_back: bool = msg_send![&*wk, canGoBack];
        let can_forward: bool = msg_send![&*wk, canGoForward];
        let loading: bool = msg_send![&*wk, isLoading];
        ViewEvent::History {
            url: url_string(url),
            can_back,
            can_forward,
            loading: Some(loading),
        }
    }
}

// ---------------------------------------------------------------- delegates

pub struct NavIvars {
    inner: Retained<AnyObject>,
    event: Rc<dyn Fn(ViewEvent)>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = NavIvars]
    pub struct KnobasSiteNavigation;

    unsafe impl NSObjectProtocol for KnobasSiteNavigation {}

    impl KnobasSiteNavigation {
        #[unsafe(method(respondsToSelector:))]
        fn responds(&self, selector: Sel) -> bool {
            let own = selector == sel!(webView:decidePolicyForNavigationAction:decisionHandler:)
                || selector == sel!(webView:didFailProvisionalNavigation:withError:)
                || selector == sel!(webView:didFailNavigation:withError:);
            // SAFETY: forwarding a question to wry's delegate.
            own || unsafe { msg_send![&*self.ivars().inner, respondsToSelector: selector] }
        }

        #[unsafe(method(forwardingTargetForSelector:))]
        fn forwarding(&self, _selector: Sel) -> *mut AnyObject {
            Retained::as_ptr(&self.ivars().inner).cast_mut()
        }

        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        fn decide(&self, webview: &AnyObject, action: &AnyObject, handler: &block2::Block<dyn Fn(isize)>) {
            // SAFETY: WKNavigationAction's own properties.
            let main = unsafe {
                let frame: Option<Retained<AnyObject>> = msg_send![action, targetFrame];
                match frame {
                    Some(frame) => msg_send![&*frame, isMainFrame],
                    // No target frame is a new window: the main frame's rule.
                    None => true,
                }
            };
            if !main {
                // A frame follows the site's own rules, as in a browser.
                handler.call((1,));
                return;
            }
            // SAFETY: wry's delegate answers this selector.
            let () = unsafe {
                msg_send![&*self.ivars().inner, webView: webview, decidePolicyForNavigationAction: action, decisionHandler: handler]
            };
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        fn failed_provisional(&self, _webview: &AnyObject, _navigation: *mut AnyObject, error: &AnyObject) {
            report_failure(&self.ivars().event, error);
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        fn failed(&self, _webview: &AnyObject, _navigation: *mut AnyObject, error: &AnyObject) {
            report_failure(&self.ivars().event, error);
        }
    }
);

fn report_failure(event: &Rc<dyn Fn(ViewEvent)>, error: &AnyObject) {
    // SAFETY: an NSError.
    let (domain, code) = unsafe {
        let domain: Retained<NSString> = msg_send![error, domain];
        let code: isize = msg_send![error, code];
        (domain.to_string(), code)
    };
    if let Some(failure) = crate::failures::of_webkit(&domain, code) {
        event(ViewEvent::Failed(failure));
    }
}

pub struct UiIvars {
    inner: Retained<AnyObject>,
    leaving: LeaveFreely,
}

fn host_of_view(webview: &AnyObject) -> String {
    // SAFETY: a WKWebView.
    let url: Option<Retained<AnyObject>> = unsafe { msg_send![webview, URL] };
    url_string(url)
        .and_then(|url| url::Url::parse(&url).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Show a sheet on `webview`'s window with `buttons`, the first the
/// default; `done` gets the index pressed and the field's text.
fn sheet(
    webview: &AnyObject,
    title: &str,
    message: &str,
    buttons: &[&str],
    field: Option<&str>,
    done: Box<dyn Fn(usize, String)>,
) {
    let Some(mtm) = MainThreadMarker::new() else {
        done(buttons.len().saturating_sub(1), String::new());
        return;
    };
    // SAFETY: AppKit on the main thread.
    unsafe {
        let window: Option<Retained<AnyObject>> = msg_send![webview, window];
        let Some(window) = window else {
            done(buttons.len().saturating_sub(1), String::new());
            return;
        };
        let alert: Retained<AnyObject> = msg_send![objc2::class!(NSAlert), new];
        let () = msg_send![&*alert, setMessageText: &*NSString::from_str(title)];
        let () = msg_send![&*alert, setInformativeText: &*NSString::from_str(message)];
        for button in buttons {
            let _: Retained<AnyObject> =
                msg_send![&*alert, addButtonWithTitle: &*NSString::from_str(button)];
        }
        let input: Option<Retained<AnyObject>> = field.map(|text| {
            let input: Retained<AnyObject> =
                msg_send![objc2::class!(NSTextField), textFieldWithString: &*NSString::from_str(text)];
            let frame = objc2_foundation::NSRect::new(
                objc2_foundation::NSPoint::new(0.0, 0.0),
                objc2_foundation::NSSize::new(280.0, 24.0),
            );
            let () = msg_send![&*input, setFrame: frame];
            let () = msg_send![&*alert, setAccessoryView: &*input];
            input
        });
        let _ = mtm;
        let block = RcBlock::new(move |response: isize| {
            let index = usize::try_from(response - 1000).unwrap_or(0);
            let text = input
                .as_ref()
                .map(|input| {
                    let value: Retained<NSString> = msg_send![&**input, stringValue];
                    value.to_string()
                })
                .unwrap_or_default();
            done(index, text);
        });
        let () = msg_send![&*alert, beginSheetModalForWindow: &*window, completionHandler: &*block];
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = UiIvars]
    pub struct KnobasSiteUi;

    unsafe impl NSObjectProtocol for KnobasSiteUi {}

    impl KnobasSiteUi {
        #[unsafe(method(respondsToSelector:))]
        fn responds(&self, selector: Sel) -> bool {
            let own = selector == sel!(webView:runJavaScriptAlertPanelWithMessage:initiatedByFrame:completionHandler:)
                || selector == sel!(webView:runJavaScriptConfirmPanelWithMessage:initiatedByFrame:completionHandler:)
                || selector == sel!(webView:runJavaScriptTextInputPanelWithPrompt:defaultText:initiatedByFrame:completionHandler:)
                || selector == sel!(webView:requestMediaCapturePermissionForOrigin:initiatedByFrame:type:decisionHandler:)
                || selector == sel!(_webView:runBeforeUnloadConfirmPanelWithMessage:initiatedByFrame:completionHandler:);
            // SAFETY: forwarding a question to wry's delegate.
            own || unsafe { msg_send![&*self.ivars().inner, respondsToSelector: selector] }
        }

        #[unsafe(method(forwardingTargetForSelector:))]
        fn forwarding(&self, _selector: Sel) -> *mut AnyObject {
            Retained::as_ptr(&self.ivars().inner).cast_mut()
        }

        #[unsafe(method(webView:runJavaScriptAlertPanelWithMessage:initiatedByFrame:completionHandler:))]
        fn alert(&self, webview: &AnyObject, message: &NSString, _frame: &AnyObject, handler: &block2::Block<dyn Fn()>) {
            let handler = handler.copy();
            let title = format!("{} says", host_of_view(webview));
            sheet(webview, &title, &message.to_string(), &["OK"], None, Box::new(move |_, _| handler.call(())));
        }

        #[unsafe(method(webView:runJavaScriptConfirmPanelWithMessage:initiatedByFrame:completionHandler:))]
        fn confirm(&self, webview: &AnyObject, message: &NSString, _frame: &AnyObject, handler: &block2::Block<dyn Fn(Bool)>) {
            let handler = handler.copy();
            let title = format!("{} says", host_of_view(webview));
            sheet(webview, &title, &message.to_string(), &["OK", "Cancel"], None, Box::new(move |index, _| {
                handler.call((Bool::new(index == 0),));
            }));
        }

        #[unsafe(method(webView:runJavaScriptTextInputPanelWithPrompt:defaultText:initiatedByFrame:completionHandler:))]
        fn prompt(&self, webview: &AnyObject, prompt: &NSString, default: *mut NSString, _frame: &AnyObject, handler: &block2::Block<dyn Fn(*mut NSString)>) {
            let handler = handler.copy();
            let title = format!("{} says", host_of_view(webview));
            // SAFETY: an NSString or nil.
            let default = unsafe { default.as_ref() }.map(ToString::to_string).unwrap_or_default();
            sheet(webview, &title, &prompt.to_string(), &["OK", "Cancel"], Some(&default), Box::new(move |index, text| {
                if index == 0 {
                    let text = NSString::from_str(&text);
                    handler.call((Retained::as_ptr(&text).cast_mut(),));
                } else {
                    handler.call((std::ptr::null_mut(),));
                }
            }));
        }

        #[unsafe(method(webView:requestMediaCapturePermissionForOrigin:initiatedByFrame:type:decisionHandler:))]
        fn media(&self, _webview: &AnyObject, _origin: &AnyObject, _frame: &AnyObject, _kind: isize, handler: &block2::Block<dyn Fn(isize)>) {
            // WKPermissionDecisionDeny. wry's own answers Grant.
            handler.call((2,));
        }

        #[unsafe(method(_webView:runBeforeUnloadConfirmPanelWithMessage:initiatedByFrame:completionHandler:))]
        fn before_unload(&self, webview: &AnyObject, _message: &NSString, _frame: &AnyObject, handler: &block2::Block<dyn Fn(Bool)>) {
            // The person already said "leave anyway" in the bar.
            if self.ivars().leaving.take() {
                handler.call((Bool::YES,));
                return;
            }
            let handler = handler.copy();
            sheet(
                webview,
                "Leave this page?",
                "It has changes it has not saved.",
                &["Stay", "Leave"],
                None,
                Box::new(move |index, _| handler.call((Bool::new(index == 1),))),
            );
        }
    }
);

// ---------------------------------------------------------------------- keys

struct Route {
    view: Retained<AnyObject>,
    key: KeyHook,
}

/// What knobas does with a key its own page holds: whether it took it.
pub type ShellKeys = Rc<dyn Fn(&Press) -> bool>;

thread_local! {
    static ROUTES: RefCell<Vec<Route>> = const { RefCell::new(Vec::new()) };
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    static SHELL_KEYS: RefCell<Option<ShellKeys>> = const { RefCell::new(None) };
}

/// What knobas does with a key its own page holds, while a site is alive:
/// ⌘Q and ⌘W before the menu sees them (`Sites::shell_key`).
pub fn set_shell_keys(keys: ShellKeys) {
    SHELL_KEYS.with(|slot| *slot.borrow_mut() = Some(keys));
}

fn press_of(event: &NSEvent) -> Press {
    let flags = event.modifierFlags();
    let code = event.keyCode();
    let key = match code {
        53 => Some(Key::Escape),
        96 => Some(Key::F5),
        _ => {
            let chars = event.charactersIgnoringModifiers();
            chars
                .and_then(|text| text.to_string().chars().next())
                .map(|ch| Key::Char(ch.to_ascii_lowercase()))
                .or(Some(Key::Other))
        }
    };
    Press {
        key,
        primary: flags.contains(NSEventModifierFlags::Command),
        shift: flags.contains(NSEventModifierFlags::Shift),
        alt: flags.contains(NSEventModifierFlags::Option),
        other: flags.contains(NSEventModifierFlags::Control),
        repeat: event.isARepeat(),
        stamp: Some(event.timestamp()),
    }
}

/// The site whose view holds the keyboard, if one does.
fn route_of_first_responder() -> Option<KeyHook> {
    let mtm = MainThreadMarker::new()?;
    let window = NSApplication::sharedApplication(mtm).keyWindow()?;
    let responder = window.firstResponder()?;
    let view_class = AnyClass::get(c"NSView")?;
    // SAFETY: plain NSObject questions.
    let is_view: bool = unsafe { msg_send![&*responder, isKindOfClass: view_class] };
    if !is_view {
        return None;
    }
    ROUTES.with(|routes| {
        routes.borrow().iter().find_map(|route| {
            // SAFETY: both are NSViews.
            let inside: bool = unsafe { msg_send![&*responder, isDescendantOf: &*route.view] };
            inside.then(|| Rc::clone(&route.key))
        })
    })
}

fn install_monitor() {
    let block = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands a live event.
        let event_ref = unsafe { event.as_ref() };
        let press = press_of(event_ref);
        if let Some(key) = route_of_first_responder() {
            let claim = key(&press);
            return if claim.pass {
                event.as_ptr()
            } else {
                std::ptr::null_mut()
            };
        }
        let taken = SHELL_KEYS.with(|keys| keys.borrow().as_ref().is_some_and(|keys| keys(&press)));
        if taken {
            std::ptr::null_mut()
        } else {
            event.as_ptr()
        }
    });
    // SAFETY: a local monitor on the main thread, removed with the last site.
    let monitor = unsafe {
        NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
    };
    MONITOR
        .with(|slot| *slot.borrow_mut() = monitor.map(|m| unsafe { Retained::cast_unchecked(m) }));
}

fn remove_monitor() {
    if let Some(monitor) = MONITOR.with(|slot| slot.borrow_mut().take()) {
        // SAFETY: the object `addLocalMonitor…` answered.
        unsafe { NSEvent::removeMonitor(&monitor) };
    }
}

/// What lives as long as a site's view on the Mac: the two wrappers (the
/// web view holds its delegates weakly) and the key route.
pub struct Wrapped {
    _navigation: Option<Retained<KnobasSiteNavigation>>,
    _ui: Option<Retained<KnobasSiteUi>>,
    view: Retained<AnyObject>,
}

impl Drop for Wrapped {
    fn drop(&mut self) {
        let empty = ROUTES.with(|routes| {
            let mut routes = routes.borrow_mut();
            routes.retain(|route| {
                !std::ptr::eq(Retained::as_ptr(&route.view), Retained::as_ptr(&self.view))
            });
            routes.is_empty()
        });
        if empty {
            remove_monitor();
        }
    }
}

/// Wrap `view`'s two delegates and route its keys.
pub fn wrap(view: &WebView, source: &str, hooks: &Hooks, leaving: &LeaveFreely) -> Wrapped {
    let wk = view.webview();
    // SAFETY: a WryWebView is an NSView.
    let as_view: Retained<AnyObject> = unsafe { Retained::cast_unchecked(wk.clone()) };
    let first = ROUTES.with(|routes| {
        let mut routes = routes.borrow_mut();
        routes.push(Route {
            view: as_view.clone(),
            key: Rc::clone(&hooks.key),
        });
        routes.len() == 1
    });
    if first {
        install_monitor();
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return Wrapped {
            _navigation: None,
            _ui: None,
            view: as_view,
        };
    };
    // SAFETY: WKWebView's own delegate properties, on the main thread.
    unsafe {
        let nav: Option<Retained<AnyObject>> = msg_send![&*wk, navigationDelegate];
        let ui: Option<Retained<AnyObject>> = msg_send![&*wk, UIDelegate];
        let navigation = nav.map(|inner| {
            debug_assert!(
                format!("{:?}", inner.class()).contains("Wry"),
                "wry's navigation delegate changed: the site's wrapper needs a look"
            );
            let proxy = mtm.alloc::<KnobasSiteNavigation>().set_ivars(NavIvars {
                inner,
                event: Rc::clone(&hooks.event),
            });
            let proxy: Retained<KnobasSiteNavigation> = msg_send![super(proxy), init];
            let () = msg_send![&*wk, setNavigationDelegate: &*proxy];
            proxy
        });
        let ui = ui.map(|inner| {
            let proxy = mtm.alloc::<KnobasSiteUi>().set_ivars(UiIvars {
                inner,
                leaving: leaving.clone(),
            });
            let proxy: Retained<KnobasSiteUi> = msg_send![super(proxy), init];
            let () = msg_send![&*wk, setUIDelegate: &*proxy];
            proxy
        });
        tracing::debug!(
            source,
            navigation = navigation.is_some(),
            ui = ui.is_some(),
            "site delegates wrapped"
        );
        Wrapped {
            _navigation: navigation,
            _ui: ui,
            view: as_view,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_stores_by_identifier_came_with_macos_14() {
        assert!(!has_data_stores(13));
        assert!(has_data_stores(14));
        assert!(has_data_stores(27));
    }

    #[test]
    fn the_user_agent_names_the_safari_of_its_macos() {
        assert_eq!(safari_version(14), 17);
        assert_eq!(safari_version(15), 18);
        assert_eq!(safari_version(26), 26);
        assert_eq!(safari_version(27), 27);
        assert!(user_agent(27).ends_with("Version/27.0 Safari/605.1.15"));
        assert!(!user_agent(27).to_lowercase().contains("knobas"));
    }
}
