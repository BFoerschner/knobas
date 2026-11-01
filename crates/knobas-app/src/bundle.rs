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
use tauri::{Assets, Context, Runtime};

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

#[cfg(test)]
mod tests {
    use super::*;

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
