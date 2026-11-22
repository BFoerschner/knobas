//! A source's own website inside knobas (ADR-0050): the app's half.
//!
//! The rules, the controller and the native view are `knobas-site`'s. Here:
//! the Tauri shell they run on (the main thread, events to the main window,
//! the window's size), the four `knobas.setting` rows of decisions §18, the
//! rules built from them and the source list, the clock, and the main
//! window's events a site follows.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use knobas_secrets::{KeychainAccount, SecretStore, WebSignIn};
use knobas_site::policy::{Base, Origin, OtherSite, SiteRules};
use knobas_site::sign_in::SignInRule;
use knobas_site::sites::{Config, Shell, SiteEvent};
use knobas_site::state::{KEPT_DEFAULT, KEPT_MAX, KEPT_MIN};
use knobas_sync::scheduler::AdapterRegistry;
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

/// *Turn off* on a site that signs in with its source's own credential
/// (ux30): `true` while the person signs in there themselves.
#[must_use]
pub fn sign_in_off_key(source: &str) -> String {
    format!("site.sign_in_off.{source}")
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
    /// The username of the saved web sign-in (ux29), when one is saved. The
    /// password never leaves the keychain item.
    pub sign_in_user: Option<String>,
    /// The site signs in with the source's own credential (ux30): its
    /// adapter says so, and the source signs in by username and password.
    /// `None` for every other site.
    pub own_sign_in: Option<OwnSignIn>,
}

/// A site that signs in with its source's own credential (ux30).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "site.ts")]
pub struct OwnSignIn {
    /// The credential's username: the source's `username` setting.
    pub user: String,
    /// Not turned off. A saved sign-in still wins over it.
    pub on: bool,
}

/// The configuration key a username-and-password source keeps its username
/// under, by convention (the Add-source form's `IDENTITY_FIELD`).
pub const USERNAME_KEY: &str = "username";

/// What a site's sign-in form is filled with (ux30).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignInFrom {
    /// The web sign-in saved for the site.
    Saved,
    /// The source's own username and password.
    Own,
    /// Nothing: the person signs in.
    Nothing,
}

/// The username a site may fill with its source's own credential: the
/// adapter declares `uses_credential` on declared paths, the source signs in
/// by username and password, and it has a username. `None` otherwise.
#[must_use]
pub fn own_user<'a>(
    declared: Option<&knobas_source::web_sign_in::WebSignInPages>,
    auth: Option<knobas_source::AuthMethod>,
    config: &'a serde_json::Value,
) -> Option<&'a str> {
    let declared = declared?;
    if !declared.uses_credential || declared.any_page || declared.paths.is_empty() {
        return None;
    }
    if auth != Some(knobas_source::AuthMethod::UserPassword) {
        return None;
    }
    config
        .get(USERNAME_KEY)
        .and_then(serde_json::Value::as_str)
        .filter(|user| !user.trim().is_empty())
}

/// Which sign-in a site fills: a saved one wins, then the source's own
/// unless turned off, else none.
#[must_use]
pub fn sign_in_from(saved: bool, own: bool, off: bool) -> SignInFrom {
    if saved {
        SignInFrom::Saved
    } else if own && !off {
        SignInFrom::Own
    } else {
        SignInFrom::Nothing
    }
}

/// What `settings_and_rules` reads the saved web sign-ins with: the keychain
/// and the adapters' declarations of their sign-in pages. Absent before
/// bring-up, when no sign-in is read.
#[derive(Clone, Copy)]
pub struct SignInReads<'a> {
    pub secrets: &'a Arc<dyn SecretStore>,
    pub registry: &'a dyn AdapterRegistry,
}

impl<'a> SignInReads<'a> {
    /// From the sources' state.
    #[must_use]
    pub fn of(state: &'a crate::sources::SourcesState) -> Self {
        Self {
            secrets: &state.secrets,
            registry: state.registry.as_ref(),
        }
    }
}

