//! A source's own website inside knobas (ADR-0048): the app's half.
//!
//! The rules, the controller and the native view are `knobas-site`'s. Here:
//! the Tauri shell they run on (the main thread, events to the main window,
//! the window's size), the four `knobas.setting` rows of decisions §18, the
//! rules built from them and the source list, the clock, and the main
//! window's events a site follows.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_site::policy::{Base, Origin, OtherSite, SiteRules};
use knobas_site::sites::{Config, Shell, SiteEvent};
use knobas_site::state::{KEPT_DEFAULT, KEPT_MAX, KEPT_MIN};
use serde::Serialize;
use sqlx::PgPool;
use tauri::Manager;
use ts_rs::TS;

use crate::IpcError;
use crate::events;

/// The managed controller.
#[derive(Clone)]
pub struct SiteState {
    pub sites: knobas_site::Sites,
}

/// The controller, or `NotReady` before the main window is up.
///
/// # Errors
///
/// [`crate::IpcErrorCode::NotReady`] before [`start`].
pub fn state<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<knobas_site::Sites, IpcError> {
    app.try_state::<SiteState>()
        .map(|state| state.sites.clone())
        .ok_or_else(|| IpcError::not_ready("sites are not ready yet"))
}

// -- settings (decisions §18) -------------------------------------------------

#[must_use]
pub fn open_inside_key(source: &str) -> String {
    format!("site.open_inside.{source}")
}

#[must_use]
pub fn allowed_key(source: &str) -> String {
    format!("site.allowed.{source}")
}

#[must_use]
pub fn kept_since_key(source: &str) -> String {
    format!("site.kept_since.{source}")
}

/// *Sites kept open*.
pub const KEPT_KEY: &str = "site.kept";

/// One source's site, as Settings and `openWeb` read it.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[ts(export_to = "site.ts")]
pub struct SiteSetting {
    pub source: String,
    /// *Open its pages inside knobas*.
    pub open_inside: bool,
    /// The source's base URL, which `openWeb` matches addresses against.
    pub base: String,
    /// `own(S)`, as scheme, host and port (`https://jira.example`); empty when the base URL is not
    /// an http(s) address.
    pub own: String,
    /// The origins the person allowed.
    pub allowed: Vec<String>,
    /// Origins *Open in ⟨source⟩* named that went to the browser, offered
    /// with *Allow* until knobas quits.
    pub suggested: Vec<String>,
    /// When a page on the source's own address first loaded after the last
    /// *Forget…*, RFC 3339.
    pub kept_since: Option<String>,
    /// Sites open inside knobas on this system (decisions §23).
    pub offered: bool,
    /// The sign-in outlives knobas here (macOS 14 and later, Windows).
    pub persists: bool,
}

async fn read_allowed(pool: &PgPool, source: &str) -> Result<Vec<Origin>, IpcError> {
    let value = crate::settings::read_value(pool, &allowed_key(source)).await?;
    Ok(value
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|item| item.as_str().and_then(|text| Origin::parse(text).ok()))
        .collect())
}

async fn write_allowed(pool: &PgPool, source: &str, allowed: &[Origin]) -> Result<(), IpcError> {
    let list: Vec<serde_json::Value> = allowed
        .iter()
        .map(|o| serde_json::Value::String(o.to_string()))
        .collect();
    crate::settings::write_value(pool, &allowed_key(source), &serde_json::Value::Array(list)).await
}

async fn read_open_inside(pool: &PgPool, source: &str) -> Result<bool, IpcError> {
    Ok(crate::settings::read_value(pool, &open_inside_key(source))
        .await?
        .and_then(|value| value.as_bool())
        .unwrap_or(false))
}

/// The sources' ids and base URLs.
async fn sources(pool: &PgPool) -> Result<Vec<(String, String)>, IpcError> {
    let rows = knobas_sync::config::list(pool).await?;
    Ok(rows.into_iter().map(|row| (row.id, row.base_url)).collect())
}

