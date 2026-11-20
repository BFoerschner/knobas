//! A source's own website inside knobas (ADR-0048, decisions §17): each
//! command a few lines over `crate::site`. Every one answers `NotReady`
//! before the main window and the database are up.

use knobas_site::SiteError;
use knobas_site::sites::{SiteAct, SiteAsked, SiteInsets};
use knobas_site::state::SiteStatus;
use tauri::{AppHandle, Runtime, State};

use crate::IpcError;
use crate::commands::app::Lifecycle;
use crate::site::{self, SiteSetting};

fn to_ipc(error: SiteError) -> IpcError {
    match error {
        SiteError::Invalid(message) => IpcError::invalid(message),
        SiteError::NotOffered => IpcError::invalid(SiteError::NotOffered),
        SiteError::NotOpen => IpcError::not_found(SiteError::NotOpen),
        SiteError::Platform(message) => IpcError::internal(message),
    }
}

/// Every source's site setting (decisions §12). Also brings the native
/// rules in line with what is stored.
///
/// # Errors
///
/// `NotReady` before bring-up; `Internal` if a read fails.
#[tauri::command]
pub async fn site_settings<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
) -> Result<Vec<SiteSetting>, IpcError> {
    let pool = lifecycle.pool()?;
    let sites = site::state(&app).ok();
    site::settings_and_rules(&pool, sites.as_ref()).await
}

/// *Open its pages inside knobas* for one source.
///
/// # Errors
///
/// `NotReady`, `NotFound` for no such source, `Internal`.
#[tauri::command]
pub async fn set_site_open_inside<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    id: String,
    on: bool,
) -> Result<SiteSetting, IpcError> {
    let pool = lifecycle.pool()?;
    site::set_open_inside(&pool, &id, on).await?;
    tracing::info!(source = %id, on, "a source's pages open inside knobas");
    site::setting_of(&pool, site::state(&app).ok().as_ref(), &id).await
}

/// Add an origin to the ones a site may show.
///
/// # Errors
///
/// `Invalid` for anything but `http(s)://host[:port]`.
#[tauri::command]
pub async fn site_allow<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    id: String,
    origin: String,
) -> Result<SiteSetting, IpcError> {
    // A bad origin is refused before anything is read.
    knobas_site::policy::Origin::parse(&origin).map_err(IpcError::invalid)?;
    let pool = lifecycle.pool()?;
    site::allow(&pool, &id, &origin, true).await?;
    site::setting_of(&pool, site::state(&app).ok().as_ref(), &id).await
}

/// Remove an allowed origin.
///
/// # Errors
///
/// `Invalid` for anything but `http(s)://host[:port]`.
#[tauri::command]
pub async fn site_disallow<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    id: String,
    origin: String,
) -> Result<SiteSetting, IpcError> {
    // A bad origin is refused before anything is read.
    knobas_site::policy::Origin::parse(&origin).map_err(IpcError::invalid)?;
    let pool = lifecycle.pool()?;
    site::allow(&pool, &id, &origin, false).await?;
    site::setting_of(&pool, site::state(&app).ok().as_ref(), &id).await
}

/// An *Open in ⟨source⟩* named an origin outside the site's and went to the
/// browser: Settings offers it with *Allow* until knobas quits.
///
/// # Errors
///
/// `NotReady`; `Invalid` for an origin that does not parse.
#[tauri::command]
pub fn site_suggest<R: Runtime>(
    app: AppHandle<R>,
    id: String,
    origin: String,
) -> Result<(), IpcError> {
    let origin = knobas_site::policy::Origin::parse(&origin).map_err(IpcError::invalid)?;
    site::state(&app)?.suggest(&id, origin);
    Ok(())
}

/// Close the site, asking first unless `force`, and remove its session.
///
/// # Errors
///
/// `NotReady`; `Invalid` for an id that is not a source id.
#[tauri::command]
pub async fn site_forget<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    id: String,
    force: bool,
) -> Result<SiteAsked, IpcError> {
    let pool = lifecycle.pool()?;
    let asked = site::state(&app)?
        .forget(&id, force)
        .await
        .map_err(to_ipc)?;
    if asked.done {
        crate::settings::write(&pool, &site::kept_since_key(&id), None).await?;
        tracing::info!(source = %id, "a site's sign-in forgotten");
    }
    Ok(asked)
}

/// Make the site's view or point it at `url`, asking the page first unless
/// `force`.
///
/// # Errors
///
/// `NotReady`; `Invalid` when the source's pages do not open inside knobas
/// or `url` is not one its site may show.
#[tauri::command]
pub async fn site_open<R: Runtime>(
    app: AppHandle<R>,
    source: String,
    url: Option<String>,
    insets: SiteInsets,
    force: bool,
) -> Result<SiteAsked, IpcError> {
    site::state(&app)?
        .open(&source, url, insets, force)
        .await
        .map_err(to_ipc)
}

/// Show the site's view at `insets`, when it has a page. `focus` gives it
/// the keyboard; `esc` says Esc has somewhere to go.
///
/// # Errors
///
/// `NotReady`.
#[tauri::command]
pub fn site_show<R: Runtime>(
    app: AppHandle<R>,
    source: String,
    insets: SiteInsets,
    focus: bool,
    esc: bool,
) -> Result<(), IpcError> {
    site::state(&app)?.show(&source, insets, focus, esc);
    Ok(())
}