/// The saved web sign-in of `source`, read from its keychain item.
///
/// # Errors
///
/// The keychain's.
pub async fn read_sign_in(
    secrets: &Arc<dyn SecretStore>,
    source: &str,
) -> Result<Option<WebSignIn>, IpcError> {
    let stored = knobas_secrets::spawn::get(secrets, &KeychainAccount::source(source))
        .await
        .map_err(|error| IpcError::internal(format!("keychain: {error}")))?;
    Ok(stored.and_then(|secret| secret.web))
}

/// Save, change or remove `source`'s web sign-in, in its keychain item beside
/// its credential, which stays as it is.
///
/// # Errors
///
/// `Invalid` for an empty username or password, or a source with no stored
/// credential; the keychain's.
pub async fn write_sign_in(
    secrets: &Arc<dyn SecretStore>,
    source: &str,
    web: Option<WebSignIn>,
) -> Result<(), IpcError> {
    if let Some(web) = &web
        && (web.username.trim().is_empty() || web.password.is_empty())
    {
        return Err(IpcError::invalid(
            "a sign-in needs a username and a password",
        ));
    }
    let account = KeychainAccount::source(source);
    let stored = knobas_secrets::spawn::get(secrets, &account)
        .await
        .map_err(|error| IpcError::internal(format!("keychain: {error}")))?;
    let Some(mut secret) = stored else {
        if web.is_none() {
            return Ok(());
        }
        return Err(IpcError::invalid(
            "this source has no stored credential to keep the sign-in beside",
        ));
    };
    secret.web = web;
    knobas_secrets::spawn::put(secrets, &account, secret)
        .await
        .map_err(|error| IpcError::internal(format!("keychain: {error}")))
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

/// *Open its pages inside knobas* as stored, or `default` when never set:
/// on for a password source (ux30, Fable's ruling: its site is what the
/// Passwords page's *Open web interface* opens), off for every other.
async fn read_open_inside(pool: &PgPool, source: &str, default: bool) -> Result<bool, IpcError> {
    Ok(crate::settings::read_value(pool, &open_inside_key(source))
        .await?
        .and_then(|value| value.as_bool())
        .unwrap_or(default))
}

async fn read_sign_in_off(pool: &PgPool, source: &str) -> Result<bool, IpcError> {
    Ok(crate::settings::read_value(pool, &sign_in_off_key(source))
        .await?
        .and_then(|value| value.as_bool())
        .unwrap_or(false))
}

/// Write *Turn off* / *Turn on* of a site's own-credential sign-in.
///
/// # Errors
///
/// A write's.
pub async fn set_sign_in_off(pool: &PgPool, source: &str, off: bool) -> Result<(), IpcError> {
    crate::settings::write_value(
        pool,
        &sign_in_off_key(source),
        &serde_json::Value::Bool(off),
    )
    .await
}

/// The sources as configured.
async fn sources(pool: &PgPool) -> Result<Vec<knobas_sync::config::SourceConfigRow>, IpcError> {
    Ok(knobas_sync::config::list(pool).await?)
}

/// The adapter template of `kind`, among the registry's.
fn template_of(
    registry: &dyn AdapterRegistry,
    kind: &str,
) -> Option<knobas_source::SourceDescriptor> {
    registry
        .descriptors()
        .into_iter()
        .find(|template| template.adapter_kind == kind)
}

fn auth_of(row: &knobas_sync::config::SourceConfigRow) -> Option<knobas_source::AuthMethod> {
    match row.auth_kind {
        knobas_sync::config::AuthKind::Method(method) => Some(method),
        knobas_sync::config::AuthKind::None => None,
    }
}

/// The username and password a site's form is filled with now (ux30): the
/// saved web sign-in, else the source's own credential where its adapter
/// declares that and it is not turned off, else `None`.
///
/// # Errors
///
/// The keychain's, or a read's.
pub async fn sign_in_for_fill(
    pool: &PgPool,
    secrets: &Arc<dyn SecretStore>,
    registry: &dyn AdapterRegistry,
    source: &str,
) -> Result<Option<WebSignIn>, IpcError> {
    let stored = knobas_secrets::spawn::get(secrets, &KeychainAccount::source(source))
        .await
        .map_err(|error| IpcError::internal(format!("keychain: {error}")))?;
    let Some(secret) = stored else {
        return Ok(None);
    };
    if secret.web.is_some() {
        return Ok(fill_from(secret, None, None, &serde_json::Value::Null, false));
    }
    let Some(row) = knobas_sync::config::get(pool, source).await? else {
        return Ok(None);
    };
    let template = template_of(registry, &row.adapter_kind);
    let declared = template.as_ref().and_then(|t| t.web_sign_in.as_ref());
    let off = read_sign_in_off(pool, source).await?;
    Ok(fill_from(secret, declared, auth_of(&row), &row.config, off))
}

/// What a site's form is filled with, from the source's keychain item and
/// what the adapter and the row say: the saved web sign-in, else the source's
/// own username and password where [`own_user`] allows it and it is not
/// turned off, else `None`. The stored credential's own kind must agree with
/// the row's: its `value` is a password only for a username-and-password
/// credential.
#[must_use]
pub fn fill_from(
    secret: knobas_secrets::Secret,
    declared: Option<&knobas_source::web_sign_in::WebSignInPages>,
    row_auth: Option<knobas_source::AuthMethod>,
    config: &serde_json::Value,
    off: bool,
) -> Option<WebSignIn> {
    let auth = row_auth.filter(|method| *method == secret.kind);
    let own = own_user(declared, auth, config);
    match sign_in_from(secret.web.is_some(), own.is_some(), off) {
        SignInFrom::Saved => secret.web,
        SignInFrom::Own => own.map(|user| WebSignIn {
            username: user.to_owned(),
            password: secret.value,
        }),
        SignInFrom::Nothing => None,
    }
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
    reads: Option<SignInReads<'_>>,
) -> Result<Vec<SiteSetting>, IpcError> {
    let mut out = Vec::new();
    let mut bases: Vec<(String, Base, Vec<Origin>)> = Vec::new();
    let listed = sources(pool).await?;
    // Each source's saved sign-in, read at once: one keychain read a source.
    let users: Vec<Option<String>> = match reads {
        Some(reads) => futures_util::future::join_all(listed.iter().map(|row| async move {
            let source = &row.id;
            match read_sign_in(reads.secrets, source).await {
                Ok(web) => web.map(|web| web.username),
                Err(error) => {
                    tracing::warn!(source = %source, %error, "a site's sign-in could not be read");
                    None
                }
            }
        }))
        .await,
        None => vec![None; listed.len()],
    };
    let mut sign_ins = HashMap::new();
    for (row, user) in listed.into_iter().zip(users) {
        let template = reads.and_then(|reads| template_of(reads.registry, &row.adapter_kind));
        let declared = template.as_ref().and_then(|t| t.web_sign_in.clone());
        let own_sign_in = match own_user(declared.as_ref(), auth_of(&row), &row.config) {
            Some(own) => Some(OwnSignIn {
                user: own.to_owned(),
                on: !read_sign_in_off(pool, &row.id).await?,
            }),
            None => None,
        };
        let source = row.id;
        let base_url = row.base_url;
        if reads.is_some() {
            let declared = declared.unwrap_or_default();
            let from = sign_in_from(
                user.is_some(),
                own_sign_in.is_some(),
                own_sign_in.as_ref().is_some_and(|own| !own.on),
            );
            sign_ins.insert(
                source.clone(),
                SignInRule {
                    saved: from != SignInFrom::Nothing,
                    paths: declared.paths,
                    any_page: declared.any_page,
                },
            );
        }
        let passwords = template.as_ref().is_some_and(|t| t.passwords.is_some());
        let open_inside = read_open_inside(pool, &source, passwords).await?;
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
            sign_in_user: user,
            own_sign_in,
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
        if reads.is_some() {
            sites.set_sign_ins(sign_ins);
        }
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
    reads: Option<SignInReads<'_>>,
    source: &str,
) -> Result<SiteSetting, IpcError> {
    settings_and_rules(pool, sites, reads)
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

    fn sign_in(&self, source: &str) {
        let app = self.app.clone();
        let source = source.to_owned();
        tauri::async_runtime::spawn(async move {
            let (Ok(sites), Ok(sources)) = (state(&app), crate::sources::state(&app)) else {
                return;
            };
            match sign_in_for_fill(
                &sources.pool,
                &sources.secrets,
                sources.registry.as_ref(),
                &source,
            )
            .await
            {
                Ok(Some(web)) => sites.fill_with(&source, web.username, web.password),
                Ok(None) => sites.fill_none(&source),
                Err(error) => {
                    tracing::warn!(source = %source, %error, "a site's sign-in could not be read");
                    sites.fill_none(&source);
                }
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

    fn store_with(secret: Option<knobas_secrets::Secret>) -> Arc<dyn SecretStore> {
        let store: Arc<dyn SecretStore> = Arc::new(knobas_secrets::MemoryStore::new());
        if let Some(secret) = secret {
            store
                .put(&KeychainAccount::source("wiki"), &secret)
                .unwrap();
        }
        store
    }

    fn web(username: &str, password: &str) -> WebSignIn {
        WebSignIn {
            username: username.into(),
            password: password.into(),
        }
    }

    /// A saved sign-in rides in the source's item and leaves its credential
    /// as it was; removing it removes it alone.
    #[tokio::test]
    async fn a_sign_in_is_kept_beside_the_credential_and_removed_alone() {
        let store = store_with(Some(knobas_secrets::Secret::just(
            knobas_source::AuthMethod::Pat,
            "token",
        )));
        write_sign_in(&store, "wiki", Some(web("bjorn", "pw")))
            .await
            .unwrap();
        let kept = store
            .get(&KeychainAccount::source("wiki"))
            .unwrap()
            .unwrap();
        assert_eq!(kept.value, "token");
        assert_eq!(
            read_sign_in(&store, "wiki").await.unwrap(),
            Some(web("bjorn", "pw"))
        );
        write_sign_in(&store, "wiki", None).await.unwrap();
        let kept = store
            .get(&KeychainAccount::source("wiki"))
            .unwrap()
            .unwrap();
        assert_eq!(kept.value, "token", "the credential stays");
        assert!(kept.web.is_none());
    }

    #[tokio::test]
    async fn an_empty_field_or_a_source_with_no_credential_is_refused() {
        let store = store_with(Some(knobas_secrets::Secret::just(
            knobas_source::AuthMethod::Pat,
            "token",
        )));
        for (user, pass) in [("", "pw"), ("  ", "pw"), ("bjorn", "")] {
            let error = write_sign_in(&store, "wiki", Some(web(user, pass)))
                .await
                .unwrap_err();
            assert_eq!(
                error.code,
                crate::IpcErrorCode::Invalid,
                "{user:?} {pass:?}"
            );
        }
        let empty = store_with(None);
        let error = write_sign_in(&empty, "wiki", Some(web("bjorn", "pw")))
            .await
            .unwrap_err();
        assert_eq!(error.code, crate::IpcErrorCode::Invalid);
        // Removing what is not there is no error.
        write_sign_in(&empty, "wiki", None).await.unwrap();
    }

    fn pages() -> knobas_source::web_sign_in::WebSignInPages {
        knobas_source::web_sign_in::WebSignInPages::at(&["/SignIn/Direct"]).with_credential()
    }

    fn user_password(password: &str) -> knobas_secrets::Secret {
        knobas_secrets::Secret::just(knobas_source::AuthMethod::UserPassword, password)
    }

    const UP: Option<knobas_source::AuthMethod> = Some(knobas_source::AuthMethod::UserPassword);

    /// The source's own credential is offered only where the adapter says the
    /// site uses it on declared pages, and the source signs in by username
    /// and password with a username.
    #[test]
    fn the_own_credential_needs_the_declaration_the_method_and_a_username() {
        let config = serde_json::json!({"username": "mara"});
        assert_eq!(own_user(Some(&pages()), UP, &config), Some("mara"));
        let plain = knobas_source::web_sign_in::WebSignInPages::at(&["/SignIn/Direct"]);
        assert_eq!(own_user(Some(&plain), UP, &config), None, "not declared");
        assert_eq!(own_user(None, UP, &config), None);
        let anywhere = knobas_source::web_sign_in::WebSignInPages {
            any_page: true,
            ..pages()
        };
        assert_eq!(own_user(Some(&anywhere), UP, &config), None, "any page");
        let no_paths = knobas_source::web_sign_in::WebSignInPages {
            paths: Vec::new(),
            ..pages()
        };
        assert_eq!(own_user(Some(&no_paths), UP, &config), None, "no paths");
        for method in [
            knobas_source::AuthMethod::Pat,
            knobas_source::AuthMethod::ApiToken,
            knobas_source::AuthMethod::OAuth,
        ] {
            assert_eq!(own_user(Some(&pages()), Some(method), &config), None, "{method:?}");
        }
        assert_eq!(own_user(Some(&pages()), None, &config), None);
        for config in [
            serde_json::json!({}),
            serde_json::json!({"username": "  "}),
            serde_json::json!({"username": 7}),
            serde_json::Value::Null,
        ] {
            assert_eq!(own_user(Some(&pages()), UP, &config), None, "{config}");
        }
    }

    #[test]
    fn a_saved_sign_in_wins_then_the_own_unless_off() {
        assert_eq!(sign_in_from(true, true, false), SignInFrom::Saved);
        assert_eq!(sign_in_from(true, true, true), SignInFrom::Saved);
        assert_eq!(sign_in_from(true, false, false), SignInFrom::Saved);
        assert_eq!(sign_in_from(false, true, false), SignInFrom::Own);
        assert_eq!(sign_in_from(false, true, true), SignInFrom::Nothing);
        assert_eq!(sign_in_from(false, false, false), SignInFrom::Nothing);
    }

    #[test]
    fn the_fill_is_the_saved_one_else_the_credential_itself() {
        let config = serde_json::json!({"username": "mara"});
        assert_eq!(
            fill_from(user_password("pw"), Some(&pages()), UP, &config, false),
            Some(web("mara", "pw"))
        );
        assert_eq!(
            fill_from(user_password("pw"), Some(&pages()), UP, &config, true),
            None,
            "turned off"
        );
        let mut saved = user_password("pw");
        saved.web = Some(web("admin", "other"));
        assert_eq!(
            fill_from(saved, Some(&pages()), UP, &config, true),
            Some(web("admin", "other")),
            "a saved one wins, off or not"
        );
        // A token stored for a row that says username and password is no
        // password to type.
        let token = knobas_secrets::Secret::just(knobas_source::AuthMethod::Pat, "tok");
        assert_eq!(fill_from(token, Some(&pages()), UP, &config, false), None);
        assert_eq!(
            fill_from(user_password("pw"), None, UP, &config, false),
            None,
            "not declared"
        );
    }

    #[test]
    fn the_keys_are_per_source() {
        assert_eq!(open_inside_key("jira"), "site.open_inside.jira");
        assert_eq!(sign_in_off_key("pw"), "site.sign_in_off.pw");
        assert_eq!(allowed_key("jira"), "site.allowed.jira");
        assert_eq!(kept_since_key("jira"), "site.kept_since.jira");
    }
}