/// Every source's site setting, and the rules of the ones that open inside
/// knobas, which the controller is given.
///
/// # Errors
///
/// [`crate::IpcErrorCode::Internal`] if a read fails.
pub async fn settings_and_rules(
    pool: &PgPool,
    sites: Option<&knobas_site::Sites>,
) -> Result<Vec<SiteSetting>, IpcError> {
    let mut out = Vec::new();
    let mut bases: Vec<(String, Base, Vec<Origin>)> = Vec::new();
    for (source, base_url) in sources(pool).await? {
        let open_inside = read_open_inside(pool, &source).await?;
        let allowed = read_allowed(pool, &source).await?;
        let kept_since = crate::settings::read_value(pool, &kept_since_key(&source))
            .await?
            .and_then(|value| value.as_str().map(str::to_owned));
        let base = Base::parse(&base_url);
        let suggested = sites
            .map(|sites| {
                sites
                    .suggestions(&source)
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default();
        out.push(SiteSetting {
            source: source.clone(),
            open_inside: open_inside && knobas_site::OFFERED && base.is_some(),
            base: base_url.clone(),
            own: base
                .as_ref()
                .map(|base| base.origin.to_string())
                .unwrap_or_default(),
            allowed: allowed.iter().map(ToString::to_string).collect(),
            suggested,
            kept_since,
            offered: knobas_site::OFFERED && base.is_some(),
            persists: knobas_site::sessions_persist(),
        });
        if open_inside && let Some(base) = base {
            bases.push((source, base, allowed));
        }
    }
    if let Some(sites) = sites {
        let mut rules = HashMap::new();
        for (source, base, allowed) in &bases {
            let others = bases
                .iter()
                .filter(|(other, _, _)| other != source)
                .map(|(other, base, _)| OtherSite {
                    source: other.clone(),
                    base: base.clone(),
                })
                .collect();
            rules.insert(
                source.clone(),
                SiteRules {
                    base: base.clone(),
                    allowed: allowed.clone(),
                    others,
                },
            );
        }
        sites.set_rules(rules);
    }
    Ok(out)
}

/// One source's setting, after a write, with the rules brought in line.
///
/// # Errors
///
/// [`crate::IpcErrorCode::NotFound`] for no such source, or a read's.
pub async fn setting_of(
    pool: &PgPool,
    sites: Option<&knobas_site::Sites>,
    source: &str,
) -> Result<SiteSetting, IpcError> {
    settings_and_rules(pool, sites)
        .await?
        .into_iter()
        .find(|setting| setting.source == source)
        .ok_or_else(|| IpcError::not_found(format!("no source `{source}`")))
}

/// Write *Open its pages inside knobas*.
///
/// # Errors
///
/// A write's.
pub async fn set_open_inside(pool: &PgPool, source: &str, on: bool) -> Result<(), IpcError> {
    crate::settings::write_value(pool, &open_inside_key(source), &serde_json::Value::Bool(on)).await
}

/// Add or remove an allowed origin. `origin` is parsed and normalised.
///
/// # Errors
///
/// [`crate::IpcErrorCode::Invalid`] for anything but `http(s)://host[:port]`.
pub async fn allow(pool: &PgPool, source: &str, origin: &str, on: bool) -> Result<(), IpcError> {
    let origin = Origin::parse(origin).map_err(IpcError::invalid)?;
    let mut allowed = read_allowed(pool, source).await?;
    allowed.retain(|known| *known != origin);
    if on {
        allowed.push(origin);
    }
    write_allowed(pool, source, &allowed).await
}

/// *Sites kept open*, 2 until set.
///
/// # Errors
///
/// A read's.
pub async fn kept(pool: &PgPool) -> Result<u32, IpcError> {
    Ok(crate::settings::read_value(pool, KEPT_KEY)
        .await?
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .map_or(KEPT_DEFAULT, |value| value.clamp(KEPT_MIN, KEPT_MAX)))
}

/// Store *Sites kept open*, held to 1 to 6.
///
/// # Errors
///
/// A write's.
pub async fn set_kept(pool: &PgPool, count: u32) -> Result<u32, IpcError> {
    let kept = count.clamp(KEPT_MIN, KEPT_MAX);
    crate::settings::write_value(pool, KEPT_KEY, &serde_json::Value::from(kept)).await?;
    Ok(kept)
}

/// Drop a deleted source's three rows (decisions §18).
///
/// # Errors
///
/// The database's.
pub async fn forget_settings(pool: &PgPool, source: &str) -> Result<(), sqlx::Error> {
    sqlx::query("delete from knobas.setting where key = any($1)")
        .bind(vec![
            open_inside_key(source),
            allowed_key(source),
            kept_since_key(source),
        ])
        .execute(pool)
        .await?;
    Ok(())
}

/// A restore brings no session: every *Kept since* goes.
///
/// # Errors
///
/// The database's.
pub async fn clear_kept_since(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("delete from knobas.setting where key like 'site.kept\\_since.%'")
        .execute(pool)
        .await?;
    Ok(())
}

// -- the shell over Tauri ---------------------------------------------------------

struct TauriShell<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    started: Instant,
}

impl<R: tauri::Runtime> Shell for TauriShell<R> {
    fn on_main(&self, f: Box<dyn FnOnce() + Send>) {
        if let Err(error) = self.app.run_on_main_thread(f) {
            tracing::debug!(%error, "the event loop has gone; a site's step dropped");
        }
    }

    fn later(&self, f: Box<dyn FnOnce() + Send>) {
        // From a worker thread, so the main thread queues it behind the
        // callback that asked, never runs it inside it.
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = app.run_on_main_thread(f);
        });
    }

    fn emit(&self, event: SiteEvent) {
        let app = &self.app;
        let target = Some("main");
        match event {
            SiteEvent::Status(status) => events::emit(app, target, events::SITE_STATUS, status),
            SiteEvent::Key(key) => events::emit(app, target, events::SITE_KEY, key),
            SiteEvent::Routed(routed) => events::emit(app, target, events::SITE_ROUTED, routed),
            SiteEvent::Download(download) => {
                events::emit(app, target, events::SITE_DOWNLOAD, download)
            }
            SiteEvent::Closed(closed) => events::emit(app, target, events::SITE_CLOSED, closed),
        }
    }

    fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }

    fn window_size(&self) -> (f64, f64) {
        let Some(window) = self.app.get_webview_window("main") else {
            return (1440.0, 868.0);
        };
        let scale = window.scale_factor().unwrap_or(1.0);
        window
            .inner_size()
            .map(|size| {
                let logical = size.to_logical::<f64>(scale);
                (logical.width, logical.height)
            })
            .unwrap_or((1440.0, 868.0))
    }

    fn quit(&self) {
        request_quit(&self.app);
    }

    fn landed(&self, source: &str) {
        let app = self.app.clone();
        let source = source.to_owned();
        tauri::async_runtime::spawn(async move {
            let Some(lifecycle) = app.try_state::<crate::commands::app::Lifecycle>() else {
                return;
            };
            let Ok(pool) = lifecycle.pool() else {
                return;
            };
            let key = kept_since_key(&source);
            if matches!(crate::settings::read_value(&pool, &key).await, Ok(None)) {
                let now = chrono::Utc::now().to_rfc3339();
                let _ = crate::settings::write_value(&pool, &key, &serde_json::Value::String(now))
                    .await;
            }
        });
    }
}

