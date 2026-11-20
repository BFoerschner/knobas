//! A source's own website, shown inside knobas (ADR-0048).
//!
//! - [`policy`]: which addresses a site may show, and what crosses from it.
//! - [`keys`]: the keys knobas takes from a site, and Esc twice.
//! - [`state`]: one site's state, and how long sites live.
//! - [`session`]: where a site keeps its session.
//! - [`scripts`]: the only scripts knobas runs in a site.
//! - [`host`]: what the controller asks of a platform.
//! - [`sites`]: the controller, its commands and its events.
//! - `wry_host`: the platform, over wry, on macOS and Windows.
//!
//! The view is made with `wry::WebViewBuilder::build_as_child`, never through
//! Tauri: Tauri hands every web view it builds its IPC bootstrap, its plugins'
//! scripts and its schemes (decisions §3). `tests/scan.rs` holds that line.

pub mod failures;
pub mod host;
pub mod keys;
pub mod policy;
pub mod scripts;
pub mod session;
pub mod sites;
pub mod state;

#[cfg(feature = "test-util")]
pub mod fake;

#[cfg(any(target_os = "macos", windows))]
pub mod wry_host;

#[cfg(target_os = "macos")]
pub mod mac;

#[cfg(windows)]
mod win;

pub use sites::{Config, Shell, SiteError, SiteEvent, Sites};

/// Whether this platform offers sites (decisions §23).
pub const OFFERED: bool = cfg!(any(target_os = "macos", windows));

/// Whether a site's sign-in outlives knobas here: always on Windows, from
/// macOS 14 on the Mac (decisions §5).
#[must_use]
pub fn sessions_persist() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::sessions_persist()
    }
    #[cfg(not(target_os = "macos"))]
    {
        OFFERED
    }
}
