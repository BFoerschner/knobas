//! *Smooth scrolling on 120 Hz displays*, Settings › Performance (2026-11-11).
//!
//! WKWebView draws page content at 60 frames a second whatever the display
//! can do: WebKit's internal feature `PreferPageRenderingUpdatesNear60FPSEnabled`
//! is on by default, and no public API turns it off (WebKit bug 294338, Tauri
//! issue 13978). Turned off on the main window's web view, the window ran at
//! 119.9 fps on a 120 Hz MacBook Pro where it ran at 60.1 (the smoothness
//! research of 2026-11-11). WebView2 and WebKitGTK have no such cap, so the
//! setting exists on macOS alone; elsewhere [`offered`] is false and nothing
//! here runs.
//!
//! **Private API.** The switch is `+[WKPreferences _features]` and
//! `-[WKPreferences _setEnabled:forFeature:]`, WebKit SPI. A build that calls
//! it would not pass Mac App Store review; knobas ships outside the store.
//! Apple can rename or remove either selector in any update, so [`apply`]
//! fails soft: a missing selector, a missing feature or a web view it cannot
//! reach leaves the window at 60, as before, with one `debug` line and
//! nothing louder.
//!
//! Stored as `shell.smooth_scrolling` in `knobas.setting`, a JSON boolean,
//! [`DEFAULT`] until set. Applied once the database is up (the window is built
//! before it), and again at once when Settings changes it.

use sqlx::PgPool;

use crate::IpcError;

/// The `knobas.setting` key.
pub const KEY: &str = "shell.smooth_scrolling";

/// On until set. Measured on the M2 Pro MacBook Pro of the research, on mains
/// power, 2026-11-11: see `docs/decisions/2026-11-11-loopback-line-endings-and-120-hz.md`
/// for the numbers and why they decide this.
pub const DEFAULT: bool = true;

/// Whether this platform has the setting: macOS, where WKWebView caps the rate.
#[must_use]
pub const fn offered() -> bool {
    cfg!(target_os = "macos")
}

fn stored_from(value: Option<&serde_json::Value>) -> bool {
    value
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(DEFAULT)
}

/// The stored setting, [`DEFAULT`] when unset or not a boolean.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn stored(pool: &PgPool) -> Result<bool, IpcError> {
    Ok(stored_from(
        crate::settings::read_value(pool, KEY).await?.as_ref(),
    ))
}

/// Store the setting and answer what is now stored.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the write fails.
pub async fn store(pool: &PgPool, on: bool) -> Result<bool, IpcError> {
    crate::settings::write_value(pool, KEY, &serde_json::Value::Bool(on)).await?;
    stored(pool).await
}

/// Lift WebKit's 60 fps cap on the main window when `on`, put it back when
/// not. Does nothing off macOS, and nothing louder than a `debug` line when
/// the private switch is not there (see the module docs).
pub fn apply<R: tauri::Runtime>(app: &tauri::AppHandle<R>, on: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager as _;
        let Some(window) = app.get_webview_window("main") else {
            tracing::debug!("smooth scrolling: no main window to set");
            return;
        };
        let asked = window.with_webview(move |webview| {
            // SAFETY: `inner()` is the window's live `WKWebView`, and this runs
            // on the main thread, where Tauri hands it out. Every private
            // selector is checked with `respondsToSelector:` before it is sent.
            let said = unsafe { webkit::set_capped(webview.inner(), !on) };
            match said {
                Ok(capped) => tracing::debug!(on, capped, "smooth scrolling set"),
                Err(why) => tracing::debug!(on, why, "smooth scrolling left as WebKit has it"),
            }
        });
        if let Err(error) = asked {
            tracing::debug!(%error, "smooth scrolling: the web view could not be reached");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, on);
}

#[cfg(target_os = "macos")]
mod webkit {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
    use objc2::sel;
    use objc2_foundation::NSString;

    /// The internal feature that holds page rendering near 60 fps.
    pub(super) const FEATURE: &str = "PreferPageRenderingUpdatesNear60FPSEnabled";