/// When the last quit was asked for: a second request within
/// [`QUIT_AGAIN`] always quits, so a page that never answers cannot keep
/// knobas open (decisions §25 step 6).
static LAST_QUIT: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);
const QUIT_AGAIN: Duration = Duration::from_secs(3);

/// Quit through the sites (decisions §10): every live site is asked at once,
/// each for at most 300 ms. With none objecting knobas quits; otherwise the
/// page is told which (`site:quit`) and the person decides.
pub fn request_quit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let again = {
        let mut last = LAST_QUIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let again = last.is_some_and(|at| at.elapsed() < QUIT_AGAIN);
        *last = Some(Instant::now());
        again
    };
    let sites = match state(app) {
        Ok(sites) if sites.alive() && !again => sites,
        _ => {
            app.exit(0);
            return;
        }
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let unsaved = sites.unsaved().await;
        if unsaved.is_empty() {
            app.exit(0);
        } else {
            tracing::info!(
                count = unsaved.len(),
                "quitting asks about sites with changes"
            );
            events::emit(&app, Some("main"), events::SITE_QUIT, unsaved);
        }
    });
}

/// Make the controller over the main window. On the main thread, once the
/// window is made.
pub fn start<R: tauri::Runtime>(app: &tauri::App<R>, profile: &crate::profile::Profile) {
    let handle = app.handle().clone();
    let shell = Arc::new(TauriShell {
        app: handle.clone(),
        started: Instant::now(),
    });
    let sites = knobas_site::Sites::new(
        shell,
        Config {
            profile_dir: profile.dir.clone(),
            keychain_service: profile.keychain_service(),
            downloads_dir: dirs::download_dir(),
            devtools: cfg!(debug_assertions),
            platform: if cfg!(target_os = "macos") {
                knobas_site::keys::Platform::Mac
            } else {
                knobas_site::keys::Platform::Windows
            },
        },
    );
    #[cfg(any(target_os = "macos", windows))]
    if let Some(window) = app.get_webview_window("main") {
        let focus = window.clone();
        let host = knobas_site::wry_host::WryHost::new(
            window,
            std::rc::Rc::new(move || {
                let webview: &tauri::Webview<R> = focus.as_ref();
                let _ = webview.set_focus();
            }),
        );
        knobas_site::sites::install_host(Box::new(host));
    }
    #[cfg(target_os = "macos")]
    {
        let keys = sites.clone();
        knobas_site::mac::set_shell_keys(std::rc::Rc::new(move |press| keys.shell_key(press)));
    }
    handle.manage(SiteState {
        sites: sites.clone(),
    });
    if let Some(window) = app.get_webview_window("main") {
        let on_event = sites.clone();
        let close_app = handle.clone();
        window.on_window_event(move |event| match event {
            tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } => {
                on_event.relayout();
            }
            tauri::WindowEvent::Moved(_) => on_event.window_moved(),
            tauri::WindowEvent::Focused(focused) => on_event.window_focused(*focused),
            tauri::WindowEvent::CloseRequested { api, .. } if on_event.alive() => {
                api.prevent_close();
                request_quit(&close_app);
            }
            _ => {}
        });
    }
    // The clock: a landing's timeout, sleep and idle close, and on the Mac
    // the shown site's address (a `pushState` raises no event). Nothing to
    // do with no site alive.
    let ticking = sites;
    tauri::async_runtime::spawn(async move {
        let mut every = tokio::time::interval(Duration::from_millis(400));
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            every.tick().await;
            if ticking.alive() {
                ticking.tick().await;
            }
        }
    });
}

/// Drop every view. On the main thread, before the database stops.
pub fn shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(state) = app.try_state::<SiteState>() {
        state.sites.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_are_per_source() {
        assert_eq!(open_inside_key("jira"), "site.open_inside.jira");
        assert_eq!(allowed_key("jira"), "site.allowed.jira");
        assert_eq!(kept_since_key("jira"), "site.kept_since.jira");
    }
}
