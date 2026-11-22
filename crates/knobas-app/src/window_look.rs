//! The window's own look before its page paints (ADR-0055, the decisions' 3.9 and 3.12).
//!
//! The page sets its theme before its first paint (`app/public/theme-boot.js`), but before the page exists the bare
//! window is the system's default, white on Windows, and its title bar follows the system even when knobas is set
//! dark. So the page tells this side its mode and each side's page colour (`window_look_set`), kept in
//! `window-look.json` in the profile's folder; the main window is built with the title bar's side and that colour.
//! Before a look is kept, it is built with knobas's own themes ([`WindowLook::knobas_own`]; not on macOS). Where the
//! look follows the system, the side is read before the window is built on Windows ([`system_dark`]), because WebView2
//! takes its colour only when it is made. Any failure on that path shows the window as before. On macOS the web view would still paint its own white over
//! the window until the page's first paint, so it is built to draw none ([`without_white`]).
//!
//! `knobas --default-theme` (or `KNOBAS_DEFAULT_THEME=1`) starts one run on knobas's own themes: the window is given
//! a script that says so before the page loads, and the page writes no theme and touches nothing kept.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The flag that starts a run on knobas's own themes.
pub const DEFAULT_THEME_FLAG: &str = "--default-theme";
/// The same, from the environment.
pub const DEFAULT_THEME_ENV: &str = "KNOBAS_DEFAULT_THEME";
/// What the main window runs before its page: the page's head script and its store read it.
pub const DEFAULT_THEME_SCRIPT: &str = "window.__knobasDefaultTheme = true;";
/// The file the look is kept in, in the profile's folder.
pub const FILE: &str = "window-look.json";

/// Whether this run starts on knobas's own themes: the flag among the arguments, or the variable set to `1`.
#[must_use]
pub fn default_theme_in<I>(args: I, env: Option<&str>) -> bool
where
    I: IntoIterator<Item = String>,
{
    args.into_iter().any(|arg| arg == DEFAULT_THEME_FLAG) || env.is_some_and(|value| value == "1")
}

/// The scripts the main window is given before its page.
#[must_use]
pub fn init_scripts(default_theme: bool) -> Vec<&'static str> {
    if default_theme {
        vec![DEFAULT_THEME_SCRIPT]
    } else {
        Vec::new()
    }
}

/// The window's mode, as Settings › Appearance › *Mode* has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    System,
    Light,
    Dark,
}

impl Mode {
    /// The mode as the page names it.
    ///
    /// # Errors
    ///
    /// Anything but `system`, `light` or `dark`.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "system" => Ok(Self::System),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            other => Err(format!("`{other}` is not a mode: system, light or dark")),
        }
    }
}

/// The look kept: the mode and each side's page colour as `#RRGGBB`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowLook {
    pub mode: Mode,
    pub light: String,
    pub dark: String,
}

/// `#RRGGBB` as its three channels, or `None`.
#[must_use]
pub fn rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some((channel(0)?, channel(2)?, channel(4)?))
}

impl WindowLook {
    /// A look from the page's three values, checked.
    ///
    /// # Errors
    ///
    /// A mode that is not one, or a colour that is not `#RRGGBB`.
    pub fn new(mode: &str, light: &str, dark: &str) -> Result<Self, String> {
        let mode = Mode::parse(mode)?;
        for colour in [light, dark] {
            if rgb(colour).is_none() {
                return Err(format!("`{colour}` is not a colour as #RRGGBB"));
            }
        }
        Ok(Self {
            mode,
            light: light.to_ascii_uppercase(),
            dark: dark.to_ascii_uppercase(),
        })
    }

    /// The title bar's side: `None` follows the system.
    #[must_use]
    pub fn theme(&self) -> Option<tauri::Theme> {
        match self.mode {
            Mode::System => None,
            Mode::Light => Some(tauri::Theme::Light),
            Mode::Dark => Some(tauri::Theme::Dark),
        }
    }

    /// The page colour of the side drawn, `system_dark` saying which side the system is on.
    #[must_use]
    pub fn colour(&self, system_dark: bool) -> tauri::window::Color {
        let dark = match self.mode {
            Mode::System => system_dark,
            Mode::Light => false,
            Mode::Dark => true,
        };
        let (r, g, b) = rgb(if dark { &self.dark } else { &self.light }).unwrap_or((0, 0, 0));
        tauri::window::Color(r, g, b, 255)
    }
}