    /// Turn [`FEATURE`] on (`capped`) or off for `webview`'s preferences, and
    /// answer what it reads afterwards. `Err` says why nothing was set.
    ///
    /// # Safety
    ///
    /// `webview` is null or a live `WKWebView`, and this runs on the main
    /// thread.
    pub(super) unsafe fn set_capped(
        webview: *mut std::ffi::c_void,
        capped: bool,
    ) -> Result<bool, &'static str> {
        let webview = webview.cast::<AnyObject>();
        if webview.is_null() {
            return Err("no web view");
        }
        let class = AnyClass::get(c"WKPreferences").ok_or("no WKPreferences class")?;
        let responds = |target: *const AnyObject, selector: Sel| -> bool {
            // SAFETY: `respondsToSelector:` is NSObject's, on a live object.
            let answer: Bool = unsafe { msg_send![target, respondsToSelector: selector] };
            answer.as_bool()
        };
        let class_object = std::ptr::from_ref(class).cast::<AnyObject>();
        if !responds(class_object, sel!(_features)) {
            return Err("+[WKPreferences _features] is not there");
        }
        // SAFETY: public `WKWebView.configuration` and
        // `WKWebViewConfiguration.preferences`, both non-null on a live view.
        let config: *mut AnyObject = unsafe { msg_send![webview, configuration] };
        if config.is_null() {
            return Err("no configuration");
        }
        let prefs: *mut AnyObject = unsafe { msg_send![config, preferences] };
        if prefs.is_null() {
            return Err("no preferences");
        }
        if !responds(prefs, sel!(_setEnabled:forFeature:))
            || !responds(prefs, sel!(_isEnabledForFeature:))
        {
            return Err("the preferences take no _setEnabled:forFeature:");
        }
        // SAFETY: `_features` answered `respondsToSelector:` above; it returns
        // an `NSArray` of `_WKFeature`.
        let features: *mut AnyObject = unsafe { msg_send![class, _features] };
        if features.is_null() {
            return Err("no feature list");
        }
        let count: usize = unsafe { msg_send![features, count] };
        for at in 0..count {
            let feature: *mut AnyObject = unsafe { msg_send![features, objectAtIndex: at] };
            if feature.is_null() || !responds(feature, sel!(key)) {
                continue;
            }
            let key: *mut AnyObject = unsafe { msg_send![feature, key] };
            if key.is_null() {
                continue;
            }
            // SAFETY: a `_WKFeature`'s `key` is an `NSString`.
            let key = unsafe { &*key.cast::<NSString>() };
            if key.to_string() != FEATURE {
                continue;
            }
            // SAFETY: both selectors answered `respondsToSelector:` above.
            let _: () =
                unsafe { msg_send![prefs, _setEnabled: Bool::new(capped), forFeature: feature] };
            let now: Bool = unsafe { msg_send![prefs, _isEnabledForFeature: feature] };
            return Ok(now.as_bool());
        }
        Err("the feature is not in the list")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_setting_reads_its_default_until_it_is_a_boolean() {
        assert_eq!(stored_from(None), DEFAULT);
        assert_eq!(stored_from(Some(&serde_json::json!("yes"))), DEFAULT);
        assert!(stored_from(Some(&serde_json::json!(true))));
        assert!(!stored_from(Some(&serde_json::json!(false))));
    }

    #[test]
    fn it_is_offered_where_the_webview_caps_the_rate() {
        assert_eq!(offered(), cfg!(target_os = "macos"));
    }

    /// The private selectors are there on this macOS: `+[WKPreferences
    /// _features]` and `-[WKPreferences _setEnabled:forFeature:]`. When Apple
    /// removes either this test says so, and the app goes on at 60 as before.
    /// A null view is refused. Asked of the Objective-C runtime alone: calling
    /// `_features` initialises WebKit, which traps off the main thread, and a
    /// test runs off it. That the feature is in the list and lifts the cap is
    /// proved in a window: see the decision record.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_private_switch_is_there_on_this_macos() {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject, Bool};
        use objc2::sel;

        // SAFETY: a null view is the case the function checks first.
        assert_eq!(
            unsafe { webkit::set_capped(std::ptr::null_mut(), false) },
            Err("no web view")
        );
        let class = AnyClass::get(c"WKPreferences").expect("WebKit is linked");
        let class_object = std::ptr::from_ref(class).cast::<AnyObject>();
        // SAFETY: NSObject's own class methods; neither initialises WebKit.
        let lists: Bool = unsafe { msg_send![class_object, respondsToSelector: sel!(_features)] };
        assert!(lists.as_bool(), "+[WKPreferences _features] is gone");
        let sets: Bool = unsafe {
            msg_send![class_object, instancesRespondToSelector: sel!(_setEnabled:forFeature:)]
        };
        assert!(
            sets.as_bool(),
            "-[WKPreferences _setEnabled:forFeature:] is gone"
        );
        let reads: Bool = unsafe {
            msg_send![class_object, instancesRespondToSelector: sel!(_isEnabledForFeature:)]
        };
        assert!(
            reads.as_bool(),
            "-[WKPreferences _isEnabledForFeature:] is gone"
        );
    }
}