/// Hide the site's view. Its page keeps running.
///
/// # Errors
///
/// `NotReady`.
#[tauri::command]
pub fn site_hide<R: Runtime>(app: AppHandle<R>, source: String) -> Result<(), IpcError> {
    site::state(&app)?.hide(&source);
    Ok(())
}

/// Back, forward, reload, stop, or the keyboard to the site or to knobas.
///
/// # Errors
///
/// `NotReady`; `NotFound` when the site is not open.
#[tauri::command]
pub async fn site_do<R: Runtime>(
    app: AppHandle<R>,
    source: String,
    act: SiteAct,
    force: bool,
) -> Result<SiteAsked, IpcError> {
    site::state(&app)?
        .act(&source, act, force)
        .await
        .map_err(to_ipc)
}

/// The answer to "⟨source⟩ wants to continue at another address". With
/// `allow` the origin is stored and the landing starts again; without, the
/// answer is the stopped address, for the page to open in the browser.
///
/// # Errors
///
/// `NotReady`; `NotFound`; `Invalid` when the site asks about no such origin.
#[tauri::command]
pub async fn site_answer<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    source: String,
    origin: String,
    allow: bool,
) -> Result<Option<String>, IpcError> {
    let browser = site::state(&app)?
        .answer(&source, &origin, allow)
        .map_err(to_ipc)?;
    if allow {
        let pool = lifecycle.pool()?;
        site::allow(&pool, &source, &origin, true).await?;
    }
    Ok(browser)
}

/// Close the site's view, asking the page first unless `force`.
///
/// # Errors
///
/// `NotReady`; `NotFound` when the site is not open.
#[tauri::command]
pub async fn site_close<R: Runtime>(
    app: AppHandle<R>,
    source: String,
    force: bool,
) -> Result<SiteAsked, IpcError> {
    site::state(&app)?
        .close(&source, force)
        .await
        .map_err(to_ipc)
}

/// The live sites.
///
/// # Errors
///
/// `NotReady`.
#[tauri::command]
pub fn sites<R: Runtime>(app: AppHandle<R>) -> Result<Vec<SiteStatus>, IpcError> {
    Ok(site::state(&app)?.statuses())
}

/// The live sites whose pages hold changes, each asked for at most 300 ms.
///
/// # Errors
///
/// `NotReady`.
#[tauri::command]
pub async fn sites_unsaved<R: Runtime>(app: AppHandle<R>) -> Result<Vec<SiteStatus>, IpcError> {
    Ok(site::state(&app)?.unsaved().await)
}

/// *Quit anyway*: knobas quits, closing every site.
///
/// # Errors
///
/// Never.
#[tauri::command]
pub fn site_quit<R: Runtime>(app: AppHandle<R>) -> Result<(), IpcError> {
    tracing::info!("quit anyway, with sites holding changes");
    app.exit(0);
    Ok(())
}

/// The window's zoom level, for every live site and each new one.
///
/// # Errors
///
/// `NotReady`.
#[tauri::command]
pub fn site_zoom<R: Runtime>(app: AppHandle<R>, level: f64) -> Result<(), IpcError> {
    site::state(&app)?.zoom(level);
    Ok(())
}

/// Show a download knobas recorded in the file manager.
///
/// # Errors
///
/// `NotReady`; `Invalid` for an id knobas never issued.
#[tauri::command]
pub fn site_reveal_download<R: Runtime>(app: AppHandle<R>, id: u32) -> Result<(), IpcError> {
    let path = site::state(&app)?.download_path(id).map_err(to_ipc)?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(IpcError::internal)
}

/// *Sites kept open*.
///
/// # Errors
///
/// `NotReady`; `Internal`.
#[tauri::command]
pub async fn sites_kept<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
) -> Result<u32, IpcError> {
    let pool = lifecycle.pool()?;
    let kept = site::kept(&pool).await?;
    if let Ok(sites) = site::state(&app) {
        sites.set_kept(kept);
    }
    Ok(kept)
}

/// Store *Sites kept open* (1 to 6) and answer what is stored.
///
/// # Errors
///
/// `NotReady`; `Internal`.
#[tauri::command]
pub async fn set_sites_kept<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, Lifecycle>,
    count: u32,
) -> Result<u32, IpcError> {
    let pool = lifecycle.pool()?;
    let kept = site::set_kept(&pool, count).await?;
    if let Ok(sites) = site::state(&app) {
        sites.set_kept(kept);
    }
    Ok(kept)
}

#[cfg(test)]
mod tests {
    const MIRROR: &str = include_str!("../../../../app/src/lib/ipc/site.ts");

    /// Every site command in the handler list is invoked from the mirror by
    /// its registered name, and the mirror invokes no other.
    #[test]
    fn the_mirror_invokes_the_commands_by_their_registered_names() {
        crate::commands::mirror::assert_mirrors("site", MIRROR, "app/src/lib/ipc/site.ts", 19);
    }

    /// The keys each call sends are the command's arguments.
    #[test]
    fn the_mirror_sends_the_arguments_by_their_names() {
        use crate::commands::mirror::{invocation, keys};
        for (command, args) in [
            ("site_open", &["source", "url", "insets", "force"][..]),
            ("site_show", &["source", "insets", "focus", "esc"]),
            ("site_do", &["source", "act", "force"]),
            ("site_answer", &["source", "origin", "allow"]),
            ("site_forget", &["id", "force"]),
            ("set_site_open_inside", &["id", "on"]),
            ("site_reveal_download", &["id"]),
        ] {
            assert_eq!(keys(invocation(MIRROR, command)), args, "{command}");
        }
    }
}