/// knobas Light's and knobas Dark's page colour (`--paper` in `tf/css/tokens.css`).
pub const KNOBAS_LIGHT_PAGE: &str = "#E4E8EC";
pub const KNOBAS_DARK_PAGE: &str = "#12171C";

impl WindowLook {
    /// knobas's own themes following the system: what a profile shows before its page has kept a look.
    #[must_use]
    pub fn knobas_own() -> Self {
        Self {
            mode: Mode::System,
            light: KNOBAS_LIGHT_PAGE.to_owned(),
            dark: KNOBAS_DARK_PAGE.to_owned(),
        }
    }
}

/// The system's side before any window exists, where the platform says it. On Windows that is *Choose your app
/// mode* (`AppsUseLightTheme`), which WebView2's `prefers-color-scheme` and tao's title bar follow. WebView2 takes its
/// background only when its controller is made: one set after the window is built is not drawn before the page's
/// first paint (the ux29 boot check, 2026-11-22). Elsewhere `None`, and the side is read from the window once it
/// exists, which draws nothing before then.
#[must_use]
pub fn system_dark() -> Option<bool> {
    #[cfg(windows)]
    {
        dark_from_apps_use_light_theme(apps_use_light_theme())
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// The side `AppsUseLightTheme` names: `0` is dark, anything else light, `None` where it cannot be read.
#[must_use]
pub fn dark_from_apps_use_light_theme(value: Option<u32>) -> Option<bool> {
    value.map(|light| light == 0)
}

/// `HKCU\…\Themes\Personalize\AppsUseLightTheme`, read as tao reads it for the title bar.
#[cfg(windows)]
fn apps_use_light_theme() -> Option<u32> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    use windows_core::w;
    let mut data: u32 = 0;
    let mut size = u32::try_from(std::mem::size_of::<u32>()).ok()?;
    // SAFETY: a DWORD read into a `u32` whose size is given; the key and value names are static wide strings.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut data).cast()),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

/// Where the look is kept for a profile.
#[must_use]
pub fn path_in(profile_dir: &Path) -> PathBuf {
    profile_dir.join(FILE)
}

/// The look kept in the profile's folder, or `None` where there is none or it cannot be read.
#[must_use]
pub fn read(profile_dir: &Path) -> Option<WindowLook> {
    let text = std::fs::read_to_string(path_in(profile_dir)).ok()?;
    let look: WindowLook = serde_json::from_str(&text).ok()?;
    // A file edited by hand into something the page would never write is no look.
    WindowLook::new(
        match look.mode {
            Mode::System => "system",
            Mode::Light => "light",
            Mode::Dark => "dark",
        },
        &look.light,
        &look.dark,
    )
    .ok()
}

/// Keeps a look in the profile's folder, written whole beside and moved into place.
///
/// # Errors
///
/// The folder cannot be written.
pub fn write(profile_dir: &Path, look: &WindowLook) -> std::io::Result<()> {
    std::fs::create_dir_all(profile_dir)?;
    let path = path_in(profile_dir);
    let next = path.with_extension("json.new");
    std::fs::write(
        &next,
        serde_json::to_vec(look).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(next, path)
}

/// The window's web view drawing no background of its own (macOS), so the window's colour shows until the page's
/// first paint. WKWebView would paint its own there for about 300 ms, white on the light side and grey on the dark,
/// whatever the theme (the design's risk 4). The page paints its whole background, so after that nothing changes.
///
/// wry does this itself only under Tauri's `macos-private-api`, which would also turn it on for the sites' web
/// views (`knobas-site`, the same wry), and a site that sets no background would show the page under it. So only
/// the windows built from here get it: a configuration made before the web view, with WebKit's `drawsBackground`
/// off (private, as wry sets it; macOS 10.14 and later). wry keeps the default data store and adds its protocols to
/// a configuration it is given, as to its own. Off the main thread the builder is left as it was.
#[cfg(target_os = "macos")]
#[must_use]
pub fn without_white<'a, R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: tauri::WebviewWindowBuilder<'a, R, M>,
) -> tauri::WebviewWindowBuilder<'a, R, M> {
    use objc2_foundation::{NSNumber, NSObjectNSKeyValueCoding, NSString};
    let Some(main_thread) = objc2::MainThreadMarker::new() else {
        return builder;
    };
    // SAFETY: on the main thread, as the marker proves; the key is the one wry sets on its own configuration.
    let configuration = unsafe {
        let configuration = objc2_web_kit::WKWebViewConfiguration::new(main_thread);
        configuration.setValue_forKey(
            Some(&NSNumber::new_bool(false)),
            &NSString::from_str("drawsBackground"),
        );
        configuration
    };
    builder.with_webview_configuration(configuration)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_or_the_variable_starts_on_knobas_s_own_themes_and_gives_the_window_its_script() {
        let args = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(default_theme_in(args(&["knobas", "--default-theme"]), None));
        assert!(default_theme_in(args(&["knobas"]), Some("1")));
        assert!(!default_theme_in(args(&["knobas", "--demo"]), None));
        assert!(!default_theme_in(args(&["knobas"]), Some("0")));
        assert_eq!(init_scripts(true), vec![DEFAULT_THEME_SCRIPT]);
        assert!(init_scripts(false).is_empty());
    }

    #[test]
    fn a_look_written_reads_back_the_same_and_a_bad_colour_is_refused_with_the_file_unchanged() {
        let dir = tempfile::tempdir().expect("dir");
        let look = WindowLook::new("dark", "#e4e8ec", "#21222C").expect("look");
        write(dir.path(), &look).expect("write");
        assert_eq!(read(dir.path()), Some(look.clone()));
        assert_eq!(look.light, "#E4E8EC");
        assert!(WindowLook::new("dark", "#12345", "#21222C").is_err());
        assert!(WindowLook::new("dusk", "#123456", "#21222C").is_err());
        // Refused before anything is written: the file is as it was.
        assert_eq!(read(dir.path()), Some(look));
    }

    #[test]
    fn a_missing_or_broken_file_reads_as_none() {
        let dir = tempfile::tempdir().expect("dir");
        assert_eq!(read(dir.path()), None);
        std::fs::write(path_in(dir.path()), b"{ not json").expect("write");
        assert_eq!(read(dir.path()), None);
        std::fs::write(
            path_in(dir.path()),
            br##"{"mode":"dark","light":"#FFF","dark":"#000000"}"##,
        )
        .expect("write");
        assert_eq!(read(dir.path()), None);
    }

    /// `AppsUseLightTheme` is `0` on the dark side and `1` on the light; a value that cannot be read says nothing.
    #[test]
    fn apps_use_light_theme_zero_is_the_dark_side_and_a_missing_value_says_nothing() {
        assert_eq!(dark_from_apps_use_light_theme(Some(0)), Some(true));
        assert_eq!(dark_from_apps_use_light_theme(Some(1)), Some(false));
        assert_eq!(dark_from_apps_use_light_theme(None), None);
    }

    /// Off Windows no side is claimed before the window exists: the main window reads it from the window once built.
    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_system_s_side_is_read_from_the_window_not_before_it() {
        assert_eq!(system_dark(), None);
    }

    /// knobas's own look is the page colours `tokens.css` draws for knobas Light and knobas Dark, following the system.
    #[test]
    fn knobas_s_own_look_is_the_page_colours_the_page_draws() {
        let own = WindowLook::knobas_own();
        assert_eq!(
            WindowLook::new("system", KNOBAS_LIGHT_PAGE, KNOBAS_DARK_PAGE).as_ref(),
            Ok(&own)
        );
        let tokens = include_str!("../../../app/src/lib/tf/css/tokens.css");
        assert!(tokens.contains(&format!(
            "--paper: var(--paper-light, {KNOBAS_LIGHT_PAGE});"
        )));
        assert!(tokens.contains(&format!("--paper: var(--paper-dark, {KNOBAS_DARK_PAGE});")));
    }

    #[test]
    fn the_window_takes_the_side_drawn_and_its_page_colour() {
        let look = WindowLook::new("system", "#E4E8EC", "#21222C").expect("look");
        assert_eq!(look.theme(), None);
        assert_eq!(
            look.colour(true),
            tauri::window::Color(0x21, 0x22, 0x2C, 255)
        );
        assert_eq!(
            look.colour(false),
            tauri::window::Color(0xE4, 0xE8, 0xEC, 255)
        );
        let dark = WindowLook::new("dark", "#E4E8EC", "#21222C").expect("look");
        assert_eq!(dark.theme(), Some(tauri::Theme::Dark));
        assert_eq!(
            dark.colour(false),
            tauri::window::Color(0x21, 0x22, 0x2C, 255)
        );
    }
}
