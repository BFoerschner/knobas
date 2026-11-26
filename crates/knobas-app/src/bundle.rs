//! A path the bundle does not have answers 404 on `tauri://` (#2390).
//!
//! Tauri's own `tauri://` protocol resolves a path in four steps: the path,
//! then `<path>.html`, then `<path>/index.html`, then `index.html` -- the
//! single-page-app fallback. So a script or WASM file the build did not emit
//! came back as the app's page (200, `text/html`), and failed far from its
//! cause: in #2388 pdf.js logged `'text/html' is not a valid JavaScript MIME
//! type` for a decoder that was simply not there.
//!
//! knobas routes by hash (`#/…`), so nothing it loads needs that last step.
//! Each window's webview gets [`BundleAssets::answer_missing_with_404`] as its
//! web resource request handler, which Tauri runs on every `tauri://`
//! response it built: where the path, `<path>.html` and `<path>/index.html`
//! are all absent from the bundle, the `index.html` Tauri fell back to is
//! replaced with an empty 404. The first three steps, the document's CSP
//! (#2312) and every asset that is there pass untouched.
//!
//! The bundle is asked directly, through the same [`tauri::Assets`] the
//! protocol reads ([`share`] puts one copy in front of both), since the asset
//! resolver itself falls back and so cannot say what is missing. The
//! `knobas-render` scheme refuses a missing path on its own (#2387,
//! `renderers::frame_response`).

use std::borrow::Cow;
use std::sync::Arc;

use tauri::http::{Request, Response, StatusCode, header};
use tauri::utils::assets::{AssetKey, AssetsIter, CspHash};
use tauri::{Assets, Context, Manager, Runtime};

/// The response a webview's `tauri://` handler may rewrite.
pub type AssetResponse = Response<Cow<'static, [u8]>>;

/// The app's bundled assets, shared between Tauri's protocol and the handler
/// that refuses its fallback.
pub struct BundleAssets<R: Runtime>(Arc<dyn Assets<R>>);

impl<R: Runtime> Clone for BundleAssets<R> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

/// Put one shared copy of `context`'s assets in front of Tauri and return the
/// other, for [`BundleAssets::answer_missing_with_404`].
pub fn share<R: Runtime>(context: &mut Context<R>) -> BundleAssets<R> {
    struct Placeholder;
    impl<R: Runtime> Assets<R> for Placeholder {
        fn get(&self, _: &AssetKey) -> Option<Cow<'_, [u8]>> {
            None
        }
        fn iter(&self) -> Box<AssetsIter<'_>> {
            Box::new(std::iter::empty())
        }
        fn csp_hashes(&self, _: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
            Box::new(std::iter::empty())
        }
    }
    let own: Arc<dyn Assets<R>> = Arc::from(context.set_assets(Box::new(Placeholder)));
    let shared = BundleAssets(own);
    context.set_assets(Box::new(shared.clone()));
    shared
}

impl<R: Runtime> Assets<R> for BundleAssets<R> {
    fn setup(&self, app: &tauri::App<R>) {
        self.0.setup(app);
    }
    fn get(&self, key: &AssetKey) -> Option<Cow<'_, [u8]>> {
        self.0.get(key)
    }
    fn iter(&self) -> Box<AssetsIter<'_>> {
        self.0.iter()
    }
    fn csp_hashes(&self, html_path: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
        self.0.csp_hashes(html_path)
    }
}

impl<R: Runtime> BundleAssets<R> {
    /// The web resource request handler every window's webview is built with.
    pub fn answer_missing_with_404(
        &self,
    ) -> impl Fn(Request<Vec<u8>>, &mut AssetResponse) + Send + Sync + 'static {
        let assets = self.clone();
        move |request, response| {
            if assets.fell_back(request.uri().path()) {
                let mut refused = Response::new(Cow::Borrowed(&[][..]));
                *refused.status_mut() = StatusCode::NOT_FOUND;
                // Kept, so a CORS fetch sees the 404 rather than a CORS error.
                if let Some(origin) = response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN) {
                    refused
                        .headers_mut()
                        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.clone());
                }
                *response = refused;
            }
        }
    }

    /// Whether Tauri answered `path` with its last resort, `index.html`: the
    /// path as Tauri reads it (`manager::AppManager::get_asset`: one trailing
    /// `/` dropped, percent-decoded, the leading `/` dropped, empty meaning
    /// `index.html`) is in the bundle under none of its first three keys.
    fn fell_back(&self, path: &str) -> bool {
        let path = path.strip_suffix('/').unwrap_or(path);
        let path = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
        let path = path.strip_prefix('/').unwrap_or(&path);
        if path.is_empty() {
            return false;
        }
        let has = |key: &str| self.0.get(&AssetKey::from(key)).is_some();
        !(has(path) || has(&format!("{path}.html")) || has(&format!("{path}/index.html")))
    }
}

/// Build the main window, declared in tauri.conf.json with `create: false`,
/// as Tauri would build it, plus the handler the config cannot name.
///
/// # Errors
///
/// The window cannot be built.
///
/// # Panics
///
/// tauri.conf.json declares no window labelled `main`.
pub fn open_main_window<R: Runtime>(
    app: &tauri::App<R>,
    bundle: &BundleAssets<R>,
    demo: bool,
    default_theme: bool,
) -> tauri::Result<()> {
    let main = app
        .config()
        .app
        .windows
        .iter()
        .find(|window| window.label == "main")
        .expect("tauri.conf.json declares the main window")
        .clone();
    let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), &main)?
        .on_web_resource_request(bundle.answer_missing_with_404());
    // `knobas --default-theme` (ADR-0055): the page reads it before anything it kept.
    for script in crate::window_look::init_scripts(default_theme) {
        builder = builder.initialization_script(script);
    }
    // The look the page last kept, or knobas's own themes before it has kept one: the window and its web view are
    // built with the title bar's side and the page colour of the side drawn, so every frame before the page paints
    // is the theme's. Where the look follows the system, its side is read before the window is built where the
    // platform says it (Windows: WebView2 takes its colour only when it is made, and one given later shows only after
    // the page's first paint); elsewhere the colour is set again once the window exists. Not built hidden and shown
    // after: on macOS a web view made hidden never loaded its page once shown (the real window, 2026-11-22). On macOS
    // the web view draws no white of its own, so the window's colour is what shows until the page's first paint;
    // there a profile with no kept look keeps the system's window colour, as before (not checked in a Mac window).
    #[cfg(target_os = "macos")]
    {
        builder = crate::window_look::without_white(builder);
    }
    let kept =
        app.path().app_data_dir().ok().and_then(|data| {
            crate::window_look::read(&crate::profile::Profile::new(demo, &data).dir)
        });
    let look = kept.or_else(|| {
        cfg!(not(target_os = "macos")).then(crate::window_look::WindowLook::knobas_own)
    });
    let start = look
        .as_ref()
        .map(|look| look_for_window(look, crate::window_look::system_dark()));
    if let Some(start) = &start {
        builder = builder.theme(start.theme).background_color(start.colour);
    }
    let window = builder.build()?;
    if let (Some(look), Some(start)) = (&look, &start)
        && start.side_after_build
    {
        let system_dark = matches!(window.theme(), Ok(tauri::Theme::Dark));
        let _ = window.set_background_color(Some(look.colour(system_dark)));
    }
    crate::webview_menu::trim(&window);
    Ok(())
}

/// How the main window starts from a look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowStart {
    /// The title bar's side: `None` follows the system.
    pub theme: Option<tauri::Theme>,
    /// The page colour the window and its web view are built with.
    pub colour: tauri::window::Color,
    /// The look follows the system and its side was not known before the window existed: the colour is set again
    /// for the window's side once it is built.
    pub side_after_build: bool,
}

/// What a window is given from a look: the title bar's side and the page colour of the side drawn, `system_dark`
/// the system's side where it is known before the window is built (light until the window says otherwise).
#[must_use]
pub fn look_for_window(
    look: &crate::window_look::WindowLook,
    system_dark: Option<bool>,
) -> WindowStart {
    WindowStart {
        theme: look.theme(),
        colour: look.colour(system_dark.unwrap_or(false)),
        side_after_build: look.theme().is_none() && system_dark.is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With a kept look the window is given the title bar's side and the page colour; the mode decides the side.
    #[test]
    fn a_kept_look_gives_the_window_its_side_and_colour() {
        let dark = crate::window_look::WindowLook::new("dark", "#E4E8EC", "#21222C").expect("look");
        assert_eq!(
            look_for_window(&dark, Some(false)),
            WindowStart {
                theme: Some(tauri::Theme::Dark),
                colour: tauri::window::Color(0x21, 0x22, 0x2C, 255),
                side_after_build: false,
            }
        );
        let light =
            crate::window_look::WindowLook::new("light", "#E4E8EC", "#21222C").expect("look");
        assert_eq!(
            look_for_window(&light, Some(true)),
            WindowStart {
                theme: Some(tauri::Theme::Light),
                colour: tauri::window::Color(0xE4, 0xE8, 0xEC, 255),
                side_after_build: false,
            }
        );
        // A fixed side needs no system side, known or not.
        assert!(!look_for_window(&dark, None).side_after_build);
        assert_eq!(
            look_for_window(&dark, None).colour,
            tauri::window::Color(0x21, 0x22, 0x2C, 255)
        );
    }

    /// Following the system, the window is built with the system's side where it is known before the window exists
    /// (Windows, where WebView2 takes its colour only when it is made): the boot check saw the light colour until the
    /// page's first paint on a dark system when the dark one came only after the build. Where it is not known, the
    /// light side is built and the colour is set again once the window says its side.
    #[test]
    fn following_the_system_the_window_is_built_in_the_side_known_before_it_exists() {
        let system =
            crate::window_look::WindowLook::new("system", "#E4E8EC", "#12171C").expect("look");
        assert_eq!(
            look_for_window(&system, Some(true)),
            WindowStart {
                theme: None,
                colour: tauri::window::Color(0x12, 0x17, 0x1C, 255),
                side_after_build: false,
            }
        );
        assert_eq!(
            look_for_window(&system, Some(false)),
            WindowStart {
                theme: None,
                colour: tauri::window::Color(0xE4, 0xE8, 0xEC, 255),
                side_after_build: false,
            }
        );
        assert_eq!(
            look_for_window(&system, None),
            WindowStart {
                theme: None,
                colour: tauri::window::Color(0xE4, 0xE8, 0xEC, 255),
                side_after_build: true,
            }
        );
    }

    /// A profile with no kept look starts on knobas's own themes: knobas Dark's page on a dark system, not white.
    #[test]
    fn with_no_kept_look_the_window_is_knobas_s_own_page_colour_of_the_system_s_side() {
        let own = crate::window_look::WindowLook::knobas_own();
        assert_eq!(
            look_for_window(
                &own,
                crate::window_look::dark_from_apps_use_light_theme(Some(0))
            ),
            WindowStart {
                theme: None,
                colour: tauri::window::Color(0x12, 0x17, 0x1C, 255),
                side_after_build: false,
            }
        );
        assert_eq!(
            look_for_window(
                &own,
                crate::window_look::dark_from_apps_use_light_theme(Some(1))
            )
            .colour,
            tauri::window::Color(0xE4, 0xE8, 0xEC, 255)
        );
    }

    /// What the bundle holds in the test below.
    struct Dist;
    impl<R: Runtime> Assets<R> for Dist {
        fn get(&self, key: &AssetKey) -> Option<Cow<'_, [u8]>> {
            // Tauri roots every key: `/index.html`.
            let body: &[u8] = match key.as_ref().trim_start_matches('/') {
                "index.html" => b"<!doctype html>app",
                "capture.html" => b"<!doctype html>capture",
                "assets/app-Ab1.js" => b"export {};",
                "assets/a b.js" => b"export const spaced = 1;",
                "pdfjs/wasm/openjpeg.wasm" => b"\0asm",
                _ => return None,
            };
            Some(Cow::Borrowed(body))
        }
        fn iter(&self) -> Box<AssetsIter<'_>> {
            Box::new(std::iter::empty())
        }
        fn csp_hashes(&self, _: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
            Box::new(std::iter::empty())
        }
    }

    /// A path the bundle lacks answers 404 with nothing in it, not the app's
    /// page; `/`, `index.html`, a page by its bare name and every asset that
    /// is there keep their answer. Driven through Tauri's own asset resolver,
    /// the `get_asset` the `tauri://` protocol answers with, so the fallback
    /// refused here is Tauri's real one.
    #[test]
    fn a_missing_asset_answers_404_and_a_present_one_passes() {
        let mut context = tauri::test::mock_context(Dist);
        let bundle = share(&mut context);
        let app = tauri::test::mock_builder()
            .build(context)
            .expect("mock app");
        let resolver = app.asset_resolver();
        let handler = bundle.answer_missing_with_404();

        // What `tauri://localhost<uri>` answers: Tauri's response, then the
        // webview's handler (`tauri::protocol::tauri::get_response`).
        let serve = |uri: &str| {
            let path = uri.split(['?', '#']).next().unwrap_or_default();
            let asset = resolver.get(path.to_owned()).expect(uri);
            let mut response = Response::builder()
                .header(header::CONTENT_TYPE, &asset.mime_type)
                .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "tauri://localhost")
                .body(Cow::Owned(asset.bytes))
                .expect("response");
            let request = Request::builder()
                .uri(format!("tauri://localhost{uri}"))
                .body(Vec::new())
                .expect("request");
            handler(request, &mut response);
            response
        };

        // Not vacuous: Tauri still falls back to the app's page.
        assert_eq!(
            resolver
                .get("/pdfjs/wasm/openjpeg_nowasm_fallback.js".to_owned())
                .expect("fallback")
                .bytes,
            b"<!doctype html>app"
        );

        for missing in [
            "/pdfjs/wasm/openjpeg_nowasm_fallback.js",
            "/assets/missing.js",
            "/assets/missing.js?v=2",
            "/assets/",
            "/nowhere/",
        ] {
            let answer = serve(missing);
            assert_eq!(answer.status(), StatusCode::NOT_FOUND, "{missing}");
            assert!(answer.body().is_empty(), "{missing}");
            assert_eq!(
                answer.headers().get(header::CONTENT_TYPE),
                None,
                "{missing}"
            );
        }

        for (present, body) in [
            ("/", &b"<!doctype html>app"[..]),
            ("/index.html", b"<!doctype html>app"),
            ("/capture.html", b"<!doctype html>capture"),
            ("/capture", b"<!doctype html>capture"),
            ("/assets/app-Ab1.js", b"export {};"),
            ("/assets/app-Ab1.js?v=2#x", b"export {};"),
            ("/assets/a%20b.js", b"export const spaced = 1;"),
            ("/pdfjs/wasm/openjpeg.wasm", b"\0asm"),
        ] {
            let answer = serve(present);
            assert_eq!(answer.status(), StatusCode::OK, "{present}");
            assert_eq!(answer.body().as_ref(), body, "{present}");
        }
    }
}
